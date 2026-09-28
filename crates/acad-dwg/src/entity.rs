//! The entity record walk (spec §4.2, §4.4).
//!
//! Entity records sit after the header and layer table, from a fixed
//! `ENTITY_START` to `meta.entity_end` (both discovered against `SUBDIV`,
//! Task 2). Each record is a `u16` type code, a `u16` of not-yet-understood
//! meaning, then the type's own fields as little-endian `f64`s. The type
//! code is a 1-based index into the entity name table recovered from
//! `ACAD.EXE`: 1 `LINE`, 2 `POINT`, 3 `CIRCLE`, 4 `SHAPE`, 5 `REPEAT`,
//! 6 `ENDREP`, 7 `TEXT`, 8 `ARC`, 9 `TRACE`, 10 `LOAD`, 11 `SOLID`,
//! 12 `BLOCK`, 13 `ENDBLK`, 14 `INSERT`. `LINE`, `CIRCLE` and `ARC` are
//! implemented here; every other code is `DwgError::UnknownEntityType` until
//! later tasks add it. `ARC`'s two angle fields are stored in radians; they
//! are converted to degrees here because `acad_model::Entity::Arc` documents
//! `start_deg`/`end_deg` as degrees, and the angles are not normalised —
//! `SUBDIV`'s own first `ARC` sweeps counter-clockwise through 0°.

use crate::header::HeaderMeta;
use crate::DwgError;
use acad_model::{Entity, Item, Point};

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

/// `ACAD.EXE`'s entity name table, entry 8: `ARC`.
const TYPE_ARC: u16 = 8;

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

/// Reads every entity record from `ENTITY_START` to `meta.entity_end`, in
/// file order — which **is** DXF order; nothing here reverses it (Task 2
/// found the opposite, an earlier plan draft's assumption, to be wrong).
///
/// Every offset read goes through `checked_u16`/`checked_f64`, so a
/// truncated file is `Err(DwgError::TruncatedEntity)`, never a panic.
pub fn read_entities(bytes: &[u8], meta: &HeaderMeta) -> Result<Vec<Entity>, DwgError> {
    let end = meta.entity_end as usize;
    let mut pos = ENTITY_START;
    let mut index: u32 = 0;
    let mut out = Vec::new();

    while pos < end {
        let header = read_record_header(bytes, pos, index)?;
        match header.type_code {
            TYPE_LINE => {
                let x1 = checked_f64(bytes, pos + 4, index)?;
                let y1 = checked_f64(bytes, pos + 12, index)?;
                let x2 = checked_f64(bytes, pos + 20, index)?;
                let y2 = checked_f64(bytes, pos + 28, index)?;
                out.push(Entity::Line {
                    start: Point { x: x1, y: y1 },
                    end: Point { x: x2, y: y2 },
                });
                pos += 36;
            }
            TYPE_CIRCLE => {
                let cx = checked_f64(bytes, pos + 4, index)?;
                let cy = checked_f64(bytes, pos + 12, index)?;
                let radius = checked_f64(bytes, pos + 20, index)?;
                out.push(Entity::Circle {
                    center: Point { x: cx, y: cy },
                    radius,
                });
                pos += 28;
            }
            TYPE_ARC => {
                let cx = checked_f64(bytes, pos + 4, index)?;
                let cy = checked_f64(bytes, pos + 12, index)?;
                let radius = checked_f64(bytes, pos + 20, index)?;
                let start_rad = checked_f64(bytes, pos + 28, index)?;
                let end_rad = checked_f64(bytes, pos + 36, index)?;
                out.push(Entity::Arc {
                    center: Point { x: cx, y: cy },
                    radius,
                    // The DWG stores these in radians; acad_model documents
                    // Arc's angles as degrees (as AutoCAD 1.4 stored them),
                    // and the DXF oracle prints degrees too. Not normalised:
                    // SUBDIV's own first ARC sweeps counter-clockwise
                    // through 0°, start (78.604°) > end (22.451°).
                    start_deg: start_rad.to_degrees(),
                    end_deg: end_rad.to_degrees(),
                });
                pos += 44;
            }
            code => return Err(DwgError::UnknownEntityType { code, at: pos }),
        }
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

/// The same walk as [`read_entities`], but keeping `BLOCK`/`ENDBLK` grouping
/// as `Item::Block` once Task 5 understands those records. Until then, every
/// entity is wrapped as `Item::Entity` — this exists now so Task 6's `parse`
/// has a stable name to call.
pub fn read_items(bytes: &[u8], meta: &HeaderMeta) -> Result<Vec<Item>, DwgError> {
    Ok(read_entities(bytes, meta)?
        .into_iter()
        .map(Item::Entity)
        .collect())
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
