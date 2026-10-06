//! Independent baseline contracts for the bounded D9 center-choice experiment.
use acad_cmd::{messages::Locale, Editor};

const CENTER: &str = "ARC: end direction point (A for angle, L for chord)";
fn run(editor: &mut Editor, inputs: &[&str]) {
    for input in inputs {
        editor.submit(input).unwrap();
    }
}
fn center_choice() -> Editor {
    let mut editor = Editor::default();
    run(&mut editor, &["POINT", "9,9", "ARC", "5,0", "C", "0,0"]);
    editor
}

#[test]
fn center_choices_keep_ascii_tokens_exact_prompts_and_point_fallback() {
    for (tokens, prompt, answer) in [
        (["A", "a", " a "], "ARC: included angle", "90"),
        (["L", "l", " l "], "ARC: chord length", "7.0710678118654755"),
    ] {
        let mut expected = center_choice();
        run(&mut expected, &[tokens[0], answer]);
        for token in tokens {
            let mut editor = center_choice();
            for locale in [Locale::En, Locale::Uk] {
                assert_eq!(editor.prompt_for_locale(locale), CENTER);
            }
            editor.submit(token).unwrap();
            assert_eq!(editor.prompt(), prompt);
            editor.submit(answer).unwrap();
            assert_eq!(editor.drawing(), expected.drawing());
            assert_eq!(editor.prompt(), "Command");
        }
    }
    for point in ["0,5", "@-5,5", "@5<135"] {
        let mut editor = center_choice();
        editor.submit(point).unwrap();
        assert_eq!(editor.prompt(), "Command");
        editor.submit("UNDO").unwrap();
        assert_eq!(editor.drawing(), center_choice().drawing());
    }
}

#[test]
fn center_choice_unknown_tokens_and_empty_return_retry_without_mutation() {
    let mut editor = center_choice();
    let before = editor.drawing().clone();
    for (input, error) in [
        ("R", "expected x,y point: R"),
        ("D", "expected x,y point: D"),
        ("ANGLE", "expected x,y point: ANGLE"),
        ("А", "expected x,y point: А"), // Cyrillic A is user input, not an alias.
        ("90", "expected x,y point: 90"),
        ("", "expected x,y point: "),
        (
            "0,0",
            "ARC end direction needs a point distinct from its center",
        ),
    ] {
        assert_eq!(editor.submit_return(input).unwrap_err(), error);
        assert_eq!(editor.prompt(), CENTER);
        assert_eq!(editor.drawing(), &before);
    }
    editor.submit("a").unwrap();
    let angle_prompt = editor.prompt().to_owned();
    assert_eq!(editor.submit("bad").unwrap_err(), "invalid number: bad");
    assert!(editor.submit("0").is_err());
    assert_eq!(editor.prompt(), angle_prompt);
    assert_eq!(editor.drawing(), &before);
    editor.submit_return("90").unwrap();
    editor.submit("UNDO").unwrap();
    assert_eq!(editor.drawing(), &before);
    editor.submit("UNDO").unwrap();
    assert!(editor.drawing().items.is_empty());
}

#[test]
fn center_choice_cancel_keeps_undo_and_prior_continuation() {
    for token in [None, Some("a"), Some("l")] {
        let mut editor = Editor::default();
        run(
            &mut editor,
            &["LINE", "0,0", "5,0", "", "ARC", "5,0", "C", "0,0"],
        );
        let before = editor.drawing().clone();
        if let Some(token) = token {
            editor.submit(token).unwrap();
        }
        editor.cancel_command().unwrap();
        assert_eq!(editor.prompt(), "Command");
        assert_eq!(editor.drawing(), &before);
        run(&mut editor, &["ARC", "", "10,5", "UNDO"]);
        assert_eq!(editor.drawing(), &before);
    }
}
