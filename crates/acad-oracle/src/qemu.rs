use serde_json::{json, Value};
use std::fs;
use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::UnixStream;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::thread;
use std::time::{Duration, Instant};

static NEXT_RUN: AtomicU64 = AtomicU64::new(0);
const BOOT_TIMEOUT: Duration = Duration::from_secs(30);
const EDIT_TIMEOUT: Duration = Duration::from_secs(20);

pub fn available() -> bool {
    Command::new("qemu-system-i386")
        .arg("--version")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .is_ok_and(|s| s.success())
}

/// Ask the original to export AC1.2 sample drawings as DXF, using the
/// System floppy in drive A and a disposable Samples floppy in drive B.
pub fn export_samples(
    system_disk: &Path,
    samples_disk: &Path,
    names: &[&str],
) -> Result<Vec<Vec<u8>>, String> {
    if names.iter().any(|name| {
        name.is_empty()
            || name.len() > 8
            || !name
                .bytes()
                .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit())
    }) {
        return Err("sample names must be 1–8 uppercase ASCII letters or digits".into());
    }
    let mut vm = Vm::boot(system_disk, Some(samples_disk))?;
    vm.wait_for_text("Enter selection:", BOOT_TIMEOUT)?;
    for (i, name) in names.iter().enumerate() {
        vm.type_line("5")?;
        vm.wait_for_text("Enter NAME of drawing", BOOT_TIMEOUT)?;
        vm.type_line(&format!("B:{name}"))?;
        vm.wait_for_text("Drawing interchange file complete.", EDIT_TIMEOUT)?;
        if i + 1 < names.len() {
            vm.type_line("")?;
            vm.wait_until_text_gone("Drawing interchange file complete.", BOOT_TIMEOUT)?;
            vm.wait_for_text("Enter selection:", BOOT_TIMEOUT)?;
        }
    }
    vm.shutdown()?;
    let image = fs::read(vm.dir.join("samples.img"))
        .map_err(|e| format!("read copied Samples floppy: {e}"))?;
    names
        .iter()
        .map(|name| fat12_file(&image, &format!("{name}.DXF")))
        .collect()
}

/// Script editor commands on the original, save with END, and return its DWG.
/// The original floppy is never mounted for writing.
pub fn generate_dwg(
    system_disk: &Path,
    name: &str,
    editor_lines: &[&str],
) -> Result<Vec<u8>, String> {
    run(system_disk, name, editor_lines, false).map(|(dwg, _)| dwg)
}

/// Also ask the original to export the generated drawing as 1983 DXF.
pub fn generate_pair(
    system_disk: &Path,
    name: &str,
    editor_lines: &[&str],
) -> Result<(Vec<u8>, Vec<u8>), String> {
    let (dwg, dxf) = run(system_disk, name, editor_lines, true)?;
    Ok((dwg, dxf.expect("export requested")))
}

