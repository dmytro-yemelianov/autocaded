//! Circular ARRAY options and BREAK point picking/TRACE (docs/native-array-break.md).
//! Expected values come from the original ACAD.EXE run by the in-tree runner
//! (see crates/acad-oracle/tests/array_break.rs).
use acad_cmd::Editor;
use acad_model::{Block, Entity, Item, Point, Repeat};

fn p(x: f64, y: f64) -> Point {
    Point { x, y }
}
fn submit(editor: &mut Editor, inputs: &[&str]) {
    for input in inputs {
        editor
            .submit(input)
            .unwrap_or_else(|error| panic!("{input}: {error}"));
    }
}
fn layered(layer: u8, entity: Entity) -> Entity {
    Entity::OnLayer {
        layer,
        entity: Box::new(entity),
    }
}
fn bare(entity: &Entity) -> &Entity {
    match entity {
        Entity::OnLayer { entity, .. } => bare(entity),
        other => other,
    }
}
fn close(a: Point, b: Point) -> bool {
    (a.x - b.x).abs() < 1e-9 && (a.y - b.y).abs() < 1e-9
}
fn line(start: Point, end: Point) -> Entity {
    Entity::Line { start, end }
}
fn trace(p1: Point, p2: Point, p3: Point, p4: Point) -> Entity {
    Entity::Trace { p1, p2, p3, p4 }
}
fn live(editor: &Editor) -> Vec<Entity> {
    editor
        .drawing()
        .entities()
        .map(|e| bare(e).clone())
        .collect()
}
fn assert_entities_close(actual: &[Entity], expected: &[Entity]) {
    assert_eq!(actual.len(), expected.len(), "{actual:?}");
    for (a, e) in actual.iter().zip(expected) {
        match (bare(a), bare(e)) {
            (Entity::Line { start, end }, Entity::Line { start: s, end: t }) => {
                assert!(close(*start, *s) && close(*end, *t), "{a:?} != {e:?}")
            }
            (
                Entity::Trace { p1, p2, p3, p4 },
                Entity::Trace {
                    p1: q1,
                    p2: q2,
                    p3: q3,
                    p4: q4,
                },
            ) => assert!(
                close(*p1, *q1) && close(*p2, *q2) && close(*p3, *q3) && close(*p4, *q4),
                "{a:?} != {e:?}"
            ),
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
                    end_deg: t,
                },
            ) => assert!(
                close(*center, *c)
                    && (radius - r).abs() < 1e-9
                    && (start_deg - s).abs() < 1e-6
                    && (end_deg - t).abs() < 1e-6,
                "{a:?} != {e:?}"
            ),
            _ => assert_eq!(a, e),
        }
    }
}

fn line_editor() -> Editor {
    let mut editor = Editor::default();
    submit(&mut editor, &["LINE", "5,3", "6,3", ""]);
    editor
}

fn circular_count(angle: &str, items: &str) -> usize {
    let mut editor = line_editor();
    submit(&mut editor, &["ARRAY", "L", "C", "4,3", angle, items]);
    assert_eq!(editor.prompt(), "Command", "{angle} {items}");
    editor.drawing().entities().count()
}

#[test]
fn circular_array_prompts_name_the_retained_options() {
    let mut editor = line_editor();
    submit(&mut editor, &["ARRAY", "L", "C"]);
    assert_eq!(editor.prompt(), "ARRAY: center point");
    submit(&mut editor, &["4,3"]);
    assert_eq!(editor.prompt(), "ARRAY: angle between items (+=CCW, -=CW)");
    submit(&mut editor, &["90"]);
    assert_eq!(
        editor.prompt(),
        "ARRAY: number of items or -(degrees to fill)"
    );
}

