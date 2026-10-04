//! PAT definition contracts; these are not original-program export fixtures.
use acad_cmd::Editor;
use acad_model::{Entity, Point};

const PATTERNS: &[&str] = &[
    "EARTH", "ESCHER", "FLEX", "GRASS", "HEX", "HONEY", "HOUND", "INSUL", "SQUARE", "STARS",
    "SWAMP", "TRANS", "TRIANG", "ZIGZAG",
];
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
fn strokes(editor: &Editor) -> Vec<(Point, Point)> {
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
            let Entity::Line { start, end } = entity.as_ref() else {
                panic!("hatch stroke")
            };
            (*start, *end)
        })
        .collect()
}
fn close(a: Point, b: Point) -> bool {
    (a.x - b.x).abs() < 1e-10 && (a.y - b.y).abs() < 1e-10
}
fn assert_strokes(actual: &[(Point, Point)], expected: &[(Point, Point)]) {
    assert_eq!(actual.len(), expected.len());
    for (a, b) in actual.iter().zip(expected) {
        assert!(close(a.0, b.0) && close(a.1, b.1), "{a:?} != {b:?}");
    }
}
fn row(lines: &[(Point, Point)], y: f64) -> Vec<(Point, Point)> {
    lines
        .iter()
        .copied()
        .filter(|(a, b)| (a.y - y).abs() < 1e-10 && (b.y - y).abs() < 1e-10)
        .collect()
}

#[test]
fn square_emits_dashes_in_family_and_sweep_order_without_gap_strokes() {
    let mut editor = Editor::default();
    rectangle(&mut editor, 0.0, 0.5, 1.0, 0.0);
    hatch(&mut editor, "SQUARE", 1.0, 0.0);
    let mut expected = Vec::new();
    for y in [0.25, 0.375, 0.125, 0.0] {
        for x in [0.0, 0.25] {
            expected.push((p(x, y), p(x + 0.125, y)));
        }
    }
    for x in [0.25, 0.125, 0.375, 0.5] {
        for y in [0.0, 0.25] {
            expected.push((p(x, y), p(x, y + 0.125)));
        }
    }
    assert_strokes(&strokes(&editor), &expected);
}

/// Row order follows the original's truncated start row (checked against the
/// in-tree original in `acad-oracle/tests/hatch_user.rs`).
#[test]
fn trans_and_insul_mix_continuous_rows_and_shifted_dash_rows() {
    for (pattern, continuous, dashed) in [
        (
            "TRANS",
            vec![0.5, 0.75, 0.25, 0.0],
            vec![0.375, 0.625, 0.875, 0.125],
        ),
        (
            "INSUL",
            vec![0.375, 0.75, 0.0],
            vec![0.5, 0.875, 0.125, 0.25, 0.625],
        ),
    ] {
        let mut editor = Editor::default();
        rectangle(&mut editor, 0.0, 1.0, 1.0, 0.0);
        hatch(&mut editor, pattern, 1.0, 0.0);
        let mut expected: Vec<_> = continuous
            .into_iter()
            .map(|y| (p(0.0, y), p(1.0, y)))
            .collect();
        for y in dashed {
            for x in [0.0, 0.25, 0.5, 0.75] {
                expected.push((p(x, y), p(x + 0.125, y)));
            }
        }
        assert_strokes(&strokes(&editor), &expected);
    }
}

#[test]
fn earth_drift_uses_signed_row_indices_across_negative_world_coordinates() {
    let mut editor = Editor::default();
    rectangle(&mut editor, -0.5, 0.5, 1.0, 0.0);
    hatch(&mut editor, "EARTH", 1.0, 0.0);
    let lines = strokes(&editor);
    assert_strokes(
        &row(&lines, 0.0),
        &[(p(-0.5, 0.0), p(-0.25, 0.0)), (p(0.0, 0.0), p(0.25, 0.0))],
    );
    assert_strokes(
        &row(&lines, -0.25),
        &[
            (p(-0.25, -0.25), p(0.0, -0.25)),
            (p(0.25, -0.25), p(0.5, -0.25)),
        ],
    );
    assert_strokes(
        &row(&lines, 0.09375),
        &[
            (p(-0.5, 0.09375), p(-0.25, 0.09375)),
            (p(0.0, 0.09375), p(0.25, 0.09375)),
        ],
    );
}

#[test]
fn dash_phase_continues_through_a_circular_hole_and_clips_partial_strokes() {
    let mut editor = Editor::default();
    inputs(&mut editor, &["CIRCLE", "0,0", "1", "CIRCLE", "0,0", "0.3"]);
    hatch(&mut editor, "SQUARE", 1.0, 0.0);
    assert_strokes(
        &row(&strokes(&editor), 0.0),
        &[
            (p(-1.0, 0.0), p(-0.875, 0.0)),
            (p(-0.75, 0.0), p(-0.625, 0.0)),
            (p(-0.5, 0.0), p(-0.375, 0.0)),
            (p(0.3, 0.0), p(0.375, 0.0)),
            (p(0.5, 0.0), p(0.625, 0.0)),
            (p(0.75, 0.0), p(0.875, 0.0)),
        ],
    );
}

