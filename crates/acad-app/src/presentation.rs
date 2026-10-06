//! Shared drawing and UI composition for GUI and automation.
use crate::{command_line, menu_panel, Session};
use acad_render::{
    flatten_with_budget, rasterize, FrameBudget, Libraries, Prim, RenderOutput, Viewport,
};

/// Reserve the command area even without a menu; use physical client pixels so
/// the policy matches both the raster and cursor coordinates on every DPI.
pub(crate) fn menu_min_inner_size(
    menu: Option<&acad_cmd::menu::MenuFile>,
) -> Option<winit::dpi::PhysicalSize<u32>> {
    let (width, height) = menu
        .map(menu_panel::required_panel_size)
        .unwrap_or((160, 1));
    Some(winit::dpi::PhysicalSize::new(
        width,
        height + command_line::HEIGHT,
    ))
}

pub(crate) fn supported_client_size(
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

#[cfg(test)]
pub(crate) fn selected_highlight(
    drawing: &acad_model::Drawing,
    input: &str,
    viewport: &Viewport,
    libraries: &Libraries,
) -> Vec<Prim> {
    selected_highlight_with_budget(
        drawing,
        input,
        viewport,
        libraries,
        &mut FrameBudget::default(),
    )
    .primitives
}

/// Highlight pass of one frame, spending from that frame's shared budget.
/// An exhausted budget omits the whole highlight (never a partial one).
pub(crate) fn selected_highlight_with_budget(
    drawing: &acad_model::Drawing,
    input: &str,
    viewport: &Viewport,
    libraries: &Libraries,
    budget: &mut FrameBudget,
) -> RenderOutput {
    let selected: std::collections::BTreeSet<usize> = input
        .split(',')
        .filter_map(|part| part.trim().parse().ok())
        .collect();
    if selected.is_empty() {
        return RenderOutput::default();
    }

    let selected_indexes: std::collections::BTreeSet<_> = acad_cmd::selectable_items(drawing)
        .filter(|object| selected.contains(&object.id))
        .map(|object| object.item_index)
        .collect();
    if selected_indexes.is_empty() {
        return RenderOutput::default();
    }
    let mut output = acad_render::flatten_selected_with_budget(
        drawing,
        viewport,
        libraries,
        &selected_indexes,
        budget,
    );
    output.primitives = highlight_primitives(output.primitives);
    output
}

/// Visible degradation for any truncated pass: an amber canvas border and a
/// command-area status prefix. The canvas is never presented as complete.
pub(crate) const BUDGET_INDICATOR: u32 = 0x00ff_8000;

/// Status prefix for an incomplete frame; budget stops are named as such,
/// other truncation (context failure, per-owner skip) points at diagnostics.
pub(crate) fn incomplete_status(
    drawing: &RenderOutput,
    highlight: Option<&RenderOutput>,
) -> Option<String> {
    if let Some(stop) = &drawing.budget_stop {
        return Some(format!(
            "RENDER BUDGET: {} owner(s) from item {} not drawn",
            stop.skipped_owners, stop.first_skipped_item
        ));
    }
    if drawing.incomplete {
        return Some("RENDER INCOMPLETE: some owners not drawn (see diagnostics)".into());
    }
    match highlight {
        Some(h) if h.budget_stop.is_some() => {
            Some("RENDER BUDGET: selection highlight omitted".into())
        }
        Some(h) if h.incomplete => {
            Some("RENDER INCOMPLETE: selection highlight incomplete (see diagnostics)".into())
        }
        _ => None,
    }
}

pub(crate) fn draw_budget_border(buffer: &mut [u32], width: u32, height: u32) {
    let (width, height) = (width as usize, height as usize);
    if width == 0 || height == 0 || buffer.len() < width.saturating_mul(height) {
        return;
    }
    const THICKNESS: usize = 3;
    for y in 0..height {
        for x in 0..width {
            if x < THICKNESS || y < THICKNESS || x + THICKNESS >= width || y + THICKNESS >= height {
                buffer[y * width + x] = BUDGET_INDICATOR;
            }
        }
    }
}

pub(crate) fn highlight_primitives(primitives: Vec<Prim>) -> Vec<Prim> {
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

/// The one viewport used to draw and to map clicks, picks and the crosshair.
/// A stored view unusable on this canvas (for example a DXF without DWGVIEW,
/// height 0) is displayed through a LIMITS fit, else a unit view at the
/// origin. This is display-only: the drawing, stored view and Previous are
/// untouched, and ZOOM/PAN commits still validate their own views.
pub(crate) fn viewport_for(drawing: &acad_model::Drawing, width: u32, height: u32) -> Viewport {
    let height = command_line::drawing_height(height).max(1);
    let (view, limits) = (drawing.header.view, drawing.header.limits);
    let limits_height = limits
        .height()
        .max(limits.width() * f64::from(height) / f64::from(width.max(1)));
    let limits_center = acad_model::Point {
        x: limits.xmin / 2.0 + limits.xmax / 2.0,
        y: limits.ymin / 2.0 + limits.ymax / 2.0,
    };
    Viewport::checked_from_view(view.center, view.height, width, height)
        .or_else(|_| Viewport::checked_from_view(limits_center, limits_height, width, height))
        .unwrap_or_else(|_| {
            Viewport::from_view(
                acad_model::Point { x: 0.0, y: 0.0 },
                1.0,
                width.max(1),
                height,
            )
        })
}

/// Temporary SKETCH strokes: unrecorded lines and the pen-down rubber legs
/// in cyan, the erase-pending suffix in red. They are display-only.
pub(crate) const SKETCH_TEMPORARY_RGB: [u8; 3] = [0, 255, 255];
pub(crate) const SKETCH_ERASE_RGB: [u8; 3] = [255, 64, 64];

pub(crate) fn sketch_primitives(
    sketch: &acad_cmd::SketchPreview,
    viewport: &Viewport,
) -> Vec<Prim> {
    let erase_from = sketch.erase_from.unwrap_or(usize::MAX);
    sketch
        .temporary
        .iter()
        .enumerate()
        .map(|(index, segment)| (index >= erase_from, segment))
        .chain(sketch.rubber.iter().map(|segment| (false, segment)))
        .map(|(erasing, [a, b])| Prim::ColoredPolyline {
            points: vec![viewport.to_screen(*a), viewport.to_screen(*b)],
            rgb: if erasing {
                SKETCH_ERASE_RGB
            } else {
                SKETCH_TEMPORARY_RGB
            },
        })
        .collect()
}

pub(crate) fn draw_crosshair(buffer: &mut [u32], width: u32, height: u32, x: f64, y: f64) {
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

pub(crate) fn draw_axis_ticks(
    buffer: &mut [u32],
    width: u32,
    height: u32,
    viewport: &Viewport,
    spacing: f64,
) {
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
                draw(buffer, x, height - 1 - offset.min(height - 1));
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
                draw(buffer, width - 1 - offset.min(width - 1), y);
            }
        }
        y_world += world_step;
        count += 1;
    }
}

