use super::record::*;
use super::*;
use crate::header::{HeaderMeta, Version};
use crate::DwgError;
const ENTITY_START: usize = 0x1D8;
use acad_model::{Entity, Item, Point};

/// One `LINE` record, laid out as Task 2's discovery run showed: the
/// type code and the layer word, then x1, y1, x2, y2 as little-endian
/// doubles. `read_entities` always starts its walk at the fixed
/// `ENTITY_START` (Task 2 verified this holds across six real files), so
/// this fixture pads up to that offset first — the brief's own draft of
/// this test predates `ENTITY_START` being pinned to a nonzero value and
/// wrote the record at offset 0, which no longer matches production
/// behaviour; see `task-3-report.md` for the note.
fn one_line(x1: f64, y1: f64, x2: f64, y2: f64) -> Vec<u8> {
    let mut r = vec![0u8; ENTITY_START];
    r.extend_from_slice(&TYPE_LINE.to_le_bytes());
    r.extend_from_slice(&0u16.to_le_bytes()); // the layer word
    for v in [x1, y1, x2, y2] {
        r.extend_from_slice(&v.to_le_bytes());
    }
    r
}

/// One `CIRCLE` record: the type code and layer word, then centre.x,
/// centre.y, radius as little-endian doubles (4 + 3*8 = 28 bytes).
fn one_circle(cx: f64, cy: f64, radius: f64) -> Vec<u8> {
    let mut r = vec![0u8; ENTITY_START];
    r.extend_from_slice(&TYPE_CIRCLE.to_le_bytes());
    r.extend_from_slice(&0u16.to_le_bytes());
    for v in [cx, cy, radius] {
        r.extend_from_slice(&v.to_le_bytes());
    }
    r
}

/// One `ARC` record: the type code and layer word, then centre.x,
/// centre.y, radius, start angle, end angle — the last two in
/// **radians**, as the DWG stores them (4 + 5*8 = 44 bytes).
fn one_arc(cx: f64, cy: f64, radius: f64, start_rad: f64, end_rad: f64) -> Vec<u8> {
    let mut r = vec![0u8; ENTITY_START];
    r.extend_from_slice(&TYPE_ARC.to_le_bytes());
    r.extend_from_slice(&0u16.to_le_bytes());
    for v in [cx, cy, radius, start_rad, end_rad] {
        r.extend_from_slice(&v.to_le_bytes());
    }
    r
}

/// A record's bytes with **no** `ENTITY_START` padding, for composing
/// several records into one buffer (`one_line`/`one_circle`/`one_arc`
/// each pad on their own, which only works for a single-record buffer).
fn raw_text(x: f64, y: f64, height_raw: f64, rotation_rad: f64, value: &str) -> Vec<u8> {
    let mut r = Vec::new();
    r.extend_from_slice(&TYPE_TEXT.to_le_bytes());
    r.extend_from_slice(&0u16.to_le_bytes());
    for v in [x, y, height_raw, rotation_rad] {
        r.extend_from_slice(&v.to_le_bytes());
    }
    r.extend_from_slice(&(value.len() as u16).to_le_bytes());
    r.extend_from_slice(value.as_bytes());
    r
}

fn raw_block(name: &str, base_x: f64, base_y: f64) -> Vec<u8> {
    let mut r = Vec::new();
    r.extend_from_slice(&TYPE_BLOCK.to_le_bytes());
    r.extend_from_slice(&0u16.to_le_bytes());
    r.extend_from_slice(&(name.len() as u16).to_le_bytes());
    r.extend_from_slice(name.as_bytes());
    for v in [base_x, base_y] {
        r.extend_from_slice(&v.to_le_bytes());
    }
    r
}

fn raw_endblk() -> Vec<u8> {
    let mut r = Vec::new();
    r.extend_from_slice(&TYPE_ENDBLK.to_le_bytes());
    r.extend_from_slice(&0u16.to_le_bytes());
    r
}

/// `SELEXOL`'s own shape (module doc / Task 1 brief): `BLOCK "OUTER"`, a
/// `LINE`, `BLOCK "INNER"`, a `LINE`, `ENDBLK` (closes `INNER`), a `LINE`,
/// `ENDBLK` (closes `OUTER`) — 7 records. `INNER` holds one `LINE`
/// (record 4); `OUTER` holds two, records 2 and 6 — not `INNER`'s.
fn nested_block_fixture() -> Vec<u8> {
    records(&[
        raw_block("OUTER", 0.0, 0.0),
        raw_line(0.0, 0.0, 1.0, 1.0),
        raw_block("INNER", 1.0, 1.0),
        raw_line(2.0, 2.0, 3.0, 3.0),
        raw_endblk(),
        raw_line(4.0, 4.0, 5.0, 5.0),
        raw_endblk(),
    ])
}

/// A single `ENDBLK` with no `BLOCK` ever open.
fn lone_endblk_fixture() -> Vec<u8> {
    records(&[raw_endblk()])
}

/// Two `BLOCK`s opened back to back, neither ever closed: `OUTER` then
/// `INNER`, then a `LINE`, then `entity_end`. The innermost open block
/// (`INNER`) is the one that should be named in the error.
fn unterminated_nest_fixture() -> Vec<u8> {
    records(&[
        raw_block("OUTER", 0.0, 0.0),
        raw_block("INNER", 1.0, 1.0),
        raw_line(0.0, 0.0, 1.0, 1.0),
    ])
}

fn raw_insert(
    name: &str,
    x: f64,
    y: f64,
    x_scale: f64,
    y_scale: f64,
    rotation_rad: f64,
) -> Vec<u8> {
    let mut r = Vec::new();
    r.extend_from_slice(&TYPE_INSERT.to_le_bytes());
    r.extend_from_slice(&0u16.to_le_bytes());
    r.extend_from_slice(&(name.len() as u16).to_le_bytes());
    r.extend_from_slice(name.as_bytes());
    for v in [x, y, x_scale, y_scale, rotation_rad] {
        r.extend_from_slice(&v.to_le_bytes());
    }
    r
}

