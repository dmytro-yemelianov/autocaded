use acad_dwg::discover::find_f64;
use acad_dwg::{
    entity::{read_entities, read_items},
    header::parse_header,
    header::HeaderMeta,
    DwgError,
};
use acad_model::{Entity, Item};

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

#[test]
fn adder_preserves_its_seven_erased_records_in_dwg_order() {
    let Some(bytes) = corpus("Samples/ADDER.DWG") else {
        return;
    };
    let drawing = acad_dwg::parse(&bytes).unwrap();
    let erased: Vec<_> = drawing
        .items
        .iter()
        .filter_map(|item| match item {
            Item::Erased(entity) => Some(entity),
            _ => None,
        })
        .collect();
    assert_eq!(erased.len(), 7);
    assert!(erased.iter().all(|entity| matches!(entity, Entity::OnLayer { entity, .. } if matches!(entity.as_ref(), Entity::Line { .. } | Entity::Insert { .. }))));
    let rewritten = acad_dwg::write_version(&drawing, acad_dwg::header::Version::Ac12).unwrap();
    let reopened = acad_dwg::parse(&rewritten).unwrap();
    assert_eq!(
        reopened
            .items
            .iter()
            .filter(|item| matches!(item, Item::Erased(_)))
            .count(),
        7
    );
}

const ENTITY_START: usize = 0x1d8;
const TYPE_LINE: u16 = 1;
const TYPE_ARC: u16 = 8;
const SIZE_LINE: usize = 36;
const SIZE_ARC: usize = 44;

/// The DXF prints six decimals, so a hit can be off by half of the last
/// place; every comparison in this file uses this tolerance.
const TOLERANCE: f64 = 5e-7;

/// Reduces a decoded entity to a comparable `(kind, numeric fields, string
/// field)` triple, so a DWG-vs-DXF comparison checks kind, values *and* any
/// carried string (`TEXT`'s value, `INSERT`'s block name) at each position,
/// not just aggregate per-kind counts.
fn kind_and_fields(e: &Entity) -> (&'static str, Vec<f64>, Option<&str>) {
    match e {
        Entity::Erased(_) => panic!("unexpected erased member in held original live fixture"),
        Entity::Repeat(r) => (
            "REPEAT",
            vec![
                f64::from(r.columns),
                f64::from(r.rows),
                r.column_spacing,
                r.row_spacing,
            ],
            None,
        ),
        Entity::OnLayer { entity, .. } => kind_and_fields(entity),
        Entity::Load { name } => ("LOAD", vec![], Some(name.as_str())),
        Entity::Shape {
            origin,
            height,
            rotation_deg,
            number,
        } => (
            "SHAPE",
            vec![origin.x, origin.y, *height, *rotation_deg, *number as f64],
            None,
        ),
        Entity::Line { start, end } => ("LINE", vec![start.x, start.y, end.x, end.y], None),
        Entity::Circle { center, radius } => ("CIRCLE", vec![center.x, center.y, *radius], None),
        Entity::Arc {
            center,
            radius,
            start_deg,
            end_deg,
        } => (
            "ARC",
            vec![center.x, center.y, *radius, *start_deg, *end_deg],
            None,
        ),
        Entity::Text {
            origin,
            height,
            rotation_deg,
            value,
        } => (
            "TEXT",
            vec![origin.x, origin.y, *height, *rotation_deg],
            Some(value.as_str()),
        ),
        Entity::Insert {
            origin,
            x_scale,
            y_scale,
            rotation_deg,
            name,
        } => (
            "INSERT",
            vec![origin.x, origin.y, *x_scale, *y_scale, *rotation_deg],
            Some(name.as_str()),
        ),
        // SUBDIV, this file's only fixture, uses none of these (Task 8's
        // new types have no DXF oracle at all — see acad-dwg's entity.rs
        // module doc); these arms exist only to keep the match exhaustive.
        Entity::Point { origin } => ("POINT", vec![origin.x, origin.y], None),
        Entity::Trace { p1, p2, p3, p4 } => (
            "TRACE",
            vec![p1.x, p1.y, p2.x, p2.y, p3.x, p3.y, p4.x, p4.y],
            None,
        ),
        Entity::Solid { p1, p2, p3, p4 } => (
            "SOLID",
            vec![p1.x, p1.y, p2.x, p2.y, p3.x, p3.y, p4.x, p4.y],
            None,
        ),
        Entity::Generic(g) => ("GENERIC", vec![], Some(g.type_name.as_str())),
        Entity::Extension(ext) => ("EXTENSION", vec![], Some(ext.type_name())),
    }
}


