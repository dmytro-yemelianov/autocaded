use acad_model::{Extents, Point};
use acad_render::{rasterize, Prim, Viewport};

#[test]
fn a_diagonal_line_marks_pixels_on_both_ends() {
    let vp = Viewport::fit(
        &Extents {
            xmin: 0.0,
            xmax: 10.0,
            ymin: 0.0,
            ymax: 10.0,
        },
        64,
        64,
    );
    let prims = vec![Prim::Polyline(vec![
        vp.to_screen(Point { x: 1.0, y: 1.0 }),
        vp.to_screen(Point { x: 9.0, y: 9.0 }),
    ])];
    let pm = rasterize(&prims, 64, 64);
    // `rasterize` fills an opaque black background and draws only opaque
    // white strokes, so every pixel's alpha is 255 regardless of what was
    // drawn (an_empty_drawing_rasterizes_to_uniform_opaque_background below
    // pins exactly that for an empty scene) — content must be told apart by
    // colour, not alpha.
    let lit = pm
        .pixels()
        .iter()
        .filter(|p| p.red() > 0 || p.green() > 0 || p.blue() > 0)
        .count();
    assert!(lit > 40, "expected a drawn diagonal, got {lit} lit pixels");
}

#[test]
fn an_empty_drawing_rasterizes_to_uniform_opaque_background() {
    let pm = rasterize(&[], 32, 32);
    assert!(pm
        .pixels()
        .iter()
        .all(|p| p.alpha() == 255 && p.red() == 0 && p.green() == 0 && p.blue() == 0));
}

#[test]
fn background_is_opaque_so_strokes_are_visible_when_composited() {
    // White strokes on a transparent background composite to white-on-white
    // in any viewer that flattens onto a light page: the drawing vanishes.
    let vp = Viewport::fit(
        &Extents {
            xmin: 0.0,
            xmax: 10.0,
            ymin: 0.0,
            ymax: 10.0,
        },
        64,
        64,
    );
    let prims = vec![Prim::Polyline(vec![
        vp.to_screen(Point { x: 1.0, y: 1.0 }),
        vp.to_screen(Point { x: 9.0, y: 9.0 }),
    ])];
    let pm = rasterize(&prims, 64, 64);
    assert!(
        pm.pixels().iter().all(|p| p.alpha() == 255),
        "every pixel must be opaque"
    );
    let bright = pm.pixels().iter().filter(|p| p.red() > 128).count();
    assert!(
        bright > 40,
        "expected bright strokes over a dark field, got {bright}"
    );
}

#[test]
fn indexed_layer_color_reaches_the_rendered_pixels() {
    let pm = rasterize(
        &[Prim::ColoredPolyline {
            points: vec![Point { x: 3.0, y: 12.0 }, Point { x: 28.0, y: 12.0 }],
            rgb: [0, 0, 255],
        }],
        32,
        24,
    );
    let pixel = pm.pixel(15, 12).unwrap();
    assert_eq!((pixel.red(), pixel.green()), (0, 0));
    assert!(pixel.blue() > 0);
}

#[test]
fn filled_polygon_colors_its_interior() {
    let pm = rasterize(
        &[Prim::FilledPolygon(vec![
            Point { x: 4.0, y: 4.0 },
            Point { x: 28.0, y: 4.0 },
            Point { x: 28.0, y: 28.0 },
            Point { x: 4.0, y: 28.0 },
            Point { x: 4.0, y: 4.0 },
        ])],
        32,
        32,
    );
    let inside = pm.pixel(16, 16).unwrap();
    let outside = pm.pixel(1, 1).unwrap();
    assert_eq!(
        (inside.red(), inside.green(), inside.blue()),
        (255, 255, 255)
    );
    assert_eq!((outside.red(), outside.green(), outside.blue()), (0, 0, 0));
}

#[test]
fn subdiv_renders_a_non_blank_image() {
    let Ok(bytes) = std::fs::read("../../corpus/Samples/SUBDIV.DXF") else {
        eprintln!("skipping: corpus absent (run ./tools/extract-corpus.sh)");
        return;
    };
    let d = acad_dxf::parse(&bytes).unwrap();
    let vp = Viewport::fit(&d.header.limits, 800, 600);
    let pm = rasterize(&acad_render::flatten(&d, &vp), 800, 600);
    // `rasterize`'s background is opaque black and its strokes are opaque
    // white, so alpha is 255 everywhere regardless of what was drawn —
    // an_empty_drawing_rasterizes_to_uniform_opaque_background above pins
    // exactly that for an empty scene, which made the old alpha-based count
    // here unable to ever fail. Content must be told apart by colour.
    let lit = pm
        .pixels()
        .iter()
        .filter(|p| p.red() > 0 || p.green() > 0 || p.blue() > 0)
        .count();
    assert!(
        lit > 5000,
        "SUBDIV should draw substantial geometry, got {lit}"
    );
}
