# QEMU CGA Monitor Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Decode AutoCAD 1.4's CGA screen from QEMU video memory and use it to run the original interactively in a window with keyboard input.

**Architecture:** A pure decoder (`cga.rs`) turns the 16 KiB `B8000h` dump into a 640×200 bitmap and classifies text vs graphics. A screenshot module (`screen.rs`) turns QEMU's PPM `screendump` into the same 640×400 display size for text mode. The private QMP `Vm` becomes a public `Session` (`session.rs`) with key-event, video-memory, and screenshot calls. A winit + softbuffer example (`examples/acad-qemu.rs`) polls the session at about 15 Hz and forwards keys.

**Tech Stack:** Rust 2021 (MSRV 1.88), `serde_json` (QMP), `winit 0.30` + `softbuffer 0.4` (dev-dependencies only), QEMU 11 `qemu-system-i386`.

**Spec:** `docs/superpowers/specs/2026-09-29-qemu-cga-monitor-design.md`

**Scope note:** The mouse (spec §4) is a follow-up plan, written after `DGMS.DRV`'s port and line settings are confirmed. This plan delivers spec §1–3 plus error handling and testing.

## Global Constraints

- **Precondition:** the working tree's pending changes (in `crates/acad-oracle/src/qemu.rs`, `lib.rs`, `Cargo.toml`, `tests/*.rs`, the untracked `acad-cmd` crate, etc.) are committed by the human partner before Task 1. Every task below edits those files; never commit someone else's pending hunks as part of a task.
- `acad-oracle` is dev-only: "this crate must not be a dependency of the shipped application." `winit`/`softbuffer` go in `[dev-dependencies]`.
- AutoCAD binaries are not modified; corpus images under `corpus/` are never written.
- QEMU keeps `-machine isapc -m 16 -d nochain` and the existing comment explaining `nochain`.
- QMP Unix socket paths must stay under 104 bytes: scratch directories live directly in `/tmp`.
- Oracle tests skip (return early with an `eprintln!`) when images or QEMU are absent.
- `cargo build -p acad-oracle --all-targets` produces no warnings; `cargo fmt` is applied.
- Commit messages end with:
  ```
  Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>
  Claude-Session: https://claude.ai/code/session_01XtaKRW1Ehc4thUEp1gtT2W
  ```

## Review Focus

1. A drawing whose top half is solid-filled (bytes `0xFF`) must stay classified as graphics, not text. Pinned in Task 1 (`solid_fill_is_graphics`).
2. A text screen full of varied characters, such as the help listing, must still be text. Pinned in Task 1 (`dense_text_is_text`).
3. A screenshot of an unexpected size (for example, 640×480 after a guest mode change) must yield 640×400 without panicking. Pinned in Task 2 (`other_sizes_scale_without_panicking`).
4. Resizing the window smaller than 640×400 or to an odd size must not panic and must keep the picture top-left anchored. Pinned in Task 5 (`blit_*`).
5. Switching window focus while a key is held must release it in the guest, not leave Shift or Ctrl stuck. Pinned in Task 5 (`held_keys_release_everything_once`).

---

### Task 1: CGA decoder and mode detection

**Files:**
- Create: `crates/acad-oracle/src/cga.rs`
- Modify: `crates/acad-oracle/src/lib.rs` (add `pub mod cga;`)

**Interfaces:**
- Produces:
  - `pub const VIDEO_BYTES: usize = 16384; pub const DISPLAY_WIDTH: usize = 640; pub const DISPLAY_HEIGHT: usize = 400;`
  - `pub enum Mode { Text, Graphics }` (`Clone, Copy, Debug, PartialEq, Eq`)
  - `pub struct Frame<'a>`; `Frame::new(memory: &'a [u8]) -> Result<Frame<'a>, String>`; `Frame::lit(&self, x: usize, y: usize) -> bool` (x < 640, y < 200); `Frame::to_rgb_640x400(&self) -> Vec<u32>` (`0x00RRGGBB`)
  - `pub fn detect(memory: &[u8]) -> Option<Mode>`
  - `pub struct ModeTracker`; `ModeTracker::new() -> Self` (starts in `Text`); `ModeTracker::update(&mut self, memory: &[u8]) -> Mode`

- [ ] **Step 1: Write the failing tests**

