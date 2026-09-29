#[cfg(unix)]
#[test]
fn original_creates_circles_with_the_requested_centers_and_radii() {
    use acad_model::{Entity, Item, Point};

    let disk = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../corpus/raw/Autodesk AutoCAD 1.4 (5.25)/System.img");
    if !disk.exists() || !acad_oracle::available() {
        eprintln!("skipping oracle: extracted System.img or qemu-system-i386 absent");
        return;
    }

    let cases = [(4.0, 3.0, 2.0), (7.25, 5.5, 1.25)];
    let mut lines = Vec::new();
    let mut expected = Vec::new();
    for (x, y, radius) in cases {
        lines.extend(["CIRCLE".to_owned(), format!("{x},{y}"), radius.to_string()]);
        expected.push(Item::Entity(Entity::OnLayer {
            layer: 1,
            entity: Box::new(Entity::Circle {
                center: Point { x, y },
                radius,
            }),
        }));
    }
    let lines: Vec<&str> = lines.iter().map(String::as_str).collect();
    let (dwg, dxf) = acad_oracle::generate_pair(&disk, "ORCCIRC", &lines).unwrap();
    assert_eq!(&dwg[..6], b"AC1.40");
    let mut drawing = acad_dxf::parse(&dxf).unwrap();
    assert_eq!(drawing.items, expected, "original CIRCLE command geometry");

    // Expected entities come from the requested inputs. Preserve only the
    // original's header, then compare our writer with its actual DXF bytes.
    drawing.items = expected;
    let end = dxf.iter().position(|&b| b == 0x1a).unwrap() + 1;
    assert_eq!(acad_dxf::write(&drawing), dxf[..end]);
    let from_dwg = acad_dwg::parse(&dwg).unwrap();
    assert_eq!(acad_dxf::write(&from_dwg), dxf[..end]);
    let (_, meta) = acad_dwg::header::parse_header(&dwg).unwrap();
    for cut in 0..meta.entity_end as usize {
        assert!(
            acad_dwg::parse(&dwg[..cut]).is_err(),
            "accepted truncated DWG at {cut:#x}"
        );
    }
}

#[cfg(unix)]
#[test]
fn original_repeat_round_trips_and_matches_the_rust_command() {
    use acad_model::{Entity, Item, Point};
    let disk = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../corpus/raw/Autodesk AutoCAD 1.4 (5.25)/System.img");
    if !disk.exists() {
        return;
    }
    let inputs = [
        "REPEAT", "LINE", "1,1", "2,1", "", "ENDREP", "2", "2", "3", "4",
    ];
    let dwg = acad_oracle::generate_dwg_in_tree(&disk, "ORCREP", &inputs).unwrap();
    let (_, meta) = acad_dwg::header::parse_header(&dwg).unwrap();
    assert_eq!(meta.entity_count, 3);
    let parsed = acad_dwg::parse(&dwg).unwrap();
    let [Item::Repeat(repeat)] = parsed.items.as_slice() else {
        panic!("expected one REPEAT group");
    };
    assert_eq!((repeat.columns, repeat.rows), (2, 2));
    assert_eq!((repeat.column_spacing, repeat.row_spacing), (3.0, 4.0));
    assert!(
        matches!(repeat.entities.as_slice(), [Entity::OnLayer { layer: 1, entity }] if matches!(entity.as_ref(), Entity::Line { start: Point { x: 1.0, y: 1.0 }, end: Point { x: 2.0, y: 1.0 } }))
    );
    assert_eq!(acad_dwg::write(&parsed).unwrap(), dwg);
    let mut editor = acad_cmd::Editor::default();
    for input in inputs {
        editor.submit(input).unwrap();
    }
    assert_eq!(editor.drawing().items, parsed.items);
    if acad_oracle::available() {
        let (qdwg, dxf) = acad_oracle::generate_pair(&disk, "ORCREP", &inputs).unwrap();
        assert_eq!(dwg, qdwg);
        let exported = acad_dxf::parse(&dxf).unwrap();
        assert_eq!(exported.items, parsed.items);
        let end = dxf.iter().position(|&b| b == 0x1a).unwrap() + 1;
        assert_eq!(acad_dxf::write(&parsed), dxf[..end]);
    }
    let multi = [
        "REPEAT", "LINE", "1,1", "2,1", "", "POINT", "1,2", "ENDREP", "2", "2", "3", "4",
    ];
    let multi_dwg = acad_oracle::generate_dwg_in_tree(&disk, "ORCREP2", &multi).unwrap();
    let (_, mm) = acad_dwg::header::parse_header(&multi_dwg).unwrap();
    assert_eq!(mm.entity_count, 4);
    let multi_parsed = acad_dwg::parse(&multi_dwg).unwrap();
    let [Item::Repeat(multi_repeat)] = multi_parsed.items.as_slice() else {
        panic!("expected multi-entity REPEAT group");
    };
    assert_eq!(multi_repeat.entities.len(), 2);
    assert_eq!(acad_dwg::write(&multi_parsed).unwrap(), multi_dwg);
}

