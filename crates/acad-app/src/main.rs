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
    menu_page: usize,
    reported: std::collections::BTreeSet<String>,
    state: Option<WindowState>,
    input: String,
    status: String,
    cursor: Option<(f64, f64)>,
}

impl App {
    fn submit_return_input(
        &mut self,
        handle_result: &mut impl FnMut(&mut Self, Result<acad_cmd::Effect, String>),
    ) {
        let input = std::mem::take(&mut self.input);
        self.status.clear();
        let command_input = if self.editor.awaiting_shape_library_name() {
            match self.resolve_shape_library(&input) {
                Ok(name) => name,
                Err(error) => {
                    self.status = error;
                    return;
                }
            }
        } else {
            input
        };
        let result = self.editor.submit_return(&command_input);
        handle_result(self, result);
    }

    fn unload_menu(&mut self) {
        self.menu = None;
        self.menu_page = 0;
        self.status.clear();
        if let Some((window, _)) = &self.state {
            self.apply_menu_window_size(window);
        }
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

    /// Dispatches a left-click against the rendered screen-menu panel, if
    /// one is loaded and the click lands inside it. Returns `true` when the
    /// click was handled (including an unsupported
    /// control-byte entry or a blank panel slot) so the caller skips the ordinary
    /// `submit_mouse_point`/`pick_mouse_entity` dispatch; returns `false`
    /// when there is no menu, no cursor position, or the click fell outside
    /// the panel rect, so the caller should fall through as usual.
    ///
    /// Separator resolution (brief Task 4 Step 1, revised by Task 5's QEMU
    /// differential test): `Editor::submit` (`crates/acad-cmd/src/dispatch.rs`)
    /// trims its input and, while in `InputState::Command`, passes the
    /// *entire* trimmed string straight to `self.command(line)` with no scan
    /// for `;` or whitespace anywhere in `submit`/`command`. `command`'s
    /// match arms compare the whole upper-cased string against literal
    /// keywords like `"END"`, so a macro's embedded `;` (e.g. `ACAD.MNU`'s
    /// `[END/save]end;` -> action bytes `b"end;"`) would never match
    /// anything if passed through whole. The same is true of an embedded
    /// space: `[ZOOM All]zoom a` decodes to `b"zoom a"`, and `"ZOOM A"` as
    /// one literal string matches neither `"ZOOM"`/`"Z"` in `InputState::Command`
    /// nor, were it to reach `InputState::Zoom` some other way, the `"A"`/`"ALL"`
    /// sub-match there — confirmed by Task 5's QEMU recovery, where the
    /// native command line echoes `zoom` and `a` as two separate pieces of
    /// input (`Command: zoom Magnification or type (ACELPW): a`). For that
    /// shipped command-prompt macro, the space separates submissions. This method
    /// splits decoded macro text on `;` and Rust's Unicode whitespace.
    /// Each semicolon-delimited piece with no words (empty or all whitespace)
    /// yields one blank submission. This preserves the existing splitter;
    /// context-sensitive text and native whitespace semantics are unrecovered.
    fn handle_panel_click(
        &mut self,
        width: u32,
        height: u32,
        handle_result: &mut impl FnMut(&mut Self, Result<acad_cmd::Effect, String>),
    ) -> bool {
        let Some(menu) = self.menu.clone() else {
            return false;
        };
        let Some((x, y)) = self.cursor else {
            return false;
        };
        let layout = menu_panel::layout_for(&menu, self.menu_page, width, height);
        if !layout.contains(x, y) {
            return false;
        }
        let Some(hit) = menu_panel::entry_at(&menu, self.menu_page, &layout, x, y) else {
            return true;
        };
        match hit {
            menu_panel::PanelHit::Entry(index) => {
                let Some(entry) = menu_panel::resolve_entry(&menu, self.menu_page, index) else {
                    return true;
                };
                let control = match entry.action.as_slice() {
                    [0x02] => Some(acad_cmd::MenuControl::Snap),
                    [0x0f] => Some(acad_cmd::MenuControl::Ortho),
                    [0x03] => Some(acad_cmd::MenuControl::Cancel),
                    _ => None,
                };
                if let Some(control) = control {
                    if control == acad_cmd::MenuControl::Cancel {
                        self.input.clear();
                    }
                    let result = self.editor.apply_menu_control(control);
                    handle_result(self, result);
                    return true;
                }
                if entry.action.len() == 1 && entry.action[0] < 0x20 {
                    self.status = format!(
                        "{} is not yet implemented (control byte 0x{:02x})",
                        entry.label, entry.action[0]
                    );
                    return true;
                }
                let Ok(text) = std::str::from_utf8(&entry.action) else {
                    self.status = format!("{}: macro is not valid UTF-8", entry.label);
                    return true;
                };
                for piece in split_macro_pieces(text) {
                    let result = self.editor.submit(piece);
                    handle_result(self, result);
                }
                true
            }
            menu_panel::PanelHit::Next => {
                self.menu_page = advance_menu_page(self.menu_page, &menu);
                true
            }
            menu_panel::PanelHit::Go => {
                self.submit_return_input(handle_result);
                true
            }
        }
    }

    /// The real MouseInput routing seam. Only effect handling needs an event
    /// loop; tests can observe effects while exercising this entire route.
    fn handle_left_click(
        &mut self,
        width: u32,
        height: u32,
        mut handle_result: impl FnMut(&mut Self, Result<acad_cmd::Effect, String>),
    ) {
        if !self.handle_panel_click(width, height, &mut handle_result) {
            if self.editor.accepts_mouse_point() {
                self.input.clear();
                self.submit_mouse_point(width, height, &mut handle_result);
            } else {
                self.pick_mouse_entity(width, height);
            }
        }
    }

    fn submit_mouse_point(
        &mut self,
        width: u32,
        height: u32,
        handle_result: &mut impl FnMut(&mut Self, Result<acad_cmd::Effect, String>),
    ) {
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
        handle_result(self, result);
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
            Ok(acad_cmd::Effect::UnloadMenu) => self.unload_menu(),
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
                    self.menu_page = 0;
                    if let Some((window, _)) = &self.state {
                        self.apply_menu_window_size(window);
                    }
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

    /// Refresh physical limits after menu load/resume/DPI changes, and grow
    /// an existing undersized client rather than leaving NEXT inaccessible.
    fn apply_menu_window_size(&self, window: &Window) {
        let minimum = menu_min_inner_size(self.menu.as_ref());
        window.set_min_inner_size(minimum);
        let current = window.inner_size();
        let supported = supported_client_size(self.menu.as_ref(), current);
        if supported != current {
            let _ = window.request_inner_size(supported);
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

/// Splits on semicolons and Unicode whitespace (`str::split_whitespace`).
/// An empty or all-whitespace semicolon-delimited piece yields one blank
/// submission; repeated whitespace between words does not. This is the
/// existing command-macro policy, not recovered general TEXT semantics.
fn split_macro_pieces(text: &str) -> impl Iterator<Item = &str> {
    text.split(';').flat_map(|piece| {
        // Preserve one blank Enter for every piece containing no words,
        // including a trailing semicolon or an all-whitespace piece.
        let mut words: Vec<&str> = piece.split_whitespace().collect();
        if words.is_empty() {
            words.push("");
        }
        words
    })
}

/// A loaded menu alone imposes a minimum; use physical client pixels so
/// the policy matches both the raster and cursor coordinates on every DPI.
fn menu_min_inner_size(
    menu: Option<&acad_cmd::menu::MenuFile>,
) -> Option<winit::dpi::PhysicalSize<u32>> {
    menu.map(|menu| {
        let (width, height) = menu_panel::required_panel_size(menu);
        winit::dpi::PhysicalSize::new(width, height)
    })
}

fn supported_client_size(
    menu: Option<&acad_cmd::menu::MenuFile>,
    current: winit::dpi::PhysicalSize<u32>,
) -> winit::dpi::PhysicalSize<u32> {
    match menu_min_inner_size(menu) {
        Some(minimum) => winit::dpi::PhysicalSize::new(
            current.width.max(minimum.width),
            current.height.max(minimum.height),
        ),
        None => current,
    }
}

/// NEXT advances independently of command submission and wraps the last page.
fn advance_menu_page(current_page: usize, menu: &acad_cmd::menu::MenuFile) -> usize {
    let pages = menu_panel::page_count(menu);
    if pages > 0 {
        (current_page + 1) % pages
    } else {
        current_page
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
        self.apply_menu_window_size(&window);
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
                        self.submit_return_input(&mut |app, result| app.handle_result(el, result));
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
                self.handle_left_click(size.width, size.height, |app, result| {
                    app.handle_result(el, result);
                });
                self.update_title(&window);
                window.request_redraw();
            }
            WindowEvent::Resized(_) | WindowEvent::ScaleFactorChanged { .. } => {
                self.apply_menu_window_size(&window);
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
                if let Some(menu) = &self.menu {
                    let layout =
                        menu_panel::layout_for(menu, self.menu_page, size.width, size.height);
                    menu_panel::draw_panel(
                        &mut buffer,
                        size.width,
                        size.height,
                        menu,
                        self.menu_page,
                        &layout,
                    );
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
        menu_page: 0,
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
            menu_page: 0,
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

    #[test]
    fn split_macro_pieces_splits_on_semicolon_as_an_enter_keypress() {
        // ACAD.MNU's `[END/save]end;` entry (menu.rs confirms action bytes
        // are b"end;") must become two submissions: "end" (matches the
        // "END" arm case-insensitively) and "" (a harmless no-op per
        // dispatch.rs's `"" => {}` arm), not one literal "end;" submission
        // that would fail to match any command keyword.
        let pieces: Vec<&str> = split_macro_pieces("end;").collect();
        assert_eq!(pieces, vec!["end", ""]);
    }

    #[test]
    fn split_macro_pieces_also_splits_on_whitespace_as_an_enter_keypress() {
        // ACAD.MNU's `[ZOOM All]zoom a` entry decodes to `b"zoom a"`: a bare
        // literal "zoom a" never matches any `command()` keyword (it only
        // ever compares the whole uppercased string), so the embedded space
        // must split exactly like `;` does — confirmed under QEMU (Task 5):
        // clicking `ZOOM All` natively echoes `zoom` and `a` as two separate
        // command-line submissions, not one.
        let pieces: Vec<&str> = split_macro_pieces("zoom a").collect();
        assert_eq!(pieces, vec!["zoom", "a"]);
    }

    #[test]
    fn advance_menu_page_wraps_next() {
        let menu =
            acad_cmd::menu::parse_menu(include_bytes!("../../../corpus/System/ACAD.MNU")).unwrap();
        assert_eq!(menu_panel::page_count(&menu), 3);
        assert_eq!(advance_menu_page(0, &menu), 1);
        assert_eq!(advance_menu_page(1, &menu), 2);
        // NEXT on the last page wraps back to page 0.
        assert_eq!(advance_menu_page(2, &menu), 0);
    }

    fn menu_app(page: usize) -> App {
        App {
            editor: acad_cmd::Editor::default(),
            libraries: Libraries::default(),
            menu: Some(load_menu_file("ACAD").unwrap().1),
            menu_page: page,
            reported: Default::default(),
            state: None,
            input: String::new(),
            status: String::new(),
            cursor: None,
        }
    }

    fn click(app: &mut App, width: u32, height: u32) {
        app.handle_left_click(width, height, |app, result| {
            assert!(matches!(result.unwrap(), acad_cmd::Effect::Continue));
            app.status = app.editor.status().to_owned();
        });
    }

    #[test]
    fn app_mouse_route_consumes_every_blank_slot_during_point_and_selection_prompts() {
        let (width, height) = (640, 480);
        for (page, blank_rows) in [(1, vec![0, 1]), (2, vec![0])] {
            for row in blank_rows {
                let mut app = menu_app(page);
                let layout =
                    menu_panel::layout_for(app.menu.as_ref().unwrap(), page, width, height);
                app.cursor = Some((
                    (layout.rect.0 + 2) as f64,
                    (row * layout.row_height + 2) as f64,
                ));
                app.editor.submit("POINT").unwrap();
                app.input = "pending input".into();
                app.status = "unchanged".into();
                click(&mut app, width, height);
                assert!(app.editor.accepts_mouse_point());
                assert_eq!(app.editor.drawing().entities().count(), 0);
                assert_eq!(app.input, "pending input");
                assert_eq!(app.status, "unchanged");

                // Put an entity under the blank panel slot. A leaked click
                // would now select it, rather than merely finding no entity.
                let (x, y) = app.cursor.unwrap();
                let world = viewport_for(app.editor.drawing(), width, height)
                    .to_world(acad_model::Point { x, y });
                let original_header = app.editor.drawing().header.clone();
                app.editor.submit_mouse_point(world).unwrap();
                let mut drawing = app.editor.drawing().clone();
                drawing.header = original_header;
                app.editor = acad_cmd::Editor::new(drawing);
                assert_eq!(app.editor.pick_entity_at(world, 0.00001), Some(1));
                app.editor.submit("ERASE").unwrap();
                assert!(app.editor.accepts_mouse_selection());
                app.input.clear();
                click(&mut app, width, height);
                assert!(app.input.is_empty(), "blank row selected a hidden entity");
                assert_eq!(app.status, "unchanged");
            }
        }
    }

    #[test]
    fn app_mouse_route_preserves_outside_panel_point_conversion_and_selection() {
        let (width, height) = (640, 480);
        let mut app = menu_app(1);
        app.cursor = Some((100.0, 200.0));
        let expected = viewport_for(app.editor.drawing(), width, height)
            .to_world(acad_model::Point { x: 100.0, y: 200.0 });
        app.editor.submit("POINT").unwrap();
        app.input = "pending input".into();
        click(&mut app, width, height);
        assert!(app.input.is_empty());
        assert_eq!(app.editor.drawing().entities().count(), 1);
        // Prove the submitted point used the drawing's coordinate conversion.
        // First-entity creation changes the view, so pick its new screen position.
        assert_eq!(app.editor.pick_entity_at(expected, 0.00001), Some(1));
        let screen = viewport_for(app.editor.drawing(), width, height).to_screen(expected);
        app.cursor = Some((screen.x, screen.y));
        app.editor.submit("ERASE").unwrap();
        assert!(app.editor.accepts_mouse_selection());
        click(&mut app, width, height);
        assert_eq!(app.input, "1");
        assert_eq!(app.status, "Selected entity 1");
    }

    #[test]
    fn app_mouse_route_dispatches_point_macro() {
        let (width, height) = (640, 480);
        let mut app = menu_app(1);
        let layout = menu_panel::layout_for(app.menu.as_ref().unwrap(), 1, width, height);
        app.cursor = Some((
            (layout.rect.0 + 2) as f64,
            (2 * layout.row_height + 2) as f64,
        ));
        click(&mut app, width, height);
        assert!(
            app.editor.accepts_mouse_point(),
            "POINT row must submit its macro"
        );
    }

    #[test]
    fn loaded_menu_minimum_keeps_next_rendered_and_routable_at_supported_sizes() {
        use winit::dpi::PhysicalSize;
        let mut app = menu_app(0);
        let menu = app.menu.as_ref().unwrap();
        let minimum = menu_min_inner_size(Some(menu)).unwrap();
        assert_eq!(minimum, PhysicalSize::new(160, 336));
        for requested_height in [335, 336, 337] {
            let requested = PhysicalSize::new(640, requested_height);
            let supported = supported_client_size(Some(menu), requested);
            assert_eq!(supported.height, requested_height.max(336));
            assert_eq!(supported_client_size(None, requested), requested);
            for page in 0..3 {
                let layout = menu_panel::layout_for(menu, page, supported.width, supported.height);
                assert_eq!(layout.rect.3, minimum.height as usize);
                let next_y = (layout.rect.3 - layout.row_height + 2) as f64;
                assert_eq!(
                    menu_panel::entry_at(menu, page, &layout, (layout.rect.0 + 2) as f64, next_y),
                    Some(menu_panel::PanelHit::Next)
                );
                let mut buffer = vec![0; supported.width as usize * supported.height as usize];
                menu_panel::draw_panel(
                    &mut buffer,
                    supported.width,
                    supported.height,
                    menu,
                    page,
                    &layout,
                );
                let text_x = layout.rect.0 + menu_panel::GLYPH_WIDTH;
                assert!(
                    ((layout.rect.3 - layout.row_height)..layout.rect.3).any(|y| {
                        (text_x..text_x + 16)
                            .any(|x| buffer[y * supported.width as usize + x] == 0x00ff_ffff)
                    })
                );
            }
        }
        assert_eq!(menu_min_inner_size(None), None);
        assert_eq!(
            supported_client_size(Some(menu), PhysicalSize::new(159, 335)),
            minimum
        );
        // At the smallest supported physical client size, actual App routing
        // still cycles all recovered pages through the visible NEXT slot.
        for expected_page in [1, 2, 0] {
            app.cursor = Some((2.0, 322.0));
            click(&mut app, minimum.width, minimum.height);
            assert_eq!(app.menu_page, expected_page);
        }
    }

    #[test]
    fn app_mouse_route_has_no_panel_without_a_loaded_menu() {
        let mut app = menu_app(1);
        app.menu = None;
        app.cursor = Some((482.0, 2.0));
        app.editor.submit("POINT").unwrap();
        click(&mut app, 640, 480);
        assert_eq!(app.editor.drawing().entities().count(), 1);
    }

    fn menu_control_fixture(path: &str) -> acad_model::Drawing {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../docs/recovery/2026-10-01-menu-controls")
            .join(path);
        acad_dwg::parse(&std::fs::read(path).unwrap()).unwrap()
    }

    fn control_cursor(app: &mut App, byte: u8) {
        let menu = app.menu.as_ref().unwrap();
        let layout = menu_panel::layout_for(menu, app.menu_page, 640, 480);
        app.cursor = (0..21).find_map(|row| {
            let (x, y) = (
                (layout.rect.0 + 2) as f64,
                (row * layout.row_height + 2) as f64,
            );
            let menu_panel::PanelHit::Entry(index) =
                menu_panel::entry_at(menu, app.menu_page, &layout, x, y)?
            else {
                return None;
            };
            (menu_panel::resolve_entry(menu, app.menu_page, index)?.action == [byte])
                .then_some((x, y))
        });
        assert!(app.cursor.is_some(), "control byte {byte:x} missing");
    }

    #[test]
    fn app_mouse_route_menu_controls_preserve_prompt_and_consume_panel() {
        for (byte, status) in [(0x02, "<Snap on>"), (0x0f, "<Ortho on>")] {
            for setup in [vec!["LINE", "2.1,3.15"], vec!["CIRCLE", "2,3"]] {
                let mut app = menu_app(0);
                app.editor.drawing_mut().header.snap.spacing = 0.5;
                for input in setup {
                    app.editor.submit(input).unwrap();
                }
                let prompt = app.editor.prompt().to_owned();
                let mut expected = app.editor.drawing().clone();
                if byte == 0x02 {
                    expected.header.snap.on = true;
                } else {
                    expected.header.ortho = true;
                }
                app.input = "pending text".into();
                control_cursor(&mut app, byte);
                let mut callbacks = 0;
                app.handle_left_click(640, 480, |app, result| {
                    callbacks += 1;
                    assert_eq!(result, Ok(acad_cmd::Effect::Continue));
                    app.status = app.editor.status().to_owned();
                });
                assert_eq!(callbacks, 1);
                assert_eq!(app.status, status);
                assert_eq!(app.input, "pending text");
                assert_eq!(app.editor.prompt(), prompt);
                assert_eq!(app.editor.drawing(), &expected);
                assert_eq!(app.menu_page, 0);
                assert!(app.menu.is_some());
                let line = prompt.starts_with("LINE");
                for input in if line {
                    vec!["4.25,5.15", ""]
                } else {
                    vec!["1.25"]
                } {
                    app.editor.submit(input).unwrap();
                }
                let id = match (byte, line) {
                    (0x02, true) => "FSLINE",
                    (0x0f, true) => "FOLINE",
                    (0x02, false) => "SCIRCLE",
                    (0x0f, false) => "OCIRCLE",
                    _ => unreachable!(),
                };
                let native = menu_control_fixture(&format!("controls/drawings/{id}.dwg"));
                assert_eq!(app.editor.drawing().items, native.items);
                assert_eq!(app.editor.drawing().header.snap, native.header.snap);
                assert_eq!(app.editor.drawing().header.ortho, native.header.ortho);
            }
        }
        let mut app = menu_app(0);
        let entry = app
            .menu
            .as_mut()
            .unwrap()
            .entries
            .iter_mut()
            .find(|entry| entry.action == [0x02])
            .unwrap();
        entry.action = vec![0x04];
        app.editor.submit("POINT").unwrap();
        app.input = "unchanged".into();
        control_cursor(&mut app, 0x04);
        app.handle_left_click(640, 480, |_, _| panic!("unknown control callback"));
        assert!(app.status.contains("0x04"));
        assert_eq!(app.input, "unchanged");
        assert!(app.editor.accepts_mouse_point());
        assert!(app.editor.drawing().items.is_empty());
        for page in [1, 2] {
            app.menu_page = page;
            let layout = menu_panel::layout_for(app.menu.as_ref().unwrap(), page, 640, 480);
            app.cursor = Some(((layout.rect.0 + 2) as f64, 2.0));
            app.handle_left_click(640, 480, |_, _| panic!("blank callback"));
            assert!(app.editor.drawing().items.is_empty());
            assert_eq!(app.menu_page, page);
        }
    }

    #[test]
    fn app_mouse_route_menu_cancel_clears_buffers_and_retains_selection() {
        for (setup, buffer) in [
            (vec![], "p"),
            (vec!["LINE"], "2"),
            (vec!["ERASE"], "1"),
            (vec!["MENU"], "AC"),
        ] {
            let mut app = menu_app(0);
            for input in setup {
                app.editor.submit(input).unwrap();
            }
            app.input = buffer.into();
            let original = app.editor.drawing().clone();
            control_cursor(&mut app, 0x03);
            let mut callbacks = 0;
            app.handle_left_click(640, 480, |app, result| {
                callbacks += 1;
                assert_eq!(result, Ok(acad_cmd::Effect::Continue));
                app.status = app.editor.status().to_owned();
            });
            assert_eq!(callbacks, 1);
            assert!(app.input.is_empty());
            assert_eq!(app.editor.prompt(), "Command");
            assert_eq!(app.status, "*Cancel*");
            assert_eq!(app.editor.drawing(), &original);
            assert_eq!(app.menu_page, 0);
            assert!(app.menu.is_some());
            for input in ["POINT", "8,7"] {
                app.editor.submit(input).unwrap();
            }
            assert_eq!(app.editor.drawing().items.len(), 1);
            assert_eq!(
                app.editor
                    .pick_entity_at(acad_model::Point { x: 8.0, y: 7.0 }, 1e-12),
                Some(1)
            );
            let id = match buffer {
                "p" => "CBIDLE",
                "2" => "CBPOINT",
                "1" => "CBSELECT",
                _ => continue,
            };
            let native = menu_control_fixture(&format!("cancel-tail/drawings/{id}.dwg"));
            assert_eq!(app.editor.drawing().items, native.items);
            assert_eq!(app.editor.drawing().header.snap, native.header.snap);
            assert_eq!(app.editor.drawing().header.ortho, native.header.ortho);
        }
        let mut app = menu_app(0);
        app.editor = acad_cmd::Editor::new(menu_control_fixture("pilot/drawings/CSELECT.dwg"));
        let original = app.editor.drawing().clone();
        app.editor.submit("ERASE").unwrap();
        let screen = viewport_for(app.editor.drawing(), 640, 480).to_screen(acad_model::Point {
            x: 6.0,
            y: 3.58823529411769,
        });
        app.cursor = Some((screen.x, screen.y));
        click(&mut app, 640, 480);
        assert_eq!(app.input, "1");
        control_cursor(&mut app, 0x03);
        click(&mut app, 640, 480);
        assert!(app.input.is_empty());
        assert_eq!(app.status, "*Cancel*");
        assert_eq!(app.editor.prompt(), "Command");
        assert_eq!(app.editor.drawing(), &original);
    }

    #[test]
    fn app_mouse_route_cancel_preserves_later_pages() {
        for (page, id) in [(1, "CPAGE1"), (2, "CPAGE2")] {
            let mut app = menu_app(page);
            let menu = app.menu.clone();
            app.editor.submit("LINE").unwrap();
            app.input = "2".into();
            let original = app.editor.drawing().clone();
            control_cursor(&mut app, 0x03);
            let mut callbacks = 0;
            app.handle_left_click(640, 480, |app, result| {
                callbacks += 1;
                assert_eq!(result, Ok(acad_cmd::Effect::Continue));
                app.status = app.editor.status().to_owned();
            });
            assert_eq!(callbacks, 1);
            assert_eq!(app.menu_page, page);
            assert_eq!(app.menu, menu);
            assert!(app.menu.is_some());
            assert!(app.input.is_empty());
            assert_eq!(app.editor.prompt(), "Command");
            assert_eq!(app.status, "*Cancel*");
            assert_eq!(app.editor.drawing(), &original);
            for input in ["POINT", "8,7"] {
                app.editor.submit(input).unwrap();
            }
            let native = menu_control_fixture(&format!("cancel-tail/drawings/{id}.dwg"));
            assert_eq!(app.editor.drawing().items, native.items);
            assert_eq!(app.editor.drawing().header.snap, native.header.snap);
            assert_eq!(app.editor.drawing().header.ortho, native.header.ortho);
        }
    }
    fn go_cursor(app: &mut App) {
        let menu = app.menu.as_ref().unwrap();
        assert!(menu.entries.iter().any(|entry| entry.kind
            == acad_cmd::menu::MenuEntryKind::Header
            && entry.action == b";"));
        let layout = menu_panel::layout_for(menu, app.menu_page, 640, 480);
        app.cursor = Some(((layout.rect.0 + 2) as f64, 2.0));
        assert_eq!(
            menu_panel::entry_at(menu, app.menu_page, &layout, app.cursor.unwrap().0, 2.0),
            Some(menu_panel::PanelHit::Go)
        );
    }

    fn return_result(app: &mut App, result: Result<acad_cmd::Effect, String>) {
        match result.unwrap() {
            acad_cmd::Effect::Continue => app.status = app.editor.status().into(),
            acad_cmd::Effect::LoadMenu(name) => {
                app.menu = Some(load_menu_file(&name).unwrap().1);
                app.menu_page = 0;
            }
            acad_cmd::Effect::UnloadMenu => app.unload_menu(),
            effect => panic!("unexpected effect: {effect:?}"),
        }
    }

    fn return_or_go(app: &mut App, go: bool) -> usize {
        let mut count = 0;
        let mut callback = |app: &mut App, result| {
            count += 1;
            return_result(app, result);
        };
        if go {
            go_cursor(app);
            app.handle_left_click(640, 480, &mut callback);
        } else {
            app.submit_return_input(&mut callback);
        }
        count
    }

    #[test]
    fn app_mouse_route_go_and_return_repeat_menu_once_then_unload() {
        for go in [false, true] {
            let mut app = menu_app(0);
            for input in ["MENU", "ACAD"] {
                app.input = input.into();
                assert_eq!(return_or_go(&mut app, false), 1);
            }
            assert_eq!(return_or_go(&mut app, go), 1);
            assert_eq!(app.editor.prompt(), "File name");
            assert!(app.menu.is_some());
            assert_eq!(app.menu_page, 0);
            assert_eq!(return_or_go(&mut app, go), 1);
            assert_eq!(app.editor.prompt(), "Command");
            assert!(app.menu.is_none());
            assert_eq!(app.menu_page, 0);
            assert_eq!(app.status, "");
            assert!(app.input.is_empty());
            assert!(app.editor.drawing().items.is_empty());
            assert_eq!(menu_min_inner_size(app.menu.as_ref()), None);
        }
    }

    #[test]
    fn app_mouse_route_menu_cancel_keeps_panel() {
        let mut app = menu_app(0);
        app.editor.submit_return("MENU").unwrap();
        app.input = "pending".into();
        control_cursor(&mut app, 0x03);
        let mut count = 0;
        app.handle_left_click(640, 480, |app, result| {
            count += 1;
            assert_eq!(result, Ok(acad_cmd::Effect::Continue));
            return_result(app, result);
        });
        assert_eq!(count, 1);
        assert_eq!(app.status, "*Cancel*");
        assert_eq!(app.editor.prompt(), "Command");
        assert!(app.menu.is_some());
        assert_eq!(app.menu_page, 0);
        assert!(app.input.is_empty());
    }

    #[test]
    fn app_mouse_route_go_empty_prompts_and_history() {
        for (setup, status, id) in [
            (vec!["LINE"], "*Invalid*", "GOFIRST"),
            (vec!["LINE", "2,3"], "", "GONEXT"),
            (vec!["CIRCLE", "2,3"], "*Invalid*", "GORADIUS"),
            (vec!["LINE", "2,3", "4,5", "", "ERASE"], "", "GOSELECT"),
        ] {
            let mut app = menu_app(0);
            for input in setup {
                app.editor.submit_return(input).unwrap();
            }
            assert_eq!(return_or_go(&mut app, true), 1);
            assert_eq!(app.status, status);
            assert_eq!(app.editor.prompt(), "Command");
            assert!(app.input.is_empty());
            assert_eq!(app.menu_page, 0);
            let native = menu_control_fixture(&format!("go/drawings/{id}.dwg"));
            assert_eq!(app.editor.drawing().items, native.items);
        }
        let mut app = menu_app(0);
        go_cursor(&mut app);
        app.handle_left_click(640, 480, |app, result| {
            assert_eq!(
                result,
                Err("Unknown command. Type ? for list of commands.".into())
            );
            app.status = result.unwrap_err();
        });
        assert_eq!(app.status, app.editor.status());
        app.input = ";".into();
        assert_eq!(app.input, ";");
        app.submit_return_input(&mut |app, result| {
            assert_eq!(
                result,
                Err("Unknown command. Type ? for list of commands.".into())
            );
            app.status = result.unwrap_err();
        });
        assert!(app.input.is_empty());
        for input in ["POINT", "8,7"] {
            app.editor.submit_return(input).unwrap();
        }
        let items = app.editor.drawing().items.clone();
        for go in [false, true] {
            assert_eq!(return_or_go(&mut app, go), 1);
            assert_eq!(app.editor.prompt(), "POINT: point");
            assert_eq!(app.editor.drawing().items, items);
            app.editor
                .apply_menu_control(acad_cmd::MenuControl::Cancel)
                .unwrap();
        }
    }

    #[test]
    fn app_mouse_route_go_submits_pending_point_and_selection_once() {
        let mut app = menu_app(0);
        app.editor.submit_return("LINE").unwrap();
        app.input = "2,3".into();
        assert_eq!(return_or_go(&mut app, true), 1);
        assert!(app.input.is_empty());
        assert_eq!(app.editor.prompt(), "LINE: next point (Enter to finish)");
        for input in ["4,5", ""] {
            app.input = input.into();
            assert_eq!(return_or_go(&mut app, true), 1);
        }
        assert_eq!(
            app.editor.drawing().items,
            menu_control_fixture("buffers/drawings/GOBUFPNT.dwg").items
        );
        for go in [false, true] {
            let mut app = menu_app(0);
            let mut seed = menu_control_fixture("pilot/drawings/CSELECT.dwg");
            seed.items.truncate(1); // Exclude CSELECT's later continuation POINT.
            app.editor = acad_cmd::Editor::new(seed);
            let original = app.editor.drawing().items.clone();
            app.editor.submit_return("ERASE").unwrap();
            let screen =
                viewport_for(app.editor.drawing(), 640, 480).to_screen(acad_model::Point {
                    x: 6.0,
                    y: 3.58823529411769,
                });
            app.cursor = Some((screen.x, screen.y));
            click(&mut app, 640, 480);
            assert_eq!(app.input, "1");
            assert_eq!(return_or_go(&mut app, go), 1);
            assert!(app.input.is_empty());
            assert_eq!(app.status, "1 selected, 1 found.");
            assert_eq!(app.editor.prompt(), "Command");
            assert_eq!(
                app.editor.drawing().items,
                original
                    .into_iter()
                    .map(|item| match item {
                        acad_model::Item::Entity(entity) => acad_model::Item::Erased(entity),
                        _ => panic!("seed must be a LINE"),
                    })
                    .collect::<Vec<_>>()
            );
            if go {
                for input in ["POINT", "8,7"] {
                    app.editor.submit_return(input).unwrap();
                }
                assert_eq!(
                    app.editor.drawing().items,
                    menu_control_fixture("buffers/drawings/GOBUFSEL.dwg").items
                );
            } else {
                assert_eq!(
                    app.editor.drawing().items,
                    menu_control_fixture("buffers/drawings/CSELBASE.dwg").items
                );
            }
        }
    }

    #[test]
    fn app_mouse_route_return_and_go_preserve_load_resolution() {
        for go in [false, true] {
            let mut app = menu_app(0);
            let path =
                std::env::temp_dir().join(format!("acad-go-load-{}-{go}.SHP", std::process::id()));
            std::fs::write(&path, b"*129,13,STAR\n2,8,(4,2),1,9,(2,0),(0,2),(0,0),0;\n").unwrap();
            app.editor.submit_return("LOAD").unwrap();
            app.input = path.to_str().unwrap().into();
            assert_eq!(return_or_go(&mut app, go), 1);
            assert!(app.input.is_empty());
            assert_eq!(app.editor.prompt(), "Command");
            let name = path.file_stem().unwrap().to_str().unwrap();
            assert!(app.libraries.get(name).is_some());
            for input in ["SHAPE", "star", "1,2", "0.5", "45"] {
                app.editor.submit_return(input).unwrap();
            }
            assert_eq!(app.editor.drawing().items.len(), 2);
            assert!(!flatten_with_libraries(
                app.editor.drawing(),
                &viewport_for(app.editor.drawing(), 640, 480),
                &app.libraries
            )
            .primitives
            .is_empty());
            std::fs::remove_file(path).unwrap();
            for (input, error) in [
                (
                    "menu-controls-no-such-library",
                    "shape library is not available: menu-controls-no-such-library",
                ),
                ("", "shape library name cannot be empty"),
            ] {
                app.editor.submit_return("LOAD").unwrap();
                app.input = input.into();
                assert_eq!(return_or_go(&mut app, go), 0);
                assert!(app.input.is_empty());
                assert_eq!(app.status, error);
                assert!(app.editor.awaiting_shape_library_name());
                app.editor.cancel_command().unwrap();
            }
        }
        // Plain macro pieces retain their LOAD bypass and empty-piece policy,
        // even when the SHP exists on disk and physical Return could resolve it.
        let mut app = menu_app(0);
        let path = std::env::temp_dir().join(format!("acad-macro-load-{}.SHP", std::process::id()));
        std::fs::write(&path, b"*129,13,STAR\n2,8,(4,2),1,9,(2,0),(0,2),(0,0),0;\n").unwrap();
        app.menu = Some(
            acad_cmd::menu::parse_menu(format!("[Load]LOAD {};\n", path.display()).as_bytes())
                .unwrap(),
        );
        let layout = menu_panel::layout_for(app.menu.as_ref().unwrap(), 0, 640, 480);
        app.cursor = Some(((layout.rect.0 + 2) as f64, (layout.row_height + 2) as f64));
        app.input = "kept".into();
        let mut results = Vec::new();
        app.handle_left_click(640, 480, |_, result| results.push(result));
        assert_eq!(
            results,
            vec![
                Ok(acad_cmd::Effect::Continue),
                Err(format!(
                    "shape library is not available: {}",
                    path.display()
                )),
                Err("shape library name cannot be empty".into())
            ]
        );
        assert_eq!(app.input, "kept");
        assert!(app.editor.awaiting_shape_library_name());
        assert!(app.editor.drawing().items.is_empty());
        assert!(app
            .libraries
            .get(path.file_stem().unwrap().to_str().unwrap())
            .is_none());
        app.editor.cancel_command().unwrap();
        assert_eq!(
            app.editor.submit_return(""),
            Err("Unknown command. Type ? for list of commands.".into())
        );
        std::fs::remove_file(path).unwrap();
    }

    #[test]
    fn app_mouse_route_go_header_only_and_next_still_wraps() {
        let mut app = menu_app(0);
        go_cursor(&mut app);
        for page in [1, 2] {
            app.menu_page = page;
            app.editor.submit("LINE").unwrap();
            app.input = "2,3".into();
            let layout = menu_panel::layout_for(app.menu.as_ref().unwrap(), page, 640, 480);
            app.cursor = Some(((layout.rect.0 + 2) as f64, 2.0));
            let mut count = 0;
            app.handle_left_click(640, 480, |_, _| count += 1);
            assert_eq!(count, 0);
            assert_eq!(app.menu_page, page);
            assert_eq!(app.input, "2,3");
            assert_eq!(app.editor.prompt(), "LINE: first point");
            assert!(app.editor.drawing().items.is_empty());
            app.editor.cancel_command().unwrap();
        }
        let layout = menu_panel::layout_for(app.menu.as_ref().unwrap(), 2, 640, 480);
        app.cursor = Some((
            (layout.rect.0 + 2) as f64,
            (layout.rect.3 - layout.row_height + 2) as f64,
        ));
        app.handle_left_click(640, 480, |_, _| panic!("NEXT submitted input"));
        assert_eq!(app.menu_page, 0);
        assert_eq!(app.input, "2,3");
    }
}
