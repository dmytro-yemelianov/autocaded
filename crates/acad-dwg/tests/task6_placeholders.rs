//! Original Main Menu task 6 (Load DXF) placeholder records
//! (docs/native-group-persistence.md, "Task 6 placeholder records").
//! The fixtures are the original's own bytes (tests/fixtures/README.md);
//! `crates/acad-oracle/tests/task6_placeholders.rs` regenerates them from
//! the original and asserts the original behaviour cited here.
use acad_dwg::{
    header::{parse_header, HeaderMeta, Version},
    DwgError,
};
use acad_model::{Drawing, Entity, Item, Point};

/// Task 6 of the original's own task-5 DXF of one LINE (0,0)-(5,5): twelve
/// zero-filled erased placeholders (-1..-13 without -5), then the LINE.
const RAW: &[u8] = include_bytes!("fixtures/task6-line.dwg");
/// The same drawing after the original opened it (task 2) and ENDed it.
const ENDED: &[u8] = include_bytes!("fixtures/task6-line-ended.dwg");
/// Task 6 of a native DXF that also has DIMARROW: -1..-13 including the
/// erased REPEAT start (-5) directly before the erased ENDREP (-6).
const DIMARROW: &[u8] = include_bytes!("fixtures/task6-dimarrow.dwg");

fn meta(bytes: &[u8]) -> HeaderMeta {
    parse_header(bytes).unwrap().1
}

fn records(bytes: &[u8]) -> &[u8] {
    let meta = meta(bytes);
    &bytes[meta.version.entity_start()..meta.entity_end as usize]
}

fn live_line() -> Vec<Item> {
    vec![Item::Entity(Entity::OnLayer {
        layer: 1,
        entity: Box::new(Entity::Line {
            start: Point { x: 0.0, y: 0.0 },
            end: Point { x: 5.0, y: 5.0 },
        }),
    })]
}

/// `records` behind a native header of `version`, with matching count/end.
fn stream(records: &[u8], count: u16, version: Version) -> Vec<u8> {
    let drawing = acad_dwg::parse(ENDED).unwrap();
    let mut bytes = acad_dwg::write_version(&drawing, version).unwrap();
    bytes.truncate(version.entity_start());
    bytes.extend(records);
    let end = bytes.len() as u32;
    bytes[0x24..0x28].copy_from_slice(&end.to_le_bytes());
    bytes[0x28..0x2a].copy_from_slice(&count.to_le_bytes());
    bytes
}

/// The original's record region in either revision.
fn original(bytes: &[u8], version: Version) -> Vec<u8> {
    match version {
        Version::Ac140 => bytes.to_vec(),
        Version::Ac12 => stream(records(bytes), meta(bytes).entity_count as u16, version),
    }
}

fn record(out: &mut Vec<u8>, kind: i16, layer: u16, body: &[u8]) {
    out.extend(kind.to_le_bytes());
    out.extend(layer.to_le_bytes());
    out.extend(body);
}

fn line(out: &mut Vec<u8>, kind: i16) {
    let body: Vec<u8> = [0.0f64, 0.0, 5.0, 5.0]
        .iter()
        .flat_map(|v| v.to_le_bytes())
        .collect();
    record(out, kind, 1, &body);
}

fn endrep(out: &mut Vec<u8>, kind: i16, columns: u16, row_spacing: f64) {
    let mut body = columns.to_le_bytes().to_vec();
    body.extend(columns.to_le_bytes());
    body.extend(0f64.to_le_bytes());
    body.extend(row_spacing.to_le_bytes());
    record(out, kind, 1, &body);
}

