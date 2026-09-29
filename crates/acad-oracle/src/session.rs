//! A QEMU guest booted from floppies and driven over QMP: keys in, video
//! memory and screenshots out.

use crate::screen::Screen;
use serde_json::{json, Value};
use std::fs;
use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::UnixStream;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, ExitStatus, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Mutex, MutexGuard};
use std::thread;
use std::time::{Duration, Instant};

static NEXT_SESSION: AtomicU64 = AtomicU64::new(0);
static QEMU_LOCK: Mutex<()> = Mutex::new(());
pub(crate) const BOOT_TIMEOUT: Duration = Duration::from_secs(30);
pub(crate) const EDIT_TIMEOUT: Duration = Duration::from_secs(20);

pub struct Session {
    child: Child,
    reader: BufReader<UnixStream>,
    writer: UnixStream,
    dir: PathBuf,
    _serial: MutexGuard<'static, ()>,
}

/// A fresh directory directly under `/tmp`; QMP socket paths must stay
/// shorter than 104 bytes.
fn scratch_dir(prefix: &str) -> Result<PathBuf, String> {
    loop {
        let candidate = PathBuf::from(format!(
            "/tmp/{prefix}-{}-{}",
            std::process::id(),
            NEXT_SESSION.fetch_add(1, Ordering::Relaxed)
        ));
        match fs::create_dir(&candidate) {
            Ok(()) => return Ok(candidate),
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(e) => return Err(format!("create {}: {e}", candidate.display())),
        }
    }
}

fn lock() -> MutexGuard<'static, ()> {
    // Concurrent guests can starve AutoCAD's keyboard and display loop,
    // causing a stable but incomplete CGA frame to be captured.
    QEMU_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

impl Session {
    /// Boot copies of the floppies. The copies live in `dir()` as
    /// `system.img` and `samples.img` and are deleted with the session.
    /// Each named `.BAK` on the Samples copy is promoted to `.DWG` first.
    pub fn boot_disposable(
        system: &Path,
        samples: Option<&Path>,
        backups: &[&str],
    ) -> Result<Self, String> {
        let serial = lock();
        let dir = scratch_dir("acad-oracle")?;
        let prepared = (|| -> Result<(), String> {
            fs::copy(system, dir.join("system.img"))
                .map_err(|e| format!("copy {}: {e}", system.display()))?;
            if let Some(samples) = samples {
                let path = dir.join("samples.img");
                fs::copy(samples, &path).map_err(|e| format!("copy {}: {e}", samples.display()))?;
                if !backups.is_empty() {
                    let mut bytes = fs::read(&path).map_err(|e| e.to_string())?;
                    for name in backups {
                        crate::qemu::promote_backup(&mut bytes, name)?;
                    }
                    fs::write(&path, bytes).map_err(|e| e.to_string())?;
                }
            }
            Ok(())
        })();
        if let Err(e) = prepared {
            let _ = fs::remove_dir_all(&dir);
            return Err(e);
        }
        let a = dir.join("system.img");
        let b = samples.map(|_| dir.join("samples.img"));
        Self::launch(serial, dir, &a, b.as_deref())
    }

    /// Boot floppy images in place. The guest writes to them, so callers pass
    /// working copies, never corpus images.
    pub fn boot_in_place(a: &Path, b: Option<&Path>) -> Result<Self, String> {
        let serial = lock();
        let dir = scratch_dir("acad-qemu")?;
        Self::launch(serial, dir, a, b)
    }

