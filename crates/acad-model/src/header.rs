use crate::geom::{Extents, Point};

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
    pub layer_colors: [u8; 128],
}
