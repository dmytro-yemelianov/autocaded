use acad_render::{flatten_with_libraries, rasterize, Libraries, Prim, Viewport};
use std::{num::NonZeroU32, rc::Rc};
use winit::keyboard::{Key, NamedKey};
use winit::{
    application::ApplicationHandler,
    event::{ElementState, WindowEvent},
    event_loop::{ActiveEventLoop, EventLoop},
    window::{Window, WindowId},
};
mod files;
mod menu_panel;

/// A window plus the softbuffer surface drawing into it. They are created
/// together on resume and torn down together, so they travel as one.
type WindowState = (Rc<Window>, softbuffer::Surface<Rc<Window>, Rc<Window>>);

struct App {
    editor: acad_cmd::Editor,
    libraries: Libraries,
    menu: Option<acad_cmd::menu::MenuFile>,
    reported: std::collections::BTreeSet<String>,
    state: Option<WindowState>,
    input: String,
    status: String,
    cursor: Option<(f64, f64)>,
}

impl App {
    fn submit_line(&mut self, el: &ActiveEventLoop, input: &str) {
        self.status.clear();
        let command_input = if self.editor.awaiting_shape_library_name() {
            match self.resolve_shape_library(input) {
                Ok(name) => name,
                Err(error) => {
                    self.status = error;
                    return;
                }
            }
        } else {
            input.to_owned()
        };
        let result = self.editor.submit(&command_input);
        self.handle_result(el, result);
    }

    fn resolve_shape_library(&mut self, requested: &str) -> Result<String, String> {
        if requested.trim().is_empty() {
            return Err("shape library name cannot be empty".into());
        }
        if let Some(path) = shape_library_path(requested) {
            let bytes = std::fs::read(&path)
                .map_err(|error| format!("cannot read {}: {error}", path.display()))?;
            let library = acad_render::shp::Library::parse(&bytes)
                .map_err(|error| format!("{}: {error}", path.display()))?;
            let name = path
                .file_stem()
                .and_then(|name| name.to_str())
                .filter(|name| !name.is_empty())
                .ok_or_else(|| format!("invalid SHP filename: {}", path.display()))?
                .to_owned();
            if let Some(existing) = self.libraries.get(&name) {
                if existing != &library {
                    return Err(format!(
                        "another SHP library named {name} is already available; rename one file"
                    ));
                }
            } else {
                self.libraries
                    .insert(&name, &bytes)
                    .map_err(|error| format!("{}: {error}", path.display()))?;
            }
            self.editor.register_shape_library(
                &name,
                library
                    .named_shapes()
                    .map(|(shape, number)| (shape.to_owned(), number)),
            );
            return Ok(name);
        }
        if let Some(library) = self.libraries.get(requested) {
            self.editor.register_shape_library(
                requested,
                library
                    .named_shapes()
                    .map(|(shape, number)| (shape.to_owned(), number)),
            );
            return Ok(requested.to_owned());
        }
        Err(format!("shape library is not available: {requested}"))
    }

    fn submit_mouse_point(&mut self, el: &ActiveEventLoop, width: u32, height: u32) {
        if !self.editor.accepts_mouse_point() {
            return;
        }
        let Some((x, y)) = self.cursor else {
            return;
        };
        let vp = viewport_for(self.editor.drawing(), width, height);
        let world = vp.to_world(acad_model::Point { x, y });
        self.status.clear();
        let result = self.editor.submit_mouse_point(world);
        self.handle_result(el, result);
    }

    fn pick_mouse_entity(&mut self, width: u32, height: u32) {
        if !self.editor.accepts_mouse_selection() {
            return;
        }
        let Some((x, y)) = self.cursor else {
            return;
        };
        let vp = viewport_for(self.editor.drawing(), width, height);
        let world = vp.to_world(acad_model::Point { x, y });
        let horizontal_neighbor = vp.to_world(acad_model::Point { x: x + 6.0, y });
        let tolerance = (horizontal_neighbor.x - world.x).hypot(horizontal_neighbor.y - world.y);
        let Some(id) = self.editor.pick_entity_at(world, tolerance) else {
            self.status = "No entity at pick point".into();
            return;
        };
        let already_selected = self
            .input
            .split(',')
            .filter_map(|part| part.trim().parse::<usize>().ok())
            .any(|selected| selected == id);
        if !already_selected {
            if !self.input.is_empty() {
                self.input.push(',');
            }
            self.input.push_str(&id.to_string());
        }
        self.status = format!("Selected entity {id}");
    }