fn run(
    system_disk: &Path,
    name: &str,
    editor_lines: &[&str],
    export_dxf: bool,
) -> Result<(Vec<u8>, Option<Vec<u8>>), String> {
    if name.is_empty()
        || name.len() > 8
        || !name
            .bytes()
            .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit())
    {
        return Err("drawing name must be 1–8 uppercase ASCII letters or digits".into());
    }
    let mut vm = Vm::boot(system_disk, None)?;
    vm.wait_for_text("Enter selection:", BOOT_TIMEOUT)?;
    vm.type_line("1")?;
    vm.wait_for_text("Enter NAME of drawing:", BOOT_TIMEOUT)
        .map_err(|e| format!("new drawing: {e}"))?;
    vm.type_line(name)?;
    // AutoCAD writes CGA bitplanes directly, so QEMU's VGA text screenshot
    // becomes garbled here. The keyboard and floppy continue to work.
    vm.wait_until_text_gone("Enter NAME of drawing:", BOOT_TIMEOUT)?;
    thread::sleep(Duration::from_millis(500));
    for line in editor_lines {
        vm.type_line(line)?;
    }
    vm.type_line("END")?;
    vm.wait_for_text("Current drawing:", EDIT_TIMEOUT)?;
    vm.wait_for_text("Enter selection:", EDIT_TIMEOUT)?;
    thread::sleep(Duration::from_millis(300));
    if export_dxf {
        vm.type_line("5")?;
        vm.wait_for_text("Enter NAME of drawing (default", BOOT_TIMEOUT)
            .map_err(|e| format!("DXF export: {e}"))?;
        vm.type_line(name)?;
        vm.wait_for_text("Drawing interchange file complete.", EDIT_TIMEOUT)?;
    }
    vm.shutdown()?;
    let image = fs::read(vm.image_path()).map_err(|e| format!("read copied floppy: {e}"))?;
    let dwg = fat12_file(&image, &format!("{name}.DWG"))?;
    let dxf = if export_dxf {
        Some(fat12_file(&image, &format!("{name}.DXF"))?)
    } else {
        None
    };
    Ok((dwg, dxf))
}

struct Vm {
    child: Child,
    reader: BufReader<UnixStream>,
    writer: UnixStream,
    dir: PathBuf,
}

