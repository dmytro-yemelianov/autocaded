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
fn circle(e: &Editor) -> (Point, f64) {
    match bare(e.drawing().entities().last().unwrap()) {
        Entity::Circle { center, radius } => (*center, *radius),
        other => panic!("{other:?}"),
    }
}

#[test]
fn radius_points_relative_polar_and_numeric_diameters_produce_the_same_circle() {
    for input in [
        vec!["5"],
        vec!["5,7"],
        vec!["@3,4"],
        vec!["@5<30"],
        vec!["D", "10"],
    ] {
        let mut e = Editor::default();
        run(&mut e, &["CIRCLE", "2,3"]);
        run(&mut e, &input);
        let (center, radius) = circle(&e);
        assert_eq!(center, p(2.0, 3.0));
        assert!((radius - 5.0).abs() < 1e-12);
        assert_eq!(e.prompt(), "Command");
    }
}

#[test]
fn two_point_circles_use_the_diameter_and_support_relative_endpoints() {
    for inputs in [
        ["CIRCLE", "2p", "-3,3", "7,3"],
        ["CIRCLE", "2P", "-3,3", "@10,0"],
        ["CIRCLE", "2P", "-3,3", "@10<0"],
    ] {
        let mut e = Editor::default();
        run(&mut e, &inputs);
        assert_eq!(circle(&e), (p(2.0, 3.0), 5.0));
    }
    let mut e = Editor::default();
    run(&mut e, &["CIRCLE", "2P", "-1e308,0", "1e308,0"]);
    assert_eq!(circle(&e), (p(0.0, 0.0), 1e308));
}

#[test]
fn three_point_circles_are_independent_of_order_and_accept_relative_third_points() {
    for (points, center) in [
        (["7,3", "2,8", "-3,3"], p(2.0, 3.0)),
        (["-1,10", "-8,9", "0,3"], p(-4.0, 6.0)),
    ] {
        for order in [
            [0, 1, 2],
            [0, 2, 1],
            [1, 0, 2],
            [1, 2, 0],
            [2, 0, 1],
            [2, 1, 0],
        ] {
            let mut e = Editor::default();
            run(
                &mut e,
                &[
                    "CIRCLE",
                    "3p",
                    points[order[0]],
                    points[order[1]],
                    points[order[2]],
                ],
            );
            let (actual, radius) = circle(&e);
            assert!((actual.x - center.x).abs() < 1e-12 && (actual.y - center.y).abs() < 1e-12);
            assert!((radius - 5.0).abs() < 1e-12);
        }
    }
    let mut e = Editor::default();
    run(&mut e, &["CIRCLE", "3P", "7,3", "@-5,5", "@-5,-5"]);
    assert_eq!(circle(&e), (p(2.0, 3.0), 5.0));
}

#[test]
fn three_point_construction_handles_tiny_large_and_translated_coordinates() {
    for radius in [1e-200, 1.0, 1e150, 1e308] {
        let mut e = Editor::default();
        run(&mut e, &["CIRCLE", "3P"]);
        for point in [p(0.0, -radius), p(radius, 0.0), p(-radius, 0.0)] {
            e.submit_mouse_point(point).unwrap();
        }
        let (center, actual) = circle(&e);
        assert!(center.x.abs() / radius < 1e-12 && center.y.abs() / radius < 1e-12);
        assert!((actual / radius - 1.0).abs() < 1e-12);
    }
    let mut e = Editor::default();
    run(&mut e, &["CIRCLE", "3P"]);
    let center = p(1e15, -1e15);
    for point in [
        p(center.x + 2.0, center.y),
        p(center.x, center.y + 2.0),
        p(center.x - 2.0, center.y),
    ] {
        e.submit_mouse_point(point).unwrap();
    }
    assert_eq!(circle(&e), (center, 2.0));
}

