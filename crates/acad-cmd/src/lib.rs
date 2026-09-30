//! Interactive command state machine for the 1983 editor.
pub mod menu;

mod dispatch;
mod editor_ops;
mod entity_ops;
mod geometry;
mod input_state;
mod parse;
mod report;
mod selection;

use acad_model::{Drawing, Entity, Extents, Header, Item, Point, UnitFormat, Units};
use entity_ops::{bare, load_library_name, normalize_library_name};
use input_state::{InputState, RepeatDistanceInput};
use selection::entity_pick_distance;
use std::collections::BTreeMap;

#[cfg(test)]
use report::LINE_HELP;

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

#[derive(Debug, Clone, PartialEq)]
pub enum Effect {
    Continue,
    Save(String),
    SaveDrawing(String, Box<Drawing>),
    LoadMenu(String),
    Files(FilesRequest),
    Report(String),
    Quit,
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
    state: InputState,
    status: String,
    previous_view: Option<acad_model::DwgView>,
    undo: Vec<UndoSnapshot>,
    last_erased: Option<Vec<ErasedItem>>,
    repeat_start: Option<usize>,
    shape_libraries: BTreeMap<String, BTreeMap<String, u16>>,
    active_shape_library: Option<String>,
}

#[derive(Debug, Clone)]
struct UndoSnapshot {
    drawing: Drawing,
    last_erased: Option<Vec<ErasedItem>>,
    active_shape_library: Option<String>,
}

#[derive(Debug, Clone)]
struct ErasedItem {
    index: usize,
    item: Item,
}

