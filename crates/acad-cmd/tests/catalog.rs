use acad_cmd::{
    catalog::{resolve_command, CommandId, COMMANDS},
    Editor,
};

#[test]
fn extracted_aliases_preserve_the_previous_dispatch_behavior() {
    // Independent inventory from the v0.4.3 dispatcher, rather than reading
    // expected aliases back from the catalog under test.
    for (token, alias) in [
        ("LINE", "L"),
        ("CIRCLE", "C"),
        ("POINT", "PO"),
        ("ARC", "A"),
        ("TEXT", "T"),
        ("INSERT", "I"),
        ("SNAP", "RES"),
        ("SNAP", "RESOLUTION"),
        ("DIM", "DIMENSION"),
        ("COLOR", "COLORS"),
        ("ZOOM", "Z"),
        ("PAN", "P"),
        ("HELP", "?"),
        ("PLOT", "PRPLOT"),
        ("ERASE", "E"),
        ("MOVE", "M"),
        ("COPY", "CO"),
        ("ROTATE", "RO"),
        ("SCALE", "SC"),
        ("ARRAY", "AR"),
        ("CHANGE", "CH"),
        ("FILLET", "F"),
        ("BREAK", "BR"),
        ("DIST", "DI"),
        ("SOLID", "SO"),
        ("TRACE", "TR"),
        ("AREA", "AA"),
        ("UNDO", "U"),
        ("QUIT", "EXIT"),
    ] {
        assert_eq!(
            resolve_command(token).unwrap().id,
            resolve_command(alias).unwrap().id
        );
        let mut canonical = Editor::default();
        let mut abbreviated = Editor::default();
        assert_eq!(
            canonical.submit(token),
            abbreviated.submit(&alias.to_ascii_lowercase()),
            "{alias}"
        );
        assert_eq!(canonical.prompt(), abbreviated.prompt(), "{alias}");
        assert_eq!(canonical.drawing(), abbreviated.drawing(), "{alias}");
    }
}

#[test]
fn identities_are_independent_of_spelling_and_shared_with_browser_data() {
    assert_eq!(resolve_command("l").unwrap().id, CommandId::Line);
    assert_eq!(
        resolve_command("LINE").unwrap().label_key,
        "command.line.label"
    );
    assert_ne!(
        resolve_command("REDRAW").unwrap().id,
        resolve_command("REGEN").unwrap().id
    );
    assert!(resolve_command("NO_SUCH_COMMAND").is_none());
    let catalog: serde_json::Value = serde_json::from_str(acad_cmd::catalog::CATALOG_JSON).unwrap();
    assert_eq!(catalog["schema_version"], 1);
    assert_eq!(
        catalog["commands"].as_array().unwrap().len(),
        COMMANDS.len()
    );
    for definition in COMMANDS {
        let source = catalog["commands"]
            .as_array()
            .unwrap()
            .iter()
            .find(|value| value["id"] == definition.stable_id)
            .unwrap();
        assert_eq!(source["token"], definition.token);
        assert_eq!(source["aliases"], serde_json::json!(definition.aliases));
    }
}

#[test]
fn unknown_and_empty_commands_keep_their_existing_contract() {
    let mut editor = Editor::default();
    assert_eq!(
        editor.submit("not_a_command").unwrap_err(),
        "unknown command: not_a_command"
    );
    assert!(editor.submit("").is_ok());
    assert_eq!(editor.prompt(), "Command");
}

