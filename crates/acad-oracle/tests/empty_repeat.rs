//! In-tree original evidence for the empty REPEAT/ENDREP pair
//! (docs/native-group-persistence.md, "R6 empty REPEAT groups"): how the
//! original writes, keeps, draws, selects and exchanges a group without
//! members. Also regenerates the committed acad-dwg fixtures byte for byte.
//! The System image is only read; drawings are private copies.
#![cfg(unix)]
use acad_oracle::in_tree::{editor_text_rows, generate_dwg_in_tree, observe_in_tree};
use std::collections::BTreeMap;

const ENDED: &[u8] = include_bytes!("../../acad-dwg/tests/fixtures/empty-repeat-ended.dwg");
const NESTED: &[u8] = include_bytes!("../../acad-dwg/tests/fixtures/empty-repeat-nested-ended.dwg");
const ENDED_DXF: &[u8] = include_bytes!("../../acad-dwg/tests/fixtures/empty-repeat-ended.dxf");

fn disk() -> Option<std::path::PathBuf> {
    let disk = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../corpus/raw/Autodesk AutoCAD 1.4 (5.25)/System.img");
    if disk.exists() {
        return Some(disk);
    }
    assert!(
        std::env::var_os("AUTOCAD_REQUIRE_CORPUS").is_none(),
        "{} absent with AUTOCAD_REQUIRE_CORPUS set",
        disk.display()
    );
    eprintln!("skipping in-tree empty REPEAT oracle: extracted System.img absent, NOT validated");
    None
}

const WATCHED: &[&str] = &["P2.DWG", "P2.DXF", "Q2.DWG"];

struct Run {
    files: BTreeMap<String, Vec<u8>>,
    rows: Vec<String>,
    cga: Vec<u8>,
}

fn run(disk: &std::path::Path, extra: &[(&str, &[u8])], keys: &str) -> Run {
    let observation = observe_in_tree(
        disk,
        b"",
        extra,
        &[(keys.as_bytes(), 1500)],
        100_000,
        WATCHED,
    )
    .unwrap();
    assert_eq!(observation.stopped, None, "{}", observation.console);
    Run {
        rows: editor_text_rows(disk, &observation.cga).unwrap(),
        cga: observation.cga,
        files: observation.created.into_iter().collect(),
    }
}

/// The original opens P2.DWG (task 2) and ENDs it.
fn reopen_end(disk: &std::path::Path, dwg: &[u8]) -> Vec<u8> {
    run(disk, &[("P2.DWG", dwg)], "2\rP2\rEND\r").files["P2.DWG"].clone()
}

/// Task 5 (Make DXF) of P2.DWG.
fn make_dxf(disk: &std::path::Path, dwg: &[u8]) -> Vec<u8> {
    run(disk, &[("P2.DWG", dwg)], "5\rP2\r").files["P2.DXF"].clone()
}

fn count(dwg: &[u8]) -> u16 {
    u16::from_le_bytes([dwg[0x28], dwg[0x29]])
}

fn region(dwg: &[u8]) -> &[u8] {
    let end = u32::from_le_bytes(dwg[0x24..0x28].try_into().unwrap()) as usize;
    &dwg[0x202..end]
}

/// `dwg` with its record region replaced (same file length).
fn with_region(dwg: &[u8], records: &[u8], count: u16) -> Vec<u8> {
    let mut out = dwg[..0x202].to_vec();
    out.extend(records);
    let end = out.len() as u32;
    out.resize(dwg.len(), 0);
    out[0x24..0x28].copy_from_slice(&end.to_le_bytes());
    out[0x28..0x2a].copy_from_slice(&count.to_le_bytes());
    out
}

fn endrep(layer: u16, columns: u16, rows: u16, column: f64, row: f64) -> Vec<u8> {
    let mut out = 6i16.to_le_bytes().to_vec();
    out.extend(layer.to_le_bytes());
    out.extend(columns.to_le_bytes());
    out.extend(rows.to_le_bytes());
    out.extend(column.to_le_bytes());
    out.extend(row.to_le_bytes());
    out
}

/// The empty pair: positive REPEAT (layer 1, no body), positive ENDREP
/// 2 columns, 1 row, spacing 5.
fn pair() -> Vec<u8> {
    [vec![5, 0, 1, 0], endrep(1, 2, 1, 5.0, 0.0)].concat()
}

fn line_record() -> Vec<u8> {
    let mut out = 1i16.to_le_bytes().to_vec();
    out.extend(1u16.to_le_bytes());
    for v in [0.0f64, 0.0, 5.0, 5.0] {
        out.extend(v.to_le_bytes());
    }
    out
}

fn entity_section(dxf: &[u8]) -> String {
    let text = String::from_utf8_lossy(dxf).into_owned();
    let text = &text[..text.find('\u{1a}').unwrap()];
    let layerc = text.find("LAYERC,1\r\n").unwrap();
    text[layerc..].splitn(10, "\r\n").nth(9).unwrap().to_owned()
}

const GROUP: [&str; 13] = [
    "REPEAT", "LINE", "1,1", "2,1", "", "LINE", "1,3", "2,3", "", "ENDREP", "2", "1", "5",
];
const NEST: [&str; 18] = [
    "REPEAT", "REPEAT", "LINE", "1,1", "2,1", "", "ENDREP", "2", "1", "5", "LINE", "1,3", "2,3",
    "", "ENDREP", "1", "2", "5",
];

/// The whole group erased by a window (members negative, markers live).
fn all_erased(disk: &std::path::Path) -> Vec<u8> {
    let mut inputs = GROUP.to_vec();
    inputs.extend(["ERASE", "W", "0,0", "10,5"]);
    generate_dwg_in_tree(disk, "EOALL", &inputs).unwrap()
}

