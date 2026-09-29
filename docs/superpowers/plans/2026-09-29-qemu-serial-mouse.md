# QEMU Serial Mouse Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Let the interactive QEMU runner drive AutoCAD 1.4's Mouse Systems serial mouse, so the crosshair follows the host cursor and host buttons pick, return, and toggle Snap.

**Architecture:** A pure module (`mouse.rs`) encodes Mouse Systems packets, mirrors the `DGMS` driver's position arithmetic, and maps CGA pixels to device units. `Session` attaches COM1 to a Unix socket and sends packets computed by that mirror. The runner converts window cursor positions to CGA pixels, and re-pins the pointer whenever the guest enters graphics mode.

**Tech Stack:** Rust 2021 (MSRV 1.88), QEMU 11 `-chardev socket` + `-serial chardev:`, winit 0.30 (dev-dependency, already present).

**Spec:** `docs/superpowers/specs/2026-09-29-qemu-cga-monitor-design.md` §4 (amended in `ca221b6`)

## Global Constraints

- `acad-oracle` is dev-only; no new runtime dependencies.
- AutoCAD binaries and corpus images are never modified.
- QEMU keeps `-machine isapc -m 16 -d nochain`; scratch sockets live under `/tmp` (paths < 104 bytes).
- Mouse deltas sent to the guest stay within `-120..=127`: bytes `0x80..=0x87` read as packet starts in `DGMS`.
- The driver's position is `0..=20480` per axis and always a multiple of 10.
- Measured screen mapping: `column = round(x × 639 / 20480)`, `row = 180 − floor(y × 191 / 20480)` (within one row).
- Buttons, active low on the wire: bit 2 left (pick), bit 1 middle (Return), bit 0 right (Snap toggle).
- Oracle tests skip when QEMU or images are absent. `cargo build -p acad-oracle --all-targets` has no warnings; `cargo fmt` applied.
- Another agent works on `main` concurrently: this plan runs in the `mouse` worktree and touches only `crates/acad-oracle/src/{mouse,lib,session}.rs`, `crates/acad-oracle/tests/cga_monitor.rs`, `crates/acad-oracle/examples/acad-qemu.rs`, and `docs/oracle-qemu.md`.
- Commit messages end with:
  ```
  Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>
  Claude-Session: https://claude.ai/code/session_01XtaKRW1Ehc4thUEp1gtT2W
  ```

## Review Focus

1. Moving the host cursor over the window at the text main menu, then entering the editor, must still put the crosshair under the cursor. The runner re-pins on every entry to graphics. Pinned in Task 1 (`pin_makes_the_mirror_exact_from_any_state`) and Task 3 (`entering_graphics_triggers_a_pin_only_on_the_transition`).
2. A cursor over the black border around the picture, or outside the window, must not move the guest pointer. Pinned in Task 3 (`picture_point_ignores_the_border`).
3. A jump across the whole screen in one event must arrive exactly, in a handful of packets. Pinned in Task 1 (`full_screen_jump_takes_few_packets`).
4. A button pressed and released between frames must reach the guest as two transitions. Pinned in Task 1 (`button_changes_each_send_a_packet`).
5. No delta may ever encode as `0x80..=0x87`. Pinned in Task 1 (`deltas_never_look_like_packet_starts`).

---

### Task 1: Mouse protocol, driver mirror and screen mapping

**Files:**
- Create: `crates/acad-oracle/src/mouse.rs`
- Modify: `crates/acad-oracle/src/lib.rs` (add `pub mod mouse;` between `pub mod in_tree;` and `pub mod mz_loader;`)