/// Owned CPU frame. GUI consumes RGB words; API consumers can request RGBA/PNG.
pub struct Frame {
    pub width: u32,
    pub height: u32,
    pub pixels: Vec<u32>,
    pub diagnostics: Vec<String>,
    /// False when the whole-frame render budget stopped drawing or omitted
    /// the selection highlight; `diagnostics` then says what was skipped.
    pub complete: bool,
}
impl Frame {
    pub fn rgba(&self) -> Vec<u8> {
        self.pixels
            .iter()
            .flat_map(|p| [(p >> 16) as u8, (p >> 8) as u8, *p as u8, 255])
            .collect()
    }
    pub fn png(&self) -> Result<Vec<u8>, String> {
        let mut bytes = Vec::new();
        {
            let mut encoder = png::Encoder::new(&mut bytes, self.width, self.height);
            encoder.set_color(png::ColorType::Rgba);
            encoder.set_depth(png::BitDepth::Eight);
            let mut writer = encoder.write_header().map_err(|e| e.to_string())?;
            writer
                .write_image_data(&self.rgba())
                .map_err(|e| e.to_string())?;
        }
        Ok(bytes)
    }
}
impl Session {
    pub fn frame(&self, width: u32, height: u32) -> Result<Frame, String> {
        // GUI frames may exceed the transport's 4 MP limit on Retina/4K
        // displays. The API validates its own bounds before composition.
        if width == 0 || height == 0 || u64::from(width) * u64::from(height) > 33_554_432 {
            return Err("frame must be nonzero and contain at most 33554432 pixels".into());
        }
        if self.main_menu_active() && !(self.in_file_utilities() && self.report_visible()) {
            let mut pixels = vec![0; width as usize * height as usize];
            draw_text_screen(
                &mut pixels,
                width,
                height,
                &self.main_menu_text().unwrap_or_default(),
            );
            command_line::draw(
                &mut pixels,
                width,
                height,
                self.prompt(),
                &self.input,
                self.display_status(),
            );
            return Ok(Frame {
                width,
                height,
                pixels,
                diagnostics: Vec::new(),
                complete: true,
            });
        }
        if self.report_visible() {
            let mut pixels = vec![0; width as usize * height as usize];
            self.report
                .as_ref()
                .unwrap()
                .draw(&mut pixels, width, height);
            command_line::draw(
                &mut pixels,
                width,
                height,
                self.prompt(),
                &self.input,
                self.display_status(),
            );
            return Ok(Frame {
                width,
                height,
                pixels,
                diagnostics: Vec::new(),
                complete: true,
            });
        }
        let drawing = self.drawing();
        let vp = viewport_for(drawing, width, height);
        // One budget for the drawing and highlight passes of this frame.
        let mut budget = FrameBudget::default();
        let mut rendered = flatten_with_budget(drawing, &vp, &self.libraries, &mut budget);
        let highlight = self.editor.accepts_mouse_selection().then(|| {
            selected_highlight_with_budget(drawing, &self.input, &vp, &self.libraries, &mut budget)
        });
        // Any truncated pass (budget, context failure, per-owner skip) marks
        // the frame incomplete; both passes' diagnostics are capped.
        let budget_status = incomplete_status(&rendered, highlight.as_ref());
        let mut primitives = std::mem::take(&mut rendered.primitives);
        let mut diagnostics = rendered.diagnostics;
        if let Some(highlight) = highlight {
            for diagnostic in highlight.diagnostics {
                if !diagnostics.contains(&diagnostic) {
                    diagnostics.push(diagnostic);
                }
            }
            primitives.extend(highlight.primitives);
        }
        if let Some(sketch) = self.editor.sketch_preview() {
            primitives.extend(sketch_primitives(&sketch, &vp));
        }
        let canvas_height = command_line::drawing_height(height);
        let pm = rasterize(&primitives, width, canvas_height.max(1));
        let mut pixels = vec![0; width as usize * height as usize];
        for (dst, src) in pixels.iter_mut().zip(pm.pixels()) {
            *dst = ((src.red() as u32) << 16) | ((src.green() as u32) << 8) | src.blue() as u32;
        }
        crate::grid::paint(&mut pixels, width, canvas_height, &drawing.header, &vp);
        if let Some(spacing) = self.editor.axis_spacing() {
            draw_axis_ticks(&mut pixels, width, canvas_height, &vp, spacing);
        }
        if let Some((x, y)) = self.crosshair_position(width, height) {
            draw_crosshair(&mut pixels, width, canvas_height, x, y);
        }
        if let Some(menu) = &self.menu {
            let layout = menu_panel::layout_for(menu, self.menu_page, width, canvas_height);
            menu_panel::draw_panel(
                &mut pixels[..width as usize * canvas_height as usize],
                width,
                canvas_height,
                menu,
                self.menu_page,
                &layout,
            );
        }
        // Last on the canvas so neither the menu panel nor the crosshair can
        // hide that this frame is incomplete.
        if budget_status.is_some() {
            draw_budget_border(&mut pixels, width, canvas_height);
        }
        let status = match &budget_status {
            Some(budget) if self.display_status().is_empty() => budget.clone(),
            Some(budget) => format!("{budget}; {}", self.display_status()),
            None => self.display_status().to_owned(),
        };
        command_line::draw(
            &mut pixels,
            width,
            height,
            self.prompt(),
            &self.input,
            &status,
        );
        Ok(Frame {
            width,
            height,
            pixels,
            diagnostics,
            complete: budget_status.is_none(),
        })
    }
}

