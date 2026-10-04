//! In-tree original anchors for circular ARRAY options and BREAK
//! (docs/native-array-break.md). Each script runs on the original ACAD.EXE
//! with the in-tree 8086/DOS runner. `compare` cases must agree with the Rust
//! editor in record kinds, erased flags, layers and geometry to 1e-6; the
//! `divergence` tests pin the measured original result where the native editor
//! deliberately differs. Every original behaviour the doc cites is here.
#![cfg(unix)]
use acad_model::{Entity, Item, Point};

fn disk() -> Option<std::path::PathBuf> {
    let disk = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../corpus/raw/Autodesk AutoCAD 1.4 (5.25)/System.img");
    if disk.exists() {
        Some(disk)
    } else {
        eprintln!("skipping in-tree oracle: extracted System.img absent");
        None
    }
}

fn close(a: f64, b: f64) -> bool {
    (a - b).abs() < 1e-6
}
fn close_point(a: &Point, b: &Point) -> bool {
    close(a.x, b.x) && close(a.y, b.y)
}
fn close_angle(a: f64, b: f64) -> bool {
    let d = (a - b).rem_euclid(360.0);
    d < 1e-6 || 360.0 - d < 1e-6
}

fn same_entity(a: &Entity, b: &Entity) -> bool {
    match (a, b) {
        (
            Entity::OnLayer { layer, entity },
            Entity::OnLayer {
                layer: other_layer,
                entity: other,
            },
        ) => layer == other_layer && same_entity(entity, other),
        (Entity::Line { start, end }, Entity::Line { start: s, end: e }) => {
            close_point(start, s) && close_point(end, e)
        }
        (
            Entity::Trace { p1, p2, p3, p4 },
            Entity::Trace {
                p1: q1,
                p2: q2,
                p3: q3,
                p4: q4,
            },
        ) => {
            close_point(p1, q1) && close_point(p2, q2) && close_point(p3, q3) && close_point(p4, q4)
        }
        (
            Entity::Arc {
                center,
                radius,
                start_deg,
                end_deg,
            },
            Entity::Arc {
                center: c,
                radius: r,
                start_deg: s,
                end_deg: e,
            },
        ) => {
            close_point(center, c)
                && close(*radius, *r)
                && close_angle(*start_deg, *s)
                && close_angle(*end_deg, *e)
        }
        (
            Entity::Insert {
                origin,
                x_scale,
                y_scale,
                rotation_deg,
                name,
            },
            Entity::Insert {
                origin: o,
                x_scale: x,
                y_scale: y,
                rotation_deg: r,
                name: n,
            },
        ) => {
            close_point(origin, o)
                && close(*x_scale, *x)
                && close(*y_scale, *y)
                && close_angle(*rotation_deg, *r)
                && name == n
        }
        _ => a == b,
    }
}

fn same_item(a: &Item, b: &Item) -> bool {
    match (a, b) {
        (Item::Entity(a), Item::Entity(b)) | (Item::Erased(a), Item::Erased(b)) => {
            same_entity(a, b)
        }
        _ => a == b,
    }
}

fn original(name: &str, inputs: &[&str]) -> Option<Vec<Item>> {
    let disk = disk()?;
    let dwg = acad_oracle::generate_dwg_in_tree(&disk, name, inputs)
        .unwrap_or_else(|error| panic!("{name} {inputs:?}: {error}"));
    Some(acad_dwg::parse(&dwg).unwrap().items)
}

fn rust(name: &str, inputs: &[&str]) -> acad_cmd::Editor {
    let mut editor = acad_cmd::Editor::default();
    for input in inputs {
        editor
            .submit(input)
            .unwrap_or_else(|error| panic!("{name} {input}: {error}"));
    }
    editor
}

fn same_items(a: &[Item], b: &[Item]) -> bool {
    a.len() == b.len() && a.iter().zip(b).all(|(a, b)| same_item(a, b))
}