    fn handle_result(&mut self, el: &ActiveEventLoop, result: Result<acad_cmd::Effect, String>) {
        match result {
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
            Ok(acad_cmd::Effect::SaveDrawing(path, drawing)) => {
                self.status = match save_dwg(&path, &drawing) {
                    Ok(()) => format!("Wrote block drawing {path}"),
                    Err(e) => format!("WBLOCK failed: {e}"),
                };
            }
            Ok(acad_cmd::Effect::LoadMenu(requested)) => match load_menu_file(&requested) {
                Ok((path, menu)) => {
                    let headers = menu
                        .entries
                        .iter()
                        .filter(|entry| entry.kind == acad_cmd::menu::MenuEntryKind::Header)
                        .count();
                    let items = menu.entries.len() - headers;
                    self.status = format!("Loaded {} menu entries from {}", items, path.display());
                    self.menu = Some(menu);
                }
                Err(error) => self.status = error,
            },
            Ok(acad_cmd::Effect::Files(request)) => match files::execute(request) {
                Ok(report) => {
                    println!("{report}");
                    self.status = report.lines().next().unwrap_or("FILES complete").to_owned();
                }
                Err(error) => self.status = error,
            },
            Ok(acad_cmd::Effect::Report(report)) => {
                println!("{report}");
                self.status = "Report printed to terminal".into();
            }
            Err(e) => self.status = e,
        }
    }

    fn update_title(&self, window: &Window) {
        let menu = self
            .menu
            .as_ref()
            .map(|menu| format!(" | screen menu: {} entries", menu.entries.len()))
            .unwrap_or_default();
        window.set_title(&format!(
            "AutoCAD 1.4 — {} {} {}{}",
            self.editor.prompt(),
            self.input,
            self.status,
            menu,
        ));
    }
}

