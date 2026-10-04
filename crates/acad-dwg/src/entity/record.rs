use crate::header::Version;
use crate::text::decode_latin1;
use crate::DwgError;
use acad_model::{Entity, Point};

pub(super) const TYPE_LOAD: u16 = 10;
pub(super) const TYPE_SHAPE: u16 = 4;

/// 1-based index into `ACAD.EXE`'s entity name table; `LINE` is its first
/// entry.
pub(super) const TYPE_LINE: u16 = 1;

/// `ACAD.EXE`'s entity name table, entry 2: `POINT`.
pub(super) const TYPE_POINT: u16 = 2;

/// `ACAD.EXE`'s entity name table, entry 3: `CIRCLE`.
pub(super) const TYPE_CIRCLE: u16 = 3;

/// `ACAD.EXE`'s entity name table, entry 5: `REPEAT`. Not a leaf entity —
/// see the module doc's `REPEAT`/`ENDREP` section.
pub(super) const TYPE_REPEAT: u16 = 5;

/// `ACAD.EXE`'s entity name table, entry 6: `ENDREP`.
pub(super) const TYPE_ENDREP: u16 = 6;

/// `ACAD.EXE`'s entity name table, entry 7: `TEXT`.
pub(super) const TYPE_TEXT: u16 = 7;

/// `ACAD.EXE`'s entity name table, entry 8: `ARC`.
pub(super) const TYPE_ARC: u16 = 8;

/// `ACAD.EXE`'s entity name table, entry 9: `TRACE`.
pub(super) const TYPE_TRACE: u16 = 9;

/// `ACAD.EXE`'s entity name table, entry 11: `SOLID`.
pub(super) const TYPE_SOLID: u16 = 11;

/// `ACAD.EXE`'s entity name table, entry 12: `BLOCK`.
pub(super) const TYPE_BLOCK: u16 = 12;

/// `ACAD.EXE`'s entity name table, entry 13: `ENDBLK`.
pub(super) const TYPE_ENDBLK: u16 = 13;

/// `ACAD.EXE`'s entity name table, entry 14: `INSERT`.
pub(super) const TYPE_INSERT: u16 = 14;

/// The 4-byte prefix every entity record starts with, before its own
/// type-specific fields.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RecordHeader {
    pub type_code: u16,
    /// Layer index stored after the type code.
    pub layer: u16,
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
    let value = f64::from_le_bytes(slice.try_into().unwrap());
    if !value.is_finite() {
        return Err(DwgError::NonFiniteEntity { index, at });
    }
    Ok(value)
}

