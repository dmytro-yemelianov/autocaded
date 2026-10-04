//! Native Rust contracts for retained HLP alternatives, beyond the 3P exports.
use acad_cmd::Editor;
use acad_model::{Entity, Point};
fn p(x: f64, y: f64) -> Point {
    Point { x, y }
}
fn run(e: &mut Editor, inputs: &[&str]) {
    for input in inputs {
        e.submit(input).unwrap();
    }
}
fn bare(e: &Entity) -> &Entity {
    match e {
        Entity::OnLayer { entity, .. } => bare(entity),
        other => other,
    }
}
fn last(e: &Editor) -> &Entity {
    bare(e.drawing().entities().last().unwrap())
}
fn near(a: f64, b: f64) {
    assert!((a - b).abs() <= 1e-12 * b.abs().max(1.0), "{a} != {b}");
}
fn arc(e: &Editor, c: Point, r: f64, a: f64, b: f64) {
    let Entity::Arc {
        center,
        radius,
        start_deg,
        end_deg,
    } = last(e)
    else {
        panic!("expected ARC");
    };
    near(center.x, c.x);
    near(center.y, c.y);
    near(*radius / r, 1.0);
    near(*start_deg, a);
    near(*end_deg, b);
}
#[test]
fn center_first_center_end_angle_and_chord_describe_the_same_quarter_circle() {
    for input in [
        vec!["ARC", "5,0", "C", "0,0", "0,10"], // direction is projected
        vec!["ARC", "C", "0,0", "@5,0", "0,5"],
        vec!["ARC", "5,0", "C", "0,0", "A", "90"],
        vec!["ARC", "5,0", "C", "0,0", "L", "7.0710678118654755"],
        vec!["ARC", "5,0", "E", "0,5", "R", "5"],
        vec!["ARC", "5,0", "E", "@-5,5", "A", "90"],
        vec!["ARC", "5,0", "E", "0,5", "D", "90"],
        vec!["ARC", "5,0", "E", "0,5", "D", "@0,1"],
        vec!["ARC", "5,0", "E", "0,5", "D", "@1<90"],
    ] {
        let mut e = Editor::default();
        run(&mut e, &input);
        arc(&e, p(0.0, 0.0), 5.0, 0.0, 90.0);
        assert_eq!(e.prompt(), "Command");
    }
}
#[test]
fn signed_angles_radius_and_chord_preserve_major_and_clockwise_sweeps() {
    for (inputs, c, a, b) in [
        (
            vec!["ARC", "5,0", "C", "0,0", "A", "-90"],
            p(0.0, 0.0),
            270.0,
            0.0,
        ),
        (
            vec!["ARC", "5,0", "C", "0,0", "L", "-7.0710678118654755"],
            p(0.0, 0.0),
            0.0,
            270.0,
        ),
        (
            vec!["ARC", "5,0", "E", "0,5", "R", "-5"],
            p(5.0, 5.0),
            270.0,
            180.0,
        ),
        (
            vec!["ARC", "5,0", "E", "0,-5", "A", "-90"],
            p(0.0, 0.0),
            270.0,
            0.0,
        ),
        (
            vec!["ARC", "5,0", "E", "0,-5", "D", "-90"],
            p(0.0, 0.0),
            270.0,
            0.0,
        ),
        (
            vec!["ARC", "5,0", "E", "0,5", "A", "270"],
            p(5.0, 5.0),
            270.0,
            180.0,
        ),
    ] {
        let mut e = Editor::default();
        run(&mut e, &inputs);
        arc(&e, c, 5.0, a, b);
    }
    for mode in ["R", "A"] {
        let mut e = Editor::default();
        run(
            &mut e,
            &[
                "ARC",
                "5,0",
                "E",
                "-5,0",
                mode,
                if mode == "R" { "5" } else { "180" },
            ],
        );
        arc(&e, p(0.0, 0.0), 5.0, 0.0, 180.0);
    }
    let mut e = Editor::default();
    run(&mut e, &["ARC", "0,-5", "C", "0,0", "A", "180"]);
    arc(&e, p(0.0, 0.0), 5.0, 270.0, 90.0);
}
#[test]
fn line_and_arc_continue_at_the_exact_committed_endpoint_and_tangent() {
    let mut e = Editor::default();
    run(&mut e, &["LINE", "0,0", "5,0", "", "ARC"]);
    e.submit_return("").unwrap();
    e.submit("10,5").unwrap();
    arc(&e, p(5.0, 5.0), 5.0, 270.0, 0.0);
    run(&mut e, &["ARC", "", "5,10"]);
    arc(&e, p(5.0, 5.0), 5.0, 0.0, 90.0);
    run(&mut e, &["LINE", ""]);
    e.submit_return("@2,3").unwrap();
    e.submit("").unwrap();
    assert_eq!(
        last(&e),
        &Entity::Line {
            start: p(5.0, 10.0),
            end: p(7.0, 13.0)
        }
    );
    // Explicit clockwise 3P must remember the user's end, not the CCW record end.
    let mut e = Editor::default();
    run(&mut e, &["ARC", "5,0", "0,-5", "-5,0", "ARC", "", "0,5"]);
    arc(&e, p(0.0, 0.0), 5.0, 90.0, 180.0);
    run(&mut e, &["LINE", "", "2,5", ""]);
    assert_eq!(
        last(&e),
        &Entity::Line {
            start: p(0.0, 5.0),
            end: p(2.0, 5.0)
        }
    );
}

