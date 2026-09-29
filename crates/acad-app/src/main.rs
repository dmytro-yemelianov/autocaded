use acad_render::{flatten_with_libraries, rasterize, Libraries, Viewport};
use std::{num::NonZeroU32, rc::Rc};
use winit::keyboard::{Key, NamedKey};
use winit::{
    application::ApplicationHandler,
    event::{ElementState, WindowEvent},
    event_loop::{ActiveEventLoop, EventLoop},
    window::{Window, WindowId},
};

/// A window plus the softbuffer surface drawing into it. They are created
/// together on resume and torn down together, so they travel as one.
type WindowState = (Rc<Window>, softbuffer::Surface<Rc<Window>, Rc<Window>>);

struct App {
    editor: acad_cmd::Editor,
    libraries: Libraries,
    reported: std::collections::BTreeSet<String>,
    state: Option<WindowState>,
    input: String,
    status: String,
}

impl ApplicationHandler for App {
    fn resumed(&mut self, el: &ActiveEventLoop) {
        let attrs = Window::default_attributes().with_title("AutoCAD 1.4");
        let window = Rc::new(el.create_window(attrs).unwrap());
        window.set_title("AutoCAD 1.4 — Command");
        let context = softbuffer::Context::new(window.clone()).unwrap();
        let surface = softbuffer::Surface::new(&context, window.clone()).unwrap();
        self.state = Some((window, surface));
    }

    fn window_event(&mut self, el: &ActiveEventLoop, _: WindowId, event: WindowEvent) {
        let Some((window, surface)) = self.state.as_mut() else {
            return;
        };
        match event {
            WindowEvent::CloseRequested => el.exit(),
            WindowEvent::KeyboardInput { event, .. } if event.state == ElementState::Pressed => {
                match &event.logical_key {
                    Key::Named(NamedKey::Enter) => {
                        let input = std::mem::take(&mut self.input);
                        self.status.clear();
                        match self.editor.submit(&input) {
                            Ok(acad_cmd::Effect::Continue) => {
                                self.status = self.editor.status().to_owned();
                            }
                            Ok(acad_cmd::Effect::Quit) => el.exit(),
                            Ok(acad_cmd::Effect::Save(path)) => {
                                self.status = match save_drawing(&path, self.editor.drawing()) {
                                    Ok(()) => format!("Saved {path}"),
                                    Err(e) => format!("Save failed: {e}"),
                                };
                            }
                            Err(e) => self.status = e,
                        }
                    }
                    Key::Named(NamedKey::Backspace) => {
                        self.input.pop();
                    }
                    Key::Named(NamedKey::Escape) => {
                        self.input.clear();
                    }
                    Key::Character(text) if !text.chars().any(char::is_control) => {
                        self.input.push_str(text);
                    }
                    _ => {}
                }
                let title = format!(
                    "AutoCAD 1.4 — {} {} {}",
                    self.editor.prompt(),
                    self.input,
                    self.status
                );
                window.set_title(&title);
                window.request_redraw();
            }
            WindowEvent::RedrawRequested => {
                let size = window.inner_size();
                let (Some(w), Some(h)) =
                    (NonZeroU32::new(size.width), NonZeroU32::new(size.height))
                else {
                    return;
                };
                surface.resize(w, h).unwrap();
                let drawing = self.editor.drawing();
                let view = drawing.header.view;
                let vp = if view.height.is_finite() && view.height > 0.0 {
                    Viewport::from_view(view.center, view.height, size.width, size.height)
                } else {
                    Viewport::fit(&drawing.header.limits, size.width, size.height)
                };
                let rendered = flatten_with_libraries(drawing, &vp, &self.libraries);
                for diagnostic in rendered.diagnostics {
                    if self.reported.insert(diagnostic.clone()) {
                        eprintln!("render: {diagnostic}");
                    }
                }
                let pm = rasterize(&rendered.primitives, size.width, size.height);
                let mut buffer = surface.buffer_mut().unwrap();
                for (dst, src) in buffer.iter_mut().zip(pm.pixels()) {
                    *dst = ((src.red() as u32) << 16)
                        | ((src.green() as u32) << 8)
                        | src.blue() as u32;
                }
                buffer.present().unwrap();
            }
            _ => {}
        }
    }
}

fn save_drawing(path: &str, drawing: &acad_model::Drawing) -> Result<(), String> {
    let path_ref = std::path::Path::new(path);
    let bytes = if path_ref
        .extension()
        .is_some_and(|ext| ext.eq_ignore_ascii_case("dxf"))
    {
        acad_dxf::write(drawing)
    } else {
        acad_dwg::write(drawing).map_err(|e| e.to_string())?
    };
    std::fs::write(path_ref, bytes).map_err(|e| e.to_string())
}

/// Report a diagnostic and exit. A bad file is a normal outcome for a tool
/// that reads 43-year-old floppies; dressing it as a crash report makes a
/// correct message look like a bug in the program.
fn fail(path: &str, e: impl std::fmt::Display) -> ! {
    eprintln!("acad: {path}: {e}");
    std::process::exit(2);
}

/// Chooses the codec by the file's own magic bytes, not its extension: a
/// `.BAK` file (the corpus has several) is a DWG whatever its name says, and
/// an AC1.2 or AC1.40 magic is unambiguous evidence either way (spec §4.2's
/// `Version::detect`). Only when neither magic matches — i.e. this is not a
/// DWG at all — does the file get handed to the DXF parser, which reports
/// its own error if it isn't a DXF either.
fn parse(path: &str, bytes: &[u8]) -> acad_model::Drawing {
    match acad_dwg::header::Version::detect(bytes) {
        Ok(_) => acad_dwg::parse(bytes).unwrap_or_else(|e| fail(path, e)),
        Err(_) => acad_dxf::parse(bytes).unwrap_or_else(|e| fail(path, e)),
    }
}

fn main() {
    let mut args = std::env::args().skip(1);
    let path = args
        .next()
        .unwrap_or_else(|| "corpus/Samples/SUBDIV.DXF".to_string());
    let bytes = std::fs::read(&path).unwrap_or_else(|e| fail(&path, e));
    let drawing = parse(&path, &bytes);
    let directories = args.map(std::path::PathBuf::from).collect::<Vec<_>>();
    let (libraries, diagnostics) =
        Libraries::for_drawing(std::path::Path::new(&path), &directories);
    for diagnostic in diagnostics {
        eprintln!("library: {diagnostic}");
    }
    println!(
        "{}: {} entities, {} blocks",
        path,
        drawing.entities().count(),
        drawing.blocks().count()
    );
    let el = EventLoop::new().unwrap();
    el.run_app(&mut App {
        editor: acad_cmd::Editor::new(drawing),
        libraries,
        reported: Default::default(),
        state: None,
        input: String::new(),
        status: String::new(),
    })
    .unwrap();
}