**Interfaces:**
- Produces:
  - `pub const RANGE: i32 = 20480; pub const LEFT: u8 = 4; pub const MIDDLE: u8 = 2; pub const RIGHT: u8 = 1;`
  - `pub fn packet(dx: i8, dy: i8, buttons: u8) -> [u8; 5]`
  - `pub fn apply(position: i32, delta: i8) -> i32`
  - `pub struct Tracker`; `Tracker::new() -> Tracker`; `Tracker::position(&self) -> (i32, i32)`; `Tracker::packets_to(&mut self, x: i32, y: i32, buttons: u8) -> Vec<[u8; 5]>`; `Tracker::pin(&mut self) -> Vec<[u8; 5]>`
  - `pub fn device_for_pixel(column: usize, row: usize) -> (i32, i32)` (CGA 640×200 pixel → device)
  - `pub fn pixel_for_device(x: i32, y: i32) -> (i32, i32)` (device → CGA column, row)

- [ ] **Step 1: Write the failing tests**

Create `crates/acad-oracle/src/mouse.rs` with only this test module:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    fn replay(start: (i32, i32), packets: &[[u8; 5]]) -> (i32, i32) {
        packets.iter().fold(start, |(x, y), p| {
            (apply(x, p[1] as i8), apply(y, p[2] as i8))
        })
    }

    #[test]
    fn packets_are_active_low_mouse_systems_frames() {
        assert_eq!(packet(1, -2, 0), [0x87, 0x01, 0xFE, 0, 0]);
        assert_eq!(packet(0, 0, LEFT), [0x83, 0, 0, 0, 0]);
        assert_eq!(packet(0, 0, LEFT | MIDDLE | RIGHT), [0x80, 0, 0, 0, 0]);
    }

    #[test]
    fn driver_arithmetic_accelerates_and_clamps() {
        assert_eq!(apply(0, 1), 10);
        assert_eq!(apply(100, -2), 80);
        assert_eq!(apply(0, 3), 180);
        assert_eq!(apply(1000, 100), 7000);
        assert_eq!(apply(50, -120), 0);
        assert_eq!(apply(20000, 127), RANGE);
    }

    #[test]
    fn tracker_reaches_every_grid_target_exactly() {
        let mut tracker = Tracker::new();
        for &(x, y) in &[(5000, 10000), (5010, 9990), (0, 0), (RANGE, RANGE), (1234, 17777)] {
            let before = tracker.position();
            let packets = tracker.packets_to(x, y, 0);
            let want = ((x + 5) / 10 * 10, (y + 5) / 10 * 10);
            assert_eq!(tracker.position(), want, "tracker for {x},{y}");
            assert_eq!(replay(before, &packets), want, "driver for {x},{y}");
        }
    }

    #[test]
    fn deltas_never_look_like_packet_starts() {
        let mut tracker = Tracker::new();
        for target in (0..=RANGE).step_by(370) {
            for p in tracker.packets_to(target, RANGE - target, 0) {
                for &b in &p[1..] {
                    assert!(!(0x80..=0x87).contains(&b), "delta byte {b:#04x}");
                }
            }
        }
    }

    #[test]
    fn full_screen_jump_takes_few_packets() {
        let mut tracker = Tracker::new();
        assert!(tracker.packets_to(RANGE, RANGE, 0).len() <= 6);
        assert!(tracker.packets_to(0, 0, 0).len() <= 6);
    }

    #[test]
    fn button_changes_each_send_a_packet() {
        let mut tracker = Tracker::new();
        tracker.packets_to(5000, 5000, 0);
        let press = tracker.packets_to(5000, 5000, LEFT);
        let release = tracker.packets_to(5000, 5000, 0);
        assert_eq!(press, [packet(0, 0, LEFT)]);
        assert_eq!(release, [packet(0, 0, 0)]);
        assert!(tracker.packets_to(5000, 5000, 0).is_empty());
    }

    #[test]
    fn pin_makes_the_mirror_exact_from_any_state() {
        for start in [(0, 0), (20480, 20480), (7310, 150)] {
            let mut tracker = Tracker::new();
            tracker.packets_to(start.0, start.1, 0);
            let packets = tracker.pin();
            assert_eq!(tracker.position(), (0, 0));
            // Whatever the guest believed, the clamp brings it to (0, 0) too.
            assert_eq!(replay((RANGE, RANGE), &packets), (0, 0));
        }
    }

    #[test]
    fn measured_screen_mapping_matches_the_crosshair_samples() {
        for (x, column) in [(0, 0), (30, 1), (1000, 31), (5000, 156), (10000, 312), (17000, 530)] {
            assert_eq!(pixel_for_device(x, 10000).0, column, "x {x}");
        }
        for (y, row) in [(5000, 134), (10000, 87), (15000, 41), (18500, 8)] {
            assert_eq!(pixel_for_device(9000, y).1, row, "y {y}");
        }
    }

    #[test]
    fn device_for_pixel_inverts_the_screen_mapping() {
        for column in 0..640 {
            for row in 8..=161 {
                let (x, y) = device_for_pixel(column, row);
                assert_eq!((x % 10, y % 10), (0, 0));
                assert_eq!(pixel_for_device(x, y), (column as i32, row as i32));
            }
        }
    }
}
```

Add `pub mod mouse;` to `crates/acad-oracle/src/lib.rs` between `pub mod in_tree;` and `pub mod mz_loader;`.

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p acad-oracle --lib mouse`
Expected: compile errors: `cannot find function packet`, `apply`, `pixel_for_device`, `device_for_pixel`; `cannot find type Tracker`; `LEFT`/`RANGE` not found.

