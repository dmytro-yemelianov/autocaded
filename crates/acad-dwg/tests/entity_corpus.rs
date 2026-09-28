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

fn lines<'a>(it: impl Iterator<Item = &'a Entity>) -> Vec<(f64, f64, f64, f64)> {
    it.filter_map(|e| match e {
        Entity::Line { start, end } => Some((start.x, start.y, end.x, end.y)),
        _ => None,
    })
    .collect()
}

/// SUBDIV's very first entity record — the one sitting at `ENTITY_START` — is
/// an `ARC` (type code 8), not a `LINE`: Task 2's discovery run found DXF
/// entity 0 is `ARC,1` at file offset `0x1dc` (its centre), which puts the
/// record's own type code 4 bytes earlier, at `0x1d8`. `ARC` is Task 4's job,
/// so `read_entities` walking the *whole* real file cannot get past record 0
/// today. This test pins that exact, real-file failure — it is the
/// "simpler" tolerance strategy the brief offers, and it double-checks
/// `ENTITY_START` and the type-code read against genuine bytes, not just the
/// synthetic fixture in `entity.rs`.
#[test]
fn read_entities_stops_at_subdivs_first_entity_an_unimplemented_arc() {
    let Some(dwg) = corpus("Samples/SUBDIV.DWG") else {
        return;
    };
    let (_, meta) = parse_header(&dwg).unwrap();
    assert_eq!(
        read_entities(&dwg, &meta).unwrap_err(),
        DwgError::UnknownEntityType {
            code: 8,
            at: 0x1d8, // ENTITY_START
        }
    );
}

/// The above means a single top-to-bottom `read_entities(&dwg, &meta)` call
/// yields zero `LINE`s from the real file — `ARC` blocks it immediately. To
/// still exercise `LINE` decoding against genuine corpus bytes (not just the
/// synthetic fixture), this test re-assembles the real `LINE` records that
/// precede SUBDIV's first `BLOCK` marker: every record in that stretch is
/// either `LINE` (36 bytes, Task 2) or `ARC` (44 bytes, the brief's own facts
/// table) — both fixed sizes — so the `LINE` records can be located and
/// concatenated into a synthetic buffer *without* decoding `ARC` itself, then
/// handed to the real `read_entities`. This is the "skip known record sizes"
/// tolerance strategy from the brief, scoped to test-only navigation rather
/// than changing `read_entities`'s contract.
#[test]
fn every_line_before_subdivs_first_block_matches_the_dxf() {
    let (Some(dwg), Some(dxf)) = (corpus("Samples/SUBDIV.DWG"), corpus("Samples/SUBDIV.DXF"))
    else {
        return;
    };

    const ENTITY_START: usize = 0x1d8;
    const TYPE_LINE: u16 = 1;
    const TYPE_ARC: u16 = 8;
    const SIZE_LINE: usize = 36;
    const SIZE_ARC: usize = 44;

    let (_, meta) = parse_header(&dwg).unwrap();
    let mut pos = ENTITY_START;
    let mut top_level_count = 0usize;
    let mut real_line_bytes = Vec::new();
    loop {
        let code = u16::from_le_bytes(dwg[pos..pos + 2].try_into().unwrap());
        match code {
            TYPE_LINE => {
                real_line_bytes.extend_from_slice(&dwg[pos..pos + SIZE_LINE]);
                pos += SIZE_LINE;
            }
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

    let mut buf = vec![0u8; ENTITY_START];
    buf.extend_from_slice(&real_line_bytes);
    let synth_meta = HeaderMeta {
        entity_count: (real_line_bytes.len() / SIZE_LINE) as u32,
        entity_end: buf.len() as u32,
    };
    let from_dwg = read_entities(&buf, &synth_meta).unwrap();

    let from_dxf = acad_dxf::parse(&dxf).unwrap();
    // Every record in the scanned prefix is LINE or ARC (no BLOCK/INSERT has
    // appeared yet to make `entities()` skip ahead), so its first
    // `top_level_count` top-level DXF entities line up 1:1 with the DWG's
    // first `top_level_count` physical records.
    let a = lines(from_dwg.iter());
    let b = lines(from_dxf.entities().take(top_level_count));

    assert_eq!(
        a.len(),
        53,
        "SUBDIV's leading stretch holds 53 LINEs before its first BLOCK"
    );
    assert_eq!(a.len(), b.len());
    // The DXF prints six decimals, so compare to that precision rather than
    // demanding the bits agree.
    for (i, (p, q)) in a.iter().zip(b.iter()).enumerate() {
        for (x, y) in [(p.0, q.0), (p.1, q.1), (p.2, q.2), (p.3, q.3)] {
            assert!((x - y).abs() < 5e-7, "LINE {i}: {x} vs {y}");
        }
    }
}
