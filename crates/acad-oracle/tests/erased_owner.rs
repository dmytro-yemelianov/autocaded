//! In-tree original evidence for erasing a whole REPEAT group whose members
//! include one already erased (docs/native-group-persistence.md, "Whole-owner
//! erasure after prior member erasure"). Every original behaviour that section
//! cites is asserted here, together with the native confirmed save that writes
//! the original's own representation. The System image is only read.
#![cfg(unix)]
use acad_model::{group_codec, Drawing, Entity, Item, Point, Repeat};
use acad_oracle::in_tree::{editor_text_rows, generate_dwg_in_tree, observe_in_tree};

fn disk() -> Option<std::path::PathBuf> {
    let disk = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../corpus/raw/Autodesk AutoCAD 1.4 (5.25)/System.img");
    if disk.exists() {
        Some(disk)
    } else {
        assert!(
            std::env::var_os("AUTOCAD_REQUIRE_CORPUS").is_none(),
            "{} absent with AUTOCAD_REQUIRE_CORPUS set",
            disk.display()
        );
        eprintln!("skipping in-tree oracle: extracted System.img absent, NOT validated");
        None
    }
}

/// A two-column group of LINE A (1,1)-(2,1) and LINE B (1,3)-(2,3); the
/// second column repeats both members five units to the right.
const GROUP: [&str; 13] = [
    "REPEAT", "LINE", "1,1", "2,1", "", "LINE", "1,3", "2,3", "", "ENDREP", "2", "1", "5",
];
/// Erase only A, through the window around its source instance.
const ERASE_A: [&str; 4] = ["ERASE", "W", "0,0", "3,2"];
/// Erase the whole group: the window holds all four instances.
const ERASE_ALL: [&str; 4] = ["ERASE", "W", "0,0", "10,5"];
const BLOCK_ALL: [&str; 6] = ["BLOCK", "B", "0,0", "W", "0,0", "10,5"];

