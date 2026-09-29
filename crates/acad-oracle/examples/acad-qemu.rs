//! Interactive AutoCAD 1.4 under QEMU, shown through a host-side CGA monitor.
//! QEMU's adapter ignores the CGA mode register, so this window decodes video
//! memory itself, as a CGA would scan it out, in both text and graphics mode.
//!
//! `cargo run -p acad-oracle --example acad-qemu [-- --fresh]`

use acad_oracle::cga::{
    text_rgb_640x400, Font, Frame, Mode, ModeTracker, DISPLAY_HEIGHT, DISPLAY_WIDTH,
};
use acad_oracle::mouse::{device_for_pixel, LEFT, MIDDLE, RIGHT};
use acad_oracle::session::Session;
use std::fs;
use std::num::NonZeroU32;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::time::{Duration, Instant};
use winit::application::ApplicationHandler;
use winit::dpi::LogicalSize;
use winit::event::{ElementState, MouseButton, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::keyboard::{KeyCode, PhysicalKey};
use winit::window::{Window, WindowId};

const FRAME_INTERVAL: Duration = Duration::from_millis(66);

type WindowState = (Rc<Window>, softbuffer::Surface<Rc<Window>, Rc<Window>>);

/// Host key → QEMU qcode. Keys absent here are reported and dropped.
fn qcode(key: KeyCode) -> Option<&'static str> {
    use KeyCode::*;
    Some(match key {
        KeyA => "a",
        KeyB => "b",
        KeyC => "c",
        KeyD => "d",
        KeyE => "e",
        KeyF => "f",
        KeyG => "g",
        KeyH => "h",
        KeyI => "i",
        KeyJ => "j",
        KeyK => "k",
        KeyL => "l",
        KeyM => "m",
        KeyN => "n",
        KeyO => "o",
        KeyP => "p",
        KeyQ => "q",
        KeyR => "r",
        KeyS => "s",
        KeyT => "t",
        KeyU => "u",
        KeyV => "v",
        KeyW => "w",
        KeyX => "x",
        KeyY => "y",
        KeyZ => "z",
        Digit0 => "0",
        Digit1 => "1",
        Digit2 => "2",
        Digit3 => "3",
        Digit4 => "4",
        Digit5 => "5",
        Digit6 => "6",
        Digit7 => "7",
        Digit8 => "8",
        Digit9 => "9",
        Minus => "minus",
        Equal => "equal",
        BracketLeft => "bracket_left",
        BracketRight => "bracket_right",
        Backslash => "backslash",
        Semicolon => "semicolon",
        Quote => "apostrophe",
        Backquote => "grave_accent",
        Comma => "comma",
        Period => "dot",
        Slash => "slash",
        Space => "spc",
        Enter => "ret",
        Backspace => "backspace",
        Tab => "tab",
        Escape => "esc",
        ArrowUp => "up",
        ArrowDown => "down",
        ArrowLeft => "left",
        ArrowRight => "right",
        Home => "home",
        End => "end",
        PageUp => "pgup",
        PageDown => "pgdn",
        Insert => "insert",
        Delete => "delete",
        F1 => "f1",
        F2 => "f2",
        F3 => "f3",
        F4 => "f4",
        F5 => "f5",
        F6 => "f6",
        F7 => "f7",
        F8 => "f8",
        F9 => "f9",
        F10 => "f10",
        ShiftLeft => "shift",
        ShiftRight => "shift_r",
        ControlLeft => "ctrl",
        ControlRight => "ctrl_r",
        AltLeft => "alt",
        AltRight => "alt_r",
        CapsLock => "caps_lock",
        // Host numpads mean digits; the guest's NumLock state is unknown and
        // the arrow keys already provide cursor movement.
        Numpad0 => "0",
        Numpad1 => "1",
        Numpad2 => "2",
        Numpad3 => "3",
        Numpad4 => "4",
        Numpad5 => "5",
        Numpad6 => "6",
        Numpad7 => "7",
        Numpad8 => "8",
        Numpad9 => "9",
        NumpadDecimal => "dot",
        NumpadEnter => "kp_enter",
        NumpadAdd => "kp_add",
        NumpadSubtract => "kp_subtract",
        NumpadMultiply => "kp_multiply",
        NumpadDivide => "kp_divide",
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

    /// Bring guest Shift, Ctrl and Alt in line with the host's modifier
    /// flags, returning the key events to send. Synthetic keystrokes, and
    /// modifiers changed while the window was unfocused, arrive only as
    /// flags, never as key events of their own.
    fn sync_modifiers(&mut self, [shift, ctrl, alt]: [bool; 3]) -> Vec<(&'static str, bool)> {
        let mut events = Vec::new();
        for (on, left, right) in [
            (shift, "shift", "shift_r"),
            (ctrl, "ctrl", "ctrl_r"),
            (alt, "alt", "alt_r"),
        ] {
            let held = [left, right].map(|q| self.0.contains(q));
            if on && !held.contains(&true) {
                self.0.insert(left);
                events.push((left, true));
            } else if !on {
                for (q, down) in [left, right].into_iter().zip(held) {
                    if down {
                        self.0.remove(q);
                        events.push((q, false));
                    }
                }
            }
        }
        events
    }
}

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

/// Integer-scale the 640×400 picture into a `width`×`height` buffer,
/// centred, with black around it; windows smaller than the picture clip it.
/// Each source row is scaled once and then copied row by row.
fn blit(picture: &[u32], out: &mut [u32], width: usize, height: usize) {
    let (scale, left, top) = layout(width, height);
    let visible = width.min(left + DISPLAY_WIDTH * scale);
    let blank = vec![0; width];
    let mut row = vec![0; width];
    let mut built = None;
    for (y, target) in out.chunks_exact_mut(width).take(height).enumerate() {
        let sy = y.checked_sub(top).map(|v| v / scale);
        let Some(sy) = sy.filter(|&sy| sy < DISPLAY_HEIGHT) else {
            target.copy_from_slice(&blank);
            continue;
        };
        if built != Some(sy) {
            let source = &picture[sy * DISPLAY_WIDTH..][..DISPLAY_WIDTH];
            let mut x = left;
            while x < visible {
                row[x] = source[(x - left) / scale];
                x += 1;
            }
            built = Some(sy);
        }
        target.copy_from_slice(&row);
    }
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
    font: Option<Font>,
    held: HeldKeys,
    mode: Mode,
    pointer: Option<(i32, i32)>,
    buttons: u8,
    picture: Vec<u32>,
    state: Option<WindowState>,
    next_frame: Instant,
    outcome: Result<(), String>,
    closing: bool,
}

impl App {
    fn poll(&mut self) -> Result<(), String> {
        if let Some(status) = self.session.exit_status() {
            return Err(format!("QEMU exited: {status}"));
        }
        let memory = self.session.video_memory()?;
        let mode = self.tracker.update(&memory);
        if entered_graphics(self.mode, mode) {
            self.session.mouse_pin()?;
            if let Some((x, y)) = self.pointer {
                self.session.mouse_to(x, y, self.buttons)?;
            }
        }
        self.mode = mode;
        self.picture = match mode {
            Mode::Graphics => Frame::new(&memory)?.to_rgb_640x400(),
            Mode::Text => {
                if self.font.is_none() {
                    self.font = Some(self.session.bios_font()?);
                }
                text_rgb_640x400(&memory, self.font.as_ref().expect("loaded"))?
            }
        };
        Ok(())
    }

    /// Send the host pointer to the guest mouse; only the editor has one.
    fn send_pointer(&mut self) -> Result<(), String> {
        match (self.mode, self.pointer) {
            (Mode::Graphics, Some((x, y))) => self.session.mouse_to(x, y, self.buttons),
            _ => Ok(()),
        }
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
            .with_inner_size(LogicalSize::new(
                DISPLAY_WIDTH as f64,
                DISPLAY_HEIGHT as f64,
            ));
        let window = Rc::new(el.create_window(attrs).unwrap());
        let context = softbuffer::Context::new(window.clone()).unwrap();
        let surface = softbuffer::Surface::new(&context, window.clone()).unwrap();
        self.state = Some((window, surface));
    }

    fn about_to_wait(&mut self, el: &ActiveEventLoop) {
        if self.closing {
            return;
        }
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

    /// Also reached by the macOS app menu's Quit (Cmd+Q), where AppKit ends
    /// the process without returning from the event loop.
    fn exiting(&mut self, _: &ActiveEventLoop) {
        if !std::mem::replace(&mut self.closing, true) {
            if let Err(e) = self.session.shutdown() {
                eprintln!("acad-qemu: shutdown: {e}");
            }
        }
    }

    fn window_event(&mut self, el: &ActiveEventLoop, _: WindowId, event: WindowEvent) {
        match event {
            WindowEvent::CloseRequested => {
                self.closing = true;
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
                self.buttons = 0;
                if let Err(e) = self.send_pointer() {
                    return self.fail(el, e);
                }
            }
            WindowEvent::ModifiersChanged(modifiers) => {
                let state = modifiers.state();
                let flags = [state.shift_key(), state.control_key(), state.alt_key()];
                for (q, down) in self.held.sync_modifiers(flags) {
                    if let Err(e) = self.session.key(q, down) {
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
                let mut buffer = surface.buffer_mut().unwrap();
                blit(
                    &self.picture,
                    &mut buffer,
                    size.width as usize,
                    size.height as usize,
                );
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
            other => fail(&format!(
                "unknown argument {other:?}; usage: acad-qemu [--fresh]"
            )),
        }
    }
    if !acad_oracle::available() {
        fail("qemu-system-i386 is not on PATH");
    }
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let corpus = root.join("corpus/raw/Autodesk AutoCAD 1.4 (5.25)");
    let [system, samples] =
        working_disks(&corpus, &root.join("target/acad-qemu"), fresh).unwrap_or_else(|e| fail(&e));
    let session = Session::boot_in_place(&system, Some(&samples)).unwrap_or_else(|e| fail(&e));
    let event_loop = match EventLoop::new() {
        Ok(event_loop) => event_loop,
        Err(e) => {
            drop(session);
            fail(&e.to_string());
        }
    };
    let mut app = App {
        session,
        tracker: ModeTracker::new(),
        font: None,
        held: HeldKeys::default(),
        mode: Mode::Text,
        pointer: None,
        buttons: 0,
        picture: vec![0; DISPLAY_WIDTH * DISPLAY_HEIGHT],
        state: None,
        next_frame: Instant::now(),
        outcome: Ok(()),
        closing: false,
    };
    let run = event_loop.run_app(&mut app).map_err(|e| e.to_string());
    let outcome = run.and(std::mem::replace(&mut app.outcome, Ok(())));
    // Dropping the session stops QEMU; `fail` exits without running
    // destructors and would leave it running on the working disks.
    drop(app);
    if let Err(e) = outcome {
        fail(&e);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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

    const REQUIRED: &[KeyCode] = &[
        KeyCode::KeyA,
        KeyCode::KeyZ,
        KeyCode::Digit0,
        KeyCode::Digit9,
        KeyCode::Minus,
        KeyCode::Equal,
        KeyCode::BracketLeft,
        KeyCode::BracketRight,
        KeyCode::Backslash,
        KeyCode::Semicolon,
        KeyCode::Quote,
        KeyCode::Backquote,
        KeyCode::Comma,
        KeyCode::Period,
        KeyCode::Slash,
        KeyCode::Space,
        KeyCode::Enter,
        KeyCode::Backspace,
        KeyCode::Tab,
        KeyCode::Escape,
        KeyCode::ArrowUp,
        KeyCode::ArrowDown,
        KeyCode::ArrowLeft,
        KeyCode::ArrowRight,
        KeyCode::Home,
        KeyCode::End,
        KeyCode::PageUp,
        KeyCode::PageDown,
        KeyCode::Insert,
        KeyCode::Delete,
        KeyCode::F1,
        KeyCode::F10,
        KeyCode::ShiftLeft,
        KeyCode::ShiftRight,
        KeyCode::ControlLeft,
        KeyCode::ControlRight,
        KeyCode::AltLeft,
        KeyCode::AltRight,
        KeyCode::CapsLock,
        KeyCode::NumpadEnter,
        KeyCode::NumpadAdd,
        KeyCode::NumpadSubtract,
        KeyCode::NumpadMultiply,
        KeyCode::NumpadDivide,
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

    #[test]
    fn numpad_digits_type_digits_whatever_the_guest_numlock() {
        assert_eq!(qcode(KeyCode::Numpad0), Some("0"));
        assert_eq!(qcode(KeyCode::Numpad7), Some("7"));
        assert_eq!(qcode(KeyCode::NumpadDecimal), Some("dot"));
        assert_eq!(qcode(KeyCode::NumLock), None);
    }

    fn picture() -> Vec<u32> {
        (1..=(DISPLAY_WIDTH * DISPLAY_HEIGHT) as u32).collect()
    }

    fn blitted(w: usize, h: usize) -> Vec<u32> {
        let mut out = vec![0xDEAD; w * h];
        blit(&picture(), &mut out, w, h);
        out
    }

    #[test]
    fn blit_scales_by_whole_pixels() {
        let out = blitted(1280, 800);
        assert_eq!((out[0], out[1], out[2]), (1, 1, 2));
        assert_eq!(out[1280], 1);
        assert_eq!(out[2 * 1280], 641);
    }

    #[test]
    fn blit_centres_leftover_space() {
        let out = blitted(1300, 800);
        assert_eq!((out[9], out[10], out[11], out[12]), (0, 1, 1, 2));
    }

    #[test]
    fn blit_clips_small_and_odd_windows() {
        for (w, h) in [(300, 200), (1, 1), (641, 399)] {
            let out = blitted(w, h);
            assert_eq!(out.len(), w * h);
            assert_eq!(out[0], 1);
            if w > 1 {
                assert_eq!(out[1], 2);
            }
        }
    }

    #[test]
    fn fullscreen_blit_fits_well_inside_a_frame() {
        // 14-inch MacBook Pro fullscreen, physical pixels. The frame
        // interval is 66 ms and the event loop also forwards keys.
        let (w, h) = (3024, 1964);
        let picture = picture();
        let mut out = vec![0; w * h];
        blit(&picture, &mut out, w, h); // fault in the buffer, as a reused one is
        let start = Instant::now();
        blit(&picture, &mut out, w, h);
        let elapsed = start.elapsed();
        assert!(elapsed < Duration::from_millis(25), "blit took {elapsed:?}");
        assert_eq!(out[(h / 2) * w + w / 2], picture[200 * DISPLAY_WIDTH + 320]);
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
    fn modifiers_follow_host_flags_without_key_events() {
        let mut held = HeldKeys::default();
        assert_eq!(held.sync_modifiers([true, false, false]), [("shift", true)]);
        assert!(held.sync_modifiers([true, false, false]).is_empty());
        assert_eq!(
            held.sync_modifiers([false, false, false]),
            [("shift", false)]
        );
        held.update("ctrl_r", true);
        assert!(held.sync_modifiers([false, true, false]).is_empty());
        assert_eq!(
            held.sync_modifiers([false, false, true]),
            [("ctrl_r", false), ("alt", true)]
        );
        assert_eq!(held.drain(), ["alt"]);
    }

    #[test]
    fn working_disks_copy_once_and_refresh_on_demand() {
        let base = std::env::temp_dir().join(format!("acad-qemu-test-{}", std::process::id()));
        let corpus = base.join("corpus");
        let work = base.join("work");
        std::fs::create_dir_all(&corpus).unwrap();
        std::fs::write(corpus.join("System.img"), b"system").unwrap();
        assert!(
            working_disks(&corpus, &work, false).is_err(),
            "Samples missing"
        );
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
