//! HATCH N/O/I island styles: HLP (`name,style`) plus Rust nesting-depth
//! geometry contracts, not native export parity.
use acad_cmd::Editor;
use acad_model::{Entity, Item, Point};

const PATTERNS: [&str; 23] = [
    "EARTH", "ESCHER", "FLEX", "GRASS", "GRATE", "HEX", "HONEY", "HOUND", "INSUL", "LINE", "MUDST",
    "NET", "NET3", "PLAST", "PLASTI", "SACNCR", "SQUARE", "STARS", "STEEL", "SWAMP", "TRANS",
    "TRIANG", "ZIGZAG",
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
fn square(editor: &mut Editor, lo: Point, hi: Point, scale: f64, angle: f64) {
    editor.submit("LINE").unwrap();
    for q in [lo, p(hi.x, lo.y), hi, p(lo.x, hi.y), lo] {
        let q = transform(q, scale, angle);
        editor.submit(&format!("{},{}", q.x, q.y)).unwrap();
    }
    editor.submit("").unwrap();
}
/// Three concentric levels: A (0..12), B (2..10), C (4..8); `levels` keeps the
/// outermost `levels` of them.
fn nested(levels: usize, scale: f64, angle: f64) -> Editor {
    let mut editor = Editor::default();
    for (lo, hi) in [(0.0, 12.0), (2.0, 10.0), (4.0, 8.0)]
        .into_iter()
        .take(levels)
    {
        square(&mut editor, p(lo, lo), p(hi, hi), scale, angle);
    }
    editor
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
        .expect("HATCH block")
        .entities
        .iter()
        .map(|entity| {
            let Entity::OnLayer { layer: 127, entity } = entity else {
                panic!("hatch layer")
            };
            entity.as_ref().clone()
        })
        .collect()
}
fn spans(editor: &Editor, y: f64) -> Vec<(f64, f64)> {
    let mut row: Vec<_> = geometry(editor)
        .into_iter()
        .filter_map(|entity| match entity {
            Entity::Line { start, end } if start.y == y && end.y == y => {
                Some((start.x.min(end.x), start.x.max(end.x)))
            }
            _ => None,
        })
        .collect();
    row.sort_by(|a, b| a.0.total_cmp(&b.0));
    row
}
fn style_editor(levels: usize, pattern: &str, style: &str) -> Editor {
    let mut editor = nested(levels, 1.0, 0.0);
    let name = if style.is_empty() {
        pattern.to_owned()
    } else {
        format!("{pattern},{style}")
    };
    hatch(&mut editor, &name, 1.0, 0.0);
    editor
}

#[test]
fn concentric_three_level_squares_distinguish_normal_outermost_and_ignore() {
    let normal = style_editor(3, "LINE", "N");
    let outer = style_editor(3, "LINE", "O");
    let ignore = style_editor(3, "LINE", "I");
    // Inside all three loops: N alternates, O keeps only the outer ring, I fills.
    assert_eq!(spans(&normal, 6.0), [(0.0, 2.0), (4.0, 8.0), (10.0, 12.0)]);
    assert_eq!(spans(&outer, 6.0), [(0.0, 2.0), (10.0, 12.0)]);
    assert_eq!(spans(&ignore, 6.0), [(0.0, 12.0)]);
    // Inside A and B only.
    assert_eq!(spans(&normal, 3.0), [(0.0, 2.0), (10.0, 12.0)]);
    assert_eq!(spans(&outer, 3.0), [(0.0, 2.0), (10.0, 12.0)]);
    assert_eq!(spans(&ignore, 3.0), [(0.0, 12.0)]);
    // Inside only A.
    for editor in [&normal, &outer, &ignore] {
        assert_eq!(spans(editor, 1.0), [(0.0, 12.0)]);
    }
}

#[test]
fn style_suffix_is_optional_case_insensitive_and_normal_is_the_existing_parity_rule() {
    for pattern in PATTERNS {
        let legacy = style_editor(3, pattern, "");
        for style in ["N", "n", " N "] {
            assert_eq!(
                geometry(&style_editor(3, pattern, style)),
                geometry(&legacy),
                "{pattern},{style}"
            );
        }
    }
}

#[test]
fn every_pattern_keeps_signed_phase_dashes_and_dots_under_outermost_and_ignore() {
    // O over A/B/C equals the parity hatch of A/B alone; I equals A alone.
    // Equality includes order, dash phase, signed row drift and dots.
    for (scale, angle) in [(1.0, 0.0), (2.0, 30.0)] {
        for pattern in PATTERNS {
            let styled = |style: &str| {
                let mut editor = nested(3, scale, angle);
                hatch(&mut editor, &format!("{pattern},{style}"), scale, angle);
                geometry(&editor)
            };
            let parity = |levels: usize| {
                let mut editor = nested(levels, scale, angle);
                hatch(&mut editor, pattern, scale, angle);
                geometry(&editor)
            };
            assert_eq!(styled("O"), parity(2), "{pattern},O {scale} {angle}");
            assert_eq!(styled("I"), parity(1), "{pattern},I {scale} {angle}");
        }
    }
}

#[test]
fn rotated_and_scaled_double_family_styles_transform_with_the_boundary() {
    for style in ["N", "O", "I"] {
        let base = {
            let mut editor = nested(3, 1.0, 0.0);
            hatch(&mut editor, &format!("NET,{style}"), 1.0, 0.0);
            geometry(&editor)
        };
        let mut rotated = nested(3, 2.0, 30.0);
        hatch(&mut rotated, &format!("NET,{style}"), 2.0, 390.0);
        let actual = geometry(&rotated);
        assert_eq!(actual.len(), base.len(), "{style}");
        for (a, b) in actual.iter().zip(&base) {
            let (Entity::Line { start, end }, Entity::Line { start: s, end: e }) = (a, b) else {
                panic!("NET draws continuous lines");
            };
            for (got, want) in [
                (start, transform(*s, 2.0, 30.0)),
                (end, transform(*e, 2.0, 30.0)),
            ] {
                assert!(
                    (got.x - want.x).abs() < 1e-9 && (got.y - want.y).abs() < 1e-9,
                    "{style}: {got:?} != {want:?}"
                );
            }
        }
    }
}

#[test]
fn disconnected_loops_each_apply_the_style_to_their_own_islands() {
    let build = || {
        let mut editor = Editor::default();
        square(&mut editor, p(0.0, 0.0), p(4.0, 4.0), 1.0, 0.0);
        square(&mut editor, p(1.0, 1.0), p(3.0, 3.0), 1.0, 0.0);
        square(&mut editor, p(10.0, 0.0), p(14.0, 4.0), 1.0, 0.0);
        square(&mut editor, p(11.0, 1.0), p(13.0, 3.0), 1.0, 0.0);
        square(&mut editor, p(20.0, 0.0), p(24.0, 4.0), 1.0, 0.0);
        editor
    };
    let mut outer = build();
    hatch(&mut outer, "LINE,O", 1.0, 0.0);
    assert_eq!(
        spans(&outer, 2.0),
        [
            (0.0, 1.0),
            (3.0, 4.0),
            (10.0, 11.0),
            (13.0, 14.0),
            (20.0, 24.0)
        ]
    );
    let mut ignore = build();
    hatch(&mut ignore, "LINE,I", 1.0, 0.0);
    assert_eq!(
        spans(&ignore, 2.0),
        [(0.0, 4.0), (10.0, 14.0), (20.0, 24.0)]
    );
}

#[test]
fn circles_and_arc_loops_nest_as_curved_islands() {
    let mut build = || {
        let mut editor = Editor::default();
        // Outer level: two ARCs closed into a circle of radius 3.
        for (start_deg, end_deg) in [(0.0, 180.0), (180.0, 360.0)] {
            editor.drawing_mut().items.push(Item::Entity(Entity::Arc {
                center: p(0.0, 0.0),
                radius: 3.0,
                start_deg,
                end_deg,
            }));
        }
        inputs(&mut editor, &["CIRCLE", "0,0", "2", "CIRCLE", "0,0", "1"]);
        editor
    };
    let styled = |style: &str, build: &mut dyn FnMut() -> Editor| {
        let mut editor = build();
        hatch(&mut editor, &format!("LINE,{style}"), 1.0, 0.0);
        spans(&editor, 0.0)
    };
    assert_eq!(
        styled("N", &mut build),
        [(-3.0, -2.0), (-1.0, 1.0), (2.0, 3.0)]
    );
    assert_eq!(styled("O", &mut build), [(-3.0, -2.0), (2.0, 3.0)]);
    assert_eq!(styled("I", &mut build), [(-3.0, 3.0)]);
    // Off-axis rows stay inside the curved annulus for O.
    let mut outer = build();
    hatch(&mut outer, "LINE,O", 1.0, 0.0);
    for entity in geometry(&outer) {
        let Entity::Line { start, end } = entity else {
            panic!("LINE draws lines");
        };
        let middle = p((start.x + end.x) / 2.0, (start.y + end.y) / 2.0);
        let radius = middle.x.hypot(middle.y);
        assert!((2.0 - 1e-9..=3.0 + 1e-9).contains(&radius), "{middle:?}");
    }
}

#[test]
fn invalid_styles_are_rejected_before_prompts_without_changing_the_drawing() {
    let mut editor = nested(3, 1.0, 0.0);
    let before = editor.drawing().clone();
    for input in ["LINE,X", "LINE,", "LINE,OI", "U,X"] {
        editor.submit("HATCH").unwrap();
        assert!(editor.submit(input).is_err(), "{input}");
        assert_eq!(editor.prompt(), "Command", "{input}");
        assert_eq!(editor.drawing(), &before);
    }
    editor.submit("HATCH").unwrap();
    editor.submit("line , o").unwrap();
    assert_eq!(editor.prompt(), "HATCH: scale for pattern {1}");
}

#[test]
fn styled_hatch_is_one_undo_step_and_survives_dwg_round_trip() {
    for style in ["N", "O", "I"] {
        let mut editor = nested(3, 1.0, 0.0);
        let before = editor.drawing().clone();
        hatch(&mut editor, &format!("SACNCR,{style}"), 1.0, 0.0);
        assert_eq!(editor.drawing().blocks().count(), 1);
        let saved = acad_dwg::parse(&acad_dwg::write(editor.drawing()).unwrap()).unwrap();
        assert_eq!(saved.items, editor.drawing().items);
        editor.submit("UNDO").unwrap();
        assert_eq!(editor.drawing(), &before);
    }
}

#[test]
fn ignore_style_budget_exhaustion_is_atomic_then_retry_and_cancel_work() {
    let mut original = Editor::default();
    square(&mut original, p(0.0, 0.0), p(8500.0, 8500.0), 1.0, 0.0);
    square(&mut original, p(1.0, 1.0), p(8499.0, 8499.0), 1.0, 0.0);
    square(&mut original, p(9000.0, 0.0), p(9001.0, 1.0), 1.0, 0.0);
    let before = original.drawing().clone();
    let mut previous = original.clone();
    previous.submit("UNDO").unwrap();

    let mut editor = original.clone();
    inputs(&mut editor, &["HATCH", "NET,I", "1", "0"]);
    let error = editor.submit("ALL").unwrap_err();
    assert!(error.contains("limit"), "{error}");
    assert_eq!(editor.drawing(), &before);
    // Retry at the same selection prompt with only the small square.
    inputs(&mut editor, &["W", "8999,-1", "9002,2"]);
    assert_eq!(editor.prompt(), "Command");
    assert_eq!(editor.drawing().blocks().count(), 1);
    editor.submit("UNDO").unwrap();
    assert_eq!(editor.drawing(), &before);
    editor.submit("UNDO").unwrap();
    assert_eq!(editor.drawing(), previous.drawing());

    // Cancel after a failure leaves no state or undo snapshot behind.
    let mut editor = original.clone();
    inputs(&mut editor, &["HATCH", "NET,O", "1", "0"]);
    assert!(editor.submit("ALL").is_err());
    editor.cancel_command().unwrap();
    assert_eq!(editor.prompt(), "Command");
    assert_eq!(editor.drawing(), &before);
    editor.submit("UNDO").unwrap();
    assert_eq!(editor.drawing(), previous.drawing());
}

fn polygon(editor: &mut Editor, points: &[Point]) {
    editor.submit("LINE").unwrap();
    for q in points.iter().chain(points.first()) {
        editor.submit(&format!("{},{}", q.x, q.y)).unwrap();
    }
    editor.submit("").unwrap();
}
fn arc(editor: &mut Editor, center: Point, radius: f64, start_deg: f64, end_deg: f64) {
    editor.drawing_mut().items.push(Item::Entity(Entity::Arc {
        center,
        radius,
        start_deg,
        end_deg,
    }));
}
fn styled_spans(build: impl Fn() -> Editor, style: &str, y: f64) -> Vec<(f64, f64)> {
    let mut editor = build();
    hatch(&mut editor, &format!("LINE,{style}"), 1.0, 0.0);
    spans(&editor, y)
}
fn with_outer(island: impl Fn(&mut Editor)) -> impl Fn() -> Editor {
    move || {
        let mut editor = Editor::default();
        square(&mut editor, p(-10.0, -10.0), p(14.0, 14.0), 1.0, 0.0);
        island(&mut editor);
        editor
    }
}

#[test]
fn island_local_minimum_vertex_on_a_row_counts_twice_for_outermost_and_ignore() {
    // The V-notch tip (2,2) is a local minimum of the island boundary.
    let notch = |editor: &mut Editor| {
        polygon(
            editor,
            &[
                p(0.0, 0.0),
                p(4.0, 0.0),
                p(4.0, 4.0),
                p(2.0, 2.0),
                p(0.0, 4.0),
            ],
        )
    };
    let build = with_outer(notch);
    assert_eq!(styled_spans(&build, "O", 2.0), [(-10.0, 0.0), (4.0, 14.0)]);
    assert_eq!(styled_spans(&build, "I", 2.0), [(-10.0, 14.0)]);
    let alone = || {
        let mut editor = Editor::default();
        notch(&mut editor);
        editor
    };
    // Both halves are hatched; touching the tip does not split the stroke.
    for style in ["O", "I"] {
        assert_eq!(styled_spans(alone, style, 2.0), [(0.0, 4.0)]);
    }
}

#[test]
fn island_local_maximum_vertex_on_a_row_does_not_cross() {
    // Mirrored notch: the tip (2,2) is a local maximum of its two edges.
    let build = with_outer(|editor| {
        polygon(
            editor,
            &[
                p(0.0, 4.0),
                p(4.0, 4.0),
                p(4.0, 0.0),
                p(2.0, 2.0),
                p(0.0, 0.0),
            ],
        )
    });
    assert_eq!(styled_spans(&build, "O", 2.0), [(-10.0, 0.0), (4.0, 14.0)]);
    assert_eq!(styled_spans(&build, "I", 2.0), [(-10.0, 14.0)]);
}

#[test]
fn arc_and_line_extremum_junctions_do_not_hatch_outside_the_region() {
    // Upper semicircle closed above by lines: the region only touches y=0 at
    // the two arc/line junctions.
    let cap = |editor: &mut Editor| {
        arc(editor, p(0.0, 0.0), 1.0, 0.0, 180.0);
        editor.submit("LINE").unwrap();
        for q in ["-1,0", "-2,2", "2,2", "1,0", ""] {
            editor.submit(q).unwrap();
        }
    };
    let alone = || {
        let mut editor = Editor::default();
        cap(&mut editor);
        editor
    };
    for style in ["O", "I"] {
        assert_eq!(styled_spans(alone, style, 0.0), []);
        assert_eq!(styled_spans(alone, style, 1.5), [(-1.75, 1.75)]);
    }
    let build = with_outer(cap);
    for style in ["O", "I"] {
        assert_eq!(styled_spans(&build, style, 0.0), [(-10.0, 14.0)]);
    }
}

#[test]
fn arc_and_line_pass_through_junctions_cross_once() {
    // A quarter arc continues monotonically into a vertical line at (2,0).
    let island = |editor: &mut Editor| {
        arc(editor, p(0.0, 0.0), 2.0, 0.0, 90.0);
        editor.submit("LINE").unwrap();
        for q in ["0,2", "-2,2", "-2,-2", "2,-2", "2,0", ""] {
            editor.submit(q).unwrap();
        }
    };
    let build = with_outer(island);
    assert_eq!(styled_spans(&build, "O", 0.0), [(-10.0, -2.0), (2.0, 14.0)]);
    assert_eq!(styled_spans(&build, "I", 0.0), [(-10.0, 14.0)]);
    let root = 3.0_f64.sqrt();
    assert_eq!(
        styled_spans(&build, "O", 1.0),
        [(-10.0, -2.0), (root, 14.0)]
    );
}

#[test]
fn rotated_notch_rows_through_the_vertex_match_the_transformed_angle_zero_result() {
    let notch = [
        p(0.0, 0.0),
        p(4.0, 0.0),
        p(4.0, 4.0),
        p(2.0, 2.0),
        p(0.0, 4.0),
    ];
    let scene = |angle: f64| {
        let mut editor = Editor::default();
        square(&mut editor, p(-10.0, -10.0), p(14.0, 14.0), 1.0, angle);
        let points: Vec<_> = notch.iter().map(|q| transform(*q, 1.0, angle)).collect();
        polygon(&mut editor, &points);
        editor
    };
    for style in ["O", "I"] {
        let mut base = scene(0.0);
        hatch(&mut base, &format!("LINE,{style}"), 1.0, 0.0);
        // The unrotated row through the notch vertex keeps its O/I spans.
        let expected_row: &[(f64, f64)] = if style == "O" {
            &[(-10.0, 0.0), (4.0, 14.0)]
        } else {
            &[(-10.0, 14.0)]
        };
        assert_eq!(spans(&base, 2.0), expected_row);
        let mut rotated = scene(30.0);
        hatch(&mut rotated, &format!("LINE,{style}"), 1.0, 30.0);
        let expected = geometry(&base);
        let actual = geometry(&rotated);
        assert_eq!(actual.len(), expected.len(), "{style}");
        for (a, b) in actual.iter().zip(&expected) {
            let (Entity::Line { start, end }, Entity::Line { start: s, end: e }) = (a, b) else {
                panic!("LINE draws continuous lines");
            };
            for (got, want) in [
                (start, transform(*s, 1.0, 30.0)),
                (end, transform(*e, 1.0, 30.0)),
            ] {
                assert!(
                    (got.x - want.x).abs() < 1e-9 && (got.y - want.y).abs() < 1e-9,
                    "{style}: {got:?} != {want:?}"
                );
            }
        }
    }
}

#[test]
fn odd_crossings_from_a_near_coincident_arc_line_junction_fail_atomically() {
    let mut editor = Editor::default();
    // The arc ends at (1,0); the closing line ends 5e-9 above it, inside the
    // 1e-8 loop tolerance but outside the 1e-10 row snap.
    arc(&mut editor, p(0.0, 0.0), 1.0, 0.0, 180.0);
    inputs(
        &mut editor,
        &["LINE", "-1,0", "-2,2", "2,2", "1,0.000000005", ""],
    );
    let before = editor.drawing().clone();
    let mut previous = editor.clone();
    previous.submit("UNDO").unwrap();
    for style in ["O", "I"] {
        inputs(&mut editor, &["HATCH", &format!("LINE,{style}"), "1", "0"]);
        let error = editor.submit("ALL").unwrap_err();
        assert!(error.contains("inconsistent"), "{style}: {error}");
        assert_eq!(editor.drawing(), &before);
        // The selection prompt stays active for a retry.
        assert_eq!(editor.prompt(), "HATCH: select objects on Window or Last");
        editor.cancel_command().unwrap();
        assert_eq!(editor.prompt(), "Command");
        assert_eq!(editor.drawing(), &before);
    }
    editor.submit("UNDO").unwrap();
    assert_eq!(editor.drawing(), previous.drawing());
}