#[cfg(unix)]
#[test]
fn original_move_and_copy_use_displacement_before_last_selection() {
    use acad_model::{Entity, Item, Point};
    let disk = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../corpus/raw/Autodesk AutoCAD 1.4 (5.25)/System.img");
    if !disk.exists() || !acad_oracle::available() {
        return;
    }
    let original_line = Item::Entity(Entity::OnLayer {
        layer: 1,
        entity: Box::new(Entity::Line {
            start: Point { x: 1.0, y: 1.0 },
            end: Point { x: 2.0, y: 1.0 },
        }),
    });
    let moved_line = Item::Entity(Entity::OnLayer {
        layer: 1,
        entity: Box::new(Entity::Line {
            start: Point { x: 4.0, y: 5.0 },
            end: Point { x: 5.0, y: 5.0 },
        }),
    });
    for (name, command, first, second) in [
        ("ORCMOVE", "MOVE", "3,4", ""),
        ("ORCCOPY", "COPY", "3,4", ""),
        ("ORCMOV2", "MOVE", "1,1", "4,5"),
    ] {
        let input = ["LINE", "1,1", "2,1", "", command, first, second, "L"];
        let dwg = acad_oracle::generate_dwg(&disk, name, &input).unwrap();
        let expected = if command == "COPY" {
            vec![original_line.clone(), moved_line.clone()]
        } else {
            vec![moved_line.clone()]
        };
        assert_eq!(
            acad_dwg::parse(&dwg).unwrap().items,
            expected,
            "{command} original"
        );
        let mut editor = acad_cmd::Editor::default();
        for line in input {
            editor.submit(line).unwrap();
        }
        assert_eq!(editor.drawing().items, expected, "{command} Rust");
    }
}

#[cfg(unix)]
#[test]
fn original_erase_marks_the_record_and_oops_restores_it() {
    use acad_model::{Entity, Item, Point};
    let disk = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../corpus/raw/Autodesk AutoCAD 1.4 (5.25)/System.img");
    if !disk.exists() || !acad_oracle::available() {
        return;
    }
    let line = Entity::OnLayer {
        layer: 1,
        entity: Box::new(Entity::Line {
            start: Point { x: 1.0, y: 1.0 },
            end: Point { x: 2.0, y: 1.0 },
        }),
    };
    for (name, restore) in [("ORCERAS", false), ("ORCOOPS", true)] {
        let mut input = vec!["LINE", "1,1", "2,1", "", "ERASE", "L"];
        if restore {
            input.push("OOPS");
        }
        let dwg = acad_oracle::generate_dwg(&disk, name, &input).unwrap();
        let (_, meta) = acad_dwg::header::parse_header(&dwg).unwrap();
        assert_eq!(meta.entity_count, 1);
        let expected = if restore {
            Item::Entity(line.clone())
        } else {
            Item::Erased(line.clone())
        };
        assert_eq!(
            i16::from_le_bytes(dwg[0x202..0x204].try_into().unwrap()),
            if restore { 1 } else { -1 }
        );
        let decoded = acad_dwg::parse(&dwg).unwrap();
        assert_eq!(decoded.items, [expected.clone()]);
        let mut editor = acad_cmd::Editor::default();
        for command in input {
            editor.submit(command).unwrap();
        }
        assert_eq!(editor.drawing().items, [expected]);
        let rust_dwg = acad_dwg::write(editor.drawing()).unwrap();
        let (_, rust_meta) = acad_dwg::header::parse_header(&rust_dwg).unwrap();
        assert_eq!(
            &rust_dwg[0x202..rust_meta.entity_end as usize],
            &dwg[0x202..meta.entity_end as usize]
        );
    }
}