#[test]
fn angle_to_cover_is_endpoint_inclusive_except_for_the_full_circle() {
    for (fill, expected) in [
        ("-360", 4),
        ("-270", 4),
        ("-180", 3),
        ("-100", 2),
        ("-90", 2),
        ("-46", 2),
        ("-45", 2),
        ("-44", 1),
        ("-1", 1),
        ("-135", 3),
        ("-134", 2),
        ("-359", 5),
        ("-361", 5),
        ("-720", 9),
        ("-1080", 13),
        ("0", 4),
        ("-0", 4),
    ] {
        assert_eq!(circular_count("90", fill), expected, "fill {fill}");
    }
    for (angle, expected) in [("100", 4), ("80", 5), ("70", 5), ("135", 3), ("360", 1)] {
        assert_eq!(circular_count(angle, "0"), expected, "angle {angle}");
        assert_eq!(circular_count(angle, "-360"), expected, "angle {angle}");
    }
}

#[test]
fn angle_to_cover_follows_the_angle_sign_and_keeps_translation_only_copies() {
    let mut editor = line_editor();
    submit(&mut editor, &["ARRAY", "L", "C", "4,3", "-90", "-180"]);
    assert_entities_close(
        &live(&editor),
        &[
            line(p(5.0, 3.0), p(6.0, 3.0)),
            line(p(4.0, 2.0), p(5.0, 2.0)),
            line(p(3.0, 3.0), p(4.0, 3.0)),
        ],
    );
    submit(&mut editor, &["UNDO"]);
    assert_eq!(editor.drawing().entities().count(), 1, "one UNDO");
}

#[test]
fn zero_and_oversized_angles_and_fractional_counts_retry_without_mutation() {
    let mut editor = line_editor();
    let before = editor.drawing().clone();
    submit(&mut editor, &["ARRAY", "L", "C", "4,3"]);
    for bad in ["0", "-0", "400", "-360.5", "x"] {
        assert!(editor.submit(bad).is_err(), "{bad}");
        assert_eq!(editor.prompt(), "ARRAY: angle between items (+=CCW, -=CW)");
    }
    submit(&mut editor, &["-360"]);
    for bad in ["2.5", "x", ""] {
        assert!(editor.submit(bad).is_err(), "{bad}");
        assert_eq!(
            editor.prompt(),
            "ARRAY: number of items or -(degrees to fill)"
        );
    }
    assert_eq!(editor.drawing(), &before);
    submit(&mut editor, &["3"]);
    assert_eq!(
        editor.drawing().entities().count(),
        3,
        "360 makes coincident copies"
    );
    let copies = live(&editor);
    assert_entities_close(
        &copies,
        &[copies[0].clone(), copies[0].clone(), copies[0].clone()],
    );
}

#[test]
fn angle_to_cover_honours_the_output_bound_before_mutation_or_undo() {
    let mut editor = line_editor();
    let before = editor.drawing().clone();
    submit(&mut editor, &["ARRAY", "L", "C", "4,3", "0.001"]);
    for fill in ["-360", "-1e300", "100001"] {
        let error = editor.submit(fill).unwrap_err();
        assert!(error.contains("100000"), "{fill}: {error}");
    }
    assert_eq!(editor.drawing(), &before);
    editor.cancel_command().unwrap();
    submit(&mut editor, &["UNDO"]);
    assert_eq!(
        editor.drawing().entities().count(),
        0,
        "the rejected arrays left no undo entry; UNDO removes the LINE"
    );
}

fn insert_editor() -> Editor {
    let mut editor = Editor::default();
    editor.drawing_mut().items = vec![
        Item::Block(Block {
            name: "B".into(),
            base: p(0.0, 0.0),
            entities: vec![layered(1, line(p(1.0, 0.0), p(2.0, 0.0)))],
        }),
        Item::Entity(layered(
            3,
            Entity::Insert {
                origin: p(5.0, 3.0),
                x_scale: 1.0,
                y_scale: 1.0,
                rotation_deg: 10.0,
                name: "B".into(),
            },
        )),
    ];
    editor
}

fn inserts(editor: &Editor) -> Vec<(Point, f64, u8)> {
    editor
        .drawing()
        .entities()
        .map(|entity| {
            let Entity::OnLayer { layer, entity } = entity else {
                panic!("layered insert")
            };
            let Entity::Insert {
                origin,
                rotation_deg,
                ..
            } = entity.as_ref()
            else {
                panic!("insert")
            };
            (*origin, *rotation_deg, *layer)
        })
        .collect()
}

