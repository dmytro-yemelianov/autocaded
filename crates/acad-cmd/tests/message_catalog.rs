#[allow(dead_code)]
#[path = "../build_support/messages.rs"]
mod builder;

use acad_cmd::messages::{resolve_key, text, FormatError, Locale, LocaleError, Message, MessageId};
use serde_json::{json, Value};

fn entry() -> Value {
    json!({"key":"test.message", "args":[], "text":{"en":"hello"}, "source":"fixture"})
}
fn catalog(entries: Vec<Value>) -> Value {
    json!({"schema_version":1,"messages":entries})
}
fn accepted(value: &Value) -> bool {
    builder::parse(&value.to_string()).is_ok()
}
#[test]
fn strict_json_fields_and_required_data() {
    let original = catalog(vec![entry()]).to_string();
    for (from, to) in [
        (
            "\"schema_version\":1",
            "\"schema_version\":1,\"schema_version\":1",
        ),
        (
            "\"key\":\"test.message\"",
            "\"key\":\"test.message\",\"key\":\"other.key\"",
        ),
        ("\"en\":\"hello\"", "\"en\":\"hello\",\"en\":\"other\""),
        (
            "\"en\":\"hello\"",
            "\"en\":\"hello\",\"uk\":\"a\",\"uk\":\"b\"",
        ),
    ] {
        assert_ne!(original.replace(from, to), original);
        assert!(builder::parse(&original.replace(from, to)).is_err());
    }
    for pointer in ["/extra", "/messages/0/extra", "/messages/0/text/fr"] {
        let mut value = catalog(vec![entry()]);
        match pointer {
            "/extra" => value["extra"] = json!(0),
            "/messages/0/extra" => value["messages"][0]["extra"] = json!(0),
            _ => value["messages"][0]["text"]["fr"] = json!("hello"),
        }
        assert!(!accepted(&value));
    }
    let mut null_locale = entry();
    null_locale["text"]["uk"] = Value::Null;
    assert!(!accepted(&catalog(vec![null_locale])));
    let mut value = catalog(vec![entry()]);
    value["schema_version"] = json!(2);
    assert!(!accepted(&value));
    for field in ["key", "args", "text", "source"] {
        let mut e = entry();
        e.as_object_mut().unwrap().remove(field);
        assert!(!accepted(&catalog(vec![e])));
    }
    for e in [
        json!({"key":"test.message","args":[],"text":{"uk":"hi"},"source":"x"}),
        json!({"key":"test.message","args":[],"text":{"en":""},"source":"x"}),
        json!({"key":"test.message","args":[],"text":{"en":"hi"},"source":" "}),
    ] {
        assert!(!accepted(&catalog(vec![e])));
    }
}
#[test]
fn identities_duplicates_and_budgets() {
    assert!(!accepted(&catalog(vec![entry(), entry()])));
    let mut second = entry();
    second["key"] = json!("test_message.x");
    let mut first = entry();
    first["key"] = json!("test.message_x");
    assert!(!accepted(&catalog(vec![first, second])));
    for key in [
        "test",
        "Test.message",
        "test..message",
        "test.1message",
        "test.m-é",
        "test.bad-name",
    ] {
        let mut e = entry();
        e["key"] = json!(key);
        assert!(!accepted(&catalog(vec![e])));
    }
    let mut e = entry();
    e["key"] = json!(format!("a.{}", "b".repeat(94)));
    assert!(accepted(&catalog(vec![e.clone()])));
    e["key"] = json!(format!("a.{}", "b".repeat(95)));
    assert!(!accepted(&catalog(vec![e])));
    assert!(!accepted(&catalog(vec![])));
    for count in [256, 257] {
        let entries = (0..count)
            .map(|i| {
                let mut e = entry();
                e["key"] = json!(format!("test.m{i}"));
                e
            })
            .collect();
        assert_eq!(accepted(&catalog(entries)), count == 256);
    }
    for size in [4096, 4097] {
        let mut e = entry();
        e["text"]["en"] = json!("Ї".repeat(size / 2) + if size % 2 == 1 { "x" } else { "" });
        assert_eq!(accepted(&catalog(vec![e])), size == 4096);
    }
    let valid = catalog(vec![entry()]).to_string();
    assert!(
        builder::parse(&(valid.clone() + &" ".repeat(builder::MAX_BYTES - valid.len()))).is_ok()
    );
    assert!(
        builder::parse(&(valid.clone() + &" ".repeat(builder::MAX_BYTES + 1 - valid.len())))
            .is_err()
    );
}
#[test]
fn argument_schema_and_template_grammar() {
    let mut e = entry();
    e["args"] = json!([{"name":"command","type":"text"}]);
    for template in [
        "{",
        "}",
        "{}",
        "{0}",
        "{command:?}",
        "{command.x}",
        "{command[0]}",
        "{{command}",
        "{command",
        "hello",
        "{other}",
    ] {
        e["text"]["en"] = json!(template);
        assert!(!accepted(&catalog(vec![e.clone()])), "{template}");
    }
    e["text"]["en"] = json!("{command}");
    e["text"]["uk"] = json!("{other}");
    assert!(!accepted(&catalog(vec![e.clone()])));
    e["text"].as_object_mut().unwrap().remove("uk");
    for arg in [
        json!({"name":"command","type":"number"}),
        json!({"name":"Command","type":"text"}),
        json!({"name":"command","type":"text","extra":0}),
    ] {
        let mut bad = e.clone();
        bad["args"] = json!([arg]);
        assert!(!accepted(&catalog(vec![bad])));
    }
    e["args"] = json!([{"name":"command","type":"text"},{"name":"command","type":"text"}]);
    assert!(!accepted(&catalog(vec![e])));
    for count in [8, 9] {
        let mut e = entry();
        e["args"] = json!((0..count)
            .map(|i| json!({"name":format!("a{i}"),"type":"text"}))
            .collect::<Vec<_>>());
        e["text"]["en"] = json!((0..count).map(|i| format!("{{a{i}}}")).collect::<String>());
        assert_eq!(accepted(&catalog(vec![e])), count == 8);
    }
    for len in [32, 33] {
        let name = "a".repeat(len);
        let mut e = entry();
        e["args"] = json!([{"name":name,"type":"text"}]);
        e["text"]["en"] = json!(format!("{{{name}}}"));
        assert_eq!(accepted(&catalog(vec![e])), len == 32);
    }
    let duplicated = r#"{"schema_version":1,"messages":[{"key":"test.message","args":[{"name":"x","name":"y","type":"text"}],"text":{"en":"{x}"},"source":"x"}]}"#;
    assert!(builder::parse(duplicated).is_err());
}
#[test]
fn actual_catalog_lookup_and_literal_payload() {
    let parsed = builder::parse(acad_cmd::messages::CATALOG_JSON).unwrap();
    assert_eq!(parsed.messages.len(), 36);
    for e in parsed.messages {
        let id = resolve_key(&e.key).unwrap();
        if e.args.is_empty() {
            assert_eq!(text(id, Locale::En).unwrap(), e.text.en);
            assert_eq!(text(id, Locale::Uk).unwrap(), e.text.uk.as_deref().unwrap());
        }
    }
    const COMMAND: &str = match text(MessageId::PromptCommand, Locale::En) {
        Ok(value) => value,
        Err(_) => panic!("static prompt"),
    };
    assert_eq!(COMMAND, "Command");
    assert_eq!(resolve_key("missing.key"), None);
    assert_eq!(
        text(MessageId::ErrorCommandUnknown, Locale::En),
        Err(FormatError::ArgumentsRequired)
    );
    let command = "{command} {{ }} Ї".repeat(5000);
    let message = Message::ErrorCommandUnknown {
        command: command.clone(),
    };
    assert_eq!(message.id(), MessageId::ErrorCommandUnknown);
    assert_eq!(
        message.render(Locale::Uk),
        format!("невідома команда: {command}")
    );
}
#[test]
fn locale_grammar() {
    for tag in ["en", "EN-us", "eN", "fr-CA", "zz", "ZZ-aa"] {
        assert_eq!(Locale::parse(tag), Ok(Locale::En));
    }
    for tag in ["uk", "Uk-uA", "UK", "uk-UA"] {
        assert_eq!(Locale::parse(tag), Ok(Locale::Uk));
    }
    for tag in [
        "", " en", "en ", "en_US", "en-", "eng", "e", "en-123", "en-USA", "en-US-x", "1n", "éé",
        "ук", "en\n",
    ] {
        assert_eq!(
            Locale::parse(tag),
            Err(LocaleError::MalformedTag),
            "{tag:?}"
        );
    }
}
#[test]
fn generated_formatter_handles_reordering_repetition_and_escaped_braces() {
    let mut e = entry();
    e["args"] = json!([{"name":"first","type":"text"},{"name":"second","type":"text"}]);
    e["text"] = json!({"en":"{{{first}}} {second} {first}","uk":"{second}: {first}/{second} {{}}"});
    let parsed = builder::parse(&catalog(vec![e]).to_string()).unwrap();
    let generated = builder::generate(&parsed);
    assert_eq!(generated, builder::generate(&parsed));
    let source=format!("#[derive(Clone,Copy)] enum Locale {{ En, Uk }} #[derive(Debug)] enum FormatError {{ ArgumentsRequired }} {generated}\nfn main() {{ let m=Message::TestMessage {{ first: \"{{second}}\".into(), second: \"}}x{{\".into() }}; assert_eq!(m.render(Locale::En), \"{{{{second}}}} }}x{{ {{second}}\"); assert_eq!(m.render(Locale::Uk), \"}}x{{: {{second}}/}}x{{ {{}}\"); }}");
    compile_fixture("braces", &source);
}

