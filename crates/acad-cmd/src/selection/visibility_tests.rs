use super::*;
use crate::Editor;
use acad_model::{Block, Repeat};
fn line(layer: u8, x: f64, y: f64) -> Entity {
    Entity::OnLayer {
        layer,
        entity: Box::new(Entity::Line {
            start: Point { x, y },
            end: Point { x: x + 1.0, y },
        }),
    }
}
fn repeat() -> Repeat {
    Repeat {
        start_layer: 1,
        end_layer: 1,
        entities: vec![line(2, 1.0, 1.0), line(3, 20.0, 20.0)],
        columns: 2,
        rows: 1,
        column_spacing: 5.0,
        row_spacing: 0.0,
    }
}

#[test]
fn picks_and_windows_gate_bare_owner_and_visible_members_with_stable_ids() {
    let mut editor = Editor::default();
    editor.drawing_mut().items = vec![
        Item::Entity(Entity::Line {
            start: Point { x: 1.0, y: 5.0 },
            end: Point { x: 2.0, y: 5.0 },
        }),
        Item::Entity(line(2, 1.0, 1.0)),
        Item::Repeat(repeat()),
        Item::Entity(Entity::OnLayer {
            layer: 4,
            entity: Box::new(Entity::Repeat(repeat())),
        }),
    ];
    editor.drawing_mut().header.off_layers.extend([1, 3, 4]);
    let before = editor.drawing().clone();
    assert_eq!(
        selectable_items(editor.drawing())
            .map(|item| item.id)
            .collect::<Vec<_>>(),
        [1, 2, 3, 4]
    );
    assert_eq!(editor.pick_entity_at(Point { x: 1.5, y: 5.0 }, 0.1), None);
    assert_eq!(
        editor.pick_entity_at(Point { x: 6.5, y: 1.0 }, 0.1),
        Some(3)
    );
    assert_eq!(editor.pick_entity_at(Point { x: 25.5, y: 20.0 }, 0.1), None);
    assert_eq!(
        entities_in_window(
            editor.drawing(),
            Point { x: 0.0, y: 0.0 },
            Point { x: 8.0, y: 2.0 }
        ),
        [2, 3]
    );
    assert_eq!(editor.drawing(), &before);
    editor.submit("UNDO").unwrap();
    assert_eq!(
        editor.drawing(),
        &before,
        "read-only discovery adds no undo"
    );
}

#[test]
fn insert_origin_pick_requires_visible_block_geometry_and_windows_transform_bounds() {
    let mut editor = Editor::default();
    editor.drawing_mut().items = vec![
        Item::Block(Block {
            name: "B".into(),
            base: Point { x: 0.0, y: 0.0 },
            entities: vec![line(3, 1.0, 1.0)],
        }),
        Item::Entity(Entity::OnLayer {
            layer: 5,
            entity: Box::new(Entity::Insert {
                name: "B".into(),
                origin: Point { x: 50.0, y: 50.0 },
                x_scale: 2.0,
                y_scale: 3.0,
                rotation_deg: 0.0,
            }),
        }),
    ];
    editor.drawing_mut().header.off_layers.insert(3);
    assert_eq!(editor.pick_entity_at(Point { x: 50.0, y: 50.0 }, 0.1), None);
    assert!(entities_in_window(
        editor.drawing(),
        Point { x: 0.0, y: 0.0 },
        Point { x: 100.0, y: 100.0 }
    )
    .is_empty());
    let Item::Block(block) = &mut editor.drawing_mut().items[0] else {
        panic!("block")
    };
    block.entities.push(line(2, 1.0, 1.0));
    assert_eq!(
        editor.pick_entity_at(Point { x: 50.0, y: 50.0 }, 0.1),
        Some(1)
    );
    assert_eq!(
        entities_in_window(
            editor.drawing(),
            Point { x: 51.0, y: 52.0 },
            Point { x: 55.0, y: 54.0 }
        ),
        [1]
    );
    editor.drawing_mut().header.off_layers.insert(5);
    assert_eq!(editor.pick_entity_at(Point { x: 50.0, y: 50.0 }, 0.1), None);
}

#[test]
fn explicit_numeric_all_and_last_select_hidden_ids_and_remain_undoable() {
    let mut editor = Editor::default();
    editor.drawing_mut().items = vec![
        Item::Entity(line(1, 1.0, 1.0)),
        Item::Entity(line(2, 2.0, 2.0)),
    ];
    editor.drawing_mut().header.off_layers.extend([1, 2]);
    let before = editor.drawing().clone();
    assert_eq!(
        selection("ALL", selectable_count(editor.drawing())).unwrap(),
        [1, 2]
    );
    assert_eq!(
        selection("LAST", selectable_count(editor.drawing())).unwrap(),
        [2]
    );
    for selector in ["1", "LAST", "ALL"] {
        editor.submit("ERASE").unwrap();
        editor.submit(selector).unwrap();
        editor.submit("UNDO").unwrap();
        assert_eq!(editor.drawing(), &before);
    }
}

#[test]
fn huge_repeats_have_compact_visible_windows_and_no_partial_pick() {
    let mut editor = Editor::default();
    editor.drawing_mut().items = vec![Item::Repeat(Repeat {
        start_layer: 1,
        end_layer: 1,
        entities: vec![line(2, 1.0, 1.0)],
        columns: u16::MAX,
        rows: u16::MAX,
        column_spacing: -2.0,
        row_spacing: 3.0,
    })];
    assert_eq!(
        entities_in_window(
            editor.drawing(),
            Point {
                x: -200000.0,
                y: -1.0
            },
            Point {
                x: 3.0,
                y: 200000.0
            }
        ),
        [1]
    );
    assert_eq!(editor.pick_entity_at(Point { x: 1.0, y: 1.0 }, 0.1), None);
    editor.drawing_mut().header.off_layers.insert(2);
    assert!(entities_in_window(
        editor.drawing(),
        Point {
            x: -200000.0,
            y: -1.0
        },
        Point {
            x: 3.0,
            y: 200000.0
        }
    )
    .is_empty());
}