#[test]
fn single_block_may_rotate_about_its_insertion_point_at_each_step() {
    let origins = [p(5.0, 3.0), p(4.0, 4.0), p(3.0, 3.0), p(4.0, 2.0)];
    for (answer, rotations) in [
        ("Y", [10.0, 100.0, 190.0, 280.0]),
        ("yes", [10.0, 100.0, 190.0, 280.0]),
        ("N", [10.0; 4]),
        ("", [10.0; 4]),
    ] {
        let mut editor = insert_editor();
        let before = editor.drawing().clone();
        submit(&mut editor, &["ARRAY", "1", "C", "4,3", "90", "4"]);
        assert_eq!(editor.prompt(), "ARRAY: rotate block as it is copied? <N>");
        assert_eq!(
            editor.drawing(),
            &before,
            "nothing changes before the answer"
        );
        submit(&mut editor, &[answer]);
        let got = inserts(&editor);
        assert_eq!(got.len(), 4);
        for ((origin, rotation, layer), (expected_origin, expected_rotation)) in
            got.iter().zip(origins.iter().zip(rotations))
        {
            assert!(close(*origin, *expected_origin), "{answer}: {got:?}");
            assert!(
                (rotation - expected_rotation).abs() < 1e-9,
                "{answer}: {got:?}"
            );
            assert_eq!(*layer, 3);
        }
        submit(&mut editor, &["UNDO"]);
        assert_eq!(editor.drawing(), &before);
    }
}

#[test]
fn rotated_block_angles_are_normalized_and_bad_answers_retry_or_cancel() {
    let mut editor = insert_editor();
    submit(&mut editor, &["ARRAY", "1", "C", "4,3", "-90", "-180"]);
    let before = editor.drawing().clone();
    assert!(editor.submit("X").is_err());
    assert_eq!(editor.prompt(), "ARRAY: rotate block as it is copied? <N>");
    submit(&mut editor, &["Y"]);
    let rotations: Vec<_> = inserts(&editor).iter().map(|i| i.1).collect();
    assert_eq!(rotations.len(), 3);
    for (got, expected) in rotations.iter().zip([10.0, 280.0, 190.0]) {
        assert!((got - expected).abs() < 1e-9, "{rotations:?}");
    }

    let mut editor = insert_editor();
    submit(&mut editor, &["ARRAY", "1", "C", "4,3", "120", "4"]);
    submit(&mut editor, &["Y"]);
    let last = inserts(&editor)[3];
    assert!(close(last.0, p(5.0, 3.0)) && (last.1 - 10.0).abs() < 1e-9);

    let mut editor = insert_editor();
    submit(&mut editor, &["ARRAY", "1", "C", "4,3", "90", "4"]);
    editor.cancel_command().unwrap();
    assert_eq!(editor.drawing(), &before);
    submit(&mut editor, &["UNDO"]);
    assert_eq!(editor.drawing(), &before, "cancel added no undo entry");
}