- [ ] **Step 3: Implement**

Put above the tests in `mouse.rs`:

```rust
//! The original's Mouse Systems serial mouse, as `DGMS.DRV` reads it.
//!
//! A packet is a start byte `0x80 | buttons` (active low: bit 2 left, bit 1
//! middle, bit 0 right) and four signed deltas `dx1, dy1, dx2, dy2`, positive
//! y upward. The driver adds `d × 10` for `|d| ≤ 2` and `d × 60` otherwise to
//! an absolute position clamped to `0..=RANGE`. It treats any byte
//! `0x80..=0x87` as a packet start, so deltas stay within `-120..=127`.

pub const RANGE: i32 = 20480;
pub const LEFT: u8 = 4;
pub const MIDDLE: u8 = 2;
pub const RIGHT: u8 = 1;
const MIN_DELTA: i8 = -120;
const FINE: i32 = 10;
const COARSE: i32 = 60;

/// One packet moving by `(dx, dy)` with `buttons` (`LEFT | MIDDLE | RIGHT`)
/// held. The second delta pair is left at zero.
pub fn packet(dx: i8, dy: i8, buttons: u8) -> [u8; 5] {
    debug_assert!(dx >= MIN_DELTA && dy >= MIN_DELTA, "delta reads as a start byte");
    [0x80 | (!buttons & 7), dx as u8, dy as u8, 0, 0]
}

/// The driver's position on one axis after one delta.
pub fn apply(position: i32, delta: i8) -> i32 {
    let d = i32::from(delta);
    let scale = if d.abs() <= 2 { FINE } else { COARSE };
    (position + d * scale).clamp(0, RANGE)
}

/// The delta that moves `position` toward `target` as far as possible
/// without overshooting. Both are multiples of 10.
fn step(position: i32, target: i32) -> i8 {
    let remaining = target - position;
    if remaining == 0 {
        0
    } else if remaining.abs() >= 3 * COARSE {
        (remaining / COARSE).clamp(i32::from(MIN_DELTA), 127) as i8
    } else {
        (remaining / FINE).clamp(-2, 2) as i8
    }
}

/// The guest driver's position and buttons, mirrored exactly from the
/// packets sent. The driver starts at `(0, 0)` with no buttons held.
pub struct Tracker {
    x: i32,
    y: i32,
    buttons: u8,
}

impl Tracker {
    pub fn new() -> Self {
        Self {
            x: 0,
            y: 0,
            buttons: 0,
        }
    }

    pub fn position(&self) -> (i32, i32) {
        (self.x, self.y)
    }

    /// Packets taking the pointer to `(x, y)`, rounded to the driver's
    /// 10-unit grid and clamped to its range, with `buttons` held. A button
    /// change alone still sends one packet.
    pub fn packets_to(&mut self, x: i32, y: i32, buttons: u8) -> Vec<[u8; 5]> {
        let grid = |v: i32| (v.clamp(0, RANGE) + 5) / 10 * 10;
        let target = (grid(x), grid(y));
        let mut pending = buttons != self.buttons;
        self.buttons = buttons;
        let mut out = Vec::new();
        while (self.x, self.y) != target || pending {
            let (dx, dy) = (step(self.x, target.0), step(self.y, target.1));
            self.x = apply(self.x, dx);
            self.y = apply(self.y, dy);
            out.push(packet(dx, dy, buttons));
            pending = false;
        }
        out
    }

    /// Packets driving the pointer into the driver's lower-left clamp, which
    /// makes the mirror exact again whatever the guest believed.
    pub fn pin(&mut self) -> Vec<[u8; 5]> {
        self.x = 0;
        self.y = 0;
        vec![packet(MIN_DELTA, MIN_DELTA, self.buttons); 3]
    }
}

impl Default for Tracker {
    fn default() -> Self {
        Self::new()
    }
}

/// The CGA pixel under device position `(x, y)`, as measured from the
/// original's crosshair: columns exactly, rows within one.
pub fn pixel_for_device(x: i32, y: i32) -> (i32, i32) {
    ((x * 639 + RANGE / 2) / RANGE, 180 - y * 191 / RANGE)
}

/// A device position on the driver's grid whose crosshair lands on CGA
/// pixel `(column, row)`: the middle of that pixel's device interval.
pub fn device_for_pixel(column: usize, row: usize) -> (i32, i32) {
    let grid = |v: f64| ((v / 10.0).round() as i32 * 10).clamp(0, RANGE);
    let x = column as f64 * f64::from(RANGE) / 639.0;
    let y = (180.0 - row as f64 + 0.5) * f64::from(RANGE) / 191.0;
    (grid(x), grid(y))
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p acad-oracle --lib mouse`
Expected: 9 passed.

