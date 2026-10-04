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
fn original_dim_exports_primitive_geometry_matched_by_rust() {
    use acad_model::{Entity, Item};

    let disk = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../corpus/raw/Autodesk AutoCAD 1.4 (5.25)/System.img");
    if !disk.exists() || !acad_oracle::available() {
        eprintln!("skipping oracle: extracted System.img or qemu-system-i386 absent");
        return;
    }

    for (name, inputs) in [
        ("DIMSHORT", ["DIM", "1,1", "5,1", "3,2", ""]),
        ("DIMLONG", ["DIM", "0,0", "0,4", "3,4", ""]),
    ] {
        let (dwg, _) = acad_oracle::generate_pair(&disk, name, &inputs).unwrap();
        let native = acad_dwg::parse(&dwg).unwrap();
        let mut rust = acad_cmd::Editor::default();
        for input in inputs {
            rust.submit(input).unwrap();
        }
        assert_eq!(rust.drawing().items.len(), native.items.len(), "{name}");
        for (index, (actual, expected)) in
            rust.drawing().items.iter().zip(&native.items).enumerate()
        {
            let (
                Item::Entity(Entity::OnLayer { entity: actual, .. }),
                Item::Entity(Entity::OnLayer {
                    entity: expected, ..
                }),
            ) = (actual, expected)
            else {
                panic!("{name} item {index}: unexpected item types {actual:?} vs {expected:?}");
            };
            let close = |a: f64, b: f64, tolerance: f64| {
                assert!(
                    (a - b).abs() <= tolerance,
                    "{name} item {index}: {a} != {b}"
                );
            };
            match (actual.as_ref(), expected.as_ref()) {
                (Entity::Line { start: a, end: b }, Entity::Line { start: c, end: d }) => {
                    close(a.x, c.x, 1e-6);
                    close(a.y, c.y, 1e-6);
                    close(b.x, d.x, 1e-6);
                    close(b.y, d.y, 1e-6);
                }
                (
                    Entity::Solid {
                        p1: a,
                        p2: b,
                        p3: c,
                        p4: d,
                    },
                    Entity::Solid {
                        p1: e,
                        p2: f,
                        p3: g,
                        p4: h,
                    },
                ) => {
                    for (actual, expected) in [(a, e), (b, f), (c, g), (d, h)] {
                        close(actual.x, expected.x, 2e-6);
                        close(actual.y, expected.y, 2e-6);
                    }
                }
                (
                    Entity::Text {
                        origin: a,
                        height: ah,
                        value: av,
                        ..
                    },
                    Entity::Text {
                        origin: b,
                        height: bh,
                        value: bv,
                        ..
                    },
                ) => {
                    close(a.x, b.x, 0.003);
                    close(a.y, b.y, 0.003);
                    close(*ah, *bh, 1e-6);
                    assert_eq!(av, bv, "{name} dimension text");
                }
                (actual, expected) => {
                    panic!("{name} item {index}: Rust entity {actual:?} != native {expected:?}")
                }
            }
        }
    }
}

#[cfg(unix)]
#[test]
fn original_wblock_star_writes_only_live_geometry_and_reachable_blocks() {
    let disk = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../corpus/raw/Autodesk AutoCAD 1.4 (5.25)/System.img");
    if !disk.exists() || !acad_oracle::available() {
        eprintln!("skipping oracle: extracted System.img or qemu-system-i386 absent");
        return;
    }

    let setup = [
        "LINE",
        "0,0",
        "1,0",
        "",
        "BLOCK",
        "USED",
        "0,0",
        "LAST",
        "LINE",
        "2,0",
        "3,0",
        "",
        "BLOCK",
        "UNUSED",
        "2,0",
        "LAST",
        "INSERT",
        "USED",
        "5,5",
        "",
        "",
        "",
        "LINE",
        "4,0",
        "5,0",
        "",
        "BLOCK",
        "ERASEDONLY",
        "4,0",
        "LAST",
        "INSERT",
        "ERASEDONLY",
        "7,7",
        "",
        "",
        "",
        "ERASE",
        "LAST",
    ];
    let mut commands = setup.to_vec();
    commands.extend(["WBLOCK", "WBOUT", "*"]);
    let (source_bytes, original_bytes) =
        acad_oracle::generate_wblock(&disk, "WBSOURCE", "WBOUT", &commands).unwrap();
    let original = acad_dwg::parse(&original_bytes).unwrap();
    assert!(original.items.iter().any(|item| matches!(
        item,
        acad_model::Item::Block(block) if block.name.eq_ignore_ascii_case("USED")
    )));
    assert!(!original.items.iter().any(|item| matches!(
        item,
        acad_model::Item::Block(block) if block.name.eq_ignore_ascii_case("UNUSED")
    )));
    assert!(!original.items.iter().any(|item| matches!(
        item,
        acad_model::Item::Block(block) if block.name.eq_ignore_ascii_case("ERASEDONLY")
    )));
    assert!(!original
        .items
        .iter()
        .any(|item| matches!(item, acad_model::Item::Erased(_))));

    let source = acad_dwg::parse(&source_bytes).unwrap();
    let mut rust = acad_cmd::Editor::new(source);
    rust.submit("WBLOCK").unwrap();
    rust.submit("WBOUT").unwrap();
    let effect = rust.submit("*").unwrap();
    let acad_cmd::Effect::SaveDrawing(path, snapshot) = effect else {
        panic!("WBLOCK should return a separate snapshot for writing");
    };
    assert_eq!(path, "WBOUT.DWG");
    assert_eq!(original.items, snapshot.items);
    assert_eq!(
        original.header, snapshot.header,
        "WBLOCK header differs from the source editor header"
    );
    let round_trip = acad_dwg::parse(&acad_dwg::write(&snapshot).unwrap()).unwrap();
    assert_eq!(round_trip, *snapshot);
}

