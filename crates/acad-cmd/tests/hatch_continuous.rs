//! Geometry contracts from retained PAT definitions, not native export replays.
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
fn rectangle(editor: &mut Editor, side: f64, scale: f64, angle: f64) {
    editor.submit("LINE").unwrap();
    for point in [
        p(0.0, 0.0),
        p(side, 0.0),
        p(side, side),
        p(0.0, side),
        p(0.0, 0.0),
    ] {
        let point = transform(point, scale, angle);
        editor.submit(&format!("{},{}", point.x, point.y)).unwrap();
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
fn strokes(editor: &Editor) -> Vec<(Point, Point)> {
    editor
        .drawing()
        .blocks()
        .next()
        .unwrap()
        .entities
        .iter()
        .map(|entity| {
            let Entity::OnLayer { layer: 127, entity } = entity else {
                panic!("hatch layer")
            };
            let Entity::Line { start, end } = entity.as_ref() else {
                panic!("continuous stroke")
            };
            (*start, *end)
        })
        .collect()
}
fn close(a: Point, b: Point) -> bool {
    (a.x - b.x).abs() < 1e-10 && (a.y - b.y).abs() < 1e-10
}
fn assert_ordered(actual: &[(Point, Point)], expected: &[(Point, Point)]) {
    assert_eq!(actual.len(), expected.len());
    for (a, b) in actual.iter().zip(expected) {
        assert!(close(a.0, b.0) && close(a.1, b.1), "{a:?} != {b:?}");
    }
}
/// Same strokes in the same order, either endpoint first: the original orients
/// continuous rows by ascending x or y (docs/native-hatch-user.md), which a
/// rotation does not preserve.
fn assert_ordered_undirected(actual: &[(Point, Point)], expected: &[(Point, Point)]) {
    assert_eq!(actual.len(), expected.len());
    for (a, b) in actual.iter().zip(expected) {
        assert!(
            (close(a.0, b.0) && close(a.1, b.1)) || (close(a.0, b.1) && close(a.1, b.0)),
            "{a:?} != {b:?}"
        );
    }
}
fn assert_unordered(actual: &[(Point, Point)], mut expected: Vec<(Point, Point)>) {
    assert_eq!(actual.len(), expected.len());
    for a in actual {
        let index = expected
            .iter()
            .position(|b| close(a.0, b.0) && close(a.1, b.1))
            .unwrap_or_else(|| panic!("unexpected stroke {a:?}"));
        expected.swap_remove(index);
    }
    assert!(expected.is_empty());
}
fn transform(point: Point, scale: f64, angle: f64) -> Point {
    let (s, c) = angle.to_radians().sin_cos();
    p(
        scale * (point.x * c - point.y * s),
        scale * (point.x * s + point.y * c),
    )
}

#[test]
fn grate_has_dense_horizontal_rows_then_the_vertical_family() {
    let mut editor = Editor::default();
    rectangle(&mut editor, 1.0, 1.0, 0.0);
    hatch(&mut editor, "GRATE", 1.0, 0.0);
    let mut expected = Vec::new();
    for i in (16..32).chain((0..16).rev()) {
        let y = f64::from(i) / 32.0;
        expected.push((p(0.0, y), p(1.0, y)));
    }
    for i in (1..=4).rev().chain(5..=8) {
        let x = f64::from(i) / 8.0;
        expected.push((p(x, 0.0), p(x, 1.0)));
    }
    assert_ordered(&strokes(&editor), &expected);
}

/// Each row starts at the row whose index truncates (centre - phase) /
/// spacing toward zero, as the original does (PLAST is checked against the
/// in-tree original in `acad-oracle/tests/hatch_user.rs`).
#[test]
fn plastic_rows_retain_their_distinct_origins_and_definition_order() {
    for (pattern, rows) in [
        (
            "PLAST",
            vec![
                vec![0.5, 0.75, 0.25, 0.0],
                vec![0.28125, 0.53125, 0.78125, 0.03125],
                vec![0.3125, 0.5625, 0.8125, 0.0625],
            ],
        ),
        (
            "PLASTI",
            vec![
                vec![0.5, 0.75, 0.25, 0.0],
                vec![0.28125, 0.53125, 0.78125, 0.03125],
                vec![0.3125, 0.5625, 0.8125, 0.0625],
                vec![0.40625, 0.65625, 0.90625, 0.15625],
            ],
        ),
    ] {
        let mut editor = Editor::default();
        rectangle(&mut editor, 1.0, 1.0, 0.0);
        hatch(&mut editor, pattern, 1.0, 0.0);
        let expected: Vec<_> = rows
            .into_iter()
            .flatten()
            .map(|y| (p(0.0, y), p(1.0, y)))
            .collect();
        assert_ordered(&strokes(&editor), &expected);
    }
}

#[test]
fn steel_has_two_diagonal_families_with_the_second_origin_in_world_coordinates() {
    let mut editor = Editor::default();
    rectangle(&mut editor, 1.0, 1.0, 0.0);
    hatch(&mut editor, "STEEL", 1.0, 0.0);
    // y=x+b; translating origin by (0,1/16) adds 1/16 to b,
    // whereas 1/8 perpendicular spacing adds sqrt(2)/8.
    let expected = [(-5..=5, 0.0), (-6..=5, 0.0625)]
        .into_iter()
        .flat_map(|(indices, phase)| {
            indices.map(move |n| {
                let b = f64::from(n) * std::f64::consts::SQRT_2 / 8.0 + phase;
                if b >= 0.0 {
                    (p(0.0, b), p(1.0 - b, 1.0))
                } else {
                    (p(-b, 0.0), p(1.0, 1.0 + b))
                }
            })
        })
        .collect();
    assert_unordered(&strokes(&editor), expected);
}

#[test]
fn net3_contains_all_three_slopes_with_exact_perpendicular_spacing() {
    let mut editor = Editor::default();
    rectangle(&mut editor, 1.0, 1.0, 0.0);
    hatch(&mut editor, "NET3", 1.0, 0.0);
    let mut expected: Vec<_> = (0..8)
        .map(|i| {
            let y = f64::from(i) / 8.0;
            (p(0.0, y), p(1.0, y))
        })
        .collect();
    let slope = 3.0_f64.sqrt();
    // Intersect y=m*x+b with each side of the unit square. For both
    // oblique families 1/8 normal spacing gives 1/4 between intercepts.
    for (m, indices, reverse) in [(slope, -6..=3, false), (-slope, 1..=10, true)] {
        for n in indices {
            let b = f64::from(n) / 4.0;
            let mut points: Vec<_> = [
                p(0.0, b),
                p(1.0, m + b),
                p(-b / m, 0.0),
                p((1.0 - b) / m, 1.0),
            ]
            .into_iter()
            .filter(|q| (0.0..=1.0).contains(&q.x) && (0.0..=1.0).contains(&q.y))
            .collect();
            points.sort_by(|a, b| a.x.total_cmp(&b.x));
            points.dedup_by(|a, b| close(*a, *b));
            assert_eq!(points.len(), 2);
            expected.push(if reverse {
                (points[1], points[0])
            } else {
                (points[0], points[1])
            });
        }
    }
    assert_unordered(&strokes(&editor), expected);
}

#[test]
fn all_continuous_patterns_preserve_holes_and_survive_dwg_save_and_undo() {
    for pattern in ["GRATE", "NET3", "PLAST", "PLASTI", "STEEL"] {
        let mut editor = Editor::default();
        inputs(&mut editor, &["CIRCLE", "0,0", "2", "CIRCLE", "0,0", "1"]);
        let before = editor.drawing().clone();
        hatch(&mut editor, pattern, 1.0, 0.0);
        let lines = strokes(&editor);
        assert!(!lines.is_empty());
        for (a, b) in &lines {
            for q in [a, b] {
                assert!(
                    (1.0 - 1e-10..=2.0 + 1e-10).contains(&q.x.hypot(q.y)),
                    "{pattern}: {q:?}"
                );
            }
            assert!(
                ((a.x + b.x) / 2.0).hypot((a.y + b.y) / 2.0) >= 1.0 - 1e-10,
                "{pattern}: hole crossed"
            );
        }
        let saved = acad_dwg::parse(&acad_dwg::write(editor.drawing()).unwrap()).unwrap();
        assert_eq!(saved.items, editor.drawing().items);
        editor.submit("UNDO").unwrap();
        assert_eq!(editor.drawing(), &before);
    }
}

#[test]
fn pattern_origins_rotate_and_scale_with_the_boundary() {
    for pattern in ["GRATE", "NET3", "PLAST", "PLASTI", "STEEL"] {
        let mut base = Editor::default();
        rectangle(&mut base, 1.0, 1.0, 0.0);
        hatch(&mut base, pattern, 1.0, 0.0);
        let expected: Vec<_> = strokes(&base)
            .into_iter()
            .map(|(a, b)| (transform(a, 2.0, 30.0), transform(b, 2.0, 30.0)))
            .collect();
        let mut rotated = Editor::default();
        rectangle(&mut rotated, 1.0, 2.0, 30.0);
        hatch(&mut rotated, pattern, 2.0, 390.0);
        assert_ordered_undirected(&strokes(&rotated), &expected);
    }
}

#[test]
fn multiple_families_share_the_limit_and_fail_without_partial_blocks_or_undo() {
    for pattern in ["GRATE", "NET3", "PLAST", "PLASTI", "STEEL"] {
        let mut editor = Editor::default();
        rectangle(&mut editor, 8500.0, 1.0, 0.0);
        let mut control = editor.clone();
        let before = editor.drawing().clone();
        inputs(&mut editor, &["HATCH", pattern, "1", "0"]);
        let error = editor.submit("ALL").unwrap_err();
        if pattern != "GRATE" {
            assert!(error.contains("aggregate line limit"), "{pattern}: {error}");
        }
        assert_eq!(editor.drawing(), &before);
        editor.cancel_command().unwrap();
        editor.submit("UNDO").unwrap();
        control.submit("UNDO").unwrap();
        assert_eq!(editor.drawing(), control.drawing());
    }
}