fn raw_circle(cx: f64, cy: f64, radius: f64) -> Vec<u8> {
    let mut r = Vec::new();
    r.extend_from_slice(&TYPE_CIRCLE.to_le_bytes());
    r.extend_from_slice(&0u16.to_le_bytes());
    for v in [cx, cy, radius] {
        r.extend_from_slice(&v.to_le_bytes());
    }
    r
}

fn raw_line(x1: f64, y1: f64, x2: f64, y2: f64) -> Vec<u8> {
    let mut r = Vec::new();
    r.extend_from_slice(&TYPE_LINE.to_le_bytes());
    r.extend_from_slice(&0u16.to_le_bytes());
    for v in [x1, y1, x2, y2] {
        r.extend_from_slice(&v.to_le_bytes());
    }
    r
}

/// An erased record: the same bytes as its live counterpart, except the
/// type code is written as a negative `i16` (spec §4.2). `magnitude`
/// must be a real, positive entity-table code.
fn erase(magnitude: u16, mut record: Vec<u8>) -> Vec<u8> {
    let negated = -(magnitude as i16);
    record[0..2].copy_from_slice(&negated.to_le_bytes());
    record
}

fn raw_point(x: f64, y: f64) -> Vec<u8> {
    let mut r = Vec::new();
    r.extend_from_slice(&TYPE_POINT.to_le_bytes());
    r.extend_from_slice(&0u16.to_le_bytes());
    for v in [x, y] {
        r.extend_from_slice(&v.to_le_bytes());
    }
    r
}

fn raw_quad(type_code: u16, corners: [(f64, f64); 4]) -> Vec<u8> {
    let mut r = Vec::new();
    r.extend_from_slice(&type_code.to_le_bytes());
    r.extend_from_slice(&0u16.to_le_bytes());
    for (x, y) in corners {
        r.extend_from_slice(&x.to_le_bytes());
        r.extend_from_slice(&y.to_le_bytes());
    }
    r
}

fn raw_trace(corners: [(f64, f64); 4]) -> Vec<u8> {
    raw_quad(TYPE_TRACE, corners)
}

fn raw_solid(corners: [(f64, f64); 4]) -> Vec<u8> {
    raw_quad(TYPE_SOLID, corners)
}

/// Two independent physical records: a four-byte REPEAT marker, then a
/// LINE header and body. The child's layer can differ from the marker layer.
fn raw_repeat_line(child_layer: u16, x1: f64, y1: f64, x2: f64, y2: f64) -> Vec<u8> {
    let mut r = Vec::new();
    r.extend_from_slice(&TYPE_REPEAT.to_le_bytes());
    r.extend_from_slice(&0u16.to_le_bytes());
    r.extend_from_slice(&TYPE_LINE.to_le_bytes());
    r.extend_from_slice(&child_layer.to_le_bytes());
    for v in [x1, y1, x2, y2] {
        r.extend_from_slice(&v.to_le_bytes());
    }
    r
}

/// A REPEAT marker followed by an independent INSERT — evidenced by BLIVET's
/// `"$BCIRC"` repeat (module doc).
fn raw_repeat_insert(
    name: &str,
    x: f64,
    y: f64,
    x_scale: f64,
    y_scale: f64,
    rotation_rad: f64,
) -> Vec<u8> {
    let mut r = Vec::new();
    r.extend_from_slice(&TYPE_REPEAT.to_le_bytes());
    r.extend_from_slice(&0u16.to_le_bytes());
    r.extend_from_slice(&TYPE_INSERT.to_le_bytes());
    r.extend_from_slice(&1u16.to_le_bytes());
    r.extend_from_slice(&(name.len() as u16).to_le_bytes());
    r.extend_from_slice(name.as_bytes());
    for v in [x, y, x_scale, y_scale, rotation_rad] {
        r.extend_from_slice(&v.to_le_bytes());
    }
    r
}

/// A REPEAT marker followed by an unsupported ordinary record type.
fn raw_repeat_unsupported(child_type: u16) -> Vec<u8> {
    let mut r = Vec::new();
    r.extend_from_slice(&TYPE_REPEAT.to_le_bytes());
    r.extend_from_slice(&0u16.to_le_bytes());
    r.extend_from_slice(&child_type.to_le_bytes());
    r.extend_from_slice(&1u16.to_le_bytes());
    r
}

/// A fixed 24-byte `ENDREP`: header, a `u16` pair, two doubles — none of
/// columns/rows and spacings interpreted by structural assembly.
fn raw_endrep(u1: u16, u2: u16, d1: f64, d2: f64) -> Vec<u8> {
    let mut r = Vec::new();
    r.extend_from_slice(&TYPE_ENDREP.to_le_bytes());
    r.extend_from_slice(&0u16.to_le_bytes());
    r.extend_from_slice(&u1.to_le_bytes());
    r.extend_from_slice(&u2.to_le_bytes());
    for v in [d1, d2] {
        r.extend_from_slice(&v.to_le_bytes());
    }
    r
}

/// Pads to `ENTITY_START`, then concatenates every record's raw bytes —
/// the multi-record equivalent of `one_line`/`one_circle`/`one_arc`.
fn records(parts: &[Vec<u8>]) -> Vec<u8> {
    let mut r = vec![0u8; ENTITY_START];
    for p in parts {
        r.extend_from_slice(p);
    }
    r
}

fn one_text(x: f64, y: f64, height_raw: f64, rotation_rad: f64, value: &str) -> Vec<u8> {
    records(&[raw_text(x, y, height_raw, rotation_rad, value)])
}

fn one_block(name: &str, base_x: f64, base_y: f64) -> Vec<u8> {
    records(&[raw_block(name, base_x, base_y)])
}

fn one_insert(
    name: &str,
    x: f64,
    y: f64,
    x_scale: f64,
    y_scale: f64,
    rotation_rad: f64,
) -> Vec<u8> {
    records(&[raw_insert(name, x, y, x_scale, y_scale, rotation_rad)])
}

