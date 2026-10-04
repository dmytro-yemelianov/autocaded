//! WBLOCK replace question (docs/native-files-menu.md, "WBLOCK overwrite
//! contract"). The editor owns no files: it reports the chosen destination
//! with `Effect::CheckWblockDestination`, and the host asks the question
//! through `ask_wblock_replace` when that destination exists. The original's
//! order, text and answer rule are asserted in
//! crates/acad-oracle/tests/files_backup.rs.
use acad_cmd::{Editor, Effect};

const QUESTION: &str = "WBLOCK: A drawing with this name already exists. Replace it? <N>";
const OPEN_QUESTION: &str = "WBLOCK: This is the open drawing's file. Replace it? <N>";
const BLOCK_NAME: &str = "WBLOCK: block name (* for entire drawing)";

fn editor_with_point() -> Editor {
    let mut editor = Editor::default();
    for input in ["POINT", "1,2"] {
        editor.submit(input).unwrap();
    }
    editor
}

#[test]
fn the_file_name_reports_its_destination_before_the_block_name() {
    let mut editor = editor_with_point();
    editor.submit("WBLOCK").unwrap();
    assert_eq!(
        editor.submit("part").unwrap(),
        Effect::CheckWblockDestination("part.DWG".into())
    );
    // Without a host answer the destination is new: the block name follows
    // and the export may only create the file, never replace one.
    assert_eq!(editor.prompt(), BLOCK_NAME);
    assert!(matches!(
        editor.submit("*").unwrap(),
        Effect::SaveDrawing(path, _) if path == "part.DWG"
    ));
}

#[test]
fn an_existing_destination_asks_and_a_leading_y_confirms_the_replacement() {
    for answer in ["Y", "y", "YES", "yep", " Y "] {
        let mut editor = editor_with_point();
        editor.submit("WBLOCK").unwrap();
        editor.submit("part").unwrap();
        editor.ask_wblock_replace(false);
        assert_eq!(editor.prompt(), QUESTION);
        assert_eq!(editor.submit(answer).unwrap(), Effect::Continue);
        assert_eq!(editor.prompt(), BLOCK_NAME, "{answer:?}");
        let Effect::ReplaceDrawing(path, drawing) = editor.submit("*").unwrap() else {
            panic!("confirmed WBLOCK must replace");
        };
        assert_eq!(path, "part.DWG");
        assert_eq!(drawing.items, editor.drawing().items);
    }
}

#[test]
fn a_confirmed_selection_export_also_replaces() {
    let mut editor = editor_with_point();
    for input in ["WBLOCK", "part"] {
        editor.submit(input).unwrap();
    }
    editor.ask_wblock_replace(false);
    for input in ["Y", "", "0,0"] {
        editor.submit(input).unwrap();
    }
    assert!(matches!(
        editor.submit("1").unwrap(),
        Effect::ReplaceDrawing(path, _) if path == "part.DWG"
    ));
}

#[test]
fn any_other_answer_ends_wblock_and_changes_nothing() {
    for answer in ["", "N", "NO", "X", "*", " "] {
        let mut editor = editor_with_point();
        let before = editor.drawing().clone();
        editor.submit("WBLOCK").unwrap();
        editor.submit("part").unwrap();
        editor.ask_wblock_replace(false);
        assert_eq!(editor.submit(answer).unwrap(), Effect::Continue);
        assert_eq!(editor.prompt(), "Command", "{answer:?}");
        assert_eq!(editor.status(), "WBLOCK: kept existing part.DWG");
        assert_eq!(editor.drawing(), &before);
        // Undo still reaches the state before the POINT, as before WBLOCK.
        editor.submit("UNDO").unwrap();
        assert!(editor.drawing().items.is_empty(), "{answer:?}");
    }
}

#[test]
fn the_open_drawing_gets_its_own_question_and_cancel_keeps_everything() {
    let mut editor = editor_with_point();
    editor.submit("WBLOCK").unwrap();
    editor.submit("part").unwrap();
    editor.ask_wblock_replace(true);
    assert_eq!(editor.prompt(), OPEN_QUESTION);
    editor.cancel_command().unwrap();
    assert_eq!(editor.prompt(), "Command");
}

#[test]
fn asking_outside_the_wblock_file_step_is_ignored() {
    let mut editor = editor_with_point();
    editor.ask_wblock_replace(false);
    assert_eq!(editor.prompt(), "Command");
    editor.submit("WBLOCK").unwrap();
    editor.submit("part").unwrap();
    editor.ask_wblock_replace(false);
    editor.submit("Y").unwrap();
    // A second request after the answer does not ask again.
    editor.ask_wblock_replace(false);
    assert_eq!(editor.prompt(), BLOCK_NAME);
}