#[cfg(unix)]
#[test]
fn original_wblock_named_block_exports_its_contents_and_insertion_base() {
    let disk = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../corpus/raw/Autodesk AutoCAD 1.4 (5.25)/System.img");
    if !disk.exists() || !acad_oracle::available() {
        eprintln!("skipping oracle: extracted System.img or qemu-system-i386 absent");
        return;
    }

    let setup = ["LINE", "2,3", "8,7", "", "BLOCK", "MARK", "1,2", "LAST"];
    let mut commands = setup.to_vec();
    commands.extend(["WBLOCK", "WBNAMED", "MARK"]);
    let (source_bytes, original_bytes) =
        acad_oracle::generate_wblock(&disk, "WBSRC2", "WBNAMED", &commands).unwrap();
    let original = acad_dwg::parse(&original_bytes).unwrap();
    let mut rust = acad_cmd::Editor::new(acad_dwg::parse(&source_bytes).unwrap());
    rust.submit("WBLOCK").unwrap();
    rust.submit("WBNAMED").unwrap();
    let effect = rust.submit("MARK").unwrap();
    let acad_cmd::Effect::SaveDrawing(path, snapshot) = effect else {
        panic!("named WBLOCK should return a separate drawing snapshot");
    };
    assert_eq!(path, "WBNAMED.DWG");
    assert_eq!(original.items, snapshot.items);
    assert_eq!(original.header.base, snapshot.header.base);
    assert_eq!(
        acad_dwg::parse(&acad_dwg::write(&snapshot).unwrap()).unwrap(),
        *snapshot
    );
}

#[cfg(unix)]
#[test]
fn original_wblock_blank_block_name_exports_the_selected_entities() {
    let disk = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../corpus/raw/Autodesk AutoCAD 1.4 (5.25)/System.img");
    if !disk.exists() || !acad_oracle::available() {
        eprintln!("skipping oracle: extracted System.img or qemu-system-i386 absent");
        return;
    }

    let setup = ["LINE", "0,0", "1,0", ""];
    let mut commands = setup.to_vec();
    commands.extend(["WBLOCK", "WBSELEC", "", "1,2", "L"]);
    let (_, original_bytes) =
        acad_oracle::generate_wblock(&disk, "WBSRC3", "WBSELEC", &commands).unwrap();
    let original = acad_dwg::parse(&original_bytes).unwrap();
    let mut rust = acad_cmd::Editor::default();
    for input in ["LINE", "0,0", "1,0", ""] {
        rust.submit(input).unwrap();
    }
    rust.submit("WBLOCK").unwrap();
    rust.submit("WBSELEC").unwrap();
    rust.submit("").unwrap();
    assert_eq!(rust.prompt(), "WBLOCK: insertion base point");
    rust.submit("1,2").unwrap();
    assert_eq!(rust.prompt(), "WBLOCK: entity numbers, ALL, or LAST");
    let effect = rust.submit("1").unwrap();
    let acad_cmd::Effect::SaveDrawing(path, snapshot) = effect else {
        panic!("selected WBLOCK should return a separate drawing snapshot");
    };
    assert_eq!(path, "WBSELEC.DWG");
    assert_eq!(original.items, snapshot.items);
    assert_eq!(original.header.base, snapshot.header.base);
    assert_eq!(
        acad_dwg::parse(&acad_dwg::write(&snapshot).unwrap()).unwrap(),
        *snapshot
    );
}

#[cfg(unix)]
#[test]
fn original_dblist_switches_to_text_and_reports_database_entities() {
    let disk = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../corpus/raw/Autodesk AutoCAD 1.4 (5.25)/System.img");
    if !disk.exists() || !acad_oracle::available() {
        eprintln!("skipping oracle: extracted System.img or qemu-system-i386 absent");
        return;
    }

    let probe = acad_oracle::generate_visual_pair(
        &disk,
        None,
        "DBLST1",
        &["LINE", "2,3", "8,3", "", "DBLIST"],
    )
    .unwrap();
    assert_eq!(
        acad_oracle::cga::detect(&probe.cga),
        Some(acad_oracle::cga::Mode::Text),
        "DBLIST should present its report in text mode"
    );
    let text: String = probe.cga[..4000]
        .chunks_exact(2)
        .map(|cell| char::from(cell[0]))
        .collect();
    assert!(
        text.contains("LINE"),
        "DBLIST text page did not name the entity: {text:?}"
    );
}

#[cfg(unix)]
#[test]
fn original_question_mark_and_help_share_the_command_help_flow() {
    let disk = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../corpus/raw/Autodesk AutoCAD 1.4 (5.25)/System.img");
    if !disk.exists() || !acad_oracle::available() {
        eprintln!("skipping oracle: extracted System.img or qemu-system-i386 absent");
        return;
    }

    let list = acad_oracle::generate_visual_pair(&disk, None, "HELPLIST", &["?", ""]).unwrap();
    assert_eq!(
        acad_oracle::cga::detect(&list.cga),
        Some(acad_oracle::cga::Mode::Text)
    );
    let text: String = list.cga[..4000]
        .chunks_exact(2)
        .map(|cell| char::from(cell[0]))
        .collect();
    assert!(text.contains("Command List"));
    assert!(text.contains("WBLOCK"));

    let line =
        acad_oracle::generate_visual_pair(&disk, None, "HELPLINE", &["HELP", "LINE"]).unwrap();
    assert_eq!(
        acad_oracle::cga::detect(&line.cga),
        Some(acad_oracle::cga::Mode::Text)
    );
    let text: String = line.cga[..4000]
        .chunks_exact(2)
        .map(|cell| char::from(cell[0]))
        .collect();
    assert!(text.contains("The  LINE  command allows you to draw straight lines."));
}

#[cfg(unix)]
#[test]
fn original_hatch_question_lists_native_patterns() {
    use acad_oracle::session::Session;
    use std::time::Duration;

    let disk = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../corpus/raw/Autodesk AutoCAD 1.4 (5.25)/System.img");
    if !disk.exists() || !acad_oracle::available() {
        eprintln!("skipping oracle: extracted System.img or qemu-system-i386 absent");
        return;
    }

    let mut vm = Session::boot_disposable(&disk, None, &[]).unwrap();
    let timeout = Duration::from_secs(30);
    vm.wait_for_text("Enter selection:", timeout).unwrap();
    vm.type_line("1").unwrap();
    vm.wait_for_text("Enter NAME of drawing:", timeout).unwrap();
    vm.type_line("HATHELP").unwrap();
    vm.wait_until_text_gone("Enter NAME of drawing:", timeout)
        .unwrap();
    vm.type_line("HATCH").unwrap();
    vm.capture_editor().unwrap();
    vm.type_line("?").unwrap();
    vm.wait_for_text("ZIGZAG", timeout).unwrap();
    let text = vm.text_screen().unwrap();
    assert!(text.contains("EARTH"));
    assert!(text.contains("LINE            - Parallel horizontal lines"));
    assert!(text.contains("ZIGZAG          - Staircase effect"));
    vm.wait_for_text("Command:", timeout).unwrap();
    vm.type_line("END").unwrap();
    vm.wait_for_text("Enter selection:", timeout).unwrap();
    vm.shutdown().unwrap();
}

