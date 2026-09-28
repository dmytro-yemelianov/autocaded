use crate::flatten::Prim;
use tiny_skia::{Color, Paint, PathBuilder, Pixmap, Stroke, Transform};

pub fn rasterize(prims: &[Prim], width: u32, height: u32) -> Pixmap {
    let mut pm = Pixmap::new(width, height).expect("non-zero pixmap dimensions");
    // Opaque black field, bright strokes: a transparent background would
    // composite white-on-white and the drawing would vanish in any viewer.
    pm.fill(Color::BLACK);
    let mut paint = Paint::default();
    paint.set_color_rgba8(255, 255, 255, 255);
    paint.anti_alias = true;
    let stroke = Stroke {
        width: 1.0,
        ..Stroke::default()
    };

    for Prim::Polyline(pts) in prims {
        if pts.len() < 2 {
            continue;
        }
        let mut pb = PathBuilder::new();
        pb.move_to(pts[0].x as f32, pts[0].y as f32);
        for p in &pts[1..] {
            pb.line_to(p.x as f32, p.y as f32);
        }
        if let Some(path) = pb.finish() {
            pm.stroke_path(&path, &paint, &stroke, Transform::identity(), None);
        }
    }
    pm
}