/// Parse `records` in both revisions (which must agree) and return the
/// AC1.40 result; error offsets are AC1.40's (records start at 0x202).
fn items(records: &[u8], count: u16) -> Result<Vec<Item>, DwgError> {
    let mut results = [Version::Ac12, Version::Ac140].map(|version| {
        let bytes = stream(records, count, version);
        let result = acad_dwg::parse(&bytes).map(|d| d.items);
        let entities = acad_dwg::entity::read_entities(&bytes, &meta(&bytes));
        if result.is_ok() {
            assert!(entities.is_ok(), "the geometry view accepts it too");
        }
        result
    });
    match &results {
        [Ok(a), Ok(b)] => assert_eq!(a, b, "both revisions agree"),
        [Err(a), Err(b)] => assert_eq!(
            std::mem::discriminant(a),
            std::mem::discriminant(b),
            "both revisions agree"
        ),
        other => panic!("revisions disagree: {other:?}"),
    }
    std::mem::replace(&mut results[1], Ok(Vec::new()))
}

fn kind(record: &[u8]) -> i16 {
    i16::from_le_bytes([record[0], record[1]])
}

/// Erased REPEAT start/ENDREP, BLOCK, ENDBLK: dropped on read.
fn structural(record: &[u8]) -> bool {
    matches!(kind(record), -5 | -6 | -12 | -13)
}

/// Items without the retained erased (non-live) records.
fn live(items: &[Item]) -> Vec<Item> {
    items
        .iter()
        .filter(|item| !matches!(item, Item::Erased(_)))
        .cloned()
        .collect()
}

fn erased(items: &[Item]) -> usize {
    items
        .iter()
        .filter(|item| matches!(item, Item::Erased(_)))
        .count()
}

/// The records a native rewrite keeps: the ordinary placeholders (as
/// erased records, byte for byte), then the LINE.
fn kept() -> Vec<u8> {
    let mut out: Vec<u8> = placeholders()
        .into_iter()
        .filter(|record| !structural(record))
        .flatten()
        .collect();
    out.extend(records(ENDED));
    out
}

/// The twelve placeholder records of [`RAW`], each with its bytes.
fn placeholders() -> Vec<Vec<u8>> {
    let region = records(RAW);
    let starts = [
        0x000, 0x024, 0x038, 0x054, 0x07a, 0x092, 0x0b8, 0x0e4, 0x128, 0x12e, 0x172, 0x188, 0x18c,
    ];
    starts
        .windows(2)
        .map(|pair| region[pair[0]..pair[1]].to_vec())
        .collect()
}

#[test]
fn the_fixtures_hold_the_measured_placeholder_records() {
    let kinds: Vec<i16> = placeholders().iter().map(|r| kind(r)).collect();
    assert_eq!(kinds, [-1, -2, -3, -4, -6, -7, -8, -9, -10, -11, -12, -13]);
    for placeholder in placeholders() {
        assert_eq!(&placeholder[2..4], &1u16.to_le_bytes(), "DXF layer 1");
        assert!(placeholder[4..].iter().all(|&b| b == 0), "zero-filled");
    }
    assert_eq!(meta(RAW).entity_count, 13);
    assert_eq!(meta(DIMARROW).entity_count, 14);
    assert_eq!(meta(ENDED).entity_count, 1);
    // DIMARROW's -5 sits directly before MODERES's zero-filled -6.
    assert_eq!(&records(DIMARROW)[0x7a..0x7c], &(-5i16).to_le_bytes());
    assert_eq!(&records(DIMARROW)[0x7e..0x80], &(-6i16).to_le_bytes());
}

#[test]
fn the_originals_task6_output_opens_with_only_its_live_records_in_both_revisions() {
    for version in [Version::Ac12, Version::Ac140] {
        for raw in [RAW, DIMARROW] {
            let bytes = original(raw, version);
            let drawing = acad_dwg::parse(&bytes).unwrap();
            assert_eq!(live(&drawing.items), live_line(), "{version:?}");
            // The nine ordinary placeholders stay as non-live erased records.
            assert_eq!(erased(&drawing.items), 9);
            assert_eq!(drawing.items.len(), 10);
            assert_eq!(drawing.entities().count(), 1);
            assert_eq!(
                live(&drawing.items),
                acad_dwg::parse(&original(ENDED, version)).unwrap().items
            );
            let entities = acad_dwg::entity::read_entities(&bytes, &meta(&bytes)).unwrap();
            assert_eq!(entities.len(), 1);
        }
    }
}