fn compare(name: &str, inputs: &[&str]) -> Option<Vec<Item>> {
    let original = original(name, inputs)?;
    let editor = rust(name, inputs);
    let rust = &editor.drawing().items;
    assert!(
        same_items(&original, rust),
        "{name} {inputs:?}\noriginal {original:#?}\nrust {rust:#?}"
    );
    Some(original)
}

fn live(items: &[Item]) -> Vec<&Entity> {
    items
        .iter()
        .filter_map(|item| match item {
            Item::Entity(entity) => Some(entity),
            _ => None,
        })
        .collect()
}

fn layered(entity: Entity) -> Entity {
    Entity::OnLayer {
        layer: 1,
        entity: Box::new(entity),
    }
}
fn p(x: f64, y: f64) -> Point {
    Point { x, y }
}
fn line(a: Point, b: Point) -> Item {
    Item::Entity(layered(Entity::Line { start: a, end: b }))
}

/// Submit every input but the last, which the native editor must reject
/// while keeping its prompt and drawing.
fn native_rejects_last(name: &str, inputs: &[&str]) -> acad_cmd::Editor {
    let (last, setup) = inputs.split_last().unwrap();
    let mut editor = rust(name, setup);
    let (prompt, before) = (editor.prompt().to_owned(), editor.drawing().clone());
    assert!(
        editor.submit(last).is_err(),
        "{name}: native accepts {last}"
    );
    assert_eq!(editor.prompt(), prompt, "{name}");
    assert_eq!(editor.drawing(), &before, "{name}");
    editor
}

const ARRAY_LINE: [&str; 8] = ["LINE", "5,3", "6,3", "", "ARRAY", "L", "C", "4,3"];

#[test]
fn original_angle_to_cover_counts_and_directions() {
    for (index, (angle, items, count)) in [
        ("90", "-360", 4),
        ("90", "-270", 4),
        ("90", "-180", 3),
        ("90", "-135", 3),
        ("90", "-134", 2),
        ("90", "-100", 2),
        ("90", "-90", 2),
        ("90", "-46", 2),
        ("90", "-45", 2),
        ("90", "-44", 1),
        ("90", "-1", 1),
        ("90", "-359", 5),
        ("90", "-361", 5),
        ("90", "-720", 9),
        ("90", "-1080", 13),
        ("90", "0", 4),
        ("90", "-0", 4),
        ("100", "0", 4),
        ("80", "0", 5),
        ("70", "0", 5),
        ("135", "0", 3),
        ("100", "-360", 4),
        ("80", "-360", 5),
        ("135", "-360", 3),
        ("-90", "-180", 3),
        ("-90", "-360", 4),
        ("360", "3", 3),
        ("360", "0", 1),
    ]
    .into_iter()
    .enumerate()
    {
        let mut inputs = ARRAY_LINE.to_vec();
        inputs.extend([angle, items]);
        let Some(items) = compare(&format!("ORCAF{index}"), &inputs) else {
            return;
        };
        assert_eq!(live(&items).len(), count, "{angle} {items:?}");
    }
    // Clockwise for a negative angle: the first copy is below the center.
    let mut inputs = ARRAY_LINE.to_vec();
    inputs.extend(["-90", "-180"]);
    if let Some(items) = original("ORCACW", &inputs) {
        assert!(same_item(&items[1], &line(p(4.0, 2.0), p(5.0, 2.0))));
    }
}

#[test]
fn divergence_original_aborts_array_where_native_retries() {
    let after = ["LINE", "0,0", "1,1", ""];
    for (index, tail) in [&["0"][..], &["400"], &["-400"], &["90", "2.5"]]
        .into_iter()
        .enumerate()
    {
        let mut inputs = ARRAY_LINE.to_vec();
        inputs.extend(tail);
        native_rejects_last("abort", &inputs);
        inputs.extend(after);
        // The original abandons ARRAY; the next line is a new command.
        if let Some(items) = original(&format!("ORCAB{index}"), &inputs) {
            assert!(
                same_items(
                    &items,
                    &[
                        line(p(5.0, 3.0), p(6.0, 3.0)),
                        line(p(0.0, 0.0), p(1.0, 1.0))
                    ]
                ),
                "{tail:?}: {items:#?}"
            );
        }
    }
}

