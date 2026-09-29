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

#[derive(Debug, Clone)]
pub struct Header {
    pub extents: Extents,
    pub limits: Extents,
    pub base: Point,
    pub view: DwgView,
    pub snap: Mode,
    pub grid: Mode,
    pub ortho: bool,
    pub fill: bool,
    pub text_size: f64,
    pub trace_width: f64,
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
            && self.snap == other.snap
            && self.grid == other.grid
            && self.ortho == other.ortho
            && self.fill == other.fill
            && self.text_size == other.text_size
            && self.trace_width == other.trace_width
            && self.current_layer == other.current_layer
            && self.layers == other.layers
    }
}
