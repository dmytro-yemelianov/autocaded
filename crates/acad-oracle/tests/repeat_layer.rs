//! In-tree original evidence for layers on REPEAT groups
//! (docs/native-group-persistence.md, "E2 layers on REPEAT groups"). Every
//! original behaviour that section cites is asserted here against ACAD.EXE
//! runs on the retained System floppy (skipped visibly without `System.img`,
//! failing under `AUTOCAD_REQUIRE_CORPUS`), together with the native editor
//! and codecs on the same keys. The System image is only read.
#![cfg(unix)]
use acad_cmd::Editor;
use acad_model::{Entity, Item, Point, Repeat};
use acad_oracle::in_tree::{generate_dwg_in_tree, generate_visual_dwg_in_tree, observe_in_tree};

fn disk() -> Option<std::path::PathBuf> {
    let disk = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../corpus/raw/Autodesk AutoCAD 1.4 (5.25)/System.img");
    if disk.exists() {
        Some(disk)
    } else {
        assert!(
            std::env::var_os("AUTOCAD_REQUIRE_CORPUS").is_none(),
            "{} absent with AUTOCAD_REQUIRE_CORPUS set",
            disk.display()
        );
        eprintln!("skipping in-tree oracle: extracted System.img absent, NOT validated");
        None
    }
}

/// Markers drawn with layer 2 current, members with layer 1 current: LINE A
/// (1,1)-(2,1) and LINE B (1,3)-(2,3), two columns five units apart.
const GROUP: [&str; 22] = [
    "LAYER", "2", "", "REPEAT", "LAYER", "1", "", "LINE", "1,1", "2,1", "", "LINE", "1,3", "2,3",
    "", "LAYER", "2", "", "ENDREP", "2", "1", "5",
];
/// CHANGE layer through a window holding all four instances.
const CHANGE_ALL: [&str; 6] = ["CHANGE", "W", "0,0", "10,5", "L", "3"];