#[test]
fn closing_edges_supply_continuation_and_failed_tangent_arcs_are_retryable() {
    let mut e = Editor::default();
    run(
        &mut e,
        &["LINE", "0,0", "5,0", "5,5", "0,5", "C", "ARC", ""],
    );
    let before = e.drawing().clone();
    for bad in ["0,0", "0,-5", "NaN,0"] {
        assert!(e.submit(bad).is_err());
        assert_eq!(e.prompt(), "ARC: continuation end point");
        assert_eq!(e.drawing(), &before);
    }
    e.submit("5,-5").unwrap();
    arc(&e, p(5.0, 0.0), 5.0, 180.0, 270.0);
    e.submit("UNDO").unwrap();
    assert_eq!(e.drawing(), &before);
    run(&mut e, &["ARC", "", "-5,-5"]);
    arc(&e, p(-5.0, 0.0), 5.0, 270.0, 0.0);
}
#[test]
fn undo_restores_continuation_and_unrelated_creation_reports_and_headers_preserve_it() {
    let mut e = Editor::default();
    run(
        &mut e,
        &["LINE", "0,0", "5,0", "", "ARC", "", "10,5", "UNDO"],
    );
    run(
        &mut e,
        &[
            "POINT", "9,9", "CIRCLE", "20,20", "2", "STATUS", "LIST", "ALL", "LAYER", "4", "ARC",
            "", "10,-5",
        ],
    );
    arc(&e, p(5.0, -5.0), 5.0, 0.0, 90.0);
    assert!(matches!(
        e.drawing().entities().last(),
        Some(Entity::OnLayer { layer: 4, .. })
    ));
    run(&mut e, &["UNDO", "LINE", ""]);
    run(&mut e, &["6,0", ""]);
    assert_eq!(
        last(&e),
        &Entity::Line {
            start: p(5.0, 0.0),
            end: p(6.0, 0.0)
        }
    );
}
#[test]
fn changed_erased_and_zero_length_sources_do_not_continue_but_undo_recovers_history() {
    for edit in [
        vec!["ERASE", "LAST"],
        vec!["MOVE", "0,1", "", "LAST"],
        vec!["CHANGE", "LAST", "L", "2"],
    ] {
        let mut e = Editor::default();
        run(&mut e, &["LINE", "0,0", "5,0", ""]);
        run(&mut e, &edit);
        e.submit("ARC").unwrap();
        assert!(e.submit("").is_err());
        e.cancel_command().unwrap();
        run(&mut e, &["UNDO", "ARC", "", "10,5"]);
        arc(&e, p(5.0, 5.0), 5.0, 270.0, 0.0);
    }
    let mut e = Editor::default();
    run(
        &mut e,
        &["LINE", "0,0", "5,0", "", "LINE", "2,2", "2,2", "", "ARC"],
    );
    assert!(e.submit("").is_err());
    e.cancel_command().unwrap();
    run(&mut e, &["UNDO", "ARC", "", "10,5"]);
    arc(&e, p(5.0, 5.0), 5.0, 270.0, 0.0);
    let mut opened = Editor::new(e.drawing().clone());
    opened.submit("ARC").unwrap();
    assert!(opened.submit("").is_err());
}
#[test]
fn invalid_values_keep_the_drawing_prompt_and_undo_for_retry() {
    for (setup, bad, valid) in [
        (
            vec!["ARC", "5,0", "C", "0,0"],
            vec!["0,0", "10,0", "NaN,0"],
            "0,5",
        ),
        (
            vec!["ARC", "5,0", "C", "0,0", "A"],
            vec!["0", "360", "-360", "NaN", "inf", "1e-3230"],
            "90",
        ),
        (
            vec!["ARC", "5,0", "C", "0,0", "L"],
            vec!["0", "11", "-11", "NaN"],
            "5",
        ),
        (
            vec!["ARC", "5,0", "E", "0,5", "R"],
            vec!["0", "1", "-1", "inf", "1e308"],
            "5",
        ),
        (
            vec!["ARC", "5,0", "E", "0,5", "A"],
            vec!["0", "360", "-360", "NaN"],
            "90",
        ),
        (
            vec!["ARC", "5,0", "E", "0,5", "D"],
            vec!["135", "315", "5,0", "NaN", "@1.7e308,1.7e308"],
            "90",
        ),
        (
            vec!["ARC", "0,0", "1,0"],
            vec!["0,0", "1,0", "2,0", "inf,0"],
            "0,1",
        ),
    ] {
        let mut e = Editor::default();
        run(&mut e, &["POINT", "9,9"]);
        run(&mut e, &setup);
        let before = e.drawing().clone();
        let prompt = e.prompt().to_owned();
        for bad in bad {
            assert!(e.submit(bad).is_err(), "{setup:?}: {bad}");
            assert_eq!(e.prompt(), prompt);
            assert_eq!(e.drawing(), &before);
        }
        e.submit(valid).unwrap();
        run(&mut e, &["UNDO"]);
        assert_eq!(e.drawing(), &before);
        run(&mut e, &["UNDO"]);
        assert!(e.drawing().items.is_empty());
    }
}
#[test]
fn invalid_intermediate_points_and_cancel_do_not_overwrite_history() {
    for setup in [
        vec!["ARC", "5,0", "C"],
        vec!["ARC", "C", "5,0"],
        vec!["ARC", "5,0", "E"],
        vec!["ARC", "5,0"],
    ] {
        let mut e = Editor::default();
        run(&mut e, &["LINE", "0,0", "5,0", ""]);
        run(&mut e, &setup);
        let before = e.drawing().clone();
        let prompt = e.prompt().to_owned();
        assert!(e.submit("5,0").is_err());
        assert_eq!(e.prompt(), prompt);
        assert_eq!(e.drawing(), &before);
        e.cancel_command().unwrap();
        run(&mut e, &["ARC", "", "10,5"]);
        arc(&e, p(5.0, 5.0), 5.0, 270.0, 0.0);
    }
    let mut e = Editor::default();
    run(&mut e, &["LINE", "1e308,0"]);
    assert!(e.submit("@1e308,0").is_err());
    assert!(e.drawing().items.is_empty());
    e.cancel_command().unwrap();
    run(&mut e, &["ARC", "1e308,0"]);
    assert!(e.submit("@1e308,0").is_err());
    assert!(e.drawing().items.is_empty());
}
#[test]
fn mouse_arc_points_snap_without_ortho_while_continued_lines_keep_their_anchor() {
    let mut e = Editor::default();
    run(&mut e, &["SNAP", "1", "ORTHO", "ON", "ARC", "C"]);
    for q in [p(0.1, 0.1), p(5.1, 0.1), p(0.1, 5.1)] {
        assert!(e.accepts_mouse_point());
        e.submit_mouse_point(q).unwrap();
    }
    arc(&e, p(0.0, 0.0), 5.0, 0.0, 90.0);
    run(&mut e, &["ARC", ""]);
    e.submit_mouse_point(p(-5.1, 0.1)).unwrap();
    arc(&e, p(0.0, 0.0), 5.0, 90.0, 180.0);
    // Numeric-only prompts must not turn a mouse point into a number.
    e.cancel_command().unwrap();
    run(&mut e, &["ARC", "5,0", "E", "0,5", "R"]);
    assert!(!e.accepts_mouse_point());
    assert!(e.submit_mouse_point(p(5.0, 0.0)).is_err());
    e.cancel_command().unwrap();
    run(&mut e, &["LINE", ""]);
    let q = e.constrain_mouse_point(p(-2.1, 2.1)).unwrap();
    assert_eq!(q.y, 0.0);
    e.submit_mouse_point(p(-2.1, 2.1)).unwrap();
    assert_eq!(
        last(&e),
        &Entity::Line {
            start: p(-5.0, 0.0),
            end: p(-2.0, 0.0)
        }
    );
}
#[test]
fn tiny_large_and_translated_arcs_stay_finite_and_round_trip_without_changing_view() {
    for size in [1e-200, 1.0, 1e150, 1e308] {
        let mut e = Editor::default();
        e.submit("ARC").unwrap();
        for q in [p(size, 0.0), p(0.0, size), p(-size, 0.0)] {
            e.submit_mouse_point(q).unwrap();
        }
        arc(&e, p(0.0, 0.0), size, 0.0, 180.0);
        let Entity::Arc { center, .. } = last(&e) else {
            unreachable!()
        };
        assert!(center.x.abs() / size < 1e-12 && center.y.abs() / size < 1e-12);
    }
    let mut e = Editor::default();
    run(&mut e, &["LAYER", "4", "PAN", "@10,20", ""]);
    let view = e.drawing().header.view;
    let limits = e.drawing().header.limits;
    run(
        &mut e,
        &[
            "ARC",
            "1000000000000002,1000000000000000",
            "C",
            "1000000000000000,1000000000000000",
            "A",
            "90",
        ],
    );
    arc(&e, p(1e15, 1e15), 2.0, 0.0, 90.0);
    run(&mut e, &["ARC", "5,0", "E", "0,5", "R", "-5"]);
    assert_eq!(e.drawing().header.view, view);
    assert_eq!(e.drawing().header.limits, limits);
    for version in [
        acad_dwg::header::Version::Ac12,
        acad_dwg::header::Version::Ac140,
    ] {
        assert_eq!(
            acad_dwg::parse(&acad_dwg::write_version(e.drawing(), version).unwrap())
                .unwrap()
                .items,
            e.drawing().items
        );
    }
}
