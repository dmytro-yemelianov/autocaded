use acad_render::{flatten, rasterize, Viewport};
use std::{num::NonZeroU32, rc::Rc};
use winit::{application::ApplicationHandler, event::WindowEvent,
    event_loop::{ActiveEventLoop, EventLoop}, window::{Window, WindowId}};

struct App {
    drawing: acad_model::Drawing,
    state: Option<(Rc<Window>, softbuffer::Surface<Rc<Window>, Rc<Window>>)>,
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
        let Some((window, surface)) = self.state.as_mut() else { return };
        match event {
            WindowEvent::CloseRequested => el.exit(),
            WindowEvent::RedrawRequested => {
                let size = window.inner_size();
                let (Some(w), Some(h)) = (NonZeroU32::new(size.width), NonZeroU32::new(size.height))
                    else { return };
                surface.resize(w, h).unwrap();
                let vp = Viewport::fit(&self.drawing.header.limits, size.width, size.height);
                let pm = rasterize(&flatten(&self.drawing, &vp), size.width, size.height);
                let mut buffer = surface.buffer_mut().unwrap();
                for (dst, src) in buffer.iter_mut().zip(pm.pixels()) {
                    *dst = ((src.red() as u32) << 16) | ((src.green() as u32) << 8) | src.blue() as u32;
                }
                buffer.present().unwrap();
            }
            _ => {}
        }
    }
}

fn main() {
    let path = std::env::args().nth(1)
        .unwrap_or_else(|| "corpus/Samples/SUBDIV.DXF".to_string());
    let bytes = std::fs::read(&path).unwrap_or_else(|e| panic!("{path}: {e}"));
    let drawing = acad_dxf::parse(&bytes).unwrap_or_else(|e| panic!("{path}: {e}"));
    println!("{}: {} entities, {} blocks", path,
        drawing.entities().count(), drawing.blocks().count());
    let el = EventLoop::new().unwrap();
    el.run_app(&mut App { drawing, state: None }).unwrap();
}