#[test]
fn reads_a_line_record() {
    // TYPE_LINE is 1: the type code indexes ACAD.EXE's entity name
    // table, whose first entry is LINE.
    let bytes = one_line(1.012459, 6.822910, 1.261682, 6.822910);
    let meta = HeaderMeta {
        version: Version::Ac12,
        entity_count: 1,
        entity_end: bytes.len() as u32,
    };
    let entities = read_entities(&bytes, &meta).unwrap();
    assert_eq!(entities.len(), 1);
    let Entity::Line { start, end } = &entities[0] else {
        panic!("expected a LINE, got {:?}", entities[0]);
    };
    assert_eq!(start.x, 1.012459);
    assert_eq!(end.y, 6.822910);
}

#[test]
fn load_and_shape_preserve_fields_order_and_truncation_errors() {
    let mut bytes = vec![0; 0x202];
    bytes.extend_from_slice(b"\x0a\0\x01\0\x04\0B:ES");
    bytes.extend_from_slice(b"\x04\0\x01\0");
    // Generated SHAPE RES 2.25,3.5 0.75 30: direct scale, radians,
    // and ES.SHP's definition number 129 at the end of the record.
    for v in [2.25f64, 3.5, 0.75, 30f64.to_radians()] {
        bytes.extend_from_slice(&v.to_le_bytes());
    }
    bytes.extend_from_slice(&129u16.to_le_bytes());
    let meta = HeaderMeta {
        version: Version::Ac140,
        entity_count: 2,
        entity_end: bytes.len() as u32,
    };
    let entities = read_entities(&bytes, &meta).unwrap();
    assert_eq!(entities.len(), 2);
    assert_eq!(
        entities[0],
        Entity::Load {
            name: "B:ES".into()
        }
    );
    let Entity::Shape {
        origin,
        height,
        rotation_deg,
        number,
    } = entities[1]
    else {
        panic!("expected SHAPE")
    };
    assert_eq!(origin, Point { x: 2.25, y: 3.5 });
    assert_eq!(height, 0.75);
    assert!((rotation_deg - 30.0).abs() < 1e-12);
    assert_eq!(number, 129);
    assert_eq!(
        read_items(&bytes, &meta).unwrap(),
        entities
            .iter()
            .cloned()
            .map(|e| Item::Entity(Entity::OnLayer {
                layer: 1,
                entity: Box::new(e),
            }))
            .collect::<Vec<_>>()
    );
    for cut in 0x202..bytes.len() {
        assert!(
            matches!(
                read_items(&bytes[..cut], &meta),
                Err(DwgError::TruncatedEntity { .. })
            ),
            "cut {cut:#x}"
        );
    }
    // Erasure must consume the complete LOAD body before the SHAPE.
    bytes[0x202..0x204].copy_from_slice(&(-10i16).to_le_bytes());
    assert_eq!(read_entities(&bytes, &meta).unwrap(), entities[1..]);
}

#[test]
fn reads_a_circle_record() {
    // TYPE_CIRCLE is 3: ACAD.EXE's entity name table, third entry.
    // Values are SUBDIV's TREE-block CIRCLE (spec's discovery run).
    let bytes = one_circle(16.464680, 12.562830, 0.420620);
    let meta = HeaderMeta {
        version: Version::Ac12,
        entity_count: 1,
        entity_end: bytes.len() as u32,
    };
    let entities = read_entities(&bytes, &meta).unwrap();
    assert_eq!(entities.len(), 1);
    let Entity::Circle { center, radius } = &entities[0] else {
        panic!("expected a CIRCLE, got {:?}", entities[0]);
    };
    assert_eq!(center.x, 16.464680);
    assert_eq!(center.y, 12.562830);
    assert_eq!(*radius, 0.420620);
}

#[test]
fn reads_an_arc_record_converting_radians_to_degrees() {
    // TYPE_ARC is 8. Values are SUBDIV's very first entity record (at
    // ENTITY_START itself): DWG doubles 3.037728, 2.906527, 2.339596,
    // 1.371900350566874, 0.39184212503599475 radians, matching the DXF's
    // 3.037728, 2.906527, 2.339596, 78.604100, 22.450900 degrees.
    let bytes = one_arc(
        3.037728,
        2.906527,
        2.339596,
        1.371900350566874,
        0.39184212503599475,
    );
    let meta = HeaderMeta {
        version: Version::Ac12,
        entity_count: 1,
        entity_end: bytes.len() as u32,
    };
    let entities = read_entities(&bytes, &meta).unwrap();
    assert_eq!(entities.len(), 1);
    let Entity::Arc {
        center,
        radius,
        start_deg,
        end_deg,
    } = &entities[0]
    else {
        panic!("expected an ARC, got {:?}", entities[0]);
    };
    assert_eq!(center.x, 3.037728);
    assert_eq!(center.y, 2.906527);
    assert_eq!(*radius, 2.339596);
    // Not normalised: this sweeps counter-clockwise through 0°, start >
    // end, and that is exactly what SUBDIV stores.
    assert!((start_deg - 78.604100).abs() < 1e-6, "{start_deg}");
    assert!((end_deg - 22.450900).abs() < 1e-6, "{end_deg}");
}

#[test]
fn reads_a_text_record_scaling_the_stored_height() {
    // TYPE_TEXT is 7. Values are SUBDIV's own "A" label at file offset
    // 0x1718: DWG doubles 9.624130, 12.295160, 0.4613464999999999, 0.0 —
    // matching the DXF's 9.624130, 12.295160, 0.346010, 0.000000. The
    // stored height needs the 0.75 factor (see read_record_body's TEXT
    // arm); without it this test fails with height == 0.4613465.
    let bytes = one_text(9.624130, 12.295160, 0.4613464999999999, 0.0, "A");
    let meta = HeaderMeta {
        version: Version::Ac12,
        entity_count: 1,
        entity_end: bytes.len() as u32,
    };
    let entities = read_entities(&bytes, &meta).unwrap();
    assert_eq!(entities.len(), 1);
    let Entity::Text {
        origin,
        height,
        rotation_deg,
        value,
    } = &entities[0]
    else {
        panic!("expected a TEXT, got {:?}", entities[0]);
    };
    assert_eq!(origin.x, 9.624130);
    assert_eq!(origin.y, 12.295160);
    assert!((height - 0.346010).abs() < 5e-7, "{height}");
    assert_eq!(*rotation_deg, 0.0);
    assert_eq!(value, "A");
}

