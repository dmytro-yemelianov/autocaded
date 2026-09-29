//! Interactive command state machine for the 1983 editor.
use acad_model::{Drawing, Entity, Extents, Header, Item, Point};
use std::collections::{BTreeMap, BTreeSet};

const MAX_ARRAY_ENTITIES: usize = 100_000;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Effect {
    Continue,
    Save(String),
    Quit,
}

#[derive(Debug, Clone, Copy)]
enum Transform {
    Translate(Point),
    Rotate { base: Point, degrees: f64 },
    Scale { base: Point, factor: f64 },
}

#[derive(Debug, Clone, PartialEq)]
enum InputState {
    Command,
    LineStart,
    LineNext(Point),
    CircleCenter,
    CircleRadius(Point),
    Point,
    ArcStart,
    ArcMiddle(Point),
    ArcEnd(Point, Point),
    TextOrigin,
    TextHeight(Point),
    TextRotation(Point, f64),
    TextValue(Point, f64, f64),
    InsertName,
    InsertOrigin(String, bool),
    InsertXScale(String, Point),
    InsertYScale(String, Point, f64),
    InsertRotation(String, Point, f64, f64),
    BlockName,
    BlockBase(String),
    BlockSelection(String, Point),
    SavePath,
    Base,
    Snap,
    Grid,
    Ortho,
    Fill,
    LimitsMin,
    LimitsMax(Point),
    Layer,
    ColorValue,
    ColorTargetLayer,
    Zoom,
    ZoomCenter,
    ZoomCenterHeight(Point),
    ZoomWindowMin,
    ZoomWindowMax(Point),
    PanCenter,
    EditSelection(EditCommand),
    EditBase(EditCommand, Vec<usize>),
    EditValue(EditCommand, Vec<usize>, Point),
    Displacement(EditCommand),
    SecondPoint(EditCommand, Point),
    DisplacedSelection(EditCommand, Point),
    ArraySelection,
    ArrayMode(Vec<usize>),
    ArrayCircularCenter(Vec<usize>),
    ArrayCircularAngle(Vec<usize>, Point),
    ArrayCircularItems(Vec<usize>, Point, f64),
    ArrayRows(Vec<usize>),
    ArrayColumns(Vec<usize>, usize),
    ArrayRowSpacing(Vec<usize>, usize, usize),
    ArrayColumnSpacing(Vec<usize>, usize, usize, f64),
    ChangeSelection,
    ChangeLayer(Vec<usize>),
    FilletSelection,
    FilletRadius(Vec<usize>),
    BreakSelection,
    BreakFirstPoint(Vec<usize>),
    BreakSecondPoint(Vec<usize>, Point),
    DistanceFirstPoint,
    DistanceSecondPoint(Point),
    IdPoint,
    SolidFirstPoint,
    SolidSecondPoint(Point),
    SolidThirdPoint(Point, Point),
    SolidFourthPoint(Point, Point, Point),
    TraceWidth,
    TraceStart(f64),
    TraceNext(f64, Vec<Point>),
    AreaFirstPoint,
    AreaNextPoint(Vec<Point>),
    AreaSelection,
    RepeatColumns,
    RepeatRows(u16),
    RepeatColumnSpacing(u16, u16),
    RepeatRowSpacing(u16, u16, f64),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum EditCommand {
    Erase,
    Move,
    Copy,
    Rotate,
    Scale,
}

impl InputState {
    fn prompt(&self) -> &'static str {
        match self {
            Self::Command => "Command",
            Self::LineStart => "LINE: first point",
            Self::LineNext(_) => "LINE: next point (Enter to finish)",
            Self::CircleCenter => "CIRCLE: center point",
            Self::CircleRadius(_) => "CIRCLE: radius",
            Self::Point => "POINT: point",
            Self::ArcStart => "ARC: start point",
            Self::ArcMiddle(_) => "ARC: second point",
            Self::ArcEnd(_, _) => "ARC: end point",
            Self::TextOrigin => "TEXT: start point",
            Self::TextHeight(_) => "TEXT: height",
            Self::TextRotation(_, _) => "TEXT: rotation angle",
            Self::TextValue(_, _, _) => "TEXT: value",
            Self::InsertName => "INSERT: block name",
            Self::InsertOrigin(_, _) => "INSERT: insertion point",
            Self::InsertXScale(_, _) => "INSERT: X scale or opposite corner x,y (Enter for 1)",
            Self::InsertYScale(_, _, _) => "INSERT: Y scale (Enter for X scale)",
            Self::InsertRotation(_, _, _, _) => "INSERT: rotation angle (Enter for 0)",
            Self::BlockName => "BLOCK: block name",
            Self::BlockBase(_) => "BLOCK: insertion base point",
            Self::BlockSelection(_, _) => "BLOCK: entity numbers, ALL or LAST",
            Self::SavePath => "SAVE: output file",
            Self::Base => "BASE: x,y",
            Self::Snap => "SNAP: spacing",
            Self::Grid => "GRID: spacing",
            Self::Ortho => "ORTHO: ON or OFF",
            Self::Fill => "FILL: ON or OFF",
            Self::LimitsMin => "LIMITS: lower-left",
            Self::LimitsMax(_) => "LIMITS: upper-right",
            Self::Layer => "LAYER: index",
            Self::ColorValue => "COLOR: color index",
            Self::ColorTargetLayer => "COLOR: next current layer (Enter to finish)",
            Self::Zoom => "ZOOM: factor, E, W, C or P",
            Self::ZoomCenter => "ZOOM CENTER: x,y",
            Self::ZoomCenterHeight(_) => "ZOOM CENTER: view height",
            Self::ZoomWindowMin => "ZOOM WINDOW: lower-left",
            Self::ZoomWindowMax(_) => "ZOOM WINDOW: upper-right",
            Self::PanCenter => "PAN: new view center x,y",
            Self::EditSelection(EditCommand::Erase) => "ERASE: entity numbers or ALL",
            Self::EditSelection(EditCommand::Move | EditCommand::Copy) => {
                unreachable!("MOVE and COPY select after displacement")
            }
            Self::EditSelection(EditCommand::Rotate) => "ROTATE: entity numbers or ALL",
            Self::EditSelection(EditCommand::Scale) => "SCALE: entity numbers or ALL",
            Self::EditBase(EditCommand::Move | EditCommand::Copy, _) => {
                unreachable!("MOVE and COPY use displacement prompts")
            }
            Self::EditBase(EditCommand::Rotate, _) => "ROTATE: base point x,y",
            Self::EditBase(EditCommand::Scale, _) => "SCALE: base point x,y",
            Self::EditBase(EditCommand::Erase, _) => "ERASE: selection",
            Self::EditValue(EditCommand::Rotate, _, _) => "ROTATE: angle in degrees",
            Self::EditValue(EditCommand::Scale, _, _) => "SCALE: factor (>0)",
            Self::EditValue(EditCommand::Move | EditCommand::Copy, _, _) => {
                unreachable!("MOVE and COPY use displacement prompts")
            }
            Self::EditValue(_, _, _) => "edit: value",
            Self::Displacement(EditCommand::Move) => "MOVE: displacement or first point x,y",
            Self::Displacement(EditCommand::Copy) => "COPY: displacement or first point x,y",
            Self::SecondPoint(EditCommand::Move, _) => {
                "MOVE: second point (Enter for displacement)"
            }
            Self::SecondPoint(EditCommand::Copy, _) => {
                "COPY: second point (Enter for displacement)"
            }
            Self::DisplacedSelection(EditCommand::Move, _) => "MOVE: select objects, ALL or LAST",
            Self::DisplacedSelection(EditCommand::Copy, _) => "COPY: select objects, ALL or LAST",
            Self::Displacement(_) | Self::SecondPoint(_, _) | Self::DisplacedSelection(_, _) => {
                unreachable!("only MOVE and COPY use displacement prompts")
            }
            Self::ArraySelection => "ARRAY: entity numbers or ALL",
            Self::ArrayMode(_) => "ARRAY: rectangular or circular (R/C)",
            Self::ArrayCircularCenter(_) => "ARRAY: center point",
            Self::ArrayCircularAngle(_, _) => "ARRAY: angle between items",
            Self::ArrayCircularItems(_, _, _) => "ARRAY: number of items",
            Self::ArrayRows(_) => "ARRAY: number of rows",
            Self::ArrayColumns(_, _) => "ARRAY: number of columns",
            Self::ArrayRowSpacing(_, _, _) => "ARRAY: row spacing",
            Self::ArrayColumnSpacing(_, _, _, _) => "ARRAY: column spacing",
            Self::ChangeSelection => "CHANGE: entity numbers or ALL",
            Self::ChangeLayer(_) => "CHANGE: new layer index",
            Self::FilletSelection => "FILLET: select two LINE entities",
            Self::FilletRadius(_) => "FILLET: radius",
            Self::BreakSelection => "BREAK: one LINE, ARC or CIRCLE entity",
            Self::BreakFirstPoint(_) => "BREAK: first point",
            Self::BreakSecondPoint(_, _) => "BREAK: second point",
            Self::DistanceFirstPoint => "DIST: first point",
            Self::DistanceSecondPoint(_) => "DIST: second point",
            Self::IdPoint => "ID: point",
            Self::SolidFirstPoint => "SOLID: first point",
            Self::SolidSecondPoint(_) => "SOLID: second point",
            Self::SolidThirdPoint(_, _) => "SOLID: third point (Enter to finish)",
            Self::SolidFourthPoint(_, _, _) => "SOLID: fourth point",
            Self::TraceWidth => "TRACE: width",
            Self::TraceStart(_) => "TRACE: first point",
            Self::TraceNext(_, _) => "TRACE: next point (Enter to finish)",
            Self::AreaFirstPoint => "AREA: first point",
            Self::AreaNextPoint(_) => "AREA: next point (Enter to finish)",
            Self::AreaSelection => "ENTITYAREA: entity numbers or ALL",
            Self::RepeatColumns => "ENDREP: columns",
            Self::RepeatRows(_) => "ENDREP: rows",
            Self::RepeatColumnSpacing(_, _) => "ENDREP: column distance",
            Self::RepeatRowSpacing(_, _, _) => "ENDREP: row distance",
        }
    }
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
}