fn script(parts: &[&[&'static str]]) -> Vec<&'static str> {
    let mut inputs = GROUP.to_vec();
    for part in parts {
        inputs.extend_from_slice(part);
    }
    inputs
}

fn original(disk: &std::path::Path, name: &str, parts: &[&[&'static str]]) -> Vec<u8> {
    generate_dwg_in_tree(disk, name, &script(parts)).unwrap_or_else(|e| panic!("{name}: {e}"))
}

/// The physical record region: signed type words live there.
fn region(dwg: &[u8]) -> &[u8] {
    let (_, meta) = acad_dwg::header::parse_header(dwg).unwrap();
    &dwg[meta.version.entity_start()..meta.entity_end as usize]
}

fn line(y: f64) -> Entity {
    Entity::OnLayer {
        layer: 1,
        entity: Box::new(Entity::Line {
            start: Point { x: 1.0, y },
            end: Point { x: 2.0, y },
        }),
    }
}
fn erased(entity: Entity) -> Entity {
    Entity::Erased(Box::new(entity))
}
fn group(entities: Vec<Entity>) -> Repeat {
    Repeat {
        start_layer: 1,
        end_layer: 1,
        entities,
        columns: 2,
        rows: 1,
        column_spacing: 5.0,
        row_spacing: 0.0,
    }
}

/// The editor's last command-area row texts after the keys, read with the
/// original's display font.
fn command_area(disk: &std::path::Path, extra: &[(&str, &[u8])], keys: &str) -> Vec<String> {
    let observation =
        observe_in_tree(disk, b"", extra, &[(keys.as_bytes(), 1500)], 100_000, &[]).unwrap();
    assert_eq!(observation.stopped, None, "{}", observation.console);
    let rows = editor_text_rows(disk, &observation.cga).unwrap();
    rows[22..].to_vec()
}

fn keys(parts: &[&[&str]]) -> String {
    let mut keys = String::from("1\rPX\r");
    for line in GROUP
        .iter()
        .chain(parts.iter().flat_map(|part| part.iter()))
    {
        keys.push_str(line);
        keys.push('\r');
    }
    keys
}

#[test]
fn original_whole_group_erase_negates_members_never_markers_and_loses_prior_member_erasure() {
    let Some(disk) = disk() else { return };
    let prior = original(&disk, "EOA", &[&ERASE_A]);
    assert_eq!(
        acad_dwg::parse(&prior).unwrap().items,
        [Item::Repeat(group(vec![erased(line(1.0)), line(3.0)]))]
    );
    let after_prior = original(&disk, "EOAALL", &[&ERASE_A, &ERASE_ALL]);
    let whole = original(&disk, "EOALL", &[&ERASE_ALL]);
    // The original's file cannot tell the two histories apart: same bytes.
    assert_eq!(after_prior, whole);
    // Positive REPEAT/ENDREP markers; each source member negative.
    let records = region(&whole);
    assert_eq!(&records[..4], &[5, 0, 1, 0]);
    assert_eq!(&records[4..6], &(-1i16).to_le_bytes());
    assert_eq!(&records[40..42], &(-1i16).to_le_bytes());
    assert_eq!(&records[76..78], &[6, 0]);
    let all_members = [Item::Repeat(group(vec![
        erased(line(1.0)),
        erased(line(3.0)),
    ]))];
    assert_eq!(acad_dwg::parse(&whole).unwrap().items, all_members);
    // Selection is per source member: a window holding only the repeated
    // column erases both source members, the same file again.
    assert_eq!(
        original(&disk, "EOCOL", &[&["ERASE", "W", "5,0", "10,5"]]),
        whole
    );
    // The window reselects A through its repeated instance: 3 found, not 2.
    assert_eq!(
        command_area(&disk, &[], &keys(&[&ERASE_A, &ERASE_ALL]))[1],
        "3 found."
    );
    assert_eq!(
        command_area(&disk, &[], &keys(&[&ERASE_ALL]))[1],
        "4 found."
    );
    // In-session OOPS then revives A as well: the original loses the prior
    // erasure even before saving.
    let live = [Item::Repeat(group(vec![line(1.0), line(3.0)]))];
    let revived = original(&disk, "EOAOOPS", &[&ERASE_A, &ERASE_ALL, &["OOPS"]]);
    let restored = original(&disk, "EOOOPS", &[&ERASE_ALL, &["OOPS"]]);
    // Byte-identical files: nothing of A's earlier erasure survives OOPS.
    assert_eq!(revived, restored);
    assert_eq!(acad_dwg::parse(&revived).unwrap().items, live);
    // Control: ungrouped, the window skips the erased LINE and OOPS keeps it erased.
    let plain = generate_dwg_in_tree(
        &disk,
        "EOPLAIN",
        &[
            "LINE", "1,1", "2,1", "", "LINE", "1,3", "2,3", "", "ERASE", "W", "0,0", "3,2",
            "ERASE", "W", "0,0", "10,5", "OOPS",
        ],
    )
    .unwrap();
    assert_eq!(
        acad_dwg::parse(&plain).unwrap().items,
        [Item::Erased(line(1.0)), Item::Entity(line(3.0))]
    );
}

#[test]
fn original_block_retains_source_with_members_erased_and_markers_live() {
    let Some(disk) = disk() else { return };
    let block = |entities| {
        Item::Block(acad_model::Block {
            name: "B".into(),
            base: Point { x: 0.0, y: 0.0 },
            entities,
        })
    };
    let source = Item::Repeat(group(vec![erased(line(1.0)), erased(line(3.0))]));
    // BLOCK copies one live record per found instance; the erased A is
    // copied live through its repeated instance.
    let prior = acad_dwg::parse(&original(&disk, "EOBA", &[&ERASE_A, &BLOCK_ALL])).unwrap();
    assert_eq!(
        prior.items,
        [source.clone(), block(vec![line(3.0), line(1.0), line(3.0)])]
    );
    let whole = acad_dwg::parse(&original(&disk, "EOB", &[&BLOCK_ALL])).unwrap();
    assert_eq!(
        whole.items,
        [
            source,
            block(vec![line(3.0), line(1.0), line(3.0), line(1.0)])
        ]
    );
    // OOPS after BLOCK revives the earlier erased A too.
    let oops = acad_dwg::parse(&original(
        &disk,
        "EOBOOPS",
        &[&ERASE_A, &BLOCK_ALL, &["OOPS"]],
    ))
    .unwrap();
    assert_eq!(
        oops.items[0],
        Item::Repeat(group(vec![line(1.0), line(3.0)]))
    );
}

#[test]
fn original_reopen_has_no_oops_and_end_drops_erased_members() {
    let Some(disk) = disk() else { return };
    let prior = original(&disk, "EOAALL", &[&ERASE_A, &ERASE_ALL]);
    let whole = original(&disk, "EOALL", &[&ERASE_ALL]);
    for source in [&prior, &whole] {
        let area = command_area(&disk, &[("P2.DWG", source)], "2\rP2\rOOPS\r");
        assert_eq!(area[1..], ["*Invalid*", "Command:"]);
        let observation = observe_in_tree(
            &disk,
            b"",
            &[("P2.DWG", source)],
            &[(b"2\rP2\rOOPS\rEND\r", 1500)],
            100_000,
            &["P2.DWG"],
        )
        .unwrap();
        let (_, written) = &observation.created[0];
        // END after reopening writes the marker pair alone: erased members
        // are not kept. The native reader refuses that empty group.
        let (_, meta) = acad_dwg::header::parse_header(written).unwrap();
        assert_eq!(meta.entity_count, 2);
        let records = region(written);
        assert_eq!(&records[..8], &[5, 0, 1, 0, 6, 0, 1, 0]);
        assert_eq!(records.len(), 4 + 24);
        assert!(acad_dwg::parse(written)
            .unwrap_err()
            .to_string()
            .contains("REPEAT needs members"));
    }
}

/// Native: open the original's file with A erased, erase the whole owner by
/// ID, then save. Plain DWG refuses; the confirmed original form writes the
/// original's exact record bytes.
fn native_after(source: Drawing, inputs: &[&str]) -> Drawing {
    let mut editor = acad_cmd::Editor::new(source);
    for input in inputs {
        editor.submit(input).unwrap();
    }
    editor.drawing().clone()
}

#[test]
fn native_confirmed_save_writes_the_originals_whole_group_erasure_bytes() {
    let Some(disk) = disk() else { return };
    let prior = acad_dwg::parse(&original(&disk, "EOA", &[&ERASE_A])).unwrap();
    let whole = original(&disk, "EOAALL", &[&ERASE_A, &ERASE_ALL]);
    let erased_owner = native_after(prior.clone(), &["ERASE", "1"]);
    assert!(group_codec::has_ambiguous_owner(&erased_owner));
    assert!(acad_dwg::write(&erased_owner)
        .unwrap_err()
        .to_string()
        .contains("already erased members"));
    let (converted, count) = group_codec::original_member_erasure(&erased_owner);
    assert_eq!(count, 1);
    let bytes = acad_dwg::write(&converted).unwrap();
    assert_eq!(region(&bytes), region(&whole));
    assert_eq!(
        acad_dwg::parse(&bytes).unwrap().items,
        acad_dwg::parse(&whole).unwrap().items
    );
    for version in [
        acad_dwg::header::Version::Ac12,
        acad_dwg::header::Version::Ac140,
    ] {
        let written = acad_dwg::write_version(&converted, version).unwrap();
        assert_eq!(acad_dwg::parse(&written).unwrap().items, converted.items);
    }
    // BLOCK's retained source becomes the original's source record too.
    let blocked = native_after(prior, &["BLOCK", "B", "0,0", "1"]);
    assert!(acad_dwg::write(&blocked).is_err());
    let (converted, count) = group_codec::original_member_erasure(&blocked);
    assert_eq!(count, 1);
    let reopened = acad_dwg::parse(&acad_dwg::write(&converted).unwrap()).unwrap();
    let original_block =
        acad_dwg::parse(&original(&disk, "EOBA", &[&ERASE_A, &BLOCK_ALL])).unwrap();
    assert_eq!(reopened.items[0], original_block.items[0]);
}
