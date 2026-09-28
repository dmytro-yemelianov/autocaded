#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Point { pub x: f64, pub y: f64 }

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Extents { pub xmin: f64, pub xmax: f64, pub ymin: f64, pub ymax: f64 }

impl Extents {
    pub fn width(&self) -> f64 { self.xmax - self.xmin }
    pub fn height(&self) -> f64 { self.ymax - self.ymin }
    pub fn is_degenerate(&self) -> bool { self.width() <= 0.0 || self.height() <= 0.0 }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extents_dimensions_from_subdiv_limits() {
        let e = Extents { xmin: -2.0, xmax: 19.0, ymin: -2.0, ymax: 14.0 };
        assert_eq!(e.width(), 21.0);
        assert_eq!(e.height(), 16.0);
        assert!(!e.is_degenerate());
    }

    #[test]
    fn single_point_extents_are_degenerate() {
        let e = Extents { xmin: 3.0, xmax: 3.0, ymin: 4.0, ymax: 4.0 };
        assert!(e.is_degenerate());
    }
}
