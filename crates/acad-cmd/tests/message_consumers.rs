use acad_cmd::{
    messages::{text, Locale, MessageId},
    CmdErrorKind, Editor,
};

#[test]
fn prompt_states_use_catalog_exact_english_and_ukrainian() {
    let cases = [
        (vec![], MessageId::PromptCommand, "Command"),
        (
            vec!["LINE"],
            MessageId::PromptLineStart,
            "LINE: first point",
        ),
        (
            vec!["LINE", "0,0"],
            MessageId::PromptLineNext,
            "LINE: next point (Enter to finish)",
        ),
        (
            vec!["CIRCLE"],
            MessageId::PromptCircleCenter,
            "CIRCLE: center point (or 2P/3P)",
        ),
        (
            vec!["CIRCLE", "0,0"],
            MessageId::PromptCircleRadius,
            "CIRCLE: radius or point (D for diameter)",
        ),
        (
            vec!["CIRCLE", "0,0", "D"],
            MessageId::PromptCircleDiameter,
            "CIRCLE: diameter",
        ),
        (
            vec!["CIRCLE", "2P"],
            MessageId::PromptCircleTwoPointFirst,
            "CIRCLE 2P: first diameter endpoint",
        ),
        (
            vec!["CIRCLE", "2P", "0,0"],
            MessageId::PromptCircleTwoPointSecond,
            "CIRCLE 2P: second diameter endpoint",
        ),
        (
            vec!["CIRCLE", "3P"],
            MessageId::PromptCircleThreePointFirst,
            "CIRCLE 3P: first point",
        ),
        (
            vec!["CIRCLE", "3P", "0,0"],
            MessageId::PromptCircleThreePointSecond,
            "CIRCLE 3P: second point",
        ),
        (
            vec!["CIRCLE", "3P", "0,0", "2,0"],
            MessageId::PromptCircleThreePointThird,
            "CIRCLE 3P: third point",
        ),
    ];
    for (inputs, id, expected) in cases {
        let mut editor = Editor::default();
        for input in inputs {
            editor.submit(input).unwrap();
        }
        assert_eq!(editor.prompt(), expected);
        assert_eq!(
            editor.prompt_for_locale(Locale::En),
            text(id, Locale::En).unwrap()
        );
        assert_eq!(
            editor.prompt_for_locale(Locale::Uk),
            text(id, Locale::Uk).unwrap()
        );
        assert_eq!(editor.command_idle(), id == MessageId::PromptCommand);
        let before = editor.prompt().to_owned();
        if !editor.command_idle() {
            assert!(editor.submit("bad answer").is_err());
            assert_eq!(editor.prompt(), before, "retry {id:?}");
        }
        editor.cancel_command().unwrap();
        assert!(editor.command_idle());
    }
}

#[test]
fn unknown_errors_are_typed_at_dispatch_and_do_not_change_return_history() {
    let mut editor = Editor::default();
    assert_eq!(
        editor.submit(" MiXeD{command} ").unwrap_err(),
        "unknown command: MiXeD{command}"
    );
    let raw = editor.command_diagnostic().unwrap();
    assert_eq!(raw.kind, CmdErrorKind::UnknownCommand);
    assert_eq!(raw.message.id(), MessageId::ErrorCommandUnknown);
    assert_eq!(
        raw.message.render(Locale::Uk),
        "невідома команда: MiXeD{command}"
    );
    assert_eq!(
        editor.execute("other").unwrap_err().kind,
        CmdErrorKind::UnknownCommand
    );
    assert_eq!(
        editor.submit_return("").unwrap_err(),
        "Unknown command. Type ? for list of commands."
    );
    assert_eq!(
        editor.command_diagnostic().unwrap().message.id(),
        MessageId::ErrorCommandUnknownReturn
    );
    editor.submit("LINE").unwrap();
    editor.submit("0,0").unwrap();
    editor.submit("1,1").unwrap();
    editor.submit("").unwrap();
    assert!(editor.command_diagnostic().is_none());
    assert!(
        editor.submit_return("").is_err(),
        "raw macro does not set Return history"
    );
    editor.submit_return("LINE").unwrap();
    editor.cancel_command().unwrap();
    assert!(editor.submit_return("bad").is_err());
    editor.submit_return("").unwrap();
    assert_eq!(
        editor.prompt(),
        "LINE: first point",
        "unknown did not replace history"
    );
    assert!(editor.command_diagnostic().is_none());
    assert_ne!(
        editor.execute("invalid").unwrap_err().kind,
        CmdErrorKind::UnknownCommand
    );
    assert!(editor.command_diagnostic().is_none());
}
