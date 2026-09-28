//! The entity record walk (spec §4.2, §4.4).
//!
//! Entity records sit after the header and layer table, from a fixed
//! `ENTITY_START` to `meta.entity_end` (both discovered against `SUBDIV`,
//! Task 2). Each record is a `u16` type code, a `u16` of not-yet-understood
//! meaning, then the type's own fields. The type code is a 1-based index
//! into the entity name table recovered from `ACAD.EXE`: 1 `LINE`, 2 `POINT`,
//! 3 `CIRCLE`, 4 `SHAPE`, 5 `REPEAT`, 6 `ENDREP`, 7 `TEXT`, 8 `ARC`,
//! 9 `TRACE`, 10 `LOAD`, 11 `SOLID`, 12 `BLOCK`, 13 `ENDBLK`, 14 `INSERT`.
//! `LINE`, `CIRCLE`, `ARC`, `TEXT`, `BLOCK`, `ENDBLK` and `INSERT` are
//! implemented here; every other code is `DwgError::UnknownEntityType` until
//! Task 8 adds it (`POINT`, `SOLID`, `TRACE`, `SHAPE` — none with a model
//! representation yet).
//!
//! `ARC`, `TEXT` and `INSERT` each carry an angle field stored in radians;
//! they are converted to degrees here because `acad_model` documents them in
//! degrees, and the angles are not normalised — `SUBDIV`'s own first `ARC`
//! sweeps counter-clockwise through 0°.
//!
//! `TEXT`'s height field is *also* not a direct copy: the raw double is 4/3
//! the DXF's printed height (see the `TYPE_TEXT` arm of `read_record_body`
//! for how this was found), so it is scaled by 0.75 on the way in. This is
//! local to the entity field — the header's own `TXTSIZE` needs no such
//! scaling — and was not expected going in; only the angle fields were known
//! to need a unit conversion.
//!
//! `BLOCK`, `TEXT` and `INSERT` also carry a length-prefixed Latin-1 string
//! (a `u16` byte count, then exactly that many bytes — no NUL terminator):
//! `BLOCK`'s and `INSERT`'s name sits right after the 4-byte header, before
//! their numeric fields; `TEXT`'s value sits *after* its four doubles. Both
//! placements were established the same way every record layout in this
//! crate was: reading real `SUBDIV.DWG` bytes at an offset `find_f64`
//! located from a `SUBDIV.DXF` value, by hand, with `xxd`/`python3` (see
//! `task-5-report.md`). `ENDBLK` has no fields at all — a bare 4-byte
//! header.
//!
//! `read_entities` decodes every entity-shaped record (including those
//! inside a `BLOCK`/`ENDBLK` pair) into one flat, file-order `Vec<Entity>`,
//! skipping `BLOCK`/`ENDBLK` themselves without erroring on them — it has no
//! way to represent them, since `acad_model::Entity` has no block variant.
//! `read_items` walks the same records through the same per-record decoder
//! but tracks `BLOCK`/`ENDBLK` nesting to group each pair's entities into one
//! `Item::Block`, preserving interleaving with loose entities exactly as
//! `acad_model::drawing`'s doc comment requires.

use crate::header::HeaderMeta;
use crate::text::decode_latin1;
use crate::DwgError;
use acad_model::{Block, Entity, Item, Point};

/// Entity records begin at a fixed offset in every `AC1.2` file. The
/// header's scalars end at `0xC4` and the layer table fills `0xC4..0x1D8`
/// (Task 7 reverses the table itself, but its length is already settled),
/// so this is a named constant rather than something derived from the
/// header.
const ENTITY_START: usize = 0x1D8;

/// 1-based index into `ACAD.EXE`'s entity name table; `LINE` is its first
/// entry.
const TYPE_LINE: u16 = 1;

/// `ACAD.EXE`'s entity name table, entry 3: `CIRCLE`.
const TYPE_CIRCLE: u16 = 3;

/// `ACAD.EXE`'s entity name table, entry 7: `TEXT`.
const TYPE_TEXT: u16 = 7;