#[test]
fn rewriting_drops_structural_placeholders_and_keeps_ordinary_ones_byte_for_byte() {
    assert_eq!(
        kept().len(),
        records(RAW).len() - (0x92 - 0x7a) - (0x18c - 0x172)
    );
    for version in [Version::Ac12, Version::Ac140] {
        for raw in [RAW, DIMARROW] {
            let drawing = acad_dwg::parse(&original(raw, version)).unwrap();
            let rewritten = acad_dwg::write_version(&drawing, version).unwrap();
            assert_eq!(records(&rewritten), kept(), "{version:?}");
            assert_eq!(meta(&rewritten).entity_count, 10);
            let reopened = acad_dwg::parse(&rewritten).unwrap();
            assert_eq!(reopened.items, drawing.items);
            assert_eq!(
                acad_dwg::write_version(&reopened, version).unwrap(),
                rewritten
            );
        }
    }
}

#[test]
fn every_placeholder_is_accepted_alone_at_the_top_level() {
    for placeholder in placeholders() {
        let retained = usize::from(!structural(&placeholder));
        let mut region = placeholder.clone();
        line(&mut region, 1);
        let read = items(&region, 2).unwrap();
        assert_eq!((live(&read), erased(&read)), (live_line(), retained));
        // Also after the live record and on its own.
        let mut after = Vec::new();
        line(&mut after, 1);
        after.extend(&placeholder);
        let read = items(&after, 2).unwrap();
        assert_eq!((live(&read), erased(&read)), (live_line(), retained));
        assert_eq!(items(&placeholder, 1).unwrap().len(), retained);
    }
    // A placeholder keeps the DXF record's layer (measured with layer 3).
    for at in [0x7a, 0x172, 0x188] {
        let mut layered = records(RAW)[at..].to_vec();
        layered[2..4].copy_from_slice(&3u16.to_le_bytes());
        let count = if at == 0x7a {
            9
        } else if at == 0x172 {
            3
        } else {
            2
        };
        assert_eq!(live(&items(&layered, count).unwrap()), live_line());
    }
    // DIMARROW then MODERES: an erased REPEAT start followed directly by a
    // zero-filled erased ENDREP.
    let mut pair = Vec::new();
    record(&mut pair, -5, 1, &[]);
    endrep(&mut pair, -6, 0, 0.0);
    line(&mut pair, 1);
    assert_eq!(items(&pair, 3).unwrap(), live_line());
    // The ordinary placeholders read as erased records with zero fields,
    // the same as a natively erased record at the origin.
    let mut point = Vec::new();
    record(&mut point, -2, 4, &[0u8; 16]);
    assert_eq!(
        items(&point, 1).unwrap(),
        [Item::Erased(Entity::OnLayer {
            layer: 4,
            entity: Box::new(Entity::Point {
                origin: Point { x: 0.0, y: 0.0 }
            }),
        })]
    );
}

