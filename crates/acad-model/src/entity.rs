use crate::geom::Point;

/// Angles are degrees, counter-clockwise, as stored by AutoCAD 1.4.
///
/// `Point`, `Trace` and `Solid` (added by Task 8) are geometry-only, on
/// purpose: a 1983 `TRACE`/`SOLID` record is a fixed-width run of doubles with
/// its own field count and fill-flag policy, but none of that is a property
/// of "four corners" as a shape, so it stays in `acad-dwg`'s codec. This
/// model just holds the four points.
#[derive(Debug, Clone, PartialEq)]
pub enum Entity {
    /// An erased ordinary stored member. It retains geometry/layer fields but
    /// has no live geometry or library effect. Structural wrappers are checked
    /// codec errors; top-level erased owners use Item::Erased instead.
    Erased(Box<Entity>),
    /// A rectangular pattern nested in a block definition.
    Repeat(crate::drawing::Repeat),
    /// Layer index stored in the entity record header. Wrapping keeps layer
    /// metadata orthogonal to geometry while preserving source compatibility
    /// for callers constructing geometry directly (default layer 1).
    OnLayer {
        layer: u8,
        entity: Box<Entity>,
    },
    /// Load a shape library or select a text font for subsequent records.
    /// Keep this in document order, including inside block definitions.
    Load {
        name: String,
    },
    /// A numbered shape from a previously loaded library; height is scale.
    Shape {
        origin: Point,
        height: f64,
        rotation_deg: f64,
        number: u16,
    },
    Line {
        start: Point,
        end: Point,
    },
    Circle {
        center: Point,
        radius: f64,
    },
    Arc {
        center: Point,
        radius: f64,
        start_deg: f64,
        end_deg: f64,
    },
    Text {
        origin: Point,
        height: f64,
        rotation_deg: f64,
        value: String,
    },
    Insert {
        origin: Point,
        x_scale: f64,
        y_scale: f64,
        rotation_deg: f64,
        name: String,
    },
    Point {
        origin: Point,
    },
    /// Four corners, in file order. Nothing about winding order or which
    /// pair forms which edge is asserted here — the DWG codec that reads
    /// `TRACE` has no DXF oracle to confirm it against (see
    /// `acad-dwg/src/entity.rs`'s module doc), so this is deliberately just
    /// "four points," not a claim about how they connect.
    Trace {
        p1: Point,
        p2: Point,
        p3: Point,
        p4: Point,
    },
    /// Four corners, in file order — same caveat as `Trace`: inferred from
    /// record size and internal consistency, not a DXF oracle.
    Solid {
        p1: Point,
        p2: Point,
        p3: Point,
        p4: Point,
    },
    /// Generic unmodeled record preserved for lossless DXF round-tripping.
    Generic(GenericEntity),
    /// Extensible entity slot for third-party or advanced entity types.
    Extension(Box<dyn CustomEntity>),
}

/// A generic data-driven entity representing an unmodeled or custom CAD record.
/// Retains its keyword, layer index, and raw DXF text rows for lossless round-tripping.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GenericEntity {
    pub type_name: String,
    pub layer: u8,
    pub rows: Vec<String>,
}

/// Trait for extensible third-party or newer AutoCAD entity types.
/// Allows adding new entity definitions without modifying the core engine enums.
pub trait CustomEntity: std::fmt::Debug + Send + Sync + std::panic::RefUnwindSafe + std::panic::UnwindSafe {
    /// The CAD entity type name (e.g., "LWPOLYLINE", "3DFACE", "ELLIPSE").

    fn type_name(&self) -> &str;

    /// Layer index (0..=255) for this entity.
    fn layer(&self) -> u8 {
        1
    }

    /// World-space bounding extents if statically determinable.
    fn bounding_extents(&self) -> Option<crate::geom::Extents> {
        None
    }

    /// Tessellate or approximate the entity as polylines for rendering.
    fn tessellate_points(&self) -> Vec<Vec<Point>> {
        Vec::new()
    }

    /// Serialize entity to DXF rows (excluding the header `KEYWORD,LAYER\r\n`).
    fn dxf_rows(&self) -> Vec<String> {
        Vec::new()
    }

