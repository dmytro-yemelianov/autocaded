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