#[test]
fn several_objects_groups_and_non_blocks_never_ask_to_rotate() {
    let mut editor = insert_editor();
    editor
        .drawing_mut()
        .items
        .push(Item::Entity(layered(4, line(p(5.0, 4.0), p(6.0, 4.0)))));
    submit(&mut editor, &["ARRAY", "ALL", "C", "4,3", "90", "2"]);
    assert_eq!(editor.prompt(), "Command");
    assert_eq!(editor.drawing().entities().count(), 4);
    for entity in editor.drawing().entities() {
        if let Entity::Insert { rotation_deg, .. } = bare(entity) {
            assert_eq!(*rotation_deg, 10.0);
        }
    }

    let mut editor = line_editor();
    submit(&mut editor, &["ARRAY", "1", "C", "4,3", "90", "2"]);
    assert_eq!(editor.prompt(), "Command");

    let mut editor = Editor::default();
    editor.drawing_mut().items.push(Item::Repeat(Repeat {
        start_layer: 1,
        end_layer: 1,
        entities: vec![Entity::Insert {
            origin: p(5.0, 3.0),
            x_scale: 1.0,
            y_scale: 1.0,
            rotation_deg: 10.0,
            name: "B".into(),
        }],
        columns: 1,
        rows: 1,
        column_spacing: 0.0,
        row_spacing: 0.0,
    }));
    submit(&mut editor, &["ARRAY", "1", "C", "4,3", "90", "2"]);
    assert_eq!(
        editor.prompt(),
        "Command",
        "REPEAT owners stay translation-only"
    );
    let copy = match &editor.drawing().items[1] {
        Item::Repeat(copy) | Item::Entity(Entity::Repeat(copy)) => copy,
        other => panic!("copied group: {other:?}"),
    };
    assert!(
        matches!(&copy.entities[0], Entity::Insert { origin, rotation_deg, .. } if close(*origin, p(4.0, 4.0)) && *rotation_deg == 10.0)
    );
}

// ---------------------------------------------------------------- BREAK

fn break_line_editor() -> Editor {
    let mut editor = Editor::default();
    submit(&mut editor, &["LAYER", "6", "LINE", "0,0", "10,0", ""]);
    editor
}

#[test]
fn point_to_object_uses_the_pick_as_first_point_and_projects_the_second() {
    let mut editor = break_line_editor();
    submit(&mut editor, &["BREAK"]);
    assert_eq!(
        editor.prompt(),
        "BREAK: point to object, or one entity number"
    );
    submit(&mut editor, &["2,0.05"]);
    assert_eq!(editor.prompt(), "BREAK: second point or F (first point)");
    submit(&mut editor, &["7,1"]);
    assert_entities_close(
        &live(&editor),
        &[
            line(p(0.0, 0.0), p(2.0, 0.0)),
            line(p(7.0, 0.0), p(10.0, 0.0)),
        ],
    );
    assert!(editor
        .drawing()
        .entities()
        .all(|e| matches!(e, Entity::OnLayer { layer: 6, .. })));
    submit(&mut editor, &["UNDO"]);
    assert_eq!(live(&editor), vec![line(p(0.0, 0.0), p(10.0, 0.0))]);
}

#[test]
fn f_re_enters_the_first_point() {
    let mut editor = break_line_editor();
    submit(&mut editor, &["BREAK", "2,0", "f"]);
    assert_eq!(editor.prompt(), "BREAK: first point");
    submit(&mut editor, &["3,0"]);
    assert_eq!(editor.prompt(), "BREAK: second point");
    assert!(editor.submit("F").is_err(), "F is offered only once");
    submit(&mut editor, &["7,0"]);
    assert_entities_close(
        &live(&editor),
        &[
            line(p(0.0, 0.0), p(3.0, 0.0)),
            line(p(7.0, 0.0), p(10.0, 0.0)),
        ],
    );
}

#[test]
fn line_ends_are_cut_off_with_the_original_record_policy() {
    // End cut: the start-side piece replaces the record in place.
    let mut editor = break_line_editor();
    submit(&mut editor, &["BREAK", "2,0", "12,0"]);
    assert_eq!(editor.drawing().items.len(), 1);
    assert_entities_close(&live(&editor), &[line(p(0.0, 0.0), p(2.0, 0.0))]);
    // Start cut: the record is erased and the remainder appended.
    let mut editor = break_line_editor();
    let original = editor.drawing().items[0].clone();
    submit(&mut editor, &["BREAK", "8,0", "-1,0"]);
    let Item::Entity(source) = original else {
        panic!("line")
    };
    assert_eq!(editor.drawing().items[0], Item::Erased(source));
    assert_entities_close(&live(&editor), &[line(p(8.0, 0.0), p(10.0, 0.0))]);
    submit(&mut editor, &["UNDO"]);
    assert_eq!(editor.drawing().items.len(), 1);
    assert_eq!(editor.drawing().entities().count(), 1);
}

