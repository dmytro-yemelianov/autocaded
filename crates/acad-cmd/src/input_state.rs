//! Input-state machine: `InputState`, `EditCommand`, and the transform kinds the editor tracks between prompts.

use crate::external_insert::InsertTarget;
use crate::geometry::{HatchRequest, HatchStyle};
use acad_model::Point;

#[derive(Debug, Clone, Copy)]
pub(crate) enum Transform {
    Translate(Point),
    Rotate { base: Point, degrees: f64 },
    Scale { base: Point, factor: f64 },
}

/// A WBLOCK output file and whether replacing an existing file was
/// confirmed at the replace question.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct WblockOutput {
    pub(crate) path: String,
    pub(crate) replace: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) enum InputState {
    Command,
    ListSelection,
    Selection(crate::selection::dialogue::Dialogue),
    LineStart,
    LineNext {
        first: Point,
        previous: Point,
    },
    CircleCenter,
    CircleRadius(Point),
    CircleDiameter(Point),
    CircleTwoPointFirst,
    CircleTwoPointSecond(Point),
    CircleThreePointFirst,
    CircleThreePointSecond(Point),
    CircleThreePointThird(Point, Point),
    Point,
    ArcStart,
    ArcMiddle(Point),
    ArcEnd(Point, Point),
    ArcCenterFirst,
    ArcStartAfterCenter(Point),
    ArcCenter(Point),
    ArcCenterChoice(Point, Point),
    ArcCenterAngle(Point, Point),
    ArcCenterChord(Point, Point),
    ArcEndpoint(Point),
    ArcEndChoice(Point, Point),
    ArcEndRadius(Point, Point),
    ArcEndAngle(Point, Point),
    ArcEndDirection(Point, Point),
    ArcContinueEnd(crate::curve_history::Tangent),
    LoadLibrary,
    ShapeName,
    ShapeOrigin(u16),
    ShapeHeight(u16, Point),
    ShapeRotation(u16, Point, f64),
    Text(crate::text::TextInput),
    InsertName,
    InsertOrigin(InsertTarget, bool),
    InsertXScale(InsertTarget, Point),
    InsertYScale(InsertTarget, Point, f64),
    InsertRotation(InsertTarget, Point, f64, f64),
    BlockName,
    BlockBase(String),
    BlockSelection(String, Point),
    WblockPath,
    WblockReplace {
        path: String,
        open_drawing: bool,
    },
    WblockName(WblockOutput),
    WblockBase(WblockOutput),
    WblockSelection(WblockOutput, Point),
    HelpCommand,
    MenuFile,
    ScriptFile,
    FilesMenu,
    FilesDrive(crate::FilesFilter),
    FilesListSpecification,
    FilesDeleteSpecification,
    FilesRenameSource,
    FilesRenameDestination(String),
    Delay,
    UnitsFormat,
    UnitsPrecision(acad_model::UnitFormat),
    SavePath,
    EndSavePath,
    /// A DWG save met an erased REPEAT owner holding members erased before
    /// it. `path` is the SAVE/END output, `None` for the attached document;
    /// `quit` leaves the editor after an END save.
    OriginalErasureSave {
        path: Option<String>,
        quit: bool,
    },
    QuitConfirmation,
    Base,
    Axis,
    Snap,
    Grid,
    Ortho,
    Fill,
    LimitsMin,
    LimitsMax(Point),
    Layer,
    LayerVisibility(bool),
    LayerColor,
    ColorValue,
    ColorTargetLayer,
    View(crate::view::ViewInput),
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
    /// Single-INSERT circular array awaiting the rotate-copies answer.
    ArrayCircularRotate(Vec<usize>, Point, f64, usize),
    ArrayRows(Vec<usize>),
    ArrayColumns(Vec<usize>, usize),
    ArrayRowSpacing(Vec<usize>, usize, usize),
    ArrayColumnSpacing(Vec<usize>, usize, usize, ArraySpacingInput),
    ChangeSelection,
    ChangeIntersection(Vec<usize>),
    ChangeLayer(Vec<usize>),
    ChangeProperties(crate::change::ChangeInput),
    FilletSelection,
    FilletRadius,
    BreakSelection,
    BreakFirstPoint(Vec<usize>),
    /// The `bool` is true after a point-to-object pick, when `F` may
    /// re-enter the first point.
    BreakSecondPoint(Vec<usize>, Point, bool),
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
    RepeatColumnSpacing(u16, u16, Point),
    RepeatRowSpacing(u16, u16, f64, Point, RepeatDistanceInput),
    DimFirstExtension,
    DimArrowSize,
    DimInsideHorizontalText {
        default_horizontal: bool,
    },
    DimOutsideHorizontalText {
        pending_inside_horizontal: bool,
        default_horizontal: bool,
    },
    DimIntersection(Point),
    DimSecondExtension(crate::dimension::DimInput),
    DimText(crate::dimension::DimInput, Point),
    HatchPattern,
    HatchScale(HatchRequest),
    HatchAngle(HatchRequest, f64),
    HatchSelection(HatchRequest, f64, f64),
    /// HLP `U`: angle, spacing (each a number or two points), then double.
    HatchUserAngle(HatchStyle),
    HatchUserAngleSecond(HatchStyle, Point),
    HatchUserSpacing(HatchStyle, f64),
    HatchUserSpacingSecond(HatchStyle, f64, Point),
    HatchUserDouble(HatchStyle, f64, f64),
    SketchIncrement,
    Sketch(Box<crate::sketch::Sketch>),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum EditCommand {
    Erase,
    Move,
    Copy,
    Rotate,
    Scale,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum RepeatDistanceInput {
    Number,
    Point,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) enum ArraySpacingInput {
    Number(f64),
    Point(Point),
}

impl InputState {
    pub(crate) fn prompt(&self) -> &'static str {
        match self {
            Self::Command => "Command",
            Self::ListSelection => "LIST: objects, ALL, LAST or W (Return ends picks)",
            Self::Selection(dialogue) => dialogue.prompt(),
            Self::LineStart => "LINE: first point",
            Self::LineNext { .. } => "LINE: next point (Enter to finish)",
            Self::CircleCenter => "CIRCLE: center point (or 2P/3P)",
            Self::CircleRadius(_) => "CIRCLE: radius or point (D for diameter)",
            Self::CircleDiameter(_) => "CIRCLE: diameter",
            Self::CircleTwoPointFirst => "CIRCLE 2P: first diameter endpoint",
            Self::CircleTwoPointSecond(_) => "CIRCLE 2P: second diameter endpoint",
            Self::CircleThreePointFirst => "CIRCLE 3P: first point",
            Self::CircleThreePointSecond(_) => "CIRCLE 3P: second point",
            Self::CircleThreePointThird(_, _) => "CIRCLE 3P: third point",
            Self::Point => "POINT: point",
            Self::ArcStart => "ARC: start point (C for center, Enter to continue)",
            Self::ArcMiddle(_) => "ARC: second point (C for center, E for end)",
            Self::ArcEnd(_, _) => "ARC: end point",
            Self::ArcCenterFirst | Self::ArcCenter(_) => "ARC: center point",
            Self::ArcStartAfterCenter(_) => "ARC: start point",
            Self::ArcCenterChoice(_, _) => "ARC: end direction point (A for angle, L for chord)",
            Self::ArcCenterAngle(_, _) | Self::ArcEndAngle(_, _) => "ARC: included angle",
            Self::ArcCenterChord(_, _) => "ARC: chord length",
            Self::ArcEndpoint(_) => "ARC: end point",
            Self::ArcEndChoice(_, _) => "ARC: R for radius, A for angle, D for starting direction",
            Self::ArcEndRadius(_, _) => "ARC: radius",
            Self::ArcEndDirection(_, _) => "ARC: starting direction angle or point",
            Self::ArcContinueEnd(_) => "ARC: continuation end point",
            Self::LoadLibrary => "LOAD: library name",
            Self::ShapeName => "SHAPE: shape name",
            Self::ShapeOrigin(_) => "SHAPE: insertion point",
            Self::ShapeHeight(_, _) => "SHAPE: height",
            Self::ShapeRotation(_, _, _) => "SHAPE: rotation angle",
            Self::Text(state) => state.prompt(),
            Self::InsertName => "INSERT: block or file name",
            Self::InsertOrigin(_, _) => "INSERT: insertion point",
            Self::InsertXScale(_, _) => "INSERT: X scale or opposite corner x,y (Enter for 1)",
            Self::InsertYScale(_, _, _) => "INSERT: Y scale (Enter for X scale)",
            Self::InsertRotation(_, _, _, _) => "INSERT: rotation angle (Enter for 0)",
            Self::BlockName => "BLOCK: block name",
            Self::BlockBase(_) => "BLOCK: insertion base point",
            Self::BlockSelection(_, _) => "BLOCK: entity numbers, ALL or LAST",
            Self::WblockPath => "WBLOCK: output file name",
            Self::WblockReplace {
                open_drawing: false,
                ..
            } => "WBLOCK: A drawing with this name already exists. Replace it? <N>",
            Self::WblockReplace {
                open_drawing: true, ..
            } => "WBLOCK: This is the open drawing's file. Replace it? <N>",
            Self::WblockName(_) => "WBLOCK: block name (* for entire drawing)",
            Self::WblockBase(_) => "WBLOCK: insertion base point",
            Self::WblockSelection(_, _) => "WBLOCK: entity numbers, ALL, or LAST",
            Self::HelpCommand => "Command name (RETURN for list)",
            Self::MenuFile => "File name",
            Self::ScriptFile => "SCRIPT: file name",
            Self::FilesMenu => "FILES: selection (0–7)",
            Self::FilesDrive(_) => "FILES: drive letter (A–Z)",
            Self::FilesListSpecification => "FILES: file specification",
            Self::FilesDeleteSpecification => "FILES: file deletion specification",
            Self::FilesRenameSource => "FILES: current filename",
            Self::FilesRenameDestination(_) => "FILES: new filename",
            Self::Delay => "DELAY: duration",
            Self::UnitsFormat => "UNITS: choice, 1 to 4",
            Self::UnitsPrecision(acad_model::UnitFormat::Architectural) => {
                "UNITS: denominator (1, 2, 4, 8, 16, 32, or 64)"
            }
            Self::UnitsPrecision(_) => "UNITS: digits to right of decimal point (0 to 8)",
            Self::SavePath => "SAVE: output file",
            Self::EndSavePath => "END: output file",
            // Short enough for an 800-pixel command line; the session's
            // status row carries the explanation.
            Self::OriginalErasureSave { quit: false, .. } => {
                "SAVE: Lose earlier member erasure? <N>"
            }
            Self::OriginalErasureSave { quit: true, .. } => "END: Lose earlier member erasure? <N>",
            Self::QuitConfirmation => "QUIT: really want to discard all changes? Y/YES",
            Self::Base => "BASE: x,y",
            Self::Axis => "AXIS: ON, OFF, or tick spacing (X for snap multiples)",
            Self::Snap => "SNAP: spacing",
            Self::Grid => "GRID: spacing",
            Self::Ortho => "ORTHO: ON or OFF",
            Self::Fill => "FILL: ON or OFF",
            Self::LimitsMin => "LIMITS: lower-left",
            Self::LimitsMax(_) => "LIMITS: upper-right",
            Self::Layer => "LAYER: index/ON/OFF/COLOR/?",
            Self::LayerVisibility(true) => "LAYER ON: layer indices (comma-separated)",
            Self::LayerVisibility(false) => "LAYER OFF: layer indices (comma-separated)",
            Self::LayerColor => "LAYER COLOR: color index",
            Self::ColorValue => "COLOR: color index",
            Self::ColorTargetLayer => "COLOR: next current layer (Enter to finish)",
            Self::View(input) => input.prompt(),
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
            Self::ArrayCircularAngle(_, _) => "ARRAY: angle between items (+=CCW, -=CW)",
            Self::ArrayCircularItems(_, _, _) => "ARRAY: number of items or -(degrees to fill)",
            Self::ArrayCircularRotate(..) => "ARRAY: rotate block as it is copied? <N>",
            Self::ArrayRows(_) => "ARRAY: number of rows",
            Self::ArrayColumns(_, _) => "ARRAY: number of columns",
            Self::ArrayRowSpacing(_, _, _) => "ARRAY: row spacing",
            Self::ArrayColumnSpacing(_, _, _, _) => "ARRAY: column spacing",
            Self::ChangeSelection => "CHANGE: entity numbers or ALL",
            Self::ChangeIntersection(_) => "CHANGE: intersection point or L",
            Self::ChangeLayer(_) => "CHANGE: new layer index",
            Self::ChangeProperties(state) => state.prompt(),
            Self::FilletSelection => "FILLET: select two LINE entities or R",
            Self::FilletRadius => "FILLET: radius",
            Self::BreakSelection => "BREAK: point to object, or one entity number",
            Self::BreakFirstPoint(_) => "BREAK: first point",
            Self::BreakSecondPoint(_, _, true) => "BREAK: second point or F (first point)",
            Self::BreakSecondPoint(_, _, false) => "BREAK: second point",
            Self::DistanceFirstPoint => "DIST: first point",
            Self::DistanceSecondPoint(_) => "DIST: second point",
            Self::IdPoint => "ID: point",
            Self::SolidFirstPoint => "SOLID: first point",
            Self::SolidSecondPoint(_) => "SOLID: second point",
            Self::SolidThirdPoint(_, _) => "SOLID: third point (Enter to finish)",
            Self::SolidFourthPoint(_, _, _) => "SOLID: fourth point (Enter for triangle)",
            Self::TraceWidth => "TRACE: width",
            Self::TraceStart(_) => "TRACE: first point",
            Self::TraceNext(_, _) => "TRACE: next point (Enter to finish)",
            Self::AreaFirstPoint => "AREA: first point",
            Self::AreaNextPoint(_) => "AREA: next point (Enter to finish)",
            Self::AreaSelection => "ENTITYAREA: entity numbers or ALL",
            Self::RepeatColumns => "ENDREP: columns",
            Self::RepeatRows(_) => "ENDREP: rows",
            Self::RepeatColumnSpacing(_, _, _) => "ENDREP: column distance",
            Self::RepeatRowSpacing(_, _, _, _, _) => "ENDREP: row distance",
            Self::DimFirstExtension => "DIM: first extension line origin or (ABCT)",
            Self::DimArrowSize => "DIM: dimension arrow size",
            Self::DimInsideHorizontalText {
                default_horizontal: true,
            } => "DIM: inside horizontal text? <Y>",
            Self::DimInsideHorizontalText {
                default_horizontal: false,
            } => "DIM: inside horizontal text? <N>",
            Self::DimOutsideHorizontalText {
                default_horizontal: true,
                ..
            } => "DIM: outside horizontal text? <Y>",
            Self::DimOutsideHorizontalText {
                default_horizontal: false,
                ..
            } => "DIM: outside horizontal text? <N>",
            Self::DimIntersection(_) => "DIM: dimension line intersection",
            Self::DimSecondExtension(_) => "DIM: second extension line origin",
            Self::DimText(_, _) => "DIM: dimension text",
            Self::HatchPattern => "HATCH: pattern (name,style / U / ?)",
            Self::HatchScale(_) => "HATCH: scale for pattern {1}",
            Self::HatchAngle(_, _) => "HATCH: angle for pattern {0}",
            Self::HatchSelection(_, _, _) => "HATCH: select objects on Window or Last",
            Self::HatchUserAngle(_) => "HATCH: angle for crosshatch lines",
            Self::HatchUserAngleSecond(..) => "HATCH: angle second point",
            Self::HatchUserSpacing(..) => "HATCH: spacing between lines",
            Self::HatchUserSpacingSecond(..) => "HATCH: spacing second point",
            Self::HatchUserDouble(..) => "HATCH: double hatch area (Y/N) <N>",
            Self::SketchIncrement => "SKETCH: record increment",
            Self::Sketch(_) => crate::sketch::SKETCH_PROMPT,
        }
    }
}