fn load_menu_file(
    requested: &str,
) -> Result<(std::path::PathBuf, acad_cmd::menu::MenuFile), String> {
    let requested = std::path::PathBuf::from(requested.trim());
    let mut candidates = vec![requested.clone()];
    if requested.extension().is_none() {
        candidates.push(requested.with_extension("MNU"));
    }
    if let Some(name) = candidates.last().cloned() {
        let corpus = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../corpus");
        candidates.push(corpus.join("System").join(&name));
        candidates.push(corpus.join("Samples").join(name));
    }
    let path = candidates
        .into_iter()
        .find(|candidate| candidate.is_file())
        .ok_or_else(|| format!("menu file not found: {}", requested.display()))?;
    let bytes = std::fs::read(&path)
        .map_err(|error| format!("cannot read menu file {}: {error}", path.display()))?;
    let menu = acad_cmd::menu::parse_menu(&bytes)
        .map_err(|error| format!("{}: {error}", path.display()))?;
    Ok((path, menu))
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
        let Some(window) = self.state.as_ref().map(|(window, _)| window.clone()) else {
            return;
        };
        match event {
            WindowEvent::CloseRequested => el.exit(),
            WindowEvent::KeyboardInput { event, .. } if event.state == ElementState::Pressed => {
                match &event.logical_key {
                    Key::Named(NamedKey::Enter) => {
                        let input = std::mem::take(&mut self.input);
                        self.submit_line(el, &input);
                    }
                    Key::Named(NamedKey::Backspace) => {
                        self.input.pop();
                    }
                    Key::Named(NamedKey::Escape) => {
                        self.input.clear();
                        let result = self.editor.cancel_command();
                        self.handle_result(el, result);
                    }
                    Key::Character(text) if !text.chars().any(char::is_control) => {
                        self.input.push_str(text);
                    }
                    _ => {}
                }
                self.update_title(&window);
                window.request_redraw();
            }
            WindowEvent::CursorMoved { position, .. } => {
                self.cursor = Some((position.x, position.y));
                window.request_redraw();
            }
            WindowEvent::MouseInput {
                state: ElementState::Pressed,
                button: winit::event::MouseButton::Left,
                ..
            } => {
                let size = window.inner_size();
                if self.editor.accepts_mouse_point() {
                    self.input.clear();
                    self.submit_mouse_point(el, size.width, size.height);
                } else {
                    self.pick_mouse_entity(size.width, size.height);
                }
                self.update_title(&window);
                window.request_redraw();
            }
            WindowEvent::RedrawRequested => {
                let size = window.inner_size();
                let (Some(w), Some(h)) =
                    (NonZeroU32::new(size.width), NonZeroU32::new(size.height))
                else {
                    return;
                };
                let drawing = self.editor.drawing();
                let vp = viewport_for(drawing, size.width, size.height);
                let rendered = flatten_with_libraries(drawing, &vp, &self.libraries);
                for diagnostic in rendered.diagnostics {
                    if self.reported.insert(diagnostic.clone()) {
                        eprintln!("render: {diagnostic}");
                    }
                }
                let mut primitives = rendered.primitives;
                if self.editor.accepts_mouse_selection() {
                    primitives.extend(selected_highlight(
                        drawing,
                        &self.input,
                        &vp,
                        &self.libraries,
                    ));
                }
                let pm = rasterize(&primitives, size.width, size.height);
                let cursor = self.cursor;
                let Some((_, surface)) = self.state.as_mut() else {
                    return;
                };
                surface.resize(w, h).unwrap();
                let mut buffer = surface.buffer_mut().unwrap();
                for (dst, src) in buffer.iter_mut().zip(pm.pixels()) {
                    *dst = ((src.red() as u32) << 16)
                        | ((src.green() as u32) << 8)
                        | src.blue() as u32;
                }
                if let Some(spacing) = self.editor.axis_spacing() {
                    draw_axis_ticks(&mut buffer, size.width, size.height, &vp, spacing);
                }
                if let Some((x, y)) = cursor {
                    draw_crosshair(&mut buffer, size.width, size.height, x, y);
                }
                buffer.present().unwrap();
            }
            _ => {}
        }
    }
}

fn shape_library_path(requested: &str) -> Option<std::path::PathBuf> {
    let path = std::path::PathBuf::from(requested.trim());
    if path
        .extension()
        .is_some_and(|extension| extension.eq_ignore_ascii_case("shp"))
        && path.is_file()
    {
        return Some(path);
    }
    if path.extension().is_none() {
        let with_extension = path.with_extension("SHP");
        if with_extension.is_file() {
            return Some(with_extension);
        }
    }
    None
}

fn selected_highlight(
    drawing: &acad_model::Drawing,
    input: &str,
    viewport: &Viewport,
    libraries: &Libraries,
) -> Vec<Prim> {
    let selected: std::collections::BTreeSet<usize> = input
        .split(',')
        .filter_map(|part| part.trim().parse().ok())
        .collect();
    if selected.is_empty() {
        return Vec::new();
    }

    let mut selectable_id = 0;
    let mut items = Vec::new();
    for item in &drawing.items {
        match item {
            acad_model::Item::Entity(entity) if !is_load_entity(entity) => {
                selectable_id += 1;
                if selected.contains(&selectable_id) {
                    items.push(item.clone());
                }
            }
            // INSERT geometry is expanded through its block definition, so
            // retain definitions while rendering the selected top-level item.
            acad_model::Item::Block(_) => items.push(item.clone()),
            _ => {}
        }
    }
    if !items
        .iter()
        .any(|item| matches!(item, acad_model::Item::Entity(_)))
    {
        return Vec::new();
    }
    let selected_drawing = acad_model::Drawing {
        header: drawing.header.clone(),
        items,
    };
    highlight_primitives(flatten_with_libraries(&selected_drawing, viewport, libraries).primitives)
}

fn is_load_entity(entity: &acad_model::Entity) -> bool {
    match entity {
        acad_model::Entity::Load { .. } => true,
        acad_model::Entity::OnLayer { entity, .. } => is_load_entity(entity),
        _ => false,
    }
}

