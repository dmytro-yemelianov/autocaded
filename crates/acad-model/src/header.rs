use crate::geom::{Extents, Point};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Mode {
    pub on: bool,
    pub spacing: f64,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DwgView {
    pub center: Point,
    pub height: f64,
}

/// The distance syntax selected by the AutoCAD 1.4 `UNITS` command.
///
/// AC1.40 stores the menu choice and its precision in the extended header.
/// The same precision field is a number of decimal places for the first
/// three formats and an inch-fraction denominator for Architectural.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnitFormat {
    Scientific,
    Decimal,
    Engineering,
    Architectural,
}

impl UnitFormat {
    pub fn from_disk(value: u16) -> Self {
        match value {
            1 => Self::Scientific,
            2 => Self::Decimal,
            3 => Self::Engineering,
            4 => Self::Architectural,
            _ => Self::Decimal,
        }
    }

    pub fn disk_value(self) -> u16 {
        match self {
            Self::Scientific => 1,
            Self::Decimal => 2,
            Self::Engineering => 3,
            Self::Architectural => 4,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Units {
    pub format: UnitFormat,
    pub precision: u16,
}

#[derive(Debug, Clone)]
pub struct Header {
    pub extents: Extents,
    pub limits: Extents,
    pub base: Point,
    pub view: DwgView,
    pub axis: Mode,
    pub snap: Mode,
    pub grid: Mode,
    pub ortho: bool,
    pub fill: bool,
    pub text_size: f64,
    pub trace_width: f64,
    pub units: Units,
    pub current_layer: u8,
    /// Defined layers, keyed by index, valued by colour index. Slots a file
    /// leaves unused are simply absent: the 1983 fixed-width layer table is
    /// the DXF codec's concern, not the model's, so this can grow named
    /// layers and per-layer state without disturbing either codec.
    pub layers: BTreeMap<u8, u8>,
    /// Original fixed DWG header bytes, retained so the DWG writer can carry
    /// through fields this model has not identified yet. This is codec
    /// metadata, not drawing semantics, so it is intentionally omitted from
    /// `PartialEq`.
    pub dwg_header_passthrough: Option<Vec<u8>>,
}

impl PartialEq for Header {
    fn eq(&self, other: &Self) -> bool {
        self.extents == other.extents
            && self.limits == other.limits
            && self.base == other.base
            && self.view == other.view
            && self.axis == other.axis
            && self.snap == other.snap
            && self.grid == other.grid
            && self.ortho == other.ortho
            && self.fill == other.fill
            && self.text_size == other.text_size
            && self.trace_width == other.trace_width
            && self.units == other.units
            && self.current_layer == other.current_layer
            && self.layers == other.layers
    }
}