fn assert_entities_match(a: &Entity, b: &Entity, ctx: &str) {
    let (ka, fa, sa) = kind_and_fields(a);
    let (kb, fb, sb) = kind_and_fields(b);
    assert_eq!(ka, kb, "{ctx}: kind mismatch ({ka} vs {kb})");
    assert_eq!(fa.len(), fb.len(), "{ctx}: field count mismatch");
    for (j, (x, y)) in fa.iter().zip(fb.iter()).enumerate() {
        assert!((x - y).abs() < TOLERANCE, "{ctx} field {j}: {x} vs {y}");
    }
    assert_eq!(sa, sb, "{ctx}: string mismatch");
}

fn assert_same_order(a: &[Entity], b: &[Entity]) {
    assert_eq!(a.len(), b.len(), "entity count mismatch");
    for (i, (ea, eb)) in a.iter().zip(b.iter()).enumerate() {
        assert_entities_match(ea, eb, &format!("record {i}"));
    }
}

/// The DXF's document-order sequence of entity-kind records with `BLOCK`s
/// unwrapped in place — the same flattening `read_entities` performs on the
/// DWG side (it walks past `BLOCK`/`ENDBLK` without emitting anything for
/// them, so a block's entities appear exactly where they sit in the file).
fn flatten_dxf_entities(items: &[Item]) -> Vec<Entity> {
    let mut out = Vec::new();
    for item in items {
        match item {
            Item::Entity(e) => out.push(e.clone()),
            Item::Block(b) => out.extend(b.entities.iter().cloned()),
            Item::Repeat(r) => out.extend(r.entities.iter().cloned()),
            Item::Erased(_) => {}
        }
    }
    out
}

/// Compares two `Item` sequences position by position: kind, fields, and for
/// a `Block`, its name, base point and entities. Plain `assert_eq!` on
/// `Item`/`Entity` would fail on the DXF's six-decimal rounding, so every
/// numeric comparison goes through `TOLERANCE` instead.
fn assert_same_items_order(a: &[Item], b: &[Item]) {
    assert_eq!(a.len(), b.len(), "item count mismatch");
    for (i, (ia, ib)) in a.iter().zip(b.iter()).enumerate() {
        match (ia, ib) {
            (Item::Entity(ea), Item::Entity(eb)) => {
                assert_eq!(layer_of(ea), layer_of(eb), "item {i}: layer mismatch");
                assert_entities_match(ea, eb, &format!("item {i}"))
            }
            (Item::Block(ba), Item::Block(bb)) => {
                assert_eq!(ba.name, bb.name, "item {i}: block name mismatch");
                assert!(
                    (ba.base.x - bb.base.x).abs() < TOLERANCE,
                    "item {i}: block base.x {} vs {}",
                    ba.base.x,
                    bb.base.x
                );
                assert!(
                    (ba.base.y - bb.base.y).abs() < TOLERANCE,
                    "item {i}: block base.y {} vs {}",
                    ba.base.y,
                    bb.base.y
                );
                assert_eq!(
                    ba.entities.len(),
                    bb.entities.len(),
                    "item {i}: block \"{}\" entity count mismatch",
                    ba.name
                );
                for (j, (ea, eb)) in ba.entities.iter().zip(bb.entities.iter()).enumerate() {
                    assert_eq!(layer_of(ea), layer_of(eb), "item {i} entity {j}: layer mismatch");
                    assert_entities_match(ea, eb, &format!("item {i} (block \"{}\") entity {j}", ba.name));
                }
            }
            _ => panic!("item {i}: kind mismatch (Entity vs Block) — DWG's block boundaries don't line up with the DXF's"),
        }
    }
}

