//! Native multi-object CHANGE contract (docs/native-change.md): reverse
//! visiting order, one shared INSERT angle, TEXT erase-and-append, skipped
//! kinds, group refusal, atomic staging and one UNDO. Prompt order and
//! geometry are pinned against the original in
//! acad-oracle/tests/insert_change.rs.
use acad_cmd::Editor;
use acad_model::{Entity, Item, Point, Repeat};

fn submit(e: &mut Editor, inputs: &[&str]) {
    for input in inputs {
        e.submit(input)
            .unwrap_or_else(|error| panic!("{input}: {error}"));
    }
}

const SETUP: [&str; 19] = [
    "LINE", "0,0", "1,0", "", "BLOCK", "B", "0,0", "LAST", "ARC", "4,3", "3,4", "2,3", "POINT",
    "5,1", "TEXT", "2,1", "0.5", "0", "T1",
];

#[test]
fn every_answer_is_staged_and_cancel_or_retry_changes_nothing() {
    let mut e = Editor::default();
    submit(&mut e, &SETUP);
    submit(
        &mut e,
        &["INSERT", "B", "4,1", "1", "", "", "LINE", "6,1", "7,1", ""],
    );
    let before = e.drawing().clone();
    // Reverse order: LINE (silent), INSERT angle, then the TEXT prompts.
    submit(&mut e, &["CHANGE", "ALL", "3,5"]);
    assert!(
        e.prompt().starts_with("CHANGE: new angle"),
        "{}",
        e.prompt()
    );
    submit(&mut e, &["45"]);
    assert!(e.prompt().contains("new height"));
    assert!(e.submit("0").is_err() && e.submit("x").is_err());
    submit(&mut e, &["2", "@1,1"]);
    assert!(e.prompt().contains("new text"));
    assert_eq!(e.drawing(), &before);
    e.cancel_command().unwrap();
    assert_eq!(e.drawing(), &before);

    submit(&mut e, &["CHANGE", "ALL", "3,5", "45", "2", "@1,1", "NEW"]);
    let items = &e.drawing().items;
    assert_eq!(items.len(), before.items.len() + 1);
    // ARC and POINT are left alone, as the original does.
    assert_eq!(items[2], before.items[2]);
    assert_eq!(items[3], before.items[3]);
    assert!(matches!(items[4], Item::Erased(_)), "old TEXT erased");
    let Some(Item::Entity(Entity::OnLayer { entity, .. })) = items.last() else {
        panic!()
    };
    let Entity::Text {
        origin,
        height,
        rotation_deg,
        value,
    } = entity.as_ref()
    else {
        panic!()
    };
    assert_eq!(
        (*origin, *height, value.as_str()),
        (Point { x: 3.0, y: 5.0 }, 2.0, "NEW")
    );
    assert!(
        (rotation_deg - 45.0).abs() < 1e-9,
        "angle point from the new origin"
    );
    submit(&mut e, &["UNDO"]);
    assert_eq!(e.drawing(), &before, "one UNDO");
}

#[test]
fn only_skipped_kinds_change_nothing_and_add_no_undo() {
    let mut e = Editor::default();
    submit(&mut e, &["ARC", "4,3", "3,4", "2,3", "POINT", "5,1"]);
    let before = e.drawing().clone();
    submit(&mut e, &["CHANGE", "ALL", "3,5"]);
    assert!(e.prompt().starts_with("Command"));
    assert_eq!(e.drawing(), &before);
    submit(&mut e, &["UNDO"]);
    assert_eq!(
        e.drawing().items,
        before.items[..1],
        "UNDO removed the POINT, not a CHANGE step"
    );
}

#[test]
fn blank_point_keeps_locations_and_still_asks_properties() {
    let mut e = Editor::default();
    submit(&mut e, &SETUP);
    submit(&mut e, &["INSERT", "B", "4,1", "1", "", "10"]);
    let before = e.drawing().clone();
    submit(&mut e, &["CHANGE", "ALL", "", "30", "", "", ""]);
    let Item::Entity(Entity::OnLayer { entity, .. }) = e.drawing().items.last().unwrap() else {
        panic!()
    };
    assert!(
        matches!(entity.as_ref(), Entity::Insert { origin: Point { x: 4.0, y: 1.0 }, rotation_deg, .. } if *rotation_deg == 30.0)
    );
    assert_eq!(e.drawing().items[..5], before.items[..5]);
}

#[test]
fn groups_in_a_mixed_selection_refuse_atomically_in_both_representations() {
    for entity_form in [false, true] {
        let mut e = Editor::default();
        submit(&mut e, &["LINE", "7,1", "8,1", ""]);
        let repeat = Repeat {
            start_layer: 1,
            end_layer: 1,
            entities: vec![Entity::OnLayer {
                layer: 1,
                entity: Box::new(Entity::Line {
                    start: Point { x: 1.0, y: 1.0 },
                    end: Point { x: 2.0, y: 1.0 },
                }),
            }],
            columns: 2,
            rows: 1,
            column_spacing: 1.0,
            row_spacing: 1.0,
        };
        e.drawing_mut().items.push(if entity_form {
            Item::Entity(Entity::Repeat(repeat))
        } else {
            Item::Repeat(repeat)
        });
        let before = e.drawing().clone();
        submit(&mut e, &["CHANGE", "ALL"]);
        let error = e.submit("3,5").unwrap_err();
        assert!(error.contains("REPEAT"), "{error}");
        assert_eq!(e.drawing(), &before);
        // CHANGE L keeps the whole-owner layer rule for groups.
        submit(&mut e, &["L", "2"]);
        assert_ne!(e.drawing(), &before);
    }
}