#[cfg(unix)]
#[test]
fn original_hatch_line_window_matches_generated_block_geometry() {
    use acad_model::{Entity, Item};

    let disk = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../corpus/raw/Autodesk AutoCAD 1.4 (5.25)/System.img");
    if !disk.exists() || !acad_oracle::available() {
        eprintln!("skipping oracle: extracted System.img or qemu-system-i386 absent");
        return;
    }

    for (name, scale, angle) in [("HATCHWIN", "", ""), ("HATCHROT", "2", "30")] {
        let inputs = [
            "LINE", "1,1", "5,1", "5,5", "1,5", "1,1", "", "HATCH", "LINE", scale, angle, "W",
            "0,0", "6,6",
        ];
        let (dwg, _) = acad_oracle::generate_pair(&disk, name, &inputs).unwrap();
        let native = acad_dwg::parse(&dwg).unwrap();
        let mut rust = acad_cmd::Editor::default();
        for input in inputs {
            rust.submit(input).unwrap();
        }
        let rust_items = &rust.drawing().items;
        assert_eq!(rust_items.len(), native.items.len(), "{name} item count");
        assert_eq!(&rust_items[..4], &native.items[..4], "{name} boundary");
        let (Item::Block(rust_block), Item::Block(native_block)) =
            (&rust_items[4], &native.items[4])
        else {
            panic!("{name}: expected hatch block at item 5");
        };
        assert_eq!(rust_block.name, native_block.name, "{name} block name");
        assert_eq!(rust_block.base, native_block.base, "{name} block base");
        assert_eq!(
            rust_block.entities.len(),
            native_block.entities.len(),
            "{name} clipped line count"
        );
        for (index, (rust_entity, native_entity)) in rust_block
            .entities
            .iter()
            .zip(&native_block.entities)
            .enumerate()
        {
            let (
                Entity::OnLayer {
                    layer: rust_layer,
                    entity: rust_line,
                },
                Entity::OnLayer {
                    layer: native_layer,
                    entity: native_line,
                },
            ) = (rust_entity, native_entity)
            else {
                panic!("{name} hatch line {index}: unexpected entity wrapper");
            };
            assert_eq!(rust_layer, native_layer, "{name} line {index} layer");
            let (Entity::Line { start: rs, end: re }, Entity::Line { start: ns, end: ne }) =
                (rust_line.as_ref(), native_line.as_ref())
            else {
                panic!("{name} hatch line {index}: expected LINE");
            };
            for (actual, expected) in [(rs.x, ns.x), (rs.y, ns.y), (re.x, ne.x), (re.y, ne.y)] {
                assert!(
                    (actual - expected).abs() <= 1e-12,
                    "{name} hatch line {index}: {actual} != {expected}"
                );
            }
        }
        assert_eq!(rust_items[5], native.items[5], "{name} pattern INSERT");
    }
}

#[cfg(unix)]
#[test]
fn original_hatch_line_window_clips_to_a_circle() {
    use acad_model::{Entity, Item};

    let disk = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../corpus/raw/Autodesk AutoCAD 1.4 (5.25)/System.img");
    if !disk.exists() || !acad_oracle::available() {
        eprintln!("skipping oracle: extracted System.img or qemu-system-i386 absent");
        return;
    }

    let inputs = [
        "CIRCLE", "3,3", "2", "HATCH", "LINE", "", "", "W", "0,0", "6,6",
    ];
    let (dwg, _) = acad_oracle::generate_pair(&disk, "HATCHCIR", &inputs).unwrap();
    let native = acad_dwg::parse(&dwg).unwrap();
    let mut rust = acad_cmd::Editor::default();
    for input in inputs {
        rust.submit(input).unwrap();
    }

    assert_eq!(rust.drawing().items.len(), native.items.len());
    assert_eq!(&rust.drawing().items[..1], &native.items[..1]);
    let (Item::Block(rust_block), Item::Block(native_block)) =
        (&rust.drawing().items[1], &native.items[1])
    else {
        panic!("expected circle hatch block");
    };
    assert_eq!(rust_block.name, native_block.name);
    assert_eq!(rust_block.entities.len(), native_block.entities.len());
    for (index, (rust_entity, native_entity)) in rust_block
        .entities
        .iter()
        .zip(&native_block.entities)
        .enumerate()
    {
        let (
            Entity::OnLayer {
                layer: rust_layer,
                entity: rust_line,
            },
            Entity::OnLayer {
                layer: native_layer,
                entity: native_line,
            },
        ) = (rust_entity, native_entity)
        else {
            panic!("circle hatch line {index}: unexpected entity wrapper");
        };
        assert_eq!(rust_layer, native_layer);
        let (Entity::Line { start: rs, end: re }, Entity::Line { start: ns, end: ne }) =
            (rust_line.as_ref(), native_line.as_ref())
        else {
            panic!("circle hatch line {index}: expected LINE");
        };
        for (actual, expected) in [(rs.x, ns.x), (rs.y, ns.y), (re.x, ne.x), (re.y, ne.y)] {
            assert!(
                (actual - expected).abs() <= 1e-12,
                "circle hatch line {index}: {actual} != {expected}"
            );
        }
    }
    assert_eq!(
        rust.drawing().items[2],
        native.items[2],
        "circle pattern INSERT"
    );
}

