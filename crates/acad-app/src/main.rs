//! Native window adapter. Commands and frames are owned by the shared session.
#[cfg(unix)]
use acad_app::api;
use acad_app::{KeyModifiers, ReportAction, Session};
use std::{num::NonZeroU32, rc::Rc};
use winit::{
    application::ApplicationHandler,
    event::{ElementState, MouseScrollDelta, WindowEvent},
    event_loop::{ActiveEventLoop, ControlFlow, EventLoop},
    keyboard::{Key, ModifiersState, NamedKey},
    window::{Window, WindowId},
};
#[cfg(unix)]
type ApiEvent = acad_app::ipc::Event;
#[cfg(not(unix))]
type ApiEvent = ();
type WindowState = (Rc<Window>, softbuffer::Surface<Rc<Window>, Rc<Window>>);
struct WindowApp {
    session: Session,
    window: Option<WindowState>,
    reported: std::collections::BTreeSet<String>,
    modifiers: ModifiersState,
}
impl WindowApp {
    fn refresh(&self, window: &Window) {
        let (w, h) = self.session.minimum_size();
        window.set_min_inner_size(Some(winit::dpi::PhysicalSize::new(w, h)));
        let current = window.inner_size();
        if current.width < w || current.height < h {
            let _ = window.request_inner_size(winit::dpi::PhysicalSize::new(
                current.width.max(w),
                current.height.max(h),
            ));
        }
        window.set_title(&self.session.title());
        window.request_redraw();
    }
    fn finish(&mut self, el: &ActiveEventLoop, result: Result<bool, String>) {
        if result == Ok(true) {
            el.exit();
        }
        if let Some((window, _)) = &self.window {
            self.refresh(window);
        }
    }
}
impl ApplicationHandler<ApiEvent> for WindowApp {
    /// Command scripts advance here, between events, never by sleeping: DELAY
    /// becomes a WaitUntil deadline and due items a bounded batch per pass.
    fn about_to_wait(&mut self, el: &ActiveEventLoop) {
        let pump = self.session.pump_script();
        if pump.quit {
            el.exit();
            return;
        }
        if pump.progressed {
            if let Some((window, _)) = &self.window {
                self.refresh(window);
            }
        }
        el.set_control_flow(match self.session.script_wake_in() {
            Some(wait) if wait.is_zero() => ControlFlow::Poll,
            Some(wait) => {
                #[cfg(not(target_arch = "wasm32"))]
                {
                    ControlFlow::WaitUntil(std::time::Instant::now() + wait)
                }
                #[cfg(target_arch = "wasm32")]
                {
                    let _ = wait;
                    ControlFlow::Poll
                }
            }
            None => ControlFlow::Wait,
        });
    }
    fn resumed(&mut self, el: &ActiveEventLoop) {
        if self.window.is_some() {
            return;
        }
        let window = Rc::new(
            el.create_window(Window::default_attributes().with_title(self.session.title()))
                .unwrap(),
        );
        self.refresh(&window);
        let context = softbuffer::Context::new(window.clone()).unwrap();
        let surface = softbuffer::Surface::new(&context, window.clone()).unwrap();
        self.window = Some((window, surface));
    }
    #[cfg(unix)]
    fn user_event(&mut self, el: &ActiveEventLoop, event: ApiEvent) {
        let size = self
            .window
            .as_ref()
            .map(|(w, _)| {
                let s = w.inner_size();
                (s.width, s.height)
            })
            .unwrap_or((800, 600));
        let result = api::dispatch(&mut self.session, event.request, size);
        let quit = result
            .as_ref()
            .is_ok_and(|v| v.get("quit") == Some(&serde_json::Value::Bool(true)));
        if let Err(error) = &result {
            eprintln!("API: {error}");
        }
        let _ = event.reply.send(result);
        if quit {
            el.exit();
        }
        if let Some((window, _)) = &self.window {
            self.refresh(window);
        }
    }
    fn window_event(&mut self, el: &ActiveEventLoop, id: WindowId, event: WindowEvent) {
        let Some(window) = self.window.as_ref().map(|(w, _)| w.clone()) else {
            return;
        };
        if window.id() != id {
            return;
        }
        let size = window.inner_size();
        if size.width > 0 && size.height > 0 {
            let _ = self.session.set_viewport_size(size.width, size.height);
        }
        match event {
            WindowEvent::ModifiersChanged(modifiers) => self.modifiers = modifiers.state(),
            WindowEvent::CloseRequested => {
                let result = self.session.request_quit();
                self.finish(el, result);
            }
            WindowEvent::KeyboardInput { event, .. } if event.state == ElementState::Pressed => {
                let result = match event.logical_key {
                    Key::Named(NamedKey::Enter) => {
                        if self.session.report_visible() && self.session.input().is_empty() {
                            let size = window.inner_size();
                            self.session
                                .report_action(ReportAction::Close, size.width, size.height)
                                .map(|_| false)
                        } else {
                            let input = self.session.input().to_owned();
                            self.session.command(&input)
                        }
                    }
                    Key::Named(key) if self.session.report_visible() => {
                        let action = match key {
                            NamedKey::Escape => Some(ReportAction::Close),
                            NamedKey::ArrowUp => Some(ReportAction::Up),
                            NamedKey::ArrowDown => Some(ReportAction::Down),
                            NamedKey::PageUp => Some(ReportAction::PageUp),
                            NamedKey::PageDown => Some(ReportAction::PageDown),
                            NamedKey::Home => Some(ReportAction::Home),
                            NamedKey::End => Some(ReportAction::End),
                            _ => None,
                        };
                        let size = window.inner_size();
                        action
                            .map(|a| {
                                self.session
                                    .report_action(a, size.width, size.height)
                                    .map(|_| false)
                            })
                            .unwrap_or(Ok(false))
                    }
                    Key::Named(NamedKey::Escape) => self.session.cancel(),
                    Key::Named(NamedKey::Backspace) => {
                        let mut input = self.session.input().to_owned();
                        input.pop();
                        self.session.set_input(input);
                        Ok(false)
                    }
                    Key::Character(text) => self.session.key_character(
                        &text,
                        KeyModifiers {
                            control: self.modifiers.control_key(),
                            alt: self.modifiers.alt_key(),
                            logo: self.modifiers.super_key(),
                        },
                    ),
                    _ => Ok(false),
                };
                self.finish(el, result);
            }
            WindowEvent::CursorMoved { position, .. } => {
                self.session.cursor(Some((position.x, position.y)));
                if self.session.sketch_active() {
                    // SKETCH samples motion through the shared session seam.
                    let size = window.inner_size();
                    let result = self.session.pointer_motion(size.width, size.height);
                    self.finish(el, result);
                } else {
                    window.request_redraw();
                }
            }
            WindowEvent::MouseWheel { delta, .. } if self.session.report_visible() => {
                let amount = match delta {
                    MouseScrollDelta::LineDelta(_, y) => f64::from(y),
                    MouseScrollDelta::PixelDelta(p) => p.y,
                };
                if amount.is_finite() && amount != 0.0 {
                    let action = if amount > 0.0 {
                        ReportAction::Up
                    } else {
                        ReportAction::Down
                    };
                    let size = window.inner_size();
                    let result = self
                        .session
                        .report_action(action, size.width, size.height)
                        .map(|_| false);
                    self.finish(el, result);
                }
            }
            WindowEvent::DroppedFile(path) => {
                let result = self.session.open_dropped(&path);
                self.finish(el, result);
            }
            WindowEvent::CursorLeft { .. } => {
                self.session.cursor(None);
                window.request_redraw();
            }
            WindowEvent::MouseInput {
                state,
                button: winit::event::MouseButton::Left,
                ..
            } => {
                // Press and release use the cursor position stored for frame
                // preview; outside a SKETCH drag the press is the click.
                let size = window.inner_size();
                let result = match state {
                    ElementState::Pressed => self.session.press_cursor(size.width, size.height),
                    ElementState::Released => self.session.release_cursor(size.width, size.height),
                };
                self.finish(el, result);
            }
            WindowEvent::Resized(_) | WindowEvent::ScaleFactorChanged { .. } => {
                self.refresh(&window)
            }
            WindowEvent::RedrawRequested => {
                let size = window.inner_size();
                let (Some(w), Some(h)) =
                    (NonZeroU32::new(size.width), NonZeroU32::new(size.height))
                else {
                    return;
                };
                let frame = match self.session.frame(size.width, size.height) {
                    Ok(f) => f,
                    Err(e) => {
                        eprintln!("frame: {e}");
                        return;
                    }
                };
                for diagnostic in frame.diagnostics {
                    if self.reported.insert(diagnostic.clone()) {
                        eprintln!("render: {diagnostic}");
                    }
                }
                let Some((_, surface)) = self.window.as_mut() else {
                    return;
                };
                surface.resize(w, h).unwrap();
                let mut buffer = surface.buffer_mut().unwrap();
                buffer.copy_from_slice(&frame.pixels);
                buffer.present().unwrap();
            }
            _ => {}
        }
    }
}
fn run() -> Result<(), String> {
    let mut args = std::env::args().skip(1);
    let mut path = None;
    let mut directories = Vec::new();
    let mut socket = None;
    let mut script = None;
    let mut palette = acad_model::Palette::default();
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--palette" => {
                let name = args.next().ok_or("--palette requires pc16 or aci256")?;
                palette = acad_model::Palette::from_name(&name)
                    .ok_or_else(|| format!("unknown palette: {name} (pc16 or aci256)"))?;
            }
            "--script" => script = Some(args.next().ok_or("--script requires a file name")?),
            "--api-socket" => {
                socket = Some(std::path::PathBuf::from(
                    args.next().ok_or("--api-socket requires a path")?,
                ))
            }
            "--help" => {
                println!(
                    "acad [drawing [font-directories ...]] [--api-socket PATH] [--script FILE]\n\
                     \x20    [--palette pc16|aci256]\n\
                     Without a drawing or script, starts at the Main Menu."
                );
                return Ok(());
            }
            _ if arg.starts_with("--") => return Err(format!("unknown option: {arg}")),
            _ if path.is_none() => path = Some(arg),
            _ => directories.push(std::path::PathBuf::from(arg)),
        }
    }
    // Native policy (docs/native-main-menu.md): without a drawing or a
    // script the window starts at the Main Menu, and END/QUIT return there.
    // `--script` alone keeps the historical sample drawing.
    let mut session = match (&path, &script) {
        (None, None) => {
            println!("Main Menu");
            Session::main_menu(&directories)
        }
        _ => {
            let path = path.unwrap_or_else(|| "corpus/Samples/SUBDIV.DXF".into());
            let session = Session::open(std::path::Path::new(&path), &directories)?;
            println!(
                "{path}: {} entities, {} blocks",
                session.drawing().entities().count(),
                session.drawing().blocks().count()
            );
            session
        }
    };
    session.set_palette(palette);
    if let Some(script) = script {
        // Like the original's "Can't open script file", a bad startup script
        // stops the program; it then runs from the event loop.
        session.start_script(&script)?;
    }
    let el = EventLoop::<ApiEvent>::with_user_event()
        .build()
        .map_err(|e| e.to_string())?;
    #[cfg(unix)]
    let _server = socket
        .map(|path| {
            let proxy = el.create_proxy();
            acad_app::ipc::Server::bind(&path, move |event| {
                proxy.send_event(event).map_err(|e| e.to_string())
            })
        })
        .transpose()?;
    #[cfg(not(unix))]
    if socket.is_some() {
        return Err("GUI API sockets require Unix".into());
    }
    let mut app = WindowApp {
        session,
        window: None,
        reported: Default::default(),
        modifiers: ModifiersState::default(),
    };
    el.run_app(&mut app).map_err(|e| e.to_string())
}
fn main() {
    if let Err(error) = run() {
        eprintln!("acad: {error}");
        std::process::exit(2);
    }
}