fn highlight_primitives(primitives: Vec<Prim>) -> Vec<Prim> {
    const HIGHLIGHT: [u8; 3] = [255, 255, 0];
    primitives
        .into_iter()
        .map(|primitive| match primitive {
            Prim::Polyline(points) | Prim::ColoredPolyline { points, .. } => {
                Prim::ColoredPolyline {
                    points,
                    rgb: HIGHLIGHT,
                }
            }
            // Draw only the perimeter on top of the original filled geometry.
            Prim::FilledPolygon(points) | Prim::ColoredFilledPolygon { points, .. } => {
                Prim::ColoredPolyline {
                    points,
                    rgb: HIGHLIGHT,
                }
            }
        })
        .collect()
}

fn viewport_for(drawing: &acad_model::Drawing, width: u32, height: u32) -> Viewport {
    let view = drawing.header.view;
    if view.height.is_finite() && view.height > 0.0 {
        Viewport::from_view(view.center, view.height, width, height)
    } else {
        Viewport::fit(&drawing.header.limits, width, height)
    }
}

fn draw_crosshair(buffer: &mut [u32], width: u32, height: u32, x: f64, y: f64) {
    if !x.is_finite()
        || !y.is_finite()
        || x < 0.0
        || y < 0.0
        || x >= width as f64
        || y >= height as f64
    {
        return;
    }
    let x = x as usize;
    let y = y as usize;
    let width = width as usize;
    let height = height as usize;
    let green = 0x00d060;
    for column in 0..width {
        buffer[y * width + column] = green;
    }
    for row in 0..height {
        buffer[row * width + x] = green;
    }
}