#[cfg(unix)]
#[test]
fn original_hatch_line_window_clips_a_semicircle_and_chord() {
    use acad_model::{Entity, Item};

    let disk = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../corpus/raw/Autodesk AutoCAD 1.4 (5.25)/System.img");
    if !disk.exists() || !acad_oracle::available() {
        eprintln!("skipping oracle: extracted System.img or qemu-system-i386 absent");
        return;
    }

    let inputs = [
        "ARC", "5,3", "3,5", "1,3", "LINE", "1,3", "5,3", "", "HATCH", "LINE", "", "", "W", "0,0",
        "6,6",
    ];
    let (dwg, _) = acad_oracle::generate_pair(&disk, "HATCHARC", &inputs).unwrap();
    let native = acad_dwg::parse(&dwg).unwrap();
    let mut rust = acad_cmd::Editor::default();
    for input in inputs {
        rust.submit(input).unwrap();
    }

    fn hatch_lines(items: &[Item]) -> Vec<(f64, f64, f64, f64)> {
        let mut lines: Vec<_> = items
            .iter()
            .find_map(|item| match item {
                Item::Block(block) if block.name.starts_with("*X") => Some(block),
                _ => None,
            })
            .expect("hatch block")
            .entities
            .iter()
            .map(|entity| {
                let Entity::OnLayer { entity, .. } = entity else {
                    panic!("expected layer wrapper")
                };
                let Entity::Line { start, end } = entity.as_ref() else {
                    panic!("expected hatch LINE")
                };
                (start.x, start.y, end.x, end.y)
            })
            .collect();
        lines.sort_by(|a, b| a.1.total_cmp(&b.1).then(a.0.total_cmp(&b.0)));
        lines
    }
    let actual = hatch_lines(&rust.drawing().items);
    let expected = hatch_lines(&native.items);
    assert_eq!(actual.len(), 16, "native semicircle probe has 16 lines");
    assert_eq!(actual.len(), expected.len());
    for (index, (actual, expected)) in actual.iter().zip(&expected).enumerate() {
        for (actual, expected) in [
            (actual.0, expected.0),
            (actual.1, expected.1),
            (actual.2, expected.2),
            (actual.3, expected.3),
        ] {
            assert!(
                (actual - expected).abs() <= 1e-12,
                "arc hatch line {index}: {actual} != {expected}"
            );
        }
    }
}

#[cfg(unix)]
#[test]
fn original_hatch_line_window_keeps_inner_loop_unfilled() {
    use acad_model::{Entity, Item};

    let disk = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../corpus/raw/Autodesk AutoCAD 1.4 (5.25)/System.img");
    if !disk.exists() || !acad_oracle::available() {
        eprintln!("skipping oracle: extracted System.img or qemu-system-i386 absent");
        return;
    }

    let inputs = [
        "LINE", "1,1", "5,1", "5,5", "1,5", "1,1", "", "LINE", "2,2", "4,2", "4,4", "2,4", "2,2",
        "", "HATCH", "LINE", "", "", "W", "0,0", "6,6",
    ];
    let (dwg, _) = acad_oracle::generate_pair(&disk, "HATCHHO", &inputs).unwrap();
    let native = acad_dwg::parse(&dwg).unwrap();
    let mut rust = acad_cmd::Editor::default();
    for input in inputs {
        rust.submit(input).unwrap();
    }

    assert_eq!(&rust.drawing().items[..8], &native.items[..8]);
    fn hatch_block(items: &[Item]) -> &acad_model::Block {
        items
            .iter()
            .find_map(|item| match item {
                Item::Block(block) if block.name.starts_with("*X") => Some(block),
                _ => None,
            })
            .expect("hatch block")
    }
    let actual = hatch_block(&rust.drawing().items);
    let expected = hatch_block(&native.items);
    let line_coordinates = |block: &acad_model::Block| {
        let mut lines: Vec<_> = block
            .entities
            .iter()
            .filter_map(|entity| {
                let Entity::OnLayer { entity, .. } = entity else {
                    return None;
                };
                let Entity::Line { start, end } = entity.as_ref() else {
                    return None;
                };
                Some((start.x, start.y, end.x, end.y))
            })
            .collect();
        lines.sort_by(|a, b| {
            a.0.total_cmp(&b.0)
                .then(a.1.total_cmp(&b.1))
                .then(a.2.total_cmp(&b.2))
                .then(a.3.total_cmp(&b.3))
        });
        lines
    };
    let actual = line_coordinates(actual);
    let expected = line_coordinates(expected);
    assert_eq!(actual.len(), expected.len());
    for (index, (actual, expected)) in actual.iter().zip(&expected).enumerate() {
        for (actual, expected) in [
            (actual.0, expected.0),
            (actual.1, expected.1),
            (actual.2, expected.2),
            (actual.3, expected.3),
        ] {
            assert!(
                (actual - expected).abs() <= 1e-12,
                "inner-loop hatch line {index}: {actual} != {expected}"
            );
        }
    }
}

#[cfg(unix)]
#[test]
fn original_files_enters_the_file_utility_menu() {
    use acad_oracle::session::Session;
    use std::time::Duration;

    let disk = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../corpus/raw/Autodesk AutoCAD 1.4 (5.25)/System.img");
    if !disk.exists() || !acad_oracle::available() {
        eprintln!("skipping oracle: extracted System.img or qemu-system-i386 absent");
        return;
    }

    let mut vm = Session::boot_disposable(&disk, None, &[]).unwrap();
    let timeout = Duration::from_secs(30);
    vm.wait_for_text("Enter selection:", timeout).unwrap();
    vm.type_line("1").unwrap();
    vm.wait_for_text("Enter NAME of drawing:", timeout).unwrap();
    vm.type_line("FILEMENU").unwrap();
    vm.wait_until_text_gone("Enter NAME of drawing:", timeout)
        .unwrap();
    vm.type_line("FILES").unwrap();
    vm.wait_for_text("File Utility Menu", timeout).unwrap();
    vm.wait_for_text("List Drawing files", timeout).unwrap();
    vm.wait_for_text("Rename files", timeout).unwrap();
    let text = vm.text_screen().unwrap();
    assert!(text.contains("List Drawing files"));
    assert!(text.contains("Rename files"));
    vm.shutdown().unwrap();
}

