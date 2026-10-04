//! The original's empty REPEAT/ENDREP pair (docs/native-group-persistence.md,
//! "R6 empty REPEAT groups"). The fixtures are the original's own bytes
//! (tests/fixtures/README.md); `crates/acad-oracle/tests/empty_repeat.rs`
//! regenerates them and asserts the original behaviour cited here: the
//! original keeps the pair as a live group without members, re-saves it
//! byte for byte, never draws or window-selects it, and exports it to DXF.
use acad_dwg::{
    header::{parse_header, HeaderMeta, Version},
    DwgError,
};
use acad_model::{Drawing, Entity, Item, Repeat};

/// The original's END after reopening its whole-group erasure of a
/// two-LINE, two-column group: the marker pair alone (count 2).
const ENDED: &[u8] = include_bytes!("fixtures/empty-repeat-ended.dwg");
/// The same for a group nested in a group: the outer pair holding the
/// empty inner pair (count 4).
const NESTED: &[u8] = include_bytes!("fixtures/empty-repeat-nested-ended.dwg");

fn meta(bytes: &[u8]) -> HeaderMeta {
    parse_header(bytes).unwrap().1
}

fn records(bytes: &[u8]) -> &[u8] {
    let meta = meta(bytes);
    &bytes[meta.version.entity_start()..meta.entity_end as usize]
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

fn empty(columns: u16, rows: u16, column_spacing: f64, row_spacing: f64) -> Repeat {
    Repeat {
        start_layer: 1,
        end_layer: 1,
        entities: Vec::new(),
        columns,
        rows,
        column_spacing,
        row_spacing,
    }
}

fn ended_items() -> Vec<Item> {
    vec![Item::Repeat(empty(2, 1, 5.0, 0.0))]
}

fn nested_items() -> Vec<Item> {
    let mut outer = empty(1, 2, 5.0, 5.0);
    outer.entities = vec![Entity::Repeat(empty(2, 1, 5.0, 0.0))];
    vec![Item::Repeat(outer)]
}

fn record(out: &mut Vec<u8>, kind: i16, layer: u16, body: &[u8]) {
    out.extend(kind.to_le_bytes());
    out.extend(layer.to_le_bytes());
    out.extend(body);
}

fn endrep_body(columns: u16, rows: u16, column_spacing: f64, row_spacing: f64) -> Vec<u8> {
    let mut body = columns.to_le_bytes().to_vec();
    body.extend(rows.to_le_bytes());
    body.extend(column_spacing.to_le_bytes());
    body.extend(row_spacing.to_le_bytes());
    body
}

fn line(out: &mut Vec<u8>, kind: i16) {
    let body: Vec<u8> = [0.0f64, 0.0, 5.0, 5.0]
        .iter()
        .flat_map(|v| v.to_le_bytes())
        .collect();
    record(out, kind, 1, &body);
}

/// Parse `records` in both revisions (which must agree) and return the
/// AC1.40 result.
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

/// Rewrite in `version` and return the record region.
fn rewrite(drawing: &Drawing, version: Version) -> Vec<u8> {
    let bytes = acad_dwg::write_version(drawing, version).unwrap();
    assert_eq!(meta(&bytes).version, version);
    records(&bytes).to_vec()
}

#[test]
fn the_originals_empty_pair_opens_as_a_live_group_without_members() {
    for (bytes, want, count, live) in [
        (ENDED, ended_items(), 2u16, 0),
        (NESTED, nested_items(), 4, 1),
    ] {
        let drawing = acad_dwg::parse(bytes).unwrap();
        assert_eq!(drawing.items, want);
        assert_eq!(meta(bytes).entity_count, u32::from(count));
        // No geometry anywhere: the geometry view is empty; the live entity
        // iterator only holds the nested (empty) group record.
        assert_eq!(
            acad_dwg::entity::read_entities(bytes, &meta(bytes)).unwrap(),
            []
        );
        assert_eq!(drawing.entities().count(), live);
        // Same records behind either revision's header.
        assert_eq!(items(records(bytes), count).unwrap(), want);
    }
}

#[test]
fn the_empty_pair_resaves_byte_for_byte_in_both_revisions() {
    for bytes in [ENDED, NESTED] {
        let drawing = acad_dwg::parse(bytes).unwrap();
        for version in [Version::Ac140, Version::Ac12] {
            let region = rewrite(&drawing, version);
            assert_eq!(region, records(bytes), "{version:?}");
            let written = acad_dwg::write_version(&drawing, version).unwrap();
            assert_eq!(meta(&written).entity_count, meta(bytes).entity_count);
            let reopened = acad_dwg::parse(&written).unwrap();
            assert_eq!(reopened.items, drawing.items);
            // Stable: a second rewrite is identical.
            assert_eq!(
                acad_dwg::write_version(&reopened, version).unwrap(),
                written
            );
        }
    }
    // The native AC1.40 header round trip keeps the original's whole file
    // for the flat pair (header included).
    assert_eq!(
        acad_dwg::write(&acad_dwg::parse(ENDED).unwrap()).unwrap(),
        ENDED
    );
}

#[test]
fn empty_groups_in_any_position_round_trip() {
    // A live LINE before and after, an empty group inside a block, and an
    // empty group beside a member in a live group.
    let mut group = empty(1, 1, 0.0, 0.0);
    group.entities = vec![
        Entity::OnLayer {
            layer: 1,
            entity: Box::new(Entity::Line {
                start: acad_model::Point { x: 0.0, y: 0.0 },
                end: acad_model::Point { x: 5.0, y: 5.0 },
            }),
        },
        Entity::Repeat(empty(2, 2, 1.0, 1.0)),
    ];
    let mut drawing = acad_dwg::parse(ENDED).unwrap();
    drawing.items = vec![
        Item::Block(acad_model::Block {
            name: "B".into(),
            base: acad_model::Point { x: 0.0, y: 0.0 },
            entities: vec![Entity::Repeat(empty(4, 1, 2.0, 0.0))],
        }),
        Item::Repeat(group),
        Item::Repeat(empty(7, 9, -1.0, 3.0)),
    ];
    for version in [Version::Ac140, Version::Ac12] {
        let written = acad_dwg::write_version(&drawing, version).unwrap();
        assert_eq!(acad_dwg::parse(&written).unwrap().items, drawing.items);
    }
    // Hand-made records: line, empty pair with its own layers, line.
    let mut bytes = Vec::new();
    line(&mut bytes, 1);
    record(&mut bytes, 5, 3, &[]);
    record(&mut bytes, 6, 4, &endrep_body(3, 2, 1.5, -2.0));
    line(&mut bytes, 1);
    let read = items(&bytes, 4).unwrap();
    let mut layered = empty(3, 2, 1.5, -2.0);
    layered.start_layer = 3;
    layered.end_layer = 4;
    assert_eq!(read[1], Item::Repeat(layered));
    assert_eq!(read.len(), 3);
}

#[test]
fn a_natively_erased_empty_group_round_trips_with_negative_markers() {
    let mut drawing = acad_dwg::parse(ENDED).unwrap();
    drawing.items = vec![Item::Erased(Entity::Repeat(empty(2, 1, 5.0, 0.0)))];
    for version in [Version::Ac140, Version::Ac12] {
        let region = rewrite(&drawing, version);
        let mut want = Vec::new();
        record(&mut want, -5, 1, &[]);
        record(&mut want, -6, 1, &endrep_body(2, 1, 5.0, 0.0));
        assert_eq!(region, want);
        let written = acad_dwg::write_version(&drawing, version).unwrap();
        assert_eq!(acad_dwg::parse(&written).unwrap().items, drawing.items);
    }
}

#[test]
fn malformed_groups_are_still_refused() {
    let pair = |start: i16, end: i16, body: Vec<u8>| {
        let mut out = Vec::new();
        record(&mut out, start, 1, &[]);
        record(&mut out, end, 1, &body);
        out
    };
    let invalid = |records: &[u8], count| {
        assert!(
            matches!(
                items(records, count),
                Err(DwgError::InvalidGroupStream { .. })
            ),
            "{:?}",
            items(records, count)
        );
    };
    // Zero dimensions and nonfinite spacings, either sign (nonfinite
    // fields are refused while decoding the record).
    for sign in [1i16, -1] {
        for body in [endrep_body(0, 1, 5.0, 0.0), endrep_body(2, 0, 5.0, 0.0)] {
            invalid(&pair(5 * sign, 6 * sign, body), 2);
        }
        for body in [
            endrep_body(2, 1, f64::NAN, 0.0),
            endrep_body(2, 1, 5.0, f64::INFINITY),
        ] {
            assert!(matches!(
                items(&pair(5 * sign, 6 * sign, body), 2),
                Err(DwgError::NonFiniteEntity { .. })
            ));
        }
    }
    // Mixed signs.
    invalid(&pair(5, -6, endrep_body(2, 1, 5.0, 0.0)), 2);
    invalid(&pair(-5, 6, endrep_body(2, 1, 5.0, 0.0)), 2);
    // A negative empty subgroup inside a live group.
    let mut nested = Vec::new();
    record(&mut nested, 5, 1, &[]);
    nested.extend(pair(-5, -6, endrep_body(2, 1, 5.0, 0.0)));
    record(&mut nested, 6, 1, &endrep_body(2, 1, 5.0, 0.0));
    invalid(&nested, 4);
    // Unclosed and stray markers.
    let mut lone = Vec::new();
    record(&mut lone, 5, 1, &[]);
    assert!(matches!(
        items(&lone, 1),
        Err(DwgError::UnterminatedRepeat { .. })
    ));
    let mut stray = Vec::new();
    record(&mut stray, 6, 1, &endrep_body(2, 1, 5.0, 0.0));
    assert!(matches!(
        items(&stray, 1),
        Err(DwgError::StrayEndrep { .. })
    ));
    // Count and end must still land exactly.
    let ok = pair(5, 6, endrep_body(2, 1, 5.0, 0.0));
    assert!(matches!(
        items(&ok, 3),
        Err(DwgError::EntityCountMismatch { .. })
    ));
    assert!(items(&ok[..ok.len() - 1], 2).is_err());
    // The task 6 placeholder pair (zero-filled -6 after -5) is still the
    // only dropped form; with valid dimensions the pair is a group.
    assert_eq!(items(&pair(-5, -6, vec![0; 20]), 2).unwrap(), []);
    assert_eq!(
        items(&pair(-5, -6, endrep_body(2, 1, 5.0, 0.0)), 2).unwrap(),
        [Item::Erased(Entity::Repeat(empty(2, 1, 5.0, 0.0)))]
    );
}

#[test]
fn checked_writers_still_refuse_malformed_empty_groups() {
    let mut drawing = acad_dwg::parse(ENDED).unwrap();
    for bad in [
        empty(0, 1, 5.0, 0.0),
        empty(2, 0, 5.0, 0.0),
        empty(2, 1, f64::NAN, 0.0),
    ] {
        drawing.items = vec![Item::Repeat(bad)];
        for version in [Version::Ac140, Version::Ac12] {
            assert!(acad_dwg::write_version(&drawing, version).is_err());
        }
    }
}

/// Every accepted mutation must re-encode to exactly its own record region
/// in both revisions: nothing the reader accepts is silently dropped,
/// reordered or rewritten. Every refusal is a typed error, never a panic.
fn check_mutation(region: &[u8], count: u16, stats: &mut [usize; 2]) {
    for version in [Version::Ac140, Version::Ac12] {
        let bytes = stream(region, count, version);
        let Ok(drawing) = acad_dwg::parse(&bytes) else {
            stats[1] += 1;
            continue;
        };
        stats[0] += 1;
        let _ = acad_dwg::entity::read_entities(&bytes, &meta(&bytes));
        let written = acad_dwg::write_version(&drawing, version)
            .unwrap_or_else(|e| panic!("accepted but not writable: {e} {region:02x?}"));
        assert_eq!(
            records(&written),
            region,
            "accepted input changed on rewrite ({version:?})"
        );
        assert_eq!(meta(&written).entity_count, u32::from(count));
        assert_eq!(acad_dwg::parse(&written).unwrap().items, drawing.items);
    }
}

/// Physical record boundaries of a fixture region (REPEAT start has no
/// body, ENDREP has 20 bytes).
fn boundaries(region: &[u8]) -> Vec<std::ops::Range<usize>> {
    let mut out = Vec::new();
    let mut pos = 0;
    while pos < region.len() {
        let kind = i16::from_le_bytes([region[pos], region[pos + 1]]).unsigned_abs();
        let len = if kind == 5 { 4 } else { 24 };
        out.push(pos..pos + len);
        pos += len;
    }
    out
}

#[test]
fn broad_mutations_never_panic_or_lose_content() {
    let mut stats = [0usize; 2];
    for bytes in [ENDED, NESTED] {
        let region = records(bytes).to_vec();
        let count = meta(bytes).entity_count as u16;
        // Every byte set to edge values, and every bit flipped.
        for at in 0..region.len() {
            let mut values = vec![0x00, 0xff, 0x80, 0x01, 0x7f, 0x05, 0x06, 0xfa, 0xfb];
            values.extend((0..8).map(|bit| region[at] ^ (1 << bit)));
            for value in values {
                let mut mutated = region.clone();
                mutated[at] = value;
                check_mutation(&mutated, count, &mut stats);
            }
        }
        // Every truncation and every count.
        for len in 0..region.len() {
            for c in [0, count.saturating_sub(1), count, count + 1] {
                check_mutation(&region[..len], c, &mut stats);
            }
        }
        for c in 0..=8 {
            check_mutation(&region, c, &mut stats);
        }
        // Sign flips of each marker, record deletion, duplication and swaps.
        let records = boundaries(&region);
        for subset in 0u32..(1 << records.len()) {
            let mut mutated = region.clone();
            for (i, range) in records.iter().enumerate() {
                if subset & (1 << i) != 0 {
                    let kind = -i16::from_le_bytes([region[range.start], region[range.start + 1]]);
                    mutated[range.start..range.start + 2].copy_from_slice(&kind.to_le_bytes());
                }
            }
            check_mutation(&mutated, count, &mut stats);
        }
        for i in 0..records.len() {
            let without: Vec<u8> = records
                .iter()
                .enumerate()
                .filter(|(j, _)| *j != i)
                .flat_map(|(_, r)| region[r.clone()].to_vec())
                .collect();
            check_mutation(&without, count - 1, &mut stats);
            for j in 0..=records.len() {
                let mut order: Vec<_> = records.clone();
                order.insert(j, records[i].clone());
                let dup: Vec<u8> = order
                    .iter()
                    .flat_map(|r| region[r.clone()].to_vec())
                    .collect();
                check_mutation(&dup, count + 1, &mut stats);
            }
            for j in 0..records.len() {
                let mut order = records.clone();
                order.swap(i, j);
                let swapped: Vec<u8> = order
                    .iter()
                    .flat_map(|r| region[r.clone()].to_vec())
                    .collect();
                check_mutation(&swapped, count, &mut stats);
            }
        }
    }
    eprintln!("mutations: {} accepted, {} refused", stats[0], stats[1]);
    // Both outcomes are exercised.
    assert!(stats[0] > 100 && stats[1] > 100, "{stats:?}");
}

/// The original's task 5 DXF of `ENDED` (tests/fixtures/README.md).
const ENDED_DXF: &[u8] = include_bytes!("fixtures/empty-repeat-ended.dxf");

#[test]
fn dxf_exchange_keeps_the_empty_pair_like_the_original() {
    let drawing = acad_dwg::parse(ENDED).unwrap();
    let dxf = acad_dxf::parse(ENDED_DXF).unwrap();
    assert_eq!(dxf.items, drawing.items);
    // The native DXF of the original's drawing is the original's DXF up to
    // its end-of-file marker (the rest is the original's sector padding).
    let written = acad_dxf::try_write(&drawing).unwrap();
    let end = ENDED_DXF.iter().position(|&b| b == 0x1a).unwrap();
    assert_eq!(written, ENDED_DXF[..=end]);
    // Nested: DXF round trip keeps both pairs, and the DWG re-encodes to
    // the original's bytes.
    let nested = acad_dwg::parse(NESTED).unwrap();
    let back = acad_dxf::parse(&acad_dxf::try_write(&nested).unwrap()).unwrap();
    assert_eq!(back.items, nested.items);
    assert_eq!(
        rewrite(&back, Version::Ac140),
        records(NESTED),
        "DWG -> DXF -> DWG keeps the original's records"
    );
}