#[test]
fn discovery_metadata_matches_json_and_retained_evidence() {
    use acad_cmd::catalog::{CommandAvailability as A, CommandCategory as C, CommandLineage as L};
    use std::collections::BTreeSet;

    // Independent recovered dispatcher-help inventory from tests/help.rs.
    const RETAINED: &str = "LINE POINT CIRCLE SHAPE REPEAT ENDREP TEXT ARC TRACE LOAD SOLID LIST INSERT BASE ORTHO LAYER GRID LIMITS ID RES RESOLUTION ZOOM PAN MOVE ERASE MENU REDRAW STATUS REGEN DBLIST DIST CHANGE END QUIT ? AREA OOPS TABLET PLOT DELAY RESUME COPY BLOCK DIM QPLOT SNAP FILL HELP UNITS ARRAY WBLOCK AXIS HATCH FILLET BREAK SKETCH FILES";
    let retained: BTreeSet<_> = RETAINED
        .split_whitespace()
        .map(|token| resolve_command(token).unwrap().stable_id)
        .collect();
    assert_eq!(RETAINED.split_whitespace().count(), 57);
    assert_eq!(retained.len(), 54);
    let help: BTreeSet<_> = include_str!("../resources/acad.hlp")
        .lines()
        .filter_map(|line| line.strip_prefix('\\'))
        .collect();
    let catalog: serde_json::Value = serde_json::from_str(acad_cmd::catalog::CATALOG_JSON).unwrap();
    assert_eq!(COMMANDS.len(), 61);
    let mut additional = BTreeSet::new();
    let mut device_required = BTreeSet::new();
    for (definition, json) in COMMANDS.iter().zip(catalog["commands"].as_array().unwrap()) {
        assert_eq!(json["id"], definition.stable_id);
        assert_eq!(json["label_key"], definition.label_key);
        assert_eq!(json["summary_key"], definition.summary_key);
        assert_eq!(
            definition.label_key,
            format!("command.{}.label", definition.stable_id)
        );
        assert_eq!(
            definition.summary_key,
            format!("command.{}.summary", definition.stable_id)
        );
        assert_eq!(json["source"], definition.source);
        assert!(!definition.source.trim().is_empty());
        let category = match definition.category {
            C::Drawing => "drawing",
            C::Editing => "editing",
            C::Settings => "settings",
            C::View => "view",
            C::Inquiry => "inquiry",
            C::Files => "files",
            C::Automation => "automation",
            C::Help => "help",
            C::Devices => "devices",
        };
        assert_eq!(json["category"], category);
        let availability = match definition.availability {
            A::Software => "software",
            A::DeviceRequired => {
                device_required.insert(definition.token);
                "device_required"
            }
        };
        assert_eq!(json["availability"], availability);
        let lineage = match definition.lineage {
            L::RetainedDispatcher => {
                assert!(retained.contains(definition.stable_id));
                "retained_dispatcher"
            }
            L::Additional => {
                assert!(!retained.contains(definition.stable_id));
                additional.insert(definition.token);
                "additional"
            }
        };
        assert_eq!(json["lineage"], lineage);
        let expected_help = help.contains(definition.token).then_some(definition.token);
        assert_eq!(definition.help_topic, expected_help);
        assert_eq!(json["help_topic"], serde_json::json!(definition.help_topic));
    }
    assert_eq!(
        additional,
        BTreeSet::from([
            "SCRIPT",
            "COLOR",
            "ROTATE",
            "SCALE",
            "ENTITYAREA",
            "UNDO",
            "SAVE"
        ])
    );
    assert_eq!(device_required, BTreeSet::from(["PLOT", "QPLOT", "TABLET"]));
    assert_eq!(resolve_command("COLOR").unwrap().help_topic, Some("COLOR"));
}

#[test]
fn complete_identity_and_alias_inventory_is_unchanged() {
    // Independent v0.4.3 command catalog inventory, including order and aliases.
    const IDENTITIES: &str = "LINE:L CIRCLE:C POINT:PO ARC:A LOAD SHAPE TEXT:T INSERT:I BLOCK WBLOCK REPEAT ENDREP BASE AXIS SNAP:RES,RESOLUTION DIM:DIMENSION HATCH SKETCH DELAY RESUME SCRIPT UNITS GRID ORTHO FILL LIMITS LAYER COLOR:COLORS ZOOM:Z PAN:P LIST DBLIST HELP:? MENU FILES STATUS PLOT:PRPLOT QPLOT TABLET REDRAW REGEN ERASE:E MOVE:M COPY:CO ROTATE:RO SCALE:SC ARRAY:AR CHANGE:CH FILLET:F BREAK:BR DIST:DI ID SOLID:SO TRACE:TR AREA:AA ENTITYAREA UNDO:U OOPS SAVE END QUIT:EXIT";
    assert_eq!(IDENTITIES.split_whitespace().count(), COMMANDS.len());
    for (expected, command) in IDENTITIES.split_whitespace().zip(COMMANDS) {
        let (token, aliases) = expected.split_once(':').unwrap_or((expected, ""));
        assert_eq!(command.token, token);
        assert_eq!(command.stable_id, token.to_ascii_lowercase());
        let aliases: Vec<_> = aliases
            .split(',')
            .filter(|alias| !alias.is_empty())
            .collect();
        assert_eq!(command.aliases, aliases);
    }
}
