//! Command dispatch: `submit` and `command`, the top-level interpreter loop.

use crate::entity_ops::{
    bare, entity_anchor, item_anchor, normalize_library_name, set_insert_angle,
};
use crate::geometry::{dimension_geometry, polygon_metrics, three_point_arc, trace_quads};
use crate::input_state::{
    ArraySpacingInput, EditCommand, InputState, RepeatDistanceInput, Transform,
};
use crate::parse::{
    color_index, format_measurement, layer_index, number, parse_mode, parse_toggle, point,
    point_from, positive_count, positive_word,
};
use crate::report::{database_listing, hatch_pattern_report, help_report, list_entities};
use crate::selection::{entities_in_window, selectable_count, selected_item_indexes, selection};
use crate::{Editor, Effect, FilesFilter, FilesRequest, HATCH_PATTERNS, MAX_ARRAY_ENTITIES};
use acad_model::{Entity, Extents, Item, Point, UnitFormat, Units};

/// The native screen's fit-to-box aspect ratio for `ZOOM All`/`Extents`-style
/// commands: how many world-space units wide the drawing viewport shows per
/// world-space unit tall, once a box's shorter-relative dimension is made to
/// fill the screen exactly. Recovered empirically under QEMU (Task 5 of the
/// screen-menu-rendering plan) by setting explicit `LIMITS` boxes of several
/// shapes and sizes and reading back `ZOOM A`'s resulting DWG view: every box
/// produced the *same* ratio bit-for-bit regardless of its own size or
/// position (`(center.x - xmin) * 2 / height` when height-constrained,
/// `(center.y - ymin) * 2 / width` when width-constrained), confirming a
/// fixed device aspect rather than a per-box coincidence. This is the native
/// screen's drawing viewport (it excludes the always-present screen-menu
/// panel reserved on the right, and CGA's non-square pixels), not a generic
/// 4:3 or 640/480 constant. As an exact fraction (`Fraction(1.5223311546840958...)`
/// in lowest terms) this is precisely `2795/1836`; `1836 = 2^2 * 3^3 * 17`
/// and `2795 = 5 * 13 * 43` share no common factor, so this is not simply a
/// reduced form of some rounder pixel-count ratio either (no `W`/`H` integer
/// pair smaller than `2795`/`1836` itself reproduces it) — recorded as
/// measured rather than derived from a known device pixel geometry.
const ZOOM_ALL_DEVICE_ASPECT: f64 = 1.522_331_154_684_095_9;

/// The box a fit-to-box ZOOM variant shows: the drawing's `LIMITS`, always
/// expanded to also include `EXTENTS` — unconditionally, with no
/// degenerate-`EXTENTS` special case. An earlier version of this function
/// special-cased a degenerate (zero-area) `EXTENTS` as "no entity has caused
/// a recompute yet" and skipped unioning it in, reasoning that doing so
/// would wrongly pull the shown box toward the origin. That reasoning had
/// it backwards: a QEMU re-check (Task 5's review) with `LIMITS` boxes that
/// do *not* start at `(0, 0)` (e.g. `(5, 7)`-`(29, 25)`) showed native's
/// `ZOOM All` result always includes the origin regardless of `LIMITS`,
/// which only a plain, unconditional union with a *degenerate point at the
/// origin* (`EXTENTS`'s own real default, both on native and in this
/// crate's `Editor::default()`) explains — i.e. the origin isn't excluded
/// from a degenerate `EXTENTS`, it *is* what a degenerate `EXTENTS` at the
/// origin contributes. (Native does not appear to auto-recompute `EXTENTS`
/// from added geometry the same frame it's drawn — a LINE placed well
/// outside `LIMITS` left the stored `EXTENTS` field at `(0,0,0,0)` even
/// after `REGEN` — so the general "entities already moved `EXTENTS` away
/// from the origin, do we still union it in" case remains unverified; only
/// the degenerate-at-origin case this task's own tests exercise is
/// confirmed. A future task should verify the non-degenerate case directly
/// before relying on it.)
fn zoom_all_bounds(limits: Extents, extents: Extents) -> Extents {
    Extents {
        xmin: limits.xmin.min(extents.xmin),
        ymin: limits.ymin.min(extents.ymin),
        xmax: limits.xmax.max(extents.xmax),
        ymax: limits.ymax.max(extents.ymax),
    }
}

/// Fits `bounds` to the native device aspect ratio, anchored at its own
/// lower-left corner (`xmin`, `ymin`) rather than centered: whichever axis
/// has slack (because `bounds`'s own aspect ratio does not match the
/// device's) is extended upward/rightward from that corner, not expanded
/// symmetrically about the box's center. This anchor-at-corner behavior,
/// not simple centering, is what Task 5's QEMU recovery actually found —
/// confirmed because two boxes sharing the same `xmin`/`height` but
/// different `xmax` (so different box-centers) produced the identical
/// resulting view `center.x`.
pub(crate) fn fit_box_to_device(bounds: Extents) -> acad_model::DwgView {
    let (width, height) = if bounds.width() / bounds.height() >= ZOOM_ALL_DEVICE_ASPECT {
        (bounds.width(), bounds.width() / ZOOM_ALL_DEVICE_ASPECT)
    } else {
        (bounds.height() * ZOOM_ALL_DEVICE_ASPECT, bounds.height())
    };
    acad_model::DwgView {
        center: Point {
            x: bounds.xmin + width / 2.0,
            y: bounds.ymin + height / 2.0,
        },
        height,
    }
}

impl Editor {
    /// Submit physical Return or screen-menu GO. Raw script/macro submissions
    /// retain their separate empty-input policy and do not update this history.
    pub fn submit_return(&mut self, input: &str) -> Result<Effect, String> {
        const UNKNOWN: &str = "Unknown command. Type ? for list of commands.";
        let line = input.trim();
        if matches!(self.state, InputState::Command) {
            let command = if line.is_empty() {
                let Some(previous) = self.last_return_command.clone() else {
                    self.status = UNKNOWN.into();
                    return Err(self.status.clone());
                };
                previous
            } else {
                line.to_ascii_uppercase()
            };
            let result = self.submit(&command);
            if result.is_ok() && !line.is_empty() {
                self.last_return_command = Some(command);
            }
            return match result {
                Err(error) if error.starts_with("unknown command: ") => {
                    self.status = UNKNOWN.into();
                    Err(self.status.clone())
                }
                result => result,
            };
        }
        match &self.state {
            InputState::LineStart | InputState::CircleRadius(_) if line.is_empty() => {
                self.state = InputState::Command;
                self.status = "*Invalid*".into();
                return Ok(Effect::Continue);
            }
            InputState::EditSelection(EditCommand::Erase) if line.is_empty() => {
                self.state = InputState::Command;
                self.status.clear();
                return Ok(Effect::Continue);
            }
            InputState::MenuFile if line.is_empty() => {
                self.state = InputState::Command;
                self.status.clear();
                return Ok(Effect::UnloadMenu);
            }
            InputState::RepeatColumns if positive_word(line, "columns").is_err() => {
                self.state = InputState::Command;
                self.status = "*Invalid*".into();
                return Ok(Effect::Continue);
            }
            _ => {}
        }
        let single_erase = matches!(self.state, InputState::EditSelection(EditCommand::Erase))
            && selection(line, selectable_count(&self.drawing)).is_ok_and(|ids| ids.len() == 1);
        let result = self.submit(line);
        if single_erase && result.is_ok() {
            self.status = "1 selected, 1 found.".into();
        }
        result
    }