fn compile_fixture(label: &str, source: &str) {
    let dir = std::env::temp_dir().join(format!(
        "acad-message-fixture-{label}-{}",
        std::process::id()
    ));
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("fixture.rs"), source).unwrap();
    let compiled = std::process::Command::new("rustc")
        .arg("--edition=2021")
        .arg("-Awarnings")
        .arg(dir.join("fixture.rs"))
        .arg("-o")
        .arg(dir.join("fixture"))
        .output()
        .unwrap();
    assert!(
        compiled.status.success(),
        "{}",
        String::from_utf8_lossy(&compiled.stderr)
    );
    assert!(std::process::Command::new(dir.join("fixture"))
        .status()
        .unwrap()
        .success());
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn generated_argument_bindings_are_hygienic_and_preserve_keyword_fields() {
    let names = ["locale", "output", "type", "self", "crate", "super"];
    let mut e = entry();
    e["args"] = json!(names
        .iter()
        .map(|name| json!({"name":name,"type":"text"}))
        .collect::<Vec<_>>());
    e["text"] = json!({"en":"{locale}|{output}|{type}|{self}|{crate}|{super}",
        "uk":"{super}|{crate}|{self}|{type}|{output}|{locale}|{locale}"});
    let parsed = builder::parse(&catalog(vec![e]).to_string()).unwrap();
    let generated = builder::generate(&parsed);
    let main = r#"fn main() {
        let message = Message::TestMessage {
            locale: "{locale}".into(), output: "{output}".into(), r#type: "{type}".into(),
            argument_self: "{self}".into(), argument_crate: "{crate}".into(), argument_super: "{super}".into(),
        };
        assert_eq!(message.id(), MessageId::TestMessage);
        assert_eq!(message.render(Locale::En), "{locale}|{output}|{type}|{self}|{crate}|{super}");
        assert_eq!(message.render(Locale::Uk), "{super}|{crate}|{self}|{type}|{output}|{locale}|{locale}");
    }"#;
    let source = format!("#[derive(Clone,Copy)] enum Locale {{ En, Uk }} #[derive(Debug)] enum FormatError {{ ArgumentsRequired }} {generated} {main}");
    compile_fixture("hygiene", &source);
    for name in ["self", "crate", "super"] {
        let mut e = entry();
        let remapped = format!("argument_{name}");
        e["args"] = json!([{"name":name,"type":"text"},{"name":remapped,"type":"text"}]);
        e["text"]["en"] = json!(format!("{{{name}}} {{{remapped}}}"));
        let error = builder::parse(&catalog(vec![e]).to_string()).unwrap_err();
        assert_eq!(error, "generated argument name collision");
    }
}

#[test]
fn missing_translation_falls_back_to_complete_english_with_literal_arguments() {
    let mut e = entry();
    e["args"] = json!([{"name":"command","type":"text"}]);
    e["text"] = json!({"en":"unknown: {command} / {command}"});
    let parsed = builder::parse(&catalog(vec![e]).to_string()).unwrap();
    let generated = builder::generate(&parsed);
    let source = format!(
        r#"
        #[derive(Clone,Copy)] enum Locale {{ En, Uk }}
        #[derive(Debug)] enum FormatError {{ ArgumentsRequired }}
        {generated}
        fn main() {{
            let m = Message::TestMessage {{ command: "{{command}}".into() }};
            assert_eq!(m.render(Locale::Uk), "unknown: {{command}} / {{command}}");
            assert_eq!(m.render(Locale::Uk), m.render(Locale::En));
        }}
    "#
    );
    compile_fixture("fallback", &source);
}