#[test]
fn reads_ac140_text_at_its_extended_offset_without_height_scaling() {
    // Values independently verified by the original TEXT command in
    // acad-oracle/tests/commands.rs. This regression also runs without
    // the original disk images or QEMU.
    let mut bytes = vec![0u8; 0x202];
    bytes.extend(raw_text(2.25, 3.5, 0.75, 30f64.to_radians(), "ORACLE"));
    let meta = HeaderMeta {
        version: Version::Ac140,
        entity_count: 1,
        entity_end: bytes.len() as u32,
    };
    let entities = read_entities(&bytes, &meta).unwrap();
    assert_eq!(entities.len(), 1);
    let Entity::Text {
        origin,
        height,
        rotation_deg,
        value,
    } = &entities[0]
    else {
        panic!("expected TEXT, got {:?}", entities[0]);
    };
    assert_eq!(*origin, Point { x: 2.25, y: 3.5 });
    assert_eq!(*height, 0.75);
    assert!((rotation_deg - 30.0).abs() < 1e-12);
    assert_eq!(value, "ORACLE");
    assert_eq!(
        read_items(&bytes, &meta).unwrap(),
        vec![Item::Entity(Entity::OnLayer {
            layer: 0,
            entity: Box::new(entities[0].clone()),
        })]
    );
}

#[test]
fn reads_a_text_records_value_as_latin1() {
    // Distinct from text.rs's own decode_latin1 tests: this confirms
    // TEXT's length-prefixed value is actually routed through
    // decode_latin1 by read_record_body, not just that the function
    // works in isolation.
    let value_bytes = [b'c', b'a', b'f', 0xe9];
    let mut bytes = one_text(0.0, 0.0, 1.0, 0.0, "");
    // one_text's empty value leaves a zero-length string; splice in the
    // 4 raw (non-UTF-8) bytes and fix up the length prefix by hand,
    // since `one_text` only accepts `&str`.
    let strlen_at = bytes.len() - 2; // the trailing empty-string's length prefix
    bytes.truncate(strlen_at);
    bytes.extend_from_slice(&(value_bytes.len() as u16).to_le_bytes());
    bytes.extend_from_slice(&value_bytes);
    let meta = HeaderMeta {
        version: Version::Ac12,
        entity_count: 1,
        entity_end: bytes.len() as u32,
    };
    let entities = read_entities(&bytes, &meta).unwrap();
    let Entity::Text { value, .. } = &entities[0] else {
        panic!("expected a TEXT, got {:?}", entities[0]);
    };
    assert_eq!(value, "café");
}

#[test]
fn reads_a_text_record_converting_rotation_radians_to_degrees() {
    // SUBDIV's own 8 TEXT records all have rotation 0, which can't
    // distinguish "converted" from "not converted". This uses a
    // synthetic, clearly-non-corpus rotation to exercise the conversion
    // path itself: π/4 rad is 45°.
    let bytes = one_text(0.0, 0.0, 1.0, std::f64::consts::FRAC_PI_4, "X");
    let meta = HeaderMeta {
        version: Version::Ac12,
        entity_count: 1,
        entity_end: bytes.len() as u32,
    };
    let entities = read_entities(&bytes, &meta).unwrap();
    let Entity::Text { rotation_deg, .. } = &entities[0] else {
        panic!("expected a TEXT, got {:?}", entities[0]);
    };
    assert!((rotation_deg - 45.0).abs() < 1e-9, "{rotation_deg}");
}

#[test]
fn reads_an_insert_record_converting_radians_to_degrees() {
    // TYPE_INSERT is 14. Values are SUBDIV's rotated HOUSEA insert (file
    // offset 0x1998): DWG doubles 3.214121, 5.297620, 1.0, 1.0,
    // 0.593373373901603 radians, matching the DXF's 3.214121, 5.297620,
    // 1.000000, 1.000000, 33.997790 degrees.
    let bytes = one_insert("HOUSEA", 3.214121, 5.297620, 1.0, 1.0, 0.593373373901603);
    let meta = HeaderMeta {
        version: Version::Ac12,
        entity_count: 1,
        entity_end: bytes.len() as u32,
    };
    let entities = read_entities(&bytes, &meta).unwrap();
    assert_eq!(entities.len(), 1);
    let Entity::Insert {
        origin,
        x_scale,
        y_scale,
        rotation_deg,
        name,
    } = &entities[0]
    else {
        panic!("expected an INSERT, got {:?}", entities[0]);
    };
    assert_eq!(origin.x, 3.214121);
    assert_eq!(origin.y, 5.297620);
    assert_eq!(*x_scale, 1.0);
    assert_eq!(*y_scale, 1.0);
    assert!((rotation_deg - 33.997790).abs() < 1e-5, "{rotation_deg}");
    assert_eq!(name, "HOUSEA");
}

#[test]
fn read_entities_skips_block_and_endblk_but_keeps_interior_entities() {
    // acad_model::Entity has no block variant, so read_entities can't
    // represent BLOCK/ENDBLK — it walks over them without producing an
    // element, while still decoding whatever they contain (here, one
    // CIRCLE) exactly like a top-level record.
    let bytes = records(&[
        raw_block("B1", 1.0, 2.0),
        raw_circle(0.0, 0.0, 1.0),
        raw_endblk(),
    ]);
    let meta = HeaderMeta {
        version: Version::Ac12,
        entity_count: 3, // BLOCK + CIRCLE + ENDBLK — delimiters count too
        entity_end: bytes.len() as u32,
    };
    let entities = read_entities(&bytes, &meta).unwrap();
    assert_eq!(entities.len(), 1);
    assert!(matches!(entities[0], Entity::Circle { .. }));
}