#[cfg(unix)]
#[test]
fn original_files_rename_changes_only_the_disposable_samples_disk() {
    use acad_oracle::session::Session;
    use std::time::Duration;

    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../corpus/raw/Autodesk AutoCAD 1.4 (5.25)");
    let system = root.join("System.img");
    let samples = root.join("Samples.img");
    if !system.exists() || !samples.exists() || !acad_oracle::available() {
        eprintln!("skipping oracle: extracted floppies or qemu-system-i386 absent");
        return;
    }

    let mut vm = Session::boot_disposable(&system, Some(&samples), &[]).unwrap();
    let timeout = Duration::from_secs(30);
    let original = vm.read_samples_file("SUBDIV.DWG").unwrap();
    vm.wait_for_text("Enter selection:", timeout).unwrap();
    vm.type_line("1").unwrap();
    vm.wait_for_text("Enter NAME of drawing:", timeout).unwrap();
    vm.type_line("FILEOPS").unwrap();
    vm.wait_until_text_gone("Enter NAME of drawing:", timeout)
        .unwrap();
    vm.type_line("FILES").unwrap();
    vm.wait_for_text("File Utility Menu", timeout).unwrap();

    vm.type_line("7").unwrap();
    vm.wait_for_text("Enter current filename:", timeout)
        .unwrap();
    vm.type_line("B:SUBDIV.DWG").unwrap();
    vm.wait_for_text("Enter new filename:", timeout).unwrap();
    vm.type_line("B:RENAMED.DWG").unwrap();
    let start = std::time::Instant::now();
    while start.elapsed() < timeout
        && vm.read_samples_file("SUBDIV.DWG").is_ok()
        && vm.read_samples_file("RENAMED.DWG").is_err()
    {
        std::thread::sleep(Duration::from_millis(100));
    }
    assert!(vm.read_samples_file("SUBDIV.DWG").is_err());
    assert_eq!(vm.read_samples_file("RENAMED.DWG").unwrap(), original);
    vm.shutdown().unwrap();
}

#[cfg(unix)]
#[test]
fn original_files_delete_changes_only_the_disposable_samples_disk() {
    use acad_oracle::session::Session;
    use std::time::Duration;

    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../corpus/raw/Autodesk AutoCAD 1.4 (5.25)");
    let system = root.join("System.img");
    let samples = root.join("Samples.img");
    if !system.exists() || !samples.exists() || !acad_oracle::available() {
        eprintln!("skipping oracle: extracted floppies or qemu-system-i386 absent");
        return;
    }

    let mut vm = Session::boot_disposable(&system, Some(&samples), &[]).unwrap();
    let timeout = Duration::from_secs(30);
    let original = vm.read_samples_file("SUBDIV.DWG").unwrap();
    vm.wait_for_text("Enter selection:", timeout).unwrap();
    vm.type_line("1").unwrap();
    vm.wait_for_text("Enter NAME of drawing:", timeout).unwrap();
    vm.type_line("FILEDEL").unwrap();
    vm.wait_until_text_gone("Enter NAME of drawing:", timeout)
        .unwrap();
    vm.type_line("FILES").unwrap();
    vm.wait_for_text("File Utility Menu", timeout).unwrap();
    vm.type_line("6").unwrap();
    vm.wait_for_text("Enter file deletion specification:", timeout)
        .unwrap();
    vm.type_line("B:SUBDIV.DWG").unwrap();
    let start = std::time::Instant::now();
    while start.elapsed() < timeout && vm.read_samples_file("SUBDIV.DWG").is_ok() {
        std::thread::sleep(Duration::from_millis(100));
    }
    assert!(vm.read_samples_file("SUBDIV.DWG").is_err());
    assert!(!original.is_empty());
    vm.shutdown().unwrap();
}

#[cfg(unix)]
#[test]
fn redraw_and_regen_are_display_only_commands_in_the_original() {
    let disk = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../corpus/raw/Autodesk AutoCAD 1.4 (5.25)/System.img");
    if !disk.exists() || !acad_oracle::available() {
        eprintln!("skipping oracle: extracted System.img or qemu-system-i386 absent");
        return;
    }

    let commands = ["LINE", "2,3", "8,3", "", "STATUS", "REDRAW", "REGEN"];
    let (_, dxf) = acad_oracle::generate_pair(&disk, "REDRAW", &commands).unwrap();
    let original = acad_dxf::parse(&dxf).unwrap();
    let mut editor = acad_cmd::Editor::new(acad_model::Drawing {
        header: original.header.clone(),
        items: Vec::new(),
    });
    for command in commands {
        editor.submit(command).unwrap();
    }
    assert_eq!(editor.drawing().items, original.items);
    assert_eq!(
        original.entities().count(),
        1,
        "display refresh commands must not add or remove entities"
    );
}

#[cfg(unix)]
#[test]
fn original_menu_load_and_cancel_return_to_command_input() {
    use acad_model::Entity;

    let disk = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../corpus/raw/Autodesk AutoCAD 1.4 (5.25)/System.img");
    if !disk.exists() || !acad_oracle::available() {
        eprintln!("skipping oracle: extracted System.img or qemu-system-i386 absent");
        return;
    }

    for (name, menu_input) in [("MNLOAD", "ACAD.MNU"), ("MNCAN", "")] {
        let inputs = ["MENU", menu_input, "CIRCLE", "3,3", "2"];
        let (dwg, _) = acad_oracle::generate_pair(&disk, name, &inputs).unwrap();
        let drawing = acad_dwg::parse(&dwg).unwrap();
        let first = drawing.entities().next().map(|entity| match entity {
            Entity::OnLayer { entity, .. } => entity.as_ref(),
            entity => entity,
        });
        assert!(
            matches!(
                first,
                Some(Entity::Circle { center, radius })
                    if center.x == 3.0 && center.y == 3.0 && *radius == 2.0
            ),
            "MENU input {menu_input:?} should leave command input active; got {first:?}"
        );
        assert_eq!(drawing.entities().count(), 1);
    }
}

#[cfg(unix)]
#[test]
fn original_resolution_aliases_update_the_shared_snap_header_state() {
    use acad_model::Mode;
    let disk = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../corpus/raw/Autodesk AutoCAD 1.4 (5.25)/System.img");
    if !disk.exists() || !acad_oracle::available() {
        eprintln!("skipping oracle: extracted System.img or qemu-system-i386 absent");
        return;
    }

    let original = acad_dwg::parse(
        &acad_oracle::generate_dwg(&disk, "RESOLVE", &["RES", "2.5", "RESOLUTION", "OFF"]).unwrap(),
    )
    .unwrap();
    assert_eq!(
        original.header.snap,
        Mode {
            on: false,
            spacing: 2.5
        }
    );

    let mut rust = acad_cmd::Editor::default();
    for input in ["RES", "2.5", "RESOLUTION", "OFF"] {
        rust.submit(input).unwrap();
    }
    assert_eq!(rust.drawing().header.snap, original.header.snap);
}

