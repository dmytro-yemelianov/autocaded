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
}