#[test]
fn read_items_groups_block_and_endblk_preserving_interleaved_order() {
    // Milestone ①'s finding, restated for the DWG side: loose entities
    // and block definitions interleave, so this checks the exact
    // Item-level sequence, not just that a block with the right name and
    // contents exists somewhere.
    let bytes = records(&[
        raw_line(0.0, 0.0, 1.0, 1.0),
        raw_block("B1", 1.0, 2.0),
        raw_circle(0.0, 0.0, 1.0),
        raw_endblk(),
        raw_insert("B1", 5.0, 6.0, 1.0, 1.0, 0.0),
    ]);
    let meta = HeaderMeta {
        version: Version::Ac12,
        entity_count: 5,
        entity_end: bytes.len() as u32,
    };
    let items = read_items(&bytes, &meta).unwrap();
    assert_eq!(items.len(), 3);
    assert!(
        matches!(&items[0], Item::Entity(Entity::OnLayer { entity, .. }) if matches!(entity.as_ref(), Entity::Line { .. }))
    );
    let Item::Block(block) = &items[1] else {
        panic!("expected a Block, got {:?}", items[1]);
    };
    assert_eq!(block.name, "B1");
    assert_eq!(block.base, Point { x: 1.0, y: 2.0 });
    assert_eq!(block.entities.len(), 1);
    assert!(
        matches!(&block.entities[0], Entity::OnLayer { entity, .. } if matches!(entity.as_ref(), Entity::Circle { .. }))
    );
    assert!(
        matches!(&items[2], Item::Entity(Entity::OnLayer { entity, .. }) if matches!(entity.as_ref(), Entity::Insert { .. }))
    );
}

#[test]
fn a_lone_block_with_no_endblk_is_unterminated() {
    let bytes = one_block("B1", 1.0, 2.0);
    let meta = HeaderMeta {
        version: Version::Ac12,
        entity_count: 1,
        entity_end: bytes.len() as u32,
    };
    assert_eq!(
        read_items(&bytes, &meta).unwrap_err(),
        DwgError::UnterminatedBlock {
            name: "B1".into(),
            at: ENTITY_START,
        }
    );
}

#[test]
fn an_unterminated_block_at_end_of_stream_is_an_error_naming_its_offset() {
    let bytes = records(&[raw_line(0.0, 0.0, 1.0, 1.0), raw_block("B1", 1.0, 2.0)]);
    // The BLOCK record starts right after the LINE record (36 bytes).
    let block_at = ENTITY_START + 36;
    let meta = HeaderMeta {
        version: Version::Ac12,
        entity_count: 2,
        entity_end: bytes.len() as u32,
    };
    assert_eq!(
        read_items(&bytes, &meta).unwrap_err(),
        DwgError::UnterminatedBlock {
            name: "B1".into(),
            at: block_at,
        }
    );
}

#[test]
fn a_block_defined_inside_another_is_a_sibling_not_a_child() {
    // SELEXOL defines ARROW inside COOLER and then INSERTs ARROW at top
    // level fourteen times, so a nested definition is globally
    // referenceable: the block table is flat. This replaces the old
    // a_block_opened_before_the_previous_one_closes_is_a_nested_block_error
    // test, which asserted the removed DwgError::NestedBlock behaviour.
    let bytes = nested_block_fixture();
    let meta = HeaderMeta {
        version: Version::Ac12,
        entity_count: 7,
        entity_end: bytes.len() as u32,
    };
    let items = read_items(&bytes, &meta).unwrap();

    let blocks: Vec<&str> = items
        .iter()
        .filter_map(|i| match i {
            Item::Block(b) => Some(b.name.as_str()),
            _ => None,
        })
        .collect();
    assert_eq!(blocks, vec!["INNER", "OUTER"], "INNER closes first");

    let outer = items
        .iter()
        .find_map(|i| match i {
            Item::Block(b) if b.name == "OUTER" => Some(b),
            _ => None,
        })
        .unwrap();
    assert_eq!(
        outer.entities.len(),
        2,
        "OUTER holds its own two LINEs, not INNER's"
    );
}

#[test]
fn an_endblk_with_no_open_block_is_still_an_error() {
    // Review Focus 2: a stack makes it easy to over-pop silently.
    let bytes = lone_endblk_fixture();
    let meta = HeaderMeta {
        version: Version::Ac12,
        entity_count: 1,
        entity_end: bytes.len() as u32,
    };
    assert!(matches!(
        read_items(&bytes, &meta),
        Err(DwgError::StrayEndblk { .. })
    ));
}

#[test]
fn a_nested_block_left_open_at_the_end_names_the_innermost() {
    // Review Focus 5. The innermost unterminated block is the
    // informative one: naming the outer would send a reader to the
    // wrong offset.
    let bytes = unterminated_nest_fixture();
    let meta = HeaderMeta {
        version: Version::Ac12,
        entity_count: 3,
        entity_end: bytes.len() as u32,
    };
    let Err(DwgError::UnterminatedBlock { name, .. }) = read_items(&bytes, &meta) else {
        panic!("expected UnterminatedBlock");
    };
    assert_eq!(name, "INNER");
}

#[test]
fn a_stray_endblk_is_an_error_naming_its_offset() {
    let bytes = records(&[raw_line(0.0, 0.0, 1.0, 1.0), raw_endblk()]);
    let endblk_at = ENTITY_START + 36;
    let meta = HeaderMeta {
        version: Version::Ac12,
        entity_count: 2,
        entity_end: bytes.len() as u32,
    };
    assert_eq!(
        read_items(&bytes, &meta).unwrap_err(),
        DwgError::StrayEndblk { at: endblk_at }
    );
}

#[test]
fn a_string_length_running_past_the_end_is_an_error_not_a_panic() {
    // A BLOCK record whose name-length field claims 50 bytes but the
    // buffer holds only a handful. Review Focus 2's rule extends to
    // string reads: this must be TruncatedEntity, never a panic.
    let mut bytes = vec![0u8; ENTITY_START];
    bytes.extend_from_slice(&TYPE_BLOCK.to_le_bytes());
    bytes.extend_from_slice(&0u16.to_le_bytes());
    bytes.extend_from_slice(&50u16.to_le_bytes()); // claims 50 bytes
    bytes.extend_from_slice(b"short"); // only 5 are actually present
    let meta = HeaderMeta {
        version: Version::Ac12,
        entity_count: 1,
        entity_end: bytes.len() as u32,
    };
    assert!(matches!(
        read_entities(&bytes, &meta),
        Err(DwgError::TruncatedEntity { index: 0, .. })
    ));
}

