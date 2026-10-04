#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Point {
    pub x: f64,
    pub y: f64,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Extents {
    pub xmin: f64,
    pub xmax: f64,
    pub ymin: f64,
    pub ymax: f64,
}

impl Extents {
    pub fn width(&self) -> f64 {
        self.xmax - self.xmin
    }
    pub fn height(&self) -> f64 {
        self.ymax - self.ymin
    }
    pub fn is_degenerate(&self) -> bool {
        self.width() <= 0.0 || self.height() <= 0.0
    }

    pub fn contains_point(&self, p: Point) -> bool {
        p.x >= self.xmin && p.x <= self.xmax && p.y >= self.ymin && p.y <= self.ymax
    }

    pub fn intersects(&self, other: &Extents) -> bool {
        self.xmin <= other.xmax
            && self.xmax >= other.xmin
            && self.ymin <= other.ymax
            && self.ymax >= other.ymin
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extents_dimensions_from_subdiv_limits() {
        let e = Extents {
            xmin: -2.0,
            xmax: 19.0,
            ymin: -2.0,
            ymax: 14.0,
        };
        assert_eq!(e.width(), 21.0);
        assert_eq!(e.height(), 16.0);
        assert!(!e.is_degenerate());
    }

    #[test]
    fn single_point_extents_are_degenerate() {
        let e = Extents {
            xmin: 3.0,
            xmax: 3.0,
            ymin: 4.0,
            ymax: 4.0,
        };
        assert!(e.is_degenerate());
    }

    #[test]
    fn extents_containment_and_intersection() {
        let box1 = Extents {
            xmin: 0.0,
            xmax: 10.0,
            ymin: 0.0,
            ymax: 10.0,
        };
        let box2 = Extents {
            xmin: 5.0,
            xmax: 15.0,
            ymin: 5.0,
            ymax: 15.0,
        };
        let box3 = Extents {
            xmin: 20.0,
            xmax: 30.0,
            ymin: 20.0,
            ymax: 30.0,
        };
        assert!(box1.contains_point(Point { x: 5.0, y: 5.0 }));
        assert!(!box1.contains_point(Point { x: 15.0, y: 5.0 }));
        assert!(box1.intersects(&box2));
        assert!(box2.intersects(&box1));
        assert!(!box1.intersects(&box3));
    }
}