#[test]
fn coincident_breaks_are_rejected_atomically() {
    for (first, second) in [("2,0", "2,0"), ("2,0", "2.00000000000001,0")] {
        let mut editor = break_line_editor();
        let before = editor.drawing().clone();
        submit(&mut editor, &["BREAK", "1", first]);
        assert!(editor.submit(second).is_err(), "{first} {second}");
        assert_eq!(editor.drawing(), &before);
        assert_eq!(editor.prompt(), "BREAK: second point");
        editor.cancel_command().unwrap();
        submit(&mut editor, &["UNDO"]);
        assert_eq!(
            editor.drawing().entities().count(),
            0,
            "no BREAK undo entry"
        );
    }
}

#[test]
fn whole_object_breaks_erase_the_record_in_one_undo_step_without_feeding_oops() {
    let arc = |start, end| Entity::Arc {
        center: p(3.0, 3.0),
        radius: 2.0,
        start_deg: start,
        end_deg: end,
    };
    for (source, first, second) in [
        (line(p(0.0, 0.0), p(10.0, 0.0)), "0,0", "10,0"),
        (line(p(0.0, 0.0), p(10.0, 0.0)), "10,0", "-1,0"),
        (line(p(0.0, 0.0), p(10.0, 0.0)), "-1,0", "12,0"),
        (arc(0.0, 180.0), "5,3", "1,3"),
        (
            trace(p(0.0, 0.5), p(0.0, -0.5), p(10.0, 0.5), p(10.0, -0.5)),
            "0,0",
            "10,0",
        ),
    ] {
        let mut editor = Editor::default();
        editor.drawing_mut().items = vec![
            Item::Entity(layered(1, source.clone())),
            Item::Entity(layered(2, line(p(0.0, 7.0), p(10.0, 7.0)))),
        ];
        submit(&mut editor, &["ERASE", "2"]);
        let before = editor.drawing().clone();
        submit(&mut editor, &["BREAK", "1", first, second]);
        assert_eq!(
            editor.drawing().items[0],
            Item::Erased(layered(1, source.clone())),
            "{source:?} {first} {second}"
        );
        assert_eq!(editor.drawing().items.len(), 2, "nothing appended");
        assert_eq!(editor.drawing().entities().count(), 0);
        // OOPS restores the last ERASE only; BREAK's record stays erased.
        submit(&mut editor, &["OOPS"]);
        assert!(matches!(editor.drawing().items[0], Item::Erased(_)));
        assert!(matches!(editor.drawing().items[1], Item::Entity(_)));
        submit(&mut editor, &["UNDO", "UNDO"]);
        assert_eq!(editor.drawing(), &before, "one UNDO per command");
    }
}

#[test]
fn selector_ids_reach_break_as_ids_not_as_a_pick_point() {
    let two_lines = || {
        let mut editor = Editor::default();
        submit(
            &mut editor,
            &["LINE", "0,2", "10,2", "", "LINE", "0,5", "10,5", ""],
        );
        editor
    };
    // Window + Return over two objects: "1,2" would be a point on LINE 1.
    let mut editor = two_lines();
    let before = editor.drawing().clone();
    submit(&mut editor, &["BREAK", "W", "-1,-1", "11,6"]);
    let prompt = editor.prompt().to_owned();
    let error = editor.submit("").unwrap_err();
    assert!(error.contains("exactly one"), "{error}");
    assert_eq!(editor.prompt(), prompt);
    assert_eq!(editor.collected_selection(), &[1, 2]);
    assert_eq!(editor.drawing(), &before);

    // Window collecting one object, mouse collecting the second, Return.
    let mut editor = two_lines();
    submit(&mut editor, &["BREAK", "W", "-1,1", "11,3"]);
    assert_eq!(editor.collected_selection(), &[1]);
    assert_eq!(editor.pick_selection_at(p(5.0, 5.0), 0.1), Ok(Some(2)));
    let error = editor.submit("").unwrap_err();
    assert!(error.contains("exactly one"), "{error}");
    assert_eq!(editor.drawing(), &before);
    editor.cancel_command().unwrap();
    assert_eq!(editor.drawing(), &before);

    // One object by window or by window + mouse duplicate: explicit points follow.
    for mouse in [false, true] {
        let mut editor = two_lines();
        submit(&mut editor, &["BREAK", "W", "-1,4", "11,6"]);
        if mouse {
            assert_eq!(editor.pick_selection_at(p(5.0, 5.0), 0.1), Ok(Some(2)));
        }
        submit(&mut editor, &[""]);
        assert_eq!(editor.prompt(), "BREAK: first point");
        submit(&mut editor, &["2,5", "7,5"]);
        assert_entities_close(
            &live(&editor),
            &[
                line(p(0.0, 2.0), p(10.0, 2.0)),
                line(p(0.0, 5.0), p(2.0, 5.0)),
                line(p(7.0, 5.0), p(10.0, 5.0)),
            ],
        );
    }
}