#[cfg(unix)]
#[test]
fn block_last_marks_the_source_erased_and_adds_an_uppercase_definition() {
    use acad_model::{Entity, Item, Point};

    let disk = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../corpus/raw/Autodesk AutoCAD 1.4 (5.25)/System.img");
    if !disk.exists() {
        return;
    }
    let inputs = [
        "LINE", "2,3", "4,5", "", "CIRCLE", "5,6", "1", "BLOCK", "b1", "1,2", "LAST",
    ];
    let original = acad_oracle::generate_dwg_in_tree(&disk, "ORCBLOCK", &inputs).unwrap();
    let decoded = acad_dwg::parse(&original).unwrap();
    assert_eq!(decoded.items.len(), 3);
    assert!(matches!(decoded.items[0], Item::Entity(_)));
    assert!(matches!(decoded.items[1], Item::Erased(_)));
    let Item::Block(block) = &decoded.items[2] else {
        panic!("original did not create a block");
    };
    assert_eq!(block.name, "B1");
    assert_eq!(block.base, Point { x: 1.0, y: 2.0 });
    assert_eq!(block.entities.len(), 1);
    assert!(matches!(
        block.entities[0],
        Entity::OnLayer { ref entity, layer: 1 }
            if matches!(entity.as_ref(), Entity::Circle { center: Point { x: 5.0, y: 6.0 }, radius: 1.0 })
    ));

    let mut rust = acad_cmd::Editor::default();
    for input in inputs {
        rust.submit(input).unwrap();
    }
    assert_eq!(rust.drawing().items, decoded.items);
    let saved = acad_dwg::write(rust.drawing()).unwrap();
    assert_eq!(acad_dwg::parse(&saved).unwrap().items, decoded.items);
    if acad_oracle::available() {
        let qemu = acad_oracle::generate_dwg(&disk, "ORCBLOCK", &inputs).unwrap();
        assert_eq!(original, qemu, "in-tree BLOCK DWG differs from QEMU");
    }
}

#[cfg(unix)]
#[test]
fn insert_uses_the_new_block_with_independent_scales_and_rotation() {
    use acad_model::{Entity, Item, Point};

    let disk = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../corpus/raw/Autodesk AutoCAD 1.4 (5.25)/System.img");
    if !disk.exists() {
        return;
    }
    let inputs = [
        "LINE", "2,3", "4,5", "", "BLOCK", "b1", "1,2", "LAST", "INSERT", "B1", "7,8", "2", "3",
        "30",
    ];
    let original = acad_oracle::generate_dwg_in_tree(&disk, "ORCBINS", &inputs).unwrap();
    let decoded = acad_dwg::parse(&original).unwrap();
    assert_eq!(decoded.items.len(), 3);
    assert!(matches!(decoded.items[0], Item::Erased(_)));
    assert!(matches!(decoded.items[1], Item::Block(_)));
    let Item::Entity(Entity::OnLayer { entity, layer: 1 }) = &decoded.items[2] else {
        panic!("original INSERT was {:?}", decoded.items[2]);
    };
    let Entity::Insert {
        origin,
        x_scale,
        y_scale,
        rotation_deg,
        name,
    } = entity.as_ref()
    else {
        panic!("original INSERT was {:?}", decoded.items[2]);
    };
    assert_eq!(*origin, Point { x: 7.0, y: 8.0 });
    assert_eq!((*x_scale, *y_scale, name.as_str()), (2.0, 3.0, "B1"));
    assert!((*rotation_deg - 30.0).abs() < 1e-12);
    let mut rust = acad_cmd::Editor::default();
    for input in inputs {
        rust.submit(input).unwrap();
    }
    assert_eq!(rust.drawing().items[0], decoded.items[0]);
    assert_eq!(rust.drawing().items[1], decoded.items[1]);
    let Item::Entity(Entity::OnLayer {
        entity: rust_insert,
        layer: 1,
    }) = &rust.drawing().items[2]
    else {
        panic!("Rust did not create an INSERT");
    };
    let Entity::Insert {
        origin: rust_origin,
        x_scale: rust_x_scale,
        y_scale: rust_y_scale,
        rotation_deg: rust_rotation,
        name: rust_name,
    } = rust_insert.as_ref()
    else {
        panic!("Rust did not create an INSERT");
    };
    assert_eq!(
        (
            *rust_origin,
            *rust_x_scale,
            *rust_y_scale,
            rust_name.as_str()
        ),
        (*origin, *x_scale, *y_scale, name.as_str())
    );
    assert!((*rust_rotation - *rotation_deg).abs() < 1e-12);
    if acad_oracle::available() {
        let qemu = acad_oracle::generate_dwg(&disk, "ORCBINS", &inputs).unwrap();
        assert_eq!(original, qemu, "in-tree INSERT DWG differs from QEMU");
    }
}