If `device_for_pixel_inverts_the_screen_mapping` fails for some rows, do not widen the range: print the failing `(column, row, x, y)` and correct `device_for_pixel`'s rounding. The row formula is floor-based, so the middle of an interval is exact.

- [ ] **Step 5: Commit**

```bash
cargo fmt -p acad-oracle
git add crates/acad-oracle/src/mouse.rs crates/acad-oracle/src/lib.rs
git commit -m "feat(oracle): model the original's Mouse Systems serial mouse"
```

---

### Task 2: COM1 mouse in `Session`

**Files:**
- Modify: `crates/acad-oracle/src/session.rs` (`launch`: chardev + serial arguments and socket connection; `Session` fields; new methods)
- Test: `crates/acad-oracle/tests/cga_monitor.rs`

**Interfaces:**
- Consumes: `crate::mouse::{Tracker, LEFT}` (Task 1)
- Produces (on `Session`):
  - `pub fn mouse_to(&mut self, x: i32, y: i32, buttons: u8) -> Result<(), String>`
  - `pub fn mouse_pin(&mut self) -> Result<(), String>`
  - `pub fn pointer(&self) -> (i32, i32)`

- [ ] **Step 1: Write the failing oracle test**

Append to `crates/acad-oracle/tests/cga_monitor.rs`:

