use crate::geom::{Extents, Point};
use std::collections::{BTreeMap, BTreeSet};

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

impl DwgView {
    /// Native canvas usability, including rounded world corners and the actual
    /// pixel/world scale. This validates a view without changing stored data.
    pub fn validate_canvas(self, aspect: f64, pixel_height: u32) -> Result<(), &'static str> {
        let half_width = self.height * aspect / 2.0;
        let half_height = self.height / 2.0;
        let min = Point {
            x: self.center.x - half_width,
            y: self.center.y - half_height,
        };
        let max = Point {
            x: self.center.x + half_width,
            y: self.center.y + half_height,
        };
        let pixels_per_unit = f64::from(pixel_height) / self.height;
        let units_per_pixel = self.height / f64::from(pixel_height);
        if pixel_height == 0
            || !aspect.is_finite()
            || aspect <= 0.0
            || !self.height.is_finite()
            || self.height <= 0.0
            || !half_width.is_finite()
            || half_width <= 0.0
            || half_height <= 0.0
            || ![self.center.x, self.center.y, min.x, min.y, max.x, max.y]
                .into_iter()
                .all(f64::is_finite)
            || !(min.x < self.center.x
                && self.center.x < max.x
                && min.y < self.center.y
                && self.center.y < max.y)
            || !pixels_per_unit.is_finite()
            || pixels_per_unit <= 0.0
            || !units_per_pixel.is_finite()
            || units_per_pixel <= 0.0
        {
            return Err(
                "view must have distinct finite corners and a positive finite pixel/world scale",
            );
        }
        Ok(())
    }
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
    /// Drawing-scoped FILLET radius. AC1.40 stores it at 0x1fa; AC1.2 and
    /// historical comma DXF have no evidenced nonzero mapping.
    pub fillet_radius: f64,
    /// Explicit DIMARROW scalar. DXF omission and AC1.40 positive-zero bits
    /// decode as None; negative zero remains explicit. This is a file presence
    /// rule, not a native command default. AC1.2 has no evidenced mapping.
    /// Encoding Some(+0.0) to AC1.40 loses presence on reparse. With None,
    /// the DWG writer preserves passthrough bytes (or its existing zero fill).
    pub dim_arrow: Option<f64>,
    pub units: Units,
    pub current_layer: u8,
    /// Defined layers, keyed by index, valued by colour index. Slots a file
    /// leaves unused are absent; native visibility is separate from colors.
    pub layers: BTreeMap<u8, u8>,
    /// Native visibility policy. Historical off-layer encoding is unverified;
    /// DWG/DXF writers reject nonempty state rather than lose visibility.
    pub off_layers: BTreeSet<u8>,
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
            && self.fillet_radius == other.fillet_radius
            && self.dim_arrow == other.dim_arrow
            && self.units == other.units
            && self.current_layer == other.current_layer
            && self.layers == other.layers
            && self.off_layers == other.off_layers
    }
}

impl Header {
    pub fn layer_is_visible(&self, layer: u8) -> bool {
        !self.off_layers.contains(&layer)
    }

    /// Visibility gate for a live drawing item; IDs remain independent of this.
    pub fn item_is_visible(&self, item: &crate::Item) -> bool {
        match item {
            crate::Item::Entity(entity) => self.entity_is_visible(entity),
            crate::Item::Repeat(repeat) => {
                repeat.entities.iter().any(|e| self.entity_is_visible(e))
            }
            crate::Item::Block(_) | crate::Item::Erased(_) => false,
        }
    }

    /// Visibility gate for a record. Bare geometry belongs to layer 1;
    /// bare REPEAT markers have no owner layer and filter their children.
    /// An outer layer wrapper gates an entire INSERT or REPEAT group.
    /// LOAD records always run so hiding geometry does not change libraries.
    pub fn entity_is_visible(&self, entity: &crate::Entity) -> bool {
        match entity {
            crate::Entity::Erased(_) => false,
            crate::Entity::Load { .. } => true,
            crate::Entity::Repeat(repeat) => {
                repeat.entities.iter().any(|e| self.entity_is_visible(e))
            }
            crate::Entity::OnLayer { layer, entity } => {
                if matches!(entity.as_ref(), crate::Entity::Load { .. }) {
                    true
                } else {
                    self.layer_is_visible(*layer)
                        && match entity.as_ref() {
                            crate::Entity::OnLayer { .. }
                            | crate::Entity::Repeat(_)
                            | crate::Entity::Erased(_) => self.entity_is_visible(entity),
                            _ => true,
                        }
                }
            }
            _ => self.layer_is_visible(1),
        }
    }
}