#[test]
fn stray_or_malformed_structure_that_is_not_a_placeholder_still_refuses() {
    // A stray erased ENDREP with real fields.
    let mut stray = Vec::new();
    endrep(&mut stray, -6, 3, 5.0);
    assert_eq!(items(&stray, 1), Err(DwgError::StrayEndrep { at: 0x202 }));
    // One nonzero byte is enough to make it a stray ENDREP.
    let mut almost = Vec::new();
    endrep(&mut almost, -6, 0, 5.0);
    assert!(matches!(
        items(&almost, 1),
        Err(DwgError::StrayEndrep { .. })
    ));
    // A zero-filled ENDREP is only a placeholder when erased.
    let mut live = Vec::new();
    endrep(&mut live, 6, 0, 0.0);
    assert!(matches!(items(&live, 1), Err(DwgError::StrayEndrep { .. })));
    // A zero-filled erased ENDREP never closes a group.
    for opening in [-5i16, 5] {
        let mut closed = Vec::new();
        record(&mut closed, opening, 1, &[]);
        line(&mut closed, opening.signum());
        endrep(&mut closed, -6, 0, 0.0);
        assert!(
            matches!(items(&closed, 3), Err(DwgError::InvalidGroupStream { .. })),
            "{opening}"
        );
    }
    // An erased REPEAT start is a placeholder only directly before the
    // zero-filled erased ENDREP: alone or before a live record it still
    // refuses (docs: the original's DIMARROW-only output stays unreadable).
    let mut lone = Vec::new();
    record(&mut lone, -5, 1, &[]);
    assert!(matches!(
        items(&lone, 1),
        Err(DwgError::UnterminatedRepeat { .. })
    ));
    line(&mut lone, 1);
    assert!(matches!(
        items(&lone, 2),
        Err(DwgError::InvalidGroupStream { .. })
    ));
    // Placeholders are recognised at the top level only.
    let mut block = Vec::new();
    let mut header = 1u16.to_le_bytes().to_vec();
    header.extend(b"B");
    header.extend([0u8; 16]);
    record(&mut block, 12, 1, &header);
    line(&mut block, 1);
    let open = block.clone();
    for (inner, count, code) in [
        (placeholders()[10].clone(), 4, 12u16),
        (placeholders()[11].clone(), 4, 13),
    ] {
        let mut nested = open.clone();
        nested.extend(&inner);
        record(&mut nested, 13, 1, &[]);
        assert_eq!(
            items(&nested, count),
            Err(DwgError::UnsupportedErasedStructure {
                code,
                at: 0x202 + open.len()
            })
        );
    }
    let mut pair_in_block = open.clone();
    record(&mut pair_in_block, -5, 1, &[]);
    endrep(&mut pair_in_block, -6, 0, 0.0);
    record(&mut pair_in_block, 13, 1, &[]);
    assert!(matches!(
        items(&pair_in_block, 5),
        Err(DwgError::InvalidGroupStream { .. })
    ));
    // An erased BLOCK with a name is real structure, still unsupported.
    let mut named = Vec::new();
    record(&mut named, -12, 1, &header);
    assert_eq!(
        items(&named, 1),
        Err(DwgError::UnsupportedErasedStructure {
            code: 12,
            at: 0x202
        })
    );
    // An erased ENDBLK whose layer cannot be a DXF layer is not one either.
    let mut wide = Vec::new();
    record(&mut wide, -13, 256, &[]);
    assert!(matches!(
        items(&wide, 1),
        Err(DwgError::UnsupportedErasedStructure { code: 13, .. })
    ));
    // Count and end checks still cover placeholder records.
    let mut short = stream(records(RAW), 12, Version::Ac140);
    assert!(matches!(
        acad_dwg::parse(&short),
        Err(DwgError::EntityCountMismatch { want: 12, got: 13 })
    ));
    short = stream(&records(RAW)[..records(RAW).len() - 1], 13, Version::Ac140);
    assert!(acad_dwg::parse(&short).is_err());
}

#[test]
fn erased_records_that_are_not_placeholders_keep_the_existing_policy() {
    // Nonzero erased root LINE: retained as Item::Erased.
    let mut root = Vec::new();
    line(&mut root, -1);
    let kept = items(&root, 1).unwrap();
    assert!(matches!(kept.as_slice(), [Item::Erased(_)]));
    // A zero-filled erased record inside a live group is a B4 member.
    let mut group = Vec::new();
    record(&mut group, 5, 1, &[]);
    group.extend(&placeholders()[0]);
    line(&mut group, 1);
    endrep(&mut group, 6, 2, 1.0);
    let Item::Repeat(repeat) = &items(&group, 4).unwrap()[0] else {
        panic!("live group");
    };
    assert!(matches!(repeat.entities[0], Entity::Erased(_)));
    // Zero-filled erased ordinary records of any type stay erased records.
    let mut insert = Vec::new();
    record(&mut insert, -14, 1, &[0u8; 42]);
    assert!(matches!(
        items(&insert, 1).unwrap().as_slice(),
        [Item::Erased(_)]
    ));
    // A drawing never re-emits placeholders it did not read.
    let drawing = Drawing {
        header: acad_dwg::parse(ENDED).unwrap().header,
        items: live_line(),
    };
    assert_eq!(records(&acad_dwg::write(&drawing).unwrap()), records(ENDED));
}
