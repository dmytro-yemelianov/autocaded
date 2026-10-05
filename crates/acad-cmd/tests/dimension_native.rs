//! Offline comparisons against untouched exports from the original application.
use acad_cmd::Editor;
use acad_model::{Entity, Item, Point};
use std::path::Path;

fn near(a: f64, b: f64) -> bool {
    (a - b).abs() <= 1e-10
}
fn point(a: Point, b: Point) -> bool {
    near(a.x, b.x) && near(a.y, b.y)
}
fn entity_eq(a: &Entity, b: &Entity) -> bool {
    match (a, b) {
        (
            Entity::OnLayer {
                layer: la,
                entity: a,
            },
            Entity::OnLayer {
                layer: lb,
                entity: b,
            },
        ) => la == lb && entity_eq(a, b),
        (Entity::Line { start: a, end: b }, Entity::Line { start: c, end: d }) => {
            point(*a, *c) && point(*b, *d)
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
        ) => point(*a, *e) && point(*b, *f) && point(*c, *g) && point(*d, *h),
        (
            Entity::Text {
                origin: a,
                height: b,
                rotation_deg: c,
                value: d,
            },
            Entity::Text {
                origin: e,
                height: f,
                rotation_deg: g,
                value: h,
            },
        ) => point(*a, *e) && near(*b, *f) && near(*c, *g) && d == h,
        _ => false,
    }
}

