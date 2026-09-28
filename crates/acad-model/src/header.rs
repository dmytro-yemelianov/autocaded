use crate::geom::{Extents, Point};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Mode { pub on: bool, pub spacing: f64 }

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DwgView { pub center: Point, pub height: f64 }

#[derive(Debug, Clone, PartialEq)]
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
}