#[test]
fn an_unknown_type_code_names_the_code_and_the_offset() {
    // Review Focus 3: a silently skipped entity renders as a drawing
    // that is quietly wrong, which is worse than one that fails to
    // open.
    let mut bytes = one_line(0.0, 0.0, 1.0, 1.0);
    bytes[ENTITY_START..ENTITY_START + 2].copy_from_slice(&0x00ffu16.to_le_bytes());
    let meta = HeaderMeta {
        version: Version::Ac12,
        entity_count: 1,
        entity_end: bytes.len() as u32,
    };
    assert_eq!(
        read_entities(&bytes, &meta).unwrap_err(),
        DwgError::UnknownEntityType {
            code: 0x00ff,
            at: ENTITY_START
        }
    );
}

#[test]
fn a_record_running_past_the_end_is_an_error_not_a_panic() {
    // Review Focus 2.
    let mut bytes = one_line(0.0, 0.0, 1.0, 1.0);
    bytes.truncate(bytes.len() - 4);
    let meta = HeaderMeta {
        version: Version::Ac12,
        entity_count: 1,
        entity_end: bytes.len() as u32,
    };
    assert!(matches!(
        read_entities(&bytes, &meta),
        Err(DwgError::TruncatedEntity { index: 0, .. })
    ));
}

#[test]
fn fewer_records_than_the_header_promises_is_an_error() {
    let bytes = one_line(0.0, 0.0, 1.0, 1.0);
    let meta = HeaderMeta {
        version: Version::Ac12,
        entity_count: 5,
        entity_end: bytes.len() as u32,
    };
    assert_eq!(
        read_entities(&bytes, &meta).unwrap_err(),
        DwgError::EntityCountMismatch { want: 5, got: 1 }
    );
}

/// The bug the final review found: `entity_end` declared 20 bytes into
/// the entity region, but the buffer holds one genuine 36-byte `LINE`
/// record and `entity_count` says 1. Before this fix, `read_entities`
/// checked only `index != meta.entity_count` after the loop — 1 == 1,
/// so it returned `Ok`, silently accepting that the walk actually
/// consumed 16 bytes (36 - 20) outside the header's declared entity
/// region. This is the module doc's own claim ("the walk lands exactly
/// on entity_end") turned into an enforced check rather than an
/// unverified assertion.
#[test]
fn the_walk_must_land_exactly_on_entity_end_not_just_the_right_record_count() {
    let bytes = one_line(0.0, 0.0, 1.0, 1.0); // ENTITY_START + 36 bytes total
    let declared_end = ENTITY_START + 20;
    let meta = HeaderMeta {
        version: Version::Ac12,
        entity_count: 1,
        entity_end: declared_end as u32,
    };
    assert_eq!(
        read_entities(&bytes, &meta).unwrap_err(),
        DwgError::WalkOverran {
            pos: ENTITY_START + 36,
            entity_end: declared_end,
        }
    );
}

/// The same check, in `read_items` — the walk `parse` actually uses.
#[test]
fn read_items_also_rejects_a_walk_that_overshoots_entity_end() {
    let bytes = one_line(0.0, 0.0, 1.0, 1.0);
    let declared_end = ENTITY_START + 20;
    let meta = HeaderMeta {
        version: Version::Ac12,
        entity_count: 1,
        entity_end: declared_end as u32,
    };
    assert_eq!(
        read_items(&bytes, &meta).unwrap_err(),
        DwgError::WalkOverran {
            pos: ENTITY_START + 36,
            entity_end: declared_end,
        }
    );
}

/// The second hole the same fix closes: an `entity_end` at or before
/// `ENTITY_START` (a bad floppy read zeroing the header's `0x24..0x2a`
/// bytes could produce exactly this) with `entity_count: 0` used to
/// return `Ok` with an empty `Drawing`, because the loop never runs (its
/// own `pos < end` guard is false immediately) and 0 == 0 passed the old
/// entity-count check. `pos` never reaches `end` in this case either, so
/// the same `pos == end` guard catches it.
#[test]
fn an_entity_end_at_or_before_entity_start_is_rejected_not_silently_empty() {
    let bytes = vec![0u8; ENTITY_START];
    let declared_end = ENTITY_START - 10;
    let meta = HeaderMeta {
        version: Version::Ac12,
        entity_count: 0,
        entity_end: declared_end as u32,
    };
    assert_eq!(
        read_items(&bytes, &meta).unwrap_err(),
        DwgError::WalkOverran {
            pos: ENTITY_START,
            entity_end: declared_end,
        }
    );
}

#[test]
fn read_items_wraps_every_entity_as_item_entity() {
    let bytes = one_line(0.0, 0.0, 1.0, 1.0);
    let meta = HeaderMeta {
        version: Version::Ac12,
        entity_count: 1,
        entity_end: bytes.len() as u32,
    };
    let items = read_items(&bytes, &meta).unwrap();
    assert_eq!(items.len(), 1);
    assert!(
        matches!(&items[0], Item::Entity(Entity::OnLayer { entity, .. }) if matches!(entity.as_ref(), Entity::Line { .. }))
    );
}

#[test]
fn record_header_layer_survives_into_drawing_items() {
    let mut bytes = one_line(0.0, 0.0, 1.0, 1.0);
    bytes[ENTITY_START + 2..ENTITY_START + 4].copy_from_slice(&20u16.to_le_bytes());
    let meta = HeaderMeta {
        version: Version::Ac12,
        entity_count: 1,
        entity_end: bytes.len() as u32,
    };
    assert!(
        matches!(read_items(&bytes, &meta).unwrap().as_slice(), [Item::Entity(Entity::OnLayer { layer: 20, entity })] if matches!(entity.as_ref(), Entity::Line { .. }))
    );
}

#[test]
fn layer_word_outside_model_range_is_reported_by_index() {
    let mut bytes = one_line(0.0, 0.0, 1.0, 1.0);
    bytes[ENTITY_START + 2..ENTITY_START + 4].copy_from_slice(&256u16.to_le_bytes());
    let meta = HeaderMeta {
        version: Version::Ac12,
        entity_count: 1,
        entity_end: bytes.len() as u32,
    };
    assert_eq!(
        read_items(&bytes, &meta),
        Err(DwgError::InvalidEntityLayer {
            index: 0,
            value: 256,
        })
    );
}