```rust
/// The crosshair's column and row in a CGA frame, ignoring the fixed screen
/// dividers at rows 168–170 and columns 568–571.
fn crosshair(memory: &[u8]) -> (Option<usize>, Option<usize>) {
    let frame = Frame::new(memory).unwrap();
    let row = (0..200)
        .filter(|y| !(168..=170).contains(y))
        .find(|&y| (0..560).filter(|&x| frame.lit(x, y)).count() > 400);
    let column = (0..640)
        .filter(|x| !(568..=571).contains(x))
        .find(|&x| (10..165).filter(|&y| frame.lit(x, y)).count() > 120);
    (column, row)
}

#[test]
fn serial_mouse_points_at_the_requested_pixel_and_picks() {
    use acad_oracle::mouse::{device_for_pixel, LEFT};
    let system = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../corpus/raw/Autodesk AutoCAD 1.4 (5.25)/System.img");
    if !system.exists() || !acad_oracle::available() {
        eprintln!("skipping oracle: extracted System.img or qemu-system-i386 absent");
        return;
    }
    let mut vm = Session::boot_disposable(&system, None, &[]).unwrap();
    vm.wait_for_text("Enter selection:", WAIT).unwrap();
    vm.type_line("1").unwrap();
    vm.wait_for_text("Enter NAME of drawing:", WAIT).unwrap();
    vm.type_line("MOUSEPT").unwrap();
    vm.wait_until_text_gone("Enter NAME of drawing:", WAIT).unwrap();
    thread::sleep(Duration::from_millis(500));
    // The crosshair is drawn while AutoCAD waits for a point.
    vm.type_line("LINE").unwrap();
    vm.mouse_pin().unwrap();
    for (column, row) in [(156, 134), (312, 87), (468, 41), (40, 150)] {
        let (x, y) = device_for_pixel(column, row);
        vm.mouse_to(x, y, 0).unwrap();
        assert_eq!(vm.pointer(), (x, y));
        let (seen_column, seen_row) = crosshair(&vm.capture_editor().unwrap());
        assert_eq!(seen_column, Some(column), "column for {column},{row}");
        let seen_row = seen_row.expect("crosshair row");
        assert!(seen_row.abs_diff(row) <= 1, "row {seen_row} for {column},{row}");
    }

    // Pick two points with the left button; the line joins them.
    for (column, row) in [(100, 150), (400, 50)] {
        let (x, y) = device_for_pixel(column, row);
        vm.mouse_to(x, y, 0).unwrap();
        vm.mouse_to(x, y, LEFT).unwrap();
        thread::sleep(Duration::from_millis(300));
        vm.mouse_to(x, y, 0).unwrap();
        thread::sleep(Duration::from_millis(300));
    }
    vm.type_line("").unwrap();
    // Park the pointer on the screen menu so the crosshair is not drawn.
    let (x, y) = device_for_pixel(600, 100);
    vm.mouse_to(x, y, 0).unwrap();
    let memory = vm.capture_editor().unwrap();
    let frame = Frame::new(&memory).unwrap();
    let near = |cx: usize, cy: usize| {
        (cy - 2..=cy + 2).any(|y| (cx - 2..=cx + 2).any(|x| frame.lit(x, y)))
    };
    assert!(near(250, 100), "picked line missing at its midpoint");
    assert!(near(175, 125) && near(325, 75), "picked line missing at its quarters");
    vm.shutdown().unwrap();
}
```

- [ ] **Step 2: Run the test to verify it fails**

Run: `cargo test -p acad-oracle --test cga_monitor serial_mouse`
Expected: compile errors: `no method named mouse_pin`, `mouse_to`, `pointer` on `Session`.

- [ ] **Step 3: Implement**

In `crates/acad-oracle/src/session.rs`:

(a) Add two fields to `pub struct Session`, after `writer: UnixStream,`:

```rust
    mouse: UnixStream,
    pointer: crate::mouse::Tracker,
```

(b) In `launch`, directly after `let socket = dir.join("qmp.sock");` add:

```rust
        let mouse_socket = dir.join("mouse.sock");
```

and after `command.args(["-d", "nochain"]);` add:

```rust
        // ACAD.CFG puts the Mouse Systems mouse (DGMS) on COM1 (3F8h, IRQ4).
        command
            .arg("-chardev")
            .arg(format!(
                "socket,id=mouse,path={},server=on,wait=off",
                mouse_socket.display()
            ))
            .args(["-serial", "chardev:mouse"]);
```

(c) After the `let writer = match stream.try_clone() { … };` block add:

```rust
        let mouse = match UnixStream::connect(&mouse_socket) {
            Ok(mouse) => mouse,
            Err(e) => {
                let _ = child.kill();
                let _ = child.wait();
                let _ = fs::remove_dir_all(&dir);
                return Err(format!("connect mouse socket: {e}"));
            }
        };
```

and add `mouse,` and `pointer: crate::mouse::Tracker::new(),` to the `let mut session = Self { … }` literal after `writer,`.

(d) After `pub fn key(…)` add:

```rust
    /// Move the COM1 mouse to device position `(x, y)` (`0..=20480`, see
    /// `crate::mouse`) with `buttons` held. The pointer's position is
    /// mirrored from the packets sent, not read back.
    pub fn mouse_to(&mut self, x: i32, y: i32, buttons: u8) -> Result<(), String> {
        let packets = self.pointer.packets_to(x, y, buttons);
        self.send_mouse(&packets)
    }

    /// Drive the pointer into its lower-left clamp so the mirrored position
    /// is exact again, e.g. after AutoCAD reinitializes its mouse driver.
    pub fn mouse_pin(&mut self) -> Result<(), String> {
        let packets = self.pointer.pin();
        self.send_mouse(&packets)
    }

    /// The mirrored device position of the mouse.
    pub fn pointer(&self) -> (i32, i32) {
        self.pointer.position()
    }

    fn send_mouse(&mut self, packets: &[[u8; 5]]) -> Result<(), String> {
        self.mouse
            .write_all(&packets.concat())
            .map_err(|e| format!("write mouse packets: {e}"))
    }
```

- [ ] **Step 4: Run the new test and the whole suite**

Run: `cargo test -p acad-oracle --test cga_monitor -- --nocapture`
Expected: 3 passed (the two existing tests and `serial_mouse_points_at_the_requested_pixel_and_picks`).

If a crosshair column is off by one, print `vm.pointer()` and the seen column, and recheck `pixel_for_device` against Task 1's samples before changing any constant. The spec's mapping is measured, so a mismatch is a finding: record it in the ledger.

Run: `cargo test -p acad-oracle`
Expected: all pass. The added COM1 device must not change any existing oracle result.

- [ ] **Step 5: Commit**

```bash
cargo fmt -p acad-oracle
git add crates/acad-oracle/src/session.rs crates/acad-oracle/tests/cga_monitor.rs
git commit -m "feat(oracle): attach the Mouse Systems mouse to QEMU's COM1"
```

---

### Task 3: Mouse in the interactive runner

**Files:**
- Modify: `crates/acad-oracle/examples/acad-qemu.rs` (layout helper shared with `blit`, `picture_point`, `entered_graphics`, `App` fields, `poll`, window events, tests)
- Modify: `docs/oracle-qemu.md` (Interactive runner section)

**Interfaces:**
- Consumes: `Session::{mouse_to, mouse_pin}` (Task 2); `acad_oracle::mouse::{device_for_pixel, LEFT, MIDDLE, RIGHT}` (Task 1); `acad_oracle::cga::Mode`

- [ ] **Step 1: Write the failing tests**

Add to the example's `mod tests`:

```rust
    #[test]
    fn picture_point_inverts_the_blit_layout() {
        // 1300×800 draws the picture at scale 2 from x = 10.
        assert_eq!(picture_point(1300, 800, 10.0, 0.0), Some((0, 0)));
        assert_eq!(picture_point(1300, 800, 13.9, 3.0), Some((1, 1)));
        assert_eq!(picture_point(1300, 800, 1289.0, 799.0), Some((639, 399)));
        assert_eq!(picture_point(640, 400, 320.5, 200.5), Some((320, 200)));
    }

    #[test]
    fn picture_point_ignores_the_border() {
        assert_eq!(picture_point(1300, 800, 9.0, 100.0), None);
        assert_eq!(picture_point(1300, 800, 1290.0, 100.0), None);
        assert_eq!(picture_point(1300, 800, -5.0, 100.0), None);
        assert_eq!(picture_point(640, 400, 100.0, 400.0), None);
    }

    #[test]
    fn entering_graphics_triggers_a_pin_only_on_the_transition() {
        assert!(entered_graphics(Mode::Text, Mode::Graphics));
        assert!(!entered_graphics(Mode::Graphics, Mode::Graphics));
        assert!(!entered_graphics(Mode::Graphics, Mode::Text));
        assert!(!entered_graphics(Mode::Text, Mode::Text));
    }
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p acad-oracle --example acad-qemu`
Expected: compile errors: `cannot find function picture_point`, `entered_graphics`.

- [ ] **Step 3: Implement**

(a) Replace the first three lines of `blit`'s body:

```rust
    let scale = (width / DISPLAY_WIDTH).min(height / DISPLAY_HEIGHT).max(1);
    let left = width.saturating_sub(DISPLAY_WIDTH * scale) / 2;
    let top = height.saturating_sub(DISPLAY_HEIGHT * scale) / 2;
```

with `let (scale, left, top) = layout(width, height);`, and add above `blit`:

```rust
/// Where `blit` draws the picture in a `width`×`height` window: its integer
/// scale and the left and top offsets of its first pixel.
fn layout(width: usize, height: usize) -> (usize, usize, usize) {
    let scale = (width / DISPLAY_WIDTH).min(height / DISPLAY_HEIGHT).max(1);
    let left = width.saturating_sub(DISPLAY_WIDTH * scale) / 2;
    let top = height.saturating_sub(DISPLAY_HEIGHT * scale) / 2;
    (scale, left, top)
}

/// The 640×400 picture pixel under window position `(x, y)`, or `None` over
/// the border or outside the window.
fn picture_point(width: usize, height: usize, x: f64, y: f64) -> Option<(usize, usize)> {
    let (scale, left, top) = layout(width, height);
    if x < left as f64 || y < top as f64 {
        return None;
    }
    let px = (x as usize - left) / scale;
    let py = (y as usize - top) / scale;
    (px < DISPLAY_WIDTH && py < DISPLAY_HEIGHT).then_some((px, py))
}

/// AutoCAD (re)initializes its mouse driver when it enters the editor, so
/// the pointer is re-pinned on every switch into graphics mode.
fn entered_graphics(previous: Mode, now: Mode) -> bool {
    previous == Mode::Text && now == Mode::Graphics
}
```

(b) Imports: add `use acad_oracle::mouse::{device_for_pixel, LEFT, MIDDLE, RIGHT};` and change `use winit::event::{ElementState, WindowEvent};` to `use winit::event::{ElementState, MouseButton, WindowEvent};`.

(c) Add three fields to `struct App` after `held: HeldKeys,`:

```rust
    mode: Mode,
    pointer: Option<(i32, i32)>,
    buttons: u8,
```

and initialize them in `main`'s `App { … }` after `held: HeldKeys::default(),`:

```rust
        mode: Mode::Text,
        pointer: None,
        buttons: 0,
```

(d) In `App::poll`, replace `self.picture = match self.tracker.update(&memory) {` with:

```rust
        let mode = self.tracker.update(&memory);
        if entered_graphics(self.mode, mode) {
            self.session.mouse_pin()?;
            if let Some((x, y)) = self.pointer {
                self.session.mouse_to(x, y, self.buttons)?;
            }
        }
        self.mode = mode;
        self.picture = match mode {
```