impl Vm {
    fn boot(source: &Path, samples: Option<&Path>) -> Result<Self, String> {
        let dir = loop {
            let candidate = PathBuf::from(format!(
                "/tmp/acad-oracle-{}-{}",
                std::process::id(),
                NEXT_RUN.fetch_add(1, Ordering::Relaxed)
            ));
            match fs::create_dir(&candidate) {
                Ok(()) => break candidate,
                Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
                Err(e) => return Err(format!("create {}: {e}", candidate.display())),
            }
        };
        let image = dir.join("system.img");
        if let Err(e) = fs::copy(source, &image) {
            let _ = fs::remove_dir_all(&dir);
            return Err(format!("copy {}: {e}", source.display()));
        }
        if let Some(samples) = samples {
            if let Err(e) = fs::copy(samples, dir.join("samples.img")) {
                let _ = fs::remove_dir_all(&dir);
                return Err(format!("copy {}: {e}", samples.display()));
            }
        }
        let socket = dir.join("qmp.sock");
        let mut command = Command::new("qemu-system-i386");
        command.args(["-machine", "isapc", "-m", "16"]);
        command.arg("-drive").arg(format!(
            "file={},if=floppy,index=0,format=raw",
            image.display()
        ));
        if samples.is_some() {
            command.arg("-drive").arg(format!(
                "file={},if=floppy,index=1,format=raw",
                dir.join("samples.img").display()
            ));
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
        let mut vm = Self {
            child,
            reader: BufReader::new(stream),
            writer,
            dir,
        };
        let mut greeting = String::new();
        vm.reader
            .read_line(&mut greeting)
            .map_err(|e| format!("read QMP greeting: {e}"))?;
        if !greeting.contains("\"QMP\"") {
            return Err(format!("unexpected QMP greeting: {greeting}"));
        }
        vm.command(json!({"execute": "qmp_capabilities"}))?;
        Ok(vm)
    }

    fn image_path(&self) -> PathBuf {
        self.dir.join("system.img")
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

    fn type_line(&mut self, line: &str) -> Result<(), String> {
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

    fn text_screen(&mut self) -> Result<String, String> {
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

    fn wait_for_text(&mut self, needle: &str, timeout: Duration) -> Result<(), String> {
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

    fn wait_until_text_gone(&mut self, needle: &str, timeout: Duration) -> Result<(), String> {
        let start = Instant::now();
        while start.elapsed() < timeout {
            if !self.text_screen()?.contains(needle) {
                return Ok(());
            }
            thread::sleep(Duration::from_millis(100));
        }
        Err(format!("timed out leaving {needle:?}"))
    }

    fn shutdown(&mut self) -> Result<(), String> {
        self.command(json!({"execute": "quit"}))?;
        self.child
            .wait()
            .map_err(|e| format!("wait for QEMU: {e}"))?;
        Ok(())
    }
}

impl Drop for Vm {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
        let _ = fs::remove_dir_all(&self.dir);
    }
}

/// Read one 8.3 file from the original 360 KiB FAT12 floppy layout.
fn fat12_file(image: &[u8], filename: &str) -> Result<Vec<u8>, String> {
    fn le16(bytes: &[u8], at: usize) -> Result<usize, String> {
        let pair = bytes
            .get(at..at + 2)
            .ok_or_else(|| format!("short FAT12 image at {at}"))?;
        Ok(u16::from_le_bytes([pair[0], pair[1]]) as usize)
    }
    let sector = le16(image, 11)?;
    let cluster_sectors = *image.get(13).ok_or("short FAT12 boot sector")? as usize;
    let reserved = le16(image, 14)?;
    let fats = *image.get(16).ok_or("short FAT12 boot sector")? as usize;
    let root_entries = le16(image, 17)?;
    let fat_sectors = le16(image, 22)?;
    if sector != 512 || cluster_sectors == 0 || fats == 0 {
        return Err("unexpected FAT12 geometry".into());
    }
    let root_start = (reserved + fats * fat_sectors) * sector;
    let data_start = root_start + (root_entries * 32).div_ceil(sector) * sector;
    let fat = image
        .get(reserved * sector..(reserved + fat_sectors) * sector)
        .ok_or("FAT runs past image")?;
    let (stem, ext) = filename
        .split_once('.')
        .ok_or("FAT filename needs extension")?;
    if stem.len() > 8 || ext.len() > 3 {
        return Err("not an 8.3 filename".into());
    }
    let mut dos_name = [b' '; 11];
    dos_name[..stem.len()].copy_from_slice(stem.as_bytes());
    dos_name[8..8 + ext.len()].copy_from_slice(ext.as_bytes());
    let mut entry = None;
    for row in image
        .get(root_start..root_start + root_entries * 32)
        .ok_or("FAT root runs past image")?
        .chunks_exact(32)
    {
        if row[0..11] == dos_name && row[11] & 0x18 == 0 {
            entry = Some(row);
            break;
        }
    }
    let entry = entry.ok_or_else(|| format!("{filename} was not written to the floppy"))?;
    let mut remaining = u32::from_le_bytes(entry[28..32].try_into().unwrap()) as usize;
    if remaining > image.len() {
        return Err("FAT12 file size exceeds floppy image".into());
    }
    let mut cluster = le16(entry, 26)?;
    let mut out = Vec::with_capacity(remaining);
    let cluster_size = cluster_sectors * sector;
    while remaining > 0 {
        if !(2..0xff8).contains(&cluster) {
            return Err(format!("invalid FAT12 cluster {cluster:#x}"));
        }
        let offset = data_start + (cluster - 2) * cluster_size;
        let take = remaining.min(cluster_size);
        out.extend_from_slice(
            image
                .get(offset..offset + take)
                .ok_or("DWG cluster runs past floppy image")?,
        );
        remaining -= take;
        let at = cluster * 3 / 2;
        let word = le16(fat, at)?;
        cluster = if cluster & 1 == 0 {
            word & 0x0fff
        } else {
            word >> 4
        };
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn invalid_drawing_name_is_rejected_before_boot() {
        assert!(generate_dwg(Path::new("/missing"), "TOO-LONG-NAME", &[]).is_err());
    }

    #[test]
    fn fat12_reader_extracts_the_original_executable() {
        let path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../corpus/raw/Autodesk AutoCAD 1.4 (5.25)/System.img");
        let Ok(image) = fs::read(path) else { return };
        let exe = fat12_file(&image, "ACAD.EXE").unwrap();
        assert_eq!(&exe[..2], b"MZ");
        assert_eq!(exe.len(), 78_848);
    }
}