#[cfg(unix)]
#[test]
fn insert_blank_y_scale_reuses_x_scale_as_the_original_does() {
    use acad_model::{Entity, Item};

    let disk = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../corpus/raw/Autodesk AutoCAD 1.4 (5.25)/System.img");
    if !disk.exists() {
        return;
    }
    let inputs = [
        "LINE", "2,3", "4,5", "", "BLOCK", "B1", "1,2", "LAST", "INSERT", "B1", "7,8", "2", "", "",
    ];
    let original = acad_oracle::generate_dwg_in_tree(&disk, "ORCINSDF", &inputs).unwrap();
    let decoded = acad_dwg::parse(&original).unwrap();
    let Item::Entity(Entity::OnLayer { entity, .. }) = &decoded.items[2] else {
        panic!("original did not save an INSERT");
    };
    let Entity::Insert {
        x_scale,
        y_scale,
        rotation_deg,
        ..
    } = entity.as_ref()
    else {
        panic!("original did not save an INSERT");
    };
    assert_eq!((*x_scale, *y_scale, *rotation_deg), (2.0, 2.0, 0.0));

    let mut rust = acad_cmd::Editor::default();
    for input in inputs {
        rust.submit(input).unwrap();
    }
    assert_eq!(rust.drawing().items, decoded.items);
    let saved = acad_dwg::write(rust.drawing()).unwrap();
    assert_eq!(acad_dwg::parse(&saved).unwrap().items, decoded.items);
    if acad_oracle::available() {
        let qemu = acad_oracle::generate_dwg(&disk, "ORCINSDF", &inputs).unwrap();
        assert_eq!(original, qemu, "in-tree INSERT defaults differ from QEMU");
    }
}

#[cfg(unix)]
#[test]
fn star_insert_explodes_the_block_at_the_requested_point() {
    use acad_model::{Entity, Item, Point};

    let disk = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../corpus/raw/Autodesk AutoCAD 1.4 (5.25)/System.img");
    if !disk.exists() {
        return;
    }
    let inputs = [
        "LINE", "2,3", "4,5", "", "BLOCK", "B1", "1,2", "LAST", "INSERT", "*B1", "7,8",
    ];
    let original = acad_oracle::generate_dwg_in_tree(&disk, "ORCEXPL", &inputs).unwrap();
    let decoded = acad_dwg::parse(&original).unwrap();
    assert_eq!(decoded.items.len(), 3);
    assert!(matches!(decoded.items[0], Item::Erased(_)));
    assert!(matches!(decoded.items[1], Item::Block(_)));
    assert!(matches!(
        decoded.items[2],
        Item::Entity(Entity::OnLayer { ref entity, layer: 1 })
            if matches!(entity.as_ref(), Entity::Line {
                start: Point { x: 8.0, y: 9.0 },
                end: Point { x: 10.0, y: 11.0 },
            })
    ));
    let mut rust = acad_cmd::Editor::default();
    for input in inputs {
        rust.submit(input).unwrap();
    }
    assert_eq!(rust.drawing().items, decoded.items);
    let saved = acad_dwg::write(rust.drawing()).unwrap();
    assert_eq!(acad_dwg::parse(&saved).unwrap().items, decoded.items);
    if acad_oracle::available() {
        let qemu = acad_oracle::generate_dwg(&disk, "ORCEXPL", &inputs).unwrap();
        assert_eq!(original, qemu, "in-tree exploded INSERT differs from QEMU");
    }
}

#[cfg(unix)]
#[test]
fn insert_opposite_corner_sets_both_scales_in_the_original() {
    use acad_model::{Entity, Item, Point};

    let disk = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../corpus/raw/Autodesk AutoCAD 1.4 (5.25)/System.img");
    if !disk.exists() {
        return;
    }
    let inputs = [
        "LINE", "2,3", "4,5", "", "BLOCK", "B1", "1,2", "LAST", "INSERT", "B1", "3,3", "5,6", "0",
    ];
    let original = acad_oracle::generate_dwg_in_tree(&disk, "ORCBOX", &inputs).unwrap();
    let decoded = acad_dwg::parse(&original).unwrap();
    assert_eq!(decoded.items.len(), 3);
    assert!(matches!(decoded.items[0], Item::Erased(_)));
    assert!(matches!(
        decoded.items[2],
        Item::Entity(Entity::OnLayer { ref entity, layer: 1 })
            if matches!(entity.as_ref(), Entity::Insert {
                origin: Point { x: 3.0, y: 3.0 },
                x_scale: 2.0,
                y_scale: 3.0,
                rotation_deg: 0.0,
                ref name,
            } if name == "B1")
    ));
    let mut rust = acad_cmd::Editor::default();
    for input in inputs {
        rust.submit(input).unwrap();
    }
    assert_eq!(rust.drawing().items, decoded.items);
    let saved = acad_dwg::write(rust.drawing()).unwrap();
    assert_eq!(acad_dwg::parse(&saved).unwrap().items, decoded.items);
    if acad_oracle::available() {
        let qemu = acad_oracle::generate_dwg(&disk, "ORCBOX", &inputs).unwrap();
        assert_eq!(original, qemu, "in-tree scale box differs from QEMU");
    }
}