/// `ACAD.EXE`'s entity name table, entry 8: `ARC`.
const TYPE_ARC: u16 = 8;

/// `ACAD.EXE`'s entity name table, entry 12: `BLOCK`.
const TYPE_BLOCK: u16 = 12;

/// `ACAD.EXE`'s entity name table, entry 13: `ENDBLK`.
const TYPE_ENDBLK: u16 = 13;

/// `ACAD.EXE`'s entity name table, entry 14: `INSERT`.
const TYPE_INSERT: u16 = 14;

/// The 4-byte prefix every entity record starts with, before its own
/// type-specific fields.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RecordHeader {
    pub type_code: u16,
    /// Present in every record; what it encodes is not yet known.
    pub flags: u16,
}

fn checked_u16(bytes: &[u8], at: usize, index: u32) -> Result<u16, DwgError> {
    let slice = bytes.get(at..at + 2).ok_or(DwgError::TruncatedEntity {
        index,
        at,
        len: bytes.len(),
    })?;
    Ok(u16::from_le_bytes(slice.try_into().unwrap()))
}

fn checked_f64(bytes: &[u8], at: usize, index: u32) -> Result<f64, DwgError> {
    let slice = bytes.get(at..at + 8).ok_or(DwgError::TruncatedEntity {
        index,
        at,
        len: bytes.len(),
    })?;
    Ok(f64::from_le_bytes(slice.try_into().unwrap()))
}

fn read_record_header(bytes: &[u8], at: usize, index: u32) -> Result<RecordHeader, DwgError> {
    Ok(RecordHeader {
        type_code: checked_u16(bytes, at, index)?,
        flags: checked_u16(bytes, at + 2, index)?,
    })
}

/// Reads a length-prefixed Latin-1 string: a `u16` byte count at `at`, then
/// that many bytes starting at `at + 2`. Returns the decoded string and the
/// offset just past it. A count that runs past the end of the file is
/// `DwgError::TruncatedEntity`, never a panic — this is the one place a
/// malformed length field could otherwise index out of bounds.
fn checked_string(bytes: &[u8], at: usize, index: u32) -> Result<(String, usize), DwgError> {
    let len = checked_u16(bytes, at, index)? as usize;
    let start = at + 2;
    let end = start + len;
    let slice = bytes.get(start..end).ok_or(DwgError::TruncatedEntity {
        index,
        at: start,
        len: bytes.len(),
    })?;
    Ok((decode_latin1(slice), end))
}

/// One physical record's payload, once its header and fields are read: a
/// decoded entity, or one of the `BLOCK`/`ENDBLK` delimiters `read_items`
/// groups by. Not `pub` — `read_entities` and `read_items` are the crate's
/// two supported ways to walk the entity stream; nothing outside this module
/// needs a record-level view.
enum RecordBody {
    Entity(Entity),
    BlockStart { name: String, base: Point },
    BlockEnd,
}

