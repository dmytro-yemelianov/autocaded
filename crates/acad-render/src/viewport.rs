use acad_model::{Extents, Point};

#[derive(Debug, Clone, Copy)]
pub struct Viewport {
    pub width: u32,
    pub height: u32,
    scale: f64,
    offset: Point,
}

impl Viewport {
    /// `from_view` for a saved view that must be usable on this canvas:
    /// distinct finite world corners and a finite positive pixel/world scale.
    pub fn checked_from_view(
        center: Point,
        height: f64,
        width: u32,
        pixel_height: u32,
    ) -> Result<Self, String> {
        if width == 0 || pixel_height == 0 {
            return Err("viewport dimensions must be positive".into());
        }
        acad_model::DwgView { center, height }
            .validate_canvas(f64::from(width) / f64::from(pixel_height), pixel_height)
            .map_err(str::to_owned)?;
        Ok(Self::from_view(center, height, width, pixel_height))
    }

    /// Construct a viewport from the drawing's saved view. `height` is the
    /// world-space vertical span; the horizontal span follows the canvas
    /// aspect ratio. Unchecked: callers displaying a stored view should use
    /// `checked_from_view` and choose their own fallback when it is refused.
    pub fn from_view(center: Point, height: f64, width: u32, pixel_height: u32) -> Self {
        Self {
            width,
            height: pixel_height,
            scale: pixel_height as f64 / height,
            offset: center,
        }
    }

    pub fn fit(e: &Extents, width: u32, height: u32) -> Self {
        // A degenerate box has no extent to fit; fall back to 1:1 so the
        // transform stays finite and the drawing is simply centred.
        let (w, h) = (e.width().max(f64::EPSILON), e.height().max(f64::EPSILON));
        let scale = if e.is_degenerate() {
            1.0
        } else {
            (width as f64 / w).min(height as f64 / h)
        };
        let cx = (e.xmin + e.xmax) / 2.0;
        let cy = (e.ymin + e.ymax) / 2.0;
        Self {
            width,
            height,
            scale,
            offset: Point { x: cx, y: cy },
        }
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
        let e = Extents {
            xmin: 0.0,
            xmax: 10.0,
            ymin: 0.0,
            ymax: 10.0,
        };
        let vp = Viewport::fit(&e, 100, 100);
        let origin = vp.to_screen(Point { x: 0.0, y: 0.0 });
        let top = vp.to_screen(Point { x: 0.0, y: 10.0 });
        assert!(origin.y > top.y, "world +y must map to screen -y");
        assert!((origin.y - 100.0).abs() < 1.0);
        assert!((top.y - 0.0).abs() < 1.0);
    }

    #[test]
    fn fit_preserves_aspect_ratio() {
        let e = Extents {
            xmin: 0.0,
            xmax: 20.0,
            ymin: 0.0,
            ymax: 10.0,
        };
        let vp = Viewport::fit(&e, 200, 200);
        let a = vp.to_screen(Point { x: 0.0, y: 0.0 });
        let b = vp.to_screen(Point { x: 20.0, y: 10.0 });
        assert!(((b.x - a.x).abs() - 200.0).abs() < 1.0);
        assert!(((b.y - a.y).abs() - 100.0).abs() < 1.0);
    }

    #[test]
    fn degenerate_extents_do_not_divide_by_zero() {
        let e = Extents {
            xmin: 3.0,
            xmax: 3.0,
            ymin: 4.0,
            ymax: 4.0,
        };
        let vp = Viewport::fit(&e, 100, 100);
        let p = vp.to_screen(Point { x: 3.0, y: 4.0 });
        assert!(p.x.is_finite() && p.y.is_finite());
    }

    #[test]
    fn to_world_inverts_to_screen() {
        let e = Extents {
            xmin: -2.0,
            xmax: 19.0,
            ymin: -2.0,
            ymax: 14.0,
        };
        let vp = Viewport::fit(&e, 800, 600);
        let w = Point { x: 7.25, y: 3.5 };
        let back = vp.to_world(vp.to_screen(w));
        assert!((back.x - w.x).abs() < 1e-9 && (back.y - w.y).abs() < 1e-9);
    }

    #[test]
    fn saved_view_uses_its_world_height_and_center() {
        let vp = Viewport::from_view(Point { x: 7.0, y: 4.0 }, 5.0, 200, 100);
        assert_eq!(
            vp.to_screen(Point { x: 7.0, y: 4.0 }),
            Point { x: 100.0, y: 50.0 }
        );
        assert_eq!(
            vp.to_screen(Point { x: 7.0, y: 6.5 }),
            Point { x: 100.0, y: 0.0 }
        );
    }

    #[test]
    fn checked_saved_view_refuses_collapsed_corners_and_infinite_scale() {
        let huge = Point { x: 1e308, y: 1e308 };
        let origin = Point { x: 0.0, y: 0.0 };
        assert!(Viewport::checked_from_view(huge, 1.0, 400, 222).is_err());
        assert!(Viewport::checked_from_view(origin, 1e-308, 400, 222).is_err());
        assert!(Viewport::checked_from_view(origin, 1.0, 0, 222).is_err());
        // The same tiny height is usable on a one-pixel-high canvas.
        let vp = Viewport::checked_from_view(origin, 1e-308, 400, 1).unwrap();
        assert_eq!(vp.to_screen(origin), Point { x: 200.0, y: 0.5 });
        assert!(Viewport::checked_from_view(huge, 1e300, 400, 222).is_ok());
    }
}