#[cfg(unix)]
#[test]
fn original_units_decimal_precision_is_persisted_in_the_ac140_header() {
    use acad_model::{UnitFormat, Units};
    let disk = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../corpus/raw/Autodesk AutoCAD 1.4 (5.25)/System.img");
    if !disk.exists() || !acad_oracle::available() {
        eprintln!("skipping oracle: extracted System.img or qemu-system-i386 absent");
        return;
    }

    let original = acad_dwg::parse(
        &acad_oracle::generate_dwg(&disk, "UNITDEC", &["UNITS", "2", "3"]).unwrap(),
    )
    .unwrap();
    assert_eq!(
        original.header.units,
        Units {
            format: UnitFormat::Decimal,
            precision: 3
        }
    );

    let mut rust = acad_cmd::Editor::default();
    for input in ["UNITS", "2", "3"] {
        rust.submit(input).unwrap();
    }
    assert_eq!(rust.drawing().header.units, original.header.units);
}

#[cfg(unix)]
#[test]
fn original_axis_persists_header_settings_and_changes_the_cga_display() {
    let disk = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../corpus/raw/Autodesk AutoCAD 1.4 (5.25)/System.img");
    if !disk.exists() || !acad_oracle::available() {
        eprintln!("skipping oracle: extracted System.img or qemu-system-i386 absent");
        return;
    }

    let off =
        acad_oracle::generate_visual_pair(disk.as_path(), None, "AXIS", &["AXIS", "OFF"]).unwrap();
    let on =
        acad_oracle::generate_visual_pair(disk.as_path(), None, "AXIS", &["AXIS", "ON"]).unwrap();
    let dwg_differences = off
        .dwg
        .iter()
        .zip(&on.dwg)
        .enumerate()
        .filter_map(|(at, (a, b))| (a != b).then_some(at))
        .collect::<Vec<_>>();
    assert_eq!(
        dwg_differences,
        [0x1e0],
        "AXIS ON changes only its DWG flag"
    );
    let off_drawing = acad_dwg::parse(&off.dwg).unwrap();
    let on_drawing = acad_dwg::parse(&on.dwg).unwrap();
    assert!(!off_drawing.header.axis.on);
    assert_eq!(
        on_drawing.header.axis,
        acad_model::Mode {
            on: true,
            spacing: 0.0
        }
    );
    assert_eq!(acad_dwg::write(&on_drawing).unwrap(), on.dwg);
    assert_eq!(
        off.dxf, on.dxf,
        "AXIS display mode does not change drawing data"
    );
    assert_ne!(
        off.cga, on.cga,
        "AXIS ON and OFF must produce distinct display frames"
    );

    let spacing = acad_oracle::generate_dwg(disk.as_path(), "AXIS", &["AXIS", "5"]).unwrap();
    let snap_multiple = acad_oracle::generate_dwg(disk.as_path(), "AXIS", &["AXIS", "5X"]).unwrap();
    assert_eq!(
        spacing, snap_multiple,
        "default SNAP spacing is one drawing unit"
    );
    assert_eq!(spacing[0x1e0], 1, "numeric spacing enables AXIS");
    assert_eq!(
        f64::from_le_bytes(spacing[0x1e2..0x1ea].try_into().unwrap()),
        5.0,
        "numeric and SNAP-relative spacing are stored at 0x1e2"
    );
    let spaced_drawing = acad_dwg::parse(&spacing).unwrap();
    assert_eq!(
        spaced_drawing.header.axis,
        acad_model::Mode {
            on: true,
            spacing: 5.0
        }
    );
    assert_eq!(acad_dwg::write(&spaced_drawing).unwrap(), spacing);
}

#[cfg(unix)]
#[test]
fn original_line_accepts_relative_and_polar_points() {
    use acad_model::{Entity, Item, Point};
    let disk = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../corpus/raw/Autodesk AutoCAD 1.4 (5.25)/System.img");
    if !disk.exists() {
        return;
    }
    let inputs = ["LINE", "1,1", "@2,0", "@4<90", ""];
    let dwg = acad_oracle::generate_dwg_in_tree(&disk, "ORCRELPT", &inputs).unwrap();
    let line = |start, end| {
        Item::Entity(Entity::OnLayer {
            layer: 1,
            entity: Box::new(Entity::Line { start, end }),
        })
    };
    let expected = [
        line(Point { x: 1.0, y: 1.0 }, Point { x: 3.0, y: 1.0 }),
        line(Point { x: 3.0, y: 1.0 }, Point { x: 3.0, y: 5.0 }),
    ];
    assert_eq!(acad_dwg::parse(&dwg).unwrap().items, expected);
    let mut editor = acad_cmd::Editor::default();
    for input in inputs {
        editor.submit(input).unwrap();
    }
    assert_eq!(editor.drawing().items, expected);
    if acad_oracle::available() {
        let qemu = acad_oracle::generate_dwg(&disk, "ORCRELPT", &inputs).unwrap();
        assert_eq!(qemu, dwg);
    }
}

#[cfg(unix)]
#[test]
fn original_array_point_spacings_use_the_delta_between_points() {
    use acad_model::{Entity, Item, Point};
    let disk = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../corpus/raw/Autodesk AutoCAD 1.4 (5.25)/System.img");
    if !disk.exists() {
        return;
    }
    // Unequal point deltas make swapping the row and column axes observable.
    let inputs = [
        "LINE", "1,1", "2,1", "", "ARRAY", "L", "R", "2", "2", "3,4", "8,6",
    ];
    let dwg = acad_oracle::generate_dwg_in_tree(&disk, "ORCAPPT", &inputs).unwrap();
    let expected: Vec<_> = [(1.0, 1.0), (1.0, 3.0), (6.0, 1.0), (6.0, 3.0)]
        .into_iter()
        .map(|(x, y)| {
            Item::Entity(Entity::OnLayer {
                layer: 1,
                entity: Box::new(Entity::Line {
                    start: Point { x, y },
                    end: Point { x: x + 1.0, y },
                }),
            })
        })
        .collect();
    assert_eq!(acad_dwg::parse(&dwg).unwrap().items, expected);
    let mut editor = acad_cmd::Editor::default();
    for input in inputs {
        editor.submit(input).unwrap();
    }
    assert_eq!(editor.drawing().items, expected);
    if acad_oracle::available() {
        let qemu = acad_oracle::generate_dwg(&disk, "ORCAPPT", &inputs).unwrap();
        assert_eq!(qemu, dwg);
    }
}