    /// Clone helper for dynamic trait objects.
    fn clone_box(&self) -> Box<dyn CustomEntity>;
}

impl Clone for Box<dyn CustomEntity> {
    fn clone(&self) -> Self {
        self.clone_box()
    }
}

impl PartialEq for Box<dyn CustomEntity> {
    fn eq(&self, other: &Self) -> bool {
        self.type_name() == other.type_name()
            && self.layer() == other.layer()
            && self.bounding_extents() == other.bounding_extents()
            && self.dxf_rows() == other.dxf_rows()
    }
}

impl Entity {
    /// Erased status beneath layer wrappers. Malformed excessive wrapper depth
    /// is conservatively excluded from live traversal; codecs validate it separately.
    pub fn is_erased(&self) -> bool {
        let mut entity = self;
        for _ in 0..=crate::EngineLimits::DEFAULT_1983.max_stored_depth {
            match entity {
                Self::OnLayer { entity: inner, .. } => entity = inner,
                Self::Erased(_) => return true,
                _ => return false,
            }
        }
        true
    }

    /// World-space bounding extents of the entity if statically determinable.
    pub fn bounding_extents(&self) -> Option<crate::geom::Extents> {
        use crate::geom::Extents;
        match self {
            Self::OnLayer { entity, .. } => entity.bounding_extents(),
            Self::Erased(entity) => entity.bounding_extents(),
            Self::Point { origin } => Some(Extents {
                xmin: origin.x,
                xmax: origin.x,
                ymin: origin.y,
                ymax: origin.y,
            }),
            Self::Line { start, end } => Some(Extents {
                xmin: start.x.min(end.x),
                xmax: start.x.max(end.x),
                ymin: start.y.min(end.y),
                ymax: start.y.max(end.y),
            }),
            Self::Circle { center, radius } | Self::Arc { center, radius, .. } => {
                let r = radius.abs();
                Some(Extents {
                    xmin: center.x - r,
                    xmax: center.x + r,
                    ymin: center.y - r,
                    ymax: center.y + r,
                })
            }
            Self::Trace { p1, p2, p3, p4 } | Self::Solid { p1, p2, p3, p4 } => {
                Extents::from_points(&[*p1, *p2, *p3, *p4])
            }
            Self::Text { origin, height, .. } => {
                let h = height.abs();
                Some(Extents {
                    xmin: origin.x - h,
                    xmax: origin.x + h,
                    ymin: origin.y - h,
                    ymax: origin.y + h,
                })
            }
            Self::Shape { origin, height, .. } => {
                let h = height.abs();
                Some(Extents {
                    xmin: origin.x - h,
                    xmax: origin.x + h,
                    ymin: origin.y - h,
                    ymax: origin.y + h,
                })
            }
            Self::Repeat(repeat) => {
                let mut ext: Option<Extents> = None;
                for e in &repeat.entities {
                    if let Some(e_ext) = e.bounding_extents() {
                        ext = Some(match ext {
                            Some(cur) => cur.union(&e_ext),
                            None => e_ext,
                        });
                    }
                }
                let base = ext?;
                let col_offset =
                    f64::from(repeat.columns.saturating_sub(1)) * repeat.column_spacing;
                let row_offset = f64::from(repeat.rows.saturating_sub(1)) * repeat.row_spacing;
                Some(Extents {
                    xmin: base.xmin.min(base.xmin + col_offset),
                    xmax: base.xmax.max(base.xmax + col_offset),
                    ymin: base.ymin.min(base.ymin + row_offset),
                    ymax: base.ymax.max(base.ymax + row_offset),
                })
            }
            Self::Insert { .. } | Self::Load { .. } | Self::Generic(_) => None,
            Self::Extension(ext) => ext.bounding_extents(),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Block {
    pub name: String,
    pub base: Point,
    pub entities: Vec<Entity>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Debug, Clone)]
    struct MockPolyline {
        points: Vec<Point>,
        layer: u8,
    }

    impl CustomEntity for MockPolyline {
        fn type_name(&self) -> &str {
            "LWPOLYLINE"
        }
        fn layer(&self) -> u8 {
            self.layer
        }
        fn bounding_extents(&self) -> Option<crate::geom::Extents> {
            crate::geom::Extents::from_points(&self.points)
        }
        fn tessellate_points(&self) -> Vec<Vec<Point>> {
            vec![self.points.clone()]
        }
        fn dxf_rows(&self) -> Vec<String> {
            vec![format!("POINTS,{}", self.points.len())]
        }
        fn clone_box(&self) -> Box<dyn CustomEntity> {
            Box::new(self.clone())
        }
    }

    #[test]
    fn generic_and_extension_entities_work() {
        let generic = Entity::Generic(GenericEntity {
            type_name: "3DFACE".into(),
            layer: 5,
            rows: vec!["0,0,0".into(), "1,1,1".into()],
        });
        assert!(!generic.is_erased());
        assert_eq!(generic.bounding_extents(), None);

        let custom = Entity::Extension(Box::new(MockPolyline {
            points: vec![Point { x: 0.0, y: 0.0 }, Point { x: 10.0, y: 20.0 }],
            layer: 2,
        }));
        assert!(!custom.is_erased());
        let ext = custom.bounding_extents().unwrap();
        assert_eq!(ext.xmin, 0.0);
        assert_eq!(ext.xmax, 10.0);
        assert_eq!(ext.ymin, 0.0);
        assert_eq!(ext.ymax, 20.0);
    }

    #[test]
    fn point_holds_an_origin() {
        let p = Entity::Point {
            origin: Point { x: 1.5, y: -2.5 },
        };
        let Entity::Point { origin } = p else {
            panic!("expected a Point");
        };
        assert_eq!(origin, Point { x: 1.5, y: -2.5 });
    }

    #[test]
    fn trace_holds_four_corners() {
        let t = Entity::Trace {
            p1: Point { x: 0.0, y: 0.0 },
            p2: Point { x: 1.0, y: 0.0 },
            p3: Point { x: 1.0, y: 1.0 },
            p4: Point { x: 0.0, y: 1.0 },
        };
        let Entity::Trace { p1, p2, p3, p4 } = t else {
            panic!("expected a Trace");
        };
        assert_eq!(p1, Point { x: 0.0, y: 0.0 });
        assert_eq!(p2, Point { x: 1.0, y: 0.0 });
        assert_eq!(p3, Point { x: 1.0, y: 1.0 });
        assert_eq!(p4, Point { x: 0.0, y: 1.0 });
    }

    #[test]
    fn solid_holds_four_corners() {
        let s = Entity::Solid {
            p1: Point { x: 2.5, y: 0.5 },
            p2: Point { x: 7.0, y: 0.5 },
            p3: Point { x: 7.0, y: 0.55 },
            p4: Point { x: 2.5, y: 0.55 },
        };
        let Entity::Solid { p1, p2, p3, p4 } = s else {
            panic!("expected a Solid");
        };
        assert_eq!(p1, Point { x: 2.5, y: 0.5 });
        assert_eq!(p4, Point { x: 2.5, y: 0.55 });
        assert_eq!(p2.x, 7.0);
        assert_eq!(p3.x, 7.0);
    }

    #[test]
    fn test_bounding_extents() {
        let line = Entity::Line {
            start: Point { x: 1.0, y: 2.0 },
            end: Point { x: 5.0, y: -3.0 },
        };
        let ext = line.bounding_extents().unwrap();
        assert_eq!(ext.xmin, 1.0);
        assert_eq!(ext.xmax, 5.0);
        assert_eq!(ext.ymin, -3.0);
        assert_eq!(ext.ymax, 2.0);

        let circle = Entity::Circle {
            center: Point { x: 10.0, y: 10.0 },
            radius: 5.0,
        };
        let ext_c = circle.bounding_extents().unwrap();
        assert_eq!(ext_c.xmin, 5.0);
        assert_eq!(ext_c.xmax, 15.0);
        assert_eq!(ext_c.ymin, 5.0);
        assert_eq!(ext_c.ymax, 15.0);
    }
}