const BLOCK_ARRAY: [&str; 18] = [
    "LINE", "1,0", "2,0", "", "BLOCK", "B", "0,0", "LAST", "INSERT", "B", "5,3", "1", "1", "10",
    "ARRAY", "L", "C", "4,3",
];

fn insert_rotations(items: &[Item]) -> Vec<f64> {
    live(items)
        .into_iter()
        .filter_map(|entity| match entity {
            Entity::OnLayer { entity, .. } => match entity.as_ref() {
                Entity::Insert { rotation_deg, .. } => Some(*rotation_deg),
                _ => None,
            },
            _ => None,
        })
        .collect()
}

#[test]
fn original_single_block_rotation_choice() {
    for (name, tail, rotations) in [
        ("ORCARY", ["90", "4", "Y"], vec![10.0, 100.0, 190.0, 280.0]),
        ("ORCARN", ["90", "4", "N"], vec![10.0; 4]),
        ("ORCARE", ["90", "4", ""], vec![10.0; 4]),
        ("ORCARM", ["-90", "3", "Y"], vec![10.0, 280.0, 190.0]),
        ("ORCART", ["120", "4", "Y"], vec![10.0, 130.0, 250.0, 10.0]),
    ] {
        let mut inputs = BLOCK_ARRAY.to_vec();
        inputs.extend(tail);
        let Some(items) = compare(name, &inputs) else {
            return;
        };
        let got = insert_rotations(&items);
        assert_eq!(got.len(), rotations.len(), "{name}");
        assert!(
            got.iter().zip(&rotations).all(|(a, b)| close_angle(*a, *b)),
            "{name}: {got:?}"
        );
    }
}

#[test]
fn divergence_original_treats_other_rotate_answers_as_no() {
    let mut inputs = BLOCK_ARRAY.to_vec();
    inputs.extend(["90", "4", "X"]);
    native_rejects_last("rotate X", &inputs);
    inputs.extend(["LINE", "0,0", "1,1", ""]);
    if let Some(items) = original("ORCARX", &inputs) {
        assert_eq!(insert_rotations(&items).len(), 4);
        assert!(insert_rotations(&items)
            .iter()
            .all(|r| close_angle(*r, 10.0)));
        assert!(same_item(
            items.last().unwrap(),
            &line(p(0.0, 0.0), p(1.0, 1.0))
        ));
    }
}

#[test]
fn original_asks_to_rotate_only_for_a_single_block() {
    // Two objects by window: no question, so the following LINE runs.
    let setup = [
        "LINE", "1,0", "2,0", "", "BLOCK", "B", "0,0", "LAST", "INSERT", "B", "5,3", "1", "1",
        "10", "LINE", "5,4", "6,4", "", "ARRAY", "W", "4.5,2.5", "7,5",
    ];
    let tail = ["C", "4,3", "90", "2", "LINE", "0,0", "1,1", ""];
    let mut original_inputs = setup.to_vec();
    original_inputs.extend(tail);
    // The native window selector finishes on Return.
    let mut native_inputs = setup.to_vec();
    native_inputs.push("");
    native_inputs.extend(tail);
    let native = rust("two objects", &native_inputs);
    let native_items = native.drawing().items.clone();
    let Some(items) = original("ORCAR2", &original_inputs) else {
        return;
    };
    let (original_live, native_live) = (live(&items), live(&native_items));
    assert_eq!(original_live.len(), 5);
    assert_eq!(native_live.len(), 5);
    // Copy order within one array step differs (window order); compare as sets.
    for entity in &original_live {
        assert!(
            native_live.iter().any(|other| same_entity(entity, other)),
            "{entity:?} missing from native {native_live:#?}"
        );
    }
    assert!(insert_rotations(&items)
        .iter()
        .all(|r| close_angle(*r, 10.0)));
}