// --- POINT, TRACE, SOLID (Task 8, Part B) ---------------------------

#[test]
fn reads_a_point_record() {
    // TYPE_POINT is 2. Values are SELEXOL's own first POINT (file offset
    // 0x1f5): a clean (5.0, 5.0) immediately followed by a plausible
    // LINE — see the module doc and the later original DXF export.
    let bytes = records(&[raw_point(5.0, 5.0)]);
    let meta = HeaderMeta {
        version: Version::Ac12,
        entity_count: 1,
        entity_end: bytes.len() as u32,
    };
    let entities = read_entities(&bytes, &meta).unwrap();
    assert_eq!(entities.len(), 1);
    let Entity::Point { origin } = &entities[0] else {
        panic!("expected a POINT, got {:?}", entities[0]);
    };
    assert_eq!(*origin, Point { x: 5.0, y: 5.0 });
}

#[test]
fn reads_a_trace_record_in_file_order() {
    // TYPE_TRACE is 9. Values are SELEXOL's own TRACE (file offset
    // 0x1d39... see corpus_smoke.rs / the module doc): a thin rectangle,
    // stored as four points with no reordering by this codec — the
    // "file order" the module doc promises.
    let corners = [(7.0, 30.725), (7.0, 30.775), (2.5, 30.725), (2.5, 30.775)];
    let bytes = records(&[raw_trace(corners)]);
    let meta = HeaderMeta {
        version: Version::Ac12,
        entity_count: 1,
        entity_end: bytes.len() as u32,
    };
    let entities = read_entities(&bytes, &meta).unwrap();
    assert_eq!(entities.len(), 1);
    let Entity::Trace { p1, p2, p3, p4 } = &entities[0] else {
        panic!("expected a TRACE, got {:?}", entities[0]);
    };
    assert_eq!(*p1, Point { x: 7.0, y: 30.725 });
    assert_eq!(*p2, Point { x: 7.0, y: 30.775 });
    assert_eq!(*p3, Point { x: 2.5, y: 30.725 });
    assert_eq!(*p4, Point { x: 2.5, y: 30.775 });
}

#[test]
fn reads_a_solid_record_in_file_order() {
    // TYPE_SOLID is 11 — same field shape as TRACE (module doc);
    // exercised against FLOW in the corpus.
    let corners = [(0.0, 0.0), (1.0, 0.0), (0.0, 1.0), (1.0, 1.0)];
    let bytes = records(&[raw_solid(corners)]);
    let meta = HeaderMeta {
        version: Version::Ac12,
        entity_count: 1,
        entity_end: bytes.len() as u32,
    };
    let entities = read_entities(&bytes, &meta).unwrap();
    assert_eq!(entities.len(), 1);
    let Entity::Solid { p1, p2, p3, p4 } = &entities[0] else {
        panic!("expected a SOLID, got {:?}", entities[0]);
    };
    assert_eq!(*p1, Point { x: 0.0, y: 0.0 });
    assert_eq!(*p2, Point { x: 1.0, y: 0.0 });
    assert_eq!(*p3, Point { x: 0.0, y: 1.0 });
    assert_eq!(*p4, Point { x: 1.0, y: 1.0 });
}

// --- REPEAT / ENDREP (Task 8, Part B) --------------------------------

#[test]
fn a_line_following_repeat_marker_decodes_as_a_real_entity() {
    // BLIVET's simple case: REPEAT immediately followed by ENDREP, no
    // entities nested between them. REPEAT's own u16 pair and 4 doubles
    // are the nested LINE's fields (module doc) — values are BLIVET's
    // own first REPEAT (file offset 0x3b4).
    let bytes = records(&[
        raw_repeat_line(1, 0.01999999, 0.25, 2.23, 0.25),
        raw_endrep(1, 5, 0.0, 0.25),
    ]);
    // REPEAT and its following ordinary leaf each count once, ENDREP
    // as 1 — matching the header's own entity_count (module doc).
    let meta = HeaderMeta {
        version: Version::Ac12,
        entity_count: 3,
        entity_end: bytes.len() as u32,
    };
    let entities = read_entities(&bytes, &meta).unwrap();
    assert_eq!(entities.len(), 1);
    let Entity::Line { start, end } = &entities[0] else {
        panic!("expected a LINE, got {:?}", entities[0]);
    };
    assert_eq!(
        *start,
        Point {
            x: 0.01999999,
            y: 0.25
        }
    );
    assert_eq!(*end, Point { x: 2.23, y: 0.25 });
}

#[test]
fn an_insert_following_repeat_marker_decodes_as_a_real_entity() {
    // BLIVET's other observed nested type: INSERT of block "$BCIRC"
    // (file offset 0x14a7).
    let bytes = records(&[
        raw_repeat_insert("$BCIRC", 2.0, 2.5, 0.5, 1.0, 0.0),
        raw_endrep(1, 3, 2.0, 1.5),
    ]);
    let meta = HeaderMeta {
        version: Version::Ac12,
        entity_count: 3,
        entity_end: bytes.len() as u32,
    };
    let entities = read_entities(&bytes, &meta).unwrap();
    assert_eq!(entities.len(), 1);
    let Entity::Insert {
        origin,
        x_scale,
        y_scale,
        rotation_deg,
        name,
    } = &entities[0]
    else {
        panic!("expected an INSERT, got {:?}", entities[0]);
    };
    assert_eq!(*origin, Point { x: 2.0, y: 2.5 });
    assert_eq!(*x_scale, 0.5);
    assert_eq!(*y_scale, 1.0);
    assert_eq!(*rotation_deg, 0.0);
    assert_eq!(name, "$BCIRC");
}