#[cfg(unix)]
#[test]
fn original_change_supports_layer_and_intersection_point_modes() {
    use acad_model::{Entity, Item, Point};
    let disk = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../corpus/raw/Autodesk AutoCAD 1.4 (5.25)/System.img");
    if !disk.exists() {
        return;
    }
    for (name, inputs, expected) in [
        (
            "CHGLYR",
            &["LINE", "1,1", "2,1", "", "CHANGE", "L", "L", "2"][..],
            Item::Entity(Entity::OnLayer {
                layer: 2,
                entity: Box::new(Entity::Line {
                    start: Point { x: 1.0, y: 1.0 },
                    end: Point { x: 2.0, y: 1.0 },
                }),
            }),
        ),
        (
            "CHGLINE",
            &["LINE", "1,1", "2,1", "", "CHANGE", "L", "3,4"][..],
            Item::Entity(Entity::OnLayer {
                layer: 1,
                entity: Box::new(Entity::Line {
                    start: Point { x: 1.0, y: 1.0 },
                    end: Point { x: 3.0, y: 4.0 },
                }),
            }),
        ),
        (
            "CHGCIRC",
            &["CIRCLE", "1,1", "2", "CHANGE", "L", "4,1"][..],
            Item::Entity(Entity::OnLayer {
                layer: 1,
                entity: Box::new(Entity::Circle {
                    center: Point { x: 1.0, y: 1.0 },
                    radius: 3.0,
                }),
            }),
        ),
    ] {
        let original = acad_oracle::generate_dwg_in_tree(&disk, name, inputs).unwrap();
        let expected = [expected];
        assert_eq!(
            acad_dwg::parse(&original).unwrap().items,
            expected,
            "{name} oracle"
        );
        let mut editor = acad_cmd::Editor::default();
        for input in inputs {
            editor.submit(input).unwrap();
        }
        assert_eq!(editor.drawing().items, expected, "{name} Rust");
        if acad_oracle::available() {
            let qemu = acad_oracle::generate_dwg(&disk, name, inputs).unwrap();
            assert_eq!(qemu, original, "{name}: in-tree original vs QEMU");
            assert_eq!(
                acad_dwg::parse(&qemu).unwrap().items,
                expected,
                "{name} QEMU"
            );
        }
    }
}