(e) Add to `impl App`:

```rust
    /// Send the host pointer to the guest mouse; only the editor has one.
    fn send_pointer(&mut self) -> Result<(), String> {
        match (self.mode, self.pointer) {
            (Mode::Graphics, Some((x, y))) => self.session.mouse_to(x, y, self.buttons),
            _ => Ok(()),
        }
    }
```

(f) In `window_event`, extend the `WindowEvent::Focused(false)` arm so that it also releases the mouse buttons, after the key loop:

```rust
                self.buttons = 0;
                if let Err(e) = self.send_pointer() {
                    return self.fail(el, e);
                }
```

and add two arms before `WindowEvent::RedrawRequested`:

```rust
            WindowEvent::CursorMoved { position, .. } => {
                let Some((window, _)) = &self.state else {
                    return;
                };
                let size = window.inner_size();
                let point = picture_point(
                    size.width as usize,
                    size.height as usize,
                    position.x,
                    position.y,
                );
                if let Some((px, py)) = point {
                    self.pointer = Some(device_for_pixel(px, py / 2));
                    if let Err(e) = self.send_pointer() {
                        self.fail(el, e);
                    }
                }
            }
            WindowEvent::MouseInput { state, button, .. } => {
                let bit = match button {
                    MouseButton::Left => LEFT,
                    MouseButton::Middle => MIDDLE,
                    MouseButton::Right => RIGHT,
                    _ => return,
                };
                if state == ElementState::Pressed {
                    self.buttons |= bit;
                } else {
                    self.buttons &= !bit;
                }
                if let Err(e) = self.send_pointer() {
                    self.fail(el, e);
                }
            }
```

- [ ] **Step 4: Run tests and build**

Run: `cargo test -p acad-oracle --example acad-qemu`
Expected: 12 passed.

Run: `cargo build -p acad-oracle --all-targets 2>&1 | grep -E "^(warning|error)" ; echo done`
Expected: only `done`.

- [ ] **Step 5: Manual check**

Run: `cargo run -p acad-oracle --example acad-qemu` (from the worktree; `--fresh` is not needed).

With the screen unlocked, check and report with screenshots:
1. At the main menu, moving the host cursor over the window changes nothing.
2. After `1`, a name and Enter, then `LINE`, the crosshair follows the host cursor within a pixel.
3. Left-click twice draws a line between the two clicked points. Middle-click or Enter ends `LINE`.
4. Right-click at a point prompt shows `<Snap on>`, and again `<Snap off>`.
5. Hovering the screen menu and left-clicking, e.g. `CIRCLE`, starts that command.
6. After `END` and re-entering the editor, the crosshair is still under the cursor, so the pin worked.

- [ ] **Step 6: Document**

In `docs/oracle-qemu.md`'s Interactive runner section, replace:

```markdown
The text cursor is not drawn. The mouse is not connected yet;
the configured `DGMS` driver expects a Mouse Systems serial mouse.
```

with:

```markdown
The text cursor is not drawn.

The configured `DGMS` driver reads a Mouse Systems serial mouse on COM1,
which QEMU attaches to a socket in the session directory. The runner mirrors
the driver's position arithmetic (deltas of ±2 count 10 units, larger ones
60, clamped to `0..=20480`) and moves the pointer to the device position
under the host cursor, using the crosshair mapping measured in the spec
(`column = round(x × 639 / 20480)`, `row = 180 − floor(y × 191 / 20480)`).
It re-pins the pointer at the lower-left clamp whenever the editor starts.
Left picks, middle is Return, and right toggles Snap, as `ACAD.MNU` assigns.
```

- [ ] **Step 7: Commit**

```bash
cargo fmt -p acad-oracle
git add crates/acad-oracle/examples/acad-qemu.rs docs/oracle-qemu.md
git commit -m "feat(oracle): drive the original's mouse from the runner window"
```