/// Decodes the one record starting at `pos`, returning its payload and the
/// offset of the next record. Shared by `read_entities` (which keeps only
/// the `Entity` payloads, flattening past `BLOCK`/`ENDBLK`) and `read_items`
/// (which uses the delimiters to group).
fn read_record_body(bytes: &[u8], pos: usize, index: u32) -> Result<(RecordBody, usize), DwgError> {
    let header = read_record_header(bytes, pos, index)?;
    match header.type_code {
        TYPE_LINE => {
            let x1 = checked_f64(bytes, pos + 4, index)?;
            let y1 = checked_f64(bytes, pos + 12, index)?;
            let x2 = checked_f64(bytes, pos + 20, index)?;
            let y2 = checked_f64(bytes, pos + 28, index)?;
            Ok((
                RecordBody::Entity(Entity::Line {
                    start: Point { x: x1, y: y1 },
                    end: Point { x: x2, y: y2 },
                }),
                pos + 36,
            ))
        }
        TYPE_CIRCLE => {
            let cx = checked_f64(bytes, pos + 4, index)?;
            let cy = checked_f64(bytes, pos + 12, index)?;
            let radius = checked_f64(bytes, pos + 20, index)?;
            Ok((
                RecordBody::Entity(Entity::Circle {
                    center: Point { x: cx, y: cy },
                    radius,
                }),
                pos + 28,
            ))
        }
        TYPE_ARC => {
            let cx = checked_f64(bytes, pos + 4, index)?;
            let cy = checked_f64(bytes, pos + 12, index)?;
            let radius = checked_f64(bytes, pos + 20, index)?;
            let start_rad = checked_f64(bytes, pos + 28, index)?;
            let end_rad = checked_f64(bytes, pos + 36, index)?;
            Ok((
                RecordBody::Entity(Entity::Arc {
                    center: Point { x: cx, y: cy },
                    radius,
                    // The DWG stores these in radians; acad_model documents
                    // Arc's angles as degrees (as AutoCAD 1.4 stored them),
                    // and the DXF oracle prints degrees too. Not normalised:
                    // SUBDIV's own first ARC sweeps counter-clockwise
                    // through 0°, start (78.604°) > end (22.451°).
                    start_deg: start_rad.to_degrees(),
                    end_deg: end_rad.to_degrees(),
                }),
                pos + 44,
            ))
        }
        TYPE_TEXT => {
            // Layout established against SUBDIV's own TEXT records (e.g. its
            // "A" label at file offset 0x1718): header, then origin.x,
            // origin.y, height, rotation (radians) as four consecutive
            // doubles, then the length-prefixed value *after* the numbers —
            // unlike BLOCK/INSERT, whose name comes first.
            //
            // The stored height field is not the DXF's height directly: all
            // 8 of SUBDIV's TEXT records decode to 0.4613464999999999 there,
            // while SUBDIV.DXF prints 0.346010 for every one of them, and
            // that exact value never appears anywhere in the file (searched
            // at both the DXF's tolerance and much looser). The ratio is
            // 0.75 to seven significant figures (0.4613465 * 0.75 =
            // 0.3460098749999999, which rounds to the DXF's printed
            // 0.346010) — most likely the classic vector-font convention of
            // a 3/4 cap-height-to-design-height ratio. This is specific to
            // the entity field: the header's own TXTSIZE (spec §4.2, offset
            // 0xb4) holds SUBDIV's 0.2 raw, unconverted, so this is not a
            // global text-height unit and must not be applied there.
            let x = checked_f64(bytes, pos + 4, index)?;
            let y = checked_f64(bytes, pos + 12, index)?;
            let height_raw = checked_f64(bytes, pos + 20, index)?;
            let rotation_rad = checked_f64(bytes, pos + 28, index)?;
            let (value, next) = checked_string(bytes, pos + 36, index)?;
            Ok((
                RecordBody::Entity(Entity::Text {
                    origin: Point { x, y },
                    height: height_raw * 0.75,
                    rotation_deg: rotation_rad.to_degrees(),
                    value,
                }),
                next,
            ))
        }
        TYPE_BLOCK => {
            // Layout established against SUBDIV's first BLOCK (HOUSEA, file
            // offset 0xaac): header, then a length-prefixed name, then
            // base.x, base.y as two consecutive doubles right after it.
            let (name, after_name) = checked_string(bytes, pos + 4, index)?;
            let base_x = checked_f64(bytes, after_name, index)?;
            let base_y = checked_f64(bytes, after_name + 8, index)?;
            Ok((
                RecordBody::BlockStart {
                    name,
                    base: Point {
                        x: base_x,
                        y: base_y,
                    },
                },
                after_name + 16,
            ))
        }
        TYPE_ENDBLK => Ok((RecordBody::BlockEnd, pos + 4)),
        TYPE_INSERT => {
            // Layout established against SUBDIV's first INSERT (also
            // HOUSEA, file offset 0x1648): header, then a length-prefixed
            // name — same placement as BLOCK's — then origin.x, origin.y,
            // x_scale, y_scale, rotation (radians) as five consecutive
            // doubles.
            let (name, after_name) = checked_string(bytes, pos + 4, index)?;
            let x = checked_f64(bytes, after_name, index)?;
            let y = checked_f64(bytes, after_name + 8, index)?;
            let x_scale = checked_f64(bytes, after_name + 16, index)?;
            let y_scale = checked_f64(bytes, after_name + 24, index)?;
            let rotation_rad = checked_f64(bytes, after_name + 32, index)?;
            Ok((
                RecordBody::Entity(Entity::Insert {
                    origin: Point { x, y },
                    x_scale,
                    y_scale,
                    rotation_deg: rotation_rad.to_degrees(),
                    name,
                }),
                after_name + 40,
            ))
        }
        code => Err(DwgError::UnknownEntityType { code, at: pos }),
    }
}

