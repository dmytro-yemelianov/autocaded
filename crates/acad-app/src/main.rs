use acad_render::{flatten, rasterize, Viewport};
use std::{num::NonZeroU32, rc::Rc};
use winit::{
    application::ApplicationHandler,
    event::WindowEvent,
    event_loop::{ActiveEventLoop, EventLoop},
    window::{Window, WindowId},
};

/// A window plus the softbuffer surface drawing into it. They are created
/// together on resume and torn down together, so they travel as one.
type WindowState = (Rc<Window>, softbuffer::Surface<Rc<Window>, Rc<Window>>);

struct App {
    drawing: acad_model::Drawing,
    state: Option<WindowState>,
}

impl ApplicationHandler for App {
    fn resumed(&mut self, el: &ActiveEventLoop) {
        let attrs = Window::default_attributes().with_title("AutoCAD 1.4");
        let window = Rc::new(el.create_window(attrs).unwrap());
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
            WindowEvent::RedrawRequested => {
                let size = window.inner_size();
                let (Some(w), Some(h)) =
                    (NonZeroU32::new(size.width), NonZeroU32::new(size.height))
                else {
                    return;
                };
                surface.resize(w, h).unwrap();
                let vp = Viewport::fit(&self.drawing.header.limits, size.width, size.height);
                let pm = rasterize(&flatten(&self.drawing, &vp), size.width, size.height);
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
        // AC1.40 is a real DWG magic, just one this codec can't read yet
        // (milestone ④); acad_dwg::parse reports that itself rather than
        // this function duplicating the check.
        Ok(_) => acad_dwg::parse(bytes).unwrap_or_else(|e| fail(path, e)),
        Err(_) => acad_dxf::parse(bytes).unwrap_or_else(|e| fail(path, e)),
    }
}

fn main() {
    let path = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "corpus/Samples/SUBDIV.DXF".to_string());
    let bytes = std::fs::read(&path).unwrap_or_else(|e| fail(&path, e));
    let drawing = parse(&path, &bytes);
    let undrawn = drawing
        .entities()
        .filter(|e| matches!(e, acad_model::Entity::Text { .. }))
        .count();
    println!(
        "{}: {} entities, {} blocks",
        path,
        drawing.entities().count(),
        drawing.blocks().count()
    );
    if undrawn > 0 {
        println!("  note: {undrawn} TEXT entities are not drawn (.SHP fonts, milestone 5)");
    }
    let el = EventLoop::new().unwrap();
    el.run_app(&mut App {
        drawing,
        state: None,
    })
    .unwrap();
}