Create `crates/acad-oracle/src/cga.rs` with only the test module:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    fn text_page(chars: impl Fn(usize) -> u8, attribute: u8) -> Vec<u8> {
        let mut memory = vec![0; VIDEO_BYTES];
        for cell in 0..2000 {
            memory[cell * 2] = chars(cell);
            memory[cell * 2 + 1] = attribute;
        }
        memory
    }

    #[test]
    fn scanlines_interleave_and_high_bit_is_leftmost() {
        let mut memory = vec![0; VIDEO_BYTES];
        memory[0] = 0x80; // (0, 0)
        memory[0x2000] = 0x01; // (7, 1)
        memory[80] = 0x40; // (1, 2)
        memory[0x2000 + 99 * 80 + 79] = 0x01; // (639, 199)
        let frame = Frame::new(&memory).unwrap();
        assert!(frame.lit(0, 0) && frame.lit(7, 1) && frame.lit(1, 2) && frame.lit(639, 199));
        let total = (0..200)
            .flat_map(|y| (0..640).map(move |x| (x, y)))
            .filter(|&(x, y)| frame.lit(x, y))
            .count();
        assert_eq!(total, 4);
    }

    #[test]
    fn frame_rejects_wrong_length() {
        assert!(Frame::new(&[0; 4000]).is_err());
    }

    #[test]
    fn display_doubles_each_scanline() {
        let mut memory = vec![0; VIDEO_BYTES];
        memory[0] = 0x80;
        let rgb = Frame::new(&memory).unwrap().to_rgb_640x400();
        assert_eq!(rgb.len(), DISPLAY_WIDTH * DISPLAY_HEIGHT);
        assert_eq!((rgb[0], rgb[640], rgb[1280], rgb[1]), (0xFF_FFFF, 0xFF_FFFF, 0, 0));
    }

    #[test]
    fn blank_dos_screen_is_text() {
        assert_eq!(detect(&text_page(|_| b' ', 0x07)), Some(Mode::Text));
    }

    #[test]
    fn dense_text_is_text() {
        let page = text_page(|cell| (cell % 95) as u8 + 32, 0x07);
        assert_eq!(detect(&page), Some(Mode::Text));
    }

    #[test]
    fn blank_bitmap_is_graphics() {
        assert_eq!(detect(&vec![0; VIDEO_BYTES]), Some(Mode::Graphics));
    }

    #[test]
    fn busy_bitmap_is_graphics() {
        let mut seed = 12345u32;
        let memory: Vec<u8> = (0..VIDEO_BYTES)
            .map(|_| {
                seed = seed.wrapping_mul(1_103_515_245).wrapping_add(12345);
                (seed >> 16) as u8
            })
            .collect();
        assert_eq!(detect(&memory), Some(Mode::Graphics));
    }

    #[test]
    fn solid_fill_is_graphics() {
        assert_eq!(detect(&vec![0xFF; VIDEO_BYTES]), Some(Mode::Graphics));
    }

    #[test]
    fn short_memory_is_undecided() {
        assert_eq!(detect(&[0x20, 0x07]), None);
    }

    #[test]
    fn tracker_needs_two_agreeing_frames() {
        let text = text_page(|_| b' ', 0x07);
        let graphics = vec![0; VIDEO_BYTES];
        let mut tracker = ModeTracker::new();
        assert_eq!(tracker.update(&graphics), Mode::Text);
        assert_eq!(tracker.update(&graphics), Mode::Graphics);
        assert_eq!(tracker.update(&text), Mode::Graphics);
        assert_eq!(tracker.update(&graphics), Mode::Graphics);
        assert_eq!(tracker.update(&text), Mode::Graphics);
        assert_eq!(tracker.update(&[0]), Mode::Graphics);
        assert_eq!(tracker.update(&text), Mode::Text);
    }
}
```

In the last test, the undecidable `&[0]` does not reset the pending text vote. Two text frames with only an undecided frame between them switch the mode.

Add `pub mod cga;` to `crates/acad-oracle/src/lib.rs` after `//! Development-only oracle for the original AutoCAD.` and before `pub mod cpu8086;`.

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p acad-oracle --lib cga`
Expected: compile errors: `cannot find type Frame`, `cannot find function detect`, `VIDEO_BYTES` not found.

- [ ] **Step 3: Implement**

Put this above the test module in `cga.rs`:

```rust
//! AutoCAD's CGA screen, reconstructed from a 16 KiB dump of `B800:0000`.
//!
//! The original's display driver programs the CGA mode register directly,
//! which QEMU's VGA ignores, so the adapter's own picture is useless. Video
//! memory is still exactly what a CGA would scan out.

pub const VIDEO_BYTES: usize = 16384;
pub const DISPLAY_WIDTH: usize = 640;
pub const DISPLAY_HEIGHT: usize = 400;
const BANK: usize = 0x2000;
const ROW_BYTES: usize = 80;
const TEXT_CELLS: usize = 2000;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    Text,
    Graphics,
}

/// The 640×200 two-colour bitmap. Even scanlines start at offset 0, odd
/// scanlines at `0x2000`; the high bit of each byte is the leftmost pixel.
pub struct Frame<'a>(&'a [u8]);

impl<'a> Frame<'a> {
    pub fn new(memory: &'a [u8]) -> Result<Self, String> {
        if memory.len() != VIDEO_BYTES {
            return Err(format!(
                "CGA memory is {} bytes, expected {VIDEO_BYTES}",
                memory.len()
            ));
        }
        Ok(Self(memory))
    }

    pub fn lit(&self, x: usize, y: usize) -> bool {
        self.0[(y % 2) * BANK + (y / 2) * ROW_BYTES + x / 8] & (0x80 >> (x % 8)) != 0
    }

    /// White-on-black `0x00RRGGBB` pixels with each scanline shown twice,
    /// matching the 640×400 text screenshot size.
    pub fn to_rgb_640x400(&self) -> Vec<u32> {
        (0..DISPLAY_HEIGHT)
            .flat_map(|y| (0..DISPLAY_WIDTH).map(move |x| (x, y / 2)))
            .map(|(x, y)| if self.lit(x, y) { 0xFF_FFFF } else { 0 })
            .collect()
    }
}

/// Classify a dump without seeing the mode register. An 80×25 text page
/// stores character/attribute pairs, and DOS writes nearly every attribute
/// with the same nonzero value. Blank bitmaps are zero; drawn ones vary; a
/// solid fill repeats one value in the character bytes as well.
pub fn detect(memory: &[u8]) -> Option<Mode> {
    let page = memory.get(..TEXT_CELLS * 2)?;
    let mut counts = [0usize; 256];
    for pair in page.chunks_exact(2) {
        counts[usize::from(pair[1])] += 1;
    }
    let (value, &count) = counts
        .iter()
        .enumerate()
        .max_by_key(|&(_, count)| *count)
        .expect("256 counters");
    let same_chars = page
        .chunks_exact(2)
        .filter(|pair| usize::from(pair[0]) == value)
        .count();
    let dominant = |n: usize| n * 100 >= TEXT_CELLS * 95;
    let text = value != 0 && dominant(count) && !dominant(same_chars);
    Some(if text { Mode::Text } else { Mode::Graphics })
}

/// Mode with hysteresis: a change needs two consecutive agreeing frames, so a
/// partially redrawn screen does not flicker. Undecidable frames are ignored.
pub struct ModeTracker {
    current: Mode,
    pending: Option<Mode>,
}

impl ModeTracker {
    /// DOS boots in text mode.
    pub fn new() -> Self {
        Self {
            current: Mode::Text,
            pending: None,
        }
    }

    pub fn update(&mut self, memory: &[u8]) -> Mode {
        match detect(memory) {
            None => {}
            Some(mode) if mode == self.current => self.pending = None,
            Some(mode) if self.pending == Some(mode) => {
                self.current = mode;
                self.pending = None;
            }
            Some(mode) => self.pending = Some(mode),
        }
        self.current
    }
}