    fn launch(
        serial: MutexGuard<'static, ()>,
        dir: PathBuf,
        a: &Path,
        b: Option<&Path>,
    ) -> Result<Self, String> {
        let socket = dir.join("qmp.sock");
        let mut command = Command::new("qemu-system-i386");
        command.args(["-machine", "isapc", "-m", "16"]);
        // QEMU 11.0.1's chained TCG blocks can fail to truncate EIP after
        // a wrapping 16-bit near call (observed during CIRCLE). Disabling
        // chaining keeps AutoCAD's overlay calls inside their code segment.
        // See docs/oracle-qemu.md for the trace and upstream fix.
        command.args(["-d", "nochain"]);
        command
            .arg("-drive")
            .arg(format!("file={},if=floppy,index=0,format=raw", a.display()));
        if let Some(b) = b {
            command
                .arg("-drive")
                .arg(format!("file={},if=floppy,index=1,format=raw", b.display()));
        }
        let mut child = match command
            .args(["-boot", "a", "-display", "none", "-no-reboot"])
            .arg("-qmp")
            .arg(format!("unix:{},server=on,wait=off", socket.display()))
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
        {
            Ok(child) => child,
            Err(e) => {
                let _ = fs::remove_dir_all(&dir);
                return Err(format!("start qemu-system-i386: {e}"));
            }
        };
        let start = Instant::now();
        let stream = loop {
            match UnixStream::connect(&socket) {
                Ok(stream) => break stream,
                Err(_) if start.elapsed() < BOOT_TIMEOUT => {
                    thread::sleep(Duration::from_millis(25));
                }
                Err(e) => {
                    let _ = child.kill();
                    let _ = child.wait();
                    let _ = fs::remove_dir_all(&dir);
                    return Err(format!("connect QMP socket: {e}"));
                }
            }
        };
        if let Err(e) = stream.set_read_timeout(Some(Duration::from_secs(5))) {
            let _ = child.kill();
            let _ = child.wait();
            let _ = fs::remove_dir_all(&dir);
            return Err(format!("QMP read timeout: {e}"));
        }
        let writer = match stream.try_clone() {
            Ok(writer) => writer,
            Err(e) => {
                let _ = child.kill();
                let _ = child.wait();
                let _ = fs::remove_dir_all(&dir);
                return Err(format!("clone QMP socket: {e}"));
            }
        };
        let mut session = Self {
            child,
            reader: BufReader::new(stream),
            writer,
            dir,
            _serial: serial,
        };
        let mut greeting = String::new();
        session
            .reader
            .read_line(&mut greeting)
            .map_err(|e| format!("read QMP greeting: {e}"))?;
        if !greeting.contains("\"QMP\"") {
            return Err(format!("unexpected QMP greeting: {greeting}"));
        }
        session.command(json!({"execute": "qmp_capabilities"}))?;
        Ok(session)
    }

    /// Scratch directory holding the QMP socket, dumps, and (for disposable
    /// sessions) the floppy copies.
    pub fn dir(&self) -> &Path {
        &self.dir
    }

    /// Press or release one key. Repeated presses produce repeated make
    /// codes, as a real keyboard's typematic repeat does.
    pub fn key(&mut self, qcode: &str, down: bool) -> Result<(), String> {
        self.command(json!({
            "execute": "input-send-event",
            "arguments": {"events": [{
                "type": "key",
                "data": {"down": down, "key": {"type": "qcode", "data": qcode}}
            }]}
        }))
        .map(drop)
    }

    /// `size` bytes of guest physical memory from `address`.
    pub fn read_memory(&mut self, address: u32, size: usize) -> Result<Vec<u8>, String> {
        let dump = self.dir.join("memory.bin");
        self.command(json!({
            "execute": "pmemsave",
            "arguments": {"val": address, "size": size, "filename": dump}
        }))?;
        fs::read(dump).map_err(|e| format!("read guest memory: {e}"))
    }

    /// The 16 KiB CGA region at `B8000h`.
    pub fn video_memory(&mut self) -> Result<Vec<u8>, String> {
        self.read_memory(0xb8000, crate::cga::VIDEO_BYTES)
    }

    /// The BIOS's copy of the CGA character ROM: glyphs 0–127 at
    /// `F000:FA6E`, and 128–255 wherever INT 1Fh points, if anywhere.
    pub fn bios_font(&mut self) -> Result<crate::cga::Font, String> {
        let low = self.read_memory(0xffa6e, 1024)?;
        let vector = self.read_memory(0x1f * 4, 4)?;
        let offset = u32::from(u16::from_le_bytes([vector[0], vector[1]]));
        let segment = u32::from(u16::from_le_bytes([vector[2], vector[3]]));
        let high = if segment == 0 && offset == 0 {
            None
        } else {
            Some(self.read_memory(segment * 16 + offset, 1024)?)
        };
        crate::cga::Font::new(&low, high.as_deref())
    }

    /// QEMU's rendering of its own adapter. Correct in text mode only.
    pub fn screendump(&mut self) -> Result<Screen, String> {
        let dump = self.dir.join("screen.ppm");
        self.command(json!({"execute": "screendump", "arguments": {"filename": dump}}))?;
        Screen::from_ppm(&fs::read(dump).map_err(|e| format!("read screendump: {e}"))?)
    }

    pub fn exit_status(&mut self) -> Option<ExitStatus> {
        self.child.try_wait().ok().flatten()
    }

    fn command(&mut self, command: Value) -> Result<Value, String> {
        self.writer
            .write_all(format!("{command}\n").as_bytes())
            .map_err(|e| format!("write QMP command: {e}"))?;
        loop {
            let mut line = String::new();
            self.reader
                .read_line(&mut line)
                .map_err(|e| format!("read QMP response: {e}"))?;
            if line.is_empty() {
                return Err("QMP connection closed".into());
            }
            let reply: Value =
                serde_json::from_str(&line).map_err(|e| format!("parse QMP response: {e}"))?;
            if let Some(error) = reply.get("error") {
                return Err(format!("QMP error: {error}"));
            }
            if let Some(value) = reply.get("return") {
                return Ok(value.clone());
            }
        }
    }

