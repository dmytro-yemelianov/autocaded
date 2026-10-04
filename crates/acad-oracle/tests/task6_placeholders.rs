//! In-tree original evidence for the Load DXF (Main Menu task 6)
//! placeholder records (docs/native-group-persistence.md, "Task 6
//! placeholder records"): what the original writes, and what its END does
//! with them. Also regenerates the committed acad-dwg fixtures byte for
//! byte. The System image is only read; drawings are private copies.
#![cfg(unix)]
use acad_model::{Entity, Point};
use acad_oracle::in_tree::{generate_dwg_in_tree, observe_in_tree};
use std::collections::BTreeMap;

const RAW: &[u8] = include_bytes!("../../acad-dwg/tests/fixtures/task6-line.dwg");
const ENDED: &[u8] = include_bytes!("../../acad-dwg/tests/fixtures/task6-line-ended.dwg");
const DIMARROW: &[u8] = include_bytes!("../../acad-dwg/tests/fixtures/task6-dimarrow.dwg");

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
    eprintln!(
        "skipping in-tree task 6 placeholder oracle: extracted System.img absent, NOT validated"
    );
    None
}

const WATCHED: &[&str] = &["D2.DWG", "D2.DXF", "D3.DWG", "D3.BAK", "D3.DXF"];

fn run(disk: &std::path::Path, extra: &[(&str, &[u8])], keys: &str) -> BTreeMap<String, Vec<u8>> {
    let observation = observe_in_tree(
        disk,
        b"",
        extra,
        &[(keys.as_bytes(), 400)],
        100_000,
        WATCHED,
    )
    .unwrap();
    assert_eq!(observation.stopped, None, "{}", observation.console);
    observation.created.into_iter().collect()
}

/// Task 6 of `dxf` into a new D3.DWG.
fn load(disk: &std::path::Path, dxf: &[u8]) -> Vec<u8> {
    run(disk, &[("D3.DXF", dxf)], "6\rD3\r")["D3.DWG"].clone()
}

/// The original opens D3.DWG (task 2) and ENDs it unchanged.
fn reopen_end(disk: &std::path::Path, dwg: &[u8]) -> Vec<u8> {
    run(disk, &[("D3.DWG", dwg)], "2\rD3\rEND\r")["D3.DWG"].clone()
}

fn count(dwg: &[u8]) -> u16 {
    u16::from_le_bytes([dwg[0x28], dwg[0x29]])
}

fn region(dwg: &[u8]) -> &[u8] {
    let end = u32::from_le_bytes(dwg[0x24..0x28].try_into().unwrap()) as usize;
    &dwg[0x202..end]
}

fn word(bytes: &[u8], at: usize) -> i16 {
    i16::from_le_bytes([bytes[at], bytes[at + 1]])
}

/// One LINE (0,0)-(5,5) on layer 1, as a 36-byte record.
fn line_record() -> Vec<u8> {
    let mut out = 1i16.to_le_bytes().to_vec();
    out.extend(1u16.to_le_bytes());
    for v in [0.0f64, 0.0, 5.0, 5.0] {
        out.extend(v.to_le_bytes());
    }
    out
}

fn one_line() -> Vec<Entity> {
    vec![Entity::Line {
        start: Point { x: 0.0, y: 0.0 },
        end: Point { x: 5.0, y: 5.0 },
    }]
}

fn live(dwg: &[u8]) -> Vec<Entity> {
    acad_dwg::parse(dwg)
        .unwrap()
        .entities()
        .map(|mut entity| {
            while let Entity::OnLayer { entity: inner, .. } = entity {
                entity = inner;
            }
            entity.clone()
        })
        .collect()
}

/// The original's task 5 DXF of its own one-LINE drawing.
fn original_dxf(disk: &std::path::Path) -> Vec<u8> {
    let dwg = generate_dwg_in_tree(disk, "D2", &["LINE", "0,0", "5,5", ""]).unwrap();
    run(disk, &[("D2.DWG", &dwg)], "5\rD2\r")["D2.DXF"].clone()
}

/// The native DXF of the same drawing with a DIMARROW record.
fn native_dimarrow_dxf() -> Vec<u8> {
    let mut drawing = acad_dwg::parse(ENDED).unwrap();
    drawing.header.dim_arrow = Some(0.18);
    acad_dxf::try_write(&drawing).unwrap()
}

#[test]
fn the_committed_fixtures_are_the_originals_own_bytes() {
    let Some(disk) = disk() else { return };
    let dxf = original_dxf(&disk);
    let raw = load(&disk, &dxf);
    assert_eq!(raw, RAW, "task 6 of the original's own DXF");
    assert_eq!(reopen_end(&disk, &raw), ENDED, "the original's END of it");
    let dimarrow = load(&disk, &native_dimarrow_dxf());
    assert_eq!(dimarrow, DIMARROW, "task 6 of a native DXF with DIMARROW");
    assert_eq!(region(&reopen_end(&disk, &dimarrow)), region(ENDED));
    // Natively readable, holding only the live LINE.
    for bytes in [RAW, ENDED, DIMARROW] {
        assert_eq!(live(bytes), one_line());
    }
}