#[test]
fn repeat_can_wrap_ordinary_entities_between_it_and_endrep() {
    // FLOOR's case: the first LINE after the REPEAT marker is one edge of a small
    // rectangle, followed by three plain LINE records (decoded by the
    // ordinary top-level dispatch, nothing special about being "inside"
    // a REPEAT), then ENDREP. Values are FLOOR's own first REPEAT (file
    // offset 0x16c9).
    let bytes = records(&[
        raw_repeat_line(1, 7.12, 8.4, 7.32, 8.4),
        raw_line(7.32, 8.4, 7.32, 8.66),
        raw_line(7.32, 8.66, 7.12, 8.66),
        raw_line(7.12, 8.66, 7.12, 8.4),
        raw_endrep(3, 1, 0.672, 0.0),
    ]);
    // REPEAT marker: 1, four LINEs: 1 each, ENDREP: 1 -> 6.
    let meta = HeaderMeta {
        version: Version::Ac12,
        entity_count: 6,
        entity_end: bytes.len() as u32,
    };
    let entities = read_entities(&bytes, &meta).unwrap();
    assert_eq!(entities.len(), 4);
    assert!(entities.iter().all(|e| matches!(e, Entity::Line { .. })));
    // The rectangle closes: the first edge starts at (7.12, 8.4), the
    // last plain LINE ends there too.
    let Entity::Line { start, .. } = &entities[0] else {
        unreachable!()
    };
    let Entity::Line { end, .. } = &entities[3] else {
        unreachable!()
    };
    assert_eq!(start, end);
}

#[test]
fn an_unsupported_child_type_inside_repeat_is_an_unknown_entity_type_error() {
    // This fixture puts an unsupported ordinary type after a
    // REPEAT (module doc) — anything else must fail loudly, not guess a
    // layout with no evidence behind it.
    let bytes = records(&[raw_repeat_unsupported(99)]);
    let meta = HeaderMeta {
        version: Version::Ac12,
        entity_count: 2,
        entity_end: bytes.len() as u32,
    };
    assert_eq!(
        read_entities(&bytes, &meta).unwrap_err(),
        DwgError::UnknownEntityType {
            code: 99,
            at: ENTITY_START + 4,
        }
    );
}

#[test]
fn endrep_is_a_fixed_24_bytes_regardless_of_its_own_fields() {
    // A REPEAT/ENDREP pair with no nested entities, followed by a plain
    // LINE: if ENDREP consumed anything other than exactly 24 bytes,
    // this LINE would desync and either fail or decode to nonsense.
    let bytes = records(&[
        raw_repeat_line(1, 1.0, 1.0, 2.0, 2.0),
        raw_endrep(7, 42, 9.5, -3.25),
        raw_line(10.0, 10.0, 20.0, 20.0),
    ]);
    let meta = HeaderMeta {
        version: Version::Ac12,
        entity_count: 4, // REPEAT(2) + ENDREP(1) + LINE(1)
        entity_end: bytes.len() as u32,
    };
    let entities = read_entities(&bytes, &meta).unwrap();
    assert_eq!(entities.len(), 2);
    let Entity::Line { start, .. } = &entities[1] else {
        panic!("expected a LINE, got {:?}", entities[1]);
    };
    assert_eq!(*start, Point { x: 10.0, y: 10.0 });
}

// --- Erased entities (Task 8, Part A) --------------------------------

#[test]
fn an_erased_line_is_skipped_but_still_counted() {
    // ADDER.DWG holds 7 erased records, including -1 (an erased LINE).
    // Its body decodes exactly like a live LINE but produces no Entity;
    // entity_count still counts it (spec §4.2, module doc).
    let bytes = records(&[
        erase(TYPE_LINE, raw_line(0.0, 0.0, 1.0, 1.0)),
        raw_line(5.0, 5.0, 6.0, 6.0),
    ]);
    let meta = HeaderMeta {
        version: Version::Ac12,
        entity_count: 2,
        entity_end: bytes.len() as u32,
    };
    let entities = read_entities(&bytes, &meta).unwrap();
    assert_eq!(entities.len(), 1);
    let Entity::Line { start, .. } = &entities[0] else {
        panic!("expected a LINE, got {:?}", entities[0]);
    };
    assert_eq!(*start, Point { x: 5.0, y: 5.0 });
}

#[test]
fn an_erased_insert_is_skipped_and_its_variable_length_is_still_honoured() {
    // ADDER also holds -14, an erased INSERT — a variable-length record
    // (a length-prefixed name), so this checks the erased path still
    // reads the real size for *this* record's type, not some fixed
    // guess, before resuming the walk.
    let bytes = records(&[
        erase(TYPE_INSERT, raw_insert("HOUSEA", 1.0, 2.0, 1.0, 1.0, 0.0)),
        raw_line(9.0, 9.0, 10.0, 10.0),
    ]);
    let meta = HeaderMeta {
        version: Version::Ac12,
        entity_count: 2,
        entity_end: bytes.len() as u32,
    };
    let entities = read_entities(&bytes, &meta).unwrap();
    assert_eq!(entities.len(), 1);
    assert!(matches!(entities[0], Entity::Line { .. }));
}

#[test]
fn read_items_retains_erased_records_in_place() {
    let bytes = records(&[
        erase(TYPE_LINE, raw_line(0.0, 0.0, 1.0, 1.0)),
        raw_line(5.0, 5.0, 6.0, 6.0),
    ]);
    let meta = HeaderMeta {
        version: Version::Ac12,
        entity_count: 2,
        entity_end: bytes.len() as u32,
    };
    let items = read_items(&bytes, &meta).unwrap();
    assert_eq!(items.len(), 2);
    assert!(
        matches!(&items[0], Item::Erased(Entity::OnLayer { entity, .. }) if matches!(entity.as_ref(), Entity::Line { .. }))
    );
    assert!(
        matches!(&items[1], Item::Entity(Entity::OnLayer { entity, .. }) if matches!(entity.as_ref(), Entity::Line { .. }))
    );
}

#[test]
fn a_truncated_erased_record_is_still_an_error_not_a_panic() {
    // Review Focus 2 extends to erased records: the body is still fully
    // read (that's how OOPS could restore it), so a truncated one must
    // still be caught, not silently swallowed because it's "just" erased.
    let mut bytes = records(&[erase(TYPE_LINE, raw_line(0.0, 0.0, 1.0, 1.0))]);
    bytes.truncate(bytes.len() - 4);
    let meta = HeaderMeta {
        version: Version::Ac12,
        entity_count: 1,
        entity_end: bytes.len() as u32,
    };
    assert!(matches!(
        read_entities(&bytes, &meta),
        Err(DwgError::TruncatedEntity { index: 0, .. })
    ));
}
