use acad_cmd::{Editor, Effect};

#[test]
fn end_requests_document_save_and_an_unnamed_path_can_be_retried_or_cancelled() {
    let mut e = Editor::default();
    assert_eq!(e.submit("END").unwrap(), Effect::End);
    e.request_end_path();
    assert_eq!(e.prompt(), "END: output file");
    assert!(e.submit("").is_err());
    assert_eq!(e.prompt(), "END: output file");
    assert_eq!(
        e.submit("drawing with spaces.dwg").unwrap(),
        Effect::SaveAndQuit("drawing with spaces.dwg".into())
    );
    assert_eq!(e.prompt(), "Command");
    e.request_end_path();
    e.cancel_command().unwrap();
    assert_eq!(e.prompt(), "Command");
}

#[test]
fn quit_only_exits_on_y_or_yes_and_leaves_the_drawing_and_undo_unchanged() {
    for answer in ["", "N", "no", "maybe", "Y", "yes", " YES "] {
        let mut e = Editor::default();
        e.submit("POINT").unwrap();
        e.submit("2,3").unwrap();
        let before = e.drawing().clone();
        e.submit("QUIT").unwrap();
        assert!(e.prompt().starts_with("QUIT: really want"));
        assert_eq!(
            e.submit(answer).unwrap(),
            if answer.trim().eq_ignore_ascii_case("Y") || answer.trim().eq_ignore_ascii_case("YES")
            {
                Effect::Quit
            } else {
                Effect::Continue
            }
        );
        assert_eq!(e.prompt(), "Command");
        assert_eq!(e.drawing(), &before);
        e.submit("UNDO").unwrap();
        assert_eq!(e.drawing().entities().count(), 0);
    }
}
