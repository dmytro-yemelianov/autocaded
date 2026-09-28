use acad_dwg::discover::find_f64;
use acad_dwg::{entity::read_entities, header::parse_header, header::HeaderMeta, DwgError};
use acad_model::Entity;

/// The corpus is extracted from archives that are deliberately not in git, so
/// a fresh checkout has none. Tests that need it skip rather than fail.
fn corpus(rel: &str) -> Option<Vec<u8>> {
    match std::fs::read(format!("../../corpus/{rel}")) {
        Ok(b) => Some(b),
        Err(_) => {
            eprintln!("skipping: corpus/{rel} absent (run ./tools/extract-corpus.sh)");
            None
        }
    }
}

const ENTITY_START: usize = 0x1d8;
const TYPE_LINE: u16 = 1;
const TYPE_CIRCLE: u16 = 3;
const TYPE_ARC: u16 = 8;
const TYPE_BLOCK: u16 = 12;
const SIZE_LINE: usize = 36;
const SIZE_CIRCLE: usize = 28;
const SIZE_ARC: usize = 44;

/// Reduces a decoded entity to a comparable `(kind, fields)` pair, or `None`
/// for kinds this corpus test doesn't compare (`Text`/`Insert`, not yet
/// decoded from the DWG). Kept as one function so DWG-vs-DXF order
/// comparisons check both the record kind and its field values at each
/// position, not just aggregate per-kind counts.
fn kind_and_fields(e: &Entity) -> Option<(&'static str, Vec<f64>)> {
    match e {
        Entity::Line { start, end } => Some(("LINE", vec![start.x, start.y, end.x, end.y])),
        Entity::Circle { center, radius } => Some(("CIRCLE", vec![center.x, center.y, *radius])),
        Entity::Arc {
            center,
            radius,
            start_deg,
            end_deg,
        } => Some((
            "ARC",
            vec![center.x, center.y, *radius, *start_deg, *end_deg],
        )),
        Entity::Text { .. } | Entity::Insert { .. } => None,
    }
}

/// The DXF prints six decimals, so a hit can be off by half of the last
/// place; every comparison in this file uses this tolerance.
const TOLERANCE: f64 = 5e-7;

fn assert_same_order(a: &[Entity], b: &[Entity]) {
    let a: Vec<_> = a.iter().filter_map(kind_and_fields).collect();
    let b: Vec<_> = b.iter().filter_map(kind_and_fields).collect();
    assert_eq!(a.len(), b.len(), "entity count mismatch");
    for (i, ((ka, fa), (kb, fb))) in a.iter().zip(b.iter()).enumerate() {
        assert_eq!(ka, kb, "record {i}: kind mismatch ({ka} vs {kb})");
        assert_eq!(fa.len(), fb.len(), "record {i}: field count mismatch");
        for (j, (x, y)) in fa.iter().zip(fb.iter()).enumerate() {
            assert!(
                (x - y).abs() < TOLERANCE,
                "record {i} field {j}: {x} vs {y}"
            );
        }
    }
}

/// `SUBDIV`'s entity records begin `LINE`/`ARC`, 61 of them (53 `LINE`, 8
/// `ARC` — spec §4.4), before its first `BLOCK` marker (type code 12) at file
/// offset `0xaac`. With `LINE`, `CIRCLE` and `ARC` implemented, `BLOCK` is the
/// first record type `read_entities` still cannot decode (Task 5's job), so
/// a top-to-bottom walk of the real file now gets 61 records further than
/// Task 3 left it (which stopped at record 0, an `ARC`) before hitting that
/// wall. This test pins the new, real-file stopping point.
#[test]
fn read_entities_stops_at_subdivs_first_block_marker() {
    let Some(dwg) = corpus("Samples/SUBDIV.DWG") else {
        return;
    };
    let (_, meta) = parse_header(&dwg).unwrap();
    assert_eq!(
        read_entities(&dwg, &meta).unwrap_err(),
        DwgError::UnknownEntityType {
            code: TYPE_BLOCK,
            at: 0xaac,
        }
    );
}

