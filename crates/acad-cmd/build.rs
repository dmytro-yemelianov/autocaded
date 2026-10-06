#[path = "build_support/messages.rs"]
mod messages;
#[path = "build_support/profiles.rs"]
mod profiles;

use serde::Deserialize;
use std::{collections::BTreeSet, env, fs, path::PathBuf};

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Catalog {
    schema_version: u32,
    commands: Vec<Command>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Command {
    id: String,
    token: String,
    aliases: Vec<String>,
    source: String,
    category: CommandCategory,
    availability: CommandAvailability,
    lineage: CommandLineage,
    #[serde(deserialize_with = "required_help_topic")]
    help_topic: Option<String>,
    label_key: String,
    summary_key: String,
}

// Mirror the public types only for JSON decoding; generated records use the
// public enum variants directly, without a runtime parsing layer.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "snake_case")]
enum CommandCategory {
    Drawing,
    Editing,
    Settings,
    View,
    Inquiry,
    Files,
    Automation,
    Help,
    Devices,
}
#[derive(Debug, Deserialize)]
#[serde(rename_all = "snake_case")]
enum CommandAvailability {
    Software,
    DeviceRequired,
}
#[derive(Debug, Deserialize)]
#[serde(rename_all = "snake_case")]
enum CommandLineage {
    RetainedDispatcher,
    Additional,
}

fn required_help_topic<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> Result<Option<String>, D::Error> {
    Deserialize::deserialize(deserializer)
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct HatchCatalog {
    schema_version: u32,
    patterns: Vec<HatchPattern>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct HatchPattern {
    token: String,
    description: String,
    source: String,
}

fn main() {
    messages::build();
    profiles::build();
    const PATH: &str = "resources/commands.json";
    println!("cargo:rerun-if-changed={PATH}");
    let catalog: Catalog = serde_json::from_str(&fs::read_to_string(PATH).expect(PATH))
        .expect("invalid resources/commands.json");
    assert_eq!(
        catalog.schema_version, 1,
        "unsupported command catalog schema"
    );
    assert!(!catalog.commands.is_empty(), "empty command catalog");
    const HELP_PATH: &str = "resources/acad.hlp";
    println!("cargo:rerun-if-changed={HELP_PATH}");
    let help = fs::read_to_string(HELP_PATH).expect(HELP_PATH);
    let help_topics: BTreeSet<_> = help
        .lines()
        .filter_map(|line| line.strip_prefix('\\'))
        .collect();
    let mut ids = BTreeSet::new();
    let mut tokens = BTreeSet::new();
    let mut variants = Vec::new();
    let mut definitions = String::new();
    for command in &catalog.commands {
        assert!(
            !command.id.is_empty() && command.id.bytes().all(|b| b.is_ascii_lowercase()),
            "invalid command id: {}",
            command.id
        );
        assert!(
            ids.insert(&command.id),
            "duplicate command id: {}",
            command.id
        );
        assert!(
            !command.source.trim().is_empty() && !command.source.chars().any(char::is_control),
            "missing provenance: {}",
            command.id
        );
        assert!(
            !command.token.is_empty() && command.token.bytes().all(|b| b.is_ascii_uppercase()),
            "invalid command token: {}",
            command.token
        );
        for token in std::iter::once(&command.token).chain(&command.aliases) {
            assert!(
                !token.is_empty() && token.bytes().all(|b| b.is_ascii_uppercase() || b == b'?'),
                "invalid command spelling: {token}"
            );
            assert!(tokens.insert(token), "duplicate command spelling: {token}");
        }
        assert_eq!(
            command.label_key,
            format!("command.{}.label", command.id),
            "invalid label key"
        );
        assert_eq!(
            command.summary_key,
            format!("command.{}.summary", command.id),
            "invalid summary key"
        );
        let expected_help = help_topics
            .contains(command.token.as_str())
            .then_some(command.token.as_str());
        assert_eq!(
            command.help_topic.as_deref(),
            expected_help,
            "invalid retained help label: {}",
            command.id
        );
        let help_topic = match &command.help_topic {
            Some(topic) => format!("Some({topic:?})"),
            None => "None".to_owned(),
        };
        let variant = format!(
            "{}{}",
            command.id[..1].to_ascii_uppercase(),
            &command.id[1..]
        );
        variants.push(variant.clone());
        definitions.push_str(&format!(
            "CommandDef {{ id: CommandId::{variant}, stable_id: {:?}, token: {:?}, aliases: &{:?}, label_key: {:?}, summary_key: {:?}, category: CommandCategory::{:?}, availability: CommandAvailability::{:?}, lineage: CommandLineage::{:?}, help_topic: {help_topic}, source: {:?} }},\n",
            command.id, command.token, command.aliases,
            command.label_key, command.summary_key, command.category, command.availability, command.lineage, command.source
        ));
    }
    let generated = format!(
        "#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]\npub enum CommandId {{ {} }}\npub const COMMANDS: &[CommandDef] = &[{definitions}];\n",
        variants.join(",")
    );
    let output = PathBuf::from(env::var_os("OUT_DIR").expect("OUT_DIR"));
    fs::write(output.join("command_catalog.rs"), generated)
        .expect("write generated command catalog");

    const HATCH_PATH: &str = "resources/hatch-patterns.json";
    println!("cargo:rerun-if-changed={HATCH_PATH}");
    let hatch_catalog: HatchCatalog =
        serde_json::from_str(&fs::read_to_string(HATCH_PATH).expect(HATCH_PATH))
            .expect("invalid resources/hatch-patterns.json");
    assert_eq!(
        hatch_catalog.schema_version, 1,
        "unsupported hatch catalog schema"
    );
    assert!(!hatch_catalog.patterns.is_empty(), "empty hatch catalog");
    let mut hatch_tokens = BTreeSet::new();
    let mut hatch_definitions = String::new();
    for pattern in &hatch_catalog.patterns {
        assert!(
            !pattern.token.is_empty()
                && pattern
                    .token
                    .bytes()
                    .all(|byte| byte.is_ascii_uppercase() || byte.is_ascii_digit()),
            "invalid hatch token: {}",
            pattern.token
        );
        assert!(
            hatch_tokens.insert(&pattern.token),
            "duplicate hatch token: {}",
            pattern.token
        );
        assert!(
            !pattern.description.trim().is_empty()
                && !pattern.description.chars().any(char::is_control),
            "invalid hatch description: {}",
            pattern.token
        );
        assert!(
            !pattern.source.trim().is_empty(),
            "missing hatch provenance: {}",
            pattern.token
        );
        hatch_definitions.push_str(&format!(
            "    ({:?}, {:?}),\n",
            pattern.token, pattern.description
        ));
    }
    let generated =
        format!("pub(crate) const HATCH_PATTERNS: &[(&str, &str)] = &[\n{hatch_definitions}];\n");
    fs::write(output.join("hatch_patterns.rs"), generated).expect("write generated hatch patterns");
}