#[cfg(unix)]
#[test]
fn original_change_moves_inserts_and_accepts_a_new_angle() {
    use acad_model::{Entity, Item, Point};
    let disk = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../corpus/raw/Autodesk AutoCAD 1.4 (5.25)/System.img");
    if !disk.exists() {
        return;
    }
    for (name, angle_text, expected_angle) in [("CHGINS", "", 0.0), ("CHGANG", "45", 45.0)] {
        let mut inputs = vec![
            "LINE", "0,0", "1,0", "", "BLOCK", "B1", "0,0", "LAST", "INSERT", "B1", "1,1", "2", "",
            "", "CHANGE", "L", "3,4",
        ];
        inputs.push(angle_text);
        let original = acad_oracle::generate_dwg_in_tree(&disk, name, &inputs).unwrap();
        let parsed = acad_dwg::parse(&original).unwrap();
        let insert = |items: &[Item]| {
            let Item::Entity(Entity::OnLayer { layer: 1, entity }) = items.last().unwrap() else {
                panic!("expected final INSERT entity");
            };
            let Entity::Insert {
                origin,
                x_scale,
                y_scale,
                rotation_deg,
                name,
            } = entity.as_ref()
            else {
                panic!("expected final INSERT entity");
            };
            assert_eq!(
                (*origin, *x_scale, *y_scale, name.as_str()),
                (Point { x: 3.0, y: 4.0 }, 2.0, 2.0, "B1")
            );
            assert!((*rotation_deg - expected_angle).abs() < 1e-10);
        };
        insert(&parsed.items);

        let mut editor = acad_cmd::Editor::default();
        for input in &inputs {
            editor.submit(input).unwrap();
        }
        assert_eq!(&editor.drawing().items[..2], &parsed.items[..2]);
        insert(&editor.drawing().items);
        if acad_oracle::available() {
            let qemu = acad_oracle::generate_dwg(&disk, name, &inputs).unwrap();
            assert_eq!(qemu, original, "{name}: in-tree original vs QEMU");
            insert(&acad_dwg::parse(&qemu).unwrap().items);
        }
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

    let varied = [
        "REPEAT", "LINE", "1,1", "2,1", "", "ENDREP", "3", "4", "3", "4",
    ];
    let varied_dwg = acad_oracle::generate_dwg_in_tree(&disk, "ORCREP3", &varied).unwrap();
    assert_eq!(&varied_dwg[0x208..0x20a], &[1, 0]);
    let varied_parsed = acad_dwg::parse(&varied_dwg).unwrap();
    assert_eq!(acad_dwg::write(&varied_parsed).unwrap(), varied_dwg);
}

#[cfg(unix)]
#[test]
fn original_repeat_point_distances_use_consecutive_point_deltas() {
    use acad_model::{Entity, Item, Point};
    let disk = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../corpus/raw/Autodesk AutoCAD 1.4 (5.25)/System.img");
    if !disk.exists() {
        return;
    }
    let inputs = [
        "REPEAT", "LINE", "1,1", "2,1", "", "ENDREP", "2", "2", "3,4", "8,6",
    ];
    let original = acad_oracle::generate_dwg_in_tree(&disk, "ORCRPPT", &inputs).unwrap();
    let expected = Item::Repeat(acad_model::Repeat {
        start_layer: 1,
        end_layer: 1,
        entities: vec![Entity::OnLayer {
            layer: 1,
            entity: Box::new(Entity::Line {
                start: Point { x: 1.0, y: 1.0 },
                end: Point { x: 2.0, y: 1.0 },
            }),
        }],
        columns: 2,
        rows: 2,
        column_spacing: 5.0,
        row_spacing: 2.0,
    });
    let parsed = acad_dwg::parse(&original).unwrap();
    assert_eq!(parsed.items, [expected.clone()]);
    assert_eq!(acad_dwg::write(&parsed).unwrap(), original);
    let mut editor = acad_cmd::Editor::default();
    for input in inputs {
        editor.submit(input).unwrap();
    }
    assert_eq!(editor.drawing().items, [expected]);
    if acad_oracle::available() {
        let qemu = acad_oracle::generate_dwg(&disk, "ORCRPPT", &inputs).unwrap();
        assert_eq!(qemu, original);
    }
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
fn original_array_creates_a_rectangular_grid() {
    use acad_model::{Entity, Item, Point};
    let disk = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../corpus/raw/Autodesk AutoCAD 1.4 (5.25)/System.img");
    if !disk.exists() {
        return;
    }
    // The original asks for R/C after selection, then rows, columns, row
    // spacing, and column spacing. It writes the copies in column-major order.
    let inputs = [
        "LINE", "0,0", "1,0", "", "ARRAY", "L", "R", "2", "3", "-5", "10",
    ];
    let dwg = acad_oracle::generate_dwg_in_tree(&disk, "ORCARRAY", &inputs).unwrap();
    let parsed = acad_dwg::parse(&dwg).unwrap();
    let expected: Vec<_> = [
        (0.0, 0.0),
        (0.0, -5.0),
        (10.0, 0.0),
        (10.0, -5.0),
        (20.0, 0.0),
        (20.0, -5.0),
    ]
    .into_iter()
    .map(|(x, y)| {
        Item::Entity(Entity::OnLayer {
            layer: 1,
            entity: Box::new(Entity::Line {
                start: Point { x, y },
                end: Point { x: x + 1.0, y },
            }),
        })
    })
    .collect();
    assert_eq!(
        parsed.items, expected,
        "original ARRAY geometry and record order"
    );
    let mut editor = acad_cmd::Editor::default();
    for input in inputs {
        editor.submit(input).unwrap();
    }
    assert_eq!(
        editor.drawing().items,
        expected,
        "Rust ARRAY geometry and record order"
    );
    if acad_oracle::available() {
        let qemu = acad_oracle::generate_dwg(&disk, "ORCARRAY", &inputs).unwrap();
        assert_eq!(acad_dwg::parse(&qemu).unwrap().items, expected);
        assert_eq!(dwg, qemu);
    }
}

#[cfg(unix)]
#[test]
fn original_circular_array_rotates_copies_around_its_center() {
    use acad_model::{Entity, Item, Point};
    let disk = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../corpus/raw/Autodesk AutoCAD 1.4 (5.25)/System.img");
    if !disk.exists() {
        return;
    }
    // Non-origin center and unequal input coordinates discriminate rotation
    // about the requested center from rotation about the world origin.
    let inputs = [
        "LINE", "5,3", "6,3", "", "ARRAY", "L", "C", "4,3", "90", "4",
    ];
    let dwg = acad_oracle::generate_dwg_in_tree(&disk, "ORCARRC", &inputs).unwrap();
    let expected: Vec<_> = [
        ((5.0, 3.0), (6.0, 3.0)),
        ((4.0, 4.0), (5.0, 4.0)),
        ((3.0, 3.0), (4.0, 3.0)),
        ((4.0, 2.0), (5.0, 2.0)),
    ]
    .into_iter()
    .map(|((x1, y1), (x2, y2))| {
        Item::Entity(Entity::OnLayer {
            layer: 1,
            entity: Box::new(Entity::Line {
                start: Point { x: x1, y: y1 },
                end: Point { x: x2, y: y2 },
            }),
        })
    })
    .collect();
    assert_eq!(acad_dwg::parse(&dwg).unwrap().items, expected);
    let mut editor = acad_cmd::Editor::default();
    for input in inputs {
        editor.submit(input).unwrap();
    }
    assert_eq!(editor.drawing().items, expected);
    editor.submit("UNDO").unwrap();
    assert_eq!(editor.drawing().items, [expected[0].clone()]);
    if acad_oracle::available() {
        let qemu = acad_oracle::generate_dwg(&disk, "ORCARRC", &inputs).unwrap();
        assert_eq!(qemu, dwg);
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
fn original_zoom_all_fits_limits_matched_by_rust() {
    // `ZOOM All`'s fit-to-box behavior (Task 5 of the screen-menu-rendering
    // plan: `crates/acad-cmd/src/dispatch.rs`'s `"A" | "ALL"` arm,
    // `zoom_all_bounds`/`fit_box_to_device`) was previously validated only
    // by scratch probes that were never committed. This keyboard-only test
    // (no mouse simulation needed) is the committed evidence: it exercises
    // a wide box (width > height, width-constrained) and a box with a
    // non-zero `ymin` (so an anchor-at-origin bug, or a bug that only
    // shows up for boxes starting at (0,0), would be caught), each typed
    // directly against the native original and compared to the Rust
    // `Editor`'s own result for the identical input.
    let disk = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../corpus/raw/Autodesk AutoCAD 1.4 (5.25)/System.img");
    if !disk.exists() || !acad_oracle::available() {
        eprintln!("skipping oracle: extracted System.img or qemu-system-i386 absent");
        return;
    }

    for (name, min, max) in [("ORCZALL1", "0,0", "20,10"), ("ORCZALL2", "5,7", "29,25")] {
        let inputs = ["LIMITS", min, max, "ZOOM", "A"];
        let (_, dxf) = acad_oracle::generate_pair(&disk, name, &inputs).unwrap();
        let native = acad_dxf::parse(&dxf).unwrap();

        let mut rust = acad_cmd::Editor::default();
        for input in inputs {
            rust.submit(input).unwrap();
        }

        let native_view = native.header.view;
        let rust_view = rust.drawing().header.view;
        let close = |actual: f64, expected: f64, what: &str| {
            assert!(
                (actual - expected).abs() <= 1e-6,
                "{name} {what}: {actual} != {expected}"
            );
        };
        close(native_view.center.x, rust_view.center.x, "center.x");
        close(native_view.center.y, rust_view.center.y, "center.y");
        close(native_view.height, rust_view.height, "height");
    }
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