fn read_record_header(bytes: &[u8], at: usize, index: u32) -> Result<RecordHeader, DwgError> {
    Ok(RecordHeader {
        type_code: checked_u16(bytes, at, index)?,
        layer: checked_u16(bytes, at + 2, index)?,
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
/// decoded entity, one of the `BLOCK`/`ENDBLK` delimiters `read_items` groups
/// by, or an independent structural delimiter. Not
/// `pub` — `read_entities` and `read_items` are the crate's two supported
/// ways to walk the entity stream; nothing outside this module needs a
/// record-level view.
pub(super) enum RecordBody {
    Entity(Entity),
    ErasedEntity(Entity),
    RepeatStart {
        erased: bool,
        layer: u8,
    },
    RepeatEnd {
        erased: bool,
        layer: u8,
        columns: u16,
        rows: u16,
        column_spacing: f64,
        row_spacing: f64,
    },
    BlockStart {
        name: String,
        base: Point,
    },
    BlockEnd,
}

/// Decode one ordinary entity's fields after its independent four-byte header.
fn read_entity_fields(
    bytes: &[u8],
    type_code: u16,
    record_at: usize,
    at: usize,
    index: u32,
    version: Version,
) -> Result<(Entity, usize), DwgError> {
    match type_code {
        TYPE_SHAPE => {
            // Original SHAPE RES/CAP commands: x, y, scale, radians, then
            // a u16 definition number (129/130 in ES.SHP).
            let x = checked_f64(bytes, at, index)?;
            let y = checked_f64(bytes, at + 8, index)?;
            let height = checked_f64(bytes, at + 16, index)?;
            let rotation_deg = checked_f64(bytes, at + 24, index)?.to_degrees();
            let number = checked_u16(bytes, at + 32, index)?;
            Ok((
                Entity::Shape {
                    origin: Point { x, y },
                    height,
                    rotation_deg,
                    number,
                },
                at + 34,
            ))
        }
        TYPE_LOAD => {
            // DISC.BAK and original LOAD commands: only a length-prefixed
            // library/font name follows the record header.
            let (name, next) = checked_string(bytes, at, index)?;
            Ok((Entity::Load { name }, next))
        }
        TYPE_LINE => {
            let x1 = checked_f64(bytes, at, index)?;
            let y1 = checked_f64(bytes, at + 8, index)?;
            let x2 = checked_f64(bytes, at + 16, index)?;
            let y2 = checked_f64(bytes, at + 24, index)?;
            Ok((
                Entity::Line {
                    start: Point { x: x1, y: y1 },
                    end: Point { x: x2, y: y2 },
                },
                at + 32,
            ))
        }
        TYPE_POINT => {
            // Layout read directly off SELEXOL's own first POINT (file
            // offset 0x1f5): header, then origin.x, origin.y as two
            // consecutive doubles — a clean (5.0, 5.0) immediately followed
            // by a plausible LINE, and the whole file's walk lands exactly
            // on entity_end with this size. The original's DXF export later
            // verified it; see the module doc.
            let x = checked_f64(bytes, at, index)?;
            let y = checked_f64(bytes, at + 8, index)?;
            Ok((
                Entity::Point {
                    origin: Point { x, y },
                },
                at + 16,
            ))
        }
        TYPE_CIRCLE => {
            let cx = checked_f64(bytes, at, index)?;
            let cy = checked_f64(bytes, at + 8, index)?;
            let radius = checked_f64(bytes, at + 16, index)?;
            Ok((
                Entity::Circle {
                    center: Point { x: cx, y: cy },
                    radius,
                },
                at + 24,
            ))
        }
        TYPE_ARC => {
            let cx = checked_f64(bytes, at, index)?;
            let cy = checked_f64(bytes, at + 8, index)?;
            let radius = checked_f64(bytes, at + 16, index)?;
            let start_rad = checked_f64(bytes, at + 24, index)?;
            let end_rad = checked_f64(bytes, at + 32, index)?;
            Ok((
                Entity::Arc {
                    center: Point { x: cx, y: cy },
                    radius,
                    // The DWG stores these in radians; acad_model documents
                    // Arc's angles as degrees (as AutoCAD 1.4 stored them),
                    // and the DXF oracle prints degrees too. Not normalised:
                    // SUBDIV's own first ARC sweeps counter-clockwise
                    // through 0°, start (78.604°) > end (22.451°).
                    start_deg: start_rad.to_degrees(),
                    end_deg: end_rad.to_degrees(),
                },
                at + 40,
            ))
        }
        TYPE_TEXT => {
            // Layout established against SUBDIV's own TEXT records (e.g. its
            // "A" label at file offset 0x1718): header, then origin.x,
            // origin.y, height, rotation (radians) as four consecutive
            // doubles, then the length-prefixed value *after* the numbers —
            // unlike BLOCK/INSERT, whose name comes first.
            //
            // In AC1.2 the stored height is not the DXF height directly: all
            // 8 of SUBDIV's TEXT records decode to 0.4613464999999999 there,
            // while SUBDIV.DXF prints 0.346010 for every one of them, and
            // that exact value never appears anywhere in the file (searched
            // at both the DXF's tolerance and much looser). The ratio is
            // 0.75 to seven significant figures (0.4613465 * 0.75 =
            // 0.3460098749999999, which rounds to the DXF's printed
            // 0.346010).
            //
            // For the AC1.2 corpus, 0.75 matches `above / (above + below)` from
            // the loaded `.SHP` font's own header (spec §4.2): the DWG
            // stores the font's full cell height (above-baseline + below-
            // baseline), the DXF reports cap height (above-baseline only).
            // `corpus/System/TXT.SHP` opens `*0,4,Roman Simplex` /
            // `21,7,0,0` — 21 above the baseline, 7 below — so
            // `21/(21+7) = 0.75`, matching SUBDIV exactly. Every text font
            // this corpus ships happens to use that same 21:7 (or
            // equivalently 3:1) ratio (`ITALIC`, `ROMAN-C`, `ROMAN-S`,
            // `System/TXT.SHP`; `Samples/TXT.SHP` at `6,2`), which is why
            // 0.75 works for every AC1.2 drawing in the corpus — but a drawing
            // using a font with a different above:below split would need
            // its own ratio read from its `.SHP` file. Hardcoding 0.75 here
            // is a corpus-wide coincidence, not a discovered constant; the
            // milestone ⑤ `.SHP` loader should read this ratio from the
            // font instead of relying on this arm. This is specific to the
            // entity field either way: the header's own TXTSIZE (spec §4.2,
            // offset 0xb4) holds SUBDIV's 0.2 raw, unconverted, so this is
            // not a global text-height unit and must not be applied there.
            // AC1.40 stores cap height directly: original HOUSE exports and
            // a generated 0.75-high TEXT independently verify that difference.
            let x = checked_f64(bytes, at, index)?;
            let y = checked_f64(bytes, at + 8, index)?;
            let height_raw = checked_f64(bytes, at + 16, index)?;
            let rotation_rad = checked_f64(bytes, at + 24, index)?;
            let (value, next) = checked_string(bytes, at + 32, index)?;
            Ok((
                Entity::Text {
                    origin: Point { x, y },
                    height: height_raw
                        * match version {
                            Version::Ac12 => 0.75,
                            Version::Ac140 => 1.0,
                        },
                    rotation_deg: rotation_rad.to_degrees(),
                    value,
                },
                next,
            ))
        }
        TYPE_TRACE | TYPE_SOLID => {
            // Header, then four points as 8 consecutive doubles (68 bytes
            // total with the header). Inferred from record size and
            // internal consistency, then checked against original DXF
            // exports. TRACE's shape makes
            // SELEXOL's and BLIVET's whole-file walks land exactly on
            // entity_end with the header's own record count, and SOLID's
            // identical shape does the same for FLOW. The four points are
            // stored in AutoCAD's usual "Z" order for these two entity
            // types (p1-p2 one edge, p4-p3 the parallel opposite edge), not
            // sequential winding — SELEXOL's own TRACE, (7,30.725),
            // (7,30.775), (2.5,30.725), (2.5,30.775), is a thin rectangle
            // only under that reading; `acad-render`'s `flatten` draws them
            // accordingly.
            let mut pts = [Point { x: 0.0, y: 0.0 }; 4];
            for (i, slot) in pts.iter_mut().enumerate() {
                let x = checked_f64(bytes, at + i * 16, index)?;
                let y = checked_f64(bytes, at + i * 16 + 8, index)?;
                *slot = Point { x, y };
            }
            let entity = if type_code == TYPE_TRACE {
                Entity::Trace {
                    p1: pts[0],
                    p2: pts[1],
                    p3: pts[2],
                    p4: pts[3],
                }
            } else {
                Entity::Solid {
                    p1: pts[0],
                    p2: pts[1],
                    p3: pts[2],
                    p4: pts[3],
                }
            };
            Ok((entity, at + 64))
        }
        TYPE_INSERT => {
            // Layout established against SUBDIV's first INSERT (also
            // HOUSEA, file offset 0x1648): header, then a length-prefixed
            // name — same placement as BLOCK's — then origin.x, origin.y,
            // x_scale, y_scale, rotation (radians) as five consecutive
            // doubles.
            let (name, after_name) = checked_string(bytes, at, index)?;
            let x = checked_f64(bytes, after_name, index)?;
            let y = checked_f64(bytes, after_name + 8, index)?;
            let x_scale = checked_f64(bytes, after_name + 16, index)?;
            let y_scale = checked_f64(bytes, after_name + 24, index)?;
            let rotation_rad = checked_f64(bytes, after_name + 32, index)?;
            Ok((
                Entity::Insert {
                    origin: Point { x, y },
                    x_scale,
                    y_scale,
                    rotation_deg: rotation_rad.to_degrees(),
                    name,
                },
                after_name + 40,
            ))
        }
        code => Err(DwgError::UnknownEntityType {
            code,
            at: record_at,
        }),
    }
}

/// Decode one independent physical record; every marker and leaf counts once.
/// Signed records retain full fields; assembly validates the native whole-owner
/// sign policy. Negative BLOCK markers remain unsupported.
pub(super) fn read_record_body(
    bytes: &[u8],
    pos: usize,
    index: u32,
    version: Version,
) -> Result<(RecordBody, usize, u32), DwgError> {
    let header = read_record_header(bytes, pos, index)?;
    // The type code is a signed i16: a negative value marks an erased
    // record whose magnitude is its real type (spec §4.2, module doc). The
    // record's body is still fully, correctly decoded below — only the
    // final result is marked erased — so a truncated erased record is still
    // TruncatedEntity, never silently accepted.
    let signed = header.type_code as i16;
    let erased = signed.is_negative();
    let type_code = signed.unsigned_abs();

    let (body, next, logical): (RecordBody, usize, u32) = match type_code {
        TYPE_LINE | TYPE_POINT | TYPE_CIRCLE | TYPE_ARC | TYPE_TEXT | TYPE_TRACE | TYPE_SOLID
        | TYPE_INSERT | TYPE_LOAD | TYPE_SHAPE => {
            let (entity, next) =
                read_entity_fields(bytes, type_code, pos, pos + 4, index, version)?;
            if !acad_model::group_codec::entity_fields_are_finite(&entity) {
                return Err(DwgError::NonFiniteEntity { index, at: pos });
            }
            let layer = u8::try_from(header.layer).map_err(|_| DwgError::InvalidEntityLayer {
                index,
                value: header.layer,
            })?;
            (
                RecordBody::Entity(Entity::OnLayer {
                    layer,
                    entity: Box::new(entity),
                }),
                next,
                1,
            )
        }
        TYPE_BLOCK => {
            // Layout established against SUBDIV's first BLOCK (HOUSEA, file
            // offset 0xaac): header, then a length-prefixed name, then
            // base.x, base.y as two consecutive doubles right after it.
            let (name, after_name) = checked_string(bytes, pos + 4, index)?;
            let base_x = checked_f64(bytes, after_name, index)?;
            let base_y = checked_f64(bytes, after_name + 8, index)?;
            (
                RecordBody::BlockStart {
                    name,
                    base: Point {
                        x: base_x,
                        y: base_y,
                    },
                },
                after_name + 16,
                1,
            )
        }
        TYPE_ENDBLK => (RecordBody::BlockEnd, pos + 4, 1),
        TYPE_REPEAT => {
            let layer = u8::try_from(header.layer).map_err(|_| DwgError::InvalidEntityLayer {
                index,
                value: header.layer,
            })?;
            (RecordBody::RepeatStart { layer, erased }, pos + 4, 1)
        }
        TYPE_ENDREP => {
            // header(4) + a u16 pair(4) + 2 doubles(16) = 24 bytes, fixed.
            // Original commands confirm columns, rows and both distances.
            let columns = checked_u16(bytes, pos + 4, index)?;
            let rows = checked_u16(bytes, pos + 6, index)?;
            let column_spacing = checked_f64(bytes, pos + 8, index)?;
            let row_spacing = checked_f64(bytes, pos + 16, index)?;
            (
                RecordBody::RepeatEnd {
                    erased,
                    layer: u8::try_from(header.layer).map_err(|_| {
                        DwgError::InvalidEntityLayer {
                            index,
                            value: header.layer,
                        }
                    })?,
                    columns,
                    rows,
                    column_spacing,
                    row_spacing,
                },
                pos + 24,
                1,
            )
        }
        code => return Err(DwgError::UnknownEntityType { code, at: pos }),
    };

    if erased {
        let body = match body {
            RecordBody::Entity(entity) => RecordBody::ErasedEntity(entity),
            marker @ (RecordBody::RepeatStart { .. } | RecordBody::RepeatEnd { .. }) => marker,
            _ => {
                return Err(DwgError::UnsupportedErasedStructure {
                    code: type_code,
                    at: pos,
                })
            }
        };
        Ok((body, next, logical))
    } else {
        Ok((body, next, logical))
    }
}
