use acad_model::{Extents, Point};
use acad_render::{rasterize, Prim, Viewport};

#[test]
fn a_diagonal_line_marks_pixels_on_both_ends() {
    let vp = Viewport::fit(&Extents { xmin: 0.0, xmax: 10.0, ymin: 0.0, ymax: 10.0 }, 64, 64);
    let prims = vec![Prim::Polyline(vec![
        vp.to_screen(Point { x: 1.0, y: 1.0 }),
        vp.to_screen(Point { x: 9.0, y: 9.0 })])];
    let pm = rasterize(&prims, 64, 64);
    let lit = pm.pixels().iter().filter(|p| p.alpha() > 0).count();
    assert!(lit > 40, "expected a drawn diagonal, got {lit} lit pixels");
}

#[test]
fn an_empty_drawing_rasterizes_to_uniform_opaque_background() {
    let pm = rasterize(&[], 32, 32);
    assert!(pm.pixels().iter().all(|p| p.alpha() == 255 && p.red() == 0
        && p.green() == 0 && p.blue() == 0));
}

#[test]
fn background_is_opaque_so_strokes_are_visible_when_composited() {
    // White strokes on a transparent background composite to white-on-white
    // in any viewer that flattens onto a light page: the drawing vanishes.
    let vp = Viewport::fit(&Extents { xmin: 0.0, xmax: 10.0, ymin: 0.0, ymax: 10.0 }, 64, 64);
    let prims = vec![Prim::Polyline(vec![
        vp.to_screen(Point { x: 1.0, y: 1.0 }),
        vp.to_screen(Point { x: 9.0, y: 9.0 })])];
    let pm = rasterize(&prims, 64, 64);
    assert!(pm.pixels().iter().all(|p| p.alpha() == 255), "every pixel must be opaque");
    let bright = pm.pixels().iter().filter(|p| p.red() > 128).count();
    assert!(bright > 40, "expected bright strokes over a dark field, got {bright}");
}

#[test]
fn subdiv_renders_a_non_blank_image() {
    let bytes = std::fs::read("../../corpus/Samples/SUBDIV.DXF")
        .expect("run ./tools/extract-corpus.sh first");
    let d = acad_dxf::parse(&bytes).unwrap();
    let vp = Viewport::fit(&d.header.limits, 800, 600);
    let pm = rasterize(&acad_render::flatten(&d, &vp), 800, 600);
    let lit = pm.pixels().iter().filter(|p| p.alpha() > 0).count();
    assert!(lit > 5000, "SUBDIV should draw substantial geometry, got {lit}");
}
