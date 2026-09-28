use acad_model::{Extents, Point};

#[derive(Debug, Clone, Copy)]
pub struct Viewport { pub width: u32, pub height: u32, scale: f64, offset: Point }

impl Viewport {
    pub fn fit(e: &Extents, width: u32, height: u32) -> Self {
        // A degenerate box has no extent to fit; fall back to 1:1 so the
        // transform stays finite and the drawing is simply centred.
        let (w, h) = (e.width().max(f64::EPSILON), e.height().max(f64::EPSILON));
        let scale = if e.is_degenerate() { 1.0 }
                    else { (width as f64 / w).min(height as f64 / h) };
        let cx = (e.xmin + e.xmax) / 2.0;
        let cy = (e.ymin + e.ymax) / 2.0;
        Self { width, height, scale, offset: Point { x: cx, y: cy } }
    }

    pub fn to_screen(&self, p: Point) -> Point {
        Point {
            x: (p.x - self.offset.x) * self.scale + self.width as f64 / 2.0,
            y: self.height as f64 / 2.0 - (p.y - self.offset.y) * self.scale,
        }
    }

    pub fn to_world(&self, p: Point) -> Point {
        Point {
            x: (p.x - self.width as f64 / 2.0) / self.scale + self.offset.x,
            y: (self.height as f64 / 2.0 - p.y) / self.scale + self.offset.y,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use acad_model::{Extents, Point};

    #[test]
    fn fit_centres_and_flips_y() {
        let e = Extents { xmin: 0.0, xmax: 10.0, ymin: 0.0, ymax: 10.0 };
        let vp = Viewport::fit(&e, 100, 100);
        let origin = vp.to_screen(Point { x: 0.0, y: 0.0 });
        let top = vp.to_screen(Point { x: 0.0, y: 10.0 });
        assert!(origin.y > top.y, "world +y must map to screen -y");
        assert!((origin.y - 100.0).abs() < 1.0);
        assert!((top.y - 0.0).abs() < 1.0);
    }

    #[test]
    fn fit_preserves_aspect_ratio() {
        let e = Extents { xmin: 0.0, xmax: 20.0, ymin: 0.0, ymax: 10.0 };
        let vp = Viewport::fit(&e, 200, 200);
        let a = vp.to_screen(Point { x: 0.0, y: 0.0 });
        let b = vp.to_screen(Point { x: 20.0, y: 10.0 });
        assert!(((b.x - a.x).abs() - 200.0).abs() < 1.0);
        assert!(((b.y - a.y).abs() - 100.0).abs() < 1.0);
    }

    #[test]
    fn degenerate_extents_do_not_divide_by_zero() {
        let e = Extents { xmin: 3.0, xmax: 3.0, ymin: 4.0, ymax: 4.0 };
        let vp = Viewport::fit(&e, 100, 100);
        let p = vp.to_screen(Point { x: 3.0, y: 4.0 });
        assert!(p.x.is_finite() && p.y.is_finite());
    }

    #[test]
    fn to_world_inverts_to_screen() {
        let e = Extents { xmin: -2.0, xmax: 19.0, ymin: -2.0, ymax: 14.0 };
        let vp = Viewport::fit(&e, 800, 600);
        let w = Point { x: 7.25, y: 3.5 };
        let back = vp.to_world(vp.to_screen(w));
        assert!((back.x - w.x).abs() < 1e-9 && (back.y - w.y).abs() < 1e-9);
    }
}