#[test]
fn arrays_that_add_nothing_record_no_undo_step() {
    for inputs in [
        &["ARRAY", "L", "C", "4,3", "90", "-44"][..],
        &["ARRAY", "L", "C", "4,3", "90", "1"],
        &["ARRAY", "L", "C", "4,3", "360", "0"],
        &["ARRAY", "L", "R", "1", "1", "0", "0"],
    ] {
        let mut editor = line_editor();
        submit(&mut editor, &["LINE", "0,0", "1,1", ""]);
        let before = editor.drawing().clone();
        submit(&mut editor, inputs);
        assert_eq!(editor.drawing(), &before, "{inputs:?}");
        submit(&mut editor, &["UNDO"]);
        assert_eq!(
            editor.drawing().entities().count(),
            1,
            "{inputs:?}: UNDO removed the second LINE, not a phantom ARRAY step"
        );
    }
}

#[test]
fn picks_skip_erased_and_hidden_objects_and_refuse_blocks_groups_and_points() {
    let mut editor = Editor::default();
    editor.drawing_mut().header.off_layers.insert(2);
    editor.drawing_mut().items = vec![
        Item::Erased(layered(1, line(p(0.0, 5.0), p(10.0, 5.0)))),
        Item::Entity(layered(2, line(p(0.0, 6.0), p(10.0, 6.0)))),
        Item::Block(Block {
            name: "B".into(),
            base: p(0.0, 0.0),
            entities: vec![layered(1, line(p(0.0, 0.0), p(1.0, 0.0)))],
        }),
        Item::Entity(layered(
            1,
            Entity::Insert {
                origin: p(1.0, 1.0),
                x_scale: 1.0,
                y_scale: 1.0,
                rotation_deg: 0.0,
                name: "B".into(),
            },
        )),
        Item::Repeat(Repeat {
            start_layer: 1,
            end_layer: 1,
            entities: vec![layered(1, line(p(0.0, 3.0), p(2.0, 3.0)))],
            columns: 1,
            rows: 1,
            column_spacing: 0.0,
            row_spacing: 0.0,
        }),
        Item::Entity(layered(
            1,
            Entity::Point {
                origin: p(8.0, 8.0),
            },
        )),
        Item::Entity(layered(
            1,
            Entity::Solid {
                p1: p(4.0, 1.0),
                p2: p(5.0, 1.0),
                p3: p(4.0, 2.0),
                p4: p(5.0, 2.0),
            },
        )),
    ];
    let before = editor.drawing().clone();
    submit(&mut editor, &["BREAK"]);
    for (pick, message) in [
        ("5,5", "No object found"),
        ("5,6", "No object found"),
        ("1,1", "Can't break a block"),
        ("1,3", "REPEAT"),
        ("8,8", "BREAK needs a LINE, TRACE, CIRCLE or ARC"),
        ("4.5,1", "BREAK needs a LINE, TRACE, CIRCLE or ARC"),
    ] {
        let error = editor.submit(pick).unwrap_err();
        assert!(error.contains(message), "{pick}: {error}");
        assert_eq!(
            editor.prompt(),
            "BREAK: point to object, or one entity number"
        );
    }
    assert_eq!(editor.pick_selection_at(p(5.0, 6.0), 0.5), Ok(None));
    assert!(editor.pick_selection_at(p(1.0, 1.0), 0.5).is_err());
    assert_eq!(editor.drawing(), &before);
}