#[cfg(unix)]
#[test]
fn negative_insert_scales_mirror_the_original_block() {
    use acad_model::{Entity, Item, Point};

    let disk = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../corpus/raw/Autodesk AutoCAD 1.4 (5.25)/System.img");
    if !disk.exists() {
        return;
    }
    let inputs = [
        "LINE", "2,3", "4,5", "", "BLOCK", "B1", "1,2", "LAST", "INSERT", "B1", "3,3", "-2", "-3",
        "0",
    ];
    let original = acad_oracle::generate_dwg_in_tree(&disk, "ORCNEG", &inputs).unwrap();
    let decoded = acad_dwg::parse(&original).unwrap();
    assert!(matches!(decoded.items[0], Item::Erased(_)));
    assert!(matches!(
        decoded.items[2],
        Item::Entity(Entity::OnLayer { ref entity, layer: 1 })
            if matches!(entity.as_ref(), Entity::Insert {
                origin: Point { x: 3.0, y: 3.0 },
                x_scale: -2.0,
                y_scale: -3.0,
                rotation_deg: 0.0,
                ref name,
            } if name == "B1")
    ));
    let mut rust = acad_cmd::Editor::default();
    for input in inputs {
        rust.submit(input).unwrap();
    }
    assert_eq!(rust.drawing().items, decoded.items);
    if acad_oracle::available() {
        let qemu = acad_oracle::generate_dwg(&disk, "ORCNEG", &inputs).unwrap();
        assert_eq!(original, qemu, "in-tree negative scales differ from QEMU");
    }
}

#[cfg(unix)]
#[test]
fn original_solid_chains_its_last_edge_into_the_next_entity() {
    use acad_model::{Entity, Item, Point};

    let disk = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../corpus/raw/Autodesk AutoCAD 1.4 (5.25)/System.img");
    if !disk.exists() || !acad_oracle::available() {
        eprintln!("skipping oracle: extracted System.img or qemu-system-i386 absent");
        return;
    }
    let inputs = ["SOLID", "1,1", "4,1", "1,3", "4,3", "1,5", "4,5", ""];
    let (dwg, dxf) = acad_oracle::generate_pair(&disk, "ORCSOLID", &inputs).unwrap();
    let original = acad_dxf::parse(&dxf).unwrap();
    let solid = |p1, p2, p3, p4| {
        Item::Entity(Entity::OnLayer {
            layer: 1,
            entity: Box::new(Entity::Solid { p1, p2, p3, p4 }),
        })
    };
    let a = Point { x: 1.0, y: 1.0 };
    let b = Point { x: 4.0, y: 1.0 };
    let c = Point { x: 1.0, y: 3.0 };
    let d = Point { x: 4.0, y: 3.0 };
    let e = Point { x: 1.0, y: 5.0 };
    let f = Point { x: 4.0, y: 5.0 };
    let expected = vec![solid(a, b, c, d), solid(c, d, e, f)];
    assert_eq!(
        original.items, expected,
        "original SOLID geometry and chaining"
    );
    let decoded_dwg = acad_dwg::parse(&dwg).unwrap();
    assert_eq!(decoded_dwg.items, expected);
    let dxf_end = dxf.iter().position(|byte| *byte == 0x1a).unwrap() + 1;
    assert_eq!(acad_dxf::write(&decoded_dwg), dxf[..dxf_end]);

    let mut rust = acad_cmd::Editor::default();
    for input in inputs {
        rust.submit(input).unwrap();
    }
    assert_eq!(rust.drawing().items, expected, "Rust SOLID command");
}

#[cfg(unix)]
#[test]
fn original_trace_miters_a_bend_and_saves_its_width() {
    use acad_model::{Entity, Item, Point};

    let disk = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../corpus/raw/Autodesk AutoCAD 1.4 (5.25)/System.img");
    if !disk.exists() || !acad_oracle::available() {
        eprintln!("skipping oracle: extracted System.img or qemu-system-i386 absent");
        return;
    }
    let inputs = ["TRACE", "0.5", "1,1", "4,1", "4,4", ""];
    let (dwg, dxf) = acad_oracle::generate_pair(&disk, "ORCTRACE", &inputs).unwrap();
    let original = acad_dxf::parse(&dxf).unwrap();
    let trace = |p1, p2, p3, p4| {
        Item::Entity(Entity::OnLayer {
            layer: 1,
            entity: Box::new(Entity::Trace { p1, p2, p3, p4 }),
        })
    };
    let a = Point { x: 1.0, y: 1.25 };
    let b = Point { x: 1.0, y: 0.75 };
    let c = Point { x: 3.75, y: 1.25 };
    let d = Point { x: 4.25, y: 0.75 };
    let e = Point { x: 3.75, y: 4.0 };
    let f = Point { x: 4.25, y: 4.0 };
    let expected = vec![trace(a, b, c, d), trace(c, d, e, f)];
    assert_eq!(original.items, expected, "original TRACE miter");
    assert_eq!(original.header.trace_width, 0.5);
    let decoded_dwg = acad_dwg::parse(&dwg).unwrap();
    let dxf_end = dxf.iter().position(|byte| *byte == 0x1a).unwrap() + 1;
    let rewritten = acad_dxf::write(&decoded_dwg);
    if rewritten != dxf[..dxf_end] {
        let at = rewritten
            .iter()
            .zip(&dxf)
            .position(|(left, right)| left != right)
            .unwrap_or(rewritten.len().min(dxf_end));
        panic!(
            "TRACE DWG/DXF first difference at {at}: rewritten {:?}, original {:?}",
            String::from_utf8_lossy(
                &rewritten[at.saturating_sub(30)..(at + 60).min(rewritten.len())]
            ),
            String::from_utf8_lossy(&dxf[at.saturating_sub(30)..(at + 60).min(dxf_end)])
        );
    }
    assert_eq!(decoded_dwg.header.trace_width, 0.5);

    let mut rust = acad_cmd::Editor::default();
    for input in inputs {
        rust.submit(input).unwrap();
    }
    assert_eq!(rust.drawing().items, expected, "Rust TRACE command");
    assert_eq!(rust.drawing().header.trace_width, 0.5);
}