#[test]
fn each_header_record_becomes_one_zero_filled_erased_record_of_its_keyword_index() {
    let Some(disk) = disk() else { return };
    let text = String::from_utf8(native_dimarrow_dxf()).unwrap();
    let mut records: Vec<String> = Vec::new();
    for line in text.trim_end_matches('\u{1a}').split_inclusive("\r\n") {
        if line.starts_with(|c: char| c.is_ascii_uppercase()) {
            records.push(String::new());
        }
        records.last_mut().unwrap().push_str(line);
    }
    let line = records.pop().unwrap();
    assert!(line.starts_with("LINE,1"), "{line}");
    let keywords: Vec<&str> = records.iter().map(|r| &r[..r.find(',').unwrap()]).collect();
    assert_eq!(
        keywords,
        [
            "EXTENTS",
            "LIMITS",
            "BASE",
            "DWGVIEW",
            "DIMARROW",
            "MODERES",
            "MODEGRID",
            "MODEORTHO",
            "MODEFILL",
            "TXTSIZE",
            "TRACEWID",
            "LAYER",
            "LAYERC"
        ]
    );
    // The keyword index the original uses: DIMARROW is 5, MODERES 6.
    let index = |keyword: &str| match keyword {
        "EXTENTS" => 1,
        "LIMITS" => 2,
        "BASE" => 3,
        "DWGVIEW" => 4,
        "DIMARROW" => 5,
        "MODERES" => 6,
        "MODEGRID" => 7,
        "MODEORTHO" => 8,
        "MODEFILL" => 9,
        "TXTSIZE" => 10,
        "TRACEWID" => 11,
        "LAYER" => 12,
        _ => 13,
    };
    for (record, keyword) in records.iter().zip(&keywords) {
        // The record's own layer field (3 here) becomes the placeholder's.
        let record = record.replacen(",1\r\n", ",3\r\n", 1);
        let dwg = load(&disk, format!("{record}{line}\u{1a}").as_bytes());
        let records = region(&dwg);
        assert_eq!(count(&dwg), 2, "{keyword}");
        assert_eq!(word(records, 0), -index(keyword), "{keyword}");
        assert_eq!(word(records, 2), 3, "{keyword}");
        let (placeholder, rest) = records.split_at(records.len() - 36);
        assert!(placeholder[4..].iter().all(|&b| b == 0), "{keyword}");
        assert_eq!(rest, line_record(), "{keyword}");
        if *keyword == "DIMARROW" {
            // An erased REPEAT start with no zero-filled ENDREP after it
            // stays refused natively (documented limit).
            assert!(acad_dwg::parse(&dwg).is_err());
        } else {
            assert_eq!(live(&dwg), one_line(), "{keyword}");
        }
    }
    // Placeholders sit where the header record was, even after entities.
    let after = load(
        &disk,
        format!("{line}TXTSIZE,1\r\n0.200000\r\n\u{1a}").as_bytes(),
    );
    let records = region(&after);
    assert_eq!(&records[..36], line_record());
    assert_eq!(word(records, 36), -10);
    assert_eq!(live(&after), one_line());
    // No header records, no placeholders.
    let plain = load(&disk, format!("{line}\u{1a}").as_bytes());
    assert_eq!(region(&plain), line_record());
}

#[test]
fn the_originals_end_drops_placeholders_and_every_erased_record() {
    let Some(disk) = disk() else { return };
    // Unchanged reopen + END: only the live LINE is kept (fixture test).
    assert_eq!(region(&reopen_end(&disk, RAW)), line_record());
    // After an edit too.
    let edited = run(&disk, &[("D3.DWG", RAW)], "2\rD3\rLINE\r1,1\r2,2\r\rEND\r")["D3.DWG"].clone();
    assert_eq!(count(&edited), 2);
    assert_eq!(&region(&edited)[..36], line_record());
    assert!(region(&edited).len() == 72 && word(region(&edited), 36) == 1);
    // QUIT leaves the file as it was.
    let quit = run(&disk, &[("D3.DWG", RAW)], "2\rD3\rQUIT\rY\r")["D3.DWG"].clone();
    assert_eq!(quit, RAW);
    // An ordinary record ERASEd in the session is kept by that session's
    // END, but the next session's END drops it as well.
    let erased = generate_dwg_in_tree(
        &disk,
        "D3",
        &[
            "LINE", "0,0", "5,5", "", "LINE", "1,1", "2,2", "", "ERASE", "L",
        ],
    )
    .unwrap();
    assert_eq!(count(&erased), 2);
    assert_eq!(word(region(&erased), 36), -1);
    assert_eq!(region(&reopen_end(&disk, &erased)), line_record());
}

#[test]
fn placeholders_are_not_drawing_content_for_the_original() {
    let Some(disk) = disk() else { return };
    // Task 5 of the placeholder file equals task 5 of its ENDed form.
    let make = |dwg: &[u8]| run(&disk, &[("D3.DWG", dwg)], "5\rD3\r")["D3.DXF"].clone();
    assert_eq!(make(RAW), make(ENDED));
    // Task 6 into an existing drawing writes another placeholder run after
    // the existing records; the result opens natively.
    let dxf = original_dxf(&disk);
    let appended = run(&disk, &[("D3.DWG", RAW), ("D3.DXF", &dxf)], "6\rD3\r")["D3.DWG"].clone();
    assert_eq!(count(&appended), 26);
    let records = region(&appended);
    assert_eq!(&records[..0x18c], &region(RAW)[..0x18c]);
    assert_eq!(word(records, 0x18c), 1);
    assert_eq!(word(records, 0x1b0), -1);
    assert_eq!(live(&appended), [one_line(), one_line()].concat());
}
