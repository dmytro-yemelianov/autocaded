//! E2: a layer applied to a whole REPEAT group (docs/native-group-persistence.md,
//! "E2"). As in AutoCAD 1.4 (oracle `repeat_layer.rs`), CHANGE layer rewrites
//! the member records and leaves the REPEAT/ENDREP marker layers alone; the
//! marker layer gates neither visibility nor selection.
use acad_cmd::Editor;
use acad_dwg::header::Version;
use acad_model::{Drawing, Entity, Item, Point, Repeat};

fn submit(editor: &mut Editor, inputs: &[&str]) {
    for input in inputs {
        editor.submit(input).unwrap();
    }
}
fn on(layer: u8, entity: Entity) -> Entity {
    Entity::OnLayer {
        layer,
        entity: Box::new(entity),
    }
}
fn line(y: f64) -> Entity {
    Entity::Line {
        start: Point { x: 1.0, y },
        end: Point { x: 2.0, y },
    }
}
fn group(start_layer: u8, end_layer: u8, entities: Vec<Entity>) -> Repeat {
    Repeat {
        start_layer,
        end_layer,
        entities,
        columns: 2,
        rows: 1,
        column_spacing: 5.0,
        row_spacing: 0.0,
    }
}
/// The oracle fixture: markers drawn on layer 2, members on layer 1.
const FIXTURE: [&str; 17] = [
    "LAYER 2", "REPEAT", "LAYER 1", "LINE", "1,1", "2,1", "", "LINE", "1,3", "2,3", "", "LAYER 2",
    "ENDREP", "2", "1", "5", "0",
];
fn fixture() -> Editor {
    let mut editor = Editor::default();
    submit(&mut editor, &FIXTURE);
    assert_eq!(
        editor.drawing().items,
        [Item::Repeat(group(
            2,
            2,
            vec![on(1, line(1.0)), on(1, line(3.0))]
        ))],
        "{}",
        editor.status()
    );
    editor
}
fn reopen(drawing: &Drawing) -> Vec<Vec<Item>> {
    // DXF save/reopen is covered by acad-app `repeat_owner_layer.rs`.
    [Version::Ac12, Version::Ac140]
        .into_iter()
        .map(|version| {
            acad_dwg::parse(&acad_dwg::write_version(drawing, version).unwrap())
                .unwrap()
                .items
        })
        .collect()
}

#[test]
fn change_layer_rewrites_members_keeps_markers_and_saves_as_the_original() {
    let mut editor = fixture();
    let before = editor.drawing().clone();
    submit(&mut editor, &["CHANGE", "W", "0,0", "10,5", "", "L", "3"]);
    // The original's file for the same keys (oracle RLCHG): members 3, markers 2.
    let expected = [Item::Repeat(group(
        2,
        2,
        vec![on(3, line(1.0)), on(3, line(3.0))],
    ))];
    assert_eq!(editor.drawing().items, expected);
    for items in reopen(editor.drawing()) {
        assert_eq!(items, expected);
    }
    // One UNDO step restores the members' layers.
    submit(&mut editor, &["UNDO"]);
    assert_eq!(editor.drawing(), &before);
}

#[test]
fn marker_layer_off_still_shows_and_selects_members_member_layer_off_hides_them() {
    let mut editor = fixture();
    submit(&mut editor, &["LAYER OFF 2"]);
    let header = &editor.drawing().header;
    assert!(header.item_is_visible(&editor.drawing().items[0]));
    submit(&mut editor, &["ERASE", "W", "0,0", "10,5", ""]);
    assert!(
        matches!(&editor.drawing().items[0], Item::Erased(_)),
        "marker layer OFF: window still finds the group"
    );
    let mut editor = fixture();
    submit(&mut editor, &["LAYER OFF 1"]);
    let header = &editor.drawing().header;
    assert!(!header.item_is_visible(&editor.drawing().items[0]));
    let before = editor.drawing().clone();
    submit(&mut editor, &["ERASE", "W", "0,0"]);
    let error = editor.submit("10,5").unwrap_err();
    assert!(error.contains("no visible objects"), "{error}");
    editor.cancel_command().unwrap();
    assert_eq!(editor.drawing(), &before, "member layer OFF: nothing found");
    // After CHANGE the members' new layer gates them; the markers still do not.
    let mut editor = fixture();
    submit(
        &mut editor,
        &["CHANGE", "W", "0,0", "10,5", "", "L", "3", "LAYER OFF 2"],
    );
    assert!(editor
        .drawing()
        .header
        .item_is_visible(&editor.drawing().items[0]));
    submit(&mut editor, &["LAYER ON 2", "LAYER OFF 3"]);
    assert!(!editor
        .drawing()
        .header
        .item_is_visible(&editor.drawing().items[0]));
}

#[test]
fn change_layer_drops_an_explicit_owner_and_saves_what_the_session_shows() {
    // A library-built owner over members on other layers: refused as it is.
    let owned = Item::Entity(on(
        4,
        Entity::Repeat(group(
            2,
            5,
            vec![
                on(1, line(1.0)),
                on(6, Entity::Repeat(group(7, 8, vec![on(6, line(3.0))]))),
                Entity::Erased(Box::new(on(1, line(4.0)))),
            ],
        )),
    ));
    let mut editor = Editor::default();
    editor.drawing_mut().items = vec![owned.clone()];
    for version in [Version::Ac12, Version::Ac140] {
        let error = acad_dwg::write_version(editor.drawing(), version).unwrap_err();
        assert!(error.to_string().contains("owner layer"), "{error}");
    }
    assert_eq!(editor.drawing().items, [owned.clone()]);
    submit(&mut editor, &["CHANGE", "1", "L", "3"]);
    // Every member record, nested and erased ones too, now carries layer 3;
    // the owners are gone and all marker layers are unchanged.
    let changed = group(
        2,
        5,
        vec![
            on(3, line(1.0)),
            Entity::Repeat(group(7, 8, vec![on(3, line(3.0))])),
            Entity::Erased(Box::new(on(3, line(4.0)))),
        ],
    );
    assert_eq!(editor.drawing().items, [Item::Repeat(changed)]);
    let reopened = reopen(editor.drawing());
    assert_eq!(reopened[0], editor.drawing().items);
    assert_eq!(reopened[1], editor.drawing().items);
    submit(&mut editor, &["UNDO"]);
    assert_eq!(editor.drawing().items, [owned]);
}

#[test]
fn interactive_repeat_with_erased_member_and_empty() {
    let mut editor = Editor::default();
    submit(
        &mut editor,
        &[
            "REPEAT", "LINE", "1,1", "2,1", "", "ERASE", "L", "", "ENDREP", "2", "1", "5", "0",
        ],
    );
    assert_eq!(
        editor.drawing().items,
        [Item::Repeat(group(
            1,
            1,
            vec![Entity::Erased(Box::new(on(1, line(1.0))))]
        ))]
    );
    let reopened = reopen(editor.drawing());
    assert_eq!(reopened[0], editor.drawing().items);
    assert_eq!(reopened[1], editor.drawing().items);

    let mut editor2 = Editor::default();
    submit(&mut editor2, &["REPEAT", "ENDREP", "2", "1", "5", "0"]);
    assert_eq!(editor2.drawing().items, [Item::Repeat(group(1, 1, vec![]))]);
    let reopened2 = reopen(editor2.drawing());
    assert_eq!(reopened2[0], editor2.drawing().items);
    assert_eq!(reopened2[1], editor2.drawing().items);
}