/// The Main Menu's text screen above the command area; lines that do not
/// fit the canvas are clipped, never wrapped into the command area.
pub(crate) fn draw_text_screen(buffer: &mut [u32], width: u32, height: u32, text: &str) {
    use crate::bitmap::{draw_rect, draw_text, ui_char, GLYPH_HEIGHT, GLYPH_WIDTH};
    const PAD: usize = 8;
    let row = GLYPH_HEIGHT * 2;
    let canvas = command_line::drawing_height(height) as usize;
    let width = width as usize;
    let length = (width * canvas).min(buffer.len());
    let buffer = &mut buffer[..length];
    draw_rect(buffer, width, canvas, 0, 0, width, canvas, 0x0000_0000);
    let columns = width.saturating_sub(2 * PAD) / (GLYPH_WIDTH * 2);
    for (index, line) in text.lines().enumerate() {
        let top = PAD + index * row;
        if top + row > canvas {
            break;
        }
        let line: String = line.chars().map(ui_char).take(columns).collect();
        draw_text(buffer, width, PAD, top, &line, 0x00dd_eeee);
    }
}

pub(crate) fn validate_frame_size(width: u32, height: u32) -> Result<(), String> {
    if width == 0
        || height == 0
        || width > 4096
        || height > 4096
        || u64::from(width) * u64::from(height) > 4_194_304
    {
        Err("frame dimensions must be 1..4096, with at most 4194304 pixels".into())
    } else {
        Ok(())
    }
}

