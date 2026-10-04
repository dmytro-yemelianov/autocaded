//! Command dispatch: `submit` and `command`, the top-level interpreter loop.

use crate::dimension::{dimension_geometry, DimInput};
use crate::entity_ops::{entity_anchor, item_anchor, normalize_library_name};
use crate::geometry::{polygon_metrics, trace_quads};
use crate::input_state::{
    ArraySpacingInput, EditCommand, InputState, RepeatDistanceInput, Transform,
};
use crate::parse::{
    color_index, format_measurement, layer_index, number, parse_grid_mode, parse_mode,
    parse_toggle, point, point_from, positive_count, positive_word,
};
use crate::report::{database_listing, hatch_pattern_report, help_report, status_report};
use crate::selection::{selectable_count, selected_item_indexes, selection};
use crate::{Editor, Effect, FilesFilter, FilesRequest, HATCH_PATTERNS, MAX_ARRAY_ENTITIES};
use acad_model::{Entity, Extents, Item, Point, UnitFormat, Units};

mod blocks;
mod creation;
mod dimensions;
mod editing;
mod files;
mod hatch;
mod measurement;
mod settings;

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
pub(crate) const ZOOM_ALL_DEVICE_ASPECT: f64 = 1.522_331_154_684_095_9;

fn dim_horizontal_answer(input: &str, default: bool) -> Result<bool, String> {
    match input.to_ascii_uppercase().as_str() {
        "Y" => Ok(true),
        "N" => Ok(false),
        "" => Ok(default),
        _ => Err("expected Y, N, or Enter for the displayed default".into()),
    }
}

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
pub(crate) fn zoom_all_bounds(limits: Extents, extents: Extents) -> Extents {
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
            if line.is_empty() && matches!(command.as_str(), "TEXT" | "T") {
                return self.repeat_text();
            }
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
            InputState::LineStart if line.is_empty() && self.curve_tangent().is_none() => {
                self.state = InputState::Command;
                self.status = "*Invalid*".into();
                return Ok(Effect::Continue);
            }
            InputState::CircleRadius(_) if line.is_empty() => {
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
        let single_erase = self.single_erase_input(line);
        let text_payload = self.accepts_literal_text();
        let result = self.submit(if text_payload { input } else { line });
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
        if let Some(result) = self.route_selection_input(line) {
            return result;
        }
        match self.state.clone() {
            InputState::Command => self.command(line),
            InputState::Selection(_) | InputState::ListSelection => {
                unreachable!("shared selector handles selection dialogue")
            }
            state @ (InputState::LineStart
            | InputState::LineNext { .. }
            | InputState::CircleCenter
            | InputState::CircleRadius(..)
            | InputState::CircleDiameter(..)
            | InputState::CircleTwoPointFirst
            | InputState::CircleTwoPointSecond(..)
            | InputState::CircleThreePointFirst
            | InputState::CircleThreePointSecond(..)
            | InputState::CircleThreePointThird(..)
            | InputState::Point
            | InputState::ArcStart
            | InputState::ArcMiddle(..)
            | InputState::ArcEnd(..)
            | InputState::ArcCenterFirst
            | InputState::ArcStartAfterCenter(..)
            | InputState::ArcCenter(..)
            | InputState::ArcCenterChoice(..)
            | InputState::ArcCenterAngle(..)
            | InputState::ArcCenterChord(..)
            | InputState::ArcEndpoint(..)
            | InputState::ArcEndChoice(..)
            | InputState::ArcEndRadius(..)
            | InputState::ArcEndAngle(..)
            | InputState::ArcEndDirection(..)
            | InputState::ArcContinueEnd(..)
            | InputState::LoadLibrary
            | InputState::ShapeName
            | InputState::ShapeOrigin(..)
            | InputState::ShapeHeight(..)
            | InputState::ShapeRotation(..)
            | InputState::Text(..)
            | InputState::SolidFirstPoint
            | InputState::SolidSecondPoint(..)
            | InputState::SolidThirdPoint(..)
            | InputState::SolidFourthPoint(..)
            | InputState::TraceWidth
            | InputState::TraceStart(..)
            | InputState::TraceNext(..)) => self.submit_creation(state, line, input),
            state @ (InputState::InsertName
            | InputState::InsertOrigin(..)
            | InputState::InsertXScale(..)
            | InputState::InsertYScale(..)
            | InputState::InsertRotation(..)
            | InputState::BlockName
            | InputState::BlockBase(..)
            | InputState::BlockSelection(..)
            | InputState::RepeatColumns
            | InputState::RepeatRows(..)
            | InputState::RepeatColumnSpacing(..)
            | InputState::RepeatRowSpacing(..)) => self.submit_blocks(state, line),
            state @ (InputState::SavePath
            | InputState::EndSavePath
            | InputState::QuitConfirmation
            | InputState::WblockPath
            | InputState::WblockName(..)
            | InputState::WblockBase(..)
            | InputState::WblockSelection(..)
            | InputState::HelpCommand
            | InputState::MenuFile
            | InputState::ScriptFile
            | InputState::FilesMenu
            | InputState::FilesDrive(..)
            | InputState::FilesListSpecification
            | InputState::FilesDeleteSpecification
            | InputState::FilesRenameSource
            | InputState::FilesRenameDestination(..)) => self.submit_files(state, line),
            state @ (InputState::DimFirstExtension
            | InputState::DimArrowSize
            | InputState::DimInsideHorizontalText { .. }
            | InputState::DimOutsideHorizontalText { .. }
            | InputState::DimIntersection(..)
            | InputState::DimSecondExtension(..)
            | InputState::DimText(..)) => self.submit_dimensions(state, line),
            state @ (InputState::HatchPattern
            | InputState::HatchScale(..)
            | InputState::HatchAngle(..)
            | InputState::HatchSelection(..)
            | InputState::HatchUserAngle(..)
            | InputState::HatchUserAngleSecond(..)
            | InputState::HatchUserSpacing(..)
            | InputState::HatchUserSpacingSecond(..)
            | InputState::HatchUserDouble(..)) => self.submit_hatch(state, line),
            state @ (InputState::SketchIncrement
            | InputState::Delay
            | InputState::UnitsFormat
            | InputState::UnitsPrecision(..)
            | InputState::Base
            | InputState::Axis
            | InputState::Snap
            | InputState::Grid
            | InputState::Ortho
            | InputState::Fill
            | InputState::LimitsMin
            | InputState::LimitsMax(..)
            | InputState::Layer
            | InputState::LayerVisibility(_)
            | InputState::LayerColor
            | InputState::ColorValue
            | InputState::ColorTargetLayer) => self.submit_settings(state, line),
            state @ (InputState::FilletSelection | InputState::FilletRadius) => {
                self.submit_fillet(state, line)
            }
            InputState::View(input) => self.submit_view(input, line),
            InputState::Sketch(_) => self.submit_sketch(line),
            state @ (InputState::ChangeSelection
            | InputState::ChangeIntersection(..)
            | InputState::ChangeLayer(..)
            | InputState::ChangeProperties(..)) => self.submit_change(state, line, input),
            state @ (InputState::EditSelection(..)
            | InputState::Displacement(..)
            | InputState::SecondPoint(..)
            | InputState::DisplacedSelection(..)
            | InputState::EditBase(..)
            | InputState::EditValue(..)
            | InputState::ArraySelection
            | InputState::ArrayMode(..)
            | InputState::ArrayCircularCenter(..)
            | InputState::ArrayCircularAngle(..)
            | InputState::ArrayCircularItems(..)
            | InputState::ArrayCircularRotate(..)
            | InputState::ArrayRows(..)
            | InputState::ArrayColumns(..)
            | InputState::ArrayRowSpacing(..)
            | InputState::ArrayColumnSpacing(..)
            | InputState::BreakSelection
            | InputState::BreakFirstPoint(..)
            | InputState::BreakSecondPoint(..)) => self.submit_editing(state, line),
            state @ (InputState::DistanceFirstPoint
            | InputState::DistanceSecondPoint(..)
            | InputState::IdPoint
            | InputState::AreaFirstPoint
            | InputState::AreaNextPoint(..)
            | InputState::AreaSelection) => self.submit_measurement(state, line),
        }
    }

    fn command(&mut self, command: &str) -> Result<Effect, String> {
        if let Some((name, arguments)) = command.split_once(char::is_whitespace) {
            if name.eq_ignore_ascii_case("LAYER") {
                self.state = InputState::Layer;
                return self.submit_layer(arguments.trim());
            }
        }
        match command.to_ascii_uppercase().as_str() {
            "LINE" | "L" => self.state = InputState::LineStart,
            "CIRCLE" | "C" => self.state = InputState::CircleCenter,
            "POINT" | "PO" => self.state = InputState::Point,
            "ARC" | "A" => self.state = InputState::ArcStart,
            "LOAD" => self.state = InputState::LoadLibrary,
            "SHAPE" => self.state = InputState::ShapeName,
            "TEXT" | "T" => self.state = InputState::Text(crate::text::TextInput::Start),
            "INSERT" | "I" => self.state = InputState::InsertName,
            "BLOCK" => self.state = InputState::BlockName,
            "WBLOCK" => self.state = InputState::WblockPath,
            "REPEAT" => {
                if self.repeat_start.is_some() {
                    return Err("REPEAT is already open".into());
                }
                self.repeat_start = Some(self.drawing.items.len());
                self.repeat_start_layer = self.drawing.header.current_layer;
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
            // The application owns the command-script executor; RESUME only
            // asks it to continue an interrupted script (docs/native-scripts.md).
            "RESUME" => return Ok(Effect::Resume),
            // Native extension: AutoCAD 1.4 starts scripts only from the DOS
            // command line. The application resolves and runs the file.
            "SCRIPT" => self.state = InputState::ScriptFile,
            "UNITS" => self.state = InputState::UnitsFormat,
            "GRID" => self.state = InputState::Grid,
            "ORTHO" => self.state = InputState::Ortho,
            "FILL" => self.state = InputState::Fill,
            "LIMITS" => self.state = InputState::LimitsMin,
            "LAYER" => self.state = InputState::Layer,
            "COLOR" => self.state = InputState::ColorValue,
            "ZOOM" | "Z" => self.state = InputState::View(crate::view::ViewInput::Zoom),
            "PAN" | "P" => self.state = InputState::View(crate::view::ViewInput::PanFirst),
            "LIST" => self.state = InputState::ListSelection,
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
            "STATUS" => {
                self.status = "Drawing status".into();
                return Ok(Effect::Report(status_report(&self.drawing)));
            }
            // REDRAW and REGEN do not edit the drawing model. The
            // native window already redraws and regenerates each requested
            // frame, so no separate model operation is needed here.
            "REDRAW" | "REGEN" => {}
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
                    self.last_dimension = previous.last_dimension;
                    self.last_erased = previous.last_erased;
                    self.active_shape_library = previous.active_shape_library;
                    self.last_curve = previous.last_curve;
                    self.last_text = previous.last_text;
                }
            }
            "OOPS" => self.oops(),
            "SAVE" => self.state = InputState::SavePath,
            "END" => return Ok(Effect::End),
            "QUIT" | "EXIT" => return self.request_quit(),
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
