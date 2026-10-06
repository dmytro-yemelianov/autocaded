//! Strict build-time presentation profiles; no geometry or document policies.
use serde::Deserialize;
use std::{collections::BTreeSet, env, fs, path::PathBuf};

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Catalog {
    schema_version: u32,
    profiles: Vec<Profile>,
}
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Profile {
    id: String,
    palette: String,
    tone: String,
    title_key: String,
    badge_key: String,
    status_key: String,
    source: String,
}
pub fn parse(input: &str, messages: &super::messages::Catalog) -> Result<Catalog, String> {
    if input.len() > 16 * 1024 {
        return Err("oversized profile catalog".into());
    }
    let catalog: Catalog = serde_json::from_str(input).map_err(|e| e.to_string())?;
    if catalog.schema_version != 1 || catalog.profiles.len() != 2 {
        return Err("unsupported profile schema or count".into());
    }
    let mut ids = BTreeSet::new();
    for profile in &catalog.profiles {
        if !matches!(profile.id.as_str(), "frozen" | "modern") || !ids.insert(&profile.id) {
            return Err("unknown or duplicate profile id".into());
        }
        if !matches!(profile.palette.as_str(), "pc16" | "aci256") {
            return Err("unknown profile palette".into());
        }
        if !matches!(profile.tone.as_str(), "green" | "red") {
            return Err("unknown badge tone".into());
        }
        if profile.source.trim().is_empty() || profile.source.chars().any(char::is_control) {
            return Err("missing profile provenance".into());
        }
        for key in [&profile.title_key, &profile.badge_key, &profile.status_key] {
            let entry = messages
                .messages
                .iter()
                .find(|entry| &entry.key == key)
                .ok_or_else(|| format!("unknown profile message: {key}"))?;
            if !entry.args.is_empty() || entry.text.uk.is_none() {
                return Err(
                    "profile messages must be no-argument English/Ukrainian entries".into(),
                );
            }
        }
    }
    Ok(catalog)
}
pub fn build() {
    const PATH: &str = "resources/profiles.json";
    println!("cargo:rerun-if-changed={PATH}");
    let messages =
        super::messages::parse(&fs::read_to_string("resources/messages.json").expect("messages"))
            .expect("validated messages");
    let catalog =
        parse(&fs::read_to_string(PATH).expect(PATH), &messages).expect("invalid profiles");
    let mut definitions = String::new();
    let mut variants = Vec::new();
    for profile in &catalog.profiles {
        let variant = super::messages::variant(&profile.id);
        variants.push(variant.clone());
        let palette = match profile.palette.as_str() {
            "pc16" => "Pc16",
            _ => "Aci256",
        };
        let tone = super::messages::variant(&profile.tone);
        definitions.push_str(&format!(
            "ProfileDef {{ id: ProfileId::{variant}, stable_id: {:?}, palette: acad_model::Palette::{palette}, tone: BadgeTone::{tone}, title: MessageId::{}, badge: MessageId::{}, status: MessageId::{}, source: {:?} }},\n",
            profile.id, super::messages::variant(&profile.title_key), super::messages::variant(&profile.badge_key),
            super::messages::variant(&profile.status_key), profile.source));
    }
    let output = PathBuf::from(env::var_os("OUT_DIR").expect("OUT_DIR"));
    fs::write(output.join("profiles.rs"), format!(
        "#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]\npub enum ProfileId {{ {} }}\npub const PROFILES: &[ProfileDef] = &[{definitions}];\n", variants.join(",")))
        .expect("write generated profiles");
}
