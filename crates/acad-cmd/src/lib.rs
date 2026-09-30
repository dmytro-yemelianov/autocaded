//! Interactive command state machine for the 1983 editor.
pub mod menu;

mod input_state;
mod parse;

use acad_model::{Block, Drawing, Entity, Extents, Header, Item, Point, UnitFormat, Units};
use input_state::{ArraySpacingInput, EditCommand, InputState, RepeatDistanceInput, Transform};
use parse::{
    color_index, format_measurement, layer_index, number, parse_mode, parse_toggle, point,
    point_from, positive_count, positive_word,
};
use std::collections::{BTreeMap, BTreeSet};

const MAX_ARRAY_ENTITIES: usize = 100_000;

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

    fn cancel(&mut self) -> Result<Effect, String> {
        self.state = InputState::Command;
        Ok(Effect::Continue)
    }

    fn add(&mut self, entity: Entity) {
        self.save_undo();
        self.drawing.items.push(Item::Entity(Entity::OnLayer {
            layer: self.drawing.header.current_layer,
            entity: Box::new(entity),
        }));
        self.refresh_limits();
    }

    fn ensure_layer(&mut self, layer: u8) {
        self.drawing.header.layers.entry(layer).or_insert(15);
    }

    fn set_view(&mut self, center: Point, height: f64) {
        self.previous_view = Some(self.drawing.header.view);
        self.drawing.header.view = acad_model::DwgView { center, height };
    }

    fn create_block(&mut self, name: String, base: Point, ids: &[usize]) {
        let selected = selected_item_indexes(&self.drawing, ids);
        self.save_undo();
        let mut retained = Vec::with_capacity(self.drawing.items.len() + 1);
        let mut entities = Vec::with_capacity(selected.len());
        for (index, item) in std::mem::take(&mut self.drawing.items)
            .into_iter()
            .enumerate()
        {
            if selected.contains(&(index + 1)) {
                let Item::Entity(entity) = item else {
                    unreachable!("only entities can be selected for BLOCK");
                };
                entities.push(entity.clone());
                retained.push(Item::Erased(entity));
            } else {
                retained.push(item);
            }
        }
        retained.push(Item::Block(acad_model::Block {
            name: name.clone(),
            base,
            entities,
        }));
        self.drawing.items = retained;
        self.refresh_after_edit();
        self.status = format!("Created block {name}");
    }

    fn wblock_entire_drawing(&self) -> Drawing {
        let mut referenced = BTreeSet::new();
        for item in &self.drawing.items {
            match item {
                Item::Entity(entity) => collect_insert_names(entity, &mut referenced),
                Item::Repeat(repeat) => {
                    for entity in &repeat.entities {
                        collect_insert_names(entity, &mut referenced);
                    }
                }
                Item::Block(_) | Item::Erased(_) => {}
            }
        }

        // A referenced block can itself insert other blocks. Follow that
        // closure so WBLOCK * keeps the complete block graph while dropping
        // definitions no entity in the drawing can reach.
        loop {
            let before = referenced.len();
            for block in self.drawing.blocks() {
                if referenced.contains(&block.name.to_ascii_uppercase()) {
                    for entity in &block.entities {
                        collect_insert_names(entity, &mut referenced);
                    }
                }
            }
            if referenced.len() == before {
                break;
            }
        }

        let items = self
            .drawing
            .items
            .iter()
            .filter_map(|item| match item {
                Item::Block(block) if referenced.contains(&block.name.to_ascii_uppercase()) => {
                    Some(Item::Block(block.clone()))
                }
                Item::Block(_) | Item::Erased(_) => None,
                Item::Entity(entity) => Some(Item::Entity(entity.clone())),
                Item::Repeat(repeat) => Some(Item::Repeat(repeat.clone())),
            })
            .collect();
        Drawing {
            header: self.drawing.header.clone(),
            items,
        }
    }

    fn wblock_named_block(&self, name: &str) -> Result<Drawing, String> {
        let block = self
            .drawing
            .blocks()
            .find(|block| block.name.eq_ignore_ascii_case(name))
            .ok_or_else(|| format!("WBLOCK block not found: {name}"))?;
        let mut header = self.drawing.header.clone();
        header.base = block.base;
        Ok(Drawing {
            header,
            items: block.entities.iter().cloned().map(Item::Entity).collect(),
        })
    }

    fn wblock_selected_entities(&self, ids: &[usize], base: Point) -> Drawing {
        let selected = selected_item_indexes(&self.drawing, ids);
        let mut header = self.drawing.header.clone();
        header.base = base;
        let items = self
            .drawing
            .items
            .iter()
            .enumerate()
            .filter(|(index, _)| selected.contains(&(index + 1)))
            .map(|(_, item)| item.clone())
            .collect();
        Drawing { header, items }
    }

    fn explode_block(&mut self, name: &str, origin: Point) -> Result<(), String> {
        let block = self
            .drawing
            .blocks()
            .find(|block| block.name == name)
            .ok_or_else(|| format!("unknown block: {name}"))?;
        let delta = Point {
            x: origin.x - block.base.x,
            y: origin.y - block.base.y,
        };
        let mut copies = block.entities.clone();
        for entity in &mut copies {
            transform_entity(entity, Transform::Translate(delta));
        }
        self.save_undo();
        self.drawing
            .items
            .extend(copies.into_iter().map(Item::Entity));
        self.refresh_after_edit();
        self.status = format!("Inserted {name} as separate entities");
        Ok(())
    }

    fn erase(&mut self, ids: &[usize]) {
        self.save_undo();
        let selected = selected_item_indexes(&self.drawing, ids);
        let mut erased = Vec::new();
        for (index, item) in self.drawing.items.iter_mut().enumerate() {
            if selected.contains(&(index + 1)) {
                let previous = item.clone();
                let Item::Entity(entity) = previous.clone() else {
                    unreachable!("only live entities are selectable");
                };
                *item = Item::Erased(entity);
                erased.push(ErasedItem {
                    index,
                    item: previous,
                });
            }
        }
        self.last_erased = Some(erased);
        self.refresh_after_edit();
        self.status = format!("Erased {} entities", ids.len());
    }

    fn oops(&mut self) {
        let Some(erased) = self.last_erased.clone() else {
            self.status = "Nothing to restore".into();
            return;
        };
        self.save_undo();
        self.last_erased = None;
        for erased_item in &erased {
            if matches!(
                self.drawing.items.get(erased_item.index),
                Some(Item::Erased(_))
            ) {
                self.drawing.items[erased_item.index] = erased_item.item.clone();
            } else {
                let index = erased_item.index.min(self.drawing.items.len());
                self.drawing.items.insert(index, erased_item.item.clone());
            }
        }
        self.refresh_after_edit();
        self.status = format!("Restored {} erased entities", erased.len());
    }

    fn array(
        &mut self,
        ids: &[usize],
        rows: usize,
        columns: usize,
        row_spacing: f64,
        column_spacing: f64,
    ) {
        let source_indexes = selected_item_indexes(&self.drawing, ids);
        let sources: Vec<Entity> = self
            .drawing
            .items
            .iter()
            .enumerate()
            .filter_map(|(index, item)| {
                source_indexes.contains(&(index + 1)).then(|| match item {
                    Item::Entity(entity) => Some(entity.clone()),
                    Item::Block(_) | Item::Repeat(_) | Item::Erased(_) => None,
                })?
            })
            .collect();
        let copies_count = rows * columns - 1;
        let mut copies = Vec::with_capacity(sources.len().saturating_mul(copies_count));
        for column in 0..columns {
            for row in 0..rows {
                if row == 0 && column == 0 {
                    continue;
                }
                let offset = Point {
                    x: column_spacing * column as f64,
                    y: row_spacing * row as f64,
                };
                copies.extend(sources.iter().cloned().map(|mut entity| {
                    transform_entity(&mut entity, Transform::Translate(offset));
                    Item::Entity(entity)
                }));
            }
        }
        self.save_undo();
        let copy_count = copies.len();
        self.drawing.items.extend(copies);
        self.refresh_after_edit();
        self.status = format!("Created {rows}x{columns} array with {copy_count} copies");
    }

    fn circular_array(&mut self, ids: &[usize], center: Point, angle: f64, count: usize) {
        let source_indexes = selected_item_indexes(&self.drawing, ids);
        let sources: Vec<Entity> = self
            .drawing
            .items
            .iter()
            .enumerate()
            .filter_map(|(index, item)| {
                source_indexes.contains(&(index + 1)).then(|| match item {
                    Item::Entity(entity) => Some(entity.clone()),
                    Item::Block(_) | Item::Repeat(_) | Item::Erased(_) => None,
                })?
            })
            .collect();
        let mut copies = Vec::with_capacity(sources.len().saturating_mul(count.saturating_sub(1)));
        for item in 1..count {
            let degrees = angle * item as f64;
            for source in &sources {
                let Some(anchor) = entity_anchor(source) else {
                    continue;
                };
                let target = rotate_point(anchor, center, degrees);
                let delta = Point {
                    x: target.x - anchor.x,
                    y: target.y - anchor.y,
                };
                let mut entity = source.clone();
                transform_entity(&mut entity, Transform::Translate(delta));
                copies.push(Item::Entity(entity));
            }
        }
        self.save_undo();
        let copy_count = copies.len();
        self.drawing.items.extend(copies);
        self.refresh_after_edit();
        self.status = format!("Created circular array with {copy_count} copies");
    }

    fn change_layer(&mut self, ids: &[usize], layer: u8) {
        self.save_undo();
        let selected = selected_item_indexes(&self.drawing, ids);
        for (index, item) in self.drawing.items.iter_mut().enumerate() {
            if selected.contains(&(index + 1)) {
                if let Item::Entity(entity) = item {
                    assign_layer(entity, layer);
                }
            }
        }
        self.ensure_layer(layer);
        self.status = format!("Changed {} entities to layer {layer}", ids.len());
    }

    fn change_point(&mut self, ids: &[usize], point: Point) -> Result<(), String> {
        let selected = selected_item_indexes(&self.drawing, ids);
        for (index, item) in self.drawing.items.iter().enumerate() {
            if selected.contains(&(index + 1)) {
                let Item::Entity(entity) = item else {
                    return Err("CHANGE point mode only supports top-level entities".into());
                };
                if !can_change_point(entity) {
                    return Err(
                        "CHANGE point mode currently supports LINE, CIRCLE and INSERT".into(),
                    );
                }
            }
        }
        self.save_undo();
        for (index, item) in self.drawing.items.iter_mut().enumerate() {
            if selected.contains(&(index + 1)) {
                if let Item::Entity(entity) = item {
                    apply_change_point(entity, point);
                }
            }
        }
        self.refresh_after_edit();
        self.status = format!("Changed {} entities at the specified point", ids.len());
        Ok(())
    }

    fn fillet(&mut self, ids: &[usize], radius: f64) -> Result<(), String> {
        let indexes = selected_item_indexes(&self.drawing, ids);
        let selected: Vec<_> = indexes.into_iter().collect();
        let get_line = |position: usize| -> Result<(Point, Point), String> {
            match self.drawing.items.get(position - 1) {
                Some(Item::Entity(entity)) => {
                    line_points(entity).ok_or_else(|| "FILLET only supports LINE entities".into())
                }
                _ => Err("FILLET selection is not a top-level entity".into()),
            }
        };
        let (first_start, first_end) = get_line(selected[0])?;
        let (second_start, second_end) = get_line(selected[1])?;
        let fillet = fillet_geometry((first_start, first_end), (second_start, second_end), radius)?;

        self.save_undo();
        for (index, line) in selected.iter().zip([fillet.first, fillet.second]) {
            if let Some(Item::Entity(entity)) = self.drawing.items.get_mut(index - 1) {
                set_line_points(entity, line.0, line.1);
            }
        }
        self.drawing.items.push(Item::Entity(Entity::OnLayer {
            layer: self.drawing.header.current_layer,
            entity: Box::new(Entity::Arc {
                center: fillet.center,
                radius,
                start_deg: fillet.start_deg,
                end_deg: fillet.end_deg,
            }),
        }));
        self.refresh_after_edit();
        self.status = "Filleted two lines".into();
        Ok(())
    }

    fn break_entity(&mut self, ids: &[usize], first: Point, second: Point) -> Result<(), String> {
        let indexes = selected_item_indexes(&self.drawing, ids);
        let item_index = *indexes
            .first()
            .ok_or_else(|| "BREAK selection is empty".to_owned())?;
        let Some(Item::Entity(entity)) = self.drawing.items.get(item_index - 1) else {
            return Err("BREAK selection is not a top-level entity".into());
        };
        let entity_kind = match bare(entity) {
            Entity::Line { .. } => "line",
            Entity::Circle { .. } => "circle",
            Entity::Arc { .. } => "arc",
            _ => return Err("BREAK supports LINE, ARC and CIRCLE entities".into()),
        };
        let geometry = break_entity_geometry(entity, first, second)?;
        let layer = entity_layer(entity);

        self.save_undo();
        if let Some(Item::Entity(entity)) = self.drawing.items.get_mut(item_index - 1) {
            *entity = Entity::OnLayer {
                layer,
                entity: Box::new(geometry.primary),
            };
        }
        if let Some(secondary) = geometry.secondary {
            self.drawing.items.push(Item::Entity(Entity::OnLayer {
                layer,
                entity: Box::new(secondary),
            }));
        }
        self.refresh_after_edit();
        self.status = format!("Broke {entity_kind}");
        Ok(())
    }

    fn measure_area(&mut self, ids: &[usize]) -> Result<(), String> {
        let indexes = selected_item_indexes(&self.drawing, ids);
        let entities: Vec<_> = self
            .drawing
            .items
            .iter()
            .enumerate()
            .filter_map(|(index, item)| {
                if indexes.contains(&(index + 1)) {
                    match item {
                        Item::Entity(entity) => Some(entity),
                        Item::Block(_) | Item::Repeat(_) | Item::Erased(_) => None,
                    }
                } else {
                    None
                }
            })
            .collect();
        let (area, perimeter) = area_perimeter(&entities)?;
        self.status = format!("Area={area:.6}, Perimeter={perimeter:.6}");
        Ok(())
    }

    fn add_hatch(
        &mut self,
        pattern: &str,
        scale: f64,
        angle_deg: f64,
        ids: &[usize],
    ) -> Result<(), String> {
        if pattern != "LINE" {
            return Err(format!(
                "HATCH pattern {pattern} is listed but its geometry is not implemented"
            ));
        }
        let lines = hatch_line_geometry(&self.drawing, ids, scale, angle_deg)?;
        if lines.is_empty() {
            return Err("HATCH boundary produced no pattern lines".into());
        }
        let mut suffix = 1usize;
        let name = loop {
            let candidate = format!("*X{suffix}");
            if self
                .drawing
                .blocks()
                .all(|block| !block.name.eq_ignore_ascii_case(&candidate))
            {
                break candidate;
            }
            suffix += 1;
        };
        self.save_undo();
        self.drawing.items.push(Item::Block(Block {
            name: name.clone(),
            base: Point { x: 0.0, y: 0.0 },
            entities: lines
                .into_iter()
                .map(|entity| Entity::OnLayer {
                    layer: 127,
                    entity: Box::new(entity),
                })
                .collect(),
        }));
        self.drawing.items.push(Item::Entity(Entity::OnLayer {
            layer: self.drawing.header.current_layer,
            entity: Box::new(Entity::Insert {
                origin: Point { x: 0.0, y: 0.0 },
                x_scale: 1.0,
                y_scale: 1.0,
                rotation_deg: 0.0,
                name,
            }),
        }));
        Ok(())
    }

    fn save_undo(&mut self) {
        self.undo.push(UndoSnapshot {
            drawing: self.drawing.clone(),
            last_erased: self.last_erased.clone(),
            active_shape_library: self.active_shape_library.clone(),
        });
    }

    fn transform(&mut self, ids: &[usize], transform: Transform, copy: bool) {
        self.save_undo();
        let selected = selected_item_indexes(&self.drawing, ids);
        if copy {
            let mut copies = Vec::new();
            for (index, item) in self.drawing.items.iter().enumerate() {
                if selected.contains(&(index + 1)) {
                    if let Item::Entity(entity) = item {
                        let mut entity = entity.clone();
                        transform_entity(&mut entity, transform);
                        copies.push(Item::Entity(entity));
                    }
                }
            }
            self.drawing.items.extend(copies);
        } else {
            for (index, item) in self.drawing.items.iter_mut().enumerate() {
                if selected.contains(&(index + 1)) {
                    if let Item::Entity(entity) = item {
                        transform_entity(entity, transform);
                    }
                }
            }
        }
        self.refresh_after_edit();
        self.status = format!(
            "{} {} entities",
            if copy { "Copied" } else { "Transformed" },
            ids.len()
        );
    }

    fn refresh_limits(&mut self) {
        let mut points = Vec::new();
        for item in &self.drawing.items {
            match item {
                Item::Entity(entity) => entity_points(entity, &mut points),
                Item::Repeat(repeat) => repeat_points(repeat, &mut points),
                Item::Block(_) | Item::Erased(_) => {}
            }
        }
        if points.is_empty() {
            return;
        }
        let mut bounds = Extents {
            xmin: points.iter().map(|p| p.x).fold(f64::INFINITY, f64::min),
            xmax: points.iter().map(|p| p.x).fold(f64::NEG_INFINITY, f64::max),
            ymin: points.iter().map(|p| p.y).fold(f64::INFINITY, f64::min),
            ymax: points.iter().map(|p| p.y).fold(f64::NEG_INFINITY, f64::max),
        };
        if bounds.width() == 0.0 {
            bounds.xmin -= 0.5;
            bounds.xmax += 0.5;
        }
        if bounds.height() == 0.0 {
            bounds.ymin -= 0.5;
            bounds.ymax += 0.5;
        }
        self.drawing.header.extents = bounds;
        self.drawing.header.limits = bounds;
        self.drawing.header.view = acad_model::DwgView {
            center: Point {
                x: (bounds.xmin + bounds.xmax) / 2.0,
                y: (bounds.ymin + bounds.ymax) / 2.0,
            },
            height: bounds.height(),
        };
    }

    fn refresh_after_edit(&mut self) {
        let view = self.drawing.header.view;
        let limits = self.drawing.header.limits;
        self.refresh_limits();
        self.drawing.header.view = view;
        self.drawing.header.limits = limits;
        if !self
            .drawing
            .entities()
            .any(|entity| !matches!(bare(entity), Entity::Load { .. }))
        {
            self.drawing.header.extents = Extents {
                xmin: 0.0,
                xmax: 0.0,
                ymin: 0.0,
                ymax: 0.0,
            };
        }
    }
}

