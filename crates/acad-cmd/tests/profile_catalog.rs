#[allow(dead_code)]
#[path = "../build_support/messages.rs"]
mod messages;
#[allow(dead_code)]
#[path = "../build_support/profiles.rs"]
mod profiles;
use acad_cmd::{
    messages::{text, Locale},
    profiles::{ProfileId, PROFILES},
};
use serde_json::{json, Value};
fn accepted(input: &str) -> bool {
    let messages = messages::parse(acad_cmd::messages::CATALOG_JSON).unwrap();
    profiles::parse(input, &messages).is_ok()
}
#[test]
fn canonical_profiles_resolve_exact_english_and_ukrainian() {
    assert!(accepted(acad_cmd::profiles::CATALOG_JSON));
    assert_eq!(PROFILES.len(), 2);
    for (id, palette, title, badge, status) in [
        (
            "frozen",
            "pc16",
            "1983 Faithful (frozen)",
            "1983 Faithful",
            "Mode: 1983 Faithful (frozen)",
        ),
        (
            "modern",
            "aci256",
            "Modernized (concept)",
            "Modernized",
            "Mode: Modernized (concept)",
        ),
    ] {
        let p = ProfileId::parse(id).unwrap().definition();
        assert_eq!(p.palette.name(), palette);
        for (key, expected) in [(p.title, title), (p.badge, badge), (p.status, status)] {
            assert_eq!(text(key, Locale::En).unwrap(), expected);
            assert_ne!(text(key, Locale::Uk).unwrap(), expected);
        }
    }
    for id in ["", "Frozen", "MODERN", "modern ", "future"] {
        assert!(ProfileId::parse(id).is_err());
    }
}
#[test]
fn rejects_unknown_duplicate_missing_and_invalid_profile_data() {
    let original = acad_cmd::profiles::CATALOG_JSON;
    for (from, to) in [
        (
            "\"schema_version\": 1",
            "\"schema_version\": 1, \"schema_version\": 1",
        ),
        (
            "\"id\": \"frozen\"",
            "\"id\": \"frozen\", \"id\": \"modern\"",
        ),
        (
            "\"palette\": \"pc16\"",
            "\"palette\": \"pc16\", \"palette\": \"aci256\"",
        ),
        (
            "\"title_key\": \"profile.frozen.title\"",
            "\"title_key\": \"profile.frozen.title\", \"title_key\": \"profile.frozen.badge\"",
        ),
    ] {
        let input = original.replace(from, to);
        assert_ne!(input, original);
        assert!(!accepted(&input));
    }
    let original: Value = serde_json::from_str(original).unwrap();
    for (pointer, value) in [
        ("/schema_version", json!(2)),
        ("/profiles/0/id", json!("future")),
        ("/profiles/1/id", json!("frozen")),
        ("/profiles/0/palette", json!("rgb")),
        ("/profiles/0/tone", json!("blue")),
        ("/profiles/0/source", json!(" ")),
        ("/profiles/0/source", json!("bad\nsource")),
        ("/profiles/0/title_key", json!("missing.label")),
        ("/profiles/0/title_key", json!("error.command.unknown")),
    ] {
        let mut input = original.clone();
        *input.pointer_mut(pointer).unwrap() = value;
        assert!(!accepted(&input.to_string()), "{pointer}");
    }
    for field in [
        "id",
        "palette",
        "tone",
        "title_key",
        "badge_key",
        "status_key",
        "source",
    ] {
        let mut input = original.clone();
        input["profiles"][0].as_object_mut().unwrap().remove(field);
        assert!(!accepted(&input.to_string()));
    }
    for pointer in ["root", "profile"] {
        let mut input = original.clone();
        if pointer == "root" {
            input["extra"] = json!(1);
        } else {
            input["profiles"][0]["limits"] = json!(10);
        }
        assert!(!accepted(&input.to_string()));
    }
    let mut input = original.clone();
    input["profiles"].as_array_mut().unwrap().pop();
    assert!(!accepted(&input.to_string()));
    assert!(!accepted(&" ".repeat(16 * 1024 + 1)));
}
