# QEMU CGA monitor and interactive runner

## Goal

Run the original AutoCAD 1.4 interactively under QEMU and see its CGA
screen correctly: status line, drawing area, screen menu, and prompt area.
The tool is for studying the original by hand; it is not part of the shipped
`acad` application and lives in the dev-only `acad-oracle` crate.

Constraints carried over from the oracle harness: AutoCAD's binaries are not
modified, corpus images are never written, and QEMU keeps `-d nochain`
(see [oracle-qemu.md](../../oracle-qemu.md)).

## Why QEMU's own display is wrong

The configured display driver, `DSIBMSS` ("IBM Color/Graphics, single
screen"), never calls INT 10h. It writes the CGA CRTC through `3D4h/3D5h`,
waits for vertical retrace on `3DAh` bit 3, then writes the mode register
`3D8h` and color select `3D9h`. QEMU's `isapc` adapter (`cirrus-vga`) claims
`3B0h–3DFh` but ignores `3D8h/3D9h`, so it stays in 80×25 text mode and
shows the bitmap as characters.

Video memory is still correct. In text mode the VGA's odd/even write and read
paths agree, so a 16 KiB read of `B8000h` is the linear CGA image. A decoded
dump taken in the editor shows the complete screen, including the side menu
and prompt area, because AutoCAD draws those as pixels with its own font.

Reprogramming the VGA into mode 6 from the host was tried and rejected: it
cannot switch at the exact `3D8h` write, and the VGA's text and graphics
memory layouts differ. The single-screen flip copies a saved bitmap back
into `B800h` while still in text mode, so the layouts must agree.

## Components

### 1. CGA decoder (`src/cga.rs`)

- `Frame`: a 640×200 one-bit bitmap decoded from a 16 KiB dump. Even
  scanlines start at offset 0, odd scanlines at `0x2000`, 80 bytes per
  scanline, high bit = leftmost pixel. Accessor `lit(x, y)`.
- `detect(dump) -> Option<Mode>` where `Mode` is `Text` or `Graphics`.
  Text when at least 95% of the attribute bytes (odd offsets below 4000)
  share one nonzero value; graphics otherwise. `None` is reserved for a
  dump shorter than 4000 bytes.
- `ModeTracker`: requires two consecutive agreeing detections before
  changing mode, so a partially redrawn frame does not flicker.
- Tests that inspect CGA pixels (`font_render.rs`, and the helpers in
  other oracle tests) use `Frame` instead of private `lit()` copies.

### 2. QEMU session (`src/session.rs`, extracted from `qemu.rs`)

The private `Vm` moves to its own module as `Session`, keeping current
behavior for the tests: disposable floppy copies, QMP over a Unix socket,
the serialization lock, `wait_for_text`, `type_line`, and `capture_editor`.

New capabilities used by the runner:

- Boot from caller-provided floppy paths without copying them (the runner
  manages its own working copies).
- Optional headless display is unchanged (`-display none`); the runner
  renders frames itself.
- `key(qcode, down)` via QMP `input-send-event`, so modifiers can be held
  and host key repeat produces repeated make codes like a real keyboard.
- `video_memory() -> Vec<u8>`: 16 KiB `pmemsave` of `B8000h`.
- `screendump() -> Image`: QMP `screendump` in PPM format, decoded to RGB.
- `alive()` and `shutdown()`.

The public oracle functions (`generate_*`, `open_drawing`) keep their
signatures.

### 3. Interactive runner (`examples/acad-qemu.rs`)

`cargo run -p acad-oracle --example acad-qemu [-- --fresh]`

- Working disks: on first run, `System.img` and `Samples.img` are copied from
  `corpus/raw/Autodesk AutoCAD 1.4 (5.25)/` into `target/acad-qemu/`.
  Later runs reuse them, so drawings persist. `--fresh` recopies.
  System is drive A, Samples drive B.
- Window: winit + softbuffer (dev-dependencies, same versions as
  `acad-app`), 640×400 logical pixels, integer-scaled.
- Frame loop at about 15 Hz:
  - read video memory, update `ModeTracker`;
  - graphics: draw the decoded frame, each scanline doubled;
  - text: render the 80×25 page from video memory with the BIOS 8×8 font
    (`F000:FA6E`, upper half via INT 1Fh), each scanline doubled.
    *Amended during implementation:* QEMU's screendump was the original
    choice, but after `END` AutoCAD's CGA CRTC table leaves QEMU's VGA
    drawing 8-line cells, so its text picture is garbled. The screendump
    API remains in `Session` for tests.
- Keyboard: winit `KeyCode` → QEMU qcode through an explicit table (letters,
  digits, punctuation, Enter, Backspace, Tab, Escape, arrows, Home/End,
  PgUp/PgDn, Ins/Del, F1–F10, Shift, Ctrl, Alt). Key-up is forwarded;
  repeats are forwarded as further key-down events. Unmapped keys are
  logged to stderr and dropped.
- Exit: closing the window shuts QEMU down; if QEMU exits first, the
  runner reports its exit status and closes the window.

### 4. Mouse

*Amended after investigation (disassembly of `DGMS.DRV` and QEMU trials).*

**Driver facts.** The configuration selects `DGMS`, a Mouse Systems serial
mouse, on the port stored in `ACAD.CFG`: `3F8h` (COM1, IRQ4, vector `0Ch`).
`2F8h` in the driver is only the COM2 alternative. The driver installs an
IRQ handler, unmasks the IRQ, writes `08h` (OUT2) to the modem control
register and `01h` (receive interrupt) to the interrupt enable register. It
never programs the baud rate or line format, so under QEMU any byte on the
serial chardev reaches it.

Its parser treats any byte with `(b & 0xF8) == 0x80` as a packet start, even
mid-packet. The low three bits are buttons, active low: bit 2 left, bit 1
middle, bit 0 right. The next four bytes are signed deltas `dx1, dy1, dx2,
dy2`, positive y upward. Each delta `d` moves an absolute position by
`d × 10` when `|d| ≤ 2` and `d × 60` otherwise, clamped to `0..=20480` per
axis. The position starts at `(0, 0)`. Deltas must stay out of `-128..=-121`,
whose bytes would read as packet starts; the host uses `-120..=127`.

**Screen mapping** (measured from the crosshair over 43 positions):
`column = round(x × 639 / 20480)` exactly, and
`row = 180 − floor(y × 191 / 20480)` within one row. The crosshair is drawn
only in the drawing area, while AutoCAD waits for a point; positions right of
column 568 point at the screen menu, and rows below 168 at the prompt area.

**Buttons** follow `ACAD.MNU`: left picks, middle is Return, right toggles
Snap (`^B`).

**Design.**

- QEMU attaches COM1 to a Unix socket in the session directory
  (`-chardev socket,…,server=on,wait=off -serial chardev:mouse`).
- `Session::mouse_to(x, y, buttons)` walks there with packets whose deltas
  are chosen by mirroring the driver's arithmetic, so the host always knows
  the guest's position. `Session::mouse_pin()` drives the pointer into the
  lower-left clamp, making the mirror exact again after AutoCAD
  reinitializes its driver.
- The runner maps the host cursor's position in the 640×400 picture to device
  units with the inverse of the screen mapping, and sends `mouse_to` whenever
  the cursor moves or a button changes. The crosshair follows the host cursor
  without grabbing it. Left, middle and right host buttons are the mouse's
  own three buttons.

## Error handling

- Missing QEMU or corpus images: the runner exits with a message naming
  what is missing. Oracle tests keep skipping as today.
- QMP errors during the frame loop end the session with the error shown.
- Undecidable mode keeps the last confident mode.

## Testing

- Unit: scanline interleave and bit order; mode detection on a synthetic
  text screen (attribute `0x07`), a blank bitmap, and a busy bitmap;
  `ModeTracker` hysteresis; keycode table has no duplicate qcodes and
  covers the listed keys; PPM decoding and the 720→640 column drop.
- Oracle (skips without QEMU/images): at `Enter selection:` the detector
  reports text; in the editor after a `LINE`, it reports graphics, and the
  decoded side-menu and prompt regions contain lit pixels.
- Runner: manual run, with screenshots of the main menu (text) and the
  editor (graphics).

## Out of scope

- Patching QEMU or adding a CGA device model.
- Color: the configured drawing is monochrome; `3D9h` is not observable.
- The in-tree 8086 core's port I/O.
