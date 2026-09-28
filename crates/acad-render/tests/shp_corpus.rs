use acad_render::{flatten, flatten_with_libraries, shp::Library, Libraries, Viewport};

#[test]
fn every_supplied_font_and_shape_program_executes() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../corpus");
    if !root.join("Samples").exists() {
        eprintln!("skipping: corpus absent");
        return;
    }
    for (name, count, cap) in [
        ("Samples/TXT", 99, Some(6.)),
        ("System/TXT", 98, Some(21.)),
        ("Samples/ROMAN-S", 98, Some(21.)),
        ("Samples/ROMAN-C", 98, Some(21.)),
        ("Samples/ITALIC", 93, Some(21.)),
        ("Samples/ES", 10, None),
        ("Samples/PC", 8, None),
    ] {
        let bytes = std::fs::read(root.join(format!("{name}.SHP"))).unwrap();
        let library = Library::parse(&bytes).unwrap();
        assert_eq!(library.numbers().count(), count, "{name}");
        assert_eq!(library.cap_height, cap, "{name}");
        for id in library.numbers().filter(|id| *id >= 32) {
            let glyph = if cap.is_some() {
                library.text(&char::from_u32(id as u32).unwrap().to_string())
            } else {
                library.shape(id)
            }
            .unwrap_or_else(|e| panic!("{name} {id}: {e}"));
            assert!(glyph
                .strokes
                .iter()
                .flatten()
                .all(|p| p.x.is_finite() && p.y.is_finite()));
        }
    }
}

#[test]
fn all_21_drawings_render_without_missing_fonts_or_glyphs() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../corpus/Samples");
    if !root.exists() {
        eprintln!("skipping: corpus absent");
        return;
    }
    let (libraries, diagnostics) = Libraries::for_drawing(&root.join("DISC.BAK"), &[]);
    assert!(diagnostics.is_empty(), "{diagnostics:?}");
    assert_eq!(libraries.get("TXT").unwrap().cap_height, Some(21.));
    assert_eq!(libraries.get("B:TXT").unwrap().cap_height, Some(6.));
    assert_eq!(libraries.get("A:TXT").unwrap().cap_height, Some(21.));
    let (local, diagnostics) =
        Libraries::for_drawing(&root.join("DISC.BAK"), std::slice::from_ref(&root));
    assert!(diagnostics.is_empty(), "{diagnostics:?}");
    assert_eq!(local.get("TXT").unwrap().cap_height, Some(6.));
    let mut count = 0;
    for entry in std::fs::read_dir(&root).unwrap() {
        let path = entry.unwrap().path();
        if path.extension().is_none_or(|s| s != "DWG") && path.file_name().unwrap() != "DISC.BAK" {
            continue;
        }
        let drawing = acad_dwg::parse(&std::fs::read(&path).unwrap()).unwrap();
        let vp = Viewport::fit(&drawing.header.limits, 1200, 900);
        let rendered = flatten_with_libraries(&drawing, &vp, &libraries);
        assert!(
            rendered.diagnostics.is_empty(),
            "{}: {:?}",
            path.display(),
            rendered.diagnostics
        );
        if path.file_name().unwrap() == "DISC.BAK" || path.file_name().unwrap() == "SUBDIV.DWG" {
            assert!(
                rendered.primitives.len() > flatten(&drawing, &vp).len(),
                "{}: text added no geometry",
                path.display()
            );
        }
        count += 1;
    }
    assert_eq!(count, 21);
}
