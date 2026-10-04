use acad_cmd::{Editor, Effect};

fn report(editor: &mut Editor) -> String {
    let Effect::Report(text) = editor.submit("STATUS").unwrap() else {
        panic!("STATUS report")
    };
    assert_eq!(editor.prompt(), "Command");
    assert_eq!(editor.status(), "Drawing status");
    text
}

#[test]
fn status_describes_actual_extents_and_modes_without_mutation_or_undo_entry() {
    let mut editor = Editor::default();
    for input in [
        "LINE", "1,2", "5,6", "", "SNAP", "0.5", "ORTHO", "ON", "FILL", "OFF", "GRID", "0.25",
        "LAYER", "4", "BASE", "3,4",
    ] {
        editor.submit(input).unwrap();
    }
    let before = editor.drawing().clone();
    let mut control = editor.clone();
    let text = report(&mut editor);
    for line in [
        "EXTENTS: 1.0000,2.0000 to 5.0000,6.0000",
        "LIMITS: 0.0000,0.0000 to 12.0000,9.0000",
        "BASE: 3.0000,4.0000",
        "SNAP: ON  spacing=0.5000",
        "GRID: ON  spacing=0.2500",
        "ORTHO: ON",
        "FILL: OFF",
        "LAYER: 4",
        "UNITS: Decimal  precision=4",
    ] {
        assert!(text.contains(line), "{line}: {text}");
    }
    assert_eq!(editor.drawing(), &before);
    editor.submit("UNDO").unwrap();
    control.submit("UNDO").unwrap();
    assert_eq!(editor.drawing(), control.drawing());
}

#[test]
fn status_uses_selected_units_and_only_reports_an_explicit_dim_arrow() {
    let mut editor = Editor::default();
    assert!(!report(&mut editor).contains("DIMARROW"));
    for input in ["UNITS", "2", "2", "DIM", "0,0", "0,2", "5,0", ""] {
        editor.submit(input).unwrap();
    }
    let text = report(&mut editor);
    assert!(text.contains("UNITS: Decimal  precision=2"));
    assert!(text.contains("DIMARROW: 0.14"));
    assert!(text.contains("BASE: 0.00,0.00"));
}
