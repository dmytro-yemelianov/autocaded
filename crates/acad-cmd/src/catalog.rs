//! Shared command facts generated from `resources/commands.json`.
//!
//! Tokens remain language-independent for scripts and drawing compatibility.
//! Presentation keys can be translated without changing command identity.
//! Categories describe current presentation policy; availability is not option parity.

/// Current presentation grouping, rather than a historical classification.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum CommandCategory {
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

/// Whether the recognized command's implemented slice requires a device.
/// `Software` does not promise the complete retained command option set.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum CommandAvailability {
    Software,
    DeviceRequired,
}

/// Membership in the independently retained dispatcher-help inventory.
/// `Additional` means outside that inventory, not absent from historical CAD.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum CommandLineage {
    RetainedDispatcher,
    Additional,
}

/// A command's identity, accepted spellings, and discovery metadata.
#[derive(Debug)]
pub struct CommandDef {
    pub id: CommandId,
    pub stable_id: &'static str,
    pub token: &'static str,
    pub aliases: &'static [&'static str],
    pub label_key: &'static str,
    pub summary_key: &'static str,
    pub category: CommandCategory,
    pub availability: CommandAvailability,
    pub lineage: CommandLineage,
    /// Exact backslash-delimited label in the retained `acad.hlp`, if present.
    pub help_topic: Option<&'static str>,
    /// Snapshot of the implementation from which these facts were extracted.
    pub source: &'static str,
}

include!(concat!(env!("OUT_DIR"), "/command_catalog.rs"));
include!(concat!(env!("OUT_DIR"), "/hatch_patterns.rs"));

/// The validated, versioned source catalog, suitable for browser discovery.
pub const CATALOG_JSON: &str = include_str!("../resources/commands.json");

/// Resolve a command spelling without changing its presentation or input text.
pub fn resolve_command(token: &str) -> Option<&'static CommandDef> {
    COMMANDS.iter().find(|definition| {
        definition.token.eq_ignore_ascii_case(token)
            || definition
                .aliases
                .iter()
                .any(|alias| alias.eq_ignore_ascii_case(token))
    })
}