#[cfg(unix)]
#[test]
fn ac140_writer_matches_original_line_circle_point_record_bytes() {
    let disk = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../corpus/raw/Autodesk AutoCAD 1.4 (5.25)/System.img");
    if !disk.exists() || !acad_oracle::available() {
        return;
    }
    for (tag, commands) in [
        ("WLINE", vec!["LINE", "1,2", "3,4", ""]),
        ("WMIX", vec!["CIRCLE", "5,6", "2", "POINT", "7,8"]),
    ] {
        let (original, _) = acad_oracle::generate_pair(&disk, tag, &commands).unwrap();
        let parsed = acad_dwg::parse(&original).unwrap();
        let encoded = acad_dwg::write(&parsed).unwrap();
        let end = u32::from_le_bytes(original[0x24..0x28].try_into().unwrap()) as usize;
        assert_eq!(
            &encoded[0x202..end],
            &original[0x202..end],
            "{tag} entity stream"
        );
        assert_eq!(
            acad_dwg::parse(&encoded).unwrap().items,
            parsed.items,
            "{tag} readback"
        );
    }
}

#[cfg(unix)]
#[test]
fn original_opens_and_renders_a_rust_written_dwg() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../corpus/raw/Autodesk AutoCAD 1.4 (5.25)");
    let system = root.join("System.img");
    let samples = root.join("Samples.img");
    if !system.exists() || !samples.exists() || !acad_oracle::available() {
        return;
    }
    let probe = acad_oracle::generate_visual_pair(
        &system,
        None,
        "WRTTEST",
        &["LINE", "1,1", "11,1", "11,8", "1,8", "1,1", ""],
    )
    .unwrap();
    let drawing = acad_dxf::parse(&probe.dxf).unwrap();
    assert_eq!(drawing.items, acad_dwg::parse(&probe.dwg).unwrap().items);
    let encoded = acad_dwg::write(&drawing).unwrap();
    let opened = acad_oracle::open_drawing(&system, &samples, "HOUSE", &encoded).unwrap();
    assert_eq!(opened.len(), 16384);
    // Opening an existing file naturally changes command/menu text in the top
    // status strip. Its drawing viewport should match the source CGA frame.
    let mut viewport_pixels = 0usize;
    let mut mismatches = 0usize;
    let mut visible = 0usize;
    let (source, reopened) = (
        acad_oracle::cga::Frame::new(&probe.cga).unwrap(),
        acad_oracle::cga::Frame::new(&opened).unwrap(),
    );
    for y in 10..160 {
        for x in 0..560 {
            let expected = source.lit(x, y);
            let actual = reopened.lit(x, y);
            viewport_pixels += 1;
            mismatches += usize::from(expected != actual);
            visible += usize::from(actual);
        }
    }
    assert!(
        visible > 100,
        "AutoCAD opened no visible geometry: {visible} pixels"
    );
    assert!(
        mismatches <= viewport_pixels / 1000,
        "writer round-trip changes {mismatches}/{viewport_pixels} viewport pixels"
    );
}

