//! Synthetic native contracts; new geometry/view forms are not measured parity.
use acad_cmd::Editor;
use acad_model::{Block, DwgView, Entity, Item, Point, Repeat};
fn p(x: f64, y: f64) -> Point {
    Point { x, y }
}
fn run(e: &mut Editor, inputs: &[&str]) {
    for input in inputs {
        e.submit(input).unwrap();
    }
}
fn bare(mut e: &Entity) -> &Entity {
    while let Entity::OnLayer { entity, .. } = e {
        e = entity;
    }
    e
}
fn line(e: &Editor, index: usize) -> (Point, Point) {
    let Item::Entity(e) = &e.drawing().items[index] else {
        panic!()
    };
    let Entity::Line { start, end } = bare(e) else {
        panic!()
    };
    (*start, *end)
}
fn two_lines(first: (Point, Point), second: (Point, Point)) -> Editor {
    let mut e = Editor::default();
    e.drawing_mut().items = vec![
        Item::Entity(Entity::Line {
            start: first.0,
            end: first.1,
        }),
        Item::Entity(Entity::Line {
            start: second.0,
            end: second.1,
        }),
    ];
    e
}
fn close(a: f64, b: f64) {
    assert!((a - b).abs() < 1e-10, "{a} != {b}");
}
#[test]
fn initial_zero_extends_nearest_line_ends_and_one_undo_restores_all() {
    let mut e = two_lines((p(-5.0, 0.0), p(-2.0, 0.0)), (p(0.0, 2.0), p(0.0, 5.0)));
    let before = e.drawing().clone();
    assert_eq!(before.header.fillet_radius, 0.0);
    run(&mut e, &["FILLET", "1,2"]);
    assert_eq!(e.drawing().items.len(), 2);
    assert_eq!(line(&e, 0), (p(-5.0, 0.0), p(0.0, 0.0)));
    assert_eq!(line(&e, 1), (p(0.0, 0.0), p(0.0, 5.0)));
    assert_eq!(e.drawing().header.view, before.header.view);
    assert_eq!(e.drawing().header.limits, before.header.limits);
    run(&mut e, &["UNDO"]);
    assert_eq!(e.drawing(), &before);
    // A zero fillet already meeting at endpoints creates neither ARC nor undo.
    let mut e = two_lines((p(-5.0, 0.0), p(0.0, 0.0)), (p(0.0, 0.0), p(0.0, 5.0)));
    let before = e.drawing().clone();
    run(&mut e, &["FILLET", "1,2", "UNDO"]);
    assert_eq!(e.drawing(), &before);
}
#[test]
fn radius_dialogue_is_remembered_retryable_cancelled_and_undoable() {
    let mut e = Editor::default();
    let before = e.drawing().clone();
    run(&mut e, &["FILLET", "R"]);
    for invalid in ["-1", "NaN", "inf"] {
        assert!(e.submit(invalid).is_err());
        assert_eq!(e.drawing(), &before);
        assert_eq!(e.prompt(), "FILLET: radius");
    }
    e.cancel_command().unwrap();
    assert_eq!(e.drawing(), &before);
    run(&mut e, &["FILLET", "R", "2.5", "FILLET", "R", ""]);
    assert_eq!(e.drawing().header.fillet_radius, 2.5);
    run(&mut e, &["UNDO"]);
    assert_eq!(e.drawing(), &before, "unchanged R Return adds no undo");
}
#[test]
fn positive_acute_obtuse_and_extended_fillets_are_tangent_and_atomic() {
    for degrees in [35_f64, 90.0, 140.0] {
        let (sin, cos) = degrees.to_radians().sin_cos();
        let mut e = two_lines(
            (p(-10.0, 0.0), p(-2.0, 0.0)),
            (p(2.0 * cos, 2.0 * sin), p(10.0 * cos, 10.0 * sin)),
        );
        run(&mut e, &["FILLET", "R", "0.5"]);
        let before = e.drawing().clone();
        run(&mut e, &["FILLET", "1,2"]);
        let Item::Entity(arc) = e.drawing().items.last().unwrap() else {
            panic!()
        };
        let Entity::Arc {
            center,
            radius,
            start_deg,
            end_deg,
        } = bare(arc)
        else {
            panic!()
        };
        let tangent = [line(&e, 0).1, line(&e, 1).0];
        for (t, dir) in tangent.into_iter().zip([p(1.0, 0.0), p(cos, sin)]) {
            close((t.x - center.x).hypot(t.y - center.y), *radius);
            close((t.x - center.x) * dir.x + (t.y - center.y) * dir.y, 0.0);
        }
        close((end_deg - start_deg).rem_euclid(360.0), degrees);
        run(&mut e, &["UNDO"]);
        assert_eq!(e.drawing(), &before);
    }
}
#[test]
fn unsupported_parallel_degenerate_and_excessive_geometry_never_mutates() {
    for (first, second, radius) in [
        ((p(0.0, 0.0), p(3.0, 0.0)), (p(0.0, 1.0), p(3.0, 1.0)), 0.0),
        ((p(0.0, 0.0), p(0.0, 0.0)), (p(0.0, 1.0), p(3.0, 1.0)), 0.0),
        (
            (p(-5.0, 0.0), p(5.0, 0.0)),
            (p(0.0, -5.0), p(0.0, 5.0)),
            100.0,
        ),
        (
            (p(-1e308, 0.0), p(1e308, 0.0)),
            (p(0.0, -5.0), p(0.0, 5.0)),
            0.0,
        ),
        (
            (p(1e16 - 10.0, 1e16), p(1e16 + 10.0, 1e16)),
            (p(1e16, 1e16 - 10.0), p(1e16, 1e16 + 10.0)),
            0.1,
        ),
    ] {
        let mut e = two_lines(first, second);
        e.drawing_mut().header.fillet_radius = radius;
        let before = e.drawing().clone();
        run(&mut e, &["FILLET"]);
        assert!(e.submit("1,2").is_err());
        assert_eq!(e.drawing(), &before);
        e.cancel_command().unwrap();
        run(&mut e, &["UNDO"]);
        assert_eq!(e.drawing(), &before);
    }
}
#[test]
fn numeric_all_relative_and_x_current_relative_previous_swaps() {
    let mut e = Editor::default();
    run(&mut e, &["ZOOM", "C", "-5,8", "30", "PAN", "@10,20", ""]);
    let custom = e.drawing().header.view;
    run(&mut e, &["ZOOM", "2X"]);
    assert_eq!(
        e.drawing().header.view,
        DwgView {
            center: custom.center,
            height: 15.0
        }
    );
    run(&mut e, &["ZOOM", "2"]);
    let absolute = e.drawing().header.view;
    assert_ne!(absolute.center, custom.center);
    run(&mut e, &["ZOOM", "1"]);
    let all = e.drawing().header.view;
    assert_eq!(absolute.center, all.center);
    assert_eq!(absolute.height, all.height / 2.0);
    run(&mut e, &["ZOOM", "P"]);
    assert_eq!(e.drawing().header.view, absolute);
    run(&mut e, &["ZOOM", "P"]);
    assert_eq!(e.drawing().header.view, all);
}
#[test]
fn viewport_window_reverse_corners_lower_left_and_pan_displacement() {
    let mut e = Editor::default();
    e.set_viewport_size(200, 100).unwrap();
    run(&mut e, &["SNAP", "10", "ORTHO", "ON", "ZOOM", "W"]);
    e.submit_mouse_point(p(10.25, 4.5)).unwrap();
    e.submit_mouse_point(p(0.25, 0.5)).unwrap();
    assert_eq!(
        e.drawing().header.view,
        DwgView {
            center: p(5.25, 2.5),
            height: 5.0
        }
    );
    run(&mut e, &["ZOOM", "L", "-3,-4", "8"]);
    assert_eq!(
        e.drawing().header.view,
        DwgView {
            center: p(5.0, 0.0),
            height: 8.0
        }
    );
    let before = e.drawing().header.view;
    run(&mut e, &["PAN", "@2.5,-1"]);
    assert_eq!(e.drawing().header.view, before);
    run(&mut e, &[""]);
    assert_eq!(e.drawing().header.view.center, p(2.5, 1.0));
    run(&mut e, &["PAN", "1.25,2.5", "@-2,3"]);
    assert_eq!(e.drawing().header.view.center, p(4.5, -2.0));
    assert_eq!(e.drawing().header.view.height, 8.0);
    run(&mut e, &["PAN", "1,2"]);
    assert!(e.submit("").is_err());
    e.cancel_command().unwrap();
    assert_eq!(e.drawing().header.view.center, p(4.5, -2.0));
}
#[test]
fn views_use_visible_repeat_and_rotated_insert_geometry_not_hidden_or_definitions() {
    let mut e = Editor::default();
    e.set_viewport_size(200, 100).unwrap();
    e.drawing_mut().items = vec![
        Item::Block(Block {
            name: "B".into(),
            base: p(1.0, 1.0),
            entities: vec![Entity::Line {
                start: p(1.0, 1.0),
                end: p(5.0, 1.0),
            }],
        }),
        Item::Entity(Entity::Insert {
            name: "B".into(),
            origin: p(2.0, 3.0),
            x_scale: 2.0,
            y_scale: 1.0,
            rotation_deg: 90.0,
        }),
        Item::Repeat(Repeat {
            start_layer: 1,
            end_layer: 1,
            rows: 2,
            columns: 3,
            row_spacing: 5.0,
            column_spacing: -4.0,
            entities: vec![Entity::Point {
                origin: p(0.0, 0.0),
            }],
        }),
        Item::Entity(Entity::OnLayer {
            layer: 2,
            entity: Box::new(Entity::Circle {
                center: p(1000.0, 1000.0),
                radius: 100.0,
            }),
        }),
    ];
    e.drawing_mut().header.off_layers.insert(2);
    e.drawing_mut().header.layers.insert(2, 7);
    run(&mut e, &["ZOOM", "E"]);
    let view = e.drawing().header.view;
    close(view.center.x, -3.0);
    close(view.center.y, 5.5);
    close(view.height, 11.0);
    run(&mut e, &["ZOOM", "A"]);
    assert!(e.drawing().header.view.height < 20.0);
    assert!(e.drawing().header.view.center.x < 10.0);
}
#[test]
fn invalid_view_and_pan_results_preserve_previous_and_are_retryable() {
    let mut e = Editor::default();
    e.set_viewport_size(200, 100).unwrap();
    let original = e.drawing().clone();
    run(&mut e, &["ZOOM"]);
    for invalid in ["0", "-1", "NaN", "1e-320X"] {
        assert!(e.submit(invalid).is_err());
        assert_eq!(e.drawing(), &original);
    }
    e.cancel_command().unwrap();
    run(&mut e, &["ZOOM", "L", "1e308,1e308"]);
    assert!(e.submit("1e308").is_err());
    assert_eq!(e.drawing(), &original);
    e.cancel_command().unwrap();
    run(&mut e, &["PAN", "-1e308,0"]);
    assert!(e.submit("1e308,0").is_err());
    assert_eq!(e.drawing(), &original);
    e.cancel_command().unwrap();
    run(&mut e, &["ZOOM", "2X"]);
    let zoomed = e.drawing().header.view;
    run(&mut e, &["ZOOM", "P"]);
    assert_eq!(e.drawing().header.view, original.header.view);
    run(&mut e, &["ZOOM", "P"]);
    assert_eq!(e.drawing().header.view, zoomed);
}

