//! Offline replay of retained original HATCH exports; no guest is launched.
use acad_cmd::Editor;
use acad_model::{Entity, Item, Point};
use std::path::Path;

fn close(a: f64, b: f64) -> bool {
    (a - b).abs() <= 1e-10
}
fn point_eq(a: Point, b: Point) -> bool {
    close(a.x, b.x) && close(a.y, b.y)
}

fn entity_eq(a: &Entity, b: &Entity) -> bool {
    match (a, b) {
        (
            Entity::OnLayer {
                layer: a,
                entity: x,
            },
            Entity::OnLayer {
                layer: b,
                entity: y,
            },
        ) => a == b && entity_eq(x, y),
        (Entity::Line { start: a, end: b }, Entity::Line { start: c, end: d }) => {
            point_eq(*a, *c) && point_eq(*b, *d)
        }
        (
            Entity::Circle {
                center: a,
                radius: b,
            },
            Entity::Circle {
                center: c,
                radius: d,
            },
        ) => point_eq(*a, *c) && close(*b, *d),
        (
            Entity::Insert {
                origin: a,
                x_scale: b,
                y_scale: c,
                rotation_deg: d,
                name: e,
            },
            Entity::Insert {
                origin: f,
                x_scale: g,
                y_scale: h,
                rotation_deg: i,
                name: j,
            },
        ) => point_eq(*a, *f) && close(*b, *g) && close(*c, *h) && close(*d, *i) && e == j,
        _ => false,
    }
}

fn assert_items(actual: &[Item], expected: &[Item], label: &str) {
    assert_eq!(actual.len(), expected.len(), "{label}");
    for (index, (a, b)) in actual.iter().zip(expected).enumerate() {
        match (a, b) {
            (Item::Entity(a), Item::Entity(b)) => {
                assert!(entity_eq(a, b), "{label}[{index}]: {a:?} != {b:?}")
            }
            (Item::Block(a), Item::Block(b)) => {
                assert_eq!(a.name, b.name, "{label}[{index}]");
                assert!(point_eq(a.base, b.base), "{label}[{index}]");
                assert_eq!(a.entities.len(), b.entities.len(), "{label}[{index}]");
                for (i, (a, b)) in a.entities.iter().zip(&b.entities).enumerate() {
                    assert!(
                        entity_eq(a, b),
                        "{label}[{index}] stroke {i}: {a:?} != {b:?}"
                    );
                }
            }
            _ => panic!("{label}[{index}]: unexpected item kinds: {a:?} != {b:?}"),
        }
    }
}

#[test]
fn retained_hatch_exports_match_complete_ordered_geometry() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/hatch");
    for case in ["HNETDEF", "HLINCIRC"] {
        let dwg = std::fs::read(root.join(format!("{case}.dwg"))).unwrap();
        let native = acad_dwg::parse(&dwg).unwrap();
        assert_eq!(
            acad_dwg::write(&native).unwrap(),
            dwg,
            "{case}: retained DWG roundtrip"
        );
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
        assert_eq!(editor.prompt(), "Command");
        assert_items(&editor.drawing().items, &native.items, case);
        let saved = acad_dwg::parse(&acad_dwg::write(editor.drawing()).unwrap()).unwrap();
        assert_items(&saved.items, &native.items, case);
    }
}

fn transform(point: Point, scale: f64, angle: f64) -> Point {
    let (sine, cosine) = angle.to_radians().sin_cos();
    Point {
        x: scale * (point.x * cosine - point.y * sine),
        y: scale * (point.x * sine + point.y * cosine),
    }
}

fn transform_entity(entity: &mut Entity, scale: f64, angle: f64) {
    match entity {
        Entity::OnLayer { entity, .. } => transform_entity(entity, scale, angle),
        Entity::Line { start, end } => {
            *start = transform(*start, scale, angle);
            *end = transform(*end, scale, angle);
        }
        Entity::Insert { origin, .. } => {
            *origin = transform(*origin, scale, angle);
        }
        _ => panic!("unexpected NET fixture entity"),
    }
}

