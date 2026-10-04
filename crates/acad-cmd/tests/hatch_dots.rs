//! Dot contracts derived from retained PAT definitions, not native export parity.
use acad_cmd::Editor;
use acad_model::{Entity, Point};

fn p(x: f64, y: f64) -> Point {
    Point { x, y }
}
fn inputs(editor: &mut Editor, values: &[&str]) {
    for value in values {
        editor.submit(value).unwrap();
    }
}
fn transform(q: Point, scale: f64, angle: f64) -> Point {
    let (s, c) = angle.to_radians().sin_cos();
    p(scale * (q.x * c - q.y * s), scale * (q.x * s + q.y * c))
}
fn rectangle(editor: &mut Editor, lo: f64, hi: f64, scale: f64, angle: f64) {
    editor.submit("LINE").unwrap();
    for q in [p(lo, lo), p(hi, lo), p(hi, hi), p(lo, hi), p(lo, lo)] {
        let q = transform(q, scale, angle);
        editor.submit(&format!("{},{}", q.x, q.y)).unwrap();
    }
    editor.submit("").unwrap();
}
fn hatch(editor: &mut Editor, pattern: &str, scale: f64, angle: f64) {
    inputs(
        editor,
        &[
            "HATCH",
            pattern,
            &scale.to_string(),
            &angle.to_string(),
            "ALL",
        ],
    );
}
fn geometry(editor: &Editor) -> Vec<Entity> {
    editor
        .drawing()
        .blocks()
        .next()
        .unwrap()
        .entities
        .iter()
        .map(|e| {
            let Entity::OnLayer { layer: 127, entity } = e else {
                panic!("hatch layer")
            };
            *entity.clone()
        })
        .collect()
}
fn close(a: Point, b: Point) -> bool {
    (a.x - b.x).abs() < 1e-10 && (a.y - b.y).abs() < 1e-10
}

#[test]
fn mudst_mixes_real_points_and_strokes_with_signed_row_drift() {
    let mut editor = Editor::default();
    rectangle(&mut editor, -1.0, 1.0, 1.0, 0.0);
    hatch(&mut editor, "MUDST", 1.0, 0.0);
    let geometry = geometry(&editor);
    for (y, expected) in [
        (
            0.0,
            vec![
                Entity::Line {
                    start: p(-1.0, 0.0),
                    end: p(-0.75, 0.0),
                },
                Entity::Point {
                    origin: p(-0.5, 0.0),
                },
                Entity::Point {
                    origin: p(-0.25, 0.0),
                },
                Entity::Line {
                    start: p(0.0, 0.0),
                    end: p(0.25, 0.0),
                },
                Entity::Point {
                    origin: p(0.5, 0.0),
                },
                Entity::Point {
                    origin: p(0.75, 0.0),
                },
            ],
        ),
        (
            -0.25,
            vec![
                Entity::Point {
                    origin: p(-1.0, -0.25),
                },
                Entity::Point {
                    origin: p(-0.75, -0.25),
                },
                Entity::Line {
                    start: p(-0.5, -0.25),
                    end: p(-0.25, -0.25),
                },
                Entity::Point {
                    origin: p(0.0, -0.25),
                },
                Entity::Point {
                    origin: p(0.25, -0.25),
                },
                Entity::Line {
                    start: p(0.5, -0.25),
                    end: p(0.75, -0.25),
                },
            ],
        ),
    ] {
        let row: Vec<_> = geometry
            .iter()
            .filter(|e| match e {
                Entity::Line { start, .. } => start.y == y,
                Entity::Point { origin } => origin.y == y,
                _ => panic!("unexpected hatch entity"),
            })
            .cloned()
            .collect();
        assert_eq!(row, expected);
    }
}