impl Default for ModeTracker {
    fn default() -> Self {
        Self::new()
    }
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p acad-oracle --lib cga`
Expected: 10 passed.

- [ ] **Step 5: Commit**

```bash
cargo fmt -p acad-oracle
git add crates/acad-oracle/src/cga.rs crates/acad-oracle/src/lib.rs
git commit -m "feat(oracle): decode CGA frames and detect text or graphics mode"
```

(With the trailer from Global Constraints.)

---

### Task 2: QEMU screenshots at display size

**Files:**
- Create: `crates/acad-oracle/src/screen.rs`
- Modify: `crates/acad-oracle/src/lib.rs` (add `pub mod screen;` after `pub mod mz_loader;`)

**Interfaces:**
- Consumes: `crate::cga::{DISPLAY_WIDTH, DISPLAY_HEIGHT}` (Task 1)
- Produces: `pub struct Screen { pub width: usize, pub height: usize, pub rgb: Vec<u8> }`; `Screen::from_ppm(bytes: &[u8]) -> Result<Screen, String>`; `Screen::to_640x400(&self) -> Vec<u32>` (`0x00RRGGBB`)

- [ ] **Step 1: Write the failing tests**

Create `crates/acad-oracle/src/screen.rs` with:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    fn ppm(width: usize, height: usize, pixel: impl Fn(usize, usize) -> [u8; 3]) -> Vec<u8> {
        let mut bytes = format!("P6\n{width} {height}\n255\n").into_bytes();
        for y in 0..height {
            for x in 0..width {
                bytes.extend(pixel(x, y));
            }
        }
        bytes
    }

    #[test]
    fn parses_qemu_ppm() {
        let screen = Screen::from_ppm(&ppm(2, 1, |x, _| [x as u8, 2, 3])).unwrap();
        assert_eq!((screen.width, screen.height), (2, 1));
        assert_eq!(screen.rgb, [0, 2, 3, 1, 2, 3]);
    }

    #[test]
    fn rejects_malformed_ppm() {
        assert!(Screen::from_ppm(b"P5\n1 1\n255\n\0").is_err());
        assert!(Screen::from_ppm(b"P6\n1 1\n").is_err());
        assert!(Screen::from_ppm(b"P6\n2 2\n255\n\0\0\0").is_err());
        assert!(Screen::from_ppm(b"P6\n1 1\n65535\n\0\0\0\0\0\0").is_err());
        assert!(Screen::from_ppm(b"P6\n0 0\n255\n").is_err());
    }

    #[test]
    fn vga_text_drops_ninth_cell_column() {
        let screen = Screen::from_ppm(&ppm(720, 400, |x, y| [(x % 9) as u8 * 10, y as u8, 0])).unwrap();
        let out = screen.to_640x400();
        assert_eq!(out.len(), DISPLAY_WIDTH * DISPLAY_HEIGHT);
        for x in 0..DISPLAY_WIDTH {
            assert_eq!(out[x] >> 16, (x % 8) as u32 * 10, "column {x}");
        }
        assert_eq!((out[DISPLAY_WIDTH * 399] >> 8) & 0xFF, 399 % 256);
    }

    #[test]
    fn other_sizes_scale_without_panicking() {
        for (w, h) in [(640, 480), (1, 1), (360, 400), (1024, 768)] {
            let screen = Screen::from_ppm(&ppm(w, h, |_, _| [9, 8, 7])).unwrap();
            let out = screen.to_640x400();
            assert_eq!(out.len(), DISPLAY_WIDTH * DISPLAY_HEIGHT);
            assert!(out.iter().all(|&p| p == 0x09_0807), "{w}x{h}");
        }
    }
}
```

Add `pub mod screen;` to `lib.rs` after `pub mod mz_loader;`.

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p acad-oracle --lib screen`
Expected: compile error `cannot find type Screen`.

- [ ] **Step 3: Implement**

Put above the tests in `screen.rs`:

```rust
//! QEMU `screendump` images, scaled to the CGA monitor's 640×400 display.
//! Text-mode screens are rendered correctly by QEMU itself.

use crate::cga::{DISPLAY_HEIGHT, DISPLAY_WIDTH};

pub struct Screen {
    pub width: usize,
    pub height: usize,
    pub rgb: Vec<u8>,
}

impl Screen {
    /// Parse binary PPM (`P6`, maxval 255), the format QEMU's `screendump`
    /// writes by default.
    pub fn from_ppm(bytes: &[u8]) -> Result<Self, String> {
        let mut fields = Vec::new();
        let mut at = 0;
        while fields.len() < 4 {
            while bytes.get(at).is_some_and(u8::is_ascii_whitespace) {
                at += 1;
            }
            let start = at;
            while bytes.get(at).is_some_and(|b| !b.is_ascii_whitespace()) {
                at += 1;
            }
            if start == at {
                return Err("truncated PPM header".into());
            }
            fields.push(std::str::from_utf8(&bytes[start..at]).map_err(|_| "non-ASCII PPM header")?);
        }
        at += 1; // the single whitespace byte that ends the header
        if fields[0] != "P6" {
            return Err(format!("unsupported PPM magic {:?}", fields[0]));
        }
        let number = |s: &str| s.parse::<usize>().map_err(|_| format!("bad PPM number {s:?}"));
        let (width, height, max) = (number(fields[1])?, number(fields[2])?, number(fields[3])?);
        if max != 255 {
            return Err(format!("unsupported PPM maxval {max}"));
        }
        if width == 0 || height == 0 {
            return Err("empty PPM image".into());
        }
        let len = width
            .checked_mul(height)
            .and_then(|n| n.checked_mul(3))
            .ok_or("PPM size overflow")?;
        let rgb = bytes
            .get(at..)
            .and_then(|rest| rest.get(..len))
            .ok_or("truncated PPM pixels")?
            .to_vec();
        Ok(Self { width, height, rgb })
    }