    /// Submit one complete line of keyboard input. Coordinates use AutoCAD's
    /// `x,y`, `@dx,dy`, or `@distance<angle` notation where a prior point exists.
    /// A `LINE` remains active until an empty line is entered.
    pub fn submit(&mut self, input: &str) -> Result<Effect, String> {
        self.status.clear();
        let line = input.trim();
        match self.state.clone() {
            InputState::Command => self.command(line),
            InputState::LineStart => {
                if line.is_empty() {
                    return self.cancel();
                }
                let p = point(line)?;
                self.state = InputState::LineNext(p);
                Ok(Effect::Continue)
            }
            InputState::LineNext(previous) => {
                if line.is_empty() {
                    self.state = InputState::Command;
                    return Ok(Effect::Continue);
                }
                let next = point_from(line, previous)?;
                self.add(Entity::Line {
                    start: previous,
                    end: next,
                });
                self.state = InputState::LineNext(next);
                Ok(Effect::Continue)
            }
            InputState::CircleCenter => {
                let center = point(line)?;
                self.state = InputState::CircleRadius(center);
                Ok(Effect::Continue)
            }
            InputState::CircleRadius(center) => {
                let radius = number(line)?;
                if radius <= 0.0 {
                    return Err("radius must be positive".into());
                }
                self.add(Entity::Circle { center, radius });
                self.state = InputState::Command;
                Ok(Effect::Continue)
            }
            InputState::Point => {
                let origin = point(line)?;
                self.add(Entity::Point { origin });
                self.state = InputState::Command;
                Ok(Effect::Continue)
            }
            InputState::ArcStart => {
                let start = point(line)?;
                self.state = InputState::ArcMiddle(start);
                Ok(Effect::Continue)
            }
            InputState::ArcMiddle(start) => {
                let middle = point_from(line, start)?;
                self.state = InputState::ArcEnd(start, middle);
                Ok(Effect::Continue)
            }
            InputState::ArcEnd(start, middle) => {
                let end = point_from(line, middle)?;
                let (center, radius, start_deg, end_deg) = three_point_arc(start, middle, end)?;
                self.add(Entity::Arc {
                    center,
                    radius,
                    start_deg,
                    end_deg,
                });
                self.state = InputState::Command;
                Ok(Effect::Continue)
            }
            InputState::LoadLibrary => {
                if line.is_empty() {
                    return Err("shape library name cannot be empty".into());
                }
                let library = normalize_library_name(line);
                if !self.shape_libraries.contains_key(&library) {
                    return Err(format!("shape library is not available: {line}"));
                }
                self.add(Entity::Load {
                    name: line.to_owned(),
                });
                self.active_shape_library = Some(library);
                self.state = InputState::Command;
                Ok(Effect::Continue)
            }
            InputState::ShapeName => {
                let Some(library) = self.active_shape_library.as_ref() else {
                    return Err("SHAPE requires a library loaded with LOAD".into());
                };
                let Some(number) = self
                    .shape_libraries
                    .get(library)
                    .and_then(|shapes| shapes.get(&line.to_ascii_uppercase()))
                    .copied()
                else {
                    return Err(format!("unknown shape name: {line}"));
                };
                self.state = InputState::ShapeOrigin(number);
                Ok(Effect::Continue)
            }
            InputState::ShapeOrigin(shape_id) => {
                self.state = InputState::ShapeHeight(shape_id, point(line)?);
                Ok(Effect::Continue)
            }
            InputState::ShapeHeight(shape_id, origin) => {
                let height = number(line)?;
                if height <= 0.0 {
                    return Err("shape height must be positive".into());
                }
                self.state = InputState::ShapeRotation(shape_id, origin, height);
                Ok(Effect::Continue)
            }
            InputState::ShapeRotation(shape_id, origin, height) => {
                let rotation_deg = number(line)?;
                self.add(Entity::Shape {
                    origin,
                    height,
                    rotation_deg,
                    number: shape_id,
                });
                self.state = InputState::Command;
                Ok(Effect::Continue)
            }
            InputState::TextOrigin => {
                let origin = point(line)?;
                self.state = InputState::TextHeight(origin);
                Ok(Effect::Continue)
            }
            InputState::TextHeight(origin) => {
                let height = number(line)?;
                if height <= 0.0 {
                    return Err("text height must be positive".into());
                }
                self.state = InputState::TextRotation(origin, height);
                Ok(Effect::Continue)
            }
            InputState::TextRotation(origin, height) => {
                let rotation = number(line)?;
                self.state = InputState::TextValue(origin, height, rotation);
                Ok(Effect::Continue)
            }
            InputState::TextValue(origin, height, rotation) => {
                if input.is_empty() {
                    return Err("text value cannot be empty".into());
                }
                self.add(Entity::Text {
                    origin,
                    height,
                    rotation_deg: rotation,
                    value: input.to_owned(),
                });
                self.state = InputState::Command;
                Ok(Effect::Continue)
            }
            InputState::InsertName => {
                if line.is_empty() {
                    return Err("block name cannot be empty".into());
                }
                let (explode, requested) = match line.strip_prefix('*') {
                    Some(name) => (true, name),
                    None => (false, line),
                };
                let Some(block) = self
                    .drawing
                    .blocks()
                    .find(|block| block.name.eq_ignore_ascii_case(requested))
                else {
                    return Err(format!("unknown block: {requested}"));
                };
                let name = block.name.clone();
                self.state = InputState::InsertOrigin(name, explode);
                Ok(Effect::Continue)
            }
            InputState::InsertOrigin(name, explode) => {
                let origin = point(line)?;
                if explode {
                    self.explode_block(&name, origin)?;
                    self.state = InputState::Command;
                } else {
                    self.state = InputState::InsertXScale(name, origin);
                }
                Ok(Effect::Continue)
            }
            InputState::InsertXScale(name, origin) => {
                if line.contains(',') || line.starts_with('@') {
                    let opposite = point_from(line, origin)?;
                    let x_scale = opposite.x - origin.x;
                    let y_scale = opposite.y - origin.y;
                    if x_scale == 0.0 || y_scale == 0.0 {
                        return Err("INSERT scale box needs nonzero width and height".into());
                    }
                    self.state = InputState::InsertRotation(name, origin, x_scale, y_scale);
                    return Ok(Effect::Continue);
                }
                let scale = if line.is_empty() { 1.0 } else { number(line)? };
                if scale == 0.0 {
                    return Err("X scale must be nonzero".into());
                }
                self.state = InputState::InsertYScale(name, origin, scale);
                Ok(Effect::Continue)
            }
            InputState::InsertYScale(name, origin, x_scale) => {
                let y_scale = if line.is_empty() {
                    x_scale
                } else {
                    number(line)?
                };
                if y_scale == 0.0 {
                    return Err("Y scale must be nonzero".into());
                }
                self.state = InputState::InsertRotation(name, origin, x_scale, y_scale);
                Ok(Effect::Continue)
            }
            InputState::InsertRotation(name, origin, x_scale, y_scale) => {
                let rotation_deg = if line.is_empty() { 0.0 } else { number(line)? };
                self.add(Entity::Insert {
                    origin,
                    x_scale,
                    y_scale,
                    rotation_deg,
                    name,
                });
                self.state = InputState::Command;
                Ok(Effect::Continue)
            }
            InputState::BlockName => {
                if line.is_empty() || !line.bytes().all(|byte| byte.is_ascii_graphic()) {
                    return Err("block name must contain printable ASCII characters".into());
                }
                if self
                    .drawing
                    .blocks()
                    .any(|block| block.name.eq_ignore_ascii_case(line))
                {
                    return Err(format!("block already exists: {line}"));
                }
                self.state = InputState::BlockBase(line.to_ascii_uppercase());
                Ok(Effect::Continue)
            }
            InputState::BlockBase(name) => {
                self.state = InputState::BlockSelection(name, point(line)?);
                Ok(Effect::Continue)
            }
            InputState::BlockSelection(name, base) => {
                let count = selectable_count(&self.drawing);
                let ids = if line.eq_ignore_ascii_case("LAST") {
                    if count == 0 {
                        return Err("there are no selectable entities".into());
                    }
                    vec![count]
                } else {
                    selection(line, count)?
                };
                self.create_block(name, base, &ids);
                self.state = InputState::Command;
                Ok(Effect::Continue)
            }
            InputState::SavePath => {
                if line.is_empty() {
                    return Err("output path cannot be empty".into());
                }
                self.state = InputState::Command;
                Ok(Effect::Save(line.to_owned()))
            }
            InputState::WblockPath => {
                if line.trim().is_empty() {
                    return Err("WBLOCK output file name cannot be empty".into());
                }
                let path = std::path::Path::new(line.trim());
                if path
                    .extension()
                    .is_some_and(|ext| !ext.eq_ignore_ascii_case("dwg"))
                {
                    return Err("WBLOCK output must be a DWG file".into());
                }
                let mut path = path.to_path_buf();
                if path.extension().is_none() {
                    path.set_extension("DWG");
                }
                self.state = InputState::WblockName(path.to_string_lossy().into_owned());
                Ok(Effect::Continue)
            }
            InputState::WblockName(path) => {
                if line.trim().is_empty() {
                    self.state = InputState::WblockBase(path);
                    return Ok(Effect::Continue);
                }
                let drawing = if line.trim() == "*" {
                    self.wblock_entire_drawing()
                } else {
                    self.wblock_named_block(line.trim())?
                };
                self.state = InputState::Command;
                Ok(Effect::SaveDrawing(path, Box::new(drawing)))
            }
            InputState::WblockBase(path) => {
                self.state = InputState::WblockSelection(path, point(line)?);
                Ok(Effect::Continue)
            }
            InputState::WblockSelection(path, base) => {
                let ids = selection(line, selectable_count(&self.drawing))?;
                let drawing = self.wblock_selected_entities(&ids, base);
                self.state = InputState::Command;
                Ok(Effect::SaveDrawing(path, Box::new(drawing)))
            }
            InputState::HelpCommand => {
                let report = help_report(line)?;
                self.status = if line.trim().is_empty() {
                    "Command list".into()
                } else {
                    format!("Help for {}", line.trim().to_ascii_uppercase())
                };
                self.state = InputState::Command;
                Ok(Effect::Report(report))
            }
            InputState::MenuFile => {
                if line.trim().is_empty() {
                    return self.cancel();
                }
                self.state = InputState::Command;
                Ok(Effect::LoadMenu(line.trim().to_owned()))
            }
            InputState::FilesMenu => match line.trim() {
                "0" => self.cancel(),
                "1" => {
                    self.state = InputState::FilesDrive(FilesFilter::Drawings);
                    Ok(Effect::Continue)
                }
                "2" => {
                    self.state = InputState::FilesDrive(FilesFilter::Menus);
                    Ok(Effect::Continue)
                }
                "3" => {
                    self.state = InputState::FilesDrive(FilesFilter::Shapes);
                    Ok(Effect::Continue)
                }
                "4" => {
                    self.state = InputState::FilesDrive(FilesFilter::Patterns);
                    Ok(Effect::Continue)
                }
                "5" => {
                    self.state = InputState::FilesListSpecification;
                    Ok(Effect::Continue)
                }
                "6" => {
                    self.state = InputState::FilesDeleteSpecification;
                    Ok(Effect::Continue)
                }
                "7" => {
                    self.state = InputState::FilesRenameSource;
                    Ok(Effect::Continue)
                }
                _ => Err("FILES selection must be 0 through 7".into()),
            },
            InputState::FilesDrive(filter) => {
                let drive = line.trim();
                if drive.len() != 1 || !drive.as_bytes()[0].is_ascii_alphabetic() {
                    return Err("FILES drive must be a single letter".into());
                }
                self.state = InputState::FilesMenu;
                Ok(Effect::Files(FilesRequest::ListDrive {
                    filter,
                    drive: drive.as_bytes()[0].to_ascii_uppercase() as char,
                }))
            }
            InputState::FilesListSpecification => {
                if line.trim().is_empty() {
                    self.state = InputState::FilesMenu;
                    return Ok(Effect::Continue);
                }
                self.state = InputState::FilesMenu;
                Ok(Effect::Files(FilesRequest::ListSpecification(
                    line.trim().to_owned(),
                )))
            }
            InputState::FilesDeleteSpecification => {
                if line.trim().is_empty() {
                    self.state = InputState::FilesMenu;
                    return Ok(Effect::Continue);
                }
                self.state = InputState::FilesMenu;
                Ok(Effect::Files(FilesRequest::Delete(line.trim().to_owned())))
            }
            InputState::FilesRenameSource => {
                if line.trim().is_empty() {
                    self.state = InputState::FilesMenu;
                    return Ok(Effect::Continue);
                }
                self.state = InputState::FilesRenameDestination(line.trim().to_owned());
                Ok(Effect::Continue)
            }
            InputState::FilesRenameDestination(source) => {
                if line.trim().is_empty() {
                    self.state = InputState::FilesMenu;
                    return Ok(Effect::Continue);
                }
                self.state = InputState::FilesMenu;
                Ok(Effect::Files(FilesRequest::Rename {
                    source,
                    destination: line.trim().to_owned(),
                }))
            }
            InputState::DimFirstExtension => {
                self.state = InputState::DimIntersection(point(line)?);
                Ok(Effect::Continue)
            }
            InputState::DimIntersection(first) => {
                self.state = InputState::DimSecondExtension(first, point_from(line, first)?);
                Ok(Effect::Continue)
            }
            InputState::DimSecondExtension(first, intersection) => {
                self.state = InputState::DimText(first, intersection, point_from(line, first)?);
                Ok(Effect::Continue)
            }
            InputState::DimText(first, intersection, second) => {
                let text = if line.trim().is_empty() {
                    None
                } else {
                    Some(line.trim())
                };
                let entities = dimension_geometry(
                    first,
                    intersection,
                    second,
                    text,
                    self.drawing.header.text_size,
                    self.drawing.header.units,
                )?;
                self.save_undo();
                for entity in entities {
                    self.drawing.items.push(Item::Entity(Entity::OnLayer {
                        layer: self.drawing.header.current_layer,
                        entity: Box::new(entity),
                    }));
                }
                self.state = InputState::Command;
                self.refresh_after_edit();
                Ok(Effect::Continue)
            }
            InputState::HatchPattern => {
                let pattern = line.trim().to_ascii_uppercase();
                if pattern == "?" {
                    self.state = InputState::Command;
                    self.status = "HATCH pattern list".into();
                    return Ok(Effect::Report(hatch_pattern_report()));
                }
                if !HATCH_PATTERNS.iter().any(|(name, _)| *name == pattern) {
                    self.state = InputState::Command;
                    return Err(format!("unknown HATCH pattern: {pattern}"));
                }
                self.state = InputState::HatchScale(pattern);
                Ok(Effect::Continue)
            }
            InputState::HatchScale(pattern) => {
                let scale = if line.trim().is_empty() {
                    1.0
                } else {
                    number(line)?
                };
                if !scale.is_finite() || scale <= 0.0 {
                    return Err("HATCH pattern scale must be positive and finite".into());
                }
                self.state = InputState::HatchAngle(pattern, scale);
                Ok(Effect::Continue)
            }
            InputState::HatchAngle(pattern, scale) => {
                let angle = if line.trim().is_empty() {
                    0.0
                } else {
                    number(line)?
                };
                if !angle.is_finite() {
                    return Err("HATCH pattern angle must be finite".into());
                }
                self.state = InputState::HatchSelection(pattern, scale, angle);
                Ok(Effect::Continue)
            }
            InputState::HatchSelection(_, _, _) => {
                let InputState::HatchSelection(pattern, scale, angle) = self.state.clone() else {
                    unreachable!()
                };
                if line.trim().eq_ignore_ascii_case("W")
                    || line.trim().eq_ignore_ascii_case("WINDOW")
                {
                    self.state = InputState::HatchWindowFirst(pattern, scale, angle);
                    return Ok(Effect::Continue);
                }
                let ids = selection(line, selectable_count(&self.drawing))?;
                self.add_hatch(&pattern, scale, angle, &ids)?;
                self.state = InputState::Command;
                self.refresh_after_edit();
                Ok(Effect::Continue)
            }
            InputState::HatchWindowFirst(pattern, scale, angle) => {
                self.state = InputState::HatchWindowSecond(pattern, scale, angle, point(line)?);
                Ok(Effect::Continue)
            }
            InputState::HatchWindowSecond(pattern, scale, angle, first) => {
                let second = point(line)?;
                let ids = entities_in_window(&self.drawing, first, second);
                if ids.is_empty() {
                    return Err("HATCH window selected no boundary objects".into());
                }
                self.add_hatch(&pattern, scale, angle, &ids)?;
                self.state = InputState::Command;
                self.refresh_after_edit();
                Ok(Effect::Continue)
            }
            InputState::SketchIncrement => {
                let increment = number(line)?;
                if increment <= 0.0 || !increment.is_finite() {
                    return Err("SKETCH record increment must be positive".into());
                }
                self.state = InputState::Command;
                Err("SKETCH requires a digitizer input device after the record increment".into())
            }
            InputState::Delay => {
                let duration = number(line)?;
                if duration < 0.0 || !duration.is_finite() {
                    return Err("DELAY duration must be non-negative".into());
                }
                // The native command only affects command-script pacing. The
                // interactive editor has no queued script executor, so a
                // validated duration deliberately leaves the drawing intact.
                self.state = InputState::Command;
                Ok(Effect::Continue)
            }
            InputState::UnitsFormat => {
                let format = if line.is_empty() {
                    self.drawing.header.units.format
                } else {
                    match line.trim() {
                        "1" => UnitFormat::Scientific,
                        "2" => UnitFormat::Decimal,
                        "3" => UnitFormat::Engineering,
                        "4" => UnitFormat::Architectural,
                        _ => return Err("UNITS choice must be 1, 2, 3, or 4".into()),
                    }
                };
                self.state = InputState::UnitsPrecision(format);
                Ok(Effect::Continue)
            }
            InputState::UnitsPrecision(format) => {
                let precision = if line.is_empty() {
                    self.drawing.header.units.precision
                } else {
                    line.trim()
                        .parse::<u16>()
                        .map_err(|_| "UNITS precision must be a whole number")?
                };
                match format {
                    UnitFormat::Architectural
                        if !matches!(precision, 1 | 2 | 4 | 8 | 16 | 32 | 64) =>
                    {
                        return Err("UNITS denominator must be 1, 2, 4, 8, 16, 32, or 64".into());
                    }
                    UnitFormat::Architectural => {}
                    _ if precision > 8 => {
                        return Err("UNITS precision must be from 0 to 8".into());
                    }
                    _ => {}
                }
                let units = Units { format, precision };
                if self.drawing.header.units != units {
                    self.save_undo();
                    self.drawing.header.units = units;
                }
                self.state = InputState::Command;
                Ok(Effect::Continue)
            }
            InputState::Base => {
                self.drawing.header.base = point(line)?;
                self.state = InputState::Command;
                Ok(Effect::Continue)
            }
            InputState::Axis => {
                match line.to_ascii_uppercase().as_str() {
                    "OFF" | "NO" => self.drawing.header.axis.on = false,
                    "ON" | "YES" => self.drawing.header.axis.on = true,
                    _ => {
                        let uppercase = line.to_ascii_uppercase();
                        let (spacing_text, snap_multiple) = match uppercase.strip_suffix('X') {
                            Some(number) => (number.trim(), true),
                            None => (line, false),
                        };
                        let spacing = number(spacing_text)?
                            * if snap_multiple {
                                self.drawing.header.snap.spacing
                            } else {
                                1.0
                            };
                        if spacing <= 0.0 || !spacing.is_finite() {
                            return Err("axis tick spacing must be positive".into());
                        }
                        self.drawing.header.axis.on = true;
                        self.drawing.header.axis.spacing = spacing;
                    }
                }
                self.state = InputState::Command;
                Ok(Effect::Continue)
            }
            InputState::Snap | InputState::Grid => {
                let snap = matches!(&self.state, InputState::Snap);
                let previous = if snap {
                    self.drawing.header.snap
                } else {
                    self.drawing.header.grid
                };
                let mode = parse_mode(line, previous)?;
                if snap {
                    self.drawing.header.snap = mode;
                } else {
                    self.drawing.header.grid = mode;
                }
                self.state = InputState::Command;
                Ok(Effect::Continue)
            }
            InputState::Ortho | InputState::Fill => {
                let enabled = parse_toggle(line)?;
                if matches!(&self.state, InputState::Ortho) {
                    self.drawing.header.ortho = enabled;
                } else {
                    self.drawing.header.fill = enabled;
                }
                self.state = InputState::Command;
                Ok(Effect::Continue)
            }
            InputState::LimitsMin => {
                self.state = InputState::LimitsMax(point(line)?);
                Ok(Effect::Continue)
            }
            InputState::LimitsMax(min) => {
                let max = point_from(line, min)?;
                if max.x <= min.x || max.y <= min.y {
                    return Err("LIMITS upper-right must exceed lower-left".into());
                }
                self.drawing.header.limits = Extents {
                    xmin: min.x,
                    ymin: min.y,
                    xmax: max.x,
                    ymax: max.y,
                };
                self.state = InputState::Command;
                Ok(Effect::Continue)
            }
            InputState::Layer => {
                let layer = layer_index(line)?;
                self.ensure_layer(layer);
                self.drawing.header.current_layer = layer;
                self.state = InputState::Command;
                Ok(Effect::Continue)
            }
            InputState::ColorValue => {
                let color = color_index(line)?;
                let layer = self.drawing.header.current_layer;
                self.drawing.header.layers.insert(layer, color);
                self.state = InputState::ColorTargetLayer;
                Ok(Effect::Continue)
            }
            InputState::ColorTargetLayer => {
                if line.is_empty() {
                    self.state = InputState::Command;
                } else {
                    let layer = layer_index(line)?;
                    self.ensure_layer(layer);
                    self.drawing.header.current_layer = layer;
                    self.state = InputState::Command;
                }
                Ok(Effect::Continue)
            }
            InputState::Zoom => {
                if matches!(line.to_ascii_uppercase().as_str(), "P" | "PREVIOUS") {
                    if let Some(previous) = self.previous_view {
                        self.previous_view = Some(self.drawing.header.view);
                        self.drawing.header.view = previous;
                    }
                    self.state = InputState::Command;
                    return Ok(Effect::Continue);
                }
                match line.to_ascii_uppercase().as_str() {
                    "E" | "EXTENTS" => {
                        let bounds = self.drawing.header.extents;
                        let center = Point {
                            x: (bounds.xmin + bounds.xmax) / 2.0,
                            y: (bounds.ymin + bounds.ymax) / 2.0,
                        };
                        let height = bounds.height().max(bounds.width());
                        if !height.is_finite() || height <= 0.0 {
                            return Err("drawing extents have no positive area".into());
                        }
                        self.set_view(center, height);
                        self.state = InputState::Command;
                        return Ok(Effect::Continue);
                    }
                    "C" | "CENTER" => {
                        self.state = InputState::ZoomCenter;
                        return Ok(Effect::Continue);
                    }
                    "W" | "WINDOW" => {
                        self.state = InputState::ZoomWindowMin;
                        return Ok(Effect::Continue);
                    }
                    "A" | "ALL" => {
                        let bounds = zoom_all_bounds(
                            self.drawing.header.limits,
                            self.drawing.header.extents,
                        );
                        if bounds.is_degenerate() {
                            return Err("LIMITS and EXTENTS have no positive area".into());
                        }
                        let view = fit_box_to_device(bounds);
                        self.set_view(view.center, view.height);
                        self.state = InputState::Command;
                        return Ok(Effect::Continue);
                    }
                    _ => {}
                }
                let factor = number(line)?;
                if factor <= 0.0 {
                    return Err("zoom factor must be positive".into());
                }
                self.previous_view = Some(self.drawing.header.view);
                self.drawing.header.view.height /= factor;
                self.state = InputState::Command;
                Ok(Effect::Continue)
            }
            InputState::ZoomCenter => {
                self.state = InputState::ZoomCenterHeight(point(line)?);
                Ok(Effect::Continue)
            }
            InputState::ZoomCenterHeight(center) => {
                let height = number(line)?;
                if height <= 0.0 {
                    return Err("view height must be positive".into());
                }
                self.set_view(center, height);
                self.state = InputState::Command;
                Ok(Effect::Continue)
            }
            InputState::ZoomWindowMin => {
                self.state = InputState::ZoomWindowMax(point(line)?);
                Ok(Effect::Continue)
            }
            InputState::ZoomWindowMax(min) => {
                let max = point_from(line, min)?;
                let width = max.x - min.x;
                let height = max.y - min.y;
                if !width.is_finite() || !height.is_finite() || width <= 0.0 || height <= 0.0 {
                    return Err("zoom window must have positive width and height".into());
                }
                self.set_view(
                    Point {
                        x: (min.x + max.x) / 2.0,
                        y: (min.y + max.y) / 2.0,
                    },
                    width.max(height),
                );
                self.state = InputState::Command;
                Ok(Effect::Continue)
            }
            InputState::PanCenter => {
                let center = point(line)?;
                self.set_view(center, self.drawing.header.view.height);
                self.state = InputState::Command;
                Ok(Effect::Continue)
            }
            InputState::EditSelection(command) => {
                let ids = selection(line, selectable_count(&self.drawing))?;
                if command == EditCommand::Erase {
                    self.erase(&ids);
                    self.state = InputState::Command;
                    return Ok(Effect::Continue);
                }
                debug_assert!(matches!(command, EditCommand::Rotate | EditCommand::Scale));
                self.state = InputState::EditBase(command, ids);
                Ok(Effect::Continue)
            }
            InputState::Displacement(command) => {
                self.state = InputState::SecondPoint(command, point(line)?);
                Ok(Effect::Continue)
            }
            InputState::SecondPoint(command, first) => {
                let delta = if line.is_empty() {
                    first
                } else {
                    let second = point_from(line, first)?;
                    Point {
                        x: second.x - first.x,
                        y: second.y - first.y,
                    }
                };
                self.state = InputState::DisplacedSelection(command, delta);
                Ok(Effect::Continue)
            }
            InputState::DisplacedSelection(command, delta) => {
                let ids = selection(line, selectable_count(&self.drawing))?;
                self.transform(
                    &ids,
                    Transform::Translate(delta),
                    command == EditCommand::Copy,
                );
                self.state = InputState::Command;
                Ok(Effect::Continue)
            }
            InputState::EditBase(command, ids) => {
                self.state = InputState::EditValue(command, ids, point(line)?);
                Ok(Effect::Continue)
            }
            InputState::EditValue(command, ids, base) => {
                let transform = match command {
                    EditCommand::Move | EditCommand::Copy => {
                        unreachable!("MOVE and COPY use displacement prompts")
                    }
                    EditCommand::Rotate => Transform::Rotate {
                        base,
                        degrees: number(line)?,
                    },
                    EditCommand::Scale => {
                        let factor = number(line)?;
                        if factor <= 0.0 {
                            return Err("scale factor must be positive".into());
                        }
                        Transform::Scale { base, factor }
                    }
                    EditCommand::Erase => unreachable!("ERASE has no transform prompts"),
                };
                self.transform(&ids, transform, command == EditCommand::Copy);
                self.state = InputState::Command;
                Ok(Effect::Continue)
            }
            InputState::ArraySelection => {
                let ids = selection(line, selectable_count(&self.drawing))?;
                self.state = InputState::ArrayMode(ids);
                Ok(Effect::Continue)
            }
            InputState::ArrayMode(ids) => {
                if line.eq_ignore_ascii_case("R") || line.eq_ignore_ascii_case("RECTANGULAR") {
                    self.state = InputState::ArrayRows(ids);
                } else if line.eq_ignore_ascii_case("C") || line.eq_ignore_ascii_case("CIRCULAR") {
                    self.state = InputState::ArrayCircularCenter(ids);
                } else {
                    return Err("array mode must be rectangular (R) or circular (C)".into());
                }
                Ok(Effect::Continue)
            }
            InputState::ArrayCircularCenter(ids) => {
                self.state = InputState::ArrayCircularAngle(ids, point(line)?);
                Ok(Effect::Continue)
            }
            InputState::ArrayCircularAngle(ids, center) => {
                self.state = InputState::ArrayCircularItems(ids, center, number(line)?);
                Ok(Effect::Continue)
            }
            InputState::ArrayCircularItems(ids, center, angle) => {
                let count = positive_count(line, "array item count")?;
                if !(angle * count.saturating_sub(1) as f64).is_finite() {
                    return Err("circular array angle is too large".into());
                }
                if count.saturating_mul(ids.len()) > MAX_ARRAY_ENTITIES {
                    return Err(format!(
                        "array exceeds the {MAX_ARRAY_ENTITIES}-entity implementation limit"
                    ));
                }
                self.circular_array(&ids, center, angle, count);
                self.state = InputState::Command;
                Ok(Effect::Continue)
            }
            InputState::ArrayRows(ids) => {
                let rows = positive_count(line, "row count")?;
                self.state = InputState::ArrayColumns(ids, rows);
                Ok(Effect::Continue)
            }
            InputState::ArrayColumns(ids, rows) => {
                let columns = positive_count(line, "column count")?;
                let total = rows
                    .checked_mul(columns)
                    .ok_or_else(|| "array dimensions are too large".to_owned())?;
                if total.saturating_mul(ids.len()) > MAX_ARRAY_ENTITIES {
                    return Err(format!(
                        "array exceeds the {MAX_ARRAY_ENTITIES}-entity implementation limit"
                    ));
                }
                self.state = InputState::ArrayRowSpacing(ids, rows, columns);
                Ok(Effect::Continue)
            }
            InputState::ArrayRowSpacing(ids, rows, columns) => {
                let row_spacing = match number(line) {
                    Ok(spacing) => ArraySpacingInput::Number(spacing),
                    Err(_) => {
                        let selected = selected_item_indexes(&self.drawing, &ids);
                        let anchor = self
                            .drawing
                            .items
                            .iter()
                            .enumerate()
                            .filter(|(index, _)| selected.contains(&(index + 1)))
                            .find_map(|(_, item)| item_anchor(item))
                            .ok_or("ARRAY needs one or more ordinary entities")?;
                        ArraySpacingInput::Point(point_from(line, anchor)?)
                    }
                };
                self.state = InputState::ArrayColumnSpacing(ids, rows, columns, row_spacing);
                Ok(Effect::Continue)
            }
            InputState::ArrayColumnSpacing(ids, rows, columns, row_input) => {
                let (row_spacing, column_spacing) = match row_input {
                    ArraySpacingInput::Number(row_spacing) => (row_spacing, number(line)?),
                    ArraySpacingInput::Point(first) => {
                        let second = point_from(line, first)?;
                        (second.y - first.y, second.x - first.x)
                    }
                };
                self.array(&ids, rows, columns, row_spacing, column_spacing);
                self.state = InputState::Command;
                Ok(Effect::Continue)
            }
            InputState::ChangeSelection => {
                let ids = selection(line, selectable_count(&self.drawing))?;
                self.state = InputState::ChangeIntersection(ids);
                Ok(Effect::Continue)
            }
            InputState::ChangeIntersection(ids) if line.eq_ignore_ascii_case("L") => {
                self.state = InputState::ChangeLayer(ids);
                Ok(Effect::Continue)
            }
            InputState::ChangeIntersection(ids) => {
                let selected = selected_item_indexes(&self.drawing, &ids);
                let base = self
                    .drawing
                    .items
                    .iter()
                    .enumerate()
                    .filter(|(index, _)| selected.contains(&(index + 1)))
                    .find_map(|(_, item)| item_anchor(item))
                    .ok_or("CHANGE needs one or more entities with a point")?;
                let point = point_from(line, base)?;
                self.change_point(&ids, point)?;
                let selected = selected_item_indexes(&self.drawing, &ids);
                let has_insert = self.drawing.items.iter().enumerate().any(|(index, item)| {
                    selected.contains(&(index + 1))
                        && matches!(item, Item::Entity(entity) if matches!(bare(entity), Entity::Insert { .. }))
                });
                self.state = if has_insert {
                    InputState::ChangeInsertAngle(ids)
                } else {
                    InputState::Command
                };
                Ok(Effect::Continue)
            }
            InputState::ChangeLayer(ids) => {
                let layer = layer_index(line)?;
                self.change_layer(&ids, layer);
                self.state = InputState::Command;
                Ok(Effect::Continue)
            }
            InputState::ChangeInsertAngle(ids) => {
                if !line.is_empty() {
                    let angle = number(line)?;
                    let selected = selected_item_indexes(&self.drawing, &ids);
                    for (index, item) in self.drawing.items.iter_mut().enumerate() {
                        if selected.contains(&(index + 1)) {
                            if let Item::Entity(entity) = item {
                                set_insert_angle(entity, angle);
                            }
                        }
                    }
                    self.refresh_after_edit();
                }
                self.state = InputState::Command;
                Ok(Effect::Continue)
            }
            InputState::FilletSelection => {
                let ids = selection(line, selectable_count(&self.drawing))?;
                if ids.len() != 2 {
                    return Err("FILLET requires exactly two line entities".into());
                }
                self.state = InputState::FilletRadius(ids);
                Ok(Effect::Continue)
            }
            InputState::FilletRadius(ids) => {
                let radius = number(line)?;
                if radius <= 0.0 {
                    return Err("fillet radius must be positive".into());
                }
                self.fillet(&ids, radius)?;
                self.state = InputState::Command;
                Ok(Effect::Continue)
            }
            InputState::BreakSelection => {
                let ids = selection(line, selectable_count(&self.drawing))?;
                if ids.len() != 1 {
                    return Err("BREAK requires exactly one entity".into());
                }
                self.state = InputState::BreakFirstPoint(ids);
                Ok(Effect::Continue)
            }
            InputState::BreakFirstPoint(ids) => {
                self.state = InputState::BreakSecondPoint(ids, point(line)?);
                Ok(Effect::Continue)
            }
            InputState::BreakSecondPoint(ids, first) => {
                self.break_entity(&ids, first, point_from(line, first)?)?;
                self.state = InputState::Command;
                Ok(Effect::Continue)
            }
            InputState::DistanceFirstPoint => {
                self.state = InputState::DistanceSecondPoint(point(line)?);
                Ok(Effect::Continue)
            }
            InputState::DistanceSecondPoint(first) => {
                let second = point_from(line, first)?;
                let dx = second.x - first.x;
                let dy = second.y - first.y;
                let distance = dx.hypot(dy);
                self.status = format!(
                    "Distance={}",
                    format_measurement(distance, self.drawing.header.units)
                );
                self.state = InputState::Command;
                Ok(Effect::Continue)
            }
            InputState::IdPoint => {
                let location = point(line)?;
                self.status = format!(
                    "X = {}    Y = {}",
                    format_measurement(location.x, self.drawing.header.units),
                    format_measurement(location.y, self.drawing.header.units)
                );
                self.state = InputState::Command;
                Ok(Effect::Continue)
            }
            InputState::SolidFirstPoint => {
                if line.is_empty() {
                    return self.cancel();
                }
                self.state = InputState::SolidSecondPoint(point(line)?);
                Ok(Effect::Continue)
            }
            InputState::SolidSecondPoint(p1) => {
                if line.is_empty() {
                    return self.cancel();
                }
                self.state = InputState::SolidThirdPoint(p1, point_from(line, p1)?);
                Ok(Effect::Continue)
            }
            InputState::SolidThirdPoint(p1, p2) => {
                if line.is_empty() {
                    return self.cancel();
                }
                self.state = InputState::SolidFourthPoint(p1, p2, point_from(line, p2)?);
                Ok(Effect::Continue)
            }
            InputState::SolidFourthPoint(p1, p2, p3) => {
                let p4 = point_from(line, p3)?;
                self.add(Entity::Solid { p1, p2, p3, p4 });
                self.state = InputState::SolidThirdPoint(p3, p4);
                Ok(Effect::Continue)
            }
            InputState::TraceWidth => {
                let width = if line.is_empty() {
                    self.drawing.header.trace_width
                } else {
                    number(line)?
                };
                if width <= 0.0 {
                    return Err("TRACE width must be positive".into());
                }
                if width != self.drawing.header.trace_width {
                    self.save_undo();
                    self.drawing.header.trace_width = width;
                }
                self.state = InputState::TraceStart(width);
                Ok(Effect::Continue)
            }
            InputState::TraceStart(width) => {
                if line.is_empty() {
                    return self.cancel();
                }
                self.state = InputState::TraceNext(width, vec![point(line)?]);
                Ok(Effect::Continue)
            }
            InputState::TraceNext(width, mut points) => {
                if line.is_empty() {
                    if points.len() >= 2 {
                        for entity in trace_quads(&points, width)? {
                            self.add(entity);
                        }
                    }
                    self.state = InputState::Command;
                } else {
                    let previous = points.last().expect("TRACE has a first point");
                    let next = point_from(line, *previous)?;
                    if next == *previous {
                        return Err("TRACE needs distinct consecutive points".into());
                    }
                    points.push(next);
                    self.state = InputState::TraceNext(width, points);
                }
                Ok(Effect::Continue)
            }
            InputState::AreaFirstPoint => {
                if line.is_empty() {
                    return self.cancel();
                }
                self.state = InputState::AreaNextPoint(vec![point(line)?]);
                Ok(Effect::Continue)
            }
            InputState::AreaNextPoint(mut points) => {
                if line.is_empty() {
                    if points.len() < 3 {
                        return Err("AREA needs at least three points".into());
                    }
                    let (area, _) = polygon_metrics(&points);
                    self.status = format!("Area = {area:.4}");
                    self.state = InputState::Command;
                } else {
                    let previous = *points.last().expect("AREA has a first point");
                    points.push(point_from(line, previous)?);
                    self.state = InputState::AreaNextPoint(points);
                }
                Ok(Effect::Continue)
            }
            InputState::AreaSelection => {
                let ids = selection(line, selectable_count(&self.drawing))?;
                self.measure_area(&ids)?;
                self.state = InputState::Command;
                Ok(Effect::Continue)
            }
            InputState::RepeatColumns => {
                let columns = positive_word(line, "columns")?;
                self.state = InputState::RepeatRows(columns);
                Ok(Effect::Continue)
            }
            InputState::RepeatRows(columns) => {
                let rows = positive_word(line, "rows")?;
                let start = self.repeat_start.ok_or("ENDREP without REPEAT")?;
                let anchor = self.drawing.items[start..]
                    .iter()
                    .find_map(|item| match item {
                        Item::Entity(entity) => entity_anchor(entity),
                        _ => None,
                    })
                    .ok_or("REPEAT needs one or more ordinary entities")?;
                self.state = InputState::RepeatColumnSpacing(columns, rows, anchor);
                Ok(Effect::Continue)
            }
            InputState::RepeatColumnSpacing(columns, rows, previous) => {
                let (spacing, next, kind) = match number(line) {
                    Ok(spacing) => (
                        spacing,
                        Point {
                            x: previous.x + spacing,
                            y: previous.y,
                        },
                        RepeatDistanceInput::Number,
                    ),
                    Err(_) => {
                        let next = point_from(line, previous)?;
                        (0.0, next, RepeatDistanceInput::Point)
                    }
                };
                self.state = InputState::RepeatRowSpacing(columns, rows, spacing, next, kind);
                Ok(Effect::Continue)
            }
            InputState::RepeatRowSpacing(columns, rows, column_spacing, previous, kind) => {
                let (column_spacing, row_spacing) = match kind {
                    RepeatDistanceInput::Number => (column_spacing, number(line)?),
                    RepeatDistanceInput::Point => {
                        let next = point_from(line, previous)?;
                        (next.x - previous.x, next.y - previous.y)
                    }
                };
                let start = self.repeat_start.ok_or("ENDREP without REPEAT")?;
                let slice = &self.drawing.items[start..];
                if slice.is_empty() || slice.iter().any(|i| !matches!(i, Item::Entity(_))) {
                    return Err("REPEAT needs one or more ordinary entities".into());
                }
                self.save_undo();
                let entities = self
                    .drawing
                    .items
                    .drain(start..)
                    .map(|i| match i {
                        Item::Entity(e) => e,
                        _ => unreachable!(),
                    })
                    .collect();
                self.drawing.items.push(Item::Repeat(acad_model::Repeat {
                    entities,
                    columns,
                    rows,
                    column_spacing,
                    row_spacing,
                }));
                self.repeat_start = None;
                self.state = InputState::Command;
                self.refresh_after_edit();
                Ok(Effect::Continue)
            }
        }
    }