const COMMAND_LIST: &str = "\
Command List

ARC         DELAY       HELP        ORTHO       SKETCH
AREA        DIM         ID          PAN         SNAP
ARRAY       DIST        INSERT      PLOT        SOLID
AXIS        END         LAYER       POINT       STATUS
BASE        ENDREP      LIMITS      QPLOT       TABLET
BLOCK       ERASE       LINE        QUIT        TEXT
BREAK       FILES       LIST        REDRAW      TRACE
CHANGE      FILL        LOAD        REGEN       UNITS
CIRCLE      FILLET      MENU        REPEAT      WBLOCK
COPY        GRID        MOVE        RESUME      ZOOM
DBLIST      HATCH       OOPS        SHAPE       ?

Point Entry: Absolute: x,y; Relative: @dx,dy; Distance, angle: @d<a
Object selection: L = Last object; W = Within window
Command repeat: press space or RETURN.\n";

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

fn help_report(input: &str) -> Result<String, String> {
    let command = input.trim().to_ascii_uppercase();
    if command.is_empty() {
        return Ok(COMMAND_LIST.into());
    }
    if command == "LINE" {
        return Ok(LINE_HELP.into());
    }
    Err(format!("help text for {command} has not been recovered"))
}

const HATCH_PATTERNS: &[(&str, &str)] = &[
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

fn hatch_pattern_report() -> String {
    HATCH_PATTERNS
        .iter()
        .map(|(name, description)| format!("{name:<15} - {description}\n"))
        .collect()
}

fn entities_in_window(drawing: &Drawing, first: Point, second: Point) -> Vec<usize> {
    let min = Point {
        x: first.x.min(second.x),
        y: first.y.min(second.y),
    };
    let max = Point {
        x: first.x.max(second.x),
        y: first.y.max(second.y),
    };
    let mut ids = Vec::new();
    let mut selectable_id = 0;
    for item in &drawing.items {
        let Item::Entity(entity) = item else {
            continue;
        };
        if matches!(bare(entity), Entity::Load { .. }) {
            continue;
        }
        selectable_id += 1;
        let Some(points) = selection_extents_points(entity) else {
            continue;
        };
        if points.iter().all(|point| {
            point.x >= min.x && point.x <= max.x && point.y >= min.y && point.y <= max.y
        }) {
            ids.push(selectable_id);
        }
    }
    ids
}

fn selection_extents_points(entity: &Entity) -> Option<Vec<Point>> {
    Some(match bare(entity) {
        Entity::Line { start, end } => vec![*start, *end],
        Entity::Circle { center, radius } | Entity::Arc { center, radius, .. } => vec![
            Point {
                x: center.x - radius,
                y: center.y - radius,
            },
            Point {
                x: center.x + radius,
                y: center.y + radius,
            },
        ],
        Entity::Point { origin } | Entity::Text { origin, .. } | Entity::Shape { origin, .. } => {
            vec![*origin]
        }
        Entity::Trace { p1, p2, p3, p4 } | Entity::Solid { p1, p2, p3, p4 } => {
            vec![*p1, *p2, *p3, *p4]
        }
        Entity::Repeat(repeat) => repeat
            .entities
            .iter()
            .filter_map(selection_extents_points)
            .flatten()
            .collect(),
        Entity::Insert { .. } | Entity::Load { .. } | Entity::OnLayer { .. } => return None,
    })
}

#[derive(Clone, Copy)]
enum HatchEdge {
    Line(Point, Point),
    Arc {
        center: Point,
        radius: f64,
        start_deg: f64,
        end_deg: f64,
    },
}

fn hatch_sweep_deg(start: f64, end: f64) -> f64 {
    let sweep = (end - start).rem_euclid(360.0);
    if sweep == 0.0 {
        360.0
    } else {
        sweep
    }
}

fn hatch_line_geometry(
    drawing: &Drawing,
    ids: &[usize],
    scale: f64,
    angle_deg: f64,
) -> Result<Vec<Entity>, String> {
    let selected: BTreeSet<_> = ids.iter().copied().collect();
    let mut edges = Vec::new();
    let mut circles = Vec::new();
    let mut selectable_id = 0;
    for item in &drawing.items {
        let Item::Entity(entity) = item else {
            continue;
        };
        if matches!(bare(entity), Entity::Load { .. }) {
            continue;
        }
        selectable_id += 1;
        if !selected.contains(&selectable_id) {
            continue;
        }
        match bare(entity) {
            Entity::Line { start, end } => edges.push(HatchEdge::Line(*start, *end)),
            Entity::Arc {
                center,
                radius,
                start_deg,
                end_deg,
            } if *radius > 0.0 => {
                edges.push(HatchEdge::Arc {
                    center: *center,
                    radius: *radius,
                    start_deg: *start_deg,
                    end_deg: *end_deg,
                });
            }
            Entity::Circle { center, radius } if *radius > 0.0 => {
                circles.push((*center, *radius));
            }
            _ => {
                return Err(
                    "HATCH LINE supports closed LINE/ARC loops and CIRCLE boundaries".into(),
                )
            }
        }
    }
    if edges.is_empty() && circles.is_empty() {
        return Err("HATCH needs at least one selected boundary object".into());
    }

    let close = |a: Point, b: Point| (a.x - b.x).abs() <= 1e-8 && (a.y - b.y).abs() <= 1e-8;
    let edge_ends = |edge: HatchEdge| -> (Point, Point) {
        match edge {
            HatchEdge::Line(a, b) => (a, b),
            HatchEdge::Arc {
                center,
                radius,
                start_deg,
                end_deg,
            } => {
                let point = |angle: f64| {
                    let angle = angle.to_radians();
                    Point {
                        x: center.x + radius * angle.cos(),
                        y: center.y + radius * angle.sin(),
                    }
                };
                (point(start_deg), point(end_deg))
            }
        }
    };
    let mut unused = vec![true; edges.len()];
    let mut loops = Vec::<Vec<HatchEdge>>::new();
    while let Some(first_edge) = unused.iter().position(|is_unused| *is_unused) {
        unused[first_edge] = false;
        let (start, next) = edge_ends(edges[first_edge]);
        let mut loop_edges = vec![edges[first_edge]];
        let mut current = next;
        while !close(current, start) {
            let candidates: Vec<_> = edges
                .iter()
                .enumerate()
                .filter(|(index, _)| unused[*index])
                .filter_map(|(index, edge)| {
                    let (a, b) = edge_ends(*edge);
                    if close(a, current) {
                        Some((index, b, false))
                    } else if close(b, current) {
                        Some((index, a, true))
                    } else {
                        None
                    }
                })
                .collect();
            if candidates.len() != 1 {
                return Err("HATCH boundary lines must form closed, unbranched loops".into());
            }
            let (edge, point, reverse) = candidates[0];
            unused[edge] = false;
            let _ = reverse; // Topology is undirected; retain the arc's CCW geometry.
            loop_edges.push(edges[edge]);
            current = point;
            if loop_edges.len() > edges.len() {
                return Err("HATCH boundary loop did not close".into());
            }
        }
        if loop_edges.len() < 2 {
            return Err("HATCH boundary loop needs at least two edges".into());
        }
        loops.push(loop_edges);
    }

    let spacing = 0.125 * scale;
    if !spacing.is_finite() || spacing <= 0.0 {
        return Err("HATCH line spacing is outside the supported range".into());
    }
    let radians = angle_deg.to_radians();
    let direction = Point {
        x: radians.cos(),
        y: radians.sin(),
    };
    let normal = Point {
        x: -direction.y,
        y: direction.x,
    };
    let project = |point: Point, axis: Point| point.x * axis.x + point.y * axis.y;
    let edge_projection_extrema = |edge: HatchEdge| -> (f64, f64) {
        let (a, b) = edge_ends(edge);
        let mut values = vec![project(a, normal), project(b, normal)];
        if let HatchEdge::Arc {
            center,
            radius,
            start_deg,
            end_deg,
        } = edge
        {
            let sweep = hatch_sweep_deg(start_deg, end_deg);
            let normal_angle = normal.y.atan2(normal.x).to_degrees();
            for angle in [normal_angle, normal_angle + 180.0] {
                if (angle - start_deg).rem_euclid(360.0) <= sweep + 1e-10 {
                    let radians = angle.to_radians();
                    values.push(project(
                        Point {
                            x: center.x + radius * radians.cos(),
                            y: center.y + radius * radians.sin(),
                        },
                        normal,
                    ));
                }
            }
        }
        (
            values.iter().copied().fold(f64::INFINITY, f64::min),
            values.iter().copied().fold(f64::NEG_INFINITY, f64::max),
        )
    };
    let edge_extrema: Vec<_> = loops
        .iter()
        .flatten()
        .map(|edge| edge_projection_extrema(*edge))
        .collect();
    let line_min = edge_extrema
        .iter()
        .map(|(min, _)| *min)
        .fold(f64::INFINITY, f64::min);
    let line_max = edge_extrema
        .iter()
        .map(|(_, max)| *max)
        .fold(f64::NEG_INFINITY, f64::max);
    let circle_min = circles
        .iter()
        .map(|(center, radius)| project(*center, normal) - radius)
        .fold(f64::INFINITY, f64::min);
    let circle_max = circles
        .iter()
        .map(|(center, radius)| project(*center, normal) + radius)
        .fold(f64::NEG_INFINITY, f64::max);
    let min = line_min.min(circle_min);
    let max = line_max.max(circle_max);
    let center = (min + max) / 2.0;
    let center_index = (center / spacing).round() as i64;
    let first_index = if center_index as f64 * spacing >= max {
        ((max - 1e-10) / spacing).floor() as i64
    } else if center_index as f64 * spacing < min {
        (min / spacing).ceil() as i64
    } else {
        center_index
    };
    let count = ((max - min) / spacing).ceil().max(0.0) as usize;
    if count > MAX_ARRAY_ENTITIES {
        return Err(format!(
            "HATCH would create {count} lines; limit is {MAX_ARRAY_ENTITIES}"
        ));
    }
    let mut offsets = Vec::with_capacity(count);
    for (direction, mut index) in [(1_i64, first_index), (-1_i64, first_index - 1)] {
        while {
            let offset = index as f64 * spacing;
            offset >= min - 1e-10 && offset < max - 1e-10
        } {
            offsets.push(index as f64 * spacing);
            index += direction;
        }
    }
    let mut lines = Vec::with_capacity(count);
    for offset in offsets {
        let mut intersections = Vec::new();
        for edge in loops.iter().flatten() {
            match *edge {
                HatchEdge::Line(a, b) => {
                    let da = project(a, normal);
                    let db = project(b, normal);
                    if (da <= offset && offset < db) || (db <= offset && offset < da) {
                        let fraction = (offset - da) / (db - da);
                        let crossing = Point {
                            x: a.x + (b.x - a.x) * fraction,
                            y: a.y + (b.y - a.y) * fraction,
                        };
                        intersections.push(project(crossing, direction));
                    }
                }
                HatchEdge::Arc {
                    center,
                    radius,
                    start_deg,
                    end_deg,
                } => {
                    let center_offset = project(center, normal);
                    let delta = offset - center_offset;
                    if delta.abs() < radius {
                        let half_chord = (radius * radius - delta * delta).sqrt();
                        let center_along = project(center, direction);
                        for along in [center_along - half_chord, center_along + half_chord] {
                            let point = Point {
                                x: direction.x * along + normal.x * offset,
                                y: direction.y * along + normal.y * offset,
                            };
                            let angle = (point.y - center.y)
                                .atan2(point.x - center.x)
                                .to_degrees()
                                .rem_euclid(360.0);
                            if (angle - start_deg).rem_euclid(360.0)
                                <= hatch_sweep_deg(start_deg, end_deg) + 1e-9
                            {
                                intersections.push(along);
                            }
                        }
                    }
                }
            }
        }
        for (center, radius) in &circles {
            let center_offset = project(*center, normal);
            let delta = offset - center_offset;
            if delta.abs() < *radius {
                let half_chord = (radius * radius - delta * delta).sqrt();
                let center_along = project(*center, direction);
                intersections.push(center_along - half_chord);
                intersections.push(center_along + half_chord);
            }
        }
        intersections.sort_by(f64::total_cmp);
        intersections.dedup_by(|a, b| (*a - *b).abs() <= 1e-9);
        for pair in intersections.chunks_exact(2) {
            let point = |along: f64| Point {
                x: direction.x * along + normal.x * offset,
                y: direction.y * along + normal.y * offset,
            };
            lines.push(Entity::Line {
                start: point(pair[0]),
                end: point(pair[1]),
            });
        }
    }
    Ok(lines)
}

const LINE_HELP: &str = "\
The  LINE  command allows you to draw straight lines.

Format:     LINE  From point:  <point>
            To point:  <point>
            To point:  <point>
            To point:  RETURN to end line sequence

You can continue the previous line or arc by responding to the
\"From point:\" prompt with a space or RETURN.  If you are drawing
a sequence of lines which will become a closed polygon, you may
reply to the \"to point\" prompt with \"C\" to draw the last segment
(close the polygon).

Lines may be constrained to horizontal or vertical by the ORTHO command.

Reference:  Section 3.1 of User Guide.\n";

fn dimension_geometry(
    first: Point,
    intersection: Point,
    second: Point,
    text: Option<&str>,
    configured_text_size: f64,
    units: Units,
) -> Result<Vec<Entity>, String> {
    let dx = intersection.x - first.x;
    let dy = intersection.y - first.y;
    let baseline = dx.hypot(dy);
    if !baseline.is_finite() || baseline == 0.0 {
        return Err("DIM extension line points must be distinct".into());
    }
    let along_extension = Point {
        x: dx / baseline,
        y: dy / baseline,
    };
    let normal = Point {
        x: along_extension.y,
        y: -along_extension.x,
    };
    let offset = (second.x - intersection.x) * normal.x + (second.y - intersection.y) * normal.y;
    if !offset.is_finite() || offset == 0.0 {
        return Err("DIM extension lines must not be collinear".into());
    }
    let dimension_length = offset.abs();
    let dimension_direction = Point {
        x: normal.x * offset.signum(),
        y: normal.y * offset.signum(),
    };
    let dimension_end = Point {
        x: intersection.x + normal.x * offset,
        y: intersection.y + normal.y * offset,
    };
    let dimension_mid = Point {
        x: (intersection.x + dimension_end.x) / 2.0,
        y: (intersection.y + dimension_end.y) / 2.0,
    };

    const ARROW: f64 = 9.0 / 64.0;
    const ARROW_HALF_WIDTH: f64 = 3.0 / 128.0;
    let text_height = ((configured_text_size * 135.0).round() / 128.0).max(1.0 / 128.0);
    let dimension_text = text
        .filter(|value| !value.is_empty())
        .map(str::to_owned)
        .unwrap_or_else(|| format_measurement(dimension_length, units));
    // Native simplex numeric advances observed in QEMU are 52/63 cap heights,
    // with the narrow "1" glyph 40/63. This controls the dimension-line gap;
    // the TEXT record remains the portable glyph data.
    let text_width = dimension_text
        .chars()
        .map(|character| {
            text_height
                * if character == '1' {
                    40.0 / 63.0
                } else {
                    52.0 / 63.0
                }
        })
        .sum::<f64>();
    let line_fits = dimension_length > text_width + 4.0 * ARROW;
    let snap = |value: f64| (value * 128.0).round() / 128.0;
    let snap_point = |point: Point| Point {
        x: snap(point.x),
        y: snap(point.y),
    };
    let point_along = |origin: Point, direction: Point, distance: f64| Point {
        x: origin.x + direction.x * distance,
        y: origin.y + direction.y * distance,
    };
    let mut entities = Vec::with_capacity(7);

    entities.push(Entity::Line {
        start: first,
        end: snap_point(point_along(intersection, along_extension, ARROW)),
    });
    entities.push(Entity::Line {
        start: second,
        end: snap_point(point_along(dimension_end, along_extension, ARROW)),
    });

    let arrow_base_start;
    let arrow_base_end;
    if line_fits {
        arrow_base_start = point_along(intersection, dimension_direction, ARROW);
        arrow_base_end = point_along(dimension_end, dimension_direction, -ARROW);
        let gap_half = text_width / 2.0 + ARROW;
        let left_text_edge = point_along(dimension_mid, dimension_direction, -gap_half);
        let right_text_edge = point_along(dimension_mid, dimension_direction, gap_half);
        if dimension_length > text_width + 4.0 * ARROW {
            entities.push(Entity::Line {
                start: snap_point(arrow_base_start),
                end: left_text_edge,
            });
            entities.push(Entity::Line {
                start: snap_point(arrow_base_end),
                end: right_text_edge,
            });
        }
    } else {
        arrow_base_start = point_along(intersection, dimension_direction, -ARROW);
        arrow_base_end = point_along(dimension_end, dimension_direction, ARROW);
        entities.push(Entity::Line {
            start: snap_point(arrow_base_end),
            end: snap_point(point_along(dimension_end, dimension_direction, 2.0 * ARROW)),
        });
        entities.push(Entity::Line {
            start: snap_point(arrow_base_start),
            end: snap_point(point_along(intersection, dimension_direction, -2.0 * ARROW)),
        });
    }

    let arrow_tips = if line_fits {
        [
            (intersection, arrow_base_start),
            (dimension_end, arrow_base_end),
        ]
    } else {
        [
            (dimension_end, arrow_base_end),
            (intersection, arrow_base_start),
        ]
    };
    for (tip, base) in arrow_tips {
        let side = Point {
            x: dimension_direction.y,
            y: -dimension_direction.x,
        };
        let edge_a = snap_point(Point {
            x: base.x + side.x * ARROW_HALF_WIDTH,
            y: base.y + side.y * ARROW_HALF_WIDTH,
        });
        let edge_b = snap_point(Point {
            x: base.x - side.x * ARROW_HALF_WIDTH,
            y: base.y - side.y * ARROW_HALF_WIDTH,
        });
        let tip = snap_point(tip);
        entities.push(Entity::Solid {
            p1: edge_a,
            p2: edge_b,
            p3: tip,
            p4: tip,
        });
    }

    let text_origin = if line_fits {
        Point {
            x: dimension_mid.x - text_width / 2.0,
            y: dimension_mid.y - text_height / 2.0,
        }
    } else {
        Point {
            x: dimension_mid.x - text_width / 2.0,
            y: dimension_end.y + 2.0 * text_height,
        }
    };
    entities.push(Entity::Text {
        origin: text_origin,
        height: text_height,
        rotation_deg: 0.0,
        value: dimension_text,
    });
    Ok(entities)
}

fn repeat_points(repeat: &acad_model::Repeat, out: &mut Vec<Point>) {
    let mut base = Vec::new();
    for entity in &repeat.entities {
        entity_points(entity, &mut base);
    }
    for row in 0..repeat.rows {
        for column in 0..repeat.columns {
            let dx = f64::from(column) * repeat.column_spacing;
            let dy = f64::from(row) * repeat.row_spacing;
            out.extend(base.iter().map(|p| Point {
                x: p.x + dx,
                y: p.y + dy,
            }));
        }
    }
}

fn entity_points(entity: &Entity, out: &mut Vec<Point>) {
    match bare(entity) {
        Entity::Repeat(repeat) => repeat_points(repeat, out),
        Entity::Line { start, end } => out.extend([*start, *end]),
        Entity::Circle { center, radius } | Entity::Arc { center, radius, .. } => out.extend([
            Point {
                x: center.x - radius,
                y: center.y - radius,
            },
            Point {
                x: center.x + radius,
                y: center.y + radius,
            },
        ]),
        Entity::Point { origin } | Entity::Text { origin, .. } | Entity::Shape { origin, .. } => {
            out.push(*origin)
        }
        Entity::Trace { p1, p2, p3, p4 } | Entity::Solid { p1, p2, p3, p4 } => {
            out.extend([*p1, *p2, *p3, *p4])
        }
        Entity::Insert { .. } | Entity::Load { .. } | Entity::OnLayer { .. } => {}
    }
}

fn selectable_count(drawing: &Drawing) -> usize {
    drawing
        .items
        .iter()
        .filter(|item| {
            matches!(item, Item::Entity(entity) if !matches!(bare(entity), Entity::Load { .. }))
        })
        .count()
}

fn selection(input: &str, count: usize) -> Result<Vec<usize>, String> {
    if input.eq_ignore_ascii_case("ALL") {
        if count == 0 {
            return Err("there are no selectable entities".into());
        }
        return Ok((1..=count).collect());
    }
    if input.eq_ignore_ascii_case("LAST") || input.eq_ignore_ascii_case("L") {
        return if count == 0 {
            Err("there are no selectable entities".into())
        } else {
            Ok(vec![count])
        };
    }
    let mut ids = BTreeSet::new();
    for part in input.split(',') {
        let id = part
            .trim()
            .parse::<usize>()
            .map_err(|_| format!("invalid entity number: {}", part.trim()))?;
        if id == 0 || id > count {
            return Err(format!("entity number {id} is out of range (1..={count})"));
        }
        ids.insert(id);
    }
    if ids.is_empty() {
        return Err("select at least one entity".into());
    }
    Ok(ids.into_iter().collect())
}

fn selected_item_indexes(drawing: &Drawing, ids: &[usize]) -> BTreeSet<usize> {
    let wanted: BTreeSet<_> = ids.iter().copied().collect();
    let mut selectable = 0usize;
    drawing
        .items
        .iter()
        .enumerate()
        .filter_map(|(item_index, item)| match item {
            Item::Entity(entity) if !matches!(bare(entity), Entity::Load { .. }) => {
                selectable += 1;
                wanted.contains(&selectable).then_some(item_index + 1)
            }
            _ => None,
        })
        .collect()
}

fn entity_pick_distance(point: Point, entity: &Entity) -> Option<f64> {
    let distance = |a: Point, b: Point| (a.x - b.x).hypot(a.y - b.y);
    let segment_distance = |start: Point, end: Point| {
        let dx = end.x - start.x;
        let dy = end.y - start.y;
        let length_squared = dx * dx + dy * dy;
        if length_squared == 0.0 {
            return distance(point, start);
        }
        let along = (((point.x - start.x) * dx + (point.y - start.y) * dy) / length_squared)
            .clamp(0.0, 1.0);
        distance(
            point,
            Point {
                x: start.x + along * dx,
                y: start.y + along * dy,
            },
        )
    };
    match bare(entity) {
        Entity::Line { start, end } => Some(segment_distance(*start, *end)),
        Entity::Circle { center, radius } => Some((distance(point, *center) - radius).abs()),
        Entity::Arc {
            center,
            radius,
            start_deg,
            end_deg,
        } => {
            let radial = distance(point, *center);
            let angle = (point.y - center.y)
                .atan2(point.x - center.x)
                .to_degrees()
                .rem_euclid(360.0);
            let sweep = end_deg - start_deg;
            let offset = (angle - start_deg).rem_euclid(360.0);
            if sweep.abs() >= 360.0 || offset <= sweep.rem_euclid(360.0) {
                Some((radial - radius).abs())
            } else {
                let endpoint = |degrees: f64| Point {
                    x: center.x + radius * degrees.to_radians().cos(),
                    y: center.y + radius * degrees.to_radians().sin(),
                };
                Some(distance(point, endpoint(*start_deg)).min(distance(point, endpoint(*end_deg))))
            }
        }
        Entity::Point { origin }
        | Entity::Text { origin, .. }
        | Entity::Shape { origin, .. }
        | Entity::Insert { origin, .. } => Some(distance(point, *origin)),
        Entity::Trace { p1, p2, p3, p4 } | Entity::Solid { p1, p2, p3, p4 } => Some(
            [(*p1, *p2), (*p2, *p3), (*p3, *p4), (*p4, *p1)]
                .into_iter()
                .map(|(start, end)| segment_distance(start, end))
                .fold(f64::INFINITY, f64::min),
        ),
        Entity::Repeat(_) | Entity::Load { .. } | Entity::OnLayer { .. } => None,
    }
}

fn list_entities(drawing: &Drawing) -> String {
    let mut lines = Vec::new();
    for (index, entity) in drawing
        .entities()
        .filter(|entity| !matches!(bare(entity), Entity::Load { .. }))
        .enumerate()
    {
        let kind = match bare(entity) {
            Entity::Repeat(_) => "REPEAT",
            Entity::Line { .. } => "LINE",
            Entity::Circle { .. } => "CIRCLE",
            Entity::Arc { .. } => "ARC",
            Entity::Text { .. } => "TEXT",
            Entity::Insert { .. } => "INSERT",
            Entity::Point { .. } => "POINT",
            Entity::Trace { .. } => "TRACE",
            Entity::Solid { .. } => "SOLID",
            Entity::Shape { .. } => "SHAPE",
            Entity::Load { .. } | Entity::OnLayer { .. } => unreachable!(),
        };
        lines.push(format!("{} {kind}", index + 1));
    }
    if lines.is_empty() {
        "No entities".into()
    } else {
        lines.join(", ")
    }
}

fn database_listing(drawing: &Drawing) -> String {
    fn append_entity(lines: &mut Vec<String>, count: &mut usize, entity: &Entity) {
        *count += 1;
        lines.push(format!("Entity {}: {:#?}", count, bare(entity)));
    }

    let mut lines = vec!["Drawing database".to_owned()];
    let mut count = 0;
    for item in &drawing.items {
        match item {
            Item::Entity(entity) => append_entity(&mut lines, &mut count, entity),
            Item::Erased(_) => {}
            Item::Block(block) => {
                lines.push(format!("Block {} base {:?}", block.name, block.base));
                for entity in &block.entities {
                    append_entity(&mut lines, &mut count, entity);
                }
            }
            Item::Repeat(repeat) => {
                lines.push(format!(
                    "Repeat {} columns × {} rows",
                    repeat.columns, repeat.rows
                ));
                for entity in &repeat.entities {
                    append_entity(&mut lines, &mut count, entity);
                }
            }
        }
    }
    if count == 0 {
        lines.push("No entities".into());
    }
    lines.join("\n")
}

fn collect_insert_names(entity: &Entity, names: &mut BTreeSet<String>) {
    match entity {
        Entity::OnLayer { entity, .. } => collect_insert_names(entity, names),
        Entity::Insert { name, .. } => {
            names.insert(name.to_ascii_uppercase());
        }
        Entity::Repeat(repeat) => {
            for entity in &repeat.entities {
                collect_insert_names(entity, names);
            }
        }
        _ => {}
    }
}

fn transform_entity(entity: &mut Entity, transform: Transform) {
    if let Entity::OnLayer { entity, .. } = entity {
        transform_entity(entity, transform);
        return;
    }
    let point = |p: &mut Point| match transform {
        Transform::Translate(delta) => {
            p.x += delta.x;
            p.y += delta.y;
        }
        Transform::Rotate { base, degrees } => {
            let angle = degrees.to_radians();
            let (sin, cos) = angle.sin_cos();
            let x = p.x - base.x;
            let y = p.y - base.y;
            p.x = base.x + x * cos - y * sin;
            p.y = base.y + x * sin + y * cos;
        }
        Transform::Scale { base, factor } => {
            p.x = base.x + (p.x - base.x) * factor;
            p.y = base.y + (p.y - base.y) * factor;
        }
    };
    match entity {
        Entity::Repeat(repeat) => {
            for inner in &mut repeat.entities {
                transform_entity(inner, transform);
            }
            if let Transform::Scale { factor, .. } = transform {
                repeat.column_spacing *= factor;
                repeat.row_spacing *= factor;
            }
        }
        Entity::Line { start, end } => {
            point(start);
            point(end);
        }
        Entity::Circle { center, radius } => {
            point(center);
            if let Transform::Scale { factor, .. } = transform {
                *radius *= factor;
            }
        }
        Entity::Arc {
            center,
            radius,
            start_deg,
            end_deg,
        } => {
            point(center);
            match transform {
                Transform::Scale { factor, .. } => *radius *= factor,
                Transform::Rotate { degrees, .. } => {
                    *start_deg = (*start_deg + degrees).rem_euclid(360.0);
                    *end_deg = (*end_deg + degrees).rem_euclid(360.0);
                }
                Transform::Translate(_) => {}
            }
        }
        Entity::Text {
            origin,
            height,
            rotation_deg,
            ..
        }
        | Entity::Shape {
            origin,
            height,
            rotation_deg,
            ..
        } => {
            point(origin);
            match transform {
                Transform::Scale { factor, .. } => *height *= factor,
                Transform::Rotate { degrees, .. } => *rotation_deg += degrees,
                Transform::Translate(_) => {}
            }
        }
        Entity::Insert {
            origin,
            x_scale,
            y_scale,
            rotation_deg,
            ..
        } => {
            point(origin);
            match transform {
                Transform::Scale { factor, .. } => {
                    *x_scale *= factor;
                    *y_scale *= factor;
                }
                Transform::Rotate { degrees, .. } => *rotation_deg += degrees,
                Transform::Translate(_) => {}
            }
        }
        Entity::Point { origin } => point(origin),
        Entity::Trace { p1, p2, p3, p4 } | Entity::Solid { p1, p2, p3, p4 } => {
            point(p1);
            point(p2);
            point(p3);
            point(p4);
        }
        Entity::Load { .. } => {}
        Entity::OnLayer { .. } => unreachable!("layer wrapper was removed above"),
    }
}

fn bare(mut entity: &Entity) -> &Entity {
    while let Entity::OnLayer { entity: inner, .. } = entity {
        entity = inner;
    }
    entity
}

fn normalize_library_name(name: &str) -> String {
    let leaf = name
        .trim()
        .rsplit(['/', '\\'])
        .next()
        .unwrap_or(name)
        .to_ascii_uppercase();
    leaf.strip_suffix(".SHP").unwrap_or(&leaf).to_owned()
}

fn load_library_name(entity: &Entity) -> Option<String> {
    match bare(entity) {
        Entity::Load { name } => Some(normalize_library_name(name)),
        _ => None,
    }
}

fn entity_anchor(entity: &Entity) -> Option<Point> {
    match bare(entity) {
        Entity::Repeat(repeat) => repeat.entities.first().and_then(entity_anchor),
        Entity::Line { start, .. } => Some(*start),
        Entity::Circle { center, .. } | Entity::Arc { center, .. } => Some(*center),
        Entity::Point { origin } | Entity::Text { origin, .. } | Entity::Shape { origin, .. } => {
            Some(*origin)
        }
        Entity::Trace { p1, .. } | Entity::Solid { p1, .. } => Some(*p1),
        Entity::Insert { origin, .. } => Some(*origin),
        Entity::Load { .. } | Entity::OnLayer { .. } => None,
    }
}

fn item_anchor(item: &Item) -> Option<Point> {
    match item {
        Item::Entity(entity) => entity_anchor(entity),
        _ => None,
    }
}

fn can_change_point(entity: &Entity) -> bool {
    matches!(
        bare(entity),
        Entity::Line { .. } | Entity::Circle { .. } | Entity::Insert { .. }
    )
}

fn apply_change_point(entity: &mut Entity, point: Point) {
    match entity {
        Entity::OnLayer { entity, .. } => apply_change_point(entity, point),
        Entity::Line { start, end } => {
            let distance = |candidate: Point| {
                (candidate.x - point.x).powi(2) + (candidate.y - point.y).powi(2)
            };
            if distance(*start) <= distance(*end) {
                *start = point;
            } else {
                *end = point;
            }
        }
        Entity::Circle { center, radius } => {
            *radius = ((center.x - point.x).powi(2) + (center.y - point.y).powi(2)).sqrt();
        }
        Entity::Insert { origin, .. } => *origin = point,
        Entity::Repeat(_)
        | Entity::Load { .. }
        | Entity::Shape { .. }
        | Entity::Arc { .. }
        | Entity::Text { .. }
        | Entity::Point { .. }
        | Entity::Trace { .. }
        | Entity::Solid { .. } => unreachable!("unsupported CHANGE point entity was validated"),
    }
}

fn set_insert_angle(entity: &mut Entity, angle: f64) {
    match entity {
        Entity::OnLayer { entity, .. } => set_insert_angle(entity, angle),
        Entity::Insert { rotation_deg, .. } => *rotation_deg = angle,
        _ => {}
    }
}

fn rotate_point(point: Point, base: Point, degrees: f64) -> Point {
    let angle = degrees.to_radians();
    let (sin, cos) = angle.sin_cos();
    let x = point.x - base.x;
    let y = point.y - base.y;
    Point {
        x: base.x + x * cos - y * sin,
        y: base.y + x * sin + y * cos,
    }
}

fn trace_quads(points: &[Point], width: f64) -> Result<Vec<Entity>, String> {
    if points.len() < 2 || !width.is_finite() || width <= 0.0 {
        return Err("TRACE needs two points and a positive width".into());
    }
    let directions: Vec<_> = points
        .windows(2)
        .map(|pair| {
            let dx = pair[1].x - pair[0].x;
            let dy = pair[1].y - pair[0].y;
            let length = dx.hypot(dy);
            if !length.is_finite() || length == 0.0 {
                Err("TRACE needs distinct finite points".to_owned())
            } else {
                Ok((dx / length, dy / length))
            }
        })
        .collect::<Result<_, _>>()?;
    let normals: Vec<_> = directions.iter().map(|&(x, y)| (-y, x)).collect();
    let half = width / 2.0;
    let mut edges = Vec::with_capacity(points.len());
    for (index, center) in points.iter().enumerate() {
        let (nx, ny, factor) = if index == 0 {
            (normals[0].0, normals[0].1, half)
        } else if index + 1 == points.len() {
            let normal = normals[index - 1];
            (normal.0, normal.1, half)
        } else {
            let previous = directions[index - 1];
            let next = directions[index];
            let denom = 1.0 + previous.0 * next.0 + previous.1 * next.1;
            if denom <= 1e-9 {
                return Err("TRACE cannot miter a reversing path".into());
            }
            (
                normals[index - 1].0 + normals[index].0,
                normals[index - 1].1 + normals[index].1,
                half / denom,
            )
        };
        let x = nx * factor;
        let y = ny * factor;
        let left = Point {
            x: center.x + x,
            y: center.y + y,
        };
        let right = Point {
            x: center.x - x,
            y: center.y - y,
        };
        if ![left.x, left.y, right.x, right.y]
            .iter()
            .all(|value| value.is_finite())
        {
            return Err("TRACE corner exceeds the finite numeric range".into());
        }
        edges.push((left, right));
    }
    Ok(edges
        .windows(2)
        .map(|pair| Entity::Trace {
            p1: pair[0].0,
            p2: pair[0].1,
            p3: pair[1].0,
            p4: pair[1].1,
        })
        .collect())
}

fn area_perimeter(entities: &[&Entity]) -> Result<(f64, f64), String> {
    if entities.is_empty() {
        return Err("AREA requires at least one entity".into());
    }
    if entities
        .iter()
        .all(|entity| matches!(bare(entity), Entity::Line { .. }))
    {
        let (area, perimeter) = line_loop_area(entities)?;
        return checked_area_metrics(area, perimeter);
    }

    let mut total_area = 0.0;
    let mut total_perimeter = 0.0;
    for entity in entities {
        match bare(entity) {
            Entity::Circle { center: _, radius } if *radius > 0.0 => {
                total_area += std::f64::consts::PI * radius * radius;
                total_perimeter += std::f64::consts::TAU * radius;
            }
            Entity::Trace { p1, p2, p3, p4 } | Entity::Solid { p1, p2, p3, p4 } => {
                let (area, perimeter) = polygon_metrics(&[*p1, *p2, *p3, *p4]);
                total_area += area;
                total_perimeter += perimeter;
            }
            Entity::Circle { .. } => {
                return Err("AREA cannot measure a circle with nonpositive radius".into());
            }
            _ => return Err("AREA supports circles, TRACE/SOLID and closed LINE loops".into()),
        }
    }
    checked_area_metrics(total_area, total_perimeter)
}

fn checked_area_metrics(area: f64, perimeter: f64) -> Result<(f64, f64), String> {
    if !area.is_finite() || !perimeter.is_finite() {
        return Err("AREA result exceeds the finite numeric range".into());
    }
    Ok((area, perimeter))
}

fn line_loop_area(entities: &[&Entity]) -> Result<(f64, f64), String> {
    if entities.len() < 3 {
        return Err("AREA needs at least three lines for a closed loop".into());
    }
    let first = line_points(entities[0]).expect("the caller checked every entity is a line");
    if (first.1.x - first.0.x).hypot(first.1.y - first.0.y) <= 1e-12 {
        return Err("AREA cannot measure a zero-length line".into());
    }
    let mut points = vec![first.0, first.1];
    let mut remaining: Vec<_> = entities[1..]
        .iter()
        .map(|entity| line_points(entity).expect("the caller checked every entity is a line"))
        .collect();
    let coordinate_scale = entities
        .iter()
        .filter_map(|entity| line_points(entity))
        .flat_map(|(start, end)| [start.x.abs(), start.y.abs(), end.x.abs(), end.y.abs()])
        .fold(1.0, f64::max);
    let tolerance = coordinate_scale * 1e-9;
    let mut current = first.1;
    while !remaining.is_empty() {
        let matches: Vec<_> = remaining
            .iter()
            .enumerate()
            .filter_map(|(index, (start, end))| {
                let starts_here = (start.x - current.x).hypot(start.y - current.y) <= tolerance;
                let ends_here = (end.x - current.x).hypot(end.y - current.y) <= tolerance;
                (starts_here || ends_here).then_some((index, starts_here, ends_here))
            })
            .collect();
        if matches.len() != 1 {
            return Err("AREA line selection must form one unbranched closed loop".into());
        }
        let (index, starts_here, ends_here) = matches[0];
        if starts_here && ends_here {
            return Err("AREA cannot measure a zero-length line".into());
        }
        let (start, end) = remaining.remove(index);
        let next = if starts_here { end } else { start };
        points.push(next);
        current = next;
    }
    if (current.x - first.0.x).hypot(current.y - first.0.y) > tolerance {
        return Err("AREA line selection is open".into());
    }
    Ok(polygon_metrics(&points))
}

fn polygon_metrics(points: &[Point]) -> (f64, f64) {
    let pairs = points
        .iter()
        .copied()
        .zip(points.iter().copied().cycle().skip(1))
        .take(points.len());
    let mut twice_area = 0.0;
    let mut perimeter = 0.0;
    for (a, b) in pairs {
        twice_area += a.x * b.y - b.x * a.y;
        perimeter += (b.x - a.x).hypot(b.y - a.y);
    }
    (twice_area.abs() / 2.0, perimeter)
}

fn assign_layer(entity: &mut Entity, layer: u8) {
    if let Entity::OnLayer { layer: current, .. } = entity {
        *current = layer;
    } else {
        *entity = Entity::OnLayer {
            layer,
            entity: Box::new(entity.clone()),
        };
    }
}

fn line_points(entity: &Entity) -> Option<(Point, Point)> {
    match entity {
        Entity::OnLayer { entity, .. } => line_points(entity),
        Entity::Line { start, end } => Some((*start, *end)),
        _ => None,
    }
}

fn entity_layer(entity: &Entity) -> u8 {
    match entity {
        Entity::OnLayer { layer, .. } => *layer,
        _ => 1,
    }
}

struct BreakGeometry {
    primary: Entity,
    secondary: Option<Entity>,
}

fn break_entity_geometry(
    entity: &Entity,
    first: Point,
    second: Point,
) -> Result<BreakGeometry, String> {
    match bare(entity) {
        Entity::Line { start, end } => {
            let parts = break_line_geometry((*start, *end), first, second)?;
            Ok(BreakGeometry {
                primary: Entity::Line {
                    start: parts.first.0,
                    end: parts.first.1,
                },
                secondary: Some(Entity::Line {
                    start: parts.second.0,
                    end: parts.second.1,
                }),
            })
        }
        Entity::Circle { center, radius } => {
            let (start_deg, end_deg) = break_circle_angles(*center, *radius, first, second)?;
            Ok(BreakGeometry {
                primary: Entity::Arc {
                    center: *center,
                    radius: *radius,
                    start_deg,
                    end_deg,
                },
                secondary: None,
            })
        }
        Entity::Arc {
            center,
            radius,
            start_deg,
            end_deg,
        } => {
            let (before, after) =
                break_arc_geometry(*center, *radius, *start_deg, *end_deg, first, second)?;
            Ok(BreakGeometry {
                primary: before,
                secondary: after,
            })
        }
        _ => Err("BREAK supports LINE, ARC and CIRCLE entities".into()),
    }
}

fn break_circle_angles(
    center: Point,
    radius: f64,
    first: Point,
    second: Point,
) -> Result<(f64, f64), String> {
    if !radius.is_finite() || radius <= 0.0 {
        return Err("BREAK cannot split a circle with a nonpositive radius".into());
    }
    let angle = |point: Point| {
        (point.y - center.y)
            .atan2(point.x - center.x)
            .to_degrees()
            .rem_euclid(360.0)
    };
    let tolerance = radius.max(1.0) * 1e-8;
    for point in [first, second] {
        let distance = (point.x - center.x).hypot(point.y - center.y);
        if (distance - radius).abs() > tolerance {
            return Err("BREAK points must lie on the selected circle".into());
        }
    }
    let first_deg = angle(first);
    let second_deg = angle(second);
    let removed_sweep = (second_deg - first_deg).rem_euclid(360.0);
    if removed_sweep <= 1e-10 || (360.0 - removed_sweep) <= 1e-10 {
        return Err("BREAK points must be distinct".into());
    }
    // BREAK removes the counter-clockwise arc from the first point to the
    // second; the retained complement starts at the second point.
    Ok((second_deg, first_deg))
}

fn break_arc_geometry(
    center: Point,
    radius: f64,
    start_deg: f64,
    end_deg: f64,
    first: Point,
    second: Point,
) -> Result<(Entity, Option<Entity>), String> {
    if !radius.is_finite() || radius <= 0.0 {
        return Err("BREAK cannot split an arc with a nonpositive radius".into());
    }
    let start = start_deg.rem_euclid(360.0);
    let raw_sweep = (end_deg - start_deg).rem_euclid(360.0);
    let sweep = if raw_sweep == 0.0 { 360.0 } else { raw_sweep };
    let point_angle = |point: Point| {
        (point.y - center.y)
            .atan2(point.x - center.x)
            .to_degrees()
            .rem_euclid(360.0)
    };
    let tolerance = radius.max(1.0) * 1e-8;
    let mut path_positions = Vec::with_capacity(2);
    for point in [first, second] {
        let distance = (point.x - center.x).hypot(point.y - center.y);
        if (distance - radius).abs() > tolerance {
            return Err("BREAK points must lie on the selected arc".into());
        }
        let position = (point_angle(point) - start).rem_euclid(360.0);
        if position <= 1e-8 || position >= sweep - 1e-8 {
            return Err("BREAK points must be inside the selected arc".into());
        }
        path_positions.push(position);
    }
    if (path_positions[0] - path_positions[1]).abs() <= 1e-8 {
        return Err("BREAK points must be distinct".into());
    }
    path_positions.sort_by(f64::total_cmp);
    let angle_at = |position: f64| (start + position).rem_euclid(360.0);
    let make_arc = |start_angle, end_angle| Entity::Arc {
        center,
        radius,
        start_deg: start_angle,
        end_deg: end_angle,
    };
    Ok((
        make_arc(start, angle_at(path_positions[0])),
        Some(make_arc(
            angle_at(path_positions[1]),
            (start + sweep).rem_euclid(360.0),
        )),
    ))
}

struct LineBreakParts {
    first: (Point, Point),
    second: (Point, Point),
}

fn break_line_geometry(
    line: (Point, Point),
    first: Point,
    second: Point,
) -> Result<LineBreakParts, String> {
    let direction = Point {
        x: line.1.x - line.0.x,
        y: line.1.y - line.0.y,
    };
    let length_squared = direction.x * direction.x + direction.y * direction.y;
    if length_squared <= 1e-24 {
        return Err("BREAK cannot split a zero-length line".into());
    }
    let parameter = |point: Point| {
        ((point.x - line.0.x) * direction.x + (point.y - line.0.y) * direction.y) / length_squared
    };
    let project = |t: f64| Point {
        x: line.0.x + direction.x * t,
        y: line.0.y + direction.y * t,
    };
    let tolerance = 1e-8 * length_squared.sqrt().max(1.0);
    let first_t = parameter(first);
    let second_t = parameter(second);
    for (point, t) in [(first, first_t), (second, second_t)] {
        let projection = project(t);
        if (point.x - projection.x).hypot(point.y - projection.y) > tolerance {
            return Err("BREAK points must lie on the selected line".into());
        }
        if !(0.0 < t && t < 1.0) {
            return Err("BREAK points must be inside the selected line".into());
        }
    }
    if (first_t - second_t).abs() <= 1e-10 {
        return Err("BREAK points must be distinct".into());
    }
    let first_break = project(first_t);
    let second_break = project(second_t);
    let (first_break, second_break) = if first_t <= second_t {
        (first_break, second_break)
    } else {
        (second_break, first_break)
    };
    Ok(LineBreakParts {
        first: (line.0, first_break),
        second: (second_break, line.1),
    })
}

fn set_line_points(entity: &mut Entity, start: Point, end: Point) {
    match entity {
        Entity::OnLayer { entity, .. } => set_line_points(entity, start, end),
        Entity::Line {
            start: current_start,
            end: current_end,
        } => {
            *current_start = start;
            *current_end = end;
        }
        _ => unreachable!("FILLET validates the selected entity types first"),
    }
}

struct FilletGeometry {
    first: (Point, Point),
    second: (Point, Point),
    center: Point,
    start_deg: f64,
    end_deg: f64,
}

fn fillet_geometry(
    first: (Point, Point),
    second: (Point, Point),
    radius: f64,
) -> Result<FilletGeometry, String> {
    let sub = |a: Point, b: Point| Point {
        x: a.x - b.x,
        y: a.y - b.y,
    };
    let add = |a: Point, b: Point| Point {
        x: a.x + b.x,
        y: a.y + b.y,
    };
    let scale = |p: Point, factor: f64| Point {
        x: p.x * factor,
        y: p.y * factor,
    };
    let cross = |a: Point, b: Point| a.x * b.y - a.y * b.x;
    let length = |p: Point| p.x.hypot(p.y);
    let first_direction = sub(first.1, first.0);
    let second_direction = sub(second.1, second.0);
    let denominator = cross(first_direction, second_direction);
    let direction_scale = length(first_direction) * length(second_direction);
    if direction_scale == 0.0 || denominator.abs() <= 1e-12 * direction_scale {
        return Err("FILLET lines must be nonzero and nonparallel".into());
    }
    let between_starts = sub(second.0, first.0);
    let along_first = cross(between_starts, second_direction) / denominator;
    let along_second = cross(between_starts, first_direction) / denominator;
    let tolerance = 1e-10;
    if !(-tolerance..=1.0 + tolerance).contains(&along_first)
        || !(-tolerance..=1.0 + tolerance).contains(&along_second)
    {
        return Err("FILLET requires the line segments to intersect".into());
    }
    let vertex = add(first.0, scale(first_direction, along_first));
    // AutoCAD chooses the first line's forward ray and the second line's
    // backward ray for this selection order. The picked lines retain those
    // endpoint sides; trim the opposite endpoints to the tangent points.
    let first_ray = first_direction;
    let second_ray = scale(second_direction, -1.0);
    let first_available = length(sub(first.1, vertex));
    let second_available = length(sub(second.0, vertex));
    if first_available <= 1e-12 || second_available <= 1e-12 {
        return Err("FILLET has no room on the selected line rays".into());
    }
    let first_unit = scale(first_ray, 1.0 / length(first_ray));
    let second_unit = scale(second_ray, 1.0 / length(second_ray));
    let dot = (first_unit.x * second_unit.x + first_unit.y * second_unit.y).clamp(-1.0, 1.0);
    let angle = dot.acos();
    if angle <= 1e-10 || (std::f64::consts::PI - angle) <= 1e-10 {
        return Err("FILLET cannot round a zero or straight angle".into());
    }
    let tangent_distance = radius / (angle / 2.0).tan();
    if tangent_distance >= first_available || tangent_distance >= second_available {
        return Err("FILLET radius is too large for the selected lines".into());
    }
    let first_tangent = add(vertex, scale(first_unit, tangent_distance));
    let second_tangent = add(vertex, scale(second_unit, tangent_distance));
    let bisector = add(first_unit, second_unit);
    let center = add(
        vertex,
        scale(bisector, radius / ((angle / 2.0).sin() * length(bisector))),
    );
    let degrees = |point: Point| {
        let angle = (point.y - center.y)
            .atan2(point.x - center.x)
            .to_degrees()
            .rem_euclid(360.0);
        if angle > 360.0 - 1e-10 {
            0.0
        } else {
            angle
        }
    };
    let (start_angle, end_angle) = if cross(first_unit, second_unit) > 0.0 {
        (degrees(second_tangent), degrees(first_tangent))
    } else {
        (degrees(first_tangent), degrees(second_tangent))
    };
    Ok(FilletGeometry {
        first: (first_tangent, first.1),
        second: (second.0, second_tangent),
        center,
        start_deg: start_angle,
        end_deg: end_angle,
    })
}

fn three_point_arc(a: Point, b: Point, c: Point) -> Result<(Point, f64, f64, f64), String> {
    let d = 2.0 * (a.x * (b.y - c.y) + b.x * (c.y - a.y) + c.x * (a.y - b.y));
    if d.abs() < 1e-12 {
        return Err("ARC points are collinear".into());
    }
    let aa = a.x * a.x + a.y * a.y;
    let bb = b.x * b.x + b.y * b.y;
    let cc = c.x * c.x + c.y * c.y;
    let center = Point {
        x: (aa * (b.y - c.y) + bb * (c.y - a.y) + cc * (a.y - b.y)) / d,
        y: (aa * (c.x - b.x) + bb * (a.x - c.x) + cc * (b.x - a.x)) / d,
    };
    let radius = (a.x - center.x).hypot(a.y - center.y);
    let angle = |p: Point| {
        (p.y - center.y)
            .atan2(p.x - center.x)
            .to_degrees()
            .rem_euclid(360.0)
    };
    let (mut start, mut end) = (angle(a), angle(c));
    let middle = angle(b);
    let sweep = (end - start).rem_euclid(360.0);
    if (middle - start).rem_euclid(360.0) > sweep {
        std::mem::swap(&mut start, &mut end);
    }
    Ok((center, radius, start, end))
}

#[cfg(test)]
mod tests {
    use super::*;

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