#[test]
fn invalid_constructions_retain_the_prompt_drawing_and_undo_for_retry() {
    for (setup, invalid, valid) in [
        (
            vec!["CIRCLE", "0,0"],
            vec!["0", "-1", "NaN", "inf", "0,0", "@1.7e308,1.7e308"],
            "2",
        ),
        (
            vec!["CIRCLE", "0,0", "D"],
            vec!["0", "-1", "NaN", "1e-3230"],
            "4",
        ),
        (
            vec!["CIRCLE", "2P", "0,0"],
            vec!["0,0", "NaN,1", "1.7e308,1.7e308"],
            "4,0",
        ),
        (
            vec!["CIRCLE", "3P", "0,0", "1,0"],
            vec!["0,0", "1,0", "2,0", "inf,0"],
            "0,1",
        ),
    ] {
        let mut e = Editor::default();
        run(&mut e, &["POINT", "9,9"]);
        run(&mut e, &setup);
        let before = e.drawing().clone();
        let prompt = e.prompt().to_owned();
        for bad in invalid {
            assert!(e.submit(bad).is_err(), "{bad}");
            assert_eq!(e.prompt(), prompt);
            assert_eq!(e.drawing(), &before);
        }
        e.submit(valid).unwrap();
        e.submit("UNDO").unwrap();
        assert_eq!(e.drawing(), &before);
        e.submit("UNDO").unwrap();
        assert_eq!(e.drawing().entities().count(), 0);
    }
    let mut e = Editor::default();
    run(&mut e, &["CIRCLE", "3P", "1e308,0"]);
    assert!(e.submit("@1e308,0").is_err());
    assert_eq!(e.prompt(), "CIRCLE 3P: second point");
    assert!(e.submit("1e308,0").is_err());
    e.submit("0,1e308").unwrap();
    assert_eq!(e.prompt(), "CIRCLE 3P: third point");
}

#[test]
fn mouse_curves_snap_without_ortho_and_numeric_diameter_does_not_accept_points() {
    let mut e = Editor::default();
    run(&mut e, &["SNAP", "1", "ORTHO", "ON", "CIRCLE", "0,0"]);
    assert_eq!(e.constrain_mouse_point(p(3.1, 4.1)).unwrap(), p(3.0, 4.0));
    e.submit_mouse_point(p(3.1, 4.1)).unwrap();
    assert_eq!(circle(&e), (p(0.0, 0.0), 5.0));
    run(&mut e, &["CIRCLE", "3P"]);
    for point in [p(3.1, 0.1), p(0.1, 3.1), p(-3.1, 0.1)] {
        e.submit_mouse_point(point).unwrap();
    }
    assert_eq!(circle(&e), (p(0.0, 0.0), 3.0));
    run(&mut e, &["CIRCLE", "0,0", "D"]);
    assert!(!e.accepts_mouse_point());
    assert!(e.submit_mouse_point(p(1.0, 1.0)).is_err());
    e.cancel_command().unwrap();
    assert_eq!(e.drawing().entities().count(), 2);
}

#[test]
fn circle_options_preserve_layer_view_limits_and_round_trip_through_both_dwg_revisions() {
    let mut e = Editor::default();
    run(&mut e, &["LAYER", "4", "PAN", "@10,20", ""]);
    let before = e.drawing().clone();
    for input in [
        vec!["CIRCLE", "2P", "-3,3", "7,3"],
        vec!["CIRCLE", "3P", "7,3", "2,8", "-3,3"],
    ] {
        run(&mut e, &input);
        assert_eq!(e.drawing().header.view, before.header.view);
        assert_eq!(e.drawing().header.limits, before.header.limits);
        assert!(matches!(
            e.drawing().entities().last(),
            Some(Entity::OnLayer { layer: 4, .. })
        ));
    }
    for version in [
        acad_dwg::header::Version::Ac12,
        acad_dwg::header::Version::Ac140,
    ] {
        let bytes = acad_dwg::write_version(e.drawing(), version).unwrap();
        assert_eq!(acad_dwg::parse(&bytes).unwrap().items, e.drawing().items);
    }
}