#[test]
fn net_scale_and_rotation_transform_the_retained_geometry() {
    // Rust covariance check, not a new native scale/angle observation.
    let mut expected = acad_dwg::parse(include_bytes!("fixtures/hatch/HNETDEF.dwg")).unwrap();
    for item in &mut expected.items {
        match item {
            Item::Entity(entity) => transform_entity(entity, 2.0, 30.0),
            Item::Block(block) => {
                block.base = transform(block.base, 2.0, 30.0);
                for entity in &mut block.entities {
                    transform_entity(entity, 2.0, 30.0);
                }
            }
            _ => panic!("unexpected NET fixture item"),
        }
    }
    let mut editor = Editor::default();
    editor.submit("LINE").unwrap();
    for (x, y) in [(1.0, 1.0), (5.0, 1.0), (5.0, 5.0), (1.0, 5.0), (1.0, 1.0)] {
        let p = transform(Point { x, y }, 2.0, 30.0);
        editor.submit(&format!("{},{}", p.x, p.y)).unwrap();
    }
    for input in ["", "HATCH", "NET", "2", "30", "ALL"] {
        editor.submit(input).unwrap();
    }
    assert_items(
        &editor.drawing().items,
        &expected.items,
        "NET scale2 angle30",
    );
}

#[test]
fn net_clips_both_families_to_circles_and_preserves_holes() {
    let mut editor = Editor::default();
    for input in [
        "CIRCLE", "3,3", "2", "CIRCLE", "3,3", "1", "HATCH", "NET", "", "", "ALL",
    ] {
        editor.submit(input).unwrap();
    }
    let block = editor.drawing().blocks().next().unwrap();
    assert_eq!(block.entities.len(), 92);
    for entity in &block.entities {
        let Entity::OnLayer { layer: 127, entity } = entity else {
            panic!("hatch layer")
        };
        let Entity::Line { start, end } = entity.as_ref() else {
            panic!("hatch stroke")
        };
        assert!(close(start.x, end.x) || close(start.y, end.y));
        for p in [start, end] {
            let r = (p.x - 3.0).hypot(p.y - 3.0);
            assert!((1.0 - 1e-10..=2.0 + 1e-10).contains(&r));
        }
        let middle = Point {
            x: (start.x + end.x) / 2.0,
            y: (start.y + end.y) / 2.0,
        };
        assert!(
            (middle.x - 3.0).hypot(middle.y - 3.0) >= 1.0 - 1e-10,
            "stroke crosses the hole"
        );
    }
}

#[test]
fn net_failures_leave_the_drawing_and_undo_stack_unchanged() {
    let mut editor = Editor::default();
    for input in [
        "LINE",
        "0,0",
        "6250,0",
        "6250,6250.125",
        "0,6250.125",
        "0,0",
        "",
    ] {
        editor.submit(input).unwrap();
    }
    let mut control = editor.clone();
    let before = editor.drawing().clone();
    for input in ["HATCH", "NET", "", ""] {
        editor.submit(input).unwrap();
    }
    let error = editor.submit("ALL").unwrap_err();
    assert!(error.contains("aggregate line limit"), "{error}");
    assert_eq!(editor.drawing(), &before);
    editor.cancel_command().unwrap();
    editor.submit("UNDO").unwrap();
    control.submit("UNDO").unwrap();
    assert_eq!(editor.drawing(), control.drawing());
}

#[test]
fn net_rejects_open_boundaries_without_partial_blocks() {
    let mut editor = Editor::default();
    for input in [
        "LINE", "1,1", "5,1", "5,5", "1,5", "", "HATCH", "NET", "", "",
    ] {
        editor.submit(input).unwrap();
    }
    let before = editor.drawing().clone();
    assert!(editor
        .submit("ALL")
        .unwrap_err()
        .contains("closed, unbranched loops"));
    assert_eq!(editor.drawing(), &before);
}

#[test]
fn net_counts_strokes_split_by_holes_against_the_aggregate_limit() {
    let mut editor = Editor::default();
    // Two families have 64,000 grid offsets, but holes split them into more
    // than 100,000 actual strokes. An offset-only limit would accept this.
    for input in [
        "CIRCLE",
        "3000,3000",
        "2000",
        "CIRCLE",
        "3000,3000",
        "1999.5",
        "HATCH",
        "NET",
        "",
        "",
    ] {
        editor.submit(input).unwrap();
    }
    let before = editor.drawing().clone();
    let error = editor.submit("ALL").unwrap_err();
    assert!(error.contains("aggregate line limit"), "{error}");
    assert_eq!(editor.drawing(), &before);
}

#[test]
fn net_rejects_unrepresentable_grid_indices_without_partial_geometry() {
    let mut editor = Editor::default();
    for input in ["CIRCLE", "1e19,10", "1", "HATCH", "NET", "", ""] {
        editor.submit(input).unwrap();
    }
    let before = editor.drawing().clone();
    let error = editor.submit("ALL").unwrap_err();
    assert!(error.contains("grid index range"), "{error}");
    assert_eq!(editor.drawing(), &before);
}