fn script(parts: &[&[&'static str]]) -> Vec<&'static str> {
    let mut inputs = GROUP.to_vec();
    for part in parts {
        inputs.extend_from_slice(part);
    }
    inputs
}
fn original(disk: &std::path::Path, name: &str, parts: &[&[&'static str]]) -> Vec<u8> {
    generate_dwg_in_tree(disk, name, &script(parts)).unwrap_or_else(|e| panic!("{name}: {e}"))
}
fn items(dwg: &[u8]) -> Vec<Item> {
    acad_dwg::parse(dwg).unwrap().items
}
fn line(layer: u8, y: f64) -> Entity {
    Entity::OnLayer {
        layer,
        entity: Box::new(Entity::Line {
            start: Point { x: 1.0, y },
            end: Point { x: 2.0, y },
        }),
    }
}
fn group(start_layer: u8, end_layer: u8, entities: Vec<Entity>) -> Repeat {
    Repeat {
        start_layer,
        end_layer,
        entities,
        columns: 2,
        rows: 1,
        column_spacing: 5.0,
        row_spacing: 0.0,
    }
}
fn region(dwg: &[u8]) -> &[u8] {
    let (_, meta) = acad_dwg::header::parse_header(dwg).unwrap();
    &dwg[meta.version.entity_start()..meta.entity_end as usize]
}
/// The original's task 5 DXF of `dwg`.
fn task5(disk: &std::path::Path, dwg: &[u8]) -> Vec<u8> {
    let run = observe_in_tree(
        disk,
        b"",
        &[("D2.DWG", dwg)],
        &[(b"5\rD2\r", 400)],
        100_000,
        &["D2.DXF"],
    )
    .unwrap();
    assert_eq!(run.stopped, None, "{}", run.console);
    run.created[0].1.clone()
}
/// The original opens `dwg` (task 2) and ENDs it unchanged.
fn reopen_end(disk: &std::path::Path, dwg: &[u8]) -> Vec<u8> {
    let run = observe_in_tree(
        disk,
        b"",
        &[("D2.DWG", dwg)],
        &[(b"2\rD2\rEND\r", 400)],
        100_000,
        &["D2.DWG"],
    )
    .unwrap();
    assert_eq!(run.stopped, None, "{}", run.console);
    run.created[0].1.clone()
}
/// Lit pixels of the drawing area (below the status row, left of the screen
/// menu, above the command rows) of the original's frame after the keys.
fn drawing_area(disk: &std::path::Path, name: &str, lines: &[&str]) -> Vec<bool> {
    let probe = generate_visual_dwg_in_tree(disk, name, lines).unwrap();
    let frame = acad_oracle::cga::Frame::new(&probe.cga).unwrap();
    (8..168)
        .flat_map(|y| (0..568).map(move |x| (x, y)))
        .map(|(x, y)| frame.lit(x, y))
        .collect()
}
/// The native editor on the same keys. The native selection dialogue needs a
/// Return to finish a window, and native ENDREP asks for the row distance too.
fn native(inputs: &[&str]) -> Vec<Item> {
    let mut editor = Editor::default();
    for input in inputs {
        editor.submit(input).unwrap();
    }
    editor.drawing().items.clone()
}
const NATIVE_GROUP: [&str; 17] = [
    "LAYER 2", "REPEAT", "LAYER 1", "LINE", "1,1", "2,1", "", "LINE", "1,3", "2,3", "", "LAYER 2",
    "ENDREP", "2", "1", "5", "0",
];

#[test]
fn original_change_layer_rewrites_members_never_markers() {
    let Some(disk) = disk() else { return };
    let base = original(&disk, "RLBASE", &[]);
    // The current layer is captured by each marker and each member.
    let drawn = [Item::Repeat(group(2, 2, vec![line(1, 1.0), line(1, 3.0)]))];
    assert_eq!(items(&base), drawn);
    assert_eq!(native(&NATIVE_GROUP), drawn);
    let changed = original(&disk, "RLCHG", &[&CHANGE_ALL]);
    let expected = [Item::Repeat(group(2, 2, vec![line(3, 1.0), line(3, 3.0)]))];
    assert_eq!(items(&changed), expected);
    // Raw records: REPEAT and ENDREP keep layer 2, each member word is 3.
    let records = region(&changed);
    assert_eq!(&records[..4], &[5, 0, 2, 0]);
    assert_eq!(&records[4..8], &[1, 0, 3, 0]);
    assert_eq!(&records[40..44], &[1, 0, 3, 0]);
    assert_eq!(&records[76..80], &[6, 0, 2, 0]);
    // Selection is per source member: the repeated column alone selects both.
    assert_eq!(
        original(
            &disk,
            "RLCHGC",
            &[&["CHANGE", "W", "5,0", "10,5", "L", "3"]]
        ),
        changed
    );
    // A window around A's source instance changes A only; Last is B.
    assert_eq!(
        items(&original(
            &disk,
            "RLCHGA",
            &[&["CHANGE", "W", "0,0", "3,2", "L", "3"]]
        )),
        [Item::Repeat(group(2, 2, vec![line(3, 1.0), line(1, 3.0)]))]
    );
    assert_eq!(
        items(&original(&disk, "RLCHGL", &[&["CHANGE", "L", "L", "3"]])),
        [Item::Repeat(group(2, 2, vec![line(1, 1.0), line(3, 3.0)]))]
    );
    // The native whole-group CHANGE on the same window gives the same group.
    let mut keys = NATIVE_GROUP.to_vec();
    keys.extend(["CHANGE", "W", "0,0", "10,5", "", "L", "3"]);
    assert_eq!(native(&keys), expected);
}

#[test]
fn original_files_hold_no_owner_layer_and_native_codecs_match_them() {
    let Some(disk) = disk() else { return };
    for (name, parts) in [("RLBASE", &[][..]), ("RLCHG", &[&CHANGE_ALL[..]][..])] {
        let dwg = original(&disk, name, parts);
        let drawing = acad_dwg::parse(&dwg).unwrap();
        // Native DWG re-encoding is byte-identical; so is the original's own
        // reopen and END.
        let version = acad_dwg::header::parse_header(&dwg).unwrap().1.version;
        assert_eq!(acad_dwg::write_version(&drawing, version).unwrap(), dwg);
        assert_eq!(reopen_end(&disk, &dwg), dwg);
        // Task 5 DXF: REPEAT,<marker layer>, members with their own layers,
        // ENDREP,<marker layer>; the native writer's bytes, then zero padding.
        let dxf = task5(&disk, &dwg);
        let ours = acad_dxf::try_write(&drawing).unwrap();
        assert_eq!(&dxf[..ours.len()], &ours[..], "{name}");
        assert!(dxf[ours.len()..].iter().all(|&byte| byte == 0), "{name}");
        let member = if name == "RLCHG" { 3 } else { 1 };
        let text = String::from_utf8_lossy(&dxf);
        let body = &text[text.find("REPEAT").unwrap()..text.find('\u{1a}').unwrap()];
        assert_eq!(
            body,
            format!(
                "REPEAT,2\r\nLINE,{member}\r\n1.000000,1.000000,2.000000,1.000000\r\n\
                 LINE,{member}\r\n1.000000,3.000000,2.000000,3.000000\r\n\
                 ENDREP,2\r\n2,1,5.000000,0.000000\r\n"
            )
        );
        assert_eq!(acad_dxf::parse(&dxf).unwrap().items, drawing.items);
    }
}

#[test]
fn original_marker_layer_gates_neither_display_nor_selection() {
    let Some(disk) = disk() else { return };
    let shown = drawing_area(&disk, "RVBASE", &script(&[]));
    let empty = drawing_area(&disk, "RVEMPTY", &["LAYER", "2", ""]);
    assert!(shown.iter().any(|&lit| lit));
    assert!(empty.iter().all(|&lit| !lit));
    // Marker layer OFF: the whole pattern is still drawn.
    assert_eq!(
        drawing_area(&disk, "RVOFF2", &script(&[&["LAYER", "OFF", "2", ""]])),
        shown
    );
    // Member layer OFF: nothing is drawn.
    assert_eq!(
        drawing_area(&disk, "RVOFF1", &script(&[&["LAYER", "OFF", "1", ""]])),
        empty
    );
    // After CHANGE (to a defined layer) and REGEN, the members' new layer
    // gates the pattern; the markers do not. Before REGEN the original shows
    // only part of the pattern, so every frame here follows a REGEN.
    let changed = |name, off: &'static str| {
        let mut lines = vec!["LAYER", "3", ""];
        lines.extend(script(&[&CHANGE_ALL, &["LAYER", "OFF", off, "", "REGEN"]]));
        drawing_area(&disk, name, &lines)
    };
    assert_eq!(changed("RVCHG1", "1"), shown);
    assert_eq!(changed("RVCHG2", "2"), shown);
    assert_eq!(changed("RVCHG3", "3"), empty);
    // CHANGE to an undefined layer leaves it undefined in the file, and the
    // original then draws nothing of the members.
    assert!(!acad_dwg::parse(&original(&disk, "RLCHG", &[&CHANGE_ALL]))
        .unwrap()
        .header
        .layers
        .contains_key(&3));
    assert_eq!(
        drawing_area(&disk, "RVUNDEF", &script(&[&CHANGE_ALL, &["REGEN"]])),
        empty
    );
    // Reopening the defined-layer file shows the same: marker layer OFF and
    // the old member layer OFF keep the pattern, the new layer OFF hides it.
    let mut lines = vec!["LAYER", "3", ""];
    lines.extend(script(&[&CHANGE_ALL]));
    let file = generate_dwg_in_tree(&disk, "RLCHG3", &lines).unwrap();
    for (off, expected) in [("", &shown), ("1", &shown), ("2", &shown), ("3", &empty)] {
        let keys = if off.is_empty() {
            "2\rD2\r".to_owned()
        } else {
            format!("2\rD2\rLAYER\rOFF\r{off}\r\r")
        };
        let run = observe_in_tree(
            &disk,
            b"",
            &[("D2.DWG", &file)],
            &[(keys.as_bytes(), 3000)],
            100_000,
            &[],
        )
        .unwrap();
        assert_eq!(run.stopped, None, "{}", run.console);
        let frame = acad_oracle::cga::Frame::new(&run.cga).unwrap();
        let area: Vec<bool> = (8..168)
            .flat_map(|y| (0..568).map(move |x| (x, y)))
            .map(|(x, y)| frame.lit(x, y))
            .collect();
        assert_eq!(&area, expected, "reopened, layer {off:?} OFF");
    }
    // A window still finds the members with the marker layer OFF ...
    assert_eq!(
        items(&original(
            &disk,
            "RLERO",
            &[&["LAYER", "OFF", "2", "", "ERASE", "W", "0,0", "10,5"]]
        )),
        [Item::Repeat(group(
            2,
            2,
            vec![
                Entity::Erased(Box::new(line(1, 1.0))),
                Entity::Erased(Box::new(line(1, 3.0)))
            ]
        ))]
    );
    // ... and none with the member layer OFF.
    assert_eq!(
        items(&original(
            &disk,
            "RLERM",
            &[&["LAYER", "OFF", "1", "", "ERASE", "W", "0,0", "10,5"]]
        )),
        items(&original(&disk, "RLBASE", &[]))
    );
}

#[test]
fn original_nested_change_keeps_every_marker_layer() {
    let Some(disk) = disk() else { return };
    let mut lines: Vec<&str> = vec![
        "LAYER", "2", "", "REPEAT", "LAYER", "1", "", "LINE", "1,1", "2,1", "", "LAYER", "4", "",
        "REPEAT", "LAYER", "1", "", "LINE", "1,3", "2,3", "", "LAYER", "4", "", "ENDREP", "1", "2",
        "1", "LAYER", "2", "", "ENDREP", "2", "1", "5",
    ];
    lines.extend(["CHANGE", "W", "0,0", "10,6", "L", "3"]);
    let changed = generate_dwg_in_tree(&disk, "RLNESTC", &lines).unwrap();
    let Item::Repeat(outer) = &items(&changed)[0] else {
        panic!("outer group")
    };
    assert_eq!((outer.start_layer, outer.end_layer), (2, 2));
    assert_eq!(outer.entities[0], line(3, 1.0));
    let Entity::Repeat(inner) = &outer.entities[1] else {
        panic!("nested group")
    };
    assert_eq!((inner.start_layer, inner.end_layer), (4, 4));
    assert_eq!(inner.entities, [line(3, 3.0)]);
}

#[test]
fn original_change_layer_over_an_earlier_erased_member_is_fatal() {
    let Some(disk) = disk() else { return };
    let mut keys = String::from("1\rPX\r");
    for line in script(&[&["ERASE", "W", "0,0", "3,2"], &CHANGE_ALL]) {
        keys.push_str(line);
        keys.push('\r');
    }
    let run = observe_in_tree(&disk, b"", &[], &[(keys.as_bytes(), 1500)], 100_000, &[]).unwrap();
    // The window finds A again through its repeated instance (3 found), and
    // the original stops with a fatal error instead of changing the layers.
    assert!(run.console.contains("3 found."), "{}", run.console);
    assert!(
        run.console.contains("BAD ENTITY TYPE -1 PASSED TO EREGEN"),
        "{}",
        run.console
    );
    assert!(run.stopped.is_some());
}