#[test]
fn triangular_solid_duplicates_the_third_corner_and_preserves_existing_chain_and_undo() {
    let mut e = Editor::default();
    run(&mut e, &["LAYER", "3", "SOLID", "0,0", "4,0", "0,4"]);
    let view = e.drawing().header.view;
    let limits = e.drawing().header.limits;
    assert!(e.drawing().items.is_empty());
    e.submit_return("").unwrap();
    assert!(
        matches!(bare(e.drawing().entities().next().unwrap()),Entity::Solid{p1,p2,p3,p4} if *p1==p(0.0,0.0)&&*p2==p(4.0,0.0)&&*p3==p(0.0,4.0)&&p3==p4)
    );
    assert_eq!(e.prompt(), "SOLID: third point (Enter to finish)");
    // The existing chain reuses the stored third/fourth pair, now coincident.
    run(&mut e, &["4,4", "4,8", ""]);
    assert!(
        matches!(bare(e.drawing().entities().last().unwrap()),Entity::Solid{p1,p2,p3,p4} if *p1==p(0.0,4.0)&&p1==p2&&*p3==p(4.0,4.0)&&*p4==p(4.0,8.0))
    );
    assert_eq!(e.drawing().header.view, view);
    assert_eq!(e.drawing().header.limits, limits);
    let bytes = acad_dwg::write(e.drawing()).unwrap();
    assert_eq!(acad_dwg::parse(&bytes).unwrap().items, e.drawing().items);
    e.submit("UNDO").unwrap();
    assert_eq!(e.drawing().entities().count(), 1);
    e.submit("UNDO").unwrap();
    assert!(e.drawing().items.is_empty());
}

#[test]
fn cancellation_and_empty_circle_inputs_leave_completed_entities_intact() {
    for setup in [
        vec!["CIRCLE"],
        vec!["CIRCLE", "1,1"],
        vec!["CIRCLE", "1,1", "D"],
        vec!["CIRCLE", "2P"],
        vec!["CIRCLE", "2P", "1,1"],
        vec!["CIRCLE", "3P"],
        vec!["CIRCLE", "3P", "1,1"],
        vec!["CIRCLE", "3P", "1,1", "2,2"],
    ] {
        let mut e = Editor::default();
        run(&mut e, &["POINT", "9,9"]);
        let before = e.drawing().clone();
        run(&mut e, &setup);
        assert!(e.submit("").is_err());
        assert_eq!(e.drawing(), &before);
        e.cancel_command().unwrap();
        assert_eq!(e.prompt(), "Command");
        e.submit("UNDO").unwrap();
        assert_eq!(e.drawing().entities().count(), 0);
    }
    let mut e = Editor::default();
    run(&mut e, &["CIRCLE", "1,1"]);
    e.submit_return("").unwrap();
    assert_eq!(e.prompt(), "Command");
    assert_eq!(e.status(), "*Invalid*");
}

#[test]
fn solid_relative_overflow_is_retryable_before_a_triangle_is_committed() {
    let mut e = Editor::default();
    run(&mut e, &["SOLID", "1e308,0"]);
    assert!(e.submit("@1e308,0").is_err());
    assert_eq!(e.prompt(), "SOLID: second point");
    e.submit("1e308,1").unwrap();
    assert!(e.submit("@1e308,0").is_err());
    assert_eq!(e.prompt(), "SOLID: third point (Enter to finish)");
    e.submit("0,1").unwrap();
    assert!(e.submit("@-1.7e308,1.7e308").is_ok()); // finite fourth point, existing quad route
    assert_eq!(e.drawing().entities().count(), 1);
    e.cancel_command().unwrap();
    run(&mut e, &["SOLID", "0,0", "1,0", "1e308,1"]);
    assert!(e.submit("@1e308,0").is_err());
    assert!(e.drawing().entities().count() == 1);
    e.submit("").unwrap();
    assert_eq!(e.drawing().entities().count(), 2);
}