    /// `0x00RRGGBB` pixels, 640×400. A 720-pixel-wide VGA text screen drops
    /// the ninth column of each 9-pixel character cell: CGA cells are 8
    /// pixels wide, and VGA's ninth column only repeats the eighth for
    /// line-drawing glyphs. Other sizes use nearest-neighbour scaling.
    pub fn to_640x400(&self) -> Vec<u32> {
        let column = |x: usize| {
            if self.width == 720 {
                x / 8 * 9 + x % 8
            } else {
                x * self.width / DISPLAY_WIDTH
            }
        };
        let mut out = Vec::with_capacity(DISPLAY_WIDTH * DISPLAY_HEIGHT);
        for y in 0..DISPLAY_HEIGHT {
            let row = y * self.height / DISPLAY_HEIGHT * self.width;
            for x in 0..DISPLAY_WIDTH {
                let i = (row + column(x)) * 3;
                out.push(
                    u32::from(self.rgb[i]) << 16
                        | u32::from(self.rgb[i + 1]) << 8
                        | u32::from(self.rgb[i + 2]),
                );
            }
        }
        out
    }
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p acad-oracle --lib screen`
Expected: 4 passed.

- [ ] **Step 5: Commit**

```bash
cargo fmt -p acad-oracle
git add crates/acad-oracle/src/screen.rs crates/acad-oracle/src/lib.rs
git commit -m "feat(oracle): scale QEMU screenshots to the CGA display size"
```

---

### Task 3: Public QEMU `Session`

This moves the private `Vm` out of `qemu.rs` without changing harness behavior, then adds key events, video memory, screenshots, and an exit check.

**Files:**
- Create: `crates/acad-oracle/src/session.rs`
- Modify: `crates/acad-oracle/src/qemu.rs` (delete `struct Vm`, `impl Vm`, `impl Drop for Vm`, `QEMU_LOCK`, the timeout constants; make `promote_backup` `pub(crate)`; switch callers)
- Modify: `crates/acad-oracle/src/lib.rs` (add `#[cfg(unix)] pub mod session;`)
- Test: `crates/acad-oracle/tests/cga_monitor.rs`

**Interfaces:**
- Consumes: `crate::screen::Screen::from_ppm` (Task 2); `crate::qemu::promote_backup(image: &mut [u8], name: &str) -> Result<(), String>`
- Produces (all on `pub struct Session`):
  - `pub fn boot_disposable(system: &Path, samples: Option<&Path>, backups: &[&str]) -> Result<Session, String>` (copies to `dir()/system.img`, `dir()/samples.img`)
  - `pub fn boot_in_place(a: &Path, b: Option<&Path>) -> Result<Session, String>`
  - `pub fn dir(&self) -> &Path`
  - `pub fn key(&mut self, qcode: &str, down: bool) -> Result<(), String>`
  - `pub fn video_memory(&mut self) -> Result<Vec<u8>, String>` (16384 bytes)
  - `pub fn screendump(&mut self) -> Result<Screen, String>`
  - `pub fn exit_status(&mut self) -> Option<std::process::ExitStatus>` (`Some` once QEMU has exited)
  - Moved as `pub`: `type_line`, `text_screen`, `capture_editor`, `wait_for_text`, `wait_until_text_gone`, `shutdown`
  - `pub(crate) const BOOT_TIMEOUT: Duration` (30 s), `pub(crate) const EDIT_TIMEOUT: Duration` (20 s)

- [ ] **Step 1: Write the failing oracle test**

Create `crates/acad-oracle/tests/cga_monitor.rs`:

```rust
#![cfg(unix)]

use acad_oracle::cga::{detect, Frame, Mode};
use acad_oracle::session::Session;
use std::ops::Range;
use std::path::Path;
use std::thread;
use std::time::Duration;

const WAIT: Duration = Duration::from_secs(30);

#[test]
fn monitor_follows_text_menu_graphics_editor_and_back() {
    let system = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../corpus/raw/Autodesk AutoCAD 1.4 (5.25)/System.img");
    if !system.exists() || !acad_oracle::available() {
        eprintln!("skipping oracle: extracted System.img or qemu-system-i386 absent");
        return;
    }
    let mut vm = Session::boot_disposable(&system, None, &[]).unwrap();
    vm.wait_for_text("Enter selection:", WAIT).unwrap();
    assert_eq!(detect(&vm.video_memory().unwrap()), Some(Mode::Text));
    let menu = vm.screendump().unwrap();
    assert_eq!((menu.width, menu.height), (720, 400));

    // Select "New drawing" through input-send-event, the runner's key path.
    for qcode in ["1", "ret"] {
        vm.key(qcode, true).unwrap();
        vm.key(qcode, false).unwrap();
        thread::sleep(Duration::from_millis(150));
    }
    vm.wait_for_text("Enter NAME of drawing:", WAIT).unwrap();
    vm.type_line("CGAMON").unwrap();
    vm.wait_until_text_gone("Enter NAME of drawing:", WAIT).unwrap();
    thread::sleep(Duration::from_millis(500));
    for line in ["LINE", "1,1", "11,8", ""] {
        vm.type_line(line).unwrap();
    }
    let memory = vm.capture_editor().unwrap();
    assert_eq!(detect(&memory), Some(Mode::Graphics));
    let frame = Frame::new(&memory).unwrap();
    let lit = |xs: Range<usize>, ys: Range<usize>| {
        ys.flat_map(|y| xs.clone().map(move |x| (x, y)))
            .filter(|&(x, y)| frame.lit(x, y))
            .count()
    };
    assert!(lit(572..640, 0..166) > 200, "screen menu missing");
    assert!(lit(0..640, 172..200) > 50, "prompt area missing");
    assert!(lit(0..560, 10..160) > 100, "drawn line missing");

    vm.type_line("END").unwrap();
    vm.wait_for_text("Enter selection:", WAIT).unwrap();
    assert_eq!(detect(&vm.video_memory().unwrap()), Some(Mode::Text));
    vm.shutdown().unwrap();
    assert!(vm.exit_status().is_some());
}
```

The region bounds come from a decoded editor frame: the side menu is right of x = 572 above its divider at y ≈ 168, and the prompt occupies the last three 8-pixel rows (y 176–199).

Add `#[cfg(unix)] pub mod session;` to `lib.rs`, directly below `mod qemu;`'s `#[cfg(unix)]` block:

```rust
#[cfg(unix)]
mod qemu;
#[cfg(unix)]
pub mod session;
```

Create an empty `crates/acad-oracle/src/session.rs` so the crate resolves the module.

- [ ] **Step 2: Run the test to verify it fails**

Run: `cargo test -p acad-oracle --test cga_monitor`
Expected: compile error `cannot find type Session in module acad_oracle::session`.

- [ ] **Step 3: Write `session.rs`**

Replace `crates/acad-oracle/src/session.rs` with the code below. The bodies of `type_line`, `text_screen`, `capture_editor`, `wait_for_text`, `wait_until_text_gone`, and `shutdown` are the current `impl Vm` bodies from `qemu.rs`, moved unchanged and made `pub`.

```rust
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

    /// The 16 KiB CGA region at `B8000h`.
    pub fn video_memory(&mut self) -> Result<Vec<u8>, String> {
        let dump = self.dir.join("video.bin");
        self.command(json!({
            "execute": "pmemsave",
            "arguments": {"val": 0xb8000, "size": crate::cga::VIDEO_BYTES, "filename": dump}
        }))?;
        fs::read(dump).map_err(|e| format!("read video memory: {e}"))
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
        // body moved unchanged from qemu.rs `Vm::command`
    }

    pub fn type_line(&mut self, line: &str) -> Result<(), String> {
        // body moved unchanged from qemu.rs `Vm::type_line`
    }

    pub fn text_screen(&mut self) -> Result<String, String> {
        // body moved unchanged from qemu.rs `Vm::text_screen`
    }

    pub fn capture_editor(&mut self) -> Result<Vec<u8>, String> {
        // body moved unchanged from qemu.rs `Vm::capture_editor`
    }

    pub fn wait_for_text(&mut self, needle: &str, timeout: Duration) -> Result<(), String> {
        // body moved unchanged from qemu.rs `Vm::wait_for_text`
    }

    pub fn wait_until_text_gone(&mut self, needle: &str, timeout: Duration) -> Result<(), String> {
        // body moved unchanged from qemu.rs `Vm::wait_until_text_gone`
    }

    pub fn shutdown(&mut self) -> Result<(), String> {
        // body moved unchanged from qemu.rs `Vm::shutdown`
    }
}

impl Drop for Session {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
        let _ = fs::remove_dir_all(&self.dir);
    }
}
```

The seven `// body moved unchanged` markers mean: cut the function body verbatim from `qemu.rs` (currently `command` at `qemu.rs:397`, `type_line` 420, `text_screen` 449, `capture_editor` 471, `wait_for_text` 502, `wait_until_text_gone` 523, `shutdown` 534) and paste it here. No edits are needed; the bodies use only `self.command`, `self.dir`, `self.child`, `self.reader`, `self.writer`, `EDIT_TIMEOUT`, and std items imported above. They are not new code, so they are not reproduced.

- [ ] **Step 4: Switch `qemu.rs` to `Session`**

In `crates/acad-oracle/src/qemu.rs`:
1. Delete `struct Vm`, `impl Vm`, `impl Drop for Vm`, `static QEMU_LOCK`, `const BOOT_TIMEOUT`, `const EDIT_TIMEOUT`.
2. Add `use crate::session::{Session, BOOT_TIMEOUT, EDIT_TIMEOUT};`.
3. Replace each `Vm::boot(a, b, backups)` with `Session::boot_disposable(a, b, backups)`. There are three: in `export`, `open_drawing`, and `run`.
4. Replace `vm.image_path()` with `vm.dir().join("system.img")` and `vm.dir.join("samples.img")` with `vm.dir().join("samples.img")`.
5. Change `fn promote_backup` to `pub(crate) fn promote_backup`.
6. Remove imports that are now unused (`serde_json`, `BufRead`, `BufReader`, `Write`, `UnixStream`, `Child`, `Mutex`, `MutexGuard`, `Instant`, as the compiler reports). Keep `NEXT_RUN` for `open_drawing`.

Run: `cargo build -p acad-oracle --all-targets 2>&1 | grep -E "^(warning|error)" ; echo done`
Expected: only `done`.

- [ ] **Step 5: Run the new oracle test and the existing suite**

Run: `cargo test -p acad-oracle --test cga_monitor -- --nocapture`
Expected: 1 passed. It takes about a minute with QEMU and corpus present; otherwise it prints the skip message.

If `detect` misclassifies a real frame here, print the dominant attribute value and its count from the failing dump before changing the threshold in `cga.rs`. Record the observed values in the commit message.

Run: `cargo test -p acad-oracle`
Expected: all tests pass, including `empty_drawing`, `commands`, `sample_exports`, `shapes`, `font_render`. The existing oracle runs take several minutes.

- [ ] **Step 6: Commit**

```bash
cargo fmt -p acad-oracle
git add crates/acad-oracle/src/session.rs crates/acad-oracle/src/qemu.rs crates/acad-oracle/src/lib.rs crates/acad-oracle/tests/cga_monitor.rs
git commit -m "feat(oracle): expose a QEMU session with keys, video memory and screenshots"
```

---

### Task 4: Oracle tests use the shared decoder

**Files:**
- Modify: `crates/acad-oracle/tests/font_render.rs:13-15,63`
- Modify: `crates/acad-oracle/tests/commands.rs` (the two viewport loops in `original_opens_and_renders_a_rust_written_dwg` and `ac12_writer_matches_the_original_subdiv_viewport_in_autocad`)

**Interfaces:**
- Consumes: `acad_oracle::cga::Frame::{new, lit}` (Task 1)

- [ ] **Step 1: Replace `font_render.rs`'s private helper**

Delete:

```rust
fn lit(frame: &[u8], x: usize, y: usize) -> bool {
    frame[(y % 2) * 8192 + (y / 2) * 80 + x / 8] & (128 >> (x % 8)) != 0
}
```

After `assert_eq!(probe.cga.len(), 16384);` add:

```rust
    let frame = acad_oracle::cga::Frame::new(&probe.cga).unwrap();
```

Change `if lit(&probe.cga, x, y) {` to `if frame.lit(x, y) {`. Then search the file for any other `lit(&` and change it the same way.

- [ ] **Step 2: Replace the address arithmetic in `commands.rs`**

In `original_opens_and_renders_a_rust_written_dwg`, before the loop, add:

```rust
    let (source, reopened) = (
        acad_oracle::cga::Frame::new(&probe.cga).unwrap(),
        acad_oracle::cga::Frame::new(&opened).unwrap(),
    );
```

and replace the loop body's first four lines with:

```rust
            let expected = source.lit(x, y);
            let actual = reopened.lit(x, y);
```

In `ac12_writer_matches_the_original_subdiv_viewport_in_autocad`, add before its loop:

```rust
    let (native, rewritten) = (
        acad_oracle::cga::Frame::new(&native).unwrap(),
        acad_oracle::cga::Frame::new(&rewritten).unwrap(),
    );
```

and replace the loop body with:

```rust
            pixels += 1;
            mismatches += usize::from(native.lit(x, y) != rewritten.lit(x, y));
```

Delete the `let bit = ...` and `let at = ...` lines in both loops.

- [ ] **Step 3: Run the affected tests**

Run: `cargo test -p acad-oracle --test font_render --test commands`
Expected: all pass (unchanged assertions).

- [ ] **Step 4: Commit**

```bash
cargo fmt -p acad-oracle
git add crates/acad-oracle/tests/font_render.rs crates/acad-oracle/tests/commands.rs
git commit -m "test(oracle): read CGA pixels through the shared frame decoder"
```

---

### Task 5: Interactive runner

**Files:**
- Create: `crates/acad-oracle/examples/acad-qemu.rs`
- Modify: `crates/acad-oracle/Cargo.toml` (`[dev-dependencies]`: `winit = "0.30"`, `softbuffer = "0.4"`)
- Modify: `docs/oracle-qemu.md` (new "Interactive runner" section before "## Remaining limits")

**Interfaces:**
- Consumes: `Session::{boot_in_place, key, video_memory, screendump, exit_status, shutdown}` (Task 3); `cga::{Frame, Mode, ModeTracker, DISPLAY_WIDTH, DISPLAY_HEIGHT}` (Task 1); `Screen::to_640x400` (Task 2); `acad_oracle::available`
- Produces: binary `cargo run -p acad-oracle --example acad-qemu [-- --fresh]`

- [ ] **Step 1: Write the failing tests**

Add to `[dev-dependencies]` in `crates/acad-oracle/Cargo.toml`:

```toml
winit = "0.30"
softbuffer = "0.4"
```

Create `crates/acad-oracle/examples/acad-qemu.rs` containing only `fn main() {}` and this test module:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    const REQUIRED: &[KeyCode] = &[
        KeyCode::KeyA, KeyCode::KeyZ, KeyCode::Digit0, KeyCode::Digit9, KeyCode::Minus,
        KeyCode::Equal, KeyCode::BracketLeft, KeyCode::BracketRight, KeyCode::Backslash,
        KeyCode::Semicolon, KeyCode::Quote, KeyCode::Backquote, KeyCode::Comma,
        KeyCode::Period, KeyCode::Slash, KeyCode::Space, KeyCode::Enter, KeyCode::Backspace,
        KeyCode::Tab, KeyCode::Escape, KeyCode::ArrowUp, KeyCode::ArrowDown,
        KeyCode::ArrowLeft, KeyCode::ArrowRight, KeyCode::Home, KeyCode::End,
        KeyCode::PageUp, KeyCode::PageDown, KeyCode::Insert, KeyCode::Delete, KeyCode::F1,
        KeyCode::F10, KeyCode::ShiftLeft, KeyCode::ShiftRight, KeyCode::ControlLeft,
        KeyCode::ControlRight, KeyCode::AltLeft, KeyCode::AltRight, KeyCode::CapsLock,
    ];