#[cfg(unix)]
#[test]
fn ac12_writer_matches_the_original_subdiv_viewport_in_autocad() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let system = root.join("../../corpus/raw/Autodesk AutoCAD 1.4 (5.25)/System.img");
    let samples = root.join("../../corpus/raw/Autodesk AutoCAD 1.4 (5.25)/Samples.img");
    let original_path = root.join("../../corpus/Samples/SUBDIV.DWG");
    if !system.exists() || !samples.exists() || !original_path.exists() || !acad_oracle::available()
    {
        return;
    }
    let original = std::fs::read(original_path).unwrap();
    let drawing = acad_dwg::parse(&original).unwrap();
    let encoded = acad_dwg::write_version(&drawing, acad_dwg::header::Version::Ac12).unwrap();
    let native = acad_oracle::open_drawing(&system, &samples, "DISC", &original).unwrap();
    let rewritten = acad_oracle::open_drawing(&system, &samples, "DISC", &encoded).unwrap();
    assert_eq!(native.len(), 16384);
    assert_eq!(rewritten.len(), 16384);
    let (mut pixels, mut mismatches) = (0usize, 0usize);
    let (native, rewritten) = (
        acad_oracle::cga::Frame::new(&native).unwrap(),
        acad_oracle::cga::Frame::new(&rewritten).unwrap(),
    );
    for y in 10..160 {
        for x in 0..560 {
            pixels += 1;
            mismatches += usize::from(native.lit(x, y) != rewritten.lit(x, y));
        }
    }
    assert!(
        mismatches <= pixels / 1000,
        "AC1.2 writer changes {mismatches}/{pixels} AutoCAD viewport pixels"
    );
}

#[cfg(unix)]
#[test]
fn original_header_settings_survive_binary_decoding() {
    use acad_model::{Extents, Mode, Point};

    let disk = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../corpus/raw/Autodesk AutoCAD 1.4 (5.25)/System.img");
    if !disk.exists() || !acad_oracle::available() {
        eprintln!("skipping oracle: extracted System.img or qemu-system-i386 absent");
        return;
    }
    let (dwg, dxf) = acad_oracle::generate_pair(
        &disk,
        "ORCHEAD",
        &[
            "BASE", "2.25,3.5", "SNAP", "0.375", "GRID", "0.625", "ORTHO", "ON", "FILL", "OFF",
            "LIMITS", "-1,-2", "14,11", "LAYER", "12", "COLOR", "5", "3", "COLOR", "2", "",
        ],
    )
    .unwrap();
    let drawing = acad_dxf::parse(&dxf).unwrap();
    let h = &drawing.header;
    assert_eq!(h.base, Point { x: 2.25, y: 3.5 });
    assert_eq!(
        h.limits,
        Extents {
            xmin: -1.0,
            xmax: 14.0,
            ymin: -2.0,
            ymax: 11.0
        }
    );
    assert_eq!(
        h.snap,
        Mode {
            on: true,
            spacing: 0.375
        }
    );
    assert_eq!(
        h.grid,
        Mode {
            on: true,
            spacing: 0.625
        }
    );
    assert!(h.ortho);
    assert!(!h.fill);
    assert_eq!(h.current_layer, 3);
    assert_eq!(h.layers.keys().copied().collect::<Vec<_>>(), [0, 1, 3, 12]);
    assert_eq!(h.layers[&3], 2);
    assert_eq!(h.layers[&12], 5);
    assert!(drawing.items.is_empty());

    let end = dxf.iter().position(|&b| b == 0x1a).unwrap() + 1;
    assert_eq!(acad_dxf::write(&acad_dwg::parse(&dwg).unwrap()), dxf[..end]);
}

#[cfg(unix)]
#[test]
fn original_zoom_factor_updates_saved_view_height() {
    let disk = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../corpus/raw/Autodesk AutoCAD 1.4 (5.25)/System.img");
    if !disk.exists() || !acad_oracle::available() {
        eprintln!("skipping oracle: extracted System.img or qemu-system-i386 absent");
        return;
    }
    let (_, dxf) = acad_oracle::generate_pair(&disk, "ORCZOOM", &["ZOOM", "2"]).unwrap();
    let drawing = acad_dxf::parse(&dxf).unwrap();
    assert_eq!(drawing.header.view.height, 4.5);
}

#[cfg(unix)]
#[test]
fn original_zoom_previous_restores_the_prior_view() {
    let disk = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../corpus/raw/Autodesk AutoCAD 1.4 (5.25)/System.img");
    if !disk.exists() || !acad_oracle::available() {
        eprintln!("skipping oracle: extracted System.img or qemu-system-i386 absent");
        return;
    }
    let (_, dxf) =
        acad_oracle::generate_pair(&disk, "ORCZPRE", &["ZOOM", "2", "ZOOM", "P"]).unwrap();
    let drawing = acad_dxf::parse(&dxf).unwrap();
    assert_eq!(drawing.header.view.height, 9.0);
}

