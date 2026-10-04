//! Shared selector contracts. Original point precision/report layout is not inferred.
use acad_cmd::{selectable_items, Editor, Effect};
use acad_model::{Block, Entity, Item, Point, Repeat};
fn p(x: f64, y: f64) -> Point {
    Point { x, y }
}
fn layered(layer: u8, entity: Entity) -> Entity {
    Entity::OnLayer {
        layer,
        entity: Box::new(entity),
    }
}
fn submit(editor: &mut Editor, inputs: &[&str]) {
    for input in inputs {
        editor.submit(input).unwrap();
    }
}
fn fixture() -> Editor {
    let mut editor = Editor::default();
    editor.drawing_mut().header.off_layers.insert(2);
    editor.drawing_mut().items = vec![
        Item::Entity(layered(
            1,
            Entity::Line {
                start: p(-4.0, -2.0),
                end: p(-2.0, -2.0),
            },
        )),
        Item::Entity(layered(
            1,
            Entity::Circle {
                center: p(-1.0, 1.0),
                radius: 0.5,
            },
        )),
        Item::Entity(layered(
            2,
            Entity::Point {
                origin: p(-3.0, -1.0),
            },
        )),
        Item::Repeat(Repeat {
            start_layer: 1,
            end_layer: 1,
            entities: vec![
                layered(
                    1,
                    Entity::Point {
                        origin: p(-1.0, -1.0),
                    },
                ),
                layered(
                    2,
                    Entity::Point {
                        origin: p(100.0, 100.0),
                    },
                ),
            ],
            columns: 2,
            rows: 1,
            column_spacing: 2.0,
            row_spacing: 0.0,
        }),
        Item::Block(Block {
            name: "B".into(),
            base: p(1.0, 1.0),
            entities: vec![layered(
                1,
                Entity::Line {
                    start: p(1.0, 1.0),
                    end: p(3.0, 1.0),
                },
            )],
        }),
        Item::Entity(layered(
            1,
            Entity::Insert {
                origin: p(4.0, 0.0),
                x_scale: 2.0,
                y_scale: -1.0,
                rotation_deg: 90.0,
                name: "B".into(),
            },
        )),
        Item::Entity(layered(
            1,
            Entity::Point {
                origin: p(10.0, 10.0),
            },
        )),
    ];
    editor
}
fn line_window(editor: &mut Editor) {
    submit(editor, &["W", "-4.1,-2.1", "-1.9,-1.9"]);
}

#[test]
fn selected_list_window_keeps_visible_group_insert_owners_and_global_ids() {
    let mut editor = fixture();
    editor.drawing_mut().header.snap.on = true;
    editor.drawing_mut().header.snap.spacing = 100.0;
    editor.drawing_mut().header.ortho = true;
    let before = editor.drawing().clone();
    assert_eq!(editor.submit("LIST").unwrap(), Effect::Continue);
    assert!(editor.prompt().starts_with("LIST:"));
    submit(&mut editor, &["WINDOW"]);
    assert_eq!(
        editor.constrain_mouse_point(p(-5.0, -3.0)).unwrap(),
        p(-5.0, -3.0)
    );
    editor.submit_mouse_point(p(-5.0, -3.0)).unwrap();
    assert_eq!(
        editor.constrain_mouse_point(p(5.0, 5.0)).unwrap(),
        p(5.0, 5.0)
    );
    editor.submit("@10,8").unwrap();
    assert_eq!(editor.collected_selection(), &[1, 2, 4, 5]);
    assert_eq!(editor.drawing(), &before, "corners only collect objects");
    let Effect::Report(report) = editor.submit("").unwrap() else {
        panic!("selected LIST");
    };
    assert!(report.starts_with("1 LINE, 2 CIRCLE, 4 REPEAT, 5 INSERT\n"));
    assert!(report.contains("\n4: members=2 columns=2 rows=1"));
    assert!(report
        .contains("\n5: layer=1 origin=(4.0000,0.0000) X scale=2 Y scale=-1 angle=90 block=\"B\""));
    assert!(!report.contains("\n3:") && !report.contains("\n6:"));
    assert_eq!(editor.prompt(), "Command");
    submit(&mut editor, &["UNDO"]);
    assert_eq!(
        editor.drawing(),
        &before,
        "LIST/selection adds no undo step"
    );
}