    #[test]
    fn required_keys_map_to_distinct_qcodes() {
        let mut seen = std::collections::BTreeSet::new();
        for &key in REQUIRED {
            let q = qcode(key).unwrap_or_else(|| panic!("{key:?} unmapped"));
            assert!(seen.insert(q), "{q} mapped twice");
        }
        assert_eq!(qcode(KeyCode::KeyQ), Some("q"));
        assert_eq!(qcode(KeyCode::Enter), Some("ret"));
        assert_eq!(qcode(KeyCode::SuperLeft), None);
    }

    fn picture() -> Vec<u32> {
        (1..=(DISPLAY_WIDTH * DISPLAY_HEIGHT) as u32).collect()
    }

    #[test]
    fn blit_scales_by_whole_pixels() {
        let out = blit(&picture(), 1280, 800);
        assert_eq!((out[0], out[1], out[2]), (1, 1, 2));
        assert_eq!(out[1280], 1);
        assert_eq!(out[2 * 1280], 641);
    }

    #[test]
    fn blit_centres_leftover_space() {
        let out = blit(&picture(), 1300, 800);
        assert_eq!((out[9], out[10], out[11], out[12]), (0, 1, 1, 2));
    }

    #[test]
    fn blit_clips_small_and_odd_windows() {
        for (w, h) in [(300, 200), (1, 1), (641, 399)] {
            let out = blit(&picture(), w, h);
            assert_eq!(out.len(), w * h);
            assert_eq!(out[0], 1);
            if w > 1 {
                assert_eq!(out[1], 2);
            }
        }
    }