/// Reads every entity-shaped record from `ENTITY_START` to `meta.entity_end`,
/// in file order — which **is** DXF order; nothing here reverses it (Task 2
/// found the opposite, an earlier plan draft's assumption, to be wrong).
/// `BLOCK`/`ENDBLK` delimiters are walked over (their byte spans are skipped
/// correctly) but produce no element: `acad_model::Entity` has no block
/// variant, so an entity inside a block body appears in this flat list
/// exactly like a top-level one, indistinguishable from it. Callers that
/// need the grouping — everyone building an `acad_model::Drawing` — want
/// [`read_items`] instead.
///
/// Every offset read, including a length-prefixed string's, goes through the
/// checked helpers, so a truncated or malformed file is
/// `Err(DwgError::TruncatedEntity)`, never a panic.
pub fn read_entities(bytes: &[u8], meta: &HeaderMeta) -> Result<Vec<Entity>, DwgError> {
    let end = meta.entity_end as usize;
    let mut pos = ENTITY_START;
    let mut index: u32 = 0;
    let mut out = Vec::new();

    while pos < end {
        let (body, next) = read_record_body(bytes, pos, index)?;
        if let RecordBody::Entity(e) = body {
            out.push(e);
        }
        pos = next;
        index += 1;
    }

    if index != meta.entity_count {
        return Err(DwgError::EntityCountMismatch {
            want: meta.entity_count,
            got: index,
        });
    }
    Ok(out)
}