#[test]
fn original_break_picking_lines_arcs_and_circles() {
    for (index, tail) in [
        &["2,0", "7,0"][..],
        &["7,0", "2,0"],
        &["2,0.05", "7,1"],
        &["2,0", "F", "3,0", "7,0"],
        &["2,0", "12,0"],
        &["8,0", "10,0"],
        // First-endpoint cases: erased source plus the remainder.
        &["0,0", "5,0"],
        &["5,0", "0,0"],
        &["2,0", "F", "0,0", "5,0"],
        &["2,0", "0,0"],
        // Whole-object spans erase the record.
        &["0,0", "10,0"],
        &["10,0", "0,0"],
        &["0,0", "12,0"],
        &["0,0", "10,0", "OOPS"],
    ]
    .into_iter()
    .enumerate()
    {
        let mut inputs = vec!["LINE", "0,0", "10,0", "", "BREAK"];
        inputs.extend(tail);
        compare(&format!("ORCBL{index}"), &inputs);
    }
    // A line drawn right to left trims from its own start and end.
    for (index, tail) in [
        ["8,0", "12,0"],
        ["2,0", "0,0"],
        ["8,0", "10,0"],
        ["2,0", "12,0"],
    ]
    .into_iter()
    .enumerate()
    {
        let mut inputs = vec!["LINE", "10,0", "0,0", "", "BREAK"];
        inputs.extend(tail);
        compare(&format!("ORCBR{index}"), &inputs);
    }
    // OOPS restores the last ERASE, never BREAK's erased record.
    compare(
        "ORCBOO",
        &[
            "LINE", "0,2", "10,2", "", "LINE", "0,5", "10,5", "", "ERASE", "L", "BREAK", "0,2",
            "10,2", "OOPS",
        ],
    );
    compare("ORCBC", &["CIRCLE", "0,0", "5", "BREAK", "5,0", "0,5"]);
    let arc = ["ARC", "5,3", "3,5", "1,3", "BREAK"];
    for (index, tail) in [
        ["4,4.732", "2,4.732"],
        ["4,4.732", "4.732,2"],
        ["4,4.732", "1.268,2"],
        ["2,4.732", "3,1"],
        ["4,4.732", "3,1"],
        ["4,4.732", "3,4"],
        ["5,3", "1,3"],
    ]
    .into_iter()
    .enumerate()
    {
        let mut inputs = arc.to_vec();
        inputs.extend(tail);
        compare(&format!("ORCBA{index}"), &inputs);
    }
}