#[cfg(unix)]
#[test]
fn original_creates_a_line_arc_and_rotated_text() {
    use acad_model::{Entity, Item, Point};

    let disk = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../corpus/raw/Autodesk AutoCAD 1.4 (5.25)/System.img");
    if !disk.exists() || !acad_oracle::available() {
        eprintln!("skipping oracle: extracted System.img or qemu-system-i386 absent");
        return;
    }
    // ACAD.HLP documents RETURN to terminate LINE and three points for ARC.
    let (dwg, dxf) = acad_oracle::generate_pair(
        &disk,
        "ORCARC",
        &[
            "LINE", "1.25,2.5", "9.5,6.75", "", "ARC", "4,3", "3,4", "2,3", "TEXT", "2.25,3.5",
            "0.75", "30", "ORACLE",
        ],
    )
    .unwrap();
    let drawing = acad_dxf::parse(&dxf).unwrap();
    assert_eq!(
        drawing.items,
        vec![
            Item::Entity(Entity::OnLayer {
                layer: 1,
                entity: Box::new(Entity::Line {
                    start: Point { x: 1.25, y: 2.5 },
                    end: Point { x: 9.5, y: 6.75 },
                })
            }),
            // The three input points define the upper half of the unit
            // circle around (3, 3), counterclockwise from (4, 3) to (2, 3).
            Item::Entity(Entity::OnLayer {
                layer: 1,
                entity: Box::new(Entity::Arc {
                    center: Point { x: 3.0, y: 3.0 },
                    radius: 1.0,
                    start_deg: 0.0,
                    end_deg: 180.0,
                })
            }),
            Item::Entity(Entity::OnLayer {
                layer: 1,
                entity: Box::new(Entity::Text {
                    origin: Point { x: 2.25, y: 3.5 },
                    height: 0.75,
                    rotation_deg: 30.0,
                    value: "ORACLE".to_owned(),
                })
            }),
        ],
    );
    let end = dxf.iter().position(|&b| b == 0x1a).unwrap() + 1;
    assert_eq!(acad_dxf::write(&drawing), dxf[..end]);
    let from_dwg = acad_dwg::parse(&dwg).unwrap();
    assert_eq!(acad_dxf::write(&from_dwg), dxf[..end]);
}

#[cfg(unix)]
#[test]
fn rust_command_engine_matches_original_line_arc_and_text() {
    use acad_cmd::Editor;

    let disk = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../corpus/raw/Autodesk AutoCAD 1.4 (5.25)/System.img");
    if !disk.exists() || !acad_oracle::available() {
        return;
    }
    let commands = [
        "LINE", "1.25,2.5", "9.5,6.75", "", "ARC", "4,3", "3,4", "2,3", "TEXT", "2.25,3.5", "0.75",
        "30", "ORACLE",
    ];
    let (_, dxf) = acad_oracle::generate_pair(&disk, "ORCCMD", &commands).unwrap();
    let expected = acad_dxf::parse(&dxf).unwrap();
    let mut editor = Editor::default();
    for command in commands {
        editor.submit(command).unwrap();
    }
    assert_eq!(editor.drawing().items, expected.items);
}

#[cfg(unix)]
#[test]
fn rust_command_engine_matches_original_drawing_settings() {
    use acad_cmd::Editor;
    use acad_model::{Extents, Mode, Point};

    let disk = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../corpus/raw/Autodesk AutoCAD 1.4 (5.25)/System.img");
    if !disk.exists() || !acad_oracle::available() {
        return;
    }
    let commands = [
        "BASE", "2.25,3.5", "SNAP", "0.375", "GRID", "0.625", "ORTHO", "ON", "FILL", "OFF",
        "LIMITS", "-1,-2", "14,11", "LAYER", "12", "COLOR", "5", "3", "COLOR", "2", "",
    ];
    let (_, dxf) = acad_oracle::generate_pair(&disk, "ORCCFG", &commands).unwrap();
    let expected = acad_dxf::parse(&dxf).unwrap();
    let mut editor = Editor::default();
    for command in commands {
        editor.submit(command).unwrap();
    }
    let actual = editor.drawing();
    assert!(actual.items.is_empty());
    assert_eq!(actual.header.base, Point { x: 2.25, y: 3.5 });
    assert_eq!(
        actual.header.limits,
        Extents {
            xmin: -1.0,
            ymin: -2.0,
            xmax: 14.0,
            ymax: 11.0
        }
    );
    assert_eq!(
        actual.header.snap,
        Mode {
            on: true,
            spacing: 0.375
        }
    );
    assert_eq!(
        actual.header.grid,
        Mode {
            on: true,
            spacing: 0.625
        }
    );
    assert!(actual.header.ortho);
    assert!(!actual.header.fill);
    assert_eq!(actual.header.current_layer, expected.header.current_layer);
    assert_eq!(actual.header.layers, expected.header.layers);
}