fn nested_all_erased(disk: &std::path::Path) -> Vec<u8> {
    let mut inputs = NEST.to_vec();
    inputs.extend(["ERASE", "W", "-1,-1", "20,20"]);
    generate_dwg_in_tree(disk, "NESTE", &inputs).unwrap()
}

#[test]
fn the_committed_fixtures_are_the_originals_own_bytes() {
    let Some(disk) = disk() else { return };
    let ended = reopen_end(&disk, &all_erased(&disk));
    assert_eq!(ended, ENDED, "END after reopening the whole-group erasure");
    assert_eq!(count(&ended), 2);
    assert_eq!(region(&ended), pair());
    assert_eq!(make_dxf(&disk, ENDED), ENDED_DXF, "task 5 of it");
    let nested = nested_all_erased(&disk);
    // Both members negative, all four markers positive.
    let records = region(&nested);
    assert_eq!(&records[..8], &[5, 0, 1, 0, 5, 0, 1, 0]);
    assert_eq!(&records[8..10], &(-1i16).to_le_bytes());
    assert_eq!(&records[44..46], &[6, 0]);
    assert_eq!(&records[68..70], &(-1i16).to_le_bytes());
    assert_eq!(&records[104..106], &[6, 0]);
    let nested_ended = reopen_end(&disk, &nested);
    assert_eq!(
        nested_ended, NESTED,
        "END after reopening the nested erasure"
    );
    assert_eq!(count(NESTED), 4);
    assert_eq!(
        region(NESTED),
        [
            vec![5, 0, 1, 0, 5, 0, 1, 0],
            endrep(1, 2, 1, 5.0, 0.0),
            endrep(1, 1, 2, 5.0, 5.0)
        ]
        .concat()
    );
    // Natively readable now.
    for bytes in [ENDED, NESTED] {
        assert!(acad_dwg::parse(bytes).is_ok());
    }
}

#[test]
fn the_original_writes_and_keeps_an_empty_group() {
    let Some(disk) = disk() else { return };
    // REPEAT and ENDREP typed with nothing between them write the same pair.
    let direct =
        generate_dwg_in_tree(&disk, "DIRECT", &["REPEAT", "ENDREP", "2", "1", "5"]).unwrap();
    assert_eq!(count(&direct), 2);
    assert_eq!(region(&direct), pair());
    // Reopening and ENDing again keeps the pair: the whole file is unchanged.
    assert_eq!(reopen_end(&disk, ENDED), ENDED);
    assert_eq!(reopen_end(&disk, NESTED), NESTED);
    // A new LINE is written after the pair.
    let added = run(
        &disk,
        &[("P2.DWG", ENDED)],
        "2\rP2\rLINE\r0,0\r5,5\r\rEND\r",
    )
    .files["P2.DWG"]
        .clone();
    assert_eq!(count(&added), 3);
    assert_eq!(region(&added), [pair(), line_record()].concat());
}

#[test]
fn the_original_never_draws_or_selects_an_empty_group() {
    let Some(disk) = disk() else { return };
    // A window around everything and Last both find nothing.
    for keys in [
        "2\rP2\rERASE\rW\r-100,-100\r100,100\r\r",
        "2\rP2\rERASE\rL\r\r",
    ] {
        let rows = run(&disk, &[("P2.DWG", ENDED)], keys).rows;
        assert_eq!(rows[22..24], ["0 found.", "Command:"], "{keys:?}");
    }
    // Drawing: the screen with the pair and a LINE equals the screen with
    // the LINE alone (same header).
    let added = run(
        &disk,
        &[("P2.DWG", ENDED)],
        "2\rP2\rLINE\r0,0\r5,5\r\rEND\r",
    )
    .files["P2.DWG"]
        .clone();
    let line_only = with_region(&added, &line_record(), 1);
    let open = |dwg: &[u8]| run(&disk, &[("P2.DWG", dwg)], "2\rP2\r").cga;
    let with_pair = open(&added);
    assert_eq!(with_pair, open(&line_only));
    // Control: the LINE is drawn (the screen differs without it).
    assert_ne!(with_pair, open(&with_region(&added, &[], 0)));
}

#[test]
fn the_original_exchanges_the_empty_group_through_dxf() {
    let Some(disk) = disk() else { return };
    let pair_text = "REPEAT,1\r\nENDREP,1\r\n2,1,5.000000,0.000000\r\n";
    // Task 5 writes the pair, for the empty group and equally for a live
    // group whose members are all erased.
    assert_eq!(entity_section(ENDED_DXF), pair_text);
    assert_eq!(
        entity_section(&make_dxf(&disk, &all_erased(&disk))),
        pair_text
    );
    let nested_text = format!("REPEAT,1\r\n{pair_text}ENDREP,1\r\n1,2,5.000000,5.000000\r\n");
    assert_eq!(entity_section(&make_dxf(&disk, NESTED)), nested_text);
    assert_eq!(
        entity_section(&make_dxf(&disk, &nested_all_erased(&disk))),
        nested_text
    );
    // Task 6 reads the pair back as a live group (after its own zero-filled
    // header placeholders).
    let loaded = run(&disk, &[("Q2.DXF", ENDED_DXF)], "6\rQ2\r").files["Q2.DWG"].clone();
    assert!(region(&loaded).ends_with(&pair()));
    // Natively: the R5 ordinary placeholders stay non-live erased records,
    // the structural ones are dropped, and the live pair is the group.
    let live: Vec<_> = acad_dwg::parse(&loaded)
        .unwrap()
        .items
        .into_iter()
        .filter(|item| !matches!(item, acad_model::Item::Erased(_)))
        .collect();
    assert_eq!(live, acad_dwg::parse(ENDED).unwrap().items);
}