    pub fn type_line(&mut self, line: &str) -> Result<(), String> {
        for ch in line.chars().chain(std::iter::once('\n')) {
            let key = match ch {
                '\n' => "ret".to_owned(),
                ',' => "comma".to_owned(),
                '.' => "dot".to_owned(),
                '-' => "minus".to_owned(),
                ' ' => "spc".to_owned(),
                ':' => "semicolon".to_owned(),
                c if c.is_ascii_alphanumeric() => c.to_ascii_lowercase().to_string(),
                _ => return Err(format!("unsupported oracle input character {ch:?}")),
            };
            let mut keys = Vec::new();
            if ch.is_ascii_uppercase() || ch == ':' {
                keys.push(json!({"type": "qcode", "data": "shift"}));
            }
            keys.push(json!({"type": "qcode", "data": key}));
            self.command(json!({
                "execute": "send-key",
                "arguments": {
                    "keys": keys,
                    "hold-time": 70
                }
            }))?;
            thread::sleep(Duration::from_millis(150));
        }
        Ok(())
    }

    pub fn text_screen(&mut self) -> Result<String, String> {
        let dump = self.dir.join("text.bin");
        self.command(json!({
            "execute": "pmemsave",
            "arguments": {"val": 0xb8000, "size": 4000, "filename": dump}
        }))?;
        let bytes = fs::read(dump).map_err(|e| format!("read video RAM: {e}"))?;
        let mut text = String::new();
        for row in bytes.chunks_exact(160) {
            for pair in row.chunks_exact(2) {
                let c = pair[0];
                text.push(if (32..127).contains(&c) {
                    c as char
                } else {
                    ' '
                });
            }
            text.push('\n');
        }
        Ok(text)
    }

    pub fn capture_editor(&mut self) -> Result<Vec<u8>, String> {
        let start = Instant::now();
        let mut previous = Vec::new();
        let mut stable = 0;
        while start.elapsed() < EDIT_TIMEOUT {
            thread::sleep(Duration::from_millis(200));
            // BIOS keyboard head/tail must agree: loading a floppy library
            // can leave several already-typed commands waiting in the queue.
            let keyboard = self.dir.join("keyboard.bin");
            self.command(json!({"execute": "pmemsave", "arguments": {"val": 0x41a, "size": 4, "filename": keyboard}}))?;
            let queue = fs::read(keyboard).map_err(|e| e.to_string())?;
            if queue.len() != 4 || queue[..2] != queue[2..] {
                stable = 0;
                continue;
            }
            let dump = self.dir.join("cga.bin");
            self.command(json!({"execute": "pmemsave", "arguments": {"val": 0xb8000, "size": 16384, "filename": dump}}))?;
            let frame = fs::read(dump).map_err(|e| e.to_string())?;
            if frame == previous {
                stable += 1;
            } else {
                stable = 0;
            }
            if stable >= 3 {
                return Ok(frame);
            }
            previous = frame;
        }
        Err("timed out waiting for editor keyboard drain and stable CGA frame".into())
    }

    pub fn wait_for_text(&mut self, needle: &str, timeout: Duration) -> Result<(), String> {
        let start = Instant::now();
        while start.elapsed() < timeout {
            if self.text_screen()?.contains(needle) {
                return Ok(());
            }
            thread::sleep(Duration::from_millis(100));
        }
        let screen = self.text_screen()?;
        let visible = screen
            .lines()
            .map(str::trim)
            .filter(|line| !line.is_empty())
            .take(25)
            .collect::<Vec<_>>()
            .join(" | ");
        Err(format!(
            "timed out waiting for {needle:?} on the DOS text screen: {visible}"
        ))
    }

    pub fn wait_until_text_gone(&mut self, needle: &str, timeout: Duration) -> Result<(), String> {
        let start = Instant::now();
        while start.elapsed() < timeout {
            if !self.text_screen()?.contains(needle) {
                return Ok(());
            }
            thread::sleep(Duration::from_millis(100));
        }
        Err(format!("timed out leaving {needle:?}"))
    }

    pub fn shutdown(&mut self) -> Result<(), String> {
        self.command(json!({"execute": "quit"}))?;
        self.child
            .wait()
            .map_err(|e| format!("wait for QEMU: {e}"))?;
        Ok(())
    }
}

impl Drop for Session {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
        let _ = fs::remove_dir_all(&self.dir);
    }
}