#[cfg(test)]
#[path = "presentation/visibility_tests.rs"]
mod visibility_tests;

#[cfg(test)]
mod ui_glyph_tests {
    use super::*;
    use crate::bitmap::draw_text;

    #[test]
    fn text_screen_preserves_ukrainian_and_clips_by_characters_above_command_area() {
        let (width, height) = (64usize, 100usize); // three cells per text row
        let sentinel = 0x0012_3456;
        let mut actual = vec![sentinel; width * height];
        draw_text_screen(
            &mut actual,
            width as u32,
            height as u32,
            "Ґї’А\nЄ☃\tБ\nНЕ ВМІЩУЄТЬСЯ\nTAIL",
        );
        let canvas = command_line::drawing_height(height as u32) as usize;
        let mut expected = vec![0; width * canvas];
        draw_text(&mut expected, width, 8, 8, "Ґї’", 0x00dd_eeee);
        draw_text(&mut expected, width, 8, 24, "Є??", 0x00dd_eeee);
        // Third row exactly fits; a fourth row must not reach the command area.
        draw_text(&mut expected, width, 8, 40, "НЕ ", 0x00dd_eeee);
        assert_eq!(&actual[..width * canvas], expected);
        assert!(actual[width * canvas..].iter().all(|p| *p == sentinel));
    }

    #[test]
    fn command_area_clips_translated_prompt_input_and_status_by_cells() {
        let (width, height) = (88usize, command_line::HEIGHT as usize); // five cells
        let mut actual = vec![0; width * height];
        command_line::draw(
            &mut actual,
            width as u32,
            height as u32,
            "Команда",
            "ҐїЄА",
            "Помилка☃",
        );
        let mut expected = vec![0; width * height];
        draw_text(&mut expected, width, 4, 4, "ҐїЄА_", 0x00ff_ffff);
        draw_text(&mut expected, width, 4, 24, "Помил", 0x00ff_c080);
        for y in 0..16 {
            for x in 0..5 * 16 {
                assert_eq!(
                    actual[(24 + y) * width + 4 + x],
                    if expected[(24 + y) * width + 4 + x] == 0 {
                        0x0018_1820
                    } else {
                        0x00ff_c080
                    }
                );
                if y < 14 {
                    // final two rows contain the separately drawn caret
                    assert_eq!(
                        actual[(4 + y) * width + 4 + x],
                        if expected[(4 + y) * width + 4 + x] == 0 {
                            0x0018_1820
                        } else {
                            0x00ff_ffff
                        }
                    );
                }
            }
        }
        assert_eq!(actual[18 * width + 4 + 4 * 16], 0x00ff_ffff);
    }
}