#[test]
fn typed_selection_inspects_hidden_objects_and_replaces_collected_set() {
    let mut editor = fixture();
    submit(&mut editor, &["LIST"]);
    assert_eq!(editor.pick_selection_at(p(-3.0, -1.0), 0.0).unwrap(), None);
    assert_eq!(
        editor.pick_selection_at(p(-4.0, -2.0), 0.0).unwrap(),
        Some(1)
    );
    assert_eq!(
        editor.pick_selection_at(p(-2.0, -2.0), 0.0).unwrap(),
        Some(1)
    );
    assert_eq!(editor.collected_selection(), &[1]);
    assert!(editor.submit("invalid").is_err());
    assert_eq!(editor.collected_selection(), &[1]);
    let Effect::Report(report) = editor.submit("3,3").unwrap() else {
        panic!("LIST");
    };
    assert!(report.starts_with("3 POINT\n3: layer=2"));
    assert!(!report.contains("1 LINE"));
    submit(&mut editor, &["LIST"]);
    let Effect::Report(report) = editor.submit("LAST").unwrap() else {
        panic!("LIST");
    };
    assert!(report.starts_with("6 POINT\n"));
    submit(&mut editor, &["LIST"]);
    let Effect::Report(report) = editor.submit("ALL").unwrap() else {
        panic!("LIST");
    };
    assert!(report.starts_with("1 LINE, 2 CIRCLE, 3 POINT, 4 REPEAT, 5 INSERT, 6 POINT\n"));
}

#[test]
fn visible_picks_windows_deduplicate_and_cancel_without_editing() {
    let mut editor = fixture();
    let before = editor.drawing().clone();
    submit(&mut editor, &["ERASE"]);
    editor.pick_selection_at(p(-4.0, -2.0), 0.0).unwrap();
    line_window(&mut editor);
    assert_eq!(editor.collected_selection(), &[1]);
    submit(&mut editor, &["W", "9,9", "11,11"]);
    editor.pick_selection_at(p(10.0, 10.0), 0.0).unwrap();
    assert_eq!(editor.collected_selection(), &[1, 6]);
    assert_eq!(editor.drawing(), &before);
    editor.cancel_command().unwrap();
    assert!(editor.collected_selection().is_empty());
    submit(&mut editor, &["UNDO"]);
    assert_eq!(editor.drawing(), &before);
    submit(&mut editor, &["ERASE"]);
    editor.pick_selection_at(p(-4.0, -2.0), 0.0).unwrap();
    editor.pick_selection_at(p(10.0, 10.0), 0.0).unwrap();
    editor.submit_return("").unwrap();
    assert_eq!(selectable_items(editor.drawing()).count(), 4);
    submit(&mut editor, &["UNDO"]);
    assert_eq!(editor.drawing(), &before);
}

#[test]
fn window_retry_and_partial_geometry_exclusion_preserve_pending_selection() {
    let mut editor = fixture();
    let before = editor.drawing().clone();
    submit(&mut editor, &["LIST", "W"]);
    assert!(editor.submit("bad").is_err());
    assert!(editor.prompt().contains("first corner"));
    submit(&mut editor, &["-4.5,-2.5"]);
    assert!(editor.submit("@1e309,0").is_err());
    assert!(
        editor
            .submit("-3.0,-1.5")
            .unwrap_err()
            .contains("no visible objects"),
        "window containing one endpoint must exclude the whole LINE"
    );
    assert!(editor.prompt().contains("opposite corner"));
    submit(&mut editor, &["-1.5,-1.5"]);
    assert_eq!(editor.collected_selection(), &[1]);
    assert_eq!(editor.drawing(), &before);
    editor.cancel_command().unwrap();
    submit(&mut editor, &["LIST", ""]);
    assert_eq!(editor.prompt(), "Command");
    assert_eq!(editor.drawing(), &before);
}

#[test]
fn incompatible_hatch_window_can_retry_by_typed_replacement_or_cancel() {
    let mut editor = fixture();
    let before = editor.drawing().clone();
    submit(&mut editor, &["HATCH", "LINE", "1", "0", "W", "-5,-3"]);
    assert!(editor.submit("5,5").is_err());
    assert!(editor.accepts_mouse_selection());
    assert_eq!(editor.collected_selection(), &[1, 2, 4, 5]);
    assert_eq!(editor.drawing(), &before);
    submit(&mut editor, &["2"]);
    assert!(editor.drawing().block("*X1").is_some());
    submit(&mut editor, &["UNDO"]);
    assert_eq!(editor.drawing(), &before);
    submit(&mut editor, &["HATCH", "LINE", "1", "0", "W", "-5,-3"]);
    assert!(editor.submit("5,5").is_err());
    editor.cancel_command().unwrap();
    submit(&mut editor, &["UNDO"]);
    assert_eq!(editor.drawing(), &before);
}