fn draw_axis_ticks(buffer: &mut [u32], width: u32, height: u32, viewport: &Viewport, spacing: f64) {
    if width < 2 || height < 2 || !spacing.is_finite() || spacing <= 0.0 {
        return;
    }
    let (width, height) = (width as usize, height as usize);
    if buffer.len() < width.saturating_mul(height) {
        return;
    }
    let world_at = |x: f64, y: f64| viewport.to_world(acad_model::Point { x, y });
    let min = world_at(0.0, height as f64);
    let max = world_at(width as f64, 0.0);
    let pixel_step = (viewport
        .to_screen(acad_model::Point { x: spacing, y: 0.0 })
        .x
        - viewport.to_screen(acad_model::Point { x: 0.0, y: 0.0 }).x)
        .abs();
    if !min.x.is_finite() || !max.x.is_finite() || !min.y.is_finite() || !max.y.is_finite() {
        return;
    }
    let stride = if pixel_step > 0.0 {
        (8.0 / pixel_step).ceil().max(1.0)
    } else {
        1.0
    };
    let world_step = spacing * stride;
    if !world_step.is_finite() || world_step <= 0.0 {
        return;
    }

    const TICK: u32 = 0x0060_6060;
    const LENGTH: usize = 6;
    let draw = |buffer: &mut [u32], x: usize, y: usize| {
        if x < width && y < height {
            buffer[y * width + x] = TICK;
        }
    };

    let first_x = (min.x / world_step).ceil() * world_step;
    let mut x_world = first_x;
    let mut count = 0usize;
    while x_world <= max.x && count < width.saturating_add(1) {
        let screen_x = viewport
            .to_screen(acad_model::Point { x: x_world, y: 0.0 })
            .x;
        if screen_x.is_finite() && screen_x >= 0.0 && screen_x < width as f64 {
            let x = screen_x.round() as usize;
            for offset in 0..LENGTH {
                draw(buffer, x, offset);
                draw(buffer, x, height - 1 - offset);
            }
        }
        x_world += world_step;
        count += 1;
    }

    let first_y = (min.y / world_step).ceil() * world_step;
    let mut y_world = first_y;
    let mut count = 0usize;
    while y_world <= max.y && count < height.saturating_add(1) {
        let screen_y = viewport
            .to_screen(acad_model::Point { x: 0.0, y: y_world })
            .y;
        if screen_y.is_finite() && screen_y >= 0.0 && screen_y < height as f64 {
            let y = screen_y.round() as usize;
            for offset in 0..LENGTH {
                draw(buffer, offset, y);
                draw(buffer, width - 1 - offset, y);
            }
        }
        y_world += world_step;
        count += 1;
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

fn save_dwg(path: &str, drawing: &acad_model::Drawing) -> Result<(), String> {
    let bytes = acad_dwg::write(drawing).map_err(|e| e.to_string())?;
    std::fs::write(path, bytes).map_err(|e| e.to_string())
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
    let mut editor = acad_cmd::Editor::new(drawing);
    for (name, library) in libraries.iter() {
        editor.register_shape_library(
            name,
            library
                .named_shapes()
                .map(|(shape, number)| (shape.to_owned(), number)),
        );
    }
    let el = EventLoop::new().unwrap();
    el.run_app(&mut App {
        editor,
        libraries,
        menu: None,
        reported: Default::default(),
        state: None,
        input: String::new(),
        status: String::new(),
        cursor: None,
    })
    .unwrap();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn menu_loader_resolves_extension_and_preserves_macro_controls() {
        let (path, menu) = load_menu_file("ACAD").unwrap();
        assert!(path.ends_with("corpus/System/ACAD.MNU"));
        assert!(menu
            .entries
            .iter()
            .any(|entry| entry.label == "^Snap" && entry.action == [0x02]));
        assert!(menu
            .entries
            .iter()
            .any(|entry| entry.label == "ZOOM All" && entry.action == b"zoom a"));
    }

    #[test]
    fn crosshair_marks_both_axes_at_the_mouse_position() {
        let mut pixels = vec![0; 5 * 4];
        draw_crosshair(&mut pixels, 5, 4, 2.0, 1.0);

        for x in 0..5 {
            assert_eq!(pixels[5 + x], 0x00d060);
        }
        for y in 0..4 {
            assert_eq!(pixels[y * 5 + 2], 0x00d060);
        }
        assert_eq!(pixels[0], 0);
        assert_eq!(pixels[19], 0);
    }

    #[test]
    fn crosshair_ignores_positions_outside_the_canvas() {
        let mut pixels = vec![0; 12];
        draw_crosshair(&mut pixels, 4, 3, -1.0, 1.0);
        draw_crosshair(&mut pixels, 4, 3, f64::NAN, 1.0);
        assert_eq!(pixels, vec![0; 12]);
    }

    #[test]
    fn axis_setting_draws_ticks_at_world_space_multiples_on_each_edge() {
        let mut pixels = vec![0; 100 * 100];
        let viewport = Viewport::from_view(acad_model::Point { x: 0.0, y: 0.0 }, 10.0, 100, 100);
        draw_axis_ticks(&mut pixels, 100, 100, &viewport, 2.0);

        let tick = 0x0060_6060;
        assert_eq!(pixels[30], tick, "x=−2 reaches the top ruler");
        assert_eq!(pixels[99 * 100 + 70], tick, "x=2 reaches the bottom ruler");
        assert_eq!(pixels[30 * 100], tick, "y=2 reaches the left ruler");
        assert_eq!(pixels[70 * 100 + 99], tick, "y=−2 reaches the right ruler");
        assert_eq!(pixels[50 * 100 + 50], 0, "ticks do not cover the drawing");
    }

    #[test]
    fn axis_tick_spacing_is_decimated_to_keep_dense_views_readable() {
        let mut pixels = vec![0; 100 * 100];
        let viewport = Viewport::from_view(acad_model::Point { x: 0.0, y: 0.0 }, 10.0, 100, 100);
        draw_axis_ticks(&mut pixels, 100, 100, &viewport, 0.01);
        let top_ticks = pixels[..100].iter().filter(|&&pixel| pixel != 0).count();
        assert!(
            (1..=20).contains(&top_ticks),
            "unexpected tick density {top_ticks}"
        );
    }

    #[test]
    fn selection_highlight_uses_yellow_outlines_for_strokes_and_fills() {
        let line = vec![acad_model::Point { x: 1.0, y: 2.0 }];
        let polygon = vec![acad_model::Point { x: 3.0, y: 4.0 }];
        assert_eq!(
            highlight_primitives(vec![
                Prim::ColoredPolyline {
                    points: line.clone(),
                    rgb: [0, 255, 255],
                },
                Prim::FilledPolygon(polygon.clone()),
            ]),
            vec![
                Prim::ColoredPolyline {
                    points: line,
                    rgb: [255, 255, 0],
                },
                Prim::ColoredPolyline {
                    points: polygon,
                    rgb: [255, 255, 0],
                },
            ]
        );
    }

    #[test]
    fn selection_highlight_uses_the_editor_ids_and_skips_load_records() {
        use acad_model::{DwgView, Entity, Extents, Header, Item, Mode, Point};
        let point = |x, y| Point { x, y };
        let line = |y| {
            Item::Entity(Entity::Line {
                start: point(0.0, y),
                end: point(2.0, y),
            })
        };
        let extents = Extents {
            xmin: -1.0,
            xmax: 3.0,
            ymin: -1.0,
            ymax: 6.0,
        };
        let drawing = acad_model::Drawing {
            header: Header {
                extents,
                limits: extents,
                base: point(0.0, 0.0),
                view: DwgView {
                    center: point(0.0, 0.0),
                    height: 10.0,
                },
                axis: Mode {
                    on: false,
                    spacing: 0.0,
                },
                snap: Mode {
                    on: false,
                    spacing: 0.0,
                },
                grid: Mode {
                    on: false,
                    spacing: 0.0,
                },
                ortho: false,
                fill: false,
                text_size: 1.0,
                trace_width: 0.0,
                units: acad_model::Units {
                    format: acad_model::UnitFormat::Decimal,
                    precision: 4,
                },
                current_layer: 1,
                layers: Default::default(),
                dwg_header_passthrough: None,
            },
            items: vec![
                line(1.0),
                Item::Entity(Entity::Load {
                    name: "FONT".into(),
                }),
                line(4.0),
            ],
        };
        let viewport = Viewport::from_view(point(0.0, 0.0), 10.0, 100, 100);
        let selected = selected_highlight(&drawing, "2", &viewport, &Libraries::default());
        let expected_start = viewport.to_screen(point(0.0, 4.0));
        let expected_end = viewport.to_screen(point(2.0, 4.0));
        assert_eq!(
            selected,
            vec![Prim::ColoredPolyline {
                points: vec![expected_start, expected_end],
                rgb: [255, 255, 0],
            }]
        );
        let image = rasterize(&selected, 100, 100);
        assert!(image
            .pixels()
            .iter()
            .any(|pixel| pixel.red() > 100 && pixel.green() > 100 && pixel.blue() < 30));
    }

    #[test]
    fn load_accepts_a_new_shp_path_and_makes_its_shapes_available() {
        let path =
            std::env::temp_dir().join(format!("acad-app-load-test-{}.SHP", std::process::id()));
        std::fs::write(&path, b"*129,13,STAR\n2,8,(4,2),1,9,(2,0),(0,2),(0,0),0;\n").unwrap();
        let mut app = App {
            editor: acad_cmd::Editor::default(),
            libraries: Libraries::default(),
            menu: None,
            reported: Default::default(),
            state: None,
            input: String::new(),
            status: String::new(),
            cursor: None,
        };
        app.editor.submit("LOAD").unwrap();
        let library_name = app.resolve_shape_library(path.to_str().unwrap()).unwrap();
        app.editor.submit(&library_name).unwrap();
        for input in ["SHAPE", "star", "1,2", "0.5", "45"] {
            app.editor.submit(input).unwrap();
        }
        let items = &app.editor.drawing().items;
        assert!(matches!(
            items.first(),
            Some(acad_model::Item::Entity(acad_model::Entity::OnLayer { entity, .. }))
                if matches!(entity.as_ref(), acad_model::Entity::Load { name } if name == &library_name)
        ));
        assert!(matches!(
            items.get(1),
            Some(acad_model::Item::Entity(acad_model::Entity::OnLayer { entity, .. }))
                if matches!(entity.as_ref(), acad_model::Entity::Shape { number: 129, .. })
        ));
        let viewport = viewport_for(app.editor.drawing(), 100, 100);
        let output = flatten_with_libraries(app.editor.drawing(), &viewport, &app.libraries);
        assert!(!output.primitives.is_empty());
        std::fs::remove_file(path).unwrap();
    }
}
