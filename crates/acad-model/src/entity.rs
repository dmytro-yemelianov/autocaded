use crate::geom::Point;

/// Angles are degrees, counter-clockwise, as stored by AutoCAD 1.4.
#[derive(Debug, Clone, PartialEq)]
pub enum Entity {
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
}

#[derive(Debug, Clone, PartialEq)]
pub struct Block {
    pub name: String,
    pub base: Point,
    pub entities: Vec<Entity>,
}