#[test]
fn sacncr_points_form_the_translated_diagonal_lattice_after_continuous_rows() {
    let mut editor = Editor::default();
    rectangle(&mut editor, 0.0, 1.0, 1.0, 0.0);
    hatch(&mut editor, "SACNCR", 1.0, 0.0);
    let geometry = geometry(&editor);
    let first_dot = geometry
        .iter()
        .position(|e| matches!(e, Entity::Point { .. }))
        .unwrap();
    assert!(first_dot > 0);
    assert!(geometry[..first_dot]
        .iter()
        .all(|e| matches!(e, Entity::Line { .. })));
    let mut expected = Vec::new();
    // Independently enumerate the 2D lattice from the original origin and
    // two diagonal basis vectors, rather than reuse row clipping/dash logic.
    let step = 0.09375 / std::f64::consts::SQRT_2;
    for n in -20..=20 {
        for k in -20..=20 {
            let q = p(
                0.066291261 + f64::from(k - n) * step,
                f64::from(k + n) * step,
            );
            if q.x >= 0.0 && q.x < 1.0 && q.y >= 0.0 && q.y < 1.0 {
                expected.push(q);
            }
        }
    }
    assert_eq!(geometry.len() - first_dot, expected.len());
    for e in &geometry[first_dot..] {
        let Entity::Point { origin } = e else {
            panic!("dot family contains a line")
        };
        let i = expected
            .iter()
            .position(|q| close(*q, *origin))
            .unwrap_or_else(|| panic!("unexpected dot {origin:?}"));
        expected.swap_remove(i);
    }
    assert!(expected.is_empty());
}

#[test]
fn dot_patterns_scale_rotate_and_roundtrip_without_changing_entity_types_or_order() {
    for pattern in ["MUDST", "SACNCR"] {
        let mut base = Editor::default();
        rectangle(&mut base, -1.13, 2.17, 1.0, 0.0);
        hatch(&mut base, pattern, 1.0, 0.0);
        let expected = geometry(&base);
        let mut rotated = Editor::default();
        rectangle(&mut rotated, -1.13, 2.17, 2.0, 30.0);
        hatch(&mut rotated, pattern, 2.0, 390.0);
        let actual = geometry(&rotated);
        assert_eq!(actual.len(), expected.len(), "{pattern}");
        for (a, b) in actual.iter().zip(&expected) {
            match (a, b) {
                (Entity::Point { origin: a }, Entity::Point { origin: b }) => {
                    assert!(close(*a, transform(*b, 2.0, 30.0)))
                }
                (Entity::Line { start: a, end: b }, Entity::Line { start: c, end: d }) => {
                    assert!(
                        close(*a, transform(*c, 2.0, 30.0)) && close(*b, transform(*d, 2.0, 30.0))
                    );
                }
                _ => panic!("{pattern}: changed primitive type"),
            }
        }
        let reopened = acad_dwg::parse(&acad_dwg::write(rotated.drawing()).unwrap()).unwrap();
        assert_eq!(reopened.items, rotated.drawing().items);
    }
}

#[test]
fn dots_stay_outside_holes_keep_global_phase_and_undo_the_entire_block() {
    for pattern in ["MUDST", "SACNCR"] {
        let mut editor = Editor::default();
        inputs(&mut editor, &["CIRCLE", "0,0", "2", "CIRCLE", "0,0", "0.6"]);
        let before = editor.drawing().clone();
        hatch(&mut editor, pattern, 1.0, 0.0);
        let points: Vec<_> = geometry(&editor)
            .into_iter()
            .filter_map(|e| match e {
                Entity::Point { origin } => Some(origin),
                _ => None,
            })
            .collect();
        assert!(!points.is_empty());
        for q in &points {
            assert!(q.x.hypot(q.y) >= 0.6 - 1e-10 && q.x.hypot(q.y) <= 2.0 + 1e-10);
        }
        if pattern == "MUDST" {
            // The inner hole removes nearby dots without shifting x=0.75.
            let row: Vec<_> = points.iter().copied().filter(|q| q.y == 0.0).collect();
            assert_eq!(
                row,
                vec![
                    p(-1.5, 0.0),
                    p(-1.25, 0.0),
                    p(0.75, 0.0),
                    p(1.5, 0.0),
                    p(1.75, 0.0)
                ]
            );
        }
        editor.submit("UNDO").unwrap();
        assert_eq!(editor.drawing(), &before);
    }
}

#[test]
fn dot_budget_overflow_is_atomic_and_does_not_add_an_undo_snapshot() {
    let mut editor = Editor::default();
    rectangle(&mut editor, 0.0, 100.0, 1.0, 0.0);
    let before = editor.drawing().clone();
    let mut control = editor.clone();
    inputs(&mut editor, &["HATCH", "MUDST", "1", "0"]);
    let error = editor.submit("ALL").unwrap_err();
    assert!(error.contains("aggregate line limit"), "{error}");
    assert_eq!(editor.drawing(), &before);
    editor.cancel_command().unwrap();
    editor.submit("UNDO").unwrap();
    control.submit("UNDO").unwrap();
    assert_eq!(editor.drawing(), control.drawing());
}
