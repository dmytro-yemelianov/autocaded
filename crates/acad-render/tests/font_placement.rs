use acad_model::{Extents, Point};
use acad_render::{flatten_with_libraries, Libraries, Prim, Viewport};

fn libraries() -> Libraries {
    let mut libraries = Libraries::default();
    libraries
        .insert("TXT", b"*0,4,Vertical\n2,1,0,0\n*65,5,A\n024,2,030,1,0")
        .unwrap();
    libraries
        .insert("ALT", b"*0,4,Horizontal\n4,1,0,0\n*65,5,A\n040,2,010,1,0")
        .unwrap();
    libraries.insert("ES", b"*129,4,R\n3,2,024,0").unwrap();
    libraries
}
fn viewport() -> Viewport {
    Viewport::fit(
        &Extents {
            xmin: -10.,
            xmax: 30.,
            ymin: -10.,
            ymax: 30.,
        },
        800,
        800,
    )
}
fn world(prim: &Prim, vp: &Viewport) -> Vec<Point> {
    let Prim::Polyline(points) = prim else {
        panic!("expected stroke")
    };
    points.iter().map(|p| vp.to_world(*p)).collect()
}
fn near(a: Point, x: f64, y: f64) {
    assert!(
        (a.x - x).abs() < 1e-10 && (a.y - y).abs() < 1e-10,
        "{a:?} != ({x},{y})"
    );
}

#[test]
fn text_rotation_and_block_transform_compose_with_font_cap_scaling() {
    let drawing = acad_dxf::parse(b"BLOCK,1\r\n1,2\r\nLABEL\r\nTEXT,1\r\n2,3,0.5,30\r\nA\r\nENDBLK,1\r\nINSERT,1\r\n10,20,2,3,90\r\nLABEL\r\n").unwrap();
    let vp = viewport();
    let output = flatten_with_libraries(&drawing, &vp, &libraries());
    assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
    assert_eq!(output.primitives.len(), 1);
    let points = world(&output.primitives[0], &vp);
    near(points[0], 7., 22.);
    // Vertical .5 stroke rotated 30 degrees -> (-.25, sqrt(3)/4).
    // Outer scales (2,3), then rotates 90 degrees -> (-3*sqrt(3)/4,-.5).
    near(points[1], 7. - 3. * 3f64.sqrt() / 4., 21.5);
}

#[test]
fn load_changes_subsequent_text_and_shapes_do_not_replace_the_font() {
    let drawing = acad_dxf::parse(b"TEXT,1\r\n0,0,1,0\r\nA\r\nLOAD,1\r\nalt\r\nTEXT,1\r\n2,0,1,0\r\nA\r\nLOAD,1\r\nB:ES\r\nSHAPE,1\r\n4,0,2,90,129\r\nTEXT,1\r\n6,0,1,0\r\nA\r\n").unwrap();
    let vp = viewport();
    let output = flatten_with_libraries(&drawing, &vp, &libraries());
    assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
    assert_eq!(output.primitives.len(), 4);
    near(world(&output.primitives[0], &vp)[1], 0., 1.);
    near(world(&output.primitives[1], &vp)[1], 3., 0.);
    near(world(&output.primitives[2], &vp)[1], 2., 0.);
    near(world(&output.primitives[3], &vp)[1], 7., 0.);
}

#[test]
fn missing_libraries_and_glyphs_are_reported() {
    let drawing = acad_dxf::parse(b"TEXT,1\r\n0,0,1,0\r\nB\r\nLOAD,1\r\nMISSING\r\nTEXT,1\r\n2,0,1,0\r\nA\r\nSHAPE,1\r\n4,0,1,0,999\r\n").unwrap();
    let output = flatten_with_libraries(&drawing, &viewport(), &libraries());
    assert!(output.primitives.is_empty());
    for expected in [
        "missing shape 66",
        "LOAD: missing SHP library MISSING",
        "TEXT: missing SHP font MISSING",
        "SHAPE 999",
    ] {
        assert!(
            output.diagnostics.iter().any(|s| s.contains(expected)),
            "{:?}",
            output.diagnostics
        );
    }
}
