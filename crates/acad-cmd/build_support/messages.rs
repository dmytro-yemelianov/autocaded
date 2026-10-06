//! Build-only, strict catalog parser shared with fixture tests.
use serde::Deserialize;
use std::{collections::BTreeSet, env, fs, path::PathBuf};

pub const MAX_BYTES: usize = 1024 * 1024;
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Catalog {
    schema_version: u32,
    pub messages: Vec<Entry>,
}
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Entry {
    pub key: String,
    pub args: Vec<Argument>,
    pub text: Translation,
    source: String,
}
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Argument {
    pub name: String,
    #[serde(rename = "type")]
    kind: String,
}
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Translation {
    pub en: String,
    #[serde(default, deserialize_with = "optional_translation")]
    pub uk: Option<String>,
}
fn optional_translation<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> Result<Option<String>, D::Error> {
    String::deserialize(deserializer).map(Some)
}
#[derive(Debug, PartialEq, Eq)]
pub enum Token {
    Literal(String),
    Argument(usize),
}
fn identifier(value: &str, max: usize) -> bool {
    value.len() <= max
        && value.as_bytes().first().is_some_and(u8::is_ascii_lowercase)
        && value
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_')
}
pub fn variant(key: &str) -> String {
    key.split(['.', '_'])
        .filter(|part| !part.is_empty())
        .map(|part| {
            let mut word = part.to_owned();
            word[..1].make_ascii_uppercase();
            word
        })
        .collect()
}
fn field(name: &str) -> String {
    match name {
        "self" | "super" | "crate" => format!("r#argument_{name}"),
        _ => format!("r#{name}"),
    }
}
pub fn tokenize(template: &str, args: &[Argument]) -> Result<Vec<Token>, String> {
    if template.is_empty() || template.len() > 4096 {
        return Err("empty or oversized template".into());
    }
    let mut chars = template.chars().peekable();
    let mut tokens = Vec::new();
    let mut literal = String::new();
    let mut seen = BTreeSet::new();
    while let Some(ch) = chars.next() {
        match ch {
            '{' if chars.peek() == Some(&'{') => {
                chars.next();
                literal.push('{');
            }
            '}' if chars.peek() == Some(&'}') => {
                chars.next();
                literal.push('}');
            }
            '{' => {
                let mut name = String::new();
                loop {
                    match chars.next() {
                        Some('}') => break,
                        Some(c) if c != '{' => name.push(c),
                        _ => return Err("unclosed or nested placeholder".into()),
                    }
                }
                let index = args
                    .iter()
                    .position(|a| a.name == name)
                    .ok_or_else(|| format!("undeclared placeholder: {name}"))?;
                if !literal.is_empty() {
                    tokens.push(Token::Literal(std::mem::take(&mut literal)));
                }
                tokens.push(Token::Argument(index));
                seen.insert(index);
            }
            '}' => return Err("stray closing brace".into()),
            c => literal.push(c),
        }
    }
    if !literal.is_empty() {
        tokens.push(Token::Literal(literal));
    }
    if seen.len() != args.len() {
        return Err("unused argument".into());
    }
    Ok(tokens)
}
pub fn parse(input: &str) -> Result<Catalog, String> {
    if input.len() > MAX_BYTES {
        return Err("oversized catalog".into());
    }
    let catalog: Catalog = serde_json::from_str(input).map_err(|e| e.to_string())?;
    if catalog.schema_version != 1 {
        return Err("unsupported schema version".into());
    }
    if !(1..=256).contains(&catalog.messages.len()) {
        return Err("invalid entry count".into());
    }
    let mut keys = BTreeSet::new();
    let mut variants = BTreeSet::new();
    for entry in &catalog.messages {
        if entry.key.len() > 96
            || !entry.key.contains('.')
            || !entry.key.split('.').all(|p| identifier(p, 96))
        {
            return Err(format!("invalid key: {}", entry.key));
        }
        if !keys.insert(&entry.key) {
            return Err("duplicate key".into());
        }
        if !variants.insert(variant(&entry.key)) {
            return Err("generated name collision".into());
        }
        if entry.source.trim().is_empty() {
            return Err("missing provenance".into());
        }
        if entry.args.len() > 8 {
            return Err("too many arguments".into());
        }
        let mut names = BTreeSet::new();
        let mut fields = BTreeSet::new();
        for arg in &entry.args {
            if arg.kind != "text" || !identifier(&arg.name, 32) {
                return Err("invalid argument schema".into());
            }
            if !names.insert(&arg.name) {
                return Err("duplicate argument name".into());
            }
            if !fields.insert(field(&arg.name)) {
                return Err("generated argument name collision".into());
            }
        }
        tokenize(&entry.text.en, &entry.args)?;
        if let Some(uk) = &entry.text.uk {
            tokenize(uk, &entry.args)?;
        }
    }
    Ok(catalog)
}
fn static_value(template: &str, args: &[Argument]) -> String {
    tokenize(template, args)
        .unwrap()
        .into_iter()
        .map(|t| match t {
            Token::Literal(s) => s,
            Token::Argument(_) => unreachable!(),
        })
        .collect()
}
pub fn generate(catalog: &Catalog) -> String {
    let mut ids = String::new();
    let mut messages = String::new();
    let mut identity = String::new();
    let mut rendering = String::new();
    let mut lookup = String::new();
    let mut keys = String::new();
    for entry in &catalog.messages {
        let v = variant(&entry.key);
        ids.push_str(&format!("{v},"));
        let fields: Vec<_> = entry.args.iter().map(|a| field(&a.name)).collect();
        // Schema arguments start with a letter; private underscore bindings
        // cannot shadow the renderer locale or accumulator.
        let pattern = if fields.is_empty() {
            format!("Self::{v}")
        } else {
            format!(
                "Self::{v} {{ {} }}",
                fields
                    .iter()
                    .enumerate()
                    .map(|(i, f)| format!("{f}: __arg_{i}"))
                    .collect::<Vec<_>>()
                    .join(",")
            )
        };
        if fields.is_empty() {
            messages.push_str(&format!("{v},"));
        } else {
            messages.push_str(&format!(
                "{v} {{ {} }},",
                fields
                    .iter()
                    .map(|f| format!("{f}: String"))
                    .collect::<Vec<_>>()
                    .join(",")
            ));
        }
        let identity_pattern = if fields.is_empty() {
            format!("Self::{v}")
        } else {
            format!("Self::{v} {{ .. }}")
        };
        identity.push_str(&format!("{identity_pattern} => MessageId::{v},"));
        keys.push_str(&format!("{:?} => Some(MessageId::{v}),", entry.key));
        if fields.is_empty() {
            let en = static_value(&entry.text.en, &entry.args);
            let uk = static_value(
                entry.text.uk.as_deref().unwrap_or(&entry.text.en),
                &entry.args,
            );
            lookup.push_str(&format!("MessageId::{v} => Ok(match locale {{ Locale::En => {en:?}, Locale::Uk => {uk:?} }}),"));
            rendering.push_str(&format!(
                "{pattern} => text(MessageId::{v}, __locale).unwrap().to_owned(),"
            ));
        } else {
            lookup.push_str(&format!(
                "MessageId::{v} => Err(FormatError::ArgumentsRequired),"
            ));
            let mut bodies = Vec::new();
            for template in [
                &entry.text.en,
                entry.text.uk.as_ref().unwrap_or(&entry.text.en),
            ] {
                let mut body = "let mut __output = String::new();".to_owned();
                for token in tokenize(template, &entry.args).unwrap() {
                    match token {
                        Token::Literal(s) => body.push_str(&format!("__output.push_str({s:?});")),
                        Token::Argument(i) => {
                            body.push_str(&format!("__output.push_str(__arg_{i});"))
                        }
                    }
                }
                body.push_str("__output");
                bodies.push(body);
            }
            rendering.push_str(&format!(
                "{pattern} => match __locale {{ Locale::En => {{ {} }}, Locale::Uk => {{ {} }} }},",
                bodies[0], bodies[1]
            ));
        }
    }
    format!("#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)] pub enum MessageId {{ {ids} }}
#[derive(Clone, Debug, PartialEq, Eq)] pub enum Message {{ {messages} }}
impl Message {{ pub fn id(&self) -> MessageId {{ match self {{ {identity} }} }}
pub fn render(&self, __locale: Locale) -> String {{ match self {{ {rendering} }} }} }}
pub const fn text(id: MessageId, locale: Locale) -> Result<&'static str, FormatError> {{ match id {{ {lookup} }} }}
pub fn resolve_key(key: &str) -> Option<MessageId> {{ match key {{ {keys} _ => None }} }}")
}
pub fn build() {
    const PATH: &str = "resources/messages.json";
    println!("cargo:rerun-if-changed={PATH}");
    println!("cargo:rerun-if-changed=build_support/messages.rs");
    let bytes = fs::metadata(PATH).expect(PATH).len();
    assert!(bytes <= MAX_BYTES as u64, "oversized message catalog");
    let input = fs::read_to_string(PATH).expect(PATH);
    let catalog = parse(&input).expect("invalid resources/messages.json");
    let output = PathBuf::from(env::var_os("OUT_DIR").expect("OUT_DIR"));
    fs::write(output.join("message_catalog.rs"), generate(&catalog))
        .expect("write message catalog");
}