fn layer_of(e: &Entity) -> u8 {
    match e {
        Entity::OnLayer { layer, .. } => *layer,
        _ => 1,
    }
}

/// Exercises `LINE` and `ARC` decoding together against genuine corpus bytes,
/// in file order, matching them against the DXF oracle's top-level entities
/// (`Drawing::entities()`, which — like the DWG's leading run, before any
/// `BLOCK`/`INSERT` has appeared — holds no block-interior entities yet).
///
/// The leading `LINE`/`ARC` run sits contiguously from `ENTITY_START` to the
/// first `BLOCK` marker, so this test can hand that whole byte range to the
/// real `read_entities` directly, on a synthetic buffer whose `entity_end`
/// stops short of the real file's end — a boundary case the full-file tests
/// below don't exercise.
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
            _ => break, // the first BLOCK marker
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
        version: acad_dwg::header::Version::Ac12,
        entity_count: top_level_count as u32,
        entity_end: buf.len() as u32,
    };
    let from_dwg = read_entities(&buf, &synth_meta).unwrap();
    assert_eq!(from_dwg.len(), 61);

    let from_dxf = acad_dxf::parse(&dxf).unwrap();
    let from_dxf: Vec<Entity> = from_dxf.entities().take(top_level_count).cloned().collect();
    assert_same_order(&from_dwg, &from_dxf);
}

/// `read_entities` now understands every record type `SUBDIV` uses
/// (`BLOCK`/`ENDBLK` skipped transparently, `INSERT`/`TEXT` decoded), so a
/// real top-to-bottom walk of the whole file completes — no more
/// `UnknownEntityType`, and no more need to stop partway through. This test
/// pins the milestone's headline outcome: 171 physical records (spec §4.4,
/// `entity_count` in the header), of which 159 are entity-shaped.
#[test]
fn read_entities_walks_the_whole_file_without_an_unknown_type_code() {
    let Some(dwg) = corpus("Samples/SUBDIV.DWG") else {
        return;
    };
    let (_, meta) = parse_header(&dwg).unwrap();
    assert_eq!(meta.entity_count, 171, "spec §4.4: 171 records total");

    let entities = read_entities(&dwg, &meta).unwrap();
    assert_eq!(
        entities.len(),
        159,
        "133 LINE + 8 ARC + 2 CIRCLE + 8 TEXT + 8 INSERT (spec §4.4); \
         the other 12 records are the 6 BLOCK/ENDBLK pairs, which read_entities skips"
    );
}

/// Every entity-shaped record in the DWG, walked flat from top to bottom —
/// including the two `CIRCLE`s that sit inside `TREE`/`HYDRANT`'s block
/// bodies, and the `TEXT`/`INSERT` records read_entities newly understands —
/// matches the DXF oracle's own document-order sequence, position by
/// position, once the DXF's blocks are unwrapped in place the same way.
#[test]
fn every_entity_in_the_dwg_matches_the_flattened_dxf_in_file_order() {
    let (Some(dwg), Some(dxf)) = (corpus("Samples/SUBDIV.DWG"), corpus("Samples/SUBDIV.DXF"))
    else {
        return;
    };
    let (_, meta) = parse_header(&dwg).unwrap();
    let from_dwg = read_entities(&dwg, &meta).unwrap();

    let from_dxf = acad_dxf::parse(&dxf).unwrap();
    let from_dxf = flatten_dxf_entities(&from_dxf.items);

    assert_same_order(&from_dwg, &from_dxf);
}