#[test]
fn retained_native_dimensions_match_complete_ordered_primitives() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/dim");
    let manifest: serde_json::Value =
        serde_json::from_slice(&std::fs::read(root.join("manifest.json")).unwrap()).unwrap();
    let rows = manifest.as_array().unwrap();
    assert_eq!(rows.len(), 33, "the native comparison set must not shrink");
    let mut seen = std::collections::BTreeSet::new();
    let mut failures = Vec::new();
    for row in rows {
        let case = row["case"].as_str().unwrap();
        assert!(seen.insert(case), "duplicate fixture {case}");
        let native =
            acad_dwg::parse(&std::fs::read(root.join(format!("{case}.dwg"))).unwrap()).unwrap();
        let inputs: Vec<String> = serde_json::from_slice(
            &std::fs::read(root.join(format!("{case}.inputs.json"))).unwrap(),
        )
        .unwrap();
        let mut editor = Editor::default();
        for input in inputs {
            editor
                .submit(&input)
                .unwrap_or_else(|e| panic!("{case}: {input:?}: {e}"));
        }
        assert_eq!(editor.prompt(), "Command", "{case}");
        assert_eq!(
            editor.drawing().header.dim_arrow,
            native.header.dim_arrow,
            "{case}"
        );
        assert_eq!(editor.drawing().header.units, native.header.units, "{case}");
        let encoded = acad_dwg::write(editor.drawing()).unwrap();
        let saved = acad_dwg::parse(&encoded).unwrap();
        assert_eq!(
            saved.items,
            editor.drawing().items,
            "{case}: DWG save/reopen"
        );
        assert_eq!(
            saved.header.dim_arrow, native.header.dim_arrow,
            "{case}: saved arrow"
        );
        let actual = &editor.drawing().items;
        assert_eq!(actual.len(), native.items.len(), "{case}");
        for (i, (a, b)) in actual.iter().zip(&native.items).enumerate() {
            match (a, b) {
                (Item::Entity(a), Item::Entity(b)) if entity_eq(a, b) => {}
                _ => failures.push(format!("{case}[{i}]:\n actual: {a:?}\n native: {b:?}")),
            }
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[test]
fn alphabetic_dimension_labels_use_font_width_for_text_origin() {
    // Rust font/layout contract, not an additional original DIM observation.
    // Arrow fit on this vertical line uses the inside text height (span 1.65
    // >= h + 6A = 135/128), so the arrows are internal (docs/native-dim.md,
    // "Arrow fit"); the font width still centres the text.
    let mut editor = Editor::default();
    for input in ["DIM", "0,0", "4,0", "0,1.65", "MMMM"] {
        editor.submit(input).unwrap();
    }
    let entities: Vec<_> = editor.drawing().entities().collect();
    let Entity::OnLayer { entity, .. } = entities[4] else {
        panic!("layered arrow")
    };
    let Entity::Solid { p3, .. } = entity.as_ref() else {
        panic!("arrow")
    };
    assert!(
        point(*p3, Point { x: 4.0, y: 0.0 }),
        "first internal arrow is at the first dimension-line end"
    );
    let Entity::OnLayer { entity, .. } = entities[6] else {
        panic!("layered text")
    };
    let Entity::Text {
        origin,
        height,
        rotation_deg,
        value,
    } = entity.as_ref()
    else {
        panic!("text")
    };
    assert!(point(
        *origin,
        Point {
            x: 4.0 - 88.0 * (27.0 / 128.0) / 21.0 / 2.0,
            y: 1.65 / 2.0 - 27.0 / 256.0
        }
    ));
    assert_eq!(*height, 27.0 / 128.0);
    assert_eq!(*rotation_deg, 0.0);
    assert_eq!(value, "MMMM");
    let reopened = acad_dwg::parse(&acad_dwg::write(editor.drawing()).unwrap()).unwrap();
    assert_eq!(reopened.items, editor.drawing().items);
    editor.submit("UNDO").unwrap();
    assert!(editor.drawing().items.is_empty());
}

#[test]
fn dimension_history_is_committed_with_geometry_and_rewound_by_undo() {
    let mut editor = Editor::default();
    for option in ["B", "C"] {
        editor.submit("DIM").unwrap();
        assert!(editor.submit(option).is_err());
        assert_eq!(editor.prompt(), "Command");
        assert!(editor.drawing().items.is_empty());
    }
    for input in ["DIM", "1,1", "5,1", "3,2", ""] {
        editor.submit(input).unwrap();
    }
    let prior = editor.drawing().clone();
    for input in ["DIM", "B", "2,4", ""] {
        editor.submit(input).unwrap();
    }
    editor.submit("UNDO").unwrap();
    assert_eq!(editor.drawing(), &prior);
    editor.submit("DIM").unwrap();
    editor.submit("C").unwrap();
    editor.cancel_command().unwrap();
    for input in ["DIM", "C", "2,4", ""] {
        editor.submit(input).unwrap();
    }
    let mut control = Editor::default();
    for input in ["DIM", "1,1", "5,1", "3,2", "", "DIM", "C", "2,4", ""] {
        control.submit(input).unwrap();
    }
    assert_eq!(editor.drawing(), control.drawing());
    editor.submit("UNDO").unwrap();
    editor.submit("UNDO").unwrap();
    editor.submit("DIM").unwrap();
    assert!(editor.submit("B").is_err());
    let mut reopened = Editor::new(prior);
    reopened.submit("DIM").unwrap();
    assert!(reopened.submit("C").is_err());
}

#[test]
fn failed_dimension_preserves_drawing_and_previous_history() {
    let mut editor = Editor::default();
    for input in ["DIM", "1,1", "5,1", "3,2", ""] {
        editor.submit(input).unwrap();
    }
    let prior = editor.drawing().clone();
    for input in ["DIM", "1,1", "5,1", "3,1"] {
        editor.submit(input).unwrap();
    }
    assert!(editor.submit("").is_err());
    assert_eq!(editor.drawing(), &prior);
    editor.cancel_command().unwrap();
    for input in ["DIM", "A", "1e308", "DIM", "1,1", "5,1", "3,2"] {
        editor.submit(input).unwrap();
    }
    assert!(editor.submit("").is_err());
    assert_eq!(editor.drawing(), &prior);
    editor.cancel_command().unwrap();
    for input in ["DIM", "A", "0.140625", "DIM", "C", "2,4", ""] {
        editor.submit(input).unwrap();
    }
    // Both failures must leave the previous completed dimension as C's anchor.
    let native = acad_dwg::parse(include_bytes!("fixtures/dim/DCONT1.dwg")).unwrap();
    assert_eq!(editor.drawing().items.len(), native.items.len());
    for (actual, expected) in editor.drawing().entities().zip(native.entities()) {
        assert!(entity_eq(actual, expected));
    }
    editor.submit("UNDO").unwrap();
    assert_eq!(editor.drawing(), &prior);
}

#[test]
fn crossing_text_push_reverses_with_the_extension_direction() {
    // Native pin of the measured rule (docs/native-dim.md); the original
    // values are asserted by acad-oracle/tests/dim_arrows.rs.
    for (inputs, x) in [
        (["DIM", "A", "0.5", "DIM", "1,1", "5,1", "3,2", ""], 3.5),
        (
            ["DIM", "A", "0.5", "DIM", "9,1", "5,1", "7,2", ""],
            5.0 - (100.0 * 0.75 / 21.0 - 1.5),
        ),
        (["DIM", "A", "3", "DIM", "1,1", "5,1", "3,2", ""], 6.0),
    ] {
        let mut editor = Editor::default();
        for input in inputs {
            editor.submit(input).unwrap();
        }
        let Some(Entity::OnLayer { entity, .. }) = editor.drawing().entities().last() else {
            panic!("layered text")
        };
        let Entity::Text { origin, .. } = entity.as_ref() else {
            panic!("text")
        };
        assert!(near(origin.x, x), "{inputs:?}: {origin:?}");
    }
}

#[test]
fn pbcc_mixed_history_keeps_its_two_known_extension_line_gaps() {
    // Diagnostic, not a passing parity case: PBCC (retained dim-supplement,
    // SHA-256 c31b3e92…) replays to 28 records of which only the two
    // extension LINEs of its final C differ (native ends at y 3.5, Rust at
    // 4.5). Unrelated to arrow fit (the same pair differs under the old
    // W+6A rule). If a future change closes or widens the gap, move PBCC into
    // the manifest or explain the new records here.
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/dim");
    let native = acad_dwg::parse(&std::fs::read(root.join("PBCC.dwg")).unwrap()).unwrap();
    let inputs: Vec<String> =
        serde_json::from_slice(&std::fs::read(root.join("PBCC.inputs.json")).unwrap()).unwrap();
    let mut editor = Editor::default();
    for input in inputs {
        editor.submit(&input).unwrap();
    }
    let actual = &editor.drawing().items;
    assert_eq!(actual.len(), native.items.len());
    let differing: Vec<usize> = actual
        .iter()
        .zip(&native.items)
        .enumerate()
        .filter(|(_, pair)| !matches!(pair, (Item::Entity(a), Item::Entity(b)) if entity_eq(a, b)))
        .map(|(i, _)| i)
        .collect();
    assert_eq!(differing, [21, 22]);
}
