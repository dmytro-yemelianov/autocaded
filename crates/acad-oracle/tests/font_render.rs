#![cfg(unix)]

use acad_model::{Entity, Point};
use acad_render::{flatten_with_libraries, Libraries, Prim, Viewport};

fn bare(mut entity: &acad_model::Entity) -> &acad_model::Entity {
    while let acad_model::Entity::OnLayer { entity: inner, .. } = entity {
        entity = inner;
    }
    entity
}

fn lit(frame: &[u8], x: usize, y: usize) -> bool {
    frame[(y % 2) * 8192 + (y / 2) * 80 + x / 8] & (128 >> (x % 8)) != 0
}

#[test]
fn original_cga_text_and_shape_match_native_strokes() {
    let corpus = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../corpus");
    let images = corpus.join("raw/Autodesk AutoCAD 1.4 (5.25)");
    let system = images.join("System.img");
    let samples = images.join("Samples.img");
    if !system.exists() || !samples.exists() || !acad_oracle::available() {
        eprintln!("skipping oracle: extracted floppy images or qemu-system-i386 absent");
        return;
    }
    let probe = acad_oracle::generate_visual_pair(
        &system,
        Some(&samples),
        "ORCFONT",
        &[
            "LINE", "1,1", "11,1", "11,8", "1,8", "1,1", "", "TEXT", "2,3", "1", "0", "AA", "LOAD",
            "B:ES", "SHAPE", "RES", "7,3", "2", "0", "REDRAW",
        ],
    )
    .unwrap();
    assert_eq!(probe.cga.len(), 16384);
    let drawing = acad_dwg::parse(&probe.dwg).unwrap();
    assert_eq!(drawing.entities().count(), 7);
    assert_eq!(
        drawing
            .entities()
            .filter(|e| matches!(bare(e), Entity::Line { .. }))
            .count(),
        4
    );
    assert!(drawing
        .entities()
        .any(|e| matches!(bare(e), Entity::Text { value, .. } if value == "AA")));
    assert!(drawing
        .entities()
        .any(|e| matches!(bare(e), Entity::Shape { number: 129, .. })));
    let eof = probe.dxf.iter().position(|&b| b == 0x1a).unwrap() + 1;
    assert_eq!(acad_dxf::write(&drawing), probe.dxf[..eof]);

    // The rectangle encloses both probes. Its known world coordinates
    // calibrate CGA's unequal x/y pixel scales without guessing the driver
    // viewport or comparing modern antialiasing with 1983 rasterization.
    // Exclude the status row, side menu and bottom command area.
    let mut pixels = Vec::new();
    for y in 10..160 {
        for x in 10..550 {
            if lit(&probe.cga, x, y) {
                pixels.push(Point {
                    x: x as f64,
                    y: y as f64,
                });
            }
        }
    }
    let xmin = pixels.iter().map(|p| p.x).fold(f64::INFINITY, f64::min);
    let xmax = pixels.iter().map(|p| p.x).fold(f64::NEG_INFINITY, f64::max);
    let ymin = pixels.iter().map(|p| p.y).fold(f64::INFINITY, f64::min);
    let ymax = pixels.iter().map(|p| p.y).fold(f64::NEG_INFINITY, f64::max);
    assert!(
        xmax - xmin > 350. && ymax - ymin > 100.,
        "calibration rectangle missing"
    );
    let project = |p: Point| Point {
        x: xmin + (p.x - 1.) * (xmax - xmin) / 10.,
        y: ymax - (p.y - 1.) * (ymax - ymin) / 7.,
    };
    let (libraries, errors) = Libraries::load(&[corpus.join("System"), corpus.join("Samples")]);
    assert!(errors.is_empty(), "{errors:?}");
    let vp = Viewport::fit(&drawing.header.limits, 1200, 900);
    let rendered = flatten_with_libraries(&drawing, &vp, &libraries);
    assert!(
        rendered.diagnostics.is_empty(),
        "{:?}",
        rendered.diagnostics
    );

    for (name, lower, upper) in [
        ("AA", Point { x: 1.75, y: 2.75 }, Point { x: 4.25, y: 4.25 }),
        ("RES", Point { x: 6.75, y: 2.5 }, Point { x: 8.85, y: 3.5 }),
    ] {
        let lo = project(lower);
        let hi = project(upper);
        let inside = |p: &Point| p.x >= lo.x && p.x <= hi.x && p.y >= hi.y && p.y <= lo.y;
        let original: Vec<_> = pixels.iter().copied().filter(inside).collect();
        let mut native = Vec::new();
        for prim in &rendered.primitives {
            let points = match prim {
                Prim::Polyline(points) | Prim::ColoredPolyline { points, .. } => points,
                Prim::FilledPolygon(_) | Prim::ColoredFilledPolygon { .. } => continue,
            };
            for pair in points.windows(2) {
                let a = project(vp.to_world(pair[0]));
                let b = project(vp.to_world(pair[1]));
                let steps =
                    (((a.x - b.x).abs().max((a.y - b.y).abs()) * 2.).ceil() as usize).max(1);
                for i in 0..=steps {
                    let t = i as f64 / steps as f64;
                    let p = Point {
                        x: a.x + t * (b.x - a.x),
                        y: a.y + t * (b.y - a.y),
                    };
                    if inside(&p) {
                        native.push(p);
                    }
                }
            }
        }
        assert!(
            original.len() > 40 && native.len() > 80,
            "{name}: empty/trivial stroke coverage: {} / {}",
            original.len(),
            native.len()
        );
        let covered = |from: &[Point], to: &[Point]| {
            from.iter()
                .filter(|a| to.iter().any(|b| (a.x - b.x).hypot(a.y - b.y) <= 1.5))
                .count() as f64
                / from.len() as f64
        };
        let completeness = covered(&native, &original);
        let precision = covered(&original, &native);
        eprintln!("{name}: original {} pixels, native {} samples; completeness {completeness:.4}, precision {precision:.4}", original.len(), native.len());
        assert!(
            completeness >= 0.98 && precision >= 0.98,
            "{name}: completeness {completeness}, precision {precision}"
        );
    }
}