/// Exercises `LINE` and `ARC` decoding together against genuine corpus bytes,
/// in file order, matching them against the DXF oracle's top-level entities
/// (`Drawing::entities()`, which — like the DWG's flat record stream up to
/// the first `BLOCK` — holds no block-interior entities yet).
///
/// The leading `LINE`/`ARC` run sits contiguously from `ENTITY_START` to the
/// first `BLOCK` marker, so (unlike Task 3, which had to copy out `LINE`
/// bytes and skip over undecoded `ARC` bytes one record at a time) this test
/// can hand that whole byte range to the real `read_entities` directly: both
/// record kinds in it are now understood. The scan below still walks
/// record-by-record first, but only to find where the run ends and to
/// double check every record in it really is `LINE` or `ARC` before trusting
/// the bulk copy.
#[test]
fn every_line_and_arc_before_subdivs_first_block_matches_the_dxf() {
    let (Some(dwg), Some(dxf)) = (corpus("Samples/SUBDIV.DWG"), corpus("Samples/SUBDIV.DXF"))
    else {
        return;
    };

    let (_, meta) = parse_header(&dwg).unwrap();
    let mut pos = ENTITY_START;
    let mut top_level_count = 0usize;
    loop {
        let code = u16::from_le_bytes(dwg[pos..pos + 2].try_into().unwrap());
        match code {
            TYPE_LINE => pos += SIZE_LINE,
            TYPE_ARC => pos += SIZE_ARC,
            _ => break, // the first BLOCK marker (Task 5's job) — stop here
        }
        top_level_count += 1;
    }
    // Sanity: the scan above must actually have stopped short of the real
    // entity region's end, on a genuine "next entity" boundary, not run off
    // the file.
    assert!((pos as u32) < meta.entity_end);
    assert_eq!(
        top_level_count, 61,
        "SUBDIV holds 61 LINE/ARC records before its first BLOCK"
    );
    assert_eq!(pos, 0xaac, "the run should end exactly at the BLOCK marker");

    let mut buf = vec![0u8; ENTITY_START];
    buf.extend_from_slice(&dwg[ENTITY_START..pos]);
    let synth_meta = HeaderMeta {
        entity_count: top_level_count as u32,
        entity_end: buf.len() as u32,
    };
    let from_dwg = read_entities(&buf, &synth_meta).unwrap();
    assert_eq!(from_dwg.len(), 61);

    let from_dxf = acad_dxf::parse(&dxf).unwrap();
    // Every record in the scanned prefix is LINE or ARC (no BLOCK/INSERT has
    // appeared yet to make `entities()` skip ahead), so its first
    // `top_level_count` top-level DXF entities line up 1:1, in order, with
    // the DWG's first `top_level_count` physical records.
    let from_dxf: Vec<Entity> = from_dxf.entities().take(top_level_count).cloned().collect();
    assert_same_order(&from_dwg, &from_dxf);
}

/// `SUBDIV` holds 2 `CIRCLE`s (spec §4.4), but both sit inside block
/// definitions (`TREE`, `HYDRANT`) — the DXF text shows `CIRCLE` between a
/// `BLOCK,1`/name pair and its `ENDBLK,1`. `Drawing::entities()` skips block
/// bodies by design (`acad-model`'s `drawing.rs`), and in the DWG itself
/// those records sit *after* the first `BLOCK` marker, which `read_entities`
/// cannot yet walk past (Task 5's job). So neither side of the walk-forward
/// comparison the other two tests use can reach a `CIRCLE` today.
///
/// Instead, each `CIRCLE`'s own real bytes are located directly, the same
/// way `dwg-discover` recovers any record's layout: searching the DWG for a
/// DXF-reported field value as a little-endian `f64`. Radius is the search
/// key because — verified below with `assert_eq!(hits.len(), 1, ..)` — each
/// of SUBDIV's two circle radii (0.420620, 0.1) appears exactly once in the
/// whole file, so the match is unambiguous without needing to also search
/// centre.x/centre.y. The record header's type code is then checked before
/// trusting the match, so a coincidental radius collision elsewhere in the
/// file could not silently mislead this test.
#[test]
fn every_circle_in_the_dxf_matches_the_dwg() {
    let (Some(dwg), Some(dxf)) = (corpus("Samples/SUBDIV.DWG"), corpus("Samples/SUBDIV.DXF"))
    else {
        return;
    };
    let dxf = acad_dxf::parse(&dxf).unwrap();

    let circles: Vec<(f64, f64, f64)> = dxf
        .blocks()
        .flat_map(|b| b.entities.iter())
        .filter_map(|e| match e {
            Entity::Circle { center, radius } => Some((center.x, center.y, *radius)),
            _ => None,
        })
        .collect();
    assert_eq!(circles.len(), 2, "SUBDIV holds 2 CIRCLEs (spec §4.4)");

    for (cx, cy, r) in circles {
        let hits = find_f64(&dwg, r, TOLERANCE);
        assert_eq!(
            hits.len(),
            1,
            "radius {r} should identify exactly one CIRCLE record in the file"
        );
        let r_at = hits[0].offset;
        // Record layout (established fact): 4-byte header, then centre.x,
        // centre.y, radius as consecutive doubles — so radius sits 20 bytes
        // after the header.
        let header_at = r_at - 20;
        let type_code = u16::from_le_bytes(dwg[header_at..header_at + 2].try_into().unwrap());
        assert_eq!(type_code, TYPE_CIRCLE, "expected a CIRCLE record header");

        let mut buf = vec![0u8; ENTITY_START];
        buf.extend_from_slice(&dwg[header_at..header_at + SIZE_CIRCLE]);
        let meta = HeaderMeta {
            entity_count: 1,
            entity_end: buf.len() as u32,
        };
        let decoded = read_entities(&buf, &meta).unwrap();
        assert_eq!(decoded.len(), 1);
        let Entity::Circle { center, radius } = &decoded[0] else {
            panic!("expected a CIRCLE, got {:?}", decoded[0]);
        };
        assert!((center.x - cx).abs() < TOLERANCE, "{} vs {cx}", center.x);
        assert!((center.y - cy).abs() < TOLERANCE, "{} vs {cy}", center.y);
        assert!((*radius - r).abs() < TOLERANCE, "{radius} vs {r}");
    }
}