#[test]
fn mouse_window_fillet_uses_remembered_radius_and_radius_route_discards_pending_set() {
    let mut e = two_lines((p(-5.0, 0.0), p(5.0, 0.0)), (p(0.0, -5.0), p(0.0, 5.0)));
    let before = e.drawing().clone();
    run(&mut e, &["FILLET"]);
    assert_eq!(e.pick_selection_at(p(4.0, 0.0), 0.1).unwrap(), Some(1));
    assert_eq!(e.pick_selection_at(p(4.0, 0.0), 0.1).unwrap(), Some(1));
    assert_eq!(e.collected_selection(), &[1]);
    run(&mut e, &["R"]);
    assert_eq!(e.prompt(), "FILLET: radius");
    assert!(e.collected_selection().is_empty());
    e.cancel_command().unwrap();
    assert_eq!(e.drawing(), &before);
    run(&mut e, &["FILLET", "R", "1", "FILLET", "W", "-6,-6", "6,6"]);
    assert_eq!(e.drawing().items.len(), 2);
    assert_eq!(e.collected_selection(), &[1, 2]);
    run(&mut e, &[""]);
    assert_eq!(e.drawing().items.len(), 3);
    assert_eq!(e.drawing().header.fillet_radius, 1.0);
    run(&mut e, &["UNDO"]);
    assert_eq!(e.drawing().items, before.items);
    assert_eq!(e.drawing().header.fillet_radius, 1.0);
}
#[test]
fn global_view_bounds_budget_and_deep_wrappers_fail_before_history_changes() {
    for item in [
        Item::Entity((0..257).fold(
            Entity::Point {
                origin: p(0.0, 0.0),
            },
            |entity, _| Entity::OnLayer {
                layer: 1,
                entity: Box::new(entity),
            },
        )),
        Item::Entity(Entity::Repeat(Repeat {
            start_layer: 1,
            end_layer: 1,
            rows: 1,
            columns: 1,
            column_spacing: 0.0,
            row_spacing: 0.0,
            entities: vec![
                Entity::Point {
                    origin: p(0.0, 0.0)
                };
                100_001
            ],
        })),
    ] {
        let mut e = Editor::default();
        e.drawing_mut().items = vec![item];
        let before = e.drawing().clone();
        run(&mut e, &["ZOOM"]);
        assert!(e.submit("E").unwrap_err().contains("traversal limits"));
        assert_eq!(e.drawing(), &before);
        e.cancel_command().unwrap();
        run(&mut e, &["ZOOM", "P"]);
        assert_eq!(e.drawing(), &before);
    }
}
/// Submit `bad` after `prefix`; the error must keep drawing/view and prompt,
/// leave Previous at `previous`, and keep the same prompt retryable with `retry`.
fn assert_unusable_view(e: &mut Editor, prefix: &[&str], bad: &str, retry: &str) {
    let before = e.drawing().clone();
    for retry_after_error in [false, true] {
        run(e, prefix);
        let prompt = e.prompt().to_owned();
        let error = e.submit(bad).unwrap_err();
        assert!(
            error.contains("view must have"),
            "{prefix:?} {bad}: {error}"
        );
        assert_eq!(e.prompt(), prompt, "{prefix:?} {bad}");
        assert_eq!(e.drawing(), &before, "{prefix:?} {bad}");
        if retry_after_error {
            e.submit(retry).unwrap();
            assert_eq!(e.prompt(), "Command");
            run(e, &["ZOOM", "P"]);
            assert_eq!(e.drawing(), &before);
        } else {
            e.cancel_command().unwrap();
            // Previous was not replaced by the refused view: P toggles back
            // to the earlier committed view and then returns.
            run(e, &["ZOOM", "P"]);
            assert_ne!(
                e.drawing().header.view,
                before.header.view,
                "{prefix:?} {bad}"
            );
            run(e, &["ZOOM", "P"]);
            assert_eq!(e.drawing(), &before);
        }
    }
}
#[test]
fn collapsed_corners_and_infinite_render_scale_are_refused_for_every_view_form() {
    let mut e = Editor::default();
    e.set_viewport_size(200, 100).unwrap();
    run(&mut e, &["ZOOM", "C", "0,0", "1"]);
    let cases: [(&[&str], &str, &str); 9] = [
        // Huge centers swallow the half spans: both corners round to center.
        (&["ZOOM", "C", "1e308,1e308"], "1", "1e300"),
        (&["ZOOM", "L", "1e308,1e308"], "1", "1e300"),
        (&["PAN", "0,0"], "1e308,1e308", "1,1"),
        // Tiny positive heights give 100 px / height = infinity.
        (&["ZOOM", "C", "0,0"], "1e-308", "2"),
        (&["ZOOM", "L", "0,0"], "1e-308", "2"),
        (&["ZOOM", "W", "0,0"], "1e-308,1e-308", "2,2"),
        (&["ZOOM"], "1e308X", "2X"),
        (&["ZOOM"], "1e308", "2"),
        (&["ZOOM"], "1e-320X", "2X"),
    ];
    for (prefix, bad, retry) in cases {
        assert_unusable_view(&mut e, prefix, bad, retry);
    }

    // Relative factor collapse at a huge but initially usable center.
    let mut e = Editor::default();
    e.set_viewport_size(200, 100).unwrap();
    run(&mut e, &["ZOOM", "C", "1e308,1e308", "1e300"]);
    assert_unusable_view(&mut e, &["ZOOM"], "1e10X", "2X");

    // E: one visible point at a huge coordinate collapses its unit view; two
    // tiny-separated points produce an infinite render scale.
    for points in [vec![p(1e308, 1e308)], vec![p(0.0, 0.0), p(1e-308, 1e-308)]] {
        let mut e = Editor::default();
        e.set_viewport_size(200, 100).unwrap();
        e.drawing_mut().items = points
            .into_iter()
            .map(|origin| Item::Entity(Entity::Point { origin }))
            .collect();
        run(&mut e, &["ZOOM", "C", "0,0", "1"]);
        assert_unusable_view(&mut e, &["ZOOM"], "E", "1X");
    }

    // A: tiny limits and no geometry fit to an infinite render scale.
    let mut e = Editor::default();
    e.set_viewport_size(200, 100).unwrap();
    e.drawing_mut().header.limits = acad_model::Extents {
        xmin: 0.0,
        ymin: 0.0,
        xmax: 1e-308,
        ymax: 1e-308,
    };
    run(&mut e, &["ZOOM", "C", "0,0", "1"]);
    assert_unusable_view(&mut e, &["ZOOM"], "A", "1X");
}
#[test]
fn previous_is_revalidated_for_the_current_pixel_canvas() {
    let mut e = Editor::default();
    // A one-pixel-high canvas can display a 1e-308 high view (scale 1e308).
    e.set_viewport_size(200, 1).unwrap();
    run(
        &mut e,
        &["ZOOM", "C", "0,0", "1e-308", "ZOOM", "C", "0,0", "1"],
    );
    let current = e.drawing().clone();
    // On a 100-pixel canvas that Previous view has infinite scale.
    e.set_viewport_size(200, 100).unwrap();
    run(&mut e, &["ZOOM"]);
    assert!(e.submit("P").unwrap_err().contains("view must have"));
    assert_eq!(e.prompt(), "ZOOM: number, numberX, A, E, W, C, L or P");
    assert_eq!(e.drawing(), &current);
    e.cancel_command().unwrap();
    // Previous is retained: on the original canvas it is still reachable.
    e.set_viewport_size(200, 1).unwrap();
    run(&mut e, &["ZOOM", "P"]);
    assert_eq!(e.drawing().header.view.height, 1e-308);
}