#[test]
fn mouse_pick_starts_the_same_dialogue() {
    let mut editor = break_line_editor();
    submit(&mut editor, &["BREAK"]);
    assert!(editor.accepts_mouse_selection());
    assert_eq!(editor.pick_selection_at(p(2.0, 0.2), 0.25), Ok(Some(1)));
    assert_eq!(editor.prompt(), "BREAK: second point or F (first point)");
    assert!(editor.accepts_mouse_point());
    editor.submit_mouse_point(p(7.0, 0.0)).unwrap();
    assert_entities_close(
        &live(&editor),
        &[
            line(p(0.0, 0.0), p(2.0, 0.0)),
            line(p(7.0, 0.0), p(10.0, 0.0)),
        ],
    );
}

#[test]
fn circle_becomes_an_erased_record_and_an_appended_arc_from_projected_points() {
    let mut editor = Editor::default();
    submit(
        &mut editor,
        &["CIRCLE", "0,0", "5", "BREAK", "5.02,0", "0,3"],
    );
    assert!(matches!(editor.drawing().items[0], Item::Erased(_)));
    assert_entities_close(
        &live(&editor),
        &[Entity::Arc {
            center: p(0.0, 0.0),
            radius: 5.0,
            start_deg: 90.0,
            end_deg: 0.0,
        }],
    );
    submit(&mut editor, &["UNDO"]);
    assert!(matches!(
        live(&editor)[..],
        [Entity::Circle { radius: 5.0, .. }]
    ));
}

#[test]
fn arc_points_outside_the_sweep_snap_to_the_nearer_end() {
    let arc = |start, end| Entity::Arc {
        center: p(3.0, 3.0),
        radius: 2.0,
        start_deg: start,
        end_deg: end,
    };
    let at = |degrees: f64| {
        let r = degrees.to_radians();
        format!("{},{}", 3.0 + 2.0 * r.cos(), 3.0 + 2.0 * r.sin())
    };
    for (first, second, erased, expected) in [
        (60.0, 120.0, false, vec![arc(0.0, 60.0), arc(120.0, 180.0)]),
        (60.0, -30.0, true, vec![arc(60.0, 180.0)]),
        (60.0, 210.0, false, vec![arc(0.0, 60.0)]),
        (60.0, 270.0, false, vec![arc(0.0, 60.0)]),
        (120.0, 270.0, false, vec![arc(0.0, 120.0)]),
    ] {
        let mut editor = Editor::default();
        editor
            .drawing_mut()
            .items
            .push(Item::Entity(layered(5, arc(0.0, 180.0))));
        submit(&mut editor, &["BREAK", &at(first), &at(second)]);
        assert_eq!(
            matches!(editor.drawing().items[0], Item::Erased(_)),
            erased,
            "{first} {second}"
        );
        assert_entities_close(&live(&editor), &expected);
        assert!(editor
            .drawing()
            .entities()
            .all(|e| matches!(e, Entity::OnLayer { layer: 5, .. })));
    }
}

fn trace_editor(command: &[&str]) -> Editor {
    let mut editor = Editor::default();
    submit(&mut editor, command);
    editor
}

#[test]
fn rectangular_trace_splits_perpendicular_to_its_centerline() {
    let mut editor = trace_editor(&["TRACE", "1", "0,0", "10,0", ""]);
    let before = editor.drawing().clone();
    submit(&mut editor, &["BREAK", "2,0.5", "7,-0.3"]);
    assert_entities_close(
        &live(&editor),
        &[
            trace(p(0.0, 0.5), p(0.0, -0.5), p(2.0, 0.5), p(2.0, -0.5)),
            trace(p(7.0, 0.5), p(7.0, -0.5), p(10.0, 0.5), p(10.0, -0.5)),
        ],
    );
    submit(&mut editor, &["UNDO"]);
    assert_eq!(editor.drawing(), &before);
}

