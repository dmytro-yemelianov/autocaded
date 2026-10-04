//! Interactive command state machine for the 1983 editor.
pub mod menu;

mod change;
mod curve_history;
mod dimension;
mod dispatch;
mod editor_ops;
mod entity_ops;
mod export_context;
mod external_insert;
mod fillet;
mod geometry;
mod hatch_pattern;
pub use hatch_pattern::MAX_HATCH_PATTERN_FILE_BYTES;
mod input_state;
mod mouse_input;
mod parse;
mod report;
mod selection;
mod sketch;
mod text;
mod txt_metrics;
mod view;
pub use sketch::{SketchMode, SketchPreview, MAX_SKETCH_SEGMENTS};
pub use text::{TextMetricProvider, TextMetrics};

use acad_model::{Drawing, Extents, Header, Item, Point, UnitFormat, Units};
use entity_ops::{load_library_name, normalize_library_name};
use input_state::{InputState, RepeatDistanceInput};
use selection::item_pick_distance;
pub use selection::{selectable_items, SelectableItem};
use std::collections::BTreeMap;

pub(crate) const MAX_ARRAY_ENTITIES: usize = 100_000;

pub(crate) const HATCH_PATTERNS: &[(&str, &str)] = &[
    ("EARTH", "Earth or ground (subterranean)"),
    ("ESCHER", "Escher pattern"),
    ("FLEX", "Flexible material"),
    ("GRASS", "Grass area"),
    ("GRATE", "Grated area"),
    ("HEX", "Hexagons"),
    ("HONEY", "Honeycomb pattern"),
    ("HOUND", "Houndstooth check"),
    ("INSUL", "Insulation material"),
    ("LINE", "Parallel horizontal lines"),
    ("MUDST", "Mud and sand"),
    ("NET", "Horizontal / vertical grid"),
    ("NET3", "Network pattern 0-60-120"),
    ("PLAST", "Plastic material"),
    ("PLASTI", "Plastic material"),
    ("SACNCR", "Concrete"),
    ("SQUARE", "Small aligned squares"),
    ("STARS", "Star of David"),
    ("STEEL", "Steel material"),
    ("SWAMP", "Swampy area"),
    ("TRANS", "Heat transfer material"),
    ("TRIANG", "Equilateral triangles"),
    ("ZIGZAG", "Staircase effect"),
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MenuControl {
    Snap,
    Ortho,
    Cancel,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Effect {
    Continue,
    Save(String),
    /// Save the attached document, prompting for a path if it is unnamed.
    End,
    SaveAndQuit(String),
    /// WBLOCK output to a destination that must not exist yet: the host
    /// creates it and never replaces a file (docs/native-files-menu.md).
    SaveDrawing(String, Box<Drawing>),
    /// WBLOCK output whose replacement of an existing file was confirmed
    /// with `Y` at the replace question.
    ReplaceDrawing(String, Box<Drawing>),
    /// WBLOCK chose this output file. The host checks it now and calls
    /// [`Editor::ask_wblock_replace`] when it already exists; otherwise the
    /// block name prompt follows.
    CheckWblockDestination(String),
    LoadMenu(String),
    UnloadMenu,
    Files(FilesRequest),
    Report(String),
    Quit,
    /// Pause a running command script for this many milliseconds (native
    /// unit; see docs/native-scripts.md). Zero means no pause.
    Delay(u16),
    /// Start the named command script (application-resolved file name).
    Script(String),
    /// Continue an interrupted command script at its next unread item.
    Resume,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FilesFilter {
    Drawings,
    Menus,
    Shapes,
    Patterns,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FilesRequest {
    ListDrive { filter: FilesFilter, drive: char },
    ListSpecification(String),
    Delete(String),
    Rename { source: String, destination: String },
}

/// Command interpreter and mutable drawing under construction.
#[derive(Debug, Clone)]
pub struct Editor {
    drawing: Drawing,
    last_curve: Option<curve_history::CurveHistory>,
    last_text: Option<text::TextHistory>,
    state: InputState,
    dim_style: DimStyle,
    last_dimension: Option<dimension::DimHistory>,
    status: String,
    previous_view: Option<acad_model::DwgView>,
    view_aspect: f64,
    view_pixel_height: u32,
    undo: Vec<UndoSnapshot>,
    last_erased: Option<Vec<ErasedItem>>,
    repeat_start: Option<usize>,
    repeat_start_layer: u8,
    last_return_command: Option<String>,
    text_metrics: Option<std::sync::Arc<dyn TextMetricProvider>>,
    shape_libraries: BTreeMap<String, BTreeMap<String, u16>>,
    active_shape_library: Option<String>,
    committed_import_loads: Option<Vec<String>>,
    /// SKETCH state held while a window-close QUIT confirmation is pending,
    /// restored if the user declines.
    suspended_sketch: Option<Box<sketch::Sketch>>,
}

/// Interactive DIM settings; separate from the presence of DIMARROW in a file.
#[derive(Debug, Clone, Copy, PartialEq)]
struct DimStyle {
    arrow_size: f64,
    inside_horizontal_text: bool,
    outside_horizontal_text: bool,
}

#[derive(Debug, Clone)]
struct UndoSnapshot {
    drawing: Drawing,
    last_curve: Option<curve_history::CurveHistory>,
    last_text: Option<text::TextHistory>,
    last_dimension: Option<dimension::DimHistory>,
    last_erased: Option<Vec<ErasedItem>>,
    active_shape_library: Option<String>,
}

#[derive(Debug, Clone)]
struct ErasedItem {
    index: usize,
    item: Item,
}

impl Default for Editor {
    /// Starts a fresh drawing with the LIMITS and saved view observed in
    /// native AutoCAD 1.4's recovered standard display environment.
    fn default() -> Self {
        let zero = Extents {
            xmin: 0.0,
            xmax: 0.0,
            ymin: 0.0,
            ymax: 0.0,
        };
        let limits = Extents {
            xmin: 0.0,
            xmax: 12.0,
            ymin: 0.0,
            ymax: 9.0,
        };
        Self::new(Drawing {
            header: Header {
                extents: zero,
                limits,
                base: Point { x: 0.0, y: 0.0 },
                view: dispatch::fit_box_to_device(limits),
                axis: acad_model::Mode {
                    on: false,
                    spacing: 0.0,
                },
                snap: acad_model::Mode {
                    on: false,
                    spacing: 1.0,
                },
                grid: acad_model::Mode {
                    on: false,
                    spacing: 0.0,
                },
                ortho: false,
                fill: true,
                text_size: 0.2,
                trace_width: 0.05,
                fillet_radius: 0.0,
                dim_arrow: None,
                units: Units {
                    format: UnitFormat::Decimal,
                    precision: 4,
                },
                current_layer: 1,
                layers: BTreeMap::from([(0, 0), (1, 15)]),
                off_layers: Default::default(),
                dwg_header_passthrough: None,
            },
            items: Vec::new(),
        })
    }
}

impl Editor {
    pub fn new(drawing: Drawing) -> Self {
        let active_shape_library = drawing.items.iter().rev().find_map(|item| match item {
            Item::Entity(entity) => load_library_name(entity),
            _ => None,
        });
        Self {
            drawing,
            last_curve: None,
            last_text: None,
            state: InputState::Command,
            // Native interactive defaults; do not infer them from codec header data.
            dim_style: DimStyle {
                arrow_size: 9.0 / 64.0,
                inside_horizontal_text: true,
                outside_horizontal_text: true,
            },
            last_dimension: None,
            status: String::new(),
            previous_view: None,
            view_aspect: dispatch::ZOOM_ALL_DEVICE_ASPECT,
            view_pixel_height: 1,
            undo: Vec::new(),
            last_erased: None,
            repeat_start: None,
            repeat_start_layer: 1,
            last_return_command: None,
            text_metrics: None,
            shape_libraries: BTreeMap::new(),
            active_shape_library,
            committed_import_loads: None,
            suspended_sketch: None,
        }
    }

    /// Register available SHP libraries supplied by the application layer.
    /// Command processing remains independent of the font-file parser.
    pub fn register_shape_library(
        &mut self,
        name: &str,
        shapes: impl IntoIterator<Item = (String, u16)>,
    ) {
        self.shape_libraries.insert(
            normalize_library_name(name),
            shapes
                .into_iter()
                .map(|(name, number)| (name.to_ascii_uppercase(), number))
                .collect(),
        );
    }

    /// Whether the next command input supplies a LOAD library name.
    pub fn awaiting_shape_library_name(&self) -> bool {
        matches!(self.state, InputState::LoadLibrary)
    }

    pub fn drawing(&self) -> &Drawing {
        &self.drawing
    }

    pub fn drawing_mut(&mut self) -> &mut Drawing {
        &mut self.drawing
    }
    pub fn prompt(&self) -> &str {
        self.state.prompt()
    }
    pub fn status(&self) -> &str {
        &self.status
    }

    /// Window-close and transport requests use the same discard dialogue.
    /// An active SKETCH is suspended, not discarded, until the answer.
    pub fn request_quit(&mut self) -> Result<Effect, String> {
        let previous = std::mem::replace(&mut self.state, InputState::QuitConfirmation);
        self.suspended_sketch = match previous {
            InputState::Sketch(sketch) => Some(sketch),
            InputState::QuitConfirmation => self.suspended_sketch.take(),
            _ => None,
        };
        self.status.clear();
        Ok(Effect::Continue)
    }

    /// The application owns file paths; an unnamed END needs this prompt.
    pub fn request_end_path(&mut self) {
        self.state = InputState::EndSavePath;
    }

    /// The WBLOCK destination reported by `Effect::CheckWblockDestination`
    /// exists: ask the original's replace question before the block name.
    /// `open_drawing` names the attached document's own file. Ignored unless
    /// WBLOCK is at the block name step and has not been confirmed.
    pub fn ask_wblock_replace(&mut self, open_drawing: bool) {
        if let InputState::WblockName(output) = &self.state {
            if !output.replace {
                self.state = InputState::WblockReplace {
                    path: output.path.clone(),
                    open_drawing,
                };
            }
        }
    }

    /// Whether the current prompt takes a whole line literally, so a command
    /// script's spaces belong to the answer instead of acting as Return.
    pub fn accepts_literal_text(&self) -> bool {
        matches!(
            &self.state,
            InputState::Text(crate::text::TextInput::Value(_))
                | InputState::ChangeProperties(crate::change::ChangeInput::TextValue(_))
        )
    }

    pub fn awaiting_document_input(&self) -> bool {
        matches!(
            self.state,
            InputState::EndSavePath | InputState::QuitConfirmation
        )
    }

    /// World-space spacing of the edge ruler ticks, if enabled. A zero DWG
    /// spacing uses the current SNAP interval for native display.
    pub fn axis_spacing(&self) -> Option<f64> {
        self.drawing.header.axis.on.then(|| {
            let spacing = self.drawing.header.axis.spacing;
            if spacing > 0.0 && spacing.is_finite() {
                spacing
            } else {
                self.drawing.header.snap.spacing.max(f64::EPSILON)
            }
        })
    }

    /// Apply the recovered immediate screen-menu controls without submitting text.
    /// Menu Cancel retains the repeat marker and completed drawing mutations.
    pub fn apply_menu_control(&mut self, control: MenuControl) -> Result<Effect, String> {
        self.status = match control {
            MenuControl::Snap => {
                self.drawing.header.snap.on = !self.drawing.header.snap.on;
                if self.drawing.header.snap.on {
                    "<Snap on>"
                } else {
                    "<Snap off>"
                }
            }
            MenuControl::Ortho => {
                self.drawing.header.ortho = !self.drawing.header.ortho;
                if self.drawing.header.ortho {
                    "<Ortho on>"
                } else {
                    "<Ortho off>"
                }
            }
            MenuControl::Cancel => {
                self.state = InputState::Command;
                self.suspended_sketch = None;
                "*Cancel*"
            }
        }
        .into();
        Ok(Effect::Continue)
    }

    /// Cancel the active command and any unfinished REPEAT grouping.
    pub fn cancel_command(&mut self) -> Result<Effect, String> {
        self.state = InputState::Command;
        self.suspended_sketch = None;
        self.repeat_start = None;
        self.status.clear();
        Ok(Effect::Continue)
    }

    /// Whether the current prompt accepts a world-space point from a mouse click.
    pub fn accepts_mouse_point(&self) -> bool {
        if matches!(&self.state, InputState::ChangeProperties(state) if state.accepts_point()) {
            return true;
        }
        if matches!(&self.state, InputState::Text(state) if state.accepts_point()) {
            return true;
        }
        if matches!(&self.state, InputState::View(input) if input.accepts_point()) {
            return true;
        }
        if self.selection_window_pending() {
            return true;
        }
        matches!(
            self.state,
            InputState::Sketch(_)
                | InputState::LineStart
                | InputState::LineNext { .. }
                | InputState::CircleCenter
                | InputState::CircleRadius(_)
                | InputState::CircleTwoPointFirst
                | InputState::CircleTwoPointSecond(_)
                | InputState::CircleThreePointFirst
                | InputState::CircleThreePointSecond(_)
                | InputState::CircleThreePointThird(_, _)
                | InputState::Point
                | InputState::ArcStart
                | InputState::ArcMiddle(_)
                | InputState::ArcEnd(_, _)
                | InputState::ArcCenterFirst
                | InputState::ArcStartAfterCenter(_)
                | InputState::ArcCenter(_)
                | InputState::ArcCenterChoice(_, _)
                | InputState::ArcEndpoint(_)
                | InputState::ArcEndDirection(_, _)
                | InputState::ArcContinueEnd(_)
                | InputState::ShapeOrigin(_)
                | InputState::InsertOrigin(_, _)
                | InputState::InsertXScale(_, _)
                | InputState::BlockBase(_)
                | InputState::Base
                | InputState::LimitsMin
                | InputState::LimitsMax(_)
                | InputState::EditBase(_, _)
                | InputState::Displacement(_)
                | InputState::SecondPoint(_, _)
                | InputState::ArrayCircularCenter(_)
                | InputState::ArrayColumnSpacing(_, _, _, _)
                | InputState::ChangeIntersection(_)
                | InputState::BreakFirstPoint(_)
                | InputState::BreakSecondPoint(..)
                | InputState::DistanceFirstPoint
                | InputState::DistanceSecondPoint(_)
                | InputState::IdPoint
                | InputState::SolidFirstPoint
                | InputState::SolidSecondPoint(_)
                | InputState::SolidThirdPoint(_, _)
                | InputState::SolidFourthPoint(_, _, _)
                | InputState::TraceStart(_)
                | InputState::TraceNext(_, _)
                | InputState::AreaFirstPoint
                | InputState::AreaNextPoint(_)
                | InputState::RepeatColumnSpacing(_, _, _)
                | InputState::RepeatRowSpacing(_, _, _, _, RepeatDistanceInput::Point)
                | InputState::DimFirstExtension
                | InputState::DimIntersection(_)
                | InputState::DimSecondExtension(_)
                | InputState::HatchUserAngle(_)
                | InputState::HatchUserAngleSecond(..)
                | InputState::HatchUserSpacing(..)
                | InputState::HatchUserSpacingSecond(..)
        )
    }

    /// Apply mouse SNAP/ORTHO and submit through the ordinary command state machine.
    /// In SKETCH a point is a pen toggle at that pointer position.
    pub fn submit_mouse_point(&mut self, point: Point) -> Result<Effect, String> {
        if self.sketch_active() {
            return self.sketch_click(point);
        }
        let point = self.constrain_mouse_point(point)?;
        self.submit(&format!("{},{}", point.x, point.y))
    }

    /// Whether the current prompt accepts mouse picks of selectable entities.
    pub fn accepts_mouse_selection(&self) -> bool {
        self.state.is_selection_target() || self.collecting_selection()
    }

    /// Return the one-based selectable entity number nearest a world-space point.
    /// `None` also when the whole-drawing hit-test budget is exhausted; use
    /// [`Editor::try_pick_entity_at`] to tell that apart from a miss.
    pub fn pick_entity_at(&self, point: Point, tolerance: f64) -> Option<usize> {
        self.try_pick_entity_at(point, tolerance).ok().flatten()
    }

    /// Nearest selectable entity, or an error when scanning every owner would
    /// exceed the whole-drawing hit-test budget (no hit from a partial scan).
    pub fn try_pick_entity_at(
        &self,
        point: Point,
        tolerance: f64,
    ) -> Result<Option<usize>, String> {
        if !tolerance.is_finite() || tolerance < 0.0 {
            return Ok(None);
        }
        let mut remaining = selection::MAX_DRAWING_HIT_TEST_VISITS;
        let mut nearest: Option<(usize, f64)> = None;
        let scene = selection::visible_bounds::Scene::new(&self.drawing);
        for selected in selectable_items(&self.drawing) {
            let (distance, used) = item_pick_distance(&scene, point, selected.item);
            selection::charge_hit_test(&mut remaining, used)?;
            if let Some(distance) = distance {
                if distance <= tolerance
                    && nearest.is_none_or(|(_, nearest_distance)| distance < nearest_distance)
                {
                    nearest = Some((selected.id, distance));
                }
            }
        }
        Ok(nearest.map(|(id, _)| id))
    }
}

#[cfg(test)]
mod dim_settings_tests {
    use super::*;

    fn answer(editor: &mut Editor, option: &str, inputs: &[&str]) {
        editor.submit("DIM").unwrap();
        editor.submit(option).unwrap();
        for input in inputs {
            editor.submit(input).unwrap();
        }
    }

    #[test]
    fn dim_settings_defaults_are_session_values_not_header_values() {
        let fresh = Editor::default();
        let mut drawing = fresh.drawing.clone();
        drawing.header.dim_arrow = Some(2.0);
        let loaded = Editor::new(drawing.clone());
        assert_eq!(loaded.dim_style, fresh.dim_style);
        assert_eq!(loaded.dim_style.arrow_size, 9.0 / 64.0);
        assert!(loaded.dim_style.inside_horizontal_text);
        assert!(loaded.dim_style.outside_horizontal_text);
        assert_eq!(loaded.drawing, drawing);
    }

    #[test]
    fn dim_settings_arrow_replacement_and_errors_preserve_committed_state() {
        let mut editor = Editor::default();
        // Existing geometry and undo history must survive every settings submission.
        for input in ["LINE", "1,1", "2,2", ""] {
            editor.submit(input).unwrap();
        }
        editor.drawing.header.dim_arrow = Some(2.0);
        let drawing = editor.drawing.clone();
        let undo_count = editor.undo.len();
        for (input, expected) in [
            ("0.140625", 9.0 / 64.0),
            (".25", 0.25),
            ("0.5", 0.5),
            ("2", 2.0),
            ("0.21875", 7.0 / 32.0),
            ("0.5", 0.5),
        ] {
            answer(&mut editor, "A", &[input]);
            assert_eq!(editor.dim_style.arrow_size, expected);
            assert!(editor.dim_style.inside_horizontal_text);
            assert!(editor.dim_style.outside_horizontal_text);
            assert_eq!(editor.prompt(), "Command");
            assert_eq!(editor.drawing, drawing);
            assert_eq!(editor.undo.len(), undo_count);
        }
        let style = editor.dim_style;
        for input in [
            "", "NaN", "inf", "-inf", "0", "-0", "-1", "0,0", "1'-2\"", "1e999",
        ] {
            answer(&mut editor, "A", &[]);
            assert!(editor.submit(input).is_err(), "{input:?}");
            assert_eq!(editor.state, InputState::Command);
            assert_eq!(editor.dim_style, style, "{input:?}");
            assert_eq!(editor.drawing, drawing);
            assert_eq!(editor.undo.len(), undo_count);
        }
    }

    #[test]
    fn dim_settings_text_commits_independent_values_only_after_both_answers() {
        let mut editor = Editor::default();
        for input in ["LINE", "1,1", "2,2", ""] {
            editor.submit(input).unwrap();
        }
        let drawing = editor.drawing.clone();
        let undo_count = editor.undo.len();
        for (inside, outside, expected_inside, expected_outside) in [
            ("y", "n", true, false),
            ("n", "", false, false),
            ("", "", false, false),
            ("Y", "Y", true, true),
        ] {
            let before = editor.dim_style;
            answer(&mut editor, "T", &[]);
            assert!(editor.submit("bad").is_err());
            assert_eq!(editor.dim_style, before);
            editor.submit(inside).unwrap();
            let pending = editor.state.clone();
            assert_eq!(editor.dim_style, before);
            assert!(editor.submit("NO").is_err());
            assert_eq!(editor.state, pending);
            assert_eq!(editor.dim_style, before);
            editor.submit(outside).unwrap();
            assert_eq!(editor.dim_style.inside_horizontal_text, expected_inside);
            assert_eq!(editor.dim_style.outside_horizontal_text, expected_outside);
            assert_eq!(editor.dim_style.arrow_size, before.arrow_size);
            assert_eq!(editor.state, InputState::Command);
            assert_eq!(editor.drawing, drawing);
            assert_eq!(editor.undo.len(), undo_count);
        }
    }

    #[test]
    fn dim_settings_cancel_discards_each_pending_phase_and_preserves_repeat_policy() {
        let mut editor = Editor::default();
        for input in ["LINE", "1,1", "2,2", ""] {
            editor.submit(input).unwrap();
        }
        answer(&mut editor, "A", &["0.5"]);
        answer(&mut editor, "T", &["N", "N"]);
        let style = editor.dim_style;
        let drawing = editor.drawing.clone();
        let undo_count = editor.undo.len();
        for menu_cancel in [false, true] {
            for (option, inputs) in [("A", &[][..]), ("T", &[][..]), ("T", &["Y"][..])] {
                answer(&mut editor, option, inputs);
                editor.repeat_start = Some(3);
                if menu_cancel {
                    editor.apply_menu_control(MenuControl::Cancel).unwrap();
                    assert_eq!(editor.repeat_start, Some(3));
                } else {
                    editor.cancel_command().unwrap();
                    assert_eq!(editor.repeat_start, None);
                }
                assert_eq!(editor.state, InputState::Command);
                assert_eq!(editor.dim_style, style);
                assert_eq!(editor.drawing, drawing);
                assert_eq!(editor.undo.len(), undo_count);
            }
        }
    }
}
