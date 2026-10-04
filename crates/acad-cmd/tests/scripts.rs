//! Editor half of the command-script contract (docs/native-scripts.md): the
//! editor only parses DELAY/SCRIPT/RESUME into effects; the application owns
//! files, the queue and time.
use acad_cmd::{Editor, Effect};

fn delay(count: &str) -> Result<Effect, String> {
    let mut editor = Editor::default();
    editor.submit("DELAY").unwrap();
    assert_eq!(editor.prompt(), "DELAY: duration");
    editor.submit(count)
}

#[test]
fn delay_takes_a_signed_16_bit_integer_and_negative_means_no_pause() {
    assert_eq!(delay("1000"), Ok(Effect::Delay(1000)));
    assert_eq!(delay("32767"), Ok(Effect::Delay(32767)));
    assert_eq!(delay("0"), Ok(Effect::Delay(0)));
    assert_eq!(delay("-5"), Ok(Effect::Delay(0)));
    assert_eq!(delay("-32768"), Ok(Effect::Delay(0)));
    for invalid in ["0.5", "1e3", "abc", "", "1,5"] {
        assert!(
            delay(invalid).unwrap_err().contains("integer"),
            "{invalid:?}"
        );
    }
    for out_of_range in ["32768", "65535", "-32769"] {
        assert!(delay(out_of_range).unwrap_err().contains("-32768 to 32767"));
    }
}

#[test]
fn a_rejected_delay_count_keeps_the_prompt_and_drawing() {
    let mut editor = Editor::default();
    let before = editor.drawing().clone();
    editor.submit("DELAY").unwrap();
    assert!(editor.submit("x").is_err());
    assert_eq!(editor.prompt(), "DELAY: duration");
    assert_eq!(editor.submit("5"), Ok(Effect::Delay(5)));
    assert_eq!(editor.prompt(), "Command");
    assert_eq!(editor.drawing(), &before);
}

#[test]
fn script_and_resume_are_requests_to_the_application() {
    let mut editor = Editor::default();
    let before = editor.drawing().clone();
    assert_eq!(editor.submit("RESUME"), Ok(Effect::Resume));
    assert_eq!(editor.submit_return("resume"), Ok(Effect::Resume));
    editor.submit("SCRIPT").unwrap();
    assert_eq!(editor.prompt(), "SCRIPT: file name");
    assert!(editor.submit("  ").is_err());
    assert_eq!(editor.prompt(), "SCRIPT: file name");
    assert_eq!(editor.submit(" demo "), Ok(Effect::Script("demo".into())));
    assert_eq!(editor.prompt(), "Command");
    assert_eq!(editor.drawing(), &before);
}

#[test]
fn only_text_value_prompts_take_literal_lines() {
    let mut editor = Editor::default();
    assert!(!editor.accepts_literal_text());
    for input in ["TEXT", "0,0", "1", "0"] {
        assert!(!editor.accepts_literal_text(), "{input}");
        editor.submit(input).unwrap();
    }
    assert_eq!(editor.prompt(), "TEXT: value");
    assert!(editor.accepts_literal_text());
}
