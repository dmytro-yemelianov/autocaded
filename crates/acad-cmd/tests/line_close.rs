//! LINE C behavior specified by the retained ACAD.HLP LINE page.
use acad_cmd::Editor;
use acad_model::{Entity, Point};

fn bare(entity: &Entity) -> &Entity {
    match entity {
        Entity::OnLayer { entity, .. } => bare(entity),
        other => other,
    }
}
fn input(editor: &mut Editor, values: &[&str]) {
    for value in values {
        editor.submit(value).unwrap();
    }
}

#[test]
fn close_returns_to_the_exact_first_vertex_and_undo_removes_only_the_closing_edge() {
    let mut editor = Editor::default();
    input(&mut editor, &["LINE", "1.13,2.17", "5,2.17", "5,6", "c"]);
    assert_eq!(editor.prompt(), "Command");
    assert_eq!(editor.drawing().entities().count(), 3);
    let Entity::Line { start, end } = bare(editor.drawing().entities().last().unwrap()) else {
        panic!("closing line")
    };
    assert_eq!(
        (*start, *end),
        (Point { x: 5.0, y: 6.0 }, Point { x: 1.13, y: 2.17 })
    );
    let reopened = acad_dwg::parse(&acad_dwg::write(editor.drawing()).unwrap()).unwrap();
    assert_eq!(reopened.items, editor.drawing().items);
    editor.submit("UNDO").unwrap();
    assert_eq!(editor.drawing().entities().count(), 2);
}

#[test]
fn close_does_not_duplicate_an_existing_closing_edge_or_add_an_undo_step() {
    for vertices in [&["1,2"][..], &["1,2", "3,4", "1,2"]] {
        let mut editor = Editor::default();
        editor.submit("LINE").unwrap();
        input(&mut editor, vertices);
        let before = editor.drawing().clone();
        let mut control = editor.clone();
        editor.submit("C").unwrap();
        assert_eq!(editor.drawing(), &before);
        assert_eq!(editor.prompt(), "Command");
        editor.submit("UNDO").unwrap();
        control.cancel_command().unwrap();
        control.submit("UNDO").unwrap();
        assert_eq!(editor.drawing(), control.drawing());
    }
}

#[test]
fn closure_tracks_each_new_sequence_and_survives_invalid_input_and_mouse_constraints() {
    let mut editor = Editor::default();
    input(
        &mut editor,
        &["SNAP", "0.5", "ORTHO", "ON", "LINE", "1.13,2.17"],
    );
    editor.submit_mouse_point(Point { x: 5.1, y: 3.0 }).unwrap();
    assert!(editor.submit("bad point").is_err());
    editor.submit_mouse_point(Point { x: 5.2, y: 6.1 }).unwrap();
    editor.submit("C").unwrap();
    let Entity::Line { start, end } = bare(editor.drawing().entities().last().unwrap()) else {
        panic!("closing line")
    };
    assert_eq!(
        (*start, *end),
        (Point { x: 5.0, y: 6.0 }, Point { x: 1.13, y: 2.17 })
    );
    input(&mut editor, &["LINE", "-2,-3", "-4,-5", "C"]);
    let Entity::Line { end, .. } = bare(editor.drawing().entities().last().unwrap()) else {
        panic!("second closing line")
    };
    assert_eq!(*end, Point { x: -2.0, y: -3.0 });
}

#[test]
fn closed_line_sequence_is_an_immediately_usable_hatch_boundary() {
    let mut editor = Editor::default();
    input(
        &mut editor,
        &[
            "LINE", "0,0", "1,0", "1,1", "0,1", "C", "HATCH", "NET", "1", "0", "ALL",
        ],
    );
    assert_eq!(editor.drawing().blocks().next().unwrap().entities.len(), 16);
    assert_eq!(editor.prompt(), "Command");
}