#[test]
fn divergence_original_break_quirks_the_native_editor_does_not_reproduce() {
    let unchanged_line = |start: Point, end: Point| vec![line(start, end)];
    // A second point whose X lies below the line's X range leaves the line
    // alone (a first pick there simply misses);
    // native projects and trims (or erases a whole span).
    for (index, (from, to, tail)) in [
        ("0,0", "10,0", ["2,0", "-1,0"]),
        ("0,0", "10,0", ["8,0", "-1,0"]),
        ("10,0", "0,0", ["2,0", "-1,0"]),
    ]
    .into_iter()
    .enumerate()
    {
        let mut inputs = vec!["LINE", from, to, "", "BREAK"];
        inputs.extend(tail);
        let x = |text: &str| text.split(',').next().unwrap().parse::<f64>().unwrap();
        let (start, end) = (p(x(from), 0.0), p(x(to), 0.0));
        let native = rust("x below", &inputs);
        assert!(!same_items(
            &native.drawing().items,
            &unchanged_line(start, end)
        ));
        if let Some(items) = original(&format!("ORCQX{index}"), &inputs) {
            assert!(
                same_items(&items, &unchanged_line(start, end)),
                "{inputs:?}"
            );
        }
    }
    // Coincident points: LINE unchanged, ARC split in two, CIRCLE becomes a
    // full-circle ARC; native rejects all three.
    let inputs = ["LINE", "0,0", "10,0", "", "BREAK", "2,0", "2,0"];
    native_rejects_last("coincident line", &inputs);
    if let Some(items) = original("ORCQCL", &inputs) {
        assert!(same_items(
            &items,
            &unchanged_line(p(0.0, 0.0), p(10.0, 0.0))
        ));
    }
    let inputs = ["ARC", "5,3", "3,5", "1,3", "BREAK", "4,4.732", "4,4.732"];
    native_rejects_last("coincident arc", &inputs);
    if let Some(items) = original("ORCQCA", &inputs) {
        assert_eq!(live(&items).len(), 2);
    }
    let inputs = ["CIRCLE", "0,0", "5", "BREAK", "5,0", "5,0"];
    native_rejects_last("coincident circle", &inputs);
    if let Some(items) = original("ORCQCC", &inputs) {
        let arc = layered(Entity::Arc {
            center: p(0.0, 0.0),
            radius: 5.0,
            start_deg: 0.0,
            end_deg: 0.0,
        });
        assert!(matches!(items[0], Item::Erased(_)));
        assert!(same_item(&items[1], &Item::Entity(arc)));
    }
    // A miss aborts the original BREAK; native keeps the prompt.
    let inputs = ["LINE", "0,0", "10,0", "", "BREAK", "5,5"];
    native_rejects_last("miss", &inputs);
    let mut inputs = inputs.to_vec();
    inputs.extend(["LINE", "0,0", "1,1", ""]);
    if let Some(items) = original("ORCQMS", &inputs) {
        assert!(same_items(
            &items,
            &[
                line(p(0.0, 0.0), p(10.0, 0.0)),
                line(p(0.0, 0.0), p(1.0, 1.0))
            ]
        ));
    }
}

#[test]
fn original_trace_break_outside_miters() {
    compare(
        "ORCBTR",
        &["TRACE", "1", "0,0", "10,0", "", "BREAK", "2,0.5", "7,0.5"],
    );
    compare(
        "ORCBTS",
        &["TRACE", "1", "0,0", "10,0", "", "BREAK", "0,0", "7,0"],
    );
    compare(
        "ORCBTW",
        &["TRACE", "1", "0,0", "10,0", "", "BREAK", "0,0", "10,0"],
    );
    compare(
        "ORCBTV",
        &["TRACE", "1", "0,0", "10,0", "", "BREAK", "10,0", "0,0"],
    );
    let mitered = ["TRACE", "0.5", "1,1", "7,1", "7,6", "", "BREAK"];
    for (index, tail) in [
        &["2,1.25", "5,0.75"][..],
        &["2,0.75", "5,0.75"],
        &["3,1.25", "8,1.25"],
        &["3,1.25", "F", "1,1", "5,1.25"],
        &["1,1", "3,1.25"],
    ]
    .into_iter()
    .enumerate()
    {
        let mut inputs = mitered.to_vec();
        inputs.extend(tail);
        compare(&format!("ORCBM{index}"), &inputs);
    }
}

#[test]
fn divergence_original_cut_inside_a_miter_emits_a_self_touching_quad() {
    let inputs = [
        "TRACE", "0.5", "1,1", "7,1", "7,6", "", "BREAK", "3,1.25", "6.9,1.25",
    ];
    native_rejects_last("inside miter", &inputs);
    if let Some(items) = original("ORCBMQ", &inputs) {
        assert_eq!(items.len(), 3);
        let quad = Item::Entity(layered(Entity::Trace {
            p1: p(6.75, 1.25),
            p2: p(6.9, 0.75),
            p3: p(6.75, 1.25),
            p4: p(7.25, 0.75),
        }));
        assert!(same_item(&items[2], &quad), "{items:#?}");
    }
}
