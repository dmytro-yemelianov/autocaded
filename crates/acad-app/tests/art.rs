use acad_app::art::{Primitive, Recipe};
use acad_model::{Entity, Item};
fn recipe() -> Recipe {
    Recipe::parse(br#"{"schema_version":1,"id":"fixture","bounds":[0,0,10,10],"layers":[{"number":1,"color":7,"role":"outline"}],"primitives":[{"type":"line","layer":1,"start":[0,0],"end":[10,10]},{"type":"rectangle","layer":1,"min":[1,1],"max":[3,4],"filled":true},{"type":"grid","layer":1,"origin":[5,1],"columns":2,"rows":2,"spacing":[2,2],"size":[1,1],"filled":false}]}"#).unwrap()
}
#[test]
fn ordered_helpers_roundtrip_deterministically_and_remain_editable() {
    let r = recipe();
    let drawing = r.compile().unwrap();
    assert_eq!(drawing.items.len(), 18);
    assert!(
        matches!(&drawing.items[0],Item::Entity(Entity::OnLayer{entity,..}) if matches!(**entity,Entity::Line{..}))
    );
    let bytes = acad_dwg::write(&drawing).unwrap();
    assert_eq!(bytes, acad_dwg::write(&r.compile().unwrap()).unwrap());
    for decoded in [
        acad_dwg::parse(&bytes).unwrap(),
        acad_dxf::parse(&acad_dxf::try_write(&drawing).unwrap()).unwrap(),
    ] {
        assert_eq!(decoded.items, drawing.items);
    }
    let mut s = acad_app::Session::default();
    s.open_drawing(drawing.clone());
    s.command("LINE").unwrap();
    s.command("0,1").unwrap();
    s.command("1,2").unwrap();
    s.command("").unwrap();
    assert_eq!(s.drawing().items.len(), 19);
    s.command("UNDO").unwrap();
    assert_eq!(s.drawing().items, drawing.items);
}
#[test]
fn rejects_unknown_fields_and_invalid_geometry() {
    assert!(Recipe::parse(br#"{"schema_version":1,"id":"x","bounds":[0,0,1,1],"layers":[],"primitives":[],"commands":[]}"#).is_err());
    let mut r = recipe();
    r.schema_version = 2;
    assert!(r.compile().is_err());
    r.schema_version = 1;
    r.primitives = vec![Primitive::Line {
        layer: 1,
        start: [f64::NAN, 0.0],
        end: [1.0, 1.0],
    }];
    assert!(r.compile().is_err());
    r.primitives = vec![Primitive::Circle {
        layer: 1,
        center: [0.0, 0.0],
        radius: 1.0,
    }];
    assert!(r.compile().is_err());
    r.primitives = vec![Primitive::Solid {
        layer: 1,
        points: [[0.0, 0.0], [2.0, 2.0], [0.0, 2.0], [2.0, 0.0]],
    }];
    assert!(r.compile().is_err());
    r.primitives = vec![Primitive::Solid {
        layer: 1,
        points: [[0.0, 0.0], [2.0, 0.0], [1.0, 2.0], [1.0, 2.0]],
    }];
    assert!(r.compile().is_ok());
    r.primitives = vec![Primitive::Grid {
        layer: 1,
        origin: [0.0, 0.0],
        columns: 500,
        rows: 500,
        spacing: [1.0, 1.0],
        size: [0.5, 0.5],
        filled: true,
    }];
    assert!(r.compile().is_err());
    r.primitives = vec![Primitive::Line {
        layer: 2,
        start: [0.0, 0.0],
        end: [1.0, 1.0],
    }];
    assert!(r.compile().is_err());
}

#[test]
fn decimal_helper_sums_match_historical_dxf_precision() {
    let mut r = recipe();
    r.primitives = vec![Primitive::Grid {
        layer: 1,
        origin: [1.4, 1.6],
        columns: 2,
        rows: 1,
        spacing: [2.1, 0.0],
        size: [1.0, 1.2],
        filled: true,
    }];
    let drawing = r.compile().unwrap();
    let dxf = acad_dxf::parse(&acad_dxf::try_write(&drawing).unwrap()).unwrap();
    assert_eq!(dxf.items, drawing.items);
    let mut invalid = recipe();
    invalid.layers[0].color = 255;
    assert!(invalid.compile().is_err());
    invalid.layers[0].color = 7;
    invalid.layers[0].number = 128;
    assert!(invalid.compile().is_err());
}

#[test]
fn quantized_geometry_must_remain_inside_authored_and_saved_bounds() {
    let mut r = recipe();
    r.bounds = [0.0, 0.0, 0.00000302, 0.00000302];
    r.primitives = vec![Primitive::Circle {
        layer: 1,
        center: [0.00000151, 0.00000151],
        radius: 0.00000151,
    }];
    assert!(r
        .compile()
        .unwrap_err()
        .contains("quantized geometry exceeds"));
    r.bounds = [0.0, 0.0, 1.0000006, 1.0000006];
    r.primitives = vec![Primitive::Line {
        layer: 1,
        start: [0.0, 0.0],
        end: [1.0000006, 1.0000006],
    }];
    assert!(r
        .compile()
        .unwrap_err()
        .contains("quantized geometry exceeds"));
    r.primitives = vec![Primitive::Solid {
        layer: 1,
        points: [
            [0.0, 0.0],
            [1.0000006, 0.0],
            [0.0, 1.0000006],
            [1.0000006, 1.0000006],
        ],
    }];
    assert!(r
        .compile()
        .unwrap_err()
        .contains("quantized geometry exceeds"));
    r.primitives = vec![Primitive::Rectangle {
        layer: 1,
        min: [0.0, 0.0],
        max: [1.0000006, 1.0000006],
        filled: true,
    }];
    assert!(r
        .compile()
        .unwrap_err()
        .contains("quantized geometry exceeds"));
    r.bounds = [0.0, 0.0, 10.0, 10.0];
    r.primitives = vec![Primitive::Line {
        layer: 1,
        start: [0.0000001, 0.0],
        end: [0.0000002, 0.0],
    }];
    assert!(r.compile().is_err());
    r.primitives = vec![Primitive::Rectangle {
        layer: 1,
        min: [1.0, 1.0],
        max: [1.0000001, 2.0],
        filled: true,
    }];
    assert!(r.compile().is_err());
    r.primitives = vec![Primitive::Solid {
        layer: 1,
        points: [[1.0, 1.0], [1.0000001, 1.0], [1.0, 2.0], [1.0000001, 2.0]],
    }];
    assert!(r.compile().is_err());
}

fn text_recipe() -> Recipe {
    Recipe::parse(br#"{"schema_version":1,"id":"caption","bounds":[0,0,100,60],"layers":[{"number":1,"color":7,"role":"caption"}],"font":{"id":"autocaded","sha256":"2c0b7a2f00899f306b2f15615967478a996e53225e0ab9ae446cfa4cba9f0404"},"primitives":[{"type":"text","layer":1,"origin":[5,10],"height":3,"value":"UNDO fixes everything"}]}"#).unwrap()
}
fn text_libraries() -> acad_render::Libraries {
    let mut libraries = acad_render::Libraries::default();
    libraries
        .insert("TXT", include_bytes!("../../../demo/AUTOCADED.SHP"))
        .unwrap();
    libraries
}
#[test]
fn captions_use_actual_font_bounds_and_reject_missing_glyphs() {
    let mut recipe = text_recipe();
    let libraries = text_libraries();
    assert!(recipe.compile().unwrap_err().contains("validated TXT"));
    let drawing = recipe.compile_with_libraries(&libraries).unwrap();
    let vp = acad_render::Viewport::fit(&drawing.header.extents, 800, 600);
    let rendered = acad_render::flatten_with_libraries(&drawing, &vp, &libraries);
    assert!(!rendered.incomplete);
    assert!(rendered.diagnostics.is_empty());
    assert!(!rendered.primitives.is_empty());
    recipe.bounds[2] = 12.0;
    assert!(recipe
        .compile_with_libraries(&libraries)
        .unwrap_err()
        .contains("exceeds"));
    recipe.bounds = [0.0, 0.0, 100.0, 60.0];
    if let Primitive::Text {
        origin,
        rotation_deg,
        value,
        ..
    } = &mut recipe.primitives[0]
    {
        *origin = [20.0, 5.0];
        *rotation_deg = 90.0;
        *value = "UNDO".into();
    }
    assert!(recipe.compile_with_libraries(&libraries).is_ok());
    if let Primitive::Text { value, .. } = &mut recipe.primitives[0] {
        *value = "`".into();
    }
    let mut missing = acad_render::Libraries::default();
    missing
        .insert("TXT", b"*0,4,TEST\n10,2,0,0\n*65,4,A\n8,1,0,0\n")
        .unwrap();
    assert!(recipe.compile_with_libraries(&missing).is_err());
    if let Primitive::Text { value, .. } = &mut recipe.primitives[0] {
        *value = "УНДО".into();
    }
    assert!(recipe.compile_with_libraries(&libraries).is_err());
    recipe.font = None;
    assert!(recipe.compile_with_libraries(&libraries).is_err());
}
#[test]
fn caption_sidecar_opens_edits_saves_and_roundtrips_both_codecs() {
    let drawing = text_recipe()
        .compile_with_libraries(&text_libraries())
        .unwrap();
    let root = std::env::temp_dir().join(format!("autorust-art-caption-{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    std::fs::write(
        root.join("TXT.SHP"),
        include_bytes!("../../../demo/AUTOCADED.SHP"),
    )
    .unwrap();
    for ext in ["DWG", "DXF"] {
        let path = root.join(format!("caption.{ext}"));
        let bytes = if ext == "DWG" {
            acad_dwg::write_version(&drawing, acad_dwg::header::Version::Ac140).unwrap()
        } else {
            acad_dxf::try_write(&drawing).unwrap()
        };
        std::fs::write(&path, bytes).unwrap();
        let mut session = acad_app::Session::open(&path, &[]).unwrap();
        assert_eq!(session.drawing().items, drawing.items);
        let frame = session.frame(800, 600).unwrap();
        assert!(frame.complete && frame.diagnostics.is_empty());
        for input in ["CHANGE", "LAST", "5,10", "", "0", "UNDO works"] {
            session.command(input).unwrap();
        }
        assert_ne!(session.drawing().items, drawing.items);
        session.save(&path).unwrap();
        assert_eq!(
            acad_app::Session::open(&path, &[])
                .unwrap()
                .drawing()
                .items
                .iter()
                .filter(|item| !matches!(item, Item::Erased(_)))
                .cloned()
                .collect::<Vec<_>>(),
            session
                .drawing()
                .items
                .iter()
                .filter(|item| !matches!(item, Item::Erased(_)))
                .cloned()
                .collect::<Vec<_>>()
        );
        session.command("UNDO").unwrap();
        assert_eq!(session.drawing().items, drawing.items);
    }
    std::fs::remove_dir_all(root).unwrap();
}
