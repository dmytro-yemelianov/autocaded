use acad_cmd::{Editor, Effect};

// All names in the recovered dispatcher, including hardware-only topics.
const COMMANDS: &str = "LINE POINT CIRCLE SHAPE REPEAT ENDREP TEXT ARC TRACE LOAD SOLID LIST INSERT BASE ORTHO LAYER GRID LIMITS ID RES RESOLUTION ZOOM PAN MOVE ERASE MENU REDRAW STATUS REGEN DBLIST DIST CHANGE END QUIT ? AREA OOPS TABLET PLOT DELAY RESUME COPY BLOCK DIM QPLOT SNAP FILL HELP UNITS ARRAY WBLOCK AXIS HATCH FILLET BREAK SKETCH FILES";

#[test]
fn every_recovered_command_has_retained_help_without_editing_the_drawing() {
    for name in COMMANDS.split_whitespace() {
        let mut editor = Editor::default();
        for input in ["LINE", "1,2", "3,4", ""] {
            editor.submit(input).unwrap();
        }
        let before = editor.drawing().clone();
        editor.submit("HELP").unwrap();
        let Effect::Report(text) = editor.submit(&name.to_ascii_lowercase()).unwrap() else {
            panic!("missing help for {name}")
        };
        assert!(text.contains("Reference:"), "{name}: {text}");
        assert!(!text.contains('\r') && !text.contains('\u{1a}'));
        assert_eq!(editor.prompt(), "Command");
        assert_eq!(editor.status(), format!("Help for {name}"));
        assert_eq!(editor.drawing(), &before);
        editor.submit("UNDO").unwrap();
        assert_eq!(
            editor.drawing().entities().count(),
            0,
            "{name}: help added an undo step"
        );
    }
}

#[test]
fn unknown_help_keeps_the_query_active_and_a_valid_topic_recovers() {
    let mut editor = Editor::default();
    editor.submit("?").unwrap();
    assert_eq!(
        editor.submit("NO_SUCH_TOPIC").unwrap_err(),
        "unknown help topic: NO_SUCH_TOPIC"
    );
    assert_eq!(editor.prompt(), "Command name (RETURN for list)");
    let Effect::Report(text) = editor.submit(" circle ").unwrap() else {
        panic!("circle page")
    };
    assert!(text.contains("center point and radius"));
    assert!(text.contains("3P") && text.contains("2P") && text.contains("diameter"));
    assert_eq!(editor.prompt(), "Command");
}