    fn command(&mut self, command: &str) -> Result<Effect, String> {
        match command.to_ascii_uppercase().as_str() {
            "LINE" | "L" => self.state = InputState::LineStart,
            "CIRCLE" | "C" => self.state = InputState::CircleCenter,
            "POINT" | "PO" => self.state = InputState::Point,
            "ARC" | "A" => self.state = InputState::ArcStart,
            "LOAD" => self.state = InputState::LoadLibrary,
            "SHAPE" => self.state = InputState::ShapeName,
            "TEXT" | "T" => self.state = InputState::TextOrigin,
            "INSERT" | "I" => self.state = InputState::InsertName,
            "BLOCK" => self.state = InputState::BlockName,
            "WBLOCK" => self.state = InputState::WblockPath,
            "REPEAT" => {
                if self.repeat_start.is_some() {
                    return Err("REPEAT is already open".into());
                }
                self.repeat_start = Some(self.drawing.items.len());
            }
            "ENDREP" => {
                if self.repeat_start.is_none() {
                    return Err("ENDREP without REPEAT".into());
                }
                self.state = InputState::RepeatColumns;
            }
            "BASE" => self.state = InputState::Base,
            "AXIS" => self.state = InputState::Axis,
            "SNAP" | "RES" | "RESOLUTION" => self.state = InputState::Snap,
            "DIM" => self.state = InputState::DimFirstExtension,
            "HATCH" => self.state = InputState::HatchPattern,
            "SKETCH" => self.state = InputState::SketchIncrement,
            "DELAY" => self.state = InputState::Delay,
            // RESUME is meaningful only when the command-script executor has
            // been interrupted. There is no script queue in this editor.
            "RESUME" => {}
            "UNITS" => self.state = InputState::UnitsFormat,
            "GRID" => self.state = InputState::Grid,
            "ORTHO" => self.state = InputState::Ortho,
            "FILL" => self.state = InputState::Fill,
            "LIMITS" => self.state = InputState::LimitsMin,
            "LAYER" => self.state = InputState::Layer,
            "COLOR" => self.state = InputState::ColorValue,
            "ZOOM" | "Z" => self.state = InputState::Zoom,
            "PAN" | "P" => self.state = InputState::PanCenter,
            "LIST" => {
                self.status = list_entities(&self.drawing);
            }
            "DBLIST" => {
                let report = database_listing(&self.drawing);
                self.status = "DBLIST report".into();
                return Ok(Effect::Report(report));
            }
            "?" | "HELP" => self.state = InputState::HelpCommand,
            "MENU" => self.state = InputState::MenuFile,
            "FILES" => {
                self.status = "File Utility Menu".into();
                self.state = InputState::FilesMenu;
                return Ok(Effect::Report(FILE_UTILITY_MENU.into()));
            }
            // STATUS, REDRAW and REGEN do not edit the drawing model. The
            // native window already redraws and regenerates each requested
            // frame, so no separate model operation is needed here.
            "STATUS" | "REDRAW" | "REGEN" => {}
            "ERASE" | "E" => self.state = InputState::EditSelection(EditCommand::Erase),
            "MOVE" | "M" => self.state = InputState::Displacement(EditCommand::Move),
            "COPY" | "CO" => self.state = InputState::Displacement(EditCommand::Copy),
            "ROTATE" | "RO" => self.state = InputState::EditSelection(EditCommand::Rotate),
            "SCALE" | "SC" => self.state = InputState::EditSelection(EditCommand::Scale),
            "ARRAY" | "AR" => self.state = InputState::ArraySelection,
            "CHANGE" | "CH" => self.state = InputState::ChangeSelection,
            "FILLET" | "F" => self.state = InputState::FilletSelection,
            "BREAK" | "BR" => self.state = InputState::BreakSelection,
            "DIST" | "DI" => self.state = InputState::DistanceFirstPoint,
            "ID" => self.state = InputState::IdPoint,
            "SOLID" | "SO" => self.state = InputState::SolidFirstPoint,
            "TRACE" | "TR" => self.state = InputState::TraceWidth,
            "AREA" | "AA" => self.state = InputState::AreaFirstPoint,
            "ENTITYAREA" => self.state = InputState::AreaSelection,
            "UNDO" | "U" => {
                if let Some(previous) = self.undo.pop() {
                    self.drawing = previous.drawing;
                    self.last_erased = previous.last_erased;
                    self.active_shape_library = previous.active_shape_library;
                }
            }
            "OOPS" => self.oops(),
            "SAVE" => self.state = InputState::SavePath,
            "END" | "QUIT" | "EXIT" => return Ok(Effect::Quit),
            "" => {}
            _ => return Err(format!("unknown command: {command}")),
        }
        Ok(Effect::Continue)
    }
}

const FILE_UTILITY_MENU: &str = "\
File Utility Menu

0. Exit File Utility Menu
1. List Drawing files
2. List Menu files
3. List Shape files
4. List Pattern files
5. List User specified files
6. Delete files
7. Rename files\n";