#[test]
fn mitered_trace_matches_the_original_outside_the_miter_and_cuts_ends() {
    let command = ["TRACE", "0.5", "1,1", "7,1", "7,6", ""];
    let first_segment = trace(p(1.0, 1.25), p(1.0, 0.75), p(6.75, 1.25), p(7.25, 0.75));
    let second_segment = trace(p(6.75, 1.25), p(7.25, 0.75), p(6.75, 6.0), p(7.25, 6.0));
    let mut editor = trace_editor(&command);
    assert_entities_close(
        &live(&editor),
        &[first_segment.clone(), second_segment.clone()],
    );

    submit(&mut editor, &["BREAK", "2,1.25", "5,0.75"]);
    assert_entities_close(
        &live(&editor),
        &[
            trace(p(1.0, 1.25), p(1.0, 0.75), p(2.0, 1.25), p(2.0, 0.75)),
            second_segment.clone(),
            trace(p(5.0, 1.25), p(5.0, 0.75), p(6.75, 1.25), p(7.25, 0.75)),
        ],
    );

    let mut editor = trace_editor(&command);
    submit(&mut editor, &["BREAK", "3,1.25", "8,1.25"]);
    assert_entities_close(
        &live(&editor),
        &[
            trace(p(1.0, 1.25), p(1.0, 0.75), p(3.0, 1.25), p(3.0, 0.75)),
            second_segment.clone(),
        ],
    );

    let mut editor = trace_editor(&command);
    submit(&mut editor, &["BREAK", "3,1.25", "F", "1,1", "3,1.25"]);
    assert!(matches!(editor.drawing().items[0], Item::Erased(_)));
    assert_entities_close(
        &live(&editor),
        &[
            second_segment,
            trace(p(3.0, 1.25), p(3.0, 0.75), p(6.75, 1.25), p(7.25, 0.75)),
        ],
    );
}

#[test]
fn trace_cuts_inside_a_miter_or_with_unknown_winding_are_rejected_atomically() {
    let mut editor = trace_editor(&["TRACE", "0.5", "1,1", "7,1", "7,6", ""]);
    let before = editor.drawing().clone();
    submit(&mut editor, &["BREAK", "3,1.25"]);
    let error = editor.submit("6.9,1.25").unwrap_err();
    assert!(error.contains("miter"), "{error}");
    assert_eq!(editor.drawing(), &before);
    assert_eq!(editor.prompt(), "BREAK: second point or F (first point)");

    for corners in [
        // p3/p4 swapped: the sides cross.
        trace(p(0.0, 0.5), p(0.0, -0.5), p(10.0, -0.5), p(10.0, 0.5)),
        // A side running backwards along the centerline.
        trace(p(0.0, 0.5), p(0.0, -0.5), p(-1.0, 0.5), p(10.0, -0.5)),
        // Degenerate centerline.
        trace(p(0.0, 0.5), p(0.0, -0.5), p(0.0, 0.5), p(0.0, -0.5)),
    ] {
        let mut editor = Editor::default();
        editor
            .drawing_mut()
            .items
            .push(Item::Entity(layered(1, corners)));
        let before = editor.drawing().clone();
        submit(&mut editor, &["BREAK", "1", "2,0"]);
        let error = editor.submit("7,0").unwrap_err();
        assert!(error.contains("TRACE"), "{error}");
        assert_eq!(editor.drawing(), &before);
    }
}

#[test]
fn trace_break_round_trips_through_dwg_with_its_erased_record() {
    let mut editor = trace_editor(&["TRACE", "1", "0,0", "10,0", ""]);
    submit(&mut editor, &["BREAK", "0,0", "4,0.5"]);
    assert!(matches!(editor.drawing().items[0], Item::Erased(_)));
    let reopened = acad_dwg::parse(&acad_dwg::write(editor.drawing()).unwrap()).unwrap();
    assert_eq!(reopened.items, editor.drawing().items);
}