#[test]
fn zigzag_uses_the_translated_vertical_origin_and_row_drift() {
    let mut editor = Editor::default();
    rectangle(&mut editor, 0.0, 0.5, 1.0, 0.0);
    hatch(&mut editor, "ZIGZAG", 1.0, 0.0);
    let lines = strokes(&editor);
    assert_strokes(
        &row(&lines, 0.125),
        &[
            (p(0.125, 0.125), p(0.25, 0.125)),
            (p(0.375, 0.125), p(0.5, 0.125)),
        ],
    );
    let vertical = |x: f64| {
        lines
            .iter()
            .copied()
            .filter(|(a, b)| (a.x - x).abs() < 1e-10 && (b.x - x).abs() < 1e-10)
            .collect::<Vec<_>>()
    };
    assert_strokes(
        &vertical(0.125),
        &[
            (p(0.125, 0.0), p(0.125, 0.125)),
            (p(0.125, 0.25), p(0.125, 0.375)),
        ],
    );
    assert_strokes(
        &vertical(0.25),
        &[
            (p(0.25, 0.125), p(0.25, 0.25)),
            (p(0.25, 0.375), p(0.25, 0.5)),
        ],
    );
}

#[test]
fn all_dashed_patterns_scale_rotate_and_roundtrip_with_complete_ordered_geometry() {
    for pattern in PATTERNS {
        let mut base = Editor::default();
        rectangle(&mut base, -1.13, 2.17, 1.0, 0.0);
        hatch(&mut base, pattern, 1.0, 0.0);
        let expected: Vec<_> = strokes(&base)
            .into_iter()
            .map(|(a, b)| (transform(a, 2.0, 30.0), transform(b, 2.0, 30.0)))
            .collect();
        assert!(!expected.is_empty(), "{pattern}");
        let mut rotated = Editor::default();
        rectangle(&mut rotated, -1.13, 2.17, 2.0, 30.0);
        hatch(&mut rotated, pattern, 2.0, 390.0);
        assert_strokes(&strokes(&rotated), &expected);
        let reopened = acad_dwg::parse(&acad_dwg::write(rotated.drawing()).unwrap()).unwrap();
        assert_eq!(reopened.items, rotated.drawing().items, "{pattern}");
    }
}

#[test]
fn all_dashed_patterns_preserve_annular_holes_and_restore_the_drawing_on_undo() {
    for pattern in PATTERNS {
        let mut editor = Editor::default();
        inputs(&mut editor, &["CIRCLE", "0,0", "2", "CIRCLE", "0,0", "1"]);
        let before = editor.drawing().clone();
        hatch(&mut editor, pattern, 1.0, 0.0);
        for (a, b) in strokes(&editor) {
            for q in [a, b] {
                assert!(
                    q.x.hypot(q.y) <= 2.0 + 1e-10,
                    "{pattern}: outside outer circle"
                );
                assert!(q.x.hypot(q.y) >= 1.0 - 1e-10, "{pattern}: inside hole");
            }
            assert!(
                ((a.x + b.x) / 2.0).hypot((a.y + b.y) / 2.0) >= 1.0 - 1e-10,
                "{pattern}: hole crossed"
            );
        }
        editor.submit("UNDO").unwrap();
        assert_eq!(editor.drawing(), &before);
    }
}

#[test]
fn overflow_counts_dash_strokes_and_keeps_drawing_and_undo_atomic() {
    let mut editor = Editor::default();
    rectangle(&mut editor, 0.0, 80.0, 1.0, 0.0);
    let before = editor.drawing().clone();
    let mut control = editor.clone();
    inputs(&mut editor, &["HATCH", "SQUARE", "1", "0"]);
    let error = editor.submit("ALL").unwrap_err();
    assert!(error.contains("aggregate line limit"), "{error}");
    assert_eq!(editor.drawing(), &before);
    editor.cancel_command().unwrap();
    editor.submit("UNDO").unwrap();
    control.submit("UNDO").unwrap();
    assert_eq!(editor.drawing(), control.drawing());
}

#[test]
fn unrepresentable_dash_indices_and_excessive_cycle_work_fail_without_geometry() {
    for (lo, hi, expected) in [
        (1e16, 1e16 + 16.0, "cycle indices"),
        (0.0, 30000.0, "cycle count"),
    ] {
        let mut editor = Editor::default();
        // A thin rectangle avoids the independent sweep-count limit.
        inputs(
            &mut editor,
            &[
                "LINE",
                &format!("{lo},0"),
                &format!("{hi},0"),
                &format!("{hi},0.1"),
                &format!("{lo},0.1"),
                &format!("{lo},0"),
                "",
            ],
        );
        let before = editor.drawing().clone();
        // The horizontal family's dash guards run before vertical grid bounds.
        inputs(&mut editor, &["HATCH", "SQUARE", "1", "0"]);
        let error = editor.submit("ALL").unwrap_err();
        assert!(error.contains(expected), "{error}");
        assert_eq!(editor.drawing(), &before);
    }
}
