//! World-origin GRID dots shared by GUI and exported frames.
//! Limits clipping, density and pixel style are native Rust display policies.
use acad_model::{Header, Point};
use acad_render::Viewport;

const MAX_POINTS: u64 = 65_536;
const MIN_PIXELS: f64 = 8.0;
const COLOR: u32 = 0x0050_5050;

fn indices(min: f64, max: f64, spacing: f64) -> Option<(i64, i64)> {
    let a = (min / spacing).ceil();
    let b = (max / spacing).floor();
    const EXACT: f64 = (1_u64 << 53) as f64;
    (a.is_finite() && b.is_finite() && a <= b && a > -EXACT && b < EXACT)
        .then_some((a as i64, b as i64))
}

pub(crate) fn paint(
    buffer: &mut [u32],
    width: u32,
    height: u32,
    header: &Header,
    vp: &Viewport,
) -> usize {
    if !header.grid.on || width == 0 || height == 0 {
        return 0;
    }
    let base = if header.grid.spacing == 0.0 {
        header.snap.spacing
    } else {
        header.grid.spacing
    };
    if !base.is_finite() || base <= 0.0 {
        return 0;
    }
    let pixel_step = (vp.to_screen(Point { x: base, y: 0.0 }).x
        - vp.to_screen(Point { x: 0.0, y: 0.0 }).x)
        .abs();
    if !pixel_step.is_finite() || pixel_step <= 0.0 {
        return 0;
    }
    let a = vp.to_world(Point { x: 0.0, y: 0.0 });
    let b = vp.to_world(Point {
        x: f64::from(width - 1),
        y: f64::from(height - 1),
    });
    let limits = header.limits;
    if ![
        a.x,
        a.y,
        b.x,
        b.y,
        limits.xmin,
        limits.xmax,
        limits.ymin,
        limits.ymax,
    ]
    .iter()
    .all(|v| v.is_finite())
    {
        return 0;
    }
    let xmin = a.x.min(b.x).max(limits.xmin);
    let xmax = a.x.max(b.x).min(limits.xmax);
    let ymin = a.y.min(b.y).max(limits.ymin);
    let ymax = a.y.max(b.y).min(limits.ymax);
    // Display a coarser integer sublattice when zoomed out. Stored GRID and
    // SNAP intervals stay intact, so point placement remains exact.
    let mut spacing = base * (MIN_PIXELS / pixel_step).ceil().max(1.0);
    for _ in 0..64 {
        if !spacing.is_finite() || spacing <= 0.0 {
            return 0;
        }
        let Some((x0, x1)) = indices(xmin, xmax, spacing) else {
            return 0;
        };
        let Some((y0, y1)) = indices(ymin, ymax, spacing) else {
            return 0;
        };
        let count = ((x1 - x0 + 1) as u64).checked_mul((y1 - y0 + 1) as u64);
        if count.is_some_and(|count| count <= MAX_POINTS) {
            let mut painted = 0;
            for y in y0..=y1 {
                for x in x0..=x1 {
                    let p = vp.to_screen(Point {
                        x: x as f64 * spacing,
                        y: y as f64 * spacing,
                    });
                    let (x, y) = (p.x.round(), p.y.round());
                    if x >= 0.0 && x < f64::from(width) && y >= 0.0 && y < f64::from(height) {
                        if let Some(pixel) =
                            buffer.get_mut(y as usize * width as usize + x as usize)
                        {
                            // Geometry/selection strokes already occupy this pixel.
                            if *pixel == 0 {
                                *pixel = COLOR;
                                painted += 1;
                            }
                        }
                    }
                }
            }
            return painted;
        }
        spacing *= 2.0;
    }
    0
}

#[cfg(test)]
mod tests {
    use super::*;
    fn header() -> Header {
        let mut h = acad_cmd::Editor::default().drawing().header.clone();
        h.grid = acad_model::Mode {
            on: true,
            spacing: 1.0,
        };
        h.limits = acad_model::Extents {
            xmin: -1.0,
            xmax: 1.0,
            ymin: -1.0,
            ymax: 1.0,
        };
        h
    }
    #[test]
    fn grid_is_world_anchored_clipped_to_limits_and_stays_behind_geometry() {
        let h = header();
        let vp = Viewport::from_view(Point { x: 0.0, y: 0.0 }, 4.0, 80, 80);
        let mut pixels = vec![0; 80 * 80];
        pixels[40 * 80 + 40] = 0x00ff_0000;
        assert_eq!(paint(&mut pixels, 80, 80, &h, &vp), 8);
        for y in [20, 40, 60] {
            for x in [20, 40, 60] {
                assert_eq!(
                    pixels[y * 80 + x],
                    if x == 40 && y == 40 {
                        0x00ff_0000
                    } else {
                        COLOR
                    }
                );
            }
        }
        assert_eq!(pixels[40 * 80 + 10], 0);
        assert_eq!(pixels[40 * 80], 0);
        let vp = Viewport::from_view(Point { x: 0.5, y: 0.5 }, 4.0, 80, 80);
        let mut shifted = vec![0; 80 * 80];
        assert_eq!(paint(&mut shifted, 80, 80, &h, &vp), 9);
        assert_eq!(shifted[50 * 80 + 30], COLOR);
    }
    #[test]
    fn zero_spacing_uses_snap_even_when_snap_is_off_and_dense_grids_have_bounded_work() {
        let mut h = header();
        h.grid.spacing = 0.0;
        h.snap.spacing = 0.5;
        h.snap.on = false;
        let vp = Viewport::from_view(Point { x: 0.0, y: 0.0 }, 4.0, 80, 80);
        let mut pixels = vec![0; 80 * 80];
        assert_eq!(paint(&mut pixels, 80, 80, &h, &vp), 25);
        assert_eq!(pixels[30 * 80 + 30], COLOR);
        h.grid.spacing = 0.000001;
        let before = h.clone();
        let mut pixels = vec![0; 80 * 80];
        let count = paint(&mut pixels, 80, 80, &h, &vp);
        assert!((1..=81).contains(&count));
        assert_eq!(h, before);
    }
    #[test]
    fn disabled_invalid_and_unrepresentable_grids_paint_nothing() {
        let vp = Viewport::from_view(Point { x: 0.0, y: 0.0 }, 4.0, 80, 80);
        for spacing in [-1.0, f64::NAN, f64::INFINITY, f64::from_bits(1)] {
            let mut h = header();
            h.grid.spacing = spacing;
            let mut pixels = vec![0; 80 * 80];
            assert_eq!(paint(&mut pixels, 80, 80, &h, &vp), 0);
            assert!(pixels.iter().all(|p| *p == 0));
        }
        let mut h = header();
        h.grid.on = false;
        assert_eq!(paint(&mut [], 0, 0, &h, &vp), 0);
    }
}