    #[test]
    fn held_keys_release_everything_once() {
        let mut held = HeldKeys::default();
        held.update("shift", true);
        held.update("a", true);
        held.update("a", true);
        held.update("a", false);
        held.update("ctrl", true);
        assert_eq!(held.drain(), ["ctrl", "shift"]);
        assert!(held.drain().is_empty());
    }

    #[test]
    fn working_disks_copy_once_and_refresh_on_demand() {
        let base = std::env::temp_dir().join(format!("acad-qemu-test-{}", std::process::id()));
        let corpus = base.join("corpus");
        let work = base.join("work");
        std::fs::create_dir_all(&corpus).unwrap();
        std::fs::write(corpus.join("System.img"), b"system").unwrap();
        assert!(working_disks(&corpus, &work, false).is_err(), "Samples missing");
        std::fs::write(corpus.join("Samples.img"), b"samples").unwrap();
        let [system, samples] = working_disks(&corpus, &work, false).unwrap();
        assert_eq!(std::fs::read(&samples).unwrap(), b"samples");
        std::fs::write(&system, b"edited").unwrap();
        working_disks(&corpus, &work, false).unwrap();
        assert_eq!(std::fs::read(&system).unwrap(), b"edited");
        working_disks(&corpus, &work, true).unwrap();
        assert_eq!(std::fs::read(&system).unwrap(), b"system");
        std::fs::remove_dir_all(base).unwrap();
    }
}
```

`blit_centres_leftover_space`: a 1300×800 window has scale 2 and 20 spare columns, so the picture starts at x = 10. Test pixels are numbered from 1, so 0 means the black border: column 9 is border, columns 10–11 show the first source pixel, and column 12 shows the second.

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p acad-oracle --example acad-qemu`
Expected: compile errors: `qcode`, `blit`, `HeldKeys`, `working_disks` not found.

- [ ] **Step 3: Implement the runner**

Replace `fn main() {}` with:

```rust
//! Interactive AutoCAD 1.4 under QEMU, shown through a host-side CGA monitor.
//! QEMU's adapter ignores the CGA mode register, so this window decodes video
//! memory itself in graphics mode and uses QEMU's screenshot in text mode.
//!
//! `cargo run -p acad-oracle --example acad-qemu [-- --fresh]`

use acad_oracle::cga::{Frame, Mode, ModeTracker, DISPLAY_HEIGHT, DISPLAY_WIDTH};
use acad_oracle::session::Session;
use std::fs;
use std::num::NonZeroU32;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::time::{Duration, Instant};
use winit::application::ApplicationHandler;
use winit::dpi::LogicalSize;
use winit::event::{ElementState, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::keyboard::{KeyCode, PhysicalKey};
use winit::window::{Window, WindowId};

const FRAME_INTERVAL: Duration = Duration::from_millis(66);

type WindowState = (Rc<Window>, softbuffer::Surface<Rc<Window>, Rc<Window>>);

/// Host key → QEMU qcode. Keys absent here are reported and dropped.
fn qcode(key: KeyCode) -> Option<&'static str> {
    use KeyCode::*;
    Some(match key {
        KeyA => "a", KeyB => "b", KeyC => "c", KeyD => "d", KeyE => "e", KeyF => "f",
        KeyG => "g", KeyH => "h", KeyI => "i", KeyJ => "j", KeyK => "k", KeyL => "l",
        KeyM => "m", KeyN => "n", KeyO => "o", KeyP => "p", KeyQ => "q", KeyR => "r",
        KeyS => "s", KeyT => "t", KeyU => "u", KeyV => "v", KeyW => "w", KeyX => "x",
        KeyY => "y", KeyZ => "z",
        Digit0 => "0", Digit1 => "1", Digit2 => "2", Digit3 => "3", Digit4 => "4",
        Digit5 => "5", Digit6 => "6", Digit7 => "7", Digit8 => "8", Digit9 => "9",
        Minus => "minus", Equal => "equal", BracketLeft => "bracket_left",
        BracketRight => "bracket_right", Backslash => "backslash", Semicolon => "semicolon",
        Quote => "apostrophe", Backquote => "grave_accent", Comma => "comma", Period => "dot",
        Slash => "slash", Space => "spc", Enter => "ret", Backspace => "backspace",
        Tab => "tab", Escape => "esc", ArrowUp => "up", ArrowDown => "down",
        ArrowLeft => "left", ArrowRight => "right", Home => "home", End => "end",
        PageUp => "pgup", PageDown => "pgdn", Insert => "insert", Delete => "delete",
        F1 => "f1", F2 => "f2", F3 => "f3", F4 => "f4", F5 => "f5", F6 => "f6", F7 => "f7",
        F8 => "f8", F9 => "f9", F10 => "f10",
        ShiftLeft => "shift", ShiftRight => "shift_r", ControlLeft => "ctrl",
        ControlRight => "ctrl_r", AltLeft => "alt", AltRight => "alt_r",
        CapsLock => "caps_lock",
        _ => return None,
    })
}

/// Keys the guest currently sees as down, so focus loss can release them.
#[derive(Default)]
struct HeldKeys(std::collections::BTreeSet<&'static str>);

impl HeldKeys {
    fn update(&mut self, qcode: &'static str, down: bool) {
        if down {
            self.0.insert(qcode);
        } else {
            self.0.remove(qcode);
        }
    }

    fn drain(&mut self) -> Vec<&'static str> {
        std::mem::take(&mut self.0).into_iter().collect()
    }
}

/// Integer-scale the 640×400 picture into a `width`×`height` buffer,
/// centred, with black around it; windows smaller than the picture clip it.
fn blit(picture: &[u32], width: usize, height: usize) -> Vec<u32> {
    let scale = (width / DISPLAY_WIDTH).min(height / DISPLAY_HEIGHT).max(1);
    let left = width.saturating_sub(DISPLAY_WIDTH * scale) / 2;
    let top = height.saturating_sub(DISPLAY_HEIGHT * scale) / 2;
    let mut out = vec![0; width * height];
    for y in 0..height {
        let Some(sy) = y.checked_sub(top).map(|v| v / scale) else { continue };
        if sy >= DISPLAY_HEIGHT {
            continue;
        }
        for x in 0..width {
            let Some(sx) = x.checked_sub(left).map(|v| v / scale) else { continue };
            if sx < DISPLAY_WIDTH {
                out[y * width + x] = picture[sy * DISPLAY_WIDTH + sx];
            }
        }
    }
    out
}

/// Working copies of System (drive A) and Samples (drive B) under `work`,
/// copied from `corpus` when absent or when `fresh` is set. The guest writes
/// to these copies, so drawings persist between runs.
fn working_disks(corpus: &Path, work: &Path, fresh: bool) -> Result<[PathBuf; 2], String> {
    fs::create_dir_all(work).map_err(|e| format!("create {}: {e}", work.display()))?;
    let copy = |name: &str| -> Result<PathBuf, String> {
        let source = corpus.join(name);
        let target = work.join(name);
        if !source.exists() {
            return Err(format!("missing floppy image {}", source.display()));
        }
        if fresh || !target.exists() {
            fs::copy(&source, &target).map_err(|e| format!("copy {}: {e}", source.display()))?;
        }
        Ok(target)
    };
    Ok([copy("System.img")?, copy("Samples.img")?])
}

struct App {
    session: Session,
    tracker: ModeTracker,
    held: HeldKeys,
    picture: Vec<u32>,
    state: Option<WindowState>,
    next_frame: Instant,
    outcome: Result<(), String>,
}

impl App {
    fn poll(&mut self) -> Result<(), String> {
        if let Some(status) = self.session.exit_status() {
            return Err(format!("QEMU exited: {status}"));
        }
        let memory = self.session.video_memory()?;
        self.picture = match self.tracker.update(&memory) {
            Mode::Graphics => Frame::new(&memory)?.to_rgb_640x400(),
            Mode::Text => self.session.screendump()?.to_640x400(),
        };
        Ok(())
    }

    fn fail(&mut self, el: &ActiveEventLoop, error: String) {
        self.outcome = Err(error);
        el.exit();
    }
}

impl ApplicationHandler for App {
    fn resumed(&mut self, el: &ActiveEventLoop) {
        if self.state.is_some() {
            return;
        }
        let attrs = Window::default_attributes()
            .with_title("AutoCAD 1.4 (QEMU)")
            .with_inner_size(LogicalSize::new(DISPLAY_WIDTH as f64, DISPLAY_HEIGHT as f64));
        let window = Rc::new(el.create_window(attrs).unwrap());
        let context = softbuffer::Context::new(window.clone()).unwrap();
        let surface = softbuffer::Surface::new(&context, window.clone()).unwrap();
        self.state = Some((window, surface));
    }

    fn about_to_wait(&mut self, el: &ActiveEventLoop) {
        if Instant::now() >= self.next_frame {
            if let Err(e) = self.poll() {
                return self.fail(el, e);
            }
            if let Some((window, _)) = &self.state {
                window.request_redraw();
            }
            self.next_frame = Instant::now() + FRAME_INTERVAL;
        }
        el.set_control_flow(ControlFlow::WaitUntil(self.next_frame));
    }

    fn window_event(&mut self, el: &ActiveEventLoop, _: WindowId, event: WindowEvent) {
        match event {
            WindowEvent::CloseRequested => {
                if let Err(e) = self.session.shutdown() {
                    eprintln!("acad-qemu: shutdown: {e}");
                }
                el.exit();
            }
            WindowEvent::Focused(false) => {
                for q in self.held.drain() {
                    if let Err(e) = self.session.key(q, false) {
                        return self.fail(el, e);
                    }
                }
            }
            WindowEvent::KeyboardInput { event, .. } => {
                let PhysicalKey::Code(code) = event.physical_key else {
                    return;
                };
                let down = event.state == ElementState::Pressed;
                match qcode(code) {
                    Some(q) => {
                        self.held.update(q, down);
                        if let Err(e) = self.session.key(q, down) {
                            self.fail(el, e);
                        }
                    }
                    None if down && !event.repeat => {
                        eprintln!("acad-qemu: unmapped key {code:?}");
                    }
                    None => {}
                }
            }
            WindowEvent::RedrawRequested => {
                let Some((window, surface)) = self.state.as_mut() else {
                    return;
                };
                let size = window.inner_size();
                let (Some(w), Some(h)) =
                    (NonZeroU32::new(size.width), NonZeroU32::new(size.height))
                else {
                    return;
                };
                surface.resize(w, h).unwrap();
                let image = blit(&self.picture, size.width as usize, size.height as usize);
                let mut buffer = surface.buffer_mut().unwrap();
                buffer.copy_from_slice(&image);
                buffer.present().unwrap();
            }
            _ => {}
        }
    }
}

fn fail(message: &str) -> ! {
    eprintln!("acad-qemu: {message}");
    std::process::exit(1);
}

fn main() {
    let mut fresh = false;
    for arg in std::env::args().skip(1) {
        match arg.as_str() {
            "--fresh" => fresh = true,
            other => fail(&format!("unknown argument {other:?}; usage: acad-qemu [--fresh]")),
        }
    }
    if !acad_oracle::available() {
        fail("qemu-system-i386 is not on PATH");
    }
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let corpus = root.join("corpus/raw/Autodesk AutoCAD 1.4 (5.25)");
    let [system, samples] = working_disks(&corpus, &root.join("target/acad-qemu"), fresh)
        .unwrap_or_else(|e| fail(&e));
    let session = Session::boot_in_place(&system, Some(&samples)).unwrap_or_else(|e| fail(&e));
    let mut app = App {
        session,
        tracker: ModeTracker::new(),
        held: HeldKeys::default(),
        picture: vec![0; DISPLAY_WIDTH * DISPLAY_HEIGHT],
        state: None,
        next_frame: Instant::now(),
        outcome: Ok(()),
    };
    let event_loop = EventLoop::new().unwrap_or_else(|e| fail(&e.to_string()));
    event_loop
        .run_app(&mut app)
        .unwrap_or_else(|e| fail(&e.to_string()));
    if let Err(e) = app.outcome {
        fail(&e);
    }
}
```

Keep the `#[cfg(test)] mod tests` from Step 1 at the bottom of the file. `cargo fmt` will reflow the key table; that is fine.

- [ ] **Step 4: Run tests and build**

Run: `cargo test -p acad-oracle --example acad-qemu`
Expected: 5 passed.

Run: `cargo build -p acad-oracle --all-targets 2>&1 | grep -E "^(warning|error)" ; echo done`
Expected: only `done`.

- [ ] **Step 5: Manual check**

Run: `cargo run -p acad-oracle --example acad-qemu -- --fresh`
Check, then report to the human partner with a screenshot (`screencapture -x <scratchpad>/acad-qemu-text.png`, and again in the editor):
1. The window shows the DOS boot and then AutoCAD's main menu as readable text.
2. Typing `1`, Enter, a name, Enter enters the editor. The status line, side menu, and `Command:` prompt appear as in `docs/oracle-qemu.md`'s decoded frame.
3. `LINE`, `1,1`, `11,8`, Enter draws a line. Shifted characters such as `@` reach the guest.
4. `END` returns to the text menu.
5. Closing the window ends QEMU (`pgrep -f qemu-system-i386` prints nothing).
6. Running again without `--fresh` shows the saved drawing in the main menu's drawing list.

- [ ] **Step 6: Document**

Insert before `## Remaining limits` in `docs/oracle-qemu.md`:

```markdown
## Interactive runner

`cargo run -p acad-oracle --example acad-qemu` boots the original with
System in drive A and Samples in drive B and shows it in a window. The disks
are working copies in `target/acad-qemu/`, so drawings survive between runs;
`--fresh` recopies them from the corpus. Keys are forwarded as QMP key
events, so Shift and Ctrl can be held; unmapped host keys are reported on
stderr.

QEMU's own display cannot show the editor: `DSIBMSS` programs the CGA mode
register at `3D8h`, which QEMU's VGA ignores. The runner therefore reads the
16 KiB at `B8000h` about 15 times a second and decides from its contents
whether the guest is in text or graphics mode (`acad_oracle::cga::detect`).
Text mode shows QEMU's screenshot; graphics mode decodes the CGA bitmap,
including the status line, screen menu, and prompt area. The mouse is not
connected yet; the configured `DGMS` driver expects a Mouse Systems serial
mouse.
```

- [ ] **Step 7: Commit**

```bash
cargo fmt -p acad-oracle
git add crates/acad-oracle/examples/acad-qemu.rs crates/acad-oracle/Cargo.toml Cargo.lock docs/oracle-qemu.md
git commit -m "feat(oracle): run AutoCAD interactively under QEMU with a CGA monitor"
```