/// The same walk as [`read_entities`], over the same per-record decoder, but
/// tracking `BLOCK`/`ENDBLK` nesting to group each pair's entities into one
/// `Item::Block` rather than dropping the boundary. Loose entities and block
/// definitions interleave in document order (`acad_model::drawing`'s doc
/// comment), which this preserves: nothing is bucketed by kind.
///
/// A `BLOCK` with no matching `ENDBLK` before `meta.entity_end` — either
/// because the entity region ends first, or because another `BLOCK` opens
/// before the first one closes — is `DwgError::UnterminatedBlock`, naming
/// the offset the unterminated `BLOCK` record itself started at, not a
/// silent truncation of its entities. A stray `ENDBLK` with no `BLOCK` open
/// is `DwgError::StrayEndblk`, naming its own offset.
pub fn read_items(bytes: &[u8], meta: &HeaderMeta) -> Result<Vec<Item>, DwgError> {
    let end = meta.entity_end as usize;
    let mut pos = ENTITY_START;
    let mut index: u32 = 0;
    let mut out = Vec::new();
    let mut open: Option<(usize, Block)> = None;

    while pos < end {
        let (body, next) = read_record_body(bytes, pos, index)?;
        match body {
            RecordBody::Entity(e) => match &mut open {
                Some((_, block)) => block.entities.push(e),
                None => out.push(Item::Entity(e)),
            },
            RecordBody::BlockStart { name, base } => {
                if let Some((at, block)) = &open {
                    return Err(DwgError::UnterminatedBlock {
                        name: block.name.clone(),
                        at: *at,
                    });
                }
                open = Some((
                    pos,
                    Block {
                        name,
                        base,
                        entities: Vec::new(),
                    },
                ));
            }
            RecordBody::BlockEnd => match open.take() {
                Some((_, block)) => out.push(Item::Block(block)),
                None => return Err(DwgError::StrayEndblk { at: pos }),
            },
        }
        pos = next;
        index += 1;
    }

    if let Some((at, block)) = open {
        return Err(DwgError::UnterminatedBlock {
            name: block.name,
            at,
        });
    }
    if index != meta.entity_count {
        return Err(DwgError::EntityCountMismatch {
            want: meta.entity_count,
            got: index,
        });
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use acad_model::Entity;

    /// One `LINE` record, laid out as Task 2's discovery run showed: the
    /// type code and the flags word, then x1, y1, x2, y2 as little-endian
    /// doubles. `read_entities` always starts its walk at the fixed
    /// `ENTITY_START` (Task 2 verified this holds across six real files), so
    /// this fixture pads up to that offset first — the brief's own draft of
    /// this test predates `ENTITY_START` being pinned to a nonzero value and
    /// wrote the record at offset 0, which no longer matches production
    /// behaviour; see `task-3-report.md` for the note.
    fn one_line(x1: f64, y1: f64, x2: f64, y2: f64) -> Vec<u8> {
        let mut r = vec![0u8; ENTITY_START];
        r.extend_from_slice(&TYPE_LINE.to_le_bytes());
        r.extend_from_slice(&0u16.to_le_bytes()); // the flags word
        for v in [x1, y1, x2, y2] {
            r.extend_from_slice(&v.to_le_bytes());
        }
        r
    }

    /// One `CIRCLE` record: the type code and flags word, then centre.x,
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

    /// One `ARC` record: the type code and flags word, then centre.x,
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
    fn reads_a_circle_record() {
        // TYPE_CIRCLE is 3: ACAD.EXE's entity name table, third entry.
        // Values are SUBDIV's TREE-block CIRCLE (spec's discovery run).
        let bytes = one_circle(16.464680, 12.562830, 0.420620);
        let meta = HeaderMeta {
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
            entity_count: 5,
            entity_end: bytes.len() as u32,
        };
        let items = read_items(&bytes, &meta).unwrap();
        assert_eq!(items.len(), 3);
        assert!(matches!(items[0], Item::Entity(Entity::Line { .. })));
        let Item::Block(block) = &items[1] else {
            panic!("expected a Block, got {:?}", items[1]);
        };
        assert_eq!(block.name, "B1");
        assert_eq!(block.base, Point { x: 1.0, y: 2.0 });
        assert_eq!(block.entities.len(), 1);
        assert!(matches!(block.entities[0], Entity::Circle { .. }));
        assert!(matches!(items[2], Item::Entity(Entity::Insert { .. })));
    }

    #[test]
    fn a_lone_block_with_no_endblk_is_unterminated() {
        let bytes = one_block("B1", 1.0, 2.0);
        let meta = HeaderMeta {
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
    fn a_block_opened_before_the_previous_one_closes_is_an_error() {
        let bytes = records(&[raw_block("B1", 0.0, 0.0), raw_block("B2", 1.0, 1.0)]);
        let meta = HeaderMeta {
            entity_count: 2,
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
    fn a_stray_endblk_is_an_error_naming_its_offset() {
        let bytes = records(&[raw_line(0.0, 0.0, 1.0, 1.0), raw_endblk()]);
        let endblk_at = ENTITY_START + 36;
        let meta = HeaderMeta {
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
            entity_count: 5,
            entity_end: bytes.len() as u32,
        };
        assert_eq!(
            read_entities(&bytes, &meta).unwrap_err(),
            DwgError::EntityCountMismatch { want: 5, got: 1 }
        );
    }

    #[test]
    fn read_items_wraps_every_entity_as_item_entity() {
        let bytes = one_line(0.0, 0.0, 1.0, 1.0);
        let meta = HeaderMeta {
            entity_count: 1,
            entity_end: bytes.len() as u32,
        };
        let items = read_items(&bytes, &meta).unwrap();
        assert_eq!(items.len(), 1);
        assert!(matches!(items[0], Item::Entity(Entity::Line { .. })));
    }
}