/// `read_items` groups each `BLOCK`/`ENDBLK` pair into one `Item::Block`
/// rather than flattening it, so its output should match the DXF's own
/// `items` sequence position by position — not merely hold the same blocks
/// and entities in aggregate, but have every block boundary fall exactly
/// where the DXF's does among the loose, interleaved entities (spec §4.4:
/// 53 `LINE` records — 61 counting the interleaved `ARC`s, exactly what
/// `every_line_and_arc_before_subdivs_first_block_matches_the_dxf` above
/// verifies — precede the first block, and further blocks appear between
/// inserts).
#[test]
fn read_items_matches_the_dxfs_document_order_including_block_boundaries() {
    let (Some(dwg), Some(dxf)) = (corpus("Samples/SUBDIV.DWG"), corpus("Samples/SUBDIV.DXF"))
    else {
        return;
    };
    let (_, meta) = parse_header(&dwg).unwrap();
    let from_dwg = read_items(&dwg, &meta).unwrap();
    let from_dxf = acad_dxf::parse(&dxf).unwrap();

    assert_same_items_order(&from_dwg, &from_dxf.items);

    // spec §4.4's per-kind counts, checked directly against the DWG's own
    // output (not just inferred from the comparison above matching).
    let block_pairs = from_dwg
        .iter()
        .filter(|i| matches!(i, Item::Block(_)))
        .count();
    assert_eq!(block_pairs, 6, "SUBDIV holds 6 BLOCK/ENDBLK pairs");
    let text_count = from_dwg
        .iter()
        .filter(|i| matches!(i, Item::Entity(Entity::OnLayer { entity, .. }) if matches!(entity.as_ref(), Entity::Text { .. })))
        .count();
    assert_eq!(text_count, 8, "SUBDIV holds 8 TEXT records");
    let insert_count = from_dwg
        .iter()
        .filter(|i| matches!(i, Item::Entity(Entity::OnLayer { entity, .. }) if matches!(entity.as_ref(), Entity::Insert { .. })))
        .count();
    assert_eq!(insert_count, 8, "SUBDIV holds 8 INSERT records");

    let dwg_names: Vec<&str> = from_dwg
        .iter()
        .filter_map(|i| match i {
            Item::Block(b) => Some(b.name.as_str()),
            _ => None,
        })
        .collect();
    let dxf_names: Vec<&str> = from_dxf
        .items
        .iter()
        .filter_map(|i| match i {
            Item::Block(b) => Some(b.name.as_str()),
            _ => None,
        })
        .collect();
    assert_eq!(
        dwg_names, dxf_names,
        "blocks() should yield the same names in the same order as the DXF's"
    );
}

/// `find_f64` still finds each `CIRCLE`'s radius exactly once in the whole
/// file — a corpus-specific property this crate's discovery method has
/// relied on since Task 4 — even now that `read_entities` can reach both
/// `CIRCLE`s by a real walk instead.
#[test]
fn circle_radii_remain_unique_in_the_file() {
    let Some(dwg) = corpus("Samples/SUBDIV.DWG") else {
        return;
    };
    for r in [0.420620_f64, 0.1_f64] {
        let hits = find_f64(&dwg, r, TOLERANCE);
        assert_eq!(hits.len(), 1, "radius {r} should be unique in the file");
    }
}