#[test]
fn common_windows_preserve_displacement_and_domain_continuations() {
    let mut editor = fixture();
    let before = editor.drawing().clone();
    submit(&mut editor, &["MOVE", "0,0", "2,-1"]);
    line_window(&mut editor);
    assert_eq!(editor.drawing(), &before);
    submit(&mut editor, &[""]);
    assert!(
        matches!(&editor.drawing().items[0], Item::Entity(Entity::OnLayer { entity, .. }) if matches!(entity.as_ref(), Entity::Line { start, end } if *start == p(-2.0,-3.0) && *end == p(0.0,-3.0)))
    );
    assert_eq!(editor.drawing().header.view, before.header.view);
    submit(&mut editor, &["UNDO"]);
    assert_eq!(editor.drawing(), &before);
    submit(&mut editor, &["COPY", "1,2", ""]);
    line_window(&mut editor);
    submit(&mut editor, &[""]);
    assert_eq!(selectable_items(editor.drawing()).count(), 7);
    assert_eq!(editor.pick_entity_at(p(-3.0, 0.0), 0.0), Some(7));
    submit(&mut editor, &["UNDO"]);
    for (prefix, next_prompt) in [
        (vec!["ROTATE"], "ROTATE: base point x,y"),
        (vec!["SCALE"], "SCALE: base point x,y"),
        (vec!["ARRAY"], "ARRAY: rectangular or circular (R/C)"),
        (vec!["CHANGE"], "CHANGE: intersection point or L"),
        (vec!["BREAK"], "BREAK: first point"),
    ] {
        submit(&mut editor, &prefix);
        line_window(&mut editor);
        submit(&mut editor, &[""]);
        assert_eq!(editor.prompt(), next_prompt);
        assert_eq!(editor.drawing(), &before);
        editor.cancel_command().unwrap();
    }
}

#[test]
fn block_wblock_and_entity_area_use_the_same_window_selector() {
    let mut editor = fixture();
    let before = editor.drawing().clone();
    submit(&mut editor, &["WBLOCK", "window.dwg", "", "0,0"]);
    line_window(&mut editor);
    let Effect::SaveDrawing(_, output) = editor.submit("").unwrap() else {
        panic!("WBLOCK");
    };
    assert_eq!(output.items, vec![before.items[0].clone()]);
    assert_eq!(editor.drawing(), &before);
    submit(&mut editor, &["BLOCK", "WINDOW", "0,0"]);
    line_window(&mut editor);
    submit(&mut editor, &[""]);
    assert_eq!(
        editor.drawing().block("WINDOW").unwrap().entities,
        vec![match &before.items[0] {
            Item::Entity(entity) => entity.clone(),
            _ => unreachable!(),
        }]
    );
    submit(&mut editor, &["UNDO"]);
    assert_eq!(editor.drawing(), &before);
    submit(
        &mut editor,
        &["ENTITYAREA", "W", "-1.5,0.5", "-0.5,1.5", ""],
    );
    assert!(editor
        .status()
        .starts_with("Area=0.785398, Perimeter=3.141593"));
    assert_eq!(editor.drawing(), &before);
}

#[test]
fn fillet_cardinality_failure_keeps_selector_retryable_without_undo() {
    let mut editor = fixture();
    let before = editor.drawing().clone();
    submit(&mut editor, &["FILLET"]);
    line_window(&mut editor);
    assert!(editor.submit("").unwrap_err().contains("exactly two"));
    assert!(editor.submit("1,2,3").unwrap_err().contains("exactly two"));
    assert_eq!(editor.collected_selection(), &[1, 2, 3]);
    assert!(
        editor.submit("1,2").is_err(),
        "circle remains outside FILLET supported geometry"
    );
    assert_eq!(editor.drawing(), &before);
    editor.cancel_command().unwrap();
    submit(&mut editor, &["UNDO"]);
    assert_eq!(editor.drawing(), &before);
}