#[derive(Debug, Clone)]
struct UndoSnapshot {
    drawing: Drawing,
    last_erased: Option<Vec<ErasedItem>>,
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
                snap: acad_model::Mode {
                    on: false,
                    spacing: 1.0,
                },
                grid: acad_model::Mode {
                    on: false,
                    spacing: 1.0,
                },
                ortho: false,
                fill: true,
                text_size: 1.0,
                trace_width: 0.25,
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
        Self {
            drawing,
            state: InputState::Command,
            status: String::new(),
            previous_view: None,
            undo: Vec::new(),
            last_erased: None,
            repeat_start: None,
        }
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

    /// Submit one complete line of keyboard input. Coordinates use AutoCAD's
    /// `x,y` notation. A `LINE` remains active until an empty line is entered.
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
                let next = point(line)?;
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
                let middle = point(line)?;
                self.state = InputState::ArcEnd(start, middle);
                Ok(Effect::Continue)
            }
            InputState::ArcEnd(start, middle) => {
                let end = point(line)?;
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
                if line.contains(',') {
                    let opposite = point(line)?;
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
            InputState::Base => {
                self.drawing.header.base = point(line)?;
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
                let max = point(line)?;
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
                let max = point(line)?;
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
                    let second = point(line)?;
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
                self.state = InputState::ArrayColumnSpacing(ids, rows, columns, number(line)?);
                Ok(Effect::Continue)
            }
            InputState::ArrayColumnSpacing(ids, rows, columns, row_spacing) => {
                let column_spacing = number(line)?;
                self.array(&ids, rows, columns, row_spacing, column_spacing);
                self.state = InputState::Command;
                Ok(Effect::Continue)
            }
            InputState::ChangeSelection => {
                let ids = selection(line, selectable_count(&self.drawing))?;
                self.state = InputState::ChangeLayer(ids);
                Ok(Effect::Continue)
            }
            InputState::ChangeLayer(ids) => {
                let layer = layer_index(line)?;
                self.change_layer(&ids, layer);
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
                self.break_entity(&ids, first, point(line)?)?;
                self.state = InputState::Command;
                Ok(Effect::Continue)
            }
            InputState::DistanceFirstPoint => {
                self.state = InputState::DistanceSecondPoint(point(line)?);
                Ok(Effect::Continue)
            }
            InputState::DistanceSecondPoint(first) => {
                let second = point(line)?;
                let dx = second.x - first.x;
                let dy = second.y - first.y;
                let distance = dx.hypot(dy);
                self.status = format!("Distance={distance:.4}");
                self.state = InputState::Command;
                Ok(Effect::Continue)
            }
            InputState::IdPoint => {
                let location = point(line)?;
                self.status = format!("X = {:.4}    Y = {:.4}", location.x, location.y);
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
                self.state = InputState::SolidThirdPoint(p1, point(line)?);
                Ok(Effect::Continue)
            }
            InputState::SolidThirdPoint(p1, p2) => {
                if line.is_empty() {
                    return self.cancel();
                }
                self.state = InputState::SolidFourthPoint(p1, p2, point(line)?);
                Ok(Effect::Continue)
            }
            InputState::SolidFourthPoint(p1, p2, p3) => {
                let p4 = point(line)?;
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
                    let next = point(line)?;
                    let previous = points.last().expect("TRACE has a first point");
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
                    points.push(point(line)?);
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
                self.state = InputState::RepeatColumnSpacing(columns, rows);
                Ok(Effect::Continue)
            }
            InputState::RepeatColumnSpacing(columns, rows) => {
                let spacing = number(line)?;
                self.state = InputState::RepeatRowSpacing(columns, rows, spacing);
                Ok(Effect::Continue)
            }
            InputState::RepeatRowSpacing(columns, rows, column_spacing) => {
                let row_spacing = number(line)?;
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
            "TEXT" | "T" => self.state = InputState::TextOrigin,
            "INSERT" | "I" => self.state = InputState::InsertName,
            "BLOCK" => self.state = InputState::BlockName,
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
            "SNAP" => self.state = InputState::Snap,
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

    fn save_undo(&mut self) {
        self.undo.push(UndoSnapshot {
            drawing: self.drawing.clone(),
            last_erased: self.last_erased.clone(),
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

fn positive_count(input: &str, name: &str) -> Result<usize, String> {
    let count = input
        .parse::<usize>()
        .map_err(|_| format!("{name} must be a positive integer"))?;
    if count == 0 {
        return Err(format!("{name} must be a positive integer"));
    }
    Ok(count)
}

fn positive_word(input: &str, name: &str) -> Result<u16, String> {
    let count = positive_count(input, name)?;
    u16::try_from(count).map_err(|_| format!("{name} exceeds 65535"))
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
    let first_to_start = sub(first.0, vertex);
    let first_to_end = sub(first.1, vertex);
    let second_to_start = sub(second.0, vertex);
    let second_to_end = sub(second.1, vertex);
    let first_start_is_near = length(first_to_start) <= length(first_to_end);
    let second_start_is_near = length(second_to_start) <= length(second_to_end);
    let first_near_ray = if first_start_is_near {
        first_to_start
    } else {
        first_to_end
    };
    let first_ray = if length(first_near_ray) <= 1e-12 {
        if first_start_is_near {
            first_to_end
        } else {
            first_to_start
        }
    } else {
        first_near_ray
    };
    let second_near_ray = if second_start_is_near {
        second_to_start
    } else {
        second_to_end
    };
    let second_ray = if length(second_near_ray) <= 1e-12 {
        if second_start_is_near {
            second_to_end
        } else {
            second_to_start
        }
    } else {
        second_near_ray
    };
    let first_available = length(first_ray);
    let second_available = length(second_ray);
    let first_unit = scale(first_ray, 1.0 / first_available);
    let second_unit = scale(second_ray, 1.0 / second_available);
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
    let replace_near_endpoint = |line: (Point, Point), near_start: bool, tangent: Point| {
        if near_start {
            (tangent, line.1)
        } else {
            (line.0, tangent)
        }
    };
    Ok(FilletGeometry {
        first: replace_near_endpoint(first, first_start_is_near, first_tangent),
        second: replace_near_endpoint(second, second_start_is_near, second_tangent),
        center,
        start_deg: start_angle,
        end_deg: end_angle,
    })
}

fn number(text: &str) -> Result<f64, String> {
    text.parse::<f64>()
        .ok()
        .filter(|x| x.is_finite())
        .ok_or_else(|| format!("invalid number: {text}"))
}

fn point(text: &str) -> Result<Point, String> {
    let (x, y) = text
        .split_once(',')
        .ok_or_else(|| format!("expected x,y point: {text}"))?;
    Ok(Point {
        x: number(x.trim())?,
        y: number(y.trim())?,
    })
}

fn layer_index(text: &str) -> Result<u8, String> {
    let value = text
        .parse::<u16>()
        .map_err(|_| format!("invalid layer index: {text}"))?;
    u8::try_from(value)
        .ok()
        .filter(|layer| *layer < 128)
        .ok_or_else(|| format!("layer index out of range: {value}"))
}

fn color_index(text: &str) -> Result<u8, String> {
    let value = text
        .parse::<u16>()
        .map_err(|_| format!("invalid color index: {text}"))?;
    u8::try_from(value)
        .ok()
        .filter(|color| *color < 255)
        .ok_or_else(|| format!("color index out of range: {value}"))
}

fn parse_toggle(text: &str) -> Result<bool, String> {
    match text.to_ascii_uppercase().as_str() {
        "ON" | "YES" | "1" => Ok(true),
        "OFF" | "NO" | "0" => Ok(false),
        _ => Err(format!("expected ON or OFF: {text}")),
    }
}

fn parse_mode(text: &str, previous: acad_model::Mode) -> Result<acad_model::Mode, String> {
    match text.to_ascii_uppercase().as_str() {
        "ON" | "YES" => Ok(acad_model::Mode {
            on: true,
            spacing: previous.spacing,
        }),
        "OFF" | "NO" => Ok(acad_model::Mode {
            on: false,
            spacing: previous.spacing,
        }),
        _ => {
            let spacing = number(text)?;
            if spacing <= 0.0 {
                return Err("spacing must be positive".into());
            }
            Ok(acad_model::Mode { on: true, spacing })
        }
    }
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
        for input in ["CHANGE", "1", "7"] {
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
            matches!(entity.as_ref(), Entity::Line { start: Point { x, y: 0.0 }, end: Point { x: 5.0, y: 0.0 } } if (*x + 1.0).abs() < 1e-10)
        );
        let Entity::OnLayer { entity, .. } = entities.next().unwrap() else {
            panic!("expected layered line");
        };
        assert!(
            matches!(entity.as_ref(), Entity::Line { start: Point { x: 0.0, y, }, end: Point { x: 0.0, y: 5.0 } } if (*y + 1.0).abs() < 1e-10)
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
        assert!((center.x + 1.0).abs() < 1e-10);
        assert!((center.y + 1.0).abs() < 1e-10);
        assert_eq!(*radius, 1.0);
        assert!(start_deg.abs() < 1e-10, "start={start_deg}");
        assert!((end_deg - 90.0).abs() < 1e-10, "end={end_deg}");
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