/// The corpus evidence behind the nested-block-definitions plan's Task 1: a
/// `BLOCK` defined inside another `BLOCK`'s span is a sibling in the flat
/// block table, not a child, so `read_items` now groups both previously-
/// unsupported drawings cleanly instead of erroring.
///
/// The plan's original version of this test only asserted that the inner
/// and outer names appeared *somewhere* in the block list — a bug that put
/// `HEAD`'s entities into `PACKTWR` would have passed it. This version
/// instead checks, for every block `read_items` produces (not just the
/// nested pair): its exact entity count in document order, so cross-
/// contamination between an inner block and its outer one would show up as
/// a count mismatch even with both names still present; and, for each
/// nested pair, that the inner block's own `Item` sits at an earlier index
/// in `items` than its outer block's — matching `read_items`'s documented
/// stack behaviour (the innermost open `BLOCK` is the one popped and
/// emitted when its `ENDBLK` is seen, so it always closes, and therefore
/// appears, before the block it was nested inside).
///
/// The expected names/counts/order below were read directly off a real
/// `read_items` run over each file's own corpus bytes: a temporary
/// diagnostic test printed every `Item::Block`'s name, its index in
/// `items`, and its `entities.len()` for both files
/// (`cargo test -p acad-dwg --test entity_corpus -- --nocapture` against a
/// one-off `for (i, item) in items.iter().enumerate() { ... }` dump), and
/// its output was transcribed here verbatim, then the diagnostic was
/// discarded.
#[test]
fn selexol_and_blivet_group_their_nested_blocks() {
    for (file, count, blocks, nested) in [
        (
            "SELEXOL",
            167usize,
            [
                ("HEAD", 1usize),
                ("PACKTWR", 14),
                ("VESSEL", 4),
                ("ARROW", 6),
                ("COOLER", 3),
                ("FINFAN", 26),
                ("PUMPER", 4),
                ("COMPRESS", 4),
                ("BOX", 4),
                ("UNBOX", 4),
            ]
            .as_slice(),
            [("HEAD", "PACKTWR"), ("ARROW", "COOLER")].as_slice(),
        ),
        (
            "BLIVET",
            149,
            [("$$AROW", 2), ("$BCIRC", 1), ("BLIVET", 18), ("BLUVET", 10)].as_slice(),
            [("$BCIRC", "BLIVET")].as_slice(),
        ),
    ] {
        let Some(bytes) = corpus(&format!("Samples/{file}.DWG")) else {
            return;
        };
        let (_, meta) = parse_header(&bytes).unwrap();
        assert_eq!(meta.entity_count as usize, count, "{file} record count");

        let items =
            read_items(&bytes, &meta).unwrap_or_else(|e| panic!("{file} should now group: {e}"));

        let found: Vec<(&str, usize)> = items
            .iter()
            .filter_map(|i| match i {
                Item::Block(b) => Some((b.name.as_str(), b.entities.len())),
                _ => None,
            })
            .collect();
        if file == "BLIVET" {
            assert_eq!(
                items
                    .iter()
                    .filter(|i| matches!(i, Item::Repeat(_)))
                    .count(),
                3
            );
            assert_eq!(
                items
                    .iter()
                    .filter_map(|i| match i {
                        Item::Block(b) => Some(b),
                        _ => None,
                    })
                    .flat_map(|b| b.entities.iter())
                    .filter(|e| matches!(e, Entity::Repeat(_)))
                    .count(),
                2
            );
        }
        assert_eq!(
            found.as_slice(),
            blocks,
            "{file}: block names/entity counts, in document order"
        );

        let index_of = |name: &str| {
            items
                .iter()
                .position(|i| matches!(i, Item::Block(b) if b.name == name))
                .unwrap_or_else(|| panic!("{file}: no block named {name}"))
        };
        for (inner, outer) in nested {
            let (inner_at, outer_at) = (index_of(inner), index_of(outer));
            assert!(
                inner_at < outer_at,
                "{file}: {inner} (item {inner_at}) should close, and so appear \
                 in `items`, before its outer block {outer} (item {outer_at})"
            );
        }
    }
}

#[test]
fn stopping_the_walk_inside_a_real_block_is_an_unterminated_block_error() {
    // Not a real-corpus scenario (SUBDIV's own BLOCKs are all well-formed —
    // covered by the tests above), but a structural check using genuine
    // bytes: an `entity_end` that lands on a clean record boundary — right
    // after HOUSEA's BLOCK header and its first interior LINE — but still
    // inside HOUSEA's body, well before its own ENDBLK. This is a stopping
    // point read_items must reject, not a physically truncated file (the
    // real bytes beyond `entity_end` are untouched), so it exercises the
    // "no matching ENDBLK before the region ends" check rather than a
    // plain out-of-bounds read.
    let Some(dwg) = corpus("Samples/SUBDIV.DWG") else {
        return;
    };
    let meta = HeaderMeta {
        version: acad_dwg::header::Version::Ac12,
        entity_count: 999, // never reached: the error returns first
        entity_end: 0xaec, // BLOCK (0xaac..0xac8) + one interior LINE (36B)
    };
    match read_items(&dwg, &meta) {
        Err(DwgError::UnterminatedBlock { name, at }) => {
            assert_eq!(name, "HOUSEA");
            assert_eq!(at, 0xaac);
        }
        other => panic!("expected UnterminatedBlock, got {other:?}"),
    }
}