impl Default for Editor {
    fn default() -> Self {
        let zero = Extents {
            xmin: 0.0,
            xmax: 0.0,
            ymin: 0.0,
            ymax: 0.0,
        };
        Self::new(Drawing {
            header: Header {
                extents: zero,
                limits: Extents {
                    xmin: -10.0,
                    xmax: 10.0,
                    ymin: -10.0,
                    ymax: 10.0,
                },
                base: Point { x: 0.0, y: 0.0 },
                view: acad_model::DwgView {
                    center: Point { x: 0.0, y: 0.0 },
                    height: 20.0,
                },
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
                units: Units {
                    format: UnitFormat::Decimal,
                    precision: 4,
                },
                current_layer: 1,
                layers: BTreeMap::from([(0, 0), (1, 15)]),
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
            state: InputState::Command,
            status: String::new(),
            previous_view: None,
            undo: Vec::new(),
            last_erased: None,
            repeat_start: None,
            shape_libraries: BTreeMap::new(),
            active_shape_library,
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

    /// Cancel the active command and any unfinished REPEAT grouping.
    pub fn cancel_command(&mut self) -> Result<Effect, String> {
        self.state = InputState::Command;
        self.repeat_start = None;
        self.status.clear();
        Ok(Effect::Continue)
    }

    /// Whether the current prompt accepts a world-space point from a mouse click.
    pub fn accepts_mouse_point(&self) -> bool {
        matches!(
            self.state,
            InputState::LineStart
                | InputState::LineNext(_)
                | InputState::CircleCenter
                | InputState::Point
                | InputState::ArcStart
                | InputState::ArcMiddle(_)
                | InputState::ArcEnd(_, _)
                | InputState::ShapeOrigin(_)
                | InputState::TextOrigin
                | InputState::InsertOrigin(_, _)
                | InputState::InsertXScale(_, _)
                | InputState::BlockBase(_)
                | InputState::Base
                | InputState::LimitsMin
                | InputState::LimitsMax(_)
                | InputState::ZoomCenter
                | InputState::ZoomWindowMin
                | InputState::ZoomWindowMax(_)
                | InputState::PanCenter
                | InputState::EditBase(_, _)
                | InputState::Displacement(_)
                | InputState::SecondPoint(_, _)
                | InputState::ArrayCircularCenter(_)
                | InputState::ArrayColumnSpacing(_, _, _, _)
                | InputState::ChangeIntersection(_)
                | InputState::BreakFirstPoint(_)
                | InputState::BreakSecondPoint(_, _)
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
                | InputState::DimSecondExtension(_, _)
                | InputState::HatchWindowFirst(_, _, _)
                | InputState::HatchWindowSecond(_, _, _, _)
        )
    }

    /// Submit a mouse-selected point through the same state machine as typed coordinates.
    pub fn submit_mouse_point(&mut self, point: Point) -> Result<Effect, String> {
        if !self.accepts_mouse_point() {
            return Err("current prompt does not accept a point".into());
        }
        self.submit(&format!("{},{}", point.x, point.y))
    }

    /// Whether the current prompt accepts mouse picks of selectable entities.
    pub fn accepts_mouse_selection(&self) -> bool {
        matches!(
            self.state,
            InputState::BlockSelection(_, _)
                | InputState::EditSelection(_)
                | InputState::DisplacedSelection(_, _)
                | InputState::ArraySelection
                | InputState::ChangeSelection
                | InputState::FilletSelection
                | InputState::BreakSelection
                | InputState::AreaSelection
                | InputState::HatchSelection(_, _, _)
        )
    }

    /// Return the one-based selectable entity number nearest a world-space point.
    pub fn pick_entity_at(&self, point: Point, tolerance: f64) -> Option<usize> {
        if !tolerance.is_finite() || tolerance < 0.0 {
            return None;
        }
        let mut selectable_id = 0;
        let mut nearest: Option<(usize, f64)> = None;
        for item in &self.drawing.items {
            let Item::Entity(entity) = item else {
                continue;
            };
            if matches!(bare(entity), Entity::Load { .. }) {
                continue;
            }
            selectable_id += 1;
            if let Some(distance) = entity_pick_distance(point, entity) {
                if distance <= tolerance
                    && nearest.is_none_or(|(_, nearest_distance)| distance < nearest_distance)
                {
                    nearest = Some((selectable_id, distance));
                }
            }
        }
        nearest.map(|(id, _)| id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geometry::line_points;

    fn bare_first(editor: &Editor) -> &Entity {
        bare(editor.drawing.entities().next().unwrap())
    }

    #[test]
    fn line_keeps_accepting_vertices_until_return_and_undo_removes_the_last_segment() {
        let mut editor = Editor::default();
        editor.submit("LINE").unwrap();
        editor.submit("1.25,2.5").unwrap();
        editor.submit("9.5,6.75").unwrap();
        editor.submit("10,8").unwrap();
        editor.submit("").unwrap();
        assert_eq!(editor.drawing.entities().count(), 2);
        assert_eq!(
            bare_first(&editor),
            &Entity::Line {
                start: Point { x: 1.25, y: 2.5 },
                end: Point { x: 9.5, y: 6.75 }
            }
        );
        editor.submit("UNDO").unwrap();
        assert_eq!(editor.drawing.entities().count(), 1);
        assert_eq!(editor.prompt(), "Command");
    }

    #[test]
    fn axis_is_a_display_setting_with_snap_relative_spacing() {
        let mut editor = Editor::default();
        assert_eq!(editor.axis_spacing(), None);
        editor.submit("SNAP").unwrap();
        editor.submit("0.5").unwrap();
        editor.submit("AXIS").unwrap();
        assert_eq!(
            editor.prompt(),
            "AXIS: ON, OFF, or tick spacing (X for snap multiples)"
        );
        editor.submit("5x").unwrap();
        assert_eq!(editor.axis_spacing(), Some(2.5));

        editor.submit("SNAP").unwrap();
        editor.submit("2").unwrap();
        editor.submit("AXIS").unwrap();
        editor.submit("ON").unwrap();
        assert_eq!(
            editor.axis_spacing(),
            Some(2.5),
            "ON retains explicit spacing"
        );
        editor.submit("AXIS").unwrap();
        editor.submit("OFF").unwrap();
        assert_eq!(editor.axis_spacing(), None);
        editor.submit("AXIS").unwrap();
        editor.submit("ON").unwrap();
        assert_eq!(
            editor.axis_spacing(),
            Some(2.5),
            "OFF preserves explicit spacing"
        );

        let mut fresh = Editor::default();
        fresh.submit("SNAP").unwrap();
        fresh.submit("2").unwrap();
        fresh.submit("AXIS").unwrap();
        fresh.submit("ON").unwrap();
        assert_eq!(fresh.drawing().header.axis.spacing, 0.0);
        assert_eq!(
            fresh.axis_spacing(),
            Some(2.0),
            "zero means use SNAP spacing"
        );
    }

    #[test]
    fn axis_spacing_must_be_positive_and_finite() {
        let mut editor = Editor::default();
        editor.submit("AXIS").unwrap();
        for invalid in ["0", "-1", "NaN", "inf"] {
            assert!(editor.submit(invalid).is_err(), "accepted {invalid}");
            assert_eq!(editor.axis_spacing(), None);
        }
    }

    #[test]
    fn mouse_points_enter_line_vertices_through_the_command_state_machine() {
        let mut editor = Editor::default();
        assert!(!editor.accepts_mouse_point());
        editor.submit("LINE").unwrap();
        assert!(editor.accepts_mouse_point());
        editor
            .submit_mouse_point(Point { x: 1.25, y: -2.5 })
            .unwrap();
        editor.submit_mouse_point(Point { x: 8.0, y: 4.0 }).unwrap();
        editor.submit("").unwrap();

        let entities = editor.drawing.entities().collect::<Vec<_>>();
        assert_eq!(entities.len(), 1);
        let (start, end) = line_points(entities[0]).unwrap();
        assert_eq!(start, Point { x: 1.25, y: -2.5 });
        assert_eq!(end, Point { x: 8.0, y: 4.0 });
        assert!(!editor.accepts_mouse_point());
        assert!(editor.submit_mouse_point(Point { x: 0.0, y: 0.0 }).is_err());
    }

    #[test]
    fn mouse_entity_picks_use_selectable_numbers_and_screen_tolerance() {
        let mut editor = Editor::default();
        for input in ["LINE", "0,0", "10,0", "", "CIRCLE", "5,5", "1", "ERASE"] {
            editor.submit(input).unwrap();
        }

        assert!(editor.accepts_mouse_selection());
        assert_eq!(
            editor.pick_entity_at(Point { x: 4.0, y: 0.4 }, 0.5),
            Some(1),
            "the LINE can be picked within the pixel-derived tolerance"
        );
        assert_eq!(
            editor.pick_entity_at(Point { x: 6.0, y: 5.1 }, 0.2),
            Some(2),
            "a CIRCLE is picked by distance to its circumference"
        );
        assert_eq!(editor.pick_entity_at(Point { x: 5.0, y: 5.0 }, 0.2), None);
        assert_eq!(
            editor.pick_entity_at(Point { x: 4.0, y: 0.4 }, f64::NAN),
            None
        );
    }

    #[test]
    fn circle_and_point_use_current_layer_and_report_effects() {
        let mut editor = Editor::default();
        editor.submit("POINT").unwrap();
        editor.submit("2,3").unwrap();
        editor.submit("CIRCLE").unwrap();
        editor.submit("4,5").unwrap();
        editor.submit("2.25").unwrap();
        assert_eq!(editor.drawing.entities().count(), 2);
        assert!(
            matches!(editor.drawing.entities().next(), Some(Entity::OnLayer { layer: 1, entity }) if matches!(entity.as_ref(), Entity::Point { origin: Point { x: 2.0, y: 3.0 } }))
        );
        editor.submit("SAVE").unwrap();
        assert_eq!(
            editor.submit("sample.dwg").unwrap(),
            Effect::Save("sample.dwg".into())
        );
        assert_eq!(editor.submit("END").unwrap(), Effect::Quit);
    }

    #[test]
    fn load_and_shape_create_the_original_named_shape_records() {
        let mut editor = Editor::default();
        editor.register_shape_library("B:ES.SHP", [("RES".into(), 129), ("CAP".into(), 130)]);
        for input in ["LOAD", "B:ES", "SHAPE"] {
            editor.submit(input).unwrap();
        }
        assert!(editor.submit("MISSING").is_err());
        editor.submit("RES").unwrap();
        assert!(editor.accepts_mouse_point());
        for input in [
            "2.25,3.5", "0.75", "30", "SHAPE", "CAP", "6.5,2.75", "1.25", "75",
        ] {
            editor.submit(input).unwrap();
        }
        assert_eq!(
            editor.drawing().items,
            vec![
                Item::Entity(Entity::OnLayer {
                    layer: 1,
                    entity: Box::new(Entity::Load {
                        name: "B:ES".into()
                    }),
                }),
                Item::Entity(Entity::OnLayer {
                    layer: 1,
                    entity: Box::new(Entity::Shape {
                        origin: Point { x: 2.25, y: 3.5 },
                        height: 0.75,
                        rotation_deg: 30.0,
                        number: 129,
                    }),
                }),
                Item::Entity(Entity::OnLayer {
                    layer: 1,
                    entity: Box::new(Entity::Shape {
                        origin: Point { x: 6.5, y: 2.75 },
                        height: 1.25,
                        rotation_deg: 75.0,
                        number: 130,
                    }),
                }),
            ]
        );
        for _ in 0..3 {
            editor.submit("UNDO").unwrap();
        }
        assert!(editor.drawing().items.is_empty());
        editor.submit("SHAPE").unwrap();
        assert!(editor.submit("RES").is_err());
    }

    #[test]
    fn redraw_and_regen_leave_drawing_data_unchanged() {
        let mut editor = Editor::default();
        for input in ["LINE", "2,3", "8,3", ""] {
            editor.submit(input).unwrap();
        }
        let original = editor.drawing().clone();
        for command in ["STATUS", "REDRAW", "REGEN"] {
            assert_eq!(editor.submit(command).unwrap(), Effect::Continue);
            assert_eq!(editor.prompt(), "Command");
            assert_eq!(editor.drawing(), &original);
        }
    }

    #[test]
    fn cancel_command_returns_to_command_prompt_and_closes_repeat_grouping() {
        let mut editor = Editor::default();
        editor.submit("LINE").unwrap();
        editor.submit("2,3").unwrap();
        assert_eq!(editor.prompt(), "LINE: next point (Enter to finish)");
        editor.cancel_command().unwrap();
        assert_eq!(editor.prompt(), "Command");

        editor.submit("REPEAT").unwrap();
        editor.submit("POINT").unwrap();
        editor.submit("4,5").unwrap();
        assert_eq!(editor.drawing().entities().count(), 1);
        editor.cancel_command().unwrap();
        assert_eq!(editor.prompt(), "Command");
        assert!(editor.submit("REPEAT").is_ok());
    }

    #[test]
    fn insert_places_a_case_insensitive_block_with_default_or_explicit_scaling() {
        let mut editor = Editor::default();
        editor
            .drawing_mut()
            .items
            .push(Item::Block(acad_model::Block {
                name: "SYMBOL".into(),
                base: Point { x: 1.0, y: 2.0 },
                entities: vec![Entity::Line {
                    start: Point { x: 1.0, y: 2.0 },
                    end: Point { x: 3.0, y: 2.0 },
                }],
            }));
        for input in ["INSERT", "symbol", "10,20", "", "", ""] {
            editor.submit(input).unwrap();
        }
        assert_eq!(
            bare_first(&editor),
            &Entity::Insert {
                origin: Point { x: 10.0, y: 20.0 },
                x_scale: 1.0,
                y_scale: 1.0,
                rotation_deg: 0.0,
                name: "SYMBOL".into(),
            }
        );
        for input in ["INSERT", "SYMBOL", "-2,4", "2", "3", "45"] {
            editor.submit(input).unwrap();
        }
        assert_eq!(
            bare(editor.drawing().entities().nth(1).unwrap()),
            &Entity::Insert {
                origin: Point { x: -2.0, y: 4.0 },
                x_scale: 2.0,
                y_scale: 3.0,
                rotation_deg: 45.0,
                name: "SYMBOL".into(),
            }
        );
        assert!(editor.submit("UNDO").is_ok());
        assert_eq!(editor.drawing().entities().count(), 1);
        for input in ["INSERT", "SYMBOL", "0,0"] {
            editor.submit(input).unwrap();
        }
        assert!(editor.submit("0").is_err());
        assert_eq!(editor.drawing().entities().count(), 1);
        assert_eq!(
            editor.prompt(),
            "INSERT: X scale or opposite corner x,y (Enter for 1)"
        );
    }

    #[test]
    fn insert_rejects_unknown_blocks_without_mutation() {
        let mut editor = Editor::default();
        editor.submit("INSERT").unwrap();
        assert!(editor.submit("MISSING").is_err());
        assert_eq!(editor.prompt(), "INSERT: block name");
        assert!(editor.drawing().entities().next().is_none());
    }

    #[test]
    fn star_insert_explodes_a_block_after_translating_from_its_base() {
        let mut editor = Editor::default();
        for input in ["LINE", "2,3", "4,5", "", "BLOCK", "B1", "1,2", "LAST"] {
            editor.submit(input).unwrap();
        }
        let before = editor.drawing().clone();
        for input in ["INSERT", "*b1", "7,8"] {
            editor.submit(input).unwrap();
        }
        assert_eq!(editor.prompt(), "Command");
        assert_eq!(editor.drawing().blocks().count(), 1);
        assert_eq!(
            bare(editor.drawing().entities().next().unwrap()),
            &Entity::Line {
                start: Point { x: 8.0, y: 9.0 },
                end: Point { x: 10.0, y: 11.0 },
            }
        );
        editor.submit("UNDO").unwrap();
        assert_eq!(editor.drawing(), &before);
    }

    #[test]
    fn star_insert_keeps_load_records_and_moves_shape_geometry() {
        let mut editor = Editor::default();
        editor
            .drawing_mut()
            .items
            .push(Item::Block(acad_model::Block {
                name: "SYMBOL".into(),
                base: Point { x: 1.0, y: 2.0 },
                entities: vec![
                    Entity::Load { name: "ES".into() },
                    Entity::Shape {
                        origin: Point { x: 2.0, y: 3.0 },
                        height: 1.0,
                        rotation_deg: 0.0,
                        number: 129,
                    },
                ],
            }));
        for input in ["INSERT", "*SYMBOL", "7,8"] {
            editor.submit(input).unwrap();
        }
        assert!(matches!(
            &editor.drawing().items[1],
            Item::Entity(Entity::Load { name }) if name == "ES"
        ));
        assert!(matches!(
            &editor.drawing().items[2],
            Item::Entity(Entity::Shape {
                origin: Point { x: 8.0, y: 9.0 },
                number: 129,
                ..
            })
        ));
    }

    #[test]
    fn insert_opposite_corner_sets_both_scales_and_skips_the_y_prompt() {
        let mut editor = Editor::default();
        for input in ["LINE", "2,3", "4,5", "", "BLOCK", "B1", "1,2", "LAST"] {
            editor.submit(input).unwrap();
        }
        for input in ["INSERT", "B1", "3,3", "5,6"] {
            editor.submit(input).unwrap();
        }
        assert_eq!(editor.prompt(), "INSERT: rotation angle (Enter for 0)");
        editor.submit("30").unwrap();
        assert_eq!(
            bare(editor.drawing().entities().next().unwrap()),
            &Entity::Insert {
                origin: Point { x: 3.0, y: 3.0 },
                x_scale: 2.0,
                y_scale: 3.0,
                rotation_deg: 30.0,
                name: "B1".into(),
            }
        );
    }

    #[test]
    fn three_point_arc_uses_middle_point_to_choose_ccw_sweep() {
        let mut editor = Editor::default();
        for input in ["ARC", "4,3", "3,4", "2,3"] {
            editor.submit(input).unwrap();
        }
        assert_eq!(
            bare_first(&editor),
            &Entity::Arc {
                center: Point { x: 3.0, y: 3.0 },
                radius: 1.0,
                start_deg: 0.0,
                end_deg: 180.0
            }
        );
    }

    #[test]
    fn malformed_geometry_does_not_mutate_the_drawing() {
        let mut editor = Editor::default();
        editor.submit("CIRCLE").unwrap();
        editor.submit("1,2").unwrap();
        assert!(editor.submit("-1").is_err());
        assert_eq!(editor.drawing.entities().count(), 0);
    }

    #[test]
    fn zoom_factor_changes_saved_view_height_without_moving_its_center() {
        let mut editor = Editor::default();
        let center = editor.drawing().header.view.center;
        editor.submit("ZOOM").unwrap();
        editor.submit("2").unwrap();
        assert_eq!(editor.drawing().header.view.height, 10.0);
        assert_eq!(editor.drawing().header.view.center, center);
    }

    #[test]
    fn zoom_previous_restores_the_prior_view() {
        let mut editor = Editor::default();
        let original = editor.drawing().header.view;
        editor.submit("ZOOM").unwrap();
        editor.submit("2").unwrap();
        editor.submit("ZOOM").unwrap();
        editor.submit("P").unwrap();
        assert_eq!(editor.drawing().header.view, original);
    }

    #[test]
    fn zoom_extents_centers_on_the_drawing_and_previous_restores_the_view() {
        let mut editor = Editor::default();
        for input in ["LINE", "-4,2", "6,10", ""] {
            editor.submit(input).unwrap();
        }
        editor.submit("ZOOM").unwrap();
        editor.submit("2").unwrap();
        let zoomed = editor.drawing().header.view;
        editor.submit("ZOOM").unwrap();
        editor.submit("E").unwrap();
        assert_eq!(
            editor.drawing().header.view.center,
            Point { x: 1.0, y: 6.0 }
        );
        assert_eq!(editor.drawing().header.view.height, 10.0);
        editor.submit("ZOOM").unwrap();
        editor.submit("P").unwrap();
        assert_eq!(editor.drawing().header.view, zoomed);
    }

    #[test]
    fn zoom_window_and_center_set_views_and_reject_empty_extents() {
        let mut editor = Editor::default();
        for input in ["ZOOM", "W", "0,0", "10,5"] {
            editor.submit(input).unwrap();
        }
        assert_eq!(
            editor.drawing().header.view.center,
            Point { x: 5.0, y: 2.5 }
        );
        assert_eq!(editor.drawing().header.view.height, 10.0);
        for input in ["ZOOM", "C", "-2,3", "20"] {
            editor.submit(input).unwrap();
        }
        assert_eq!(
            editor.drawing().header.view.center,
            Point { x: -2.0, y: 3.0 }
        );
        assert_eq!(editor.drawing().header.view.height, 20.0);
        editor.submit("ZOOM").unwrap();
        editor.submit("W").unwrap();
        editor.submit("1,1").unwrap();
        assert!(editor.submit("1,4").is_err());
        assert_eq!(editor.prompt(), "ZOOM WINDOW: upper-right");
    }

    #[test]
    fn pan_changes_view_center_without_changing_height_and_previous_restores_it() {
        let mut editor = Editor::default();
        editor.submit("ZOOM").unwrap();
        editor.submit("2").unwrap();
        let before_pan = editor.drawing().header.view;
        editor.submit("PAN").unwrap();
        editor.submit("12,-4").unwrap();
        assert_eq!(
            editor.drawing().header.view.center,
            Point { x: 12.0, y: -4.0 }
        );
        assert_eq!(editor.drawing().header.view.height, before_pan.height);
        editor.submit("ZOOM").unwrap();
        editor.submit("P").unwrap();
        assert_eq!(editor.drawing().header.view, before_pan);
    }

    #[test]
    fn list_erase_and_undo_use_stable_one_based_entity_selection() {
        let mut editor = Editor::default();
        for input in ["POINT", "1,2", "CIRCLE", "4,5", "2"] {
            editor.submit(input).unwrap();
        }
        editor.submit("LIST").unwrap();
        assert_eq!(editor.status(), "1 POINT, 2 CIRCLE");
        editor.submit("ERASE").unwrap();
        assert_eq!(editor.prompt(), "ERASE: entity numbers or ALL");
        editor.submit("1").unwrap();
        assert_eq!(editor.drawing().entities().count(), 1);
        assert!(matches!(bare_first(&editor), Entity::Circle { .. }));
        assert_eq!(editor.status(), "Erased 1 entities");
        editor.submit("UNDO").unwrap();
        assert_eq!(editor.drawing().entities().count(), 2);
        assert!(matches!(bare_first(&editor), Entity::Point { .. }));
    }

    #[test]
    fn dblist_reports_live_top_level_block_and_repeat_entities_without_editing() {
        let mut editor = Editor::default();
        editor.drawing_mut().items = vec![
            Item::Entity(Entity::Line {
                start: Point { x: 1.0, y: 2.0 },
                end: Point { x: 3.0, y: 4.0 },
            }),
            Item::Erased(Entity::Point {
                origin: Point { x: 9.0, y: 9.0 },
            }),
            Item::Block(acad_model::Block {
                name: "B1".into(),
                base: Point { x: 0.0, y: 0.0 },
                entities: vec![Entity::Circle {
                    center: Point { x: 5.0, y: 6.0 },
                    radius: 2.0,
                }],
            }),
            Item::Repeat(acad_model::Repeat {
                entities: vec![Entity::Point {
                    origin: Point { x: 7.0, y: 8.0 },
                }],
                columns: 2,
                rows: 1,
                column_spacing: 10.0,
                row_spacing: 0.0,
            }),
        ];
        let source = editor.drawing().clone();

        let Effect::Report(report) = editor.submit("DBLIST").unwrap() else {
            panic!("DBLIST should return a read-only report");
        };

        assert!(report.contains("Entity 1: Line"));
        assert!(report.contains("Entity 2: Circle"));
        assert!(report.contains("Entity 3: Point"));
        assert!(report.contains("Block B1"));
        assert!(report.contains("Repeat 2 columns × 1 rows"));
        assert!(!report.contains("9.0"), "erased record leaked into DBLIST");
        assert_eq!(editor.drawing(), &source, "DBLIST must be read-only");
        assert_eq!(editor.prompt(), "Command");
    }

    #[test]
    fn question_mark_and_help_share_the_observed_command_query_flow() {
        let mut editor = Editor::default();
        let source = editor.drawing().clone();

        editor.submit("?").unwrap();
        assert_eq!(editor.prompt(), "Command name (RETURN for list)");
        let Effect::Report(list) = editor.submit("").unwrap() else {
            panic!("blank ? query should display the command list");
        };
        assert!(list.contains("Command List"));
        assert!(list.contains("LINE"));
        assert!(list.contains("WBLOCK"));
        assert_eq!(editor.prompt(), "Command");

        editor.submit("HELP").unwrap();
        assert_eq!(editor.prompt(), "Command name (RETURN for list)");
        let Effect::Report(line) = editor.submit("LINE").unwrap() else {
            panic!("HELP LINE should display command help");
        };
        assert_eq!(line, LINE_HELP);
        assert_eq!(editor.status(), "Help for LINE");
        assert_eq!(editor.drawing(), &source);

        editor.submit("HELP").unwrap();
        assert!(editor.submit("CIRCLE").is_err());
        assert_eq!(editor.prompt(), "Command name (RETURN for list)");
    }

    #[test]
    fn menu_filename_can_be_cancelled_and_files_displays_the_utility_menu() {
        let mut editor = Editor::default();
        editor.submit("MENU").unwrap();
        assert_eq!(editor.prompt(), "File name");
        editor.submit("").unwrap();
        assert_eq!(editor.prompt(), "Command");

        let Effect::Report(menu) = editor.submit("FILES").unwrap() else {
            panic!("FILES should display its utility menu");
        };
        assert!(menu.contains("File Utility Menu"));
        assert!(menu.contains("List Drawing files"));
        assert!(menu.contains("Rename files"));
        assert_eq!(editor.status(), "File Utility Menu");

        editor.submit("0").unwrap();
        editor.submit("MENU").unwrap();
        assert_eq!(
            editor.submit("ACAD.MNU").unwrap(),
            Effect::LoadMenu("ACAD.MNU".into())
        );
    }

    #[test]
    fn files_dialog_builds_list_delete_and_rename_requests() {
        let mut editor = Editor::default();
        let Effect::Report(menu) = editor.submit("FILES").unwrap() else {
            panic!("FILES should display the utility menu");
        };
        assert!(menu.contains("List Drawing files"));
        assert_eq!(editor.prompt(), "FILES: selection (0–7)");

        editor.submit("1").unwrap();
        assert_eq!(editor.prompt(), "FILES: drive letter (A–Z)");
        assert_eq!(
            editor.submit("b").unwrap(),
            Effect::Files(FilesRequest::ListDrive {
                filter: FilesFilter::Drawings,
                drive: 'B',
            })
        );

        editor.submit("6").unwrap();
        assert_eq!(editor.prompt(), "FILES: file deletion specification");
        assert_eq!(
            editor.submit("B:*.DWG").unwrap(),
            Effect::Files(FilesRequest::Delete("B:*.DWG".into()))
        );

        editor.submit("7").unwrap();
        editor.submit("B:OLD.DWG").unwrap();
        assert_eq!(editor.prompt(), "FILES: new filename");
        assert_eq!(
            editor.submit("B:NEW.DWG").unwrap(),
            Effect::Files(FilesRequest::Rename {
                source: "B:OLD.DWG".into(),
                destination: "B:NEW.DWG".into(),
            })
        );
        assert_eq!(editor.prompt(), "FILES: selection (0–7)");
        editor.submit("0").unwrap();
        assert_eq!(editor.prompt(), "Command");
    }

    #[test]
    fn dim_creates_dimension_geometry_after_the_observed_point_prompt_sequence() {
        let mut editor = Editor::default();
        let before = editor.drawing().clone();
        for (input, prompt) in [
            ("DIM", "DIM: first extension line origin or (ABCT)"),
            ("1,1", "DIM: dimension line intersection"),
            ("5,1", "DIM: second extension line origin"),
            ("3,2", "DIM: dimension text"),
        ] {
            editor.submit(input).unwrap();
            assert_eq!(editor.prompt(), prompt);
        }
        editor.submit("").unwrap();
        assert_eq!(editor.prompt(), "Command");
        assert_ne!(editor.drawing(), &before);
        assert_eq!(editor.drawing().items.len(), 7);
        let entities = editor
            .drawing()
            .items
            .iter()
            .map(|item| match item {
                Item::Entity(Entity::OnLayer { entity, .. }) => entity.as_ref(),
                other => panic!("unexpected DIM output: {other:?}"),
            })
            .collect::<Vec<_>>();
        assert!(matches!(entities[0], Entity::Line { start, end }
            if *start == Point { x: 1.0, y: 1.0 }
                && *end == Point { x: 5.140625, y: 1.0 }));
        assert!(matches!(entities[1], Entity::Line { start, end }
            if *start == Point { x: 3.0, y: 2.0 }
                && *end == Point { x: 5.140625, y: 2.0 }));
        assert!(matches!(entities[4], Entity::Solid { p3, p4, .. }
            if *p3 == Point { x: 5.0, y: 2.0 } && *p4 == *p3));
        assert!(matches!(entities[6], Entity::Text { value, height, .. }
            if value == "1.0000" && *height == 0.2109375));
    }

    #[test]
    fn hatch_lists_patterns_and_creates_clipped_line_pattern_blocks() {
        let mut editor = Editor::default();
        editor.submit("HATCH").unwrap();
        assert_eq!(editor.prompt(), "HATCH: pattern (name,style / U / ?)");
        let Effect::Report(patterns) = editor.submit("?").unwrap() else {
            panic!("HATCH ? should show its pattern list");
        };
        assert!(patterns.contains("LINE            - Parallel horizontal lines"));
        assert!(patterns.contains("ZIGZAG          - Staircase effect"));
        assert_eq!(editor.prompt(), "Command");

        editor.submit("HATCH").unwrap();
        assert!(editor
            .submit("ANSI31")
            .unwrap_err()
            .contains("unknown HATCH pattern"));
        assert_eq!(editor.prompt(), "Command");

        for input in ["LINE", "1,1", "5,1", "5,5", "1,5", "1,1", ""] {
            editor.submit(input).unwrap();
        }

        editor.submit("HATCH").unwrap();
        editor.submit("LINE").unwrap();
        assert_eq!(editor.prompt(), "HATCH: scale for pattern {1}");
        editor.submit("").unwrap();
        assert_eq!(editor.prompt(), "HATCH: angle for pattern {0}");
        editor.submit("").unwrap();
        assert_eq!(editor.prompt(), "HATCH: select objects on Window or Last");
        assert!(editor.accepts_mouse_selection());
        editor.submit("W").unwrap();
        assert_eq!(editor.prompt(), "HATCH: lower left corner");
        assert!(editor.accepts_mouse_point());
        editor.submit("0,0").unwrap();
        assert_eq!(editor.prompt(), "HATCH: upper right corner");
        editor.submit("6,6").unwrap();
        assert_eq!(editor.prompt(), "Command");
        let Item::Block(hatch) = &editor.drawing().items[4] else {
            panic!("HATCH should store its clipped pattern in a block");
        };
        assert_eq!(hatch.name, "*X1");
        assert_eq!(hatch.entities.len(), 32);
        let Entity::OnLayer { layer: 127, entity } = &hatch.entities[0] else {
            panic!("pattern strokes use layer 127");
        };
        assert!(matches!(entity.as_ref(), Entity::Line { start, end }
            if *start == Point { x: 1.0, y: 3.0 } && *end == Point { x: 5.0, y: 3.0 }));
        assert!(
            matches!(&editor.drawing().items[5], Item::Entity(Entity::OnLayer {
            layer: 1,
            entity,
        }) if matches!(entity.as_ref(), Entity::Insert { name, .. } if name == "*X1"))
        );

        editor.drawing_mut().items.clear();
        editor.submit("SKETCH").unwrap();
        assert_eq!(editor.prompt(), "SKETCH: record increment");
        assert!(editor.submit("0").unwrap_err().contains("must be positive"));
        assert_eq!(editor.prompt(), "SKETCH: record increment");
        assert!(editor
            .submit("0.5")
            .unwrap_err()
            .contains("digitizer input device"));
        assert_eq!(editor.prompt(), "Command");
        assert!(editor.drawing().items.is_empty());
    }

    #[test]
    fn hatch_rejects_an_open_boundary_without_adding_pattern_geometry() {
        let mut editor = Editor::default();
        for input in [
            "LINE", "1,1", "5,1", "5,5", "1,5", "", "HATCH", "LINE", "", "", "W", "0,0",
        ] {
            editor.submit(input).unwrap();
        }
        let before = editor.drawing().items.clone();
        assert!(editor
            .submit("6,6")
            .unwrap_err()
            .contains("closed, unbranched loops"));
        assert_eq!(editor.drawing().items, before);
        assert_eq!(editor.prompt(), "HATCH: upper right corner");
    }

    #[test]
    fn hatch_line_pattern_clips_circle_chords_without_emitting_tangents() {
        let mut editor = Editor::default();
        for input in [
            "CIRCLE", "3,3", "2", "HATCH", "LINE", "", "", "W", "0,0", "6,6",
        ] {
            editor.submit(input).unwrap();
        }
        let Item::Block(hatch) = &editor.drawing().items[1] else {
            panic!("HATCH should store circle chords in a block");
        };
        assert_eq!(hatch.entities.len(), 31);
        assert!(matches!(&hatch.entities[0], Entity::OnLayer {
            layer: 127,
            entity,
        } if matches!(entity.as_ref(), Entity::Line { start, end }
            if start.x == 1.0 && start.y == 3.0 && end.x == 5.0 && end.y == 3.0)));
        assert!(matches!(&hatch.entities[1], Entity::OnLayer {
            layer: 127,
            entity,
        } if matches!(entity.as_ref(), Entity::Line { start, end }
            if start.y == 3.125 && end.y == 3.125 && start.x > 1.0 && end.x < 5.0)));
    }

    #[test]
    fn oops_restores_the_last_erased_items_in_place_and_undo_reverses_it() {
        let mut editor = Editor::default();
        editor.drawing_mut().items = vec![
            Item::Entity(Entity::Point {
                origin: Point { x: 1.0, y: 0.0 },
            }),
            Item::Block(acad_model::Block {
                name: "KEEP".into(),
                base: Point { x: 0.0, y: 0.0 },
                entities: vec![],
            }),
            Item::Entity(Entity::Point {
                origin: Point { x: 2.0, y: 0.0 },
            }),
            Item::Entity(Entity::Load { name: "TXT".into() }),
            Item::Entity(Entity::Point {
                origin: Point { x: 3.0, y: 0.0 },
            }),
        ];
        let before = editor.drawing().items.clone();
        editor.submit("ERASE").unwrap();
        editor.submit("1,3").unwrap();
        assert_eq!(editor.drawing().items.len(), 5);
        assert_eq!(editor.drawing().entities().count(), 2);
        assert!(matches!(editor.drawing().items[0], Item::Erased(_)));
        assert!(matches!(editor.drawing().items[4], Item::Erased(_)));

        editor.submit("OOPS").unwrap();
        assert_eq!(editor.drawing().items, before);
        assert_eq!(editor.status(), "Restored 2 erased entities");

        editor.submit("UNDO").unwrap();
        assert_eq!(editor.drawing().items.len(), 5);
        assert_eq!(editor.drawing().entities().count(), 2);
        editor.submit("OOPS").unwrap();
        assert_eq!(editor.drawing().items, before);
    }

    #[test]
    fn move_copy_rotate_scale_and_undo_transform_model_geometry() {
        let mut editor = Editor::default();
        for input in ["LINE", "1,0", "2,0", ""] {
            editor.submit(input).unwrap();
        }
        for input in ["MOVE", "0,0", "1,2", "1"] {
            editor.submit(input).unwrap();
        }
        assert_eq!(
            bare_first(&editor),
            &Entity::Line {
                start: Point { x: 2.0, y: 2.0 },
                end: Point { x: 3.0, y: 2.0 },
            }
        );
        for input in ["COPY", "0,0", "-1,0", "1"] {
            editor.submit(input).unwrap();
        }
        assert_eq!(editor.drawing().entities().count(), 2);
        for input in ["ROTATE", "1", "0,0", "90"] {
            editor.submit(input).unwrap();
        }
        let Entity::Line { start, end } = bare_first(&editor) else {
            panic!("expected the selected line");
        };
        assert!((start.x + 2.0).abs() < 1e-12);
        assert!((start.y - 2.0).abs() < 1e-12);
        assert!((end.x + 2.0).abs() < 1e-12);
        assert!((end.y - 3.0).abs() < 1e-12);
        for input in ["SCALE", "2", "0,0", "2"] {
            editor.submit(input).unwrap();
        }
        assert_eq!(
            bare(editor.drawing().entities().nth(1).unwrap()),
            &Entity::Line {
                start: Point { x: 2.0, y: 4.0 },
                end: Point { x: 4.0, y: 4.0 },
            }
        );
        editor.submit("UNDO").unwrap();
        assert_eq!(editor.drawing().entities().count(), 2);
        assert_eq!(
            bare(editor.drawing().entities().nth(1).unwrap()),
            &Entity::Line {
                start: Point { x: 1.0, y: 2.0 },
                end: Point { x: 2.0, y: 2.0 },
            }
        );
    }

    #[test]
    fn rectangular_array_copies_selected_geometry_by_signed_row_and_column_spacing() {
        let mut editor = Editor::default();
        for input in ["LINE", "0,0", "1,0", ""] {
            editor.submit(input).unwrap();
        }
        for input in ["ARRAY", "1", "R", "2", "3", "-5", "10"] {
            editor.submit(input).unwrap();
        }
        assert_eq!(editor.drawing().entities().count(), 6);
        let lines: Vec<_> = editor
            .drawing()
            .entities()
            .map(|entity| match bare(entity) {
                Entity::Line { start, end } => (*start, *end),
                other => panic!("expected array line, got {other:?}"),
            })
            .collect();
        assert_eq!(
            lines,
            [
                (Point { x: 0.0, y: 0.0 }, Point { x: 1.0, y: 0.0 }),
                (Point { x: 0.0, y: -5.0 }, Point { x: 1.0, y: -5.0 }),
                (Point { x: 10.0, y: 0.0 }, Point { x: 11.0, y: 0.0 }),
                (Point { x: 10.0, y: -5.0 }, Point { x: 11.0, y: -5.0 }),
                (Point { x: 20.0, y: 0.0 }, Point { x: 21.0, y: 0.0 }),
                (Point { x: 20.0, y: -5.0 }, Point { x: 21.0, y: -5.0 }),
            ]
        );
        assert_eq!(editor.status(), "Created 2x3 array with 5 copies");
        editor.submit("UNDO").unwrap();
        assert_eq!(editor.drawing().entities().count(), 1);
    }

    #[test]
    fn array_rejects_zero_dimensions_and_overflow_before_creating_copies() {
        let mut editor = Editor::default();
        for input in ["POINT", "2,3", "ARRAY", "ALL", "R"] {
            editor.submit(input).unwrap();
        }
        assert!(editor.submit("0").is_err());
        assert_eq!(editor.prompt(), "ARRAY: number of rows");
        editor.submit("2").unwrap();
        assert!(editor.submit(usize::MAX.to_string().as_str()).is_err());
        assert!(editor.submit("100001").is_err());
        assert_eq!(editor.prompt(), "ARRAY: number of columns");
        assert_eq!(editor.drawing().entities().count(), 1);
    }

    #[test]
    fn change_moves_selected_entities_to_a_layer_and_undo_restores_header_and_geometry() {
        let mut editor = Editor::default();
        for input in ["POINT", "1,2", "LAYER", "2", "POINT", "3,4"] {
            editor.submit(input).unwrap();
        }
        let before: Vec<_> = editor.drawing().entities().cloned().collect();
        for input in ["CHANGE", "1", "L", "7"] {
            editor.submit(input).unwrap();
        }
        let changed: Vec<_> = editor.drawing().entities().cloned().collect();
        assert_eq!(bare(&changed[0]), bare(&before[0]));
        assert_eq!(layer_of(&changed[0]), 7);
        assert_eq!(changed[1], before[1]);
        assert_eq!(editor.drawing().header.layers.get(&7), Some(&15));
        assert_eq!(editor.status(), "Changed 1 entities to layer 7");

        editor.submit("UNDO").unwrap();
        assert_eq!(
            editor.drawing().entities().cloned().collect::<Vec<_>>(),
            before
        );
        assert!(!editor.drawing().header.layers.contains_key(&7));
    }

    #[test]
    fn change_point_moves_the_nearest_line_endpoint_and_undo_restores_it() {
        let mut editor = Editor::default();
        for input in ["LINE", "1,1", "2,1", "", "CHANGE", "1", "3,4"] {
            editor.submit(input).unwrap();
        }
        assert_eq!(
            editor.drawing().entities().next().unwrap(),
            &Entity::OnLayer {
                layer: 1,
                entity: Box::new(Entity::Line {
                    start: Point { x: 1.0, y: 1.0 },
                    end: Point { x: 3.0, y: 4.0 },
                }),
            }
        );
        editor.submit("UNDO").unwrap();
        assert_eq!(
            editor.drawing().entities().next().unwrap(),
            &Entity::OnLayer {
                layer: 1,
                entity: Box::new(Entity::Line {
                    start: Point { x: 1.0, y: 1.0 },
                    end: Point { x: 2.0, y: 1.0 },
                }),
            }
        );
    }

    #[test]
    fn fillet_trims_two_intersecting_lines_and_adds_the_tangent_arc() {
        let mut editor = Editor::default();
        for input in [
            "LINE", "-5,0", "5,0", "", "LINE", "0,-5", "0,5", "", "FILLET", "1,2", "1",
        ] {
            editor.submit(input).unwrap();
        }
        let mut entities = editor.drawing().entities();
        let Entity::OnLayer { entity, .. } = entities.next().unwrap() else {
            panic!("expected layered line");
        };
        assert!(
            matches!(entity.as_ref(), Entity::Line { start: Point { x, y: 0.0 }, end: Point { x: 5.0, y: 0.0 } } if (*x - 1.0).abs() < 1e-10)
        );
        let Entity::OnLayer { entity, .. } = entities.next().unwrap() else {
            panic!("expected layered line");
        };
        assert!(
            matches!(entity.as_ref(), Entity::Line { start: Point { x: 0.0, y: -5.0 }, end: Point { x: 0.0, y, } } if (*y + 1.0).abs() < 1e-10)
        );
        let Entity::OnLayer { entity, .. } = entities.next().unwrap() else {
            panic!("expected layered arc");
        };
        let Entity::Arc {
            center,
            radius,
            start_deg,
            end_deg,
        } = entity.as_ref()
        else {
            panic!("expected fillet arc");
        };
        assert!((center.x - 1.0).abs() < 1e-10);
        assert!((center.y + 1.0).abs() < 1e-10);
        assert_eq!(*radius, 1.0);
        assert!((start_deg - 90.0).abs() < 1e-10, "start={start_deg}");
        assert!((end_deg - 180.0).abs() < 1e-10, "end={end_deg}");
        assert_eq!(editor.status(), "Filleted two lines");

        drop(entities);
        editor.submit("UNDO").unwrap();
        assert_eq!(editor.drawing().entities().count(), 2);
        assert_eq!(
            bare(editor.drawing().entities().next().unwrap()),
            &Entity::Line {
                start: Point { x: -5.0, y: 0.0 },
                end: Point { x: 5.0, y: 0.0 },
            }
        );
    }

    #[test]
    fn fillet_rejects_parallel_lines_and_excessive_radius_without_mutation() {
        let mut editor = Editor::default();
        for input in ["LINE", "0,0", "10,0", "", "LINE", "0,1", "10,1", ""] {
            editor.submit(input).unwrap();
        }
        let original = editor.drawing().items.clone();
        for input in ["FILLET", "1,2"] {
            editor.submit(input).unwrap();
        }
        assert!(editor.submit("1").is_err());
        assert_eq!(editor.drawing().items, original);
        assert_eq!(editor.prompt(), "FILLET: radius");

        editor = Editor::default();
        for input in ["LINE", "-5,0", "5,0", "", "LINE", "0,-5", "0,5", ""] {
            editor.submit(input).unwrap();
        }
        editor.submit("FILLET").unwrap();
        editor.submit("1,2").unwrap();
        assert!(editor.submit("100").is_err());
        assert_eq!(editor.drawing().items.len(), 2);
    }

    #[test]
    fn break_removes_the_segment_between_two_points_and_preserves_layer() {
        let mut editor = Editor::default();
        for input in [
            "LAYER", "7", "LINE", "0,0", "10,0", "", "BREAK", "1", "7,0", "2,0",
        ] {
            editor.submit(input).unwrap();
        }
        let pieces: Vec<_> = editor.drawing().entities().cloned().collect();
        assert_eq!(pieces.len(), 2);
        assert_eq!(layer_of(&pieces[0]), 7);
        assert_eq!(layer_of(&pieces[1]), 7);
        assert_eq!(
            bare(&pieces[0]),
            &Entity::Line {
                start: Point { x: 0.0, y: 0.0 },
                end: Point { x: 2.0, y: 0.0 },
            }
        );
        assert_eq!(
            bare(&pieces[1]),
            &Entity::Line {
                start: Point { x: 7.0, y: 0.0 },
                end: Point { x: 10.0, y: 0.0 },
            }
        );
        editor.submit("UNDO").unwrap();
        assert_eq!(editor.drawing().entities().count(), 1);
        assert_eq!(layer_of(editor.drawing().entities().next().unwrap()), 7);
    }

    #[test]
    fn break_rejects_points_off_the_line_or_at_an_endpoint_without_mutation() {
        let mut editor = Editor::default();
        for input in ["LINE", "0,0", "10,0", ""] {
            editor.submit(input).unwrap();
        }
        let original = editor.drawing().items.clone();
        for input in ["BREAK", "1", "2,1"] {
            editor.submit(input).unwrap();
        }
        assert!(editor.submit("7,0").is_err());
        assert_eq!(editor.drawing().items, original);
        assert_eq!(editor.prompt(), "BREAK: second point");

        let mut endpoint_editor = Editor::default();
        for input in ["LINE", "0,0", "10,0", ""] {
            endpoint_editor.submit(input).unwrap();
        }
        let endpoint_original = endpoint_editor.drawing().items.clone();
        for input in ["BREAK", "1", "0,0"] {
            endpoint_editor.submit(input).unwrap();
        }
        assert!(endpoint_editor.submit("5,0").is_err());
        assert_eq!(endpoint_editor.drawing().items, endpoint_original);
    }

    #[test]
    fn break_circle_retains_the_complementary_arc_across_zero_degrees() {
        let mut editor = Editor::default();
        for input in ["CIRCLE", "0,0", "5"] {
            editor.submit(input).unwrap();
        }
        let point_at = |degrees: f64| {
            let radians = degrees.to_radians();
            format!("{:.12},{:.12}", 5.0 * radians.cos(), 5.0 * radians.sin())
        };
        for input in ["BREAK", "1"] {
            editor.submit(input).unwrap();
        }
        editor.submit(&point_at(350.0)).unwrap();
        editor.submit(&point_at(10.0)).unwrap();
        let Entity::OnLayer { entity, .. } = editor.drawing().entities().next().unwrap() else {
            panic!("expected layered arc");
        };
        let Entity::Arc {
            center,
            radius,
            start_deg,
            end_deg,
        } = entity.as_ref()
        else {
            panic!("BREAK should turn the retained circle into one arc");
        };
        assert_eq!(*center, Point { x: 0.0, y: 0.0 });
        assert_eq!(*radius, 5.0);
        assert!((*start_deg - 10.0).abs() < 1e-8);
        assert!((*end_deg - 350.0).abs() < 1e-8);
        editor.submit("UNDO").unwrap();
        assert!(matches!(
            bare(editor.drawing().entities().next().unwrap()),
            Entity::Circle { radius: 5.0, .. }
        ));
    }

    #[test]
    fn break_circle_rejects_points_inside_the_circumference() {
        let mut editor = Editor::default();
        for input in ["CIRCLE", "1,2", "3", "BREAK", "1", "4,2"] {
            editor.submit(input).unwrap();
        }
        let original = editor.drawing().items.clone();
        assert!(editor.submit("1,4").is_err());
        assert_eq!(editor.drawing().items, original);
        assert_eq!(editor.prompt(), "BREAK: second point");
    }

    #[test]
    fn break_arc_creates_two_arcs_when_the_source_sweep_crosses_zero() {
        let mut editor = Editor::default();
        editor
            .drawing_mut()
            .items
            .push(Item::Entity(Entity::OnLayer {
                layer: 4,
                entity: Box::new(Entity::Arc {
                    center: Point { x: 0.0, y: 0.0 },
                    radius: 5.0,
                    start_deg: 300.0,
                    end_deg: 60.0,
                }),
            }));
        let point_at = |degrees: f64| {
            let radians = degrees.to_radians();
            format!("{:.12},{:.12}", 5.0 * radians.cos(), 5.0 * radians.sin())
        };
        for input in ["BREAK", "1"] {
            editor.submit(input).unwrap();
        }
        editor.submit(&point_at(330.0)).unwrap();
        editor.submit(&point_at(30.0)).unwrap();

        let arcs: Vec<_> = editor
            .drawing()
            .entities()
            .map(|entity| {
                assert_eq!(layer_of(entity), 4);
                let Entity::Arc {
                    start_deg, end_deg, ..
                } = bare(entity)
                else {
                    panic!("BREAK should preserve both sides as arcs");
                };
                (*start_deg, *end_deg)
            })
            .collect();
        assert_eq!(arcs.len(), 2);
        assert!((arcs[0].0 - 300.0).abs() < 1e-8);
        assert!((arcs[0].1 - 330.0).abs() < 1e-8);
        assert!((arcs[1].0 - 30.0).abs() < 1e-8);
        assert!((arcs[1].1 - 60.0).abs() < 1e-8);

        editor.submit("UNDO").unwrap();
        let Entity::Arc {
            start_deg, end_deg, ..
        } = bare(editor.drawing().entities().next().unwrap())
        else {
            panic!("undo should restore the source arc");
        };
        assert_eq!((*start_deg, *end_deg), (300.0, 60.0));
    }

    #[test]
    fn break_arc_rejects_a_point_outside_its_sweep_without_mutation() {
        let mut editor = Editor::default();
        editor.drawing_mut().items.push(Item::Entity(Entity::Arc {
            center: Point { x: 0.0, y: 0.0 },
            radius: 2.0,
            start_deg: 20.0,
            end_deg: 160.0,
        }));
        for input in ["BREAK", "1", "0,2"] {
            editor.submit(input).unwrap();
        }
        let original = editor.drawing().items.clone();
        assert!(editor.submit("-2,0").is_err());
        assert_eq!(editor.drawing().items, original);
    }

    #[test]
    fn distance_reports_original_four_decimal_length() {
        let mut editor = Editor::default();
        for input in ["DIST", "1,2", "4,6"] {
            editor.submit(input).unwrap();
        }
        assert_eq!(editor.status(), "Distance=5.0000");
        assert_eq!(editor.drawing().entities().count(), 0);
        assert_eq!(editor.prompt(), "Command");
    }

    #[test]
    fn id_reports_coordinates_without_changing_the_drawing() {
        let mut editor = Editor::default();
        let before = editor.drawing().clone();
        editor.submit("ID").unwrap();
        assert_eq!(editor.prompt(), "ID: point");
        editor.submit("3,4").unwrap();
        assert_eq!(editor.status(), "X = 3.0000    Y = 4.0000");
        assert_eq!(editor.prompt(), "Command");
        assert_eq!(editor.drawing(), &before);
    }

    #[test]
    fn block_moves_last_entity_into_a_named_definition_and_undo_restores_it() {
        let mut editor = Editor::default();
        for input in ["LINE", "2,3", "4,5", "", "CIRCLE", "5,6", "1"] {
            editor.submit(input).unwrap();
        }
        let before = editor.drawing().clone();
        for input in ["BLOCK", "b1", "1,2", "LAST"] {
            editor.submit(input).unwrap();
        }
        assert_eq!(editor.prompt(), "Command");
        assert_eq!(editor.drawing().entities().count(), 1);
        let block = editor.drawing().block("B1").unwrap();
        assert_eq!(block.base, Point { x: 1.0, y: 2.0 });
        assert_eq!(block.entities.len(), 1);
        assert!(matches!(bare(&block.entities[0]), Entity::Circle { .. }));
        editor.submit("UNDO").unwrap();
        assert_eq!(editor.drawing(), &before);
    }

    #[test]
    fn block_rejects_duplicate_name_and_invalid_selection_without_mutation() {
        let mut editor = Editor::default();
        for input in ["POINT", "1,2", "BLOCK", "SYMBOL", "0,0", "LAST"] {
            editor.submit(input).unwrap();
        }
        let after_first = editor.drawing().clone();
        editor.submit("BLOCK").unwrap();
        assert!(editor.submit("symbol").is_err());
        assert_eq!(editor.drawing(), &after_first);
        editor.submit("OTHER").unwrap();
        editor.submit("0,0").unwrap();
        assert!(editor.submit("LAST").is_err());
        assert_eq!(editor.drawing(), &after_first);
    }

    #[test]
    fn solid_chains_the_prior_third_and_fourth_points() {
        let mut editor = Editor::default();
        for input in ["SOLID", "1,1", "4,1", "1,3", "4,3", "1,5", "4,5", ""] {
            editor.submit(input).unwrap();
        }
        let solids: Vec<_> = editor.drawing().entities().cloned().collect();
        assert_eq!(
            solids,
            [
                Entity::OnLayer {
                    layer: 1,
                    entity: Box::new(Entity::Solid {
                        p1: Point { x: 1.0, y: 1.0 },
                        p2: Point { x: 4.0, y: 1.0 },
                        p3: Point { x: 1.0, y: 3.0 },
                        p4: Point { x: 4.0, y: 3.0 },
                    }),
                },
                Entity::OnLayer {
                    layer: 1,
                    entity: Box::new(Entity::Solid {
                        p1: Point { x: 1.0, y: 3.0 },
                        p2: Point { x: 4.0, y: 3.0 },
                        p3: Point { x: 1.0, y: 5.0 },
                        p4: Point { x: 4.0, y: 5.0 },
                    }),
                },
            ]
        );
        assert_eq!(editor.prompt(), "Command");
    }

    #[test]
    fn trace_miters_a_bend_and_updates_saved_width() {
        let mut editor = Editor::default();
        for input in ["TRACE", "0.5", "1,1", "4,1", "4,4", ""] {
            editor.submit(input).unwrap();
        }
        assert_eq!(editor.drawing().header.trace_width, 0.5);
        let traces: Vec<_> = editor.drawing().entities().cloned().collect();
        assert_eq!(
            traces,
            [
                Entity::OnLayer {
                    layer: 1,
                    entity: Box::new(Entity::Trace {
                        p1: Point { x: 1.0, y: 1.25 },
                        p2: Point { x: 1.0, y: 0.75 },
                        p3: Point { x: 3.75, y: 1.25 },
                        p4: Point { x: 4.25, y: 0.75 },
                    }),
                },
                Entity::OnLayer {
                    layer: 1,
                    entity: Box::new(Entity::Trace {
                        p1: Point { x: 3.75, y: 1.25 },
                        p2: Point { x: 4.25, y: 0.75 },
                        p3: Point { x: 3.75, y: 4.0 },
                        p4: Point { x: 4.25, y: 4.0 },
                    }),
                },
            ]
        );
        assert_eq!(editor.prompt(), "Command");
    }

    #[test]
    fn area_follows_original_point_sequence_and_reports_four_decimals() {
        let mut editor = Editor::default();
        for input in ["AREA", "0,0", "4,0", "4,3"] {
            editor.submit(input).unwrap();
        }
        assert_eq!(editor.prompt(), "AREA: next point (Enter to finish)");
        editor.submit("").unwrap();
        assert_eq!(editor.status(), "Area = 6.0000");
        assert_eq!(editor.prompt(), "Command");
        assert!(editor.drawing().items.is_empty());
    }

    #[test]
    fn entity_area_measures_circles_and_reordered_closed_line_loops() {
        let mut circle_editor = Editor::default();
        for input in ["CIRCLE", "1,2", "3", "ENTITYAREA", "1"] {
            circle_editor.submit(input).unwrap();
        }
        assert_eq!(
            circle_editor.status(),
            "Area=28.274334, Perimeter=18.849556"
        );

        let mut polygon = Editor::default();
        for (start, end) in [
            ("4,3", "4,0"),
            ("0,0", "4,0"),
            ("0,3", "0,0"),
            ("4,3", "0,3"),
        ] {
            for input in ["LINE", start, end, ""] {
                polygon.submit(input).unwrap();
            }
        }
        polygon.submit("ENTITYAREA").unwrap();
        polygon.submit("ALL").unwrap();
        assert_eq!(polygon.status(), "Area=12.000000, Perimeter=14.000000");
    }

    #[test]
    fn entity_area_rejects_an_open_line_selection_without_mutating_it() {
        let mut editor = Editor::default();
        for (start, end) in [("0,0", "4,0"), ("4,0", "4,3")] {
            for input in ["LINE", start, end, ""] {
                editor.submit(input).unwrap();
            }
        }
        let original = editor.drawing().items.clone();
        editor.submit("ENTITYAREA").unwrap();
        assert!(editor.submit("ALL").is_err());
        assert_eq!(editor.drawing().items, original);
        assert_eq!(editor.prompt(), "ENTITYAREA: entity numbers or ALL");
    }

    #[test]
    fn wblock_star_exports_live_entities_and_reachable_blocks_without_changing_source() {
        let mut editor = Editor::default();
        for input in [
            "LINE",
            "0,0",
            "1,0",
            "",
            "BLOCK",
            "USED",
            "0,0",
            "LAST",
            "LINE",
            "2,0",
            "3,0",
            "",
            "BLOCK",
            "UNUSED",
            "2,0",
            "LAST",
            "INSERT",
            "USED",
            "5,5",
            "",
            "",
            "",
            "",
            "LINE",
            "4,0",
            "5,0",
            "",
            "BLOCK",
            "ERASEDONLY",
            "4,0",
            "LAST",
            "INSERT",
            "ERASEDONLY",
            "7,7",
            "",
            "",
            "",
            "ERASE",
            "LAST",
        ] {
            editor.submit(input).unwrap();
        }
        let source = editor.drawing().clone();
        editor.submit("WBLOCK").unwrap();
        assert_eq!(editor.prompt(), "WBLOCK: output file name");
        editor.submit("whole").unwrap();
        assert_eq!(editor.prompt(), "WBLOCK: block name (* for entire drawing)");
        let effect = editor.submit("*").unwrap();
        let Effect::SaveDrawing(path, exported) = effect else {
            panic!("expected WBLOCK to return a separate drawing snapshot");
        };
        assert_eq!(path, "whole.DWG");
        assert_eq!(editor.drawing(), &source, "WBLOCK must not edit the source");
        assert_eq!(exported.header, source.header);
        assert_eq!(
            exported.items,
            vec![
                Item::Block(acad_model::Block {
                    name: "USED".into(),
                    base: Point { x: 0.0, y: 0.0 },
                    entities: vec![Entity::OnLayer {
                        layer: 1,
                        entity: Box::new(Entity::Line {
                            start: Point { x: 0.0, y: 0.0 },
                            end: Point { x: 1.0, y: 0.0 },
                        }),
                    }],
                }),
                Item::Entity(Entity::OnLayer {
                    layer: 1,
                    entity: Box::new(Entity::Insert {
                        origin: Point { x: 5.0, y: 5.0 },
                        x_scale: 1.0,
                        y_scale: 1.0,
                        rotation_deg: 0.0,
                        name: "USED".into(),
                    }),
                }),
            ]
        );
    }

    #[test]
    fn wblock_blank_block_name_exports_only_selected_entities_and_base_point() {
        let mut editor = Editor::default();
        for input in ["LINE", "0,0", "1,0", "", "LINE", "2,0", "3,0", ""] {
            editor.submit(input).unwrap();
        }
        let source = editor.drawing().clone();
        for (input, prompt) in [
            ("WBLOCK", "WBLOCK: output file name"),
            ("selected", "WBLOCK: block name (* for entire drawing)"),
            ("", "WBLOCK: insertion base point"),
            ("1,2", "WBLOCK: entity numbers, ALL, or LAST"),
        ] {
            editor.submit(input).unwrap();
            assert_eq!(editor.prompt(), prompt);
        }
        let effect = editor.submit("2").unwrap();
        let Effect::SaveDrawing(path, drawing) = effect else {
            panic!("selected WBLOCK should return a separate drawing snapshot");
        };
        assert_eq!(path, "selected.DWG");
        assert_eq!(drawing.header.base, Point { x: 1.0, y: 2.0 });
        assert_eq!(drawing.items, vec![source.items[1].clone()]);
        assert_eq!(editor.drawing(), &source, "WBLOCK must not edit the source");
    }

    #[test]
    fn resolution_aliases_share_snap_state_and_delay_and_resume_do_not_edit() {
        let mut editor = Editor::default();
        editor.submit("RES").unwrap();
        assert_eq!(editor.prompt(), "SNAP: spacing");
        editor.submit("2.5").unwrap();
        assert_eq!(
            editor.drawing().header.snap,
            acad_model::Mode {
                on: true,
                spacing: 2.5
            }
        );
        editor.submit("RESOLUTION").unwrap();
        editor.submit("OFF").unwrap();
        assert_eq!(
            editor.drawing().header.snap,
            acad_model::Mode {
                on: false,
                spacing: 2.5
            }
        );
        let before = editor.drawing().clone();
        editor.submit("DELAY").unwrap();
        assert_eq!(editor.prompt(), "DELAY: duration");
        editor.submit("0").unwrap();
        editor.submit("RESUME").unwrap();
        assert_eq!(editor.drawing(), &before);
        assert_eq!(editor.prompt(), "Command");
    }

    #[test]
    fn units_persists_the_native_choice_and_validates_its_precision() {
        let mut editor = Editor::default();
        editor.submit("UNITS").unwrap();
        assert_eq!(editor.prompt(), "UNITS: choice, 1 to 4");
        editor.submit("4").unwrap();
        assert_eq!(
            editor.prompt(),
            "UNITS: denominator (1, 2, 4, 8, 16, 32, or 64)"
        );
        editor.submit("16").unwrap();
        assert_eq!(
            editor.drawing().header.units,
            Units {
                format: UnitFormat::Architectural,
                precision: 16
            }
        );
        for input in ["DIST", "0,0", "1'-3 1/2\",0"] {
            editor.submit(input).unwrap();
        }
        assert_eq!(editor.status(), "Distance=1'-3 1/2\"");
        editor.submit("UNITS").unwrap();
        editor.submit("2").unwrap();
        assert!(editor.submit("9").is_err());
        assert_eq!(
            editor.prompt(),
            "UNITS: digits to right of decimal point (0 to 8)"
        );
        editor.submit("3").unwrap();
        assert_eq!(
            editor.drawing().header.units,
            Units {
                format: UnitFormat::Decimal,
                precision: 3
            }
        );
    }

    fn layer_of(entity: &Entity) -> u8 {
        match entity {
            Entity::OnLayer { layer, .. } => *layer,
            _ => 1,
        }
    }

    #[test]
    fn invalid_selection_does_not_change_the_drawing() {
        let mut editor = Editor::default();
        for input in ["POINT", "1,2", "ERASE"] {
            editor.submit(input).unwrap();
        }
        assert!(editor.submit("2").is_err());
        assert_eq!(editor.drawing().entities().count(), 1);
        assert_eq!(editor.prompt(), "ERASE: entity numbers or ALL");
    }

    #[test]
    fn editing_geometry_preserves_the_user_view_and_limits() {
        let mut editor = Editor::default();
        for input in ["LINE", "0,0", "10,0", ""] {
            editor.submit(input).unwrap();
        }
        for input in ["ZOOM", "2", "LIMITS", "-50,-40", "60,70"] {
            editor.submit(input).unwrap();
        }
        let view = editor.drawing().header.view;
        let limits = editor.drawing().header.limits;
        for input in ["MOVE", "0,0", "1,0", "ALL"] {
            editor.submit(input).unwrap();
        }
        assert_eq!(editor.drawing().header.view, view);
        assert_eq!(editor.drawing().header.limits, limits);
        assert_eq!(editor.drawing().header.extents.xmin, 1.0);
        assert_eq!(editor.drawing().header.extents.xmax, 11.0);
    }
}
