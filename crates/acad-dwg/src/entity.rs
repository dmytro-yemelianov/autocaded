//! The entity record walk (spec §4.2, §4.4).
//!
//! Entity records sit after the header and layer table, from the
//! version-specific start (`AC1.2`: 0x1D8; `AC1.40`: 0x202) to
//! `meta.entity_end`. Each record is a `u16` type code, a `u16` of not-yet-understood
//! meaning ("flags" below), then the type's own fields. The type code is a
//! 1-based index into the entity name table recovered from `ACAD.EXE`:
//! 1 `LINE`, 2 `POINT`, 3 `CIRCLE`, 4 `SHAPE`, 5 `REPEAT`, 6 `ENDREP`,
//! 7 `TEXT`, 8 `ARC`, 9 `TRACE`, 10 `LOAD`, 11 `SOLID`, 12 `BLOCK`,
//! 13 `ENDBLK`, 14 `INSERT`. Every code except `SHAPE`(4) and `LOAD`(10) is
//! implemented here. `DISC.BAK` uses `LOAD` to select fonts, which the
//! model cannot yet preserve; it remains `DwgError::UnknownEntityType`.
//!
//! **Erased entities (spec §4.2, Part A of Task 8).** The type code is a
//! **signed** `i16`; a negative value marks an erased entity whose magnitude
//! is its real type (`ADDER.DWG`'s `-1` is an erased `LINE`, `-14` an erased
//! `INSERT` — read as unsigned they are the nonsensical 65535 and 65522).
//! The record keeps its full body — presumably how `OOPS` restores the last
//! erase — so the byte walk decodes it exactly like the live version of the
//! same type and simply discards the result. Confirmed on `ADDER`, the
//! corpus's only file with any: reading the sign makes it walk exactly 66
//! records to `entity_end` (matching its header's `entity_count`, which
//! counts all 7 erased records too), with all 7 landing on `LINE` or
//! `INSERT`. `SUBDIV` and the other 15 `AC1.2` drawings have none.
//!
//! **`POINT`, `TRACE`, `SOLID` (Task 8, Part B).** Straightforward
//! fixed-field records like `LINE`/`CIRCLE`: `POINT` is a header plus one
//! point (20 bytes); `TRACE` and `SOLID` are a header plus four points, 8
//! doubles, 68 bytes. They were initially inferred from record size and internal
//! consistency (the whole-file walk landing exactly on `entity_end` with a
//! record count matching the header's, across the files that use them: 68
//! bytes for `TRACE` on `SELEXOL` — 167/167 records, exact byte landing —
//! and independently on `BLIVET`; `SOLID` the same shape on `FLOW`,
//! 177/177). `POINT`'s two doubles were read directly off `SELEXOL` bytes:
//! a clean `(5.0, 5.0)` immediately followed by a plausible `LINE`, with no
//! plausible alternative field count. DXFs freshly exported by the original
//! under QEMU now independently verify all three layouts (`acad-oracle`).
//!
//! **`REPEAT`/`ENDREP` (Task 8, Part B) — the one non-leaf construct.**
//! Initially inferred without a DXF oracle, but cross-checked against
//! *two* independent corpus files (`FLOOR`, whose only failing code was
//! `REPEAT`, and `BLIVET`, which turned out to use it too once `TRACE` was
//! in place) — both land exactly on `entity_end` with a record count
//! matching the header once the accounting below is applied. `REPEAT`
//! fuses two logical records into one physical one: header, then a `u16`
//! pair — the first (`nested_type`) is *itself* an entity-table type code,
//! the second is uninterpreted here — then that nested type's own fields
//! **with no separate 4-byte header of its own** (`REPEAT`'s header stands
//! in for it). Evidenced nested types: `LINE` (`FLOOR`, both of its
//! `REPEAT`s; `BLIVET`, 3 of its 5) and `INSERT` (`BLIVET`, 2 of its 5, one
//! naming block `"$BCIRC"`) — no other nested type appears anywhere in the
//! corpus, so only those two are decoded; anything else is
//! `DwgError::UnknownEntityType`, not a guess. This nested record **is**
//! counted as its own entity by the header's `entity_count` even though it
//! shares `REPEAT`'s physical bytes: `FLOOR` has 2 `REPEAT`s and its walk is
//! short by exactly 2 until this is accounted for; `BLIVET` has 5 and is
//! short by exactly 5. So it is decoded and emitted as a real `Entity` here
//! — it is unambiguously present, real geometry, not invented.
//!
//! What is *not* modelled, because the corpus gives no evidence for it: the
//! actual "repeat" semantics. `REPEAT`'s second `u16` (a plausible "repeat
//! count") is 1 in every simple case and otherwise uncorrelated with
//! anything checkable, and no case in the corpus shows more than one copy
//! of a repeated element with a discoverable offset between copies — so no
//! repetition is synthesised; the nested entity is emitted exactly once,
//! and ordinary entity records between a `REPEAT` and its `ENDREP` (`FLOOR`
//! nests 3 plain `LINE` records this way) are decoded completely normally
//! by the same per-record dispatch, with no special "inside a repeat"
//! behaviour at all. `ENDREP` is simpler to size (header, the same kind of
//! `u16` pair, then always exactly two more doubles — 24 bytes, fixed,
//! regardless of what the pair holds) but its own two doubles and its `u16`
//! pair have no evidenced meaning — no correlation was found against the
//! `REPEAT` they close, the nested entities between, or anything else
//! checkable — so they are consumed for correct byte alignment and produce
//! no entity. This is the "decode and skip the construct" half of Task 8's
//! dispatch guidance: the construct's own open/close bookkeeping is walked
//! over, not represented, while the genuine entities it contains (both the
//! ones between `REPEAT`/`ENDREP` and the one fused into `REPEAT` itself)
//! are read like any other record. Fresh original DXF exports verify the
//! enclosed entities and their order, while full repeat semantics remain open.
//!
//! `ARC`, `TEXT` and `INSERT` each carry an angle field stored in radians;
//! they are converted to degrees here because `acad_model` documents them in
//! degrees, and the angles are not normalised — `SUBDIV`'s own first `ARC`
//! sweeps counter-clockwise through 0°.
//!
//! In `AC1.2`, `TEXT`'s height field is *also* not a direct copy: the raw double is 4/3
//! the DXF's printed height (see the `TYPE_TEXT` arm of `read_entity_fields`
//! for how this was found), so it is scaled by 0.75 on the way in. This is
//! local to the entity field — the header's own `TXTSIZE` needs no such
//! scaling — and was not expected going in; only the angle fields were known
//! to need a unit conversion. `AC1.40` stores the DXF height directly,
//! verified by generated TEXT commands and the HOUSE/OFFICE exports.
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
//! inside a `BLOCK`/`ENDBLK` pair, or a `REPEAT`/`ENDREP` pair) into one
//! flat, file-order `Vec<Entity>`, skipping `BLOCK`/`ENDBLK`/`ENDREP`
//! themselves without erroring on them — it has no way to represent them,
//! since `acad_model::Entity` has no block or repeat variant. `read_items`
//! walks the same records through the same per-record decoder but tracks
//! `BLOCK`/`ENDBLK` nesting to group each pair's entities into one
//! `Item::Block`, preserving interleaving with loose entities exactly as
//! `acad_model::drawing`'s doc comment requires; `REPEAT`/`ENDREP` are not
//! given the same grouping treatment (see above) — the entities they
//! surround land as ordinary loose `Item::Entity`s, same as `read_entities`.

use crate::header::{HeaderMeta, Version};
use crate::text::decode_latin1;
use crate::DwgError;
use acad_model::{Block, Entity, Item, Point};

/// Entity start for AC1.2 test fixtures; production reads it from `Version`.
#[cfg(test)]
const ENTITY_START: usize = 0x1D8;

/// 1-based index into `ACAD.EXE`'s entity name table; `LINE` is its first
/// entry.
const TYPE_LINE: u16 = 1;

/// `ACAD.EXE`'s entity name table, entry 2: `POINT`.
const TYPE_POINT: u16 = 2;

/// `ACAD.EXE`'s entity name table, entry 3: `CIRCLE`.
const TYPE_CIRCLE: u16 = 3;

/// `ACAD.EXE`'s entity name table, entry 5: `REPEAT`. Not a leaf entity —
/// see the module doc's `REPEAT`/`ENDREP` section.
const TYPE_REPEAT: u16 = 5;

/// `ACAD.EXE`'s entity name table, entry 6: `ENDREP`.
const TYPE_ENDREP: u16 = 6;

/// `ACAD.EXE`'s entity name table, entry 7: `TEXT`.
const TYPE_TEXT: u16 = 7;

/// `ACAD.EXE`'s entity name table, entry 8: `ARC`.
const TYPE_ARC: u16 = 8;

/// `ACAD.EXE`'s entity name table, entry 9: `TRACE`.
const TYPE_TRACE: u16 = 9;

/// `ACAD.EXE`'s entity name table, entry 11: `SOLID`.
const TYPE_SOLID: u16 = 11;

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
/// decoded entity, one of the `BLOCK`/`ENDBLK` delimiters `read_items` groups
/// by, or nothing (an erased record, or `ENDREP` — see the module doc). Not
/// `pub` — `read_entities` and `read_items` are the crate's two supported
/// ways to walk the entity stream; nothing outside this module needs a
/// record-level view.
enum RecordBody {
    Entity(Entity),
    BlockStart {
        name: String,
        base: Point,
    },
    BlockEnd,
    /// Consumed correctly for byte alignment, but produces no output:
    /// either the type code was negative (an erased record — spec §4.2) and
    /// its otherwise-normal body is simply discarded, or the record is
    /// `ENDREP`, whose own fields have no evidenced meaning (module doc).
    Skip,
}

/// Decodes one entity's fields, given its type code and the offset right
/// after its 4-byte header. Shared by the top-level dispatch in
/// `read_record_body` (where that header is a genuine on-disk record header
/// at `record_at`) and `REPEAT`'s nested entity (where `record_at` is the
/// enclosing `REPEAT` record's own start — it has no separate header of its
/// own on disk; `REPEAT`'s header and `u16` pair stand in for it). `at` is
/// where the type-specific fields actually begin, which for the two callers
/// is `record_at + 4` and `record_at + 8` respectively.
fn read_entity_fields(
    bytes: &[u8],
    type_code: u16,
    record_at: usize,
    at: usize,
    index: u32,
    version: Version,
) -> Result<(Entity, usize), DwgError> {
    match type_code {
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

/// Decodes the one record starting at `pos`, returning its payload, the
/// offset of the next record, and how many of the header's `entity_count`
/// slots this physical record fills — 1 for everything except `REPEAT`,
/// which fuses a container marker with one nested entity into a single
/// on-disk record but counts as 2 (module doc). Shared by `read_entities`
/// (which keeps only the `Entity` payloads, flattening past
/// `BLOCK`/`ENDBLK`/`ENDREP`) and `read_items` (which uses the `BLOCK`
/// delimiters to group).
fn read_record_body(
    bytes: &[u8],
    pos: usize,
    index: u32,
    version: Version,
) -> Result<(RecordBody, usize, u32), DwgError> {
    let header = read_record_header(bytes, pos, index)?;
    // The type code is a signed i16: a negative value marks an erased
    // record whose magnitude is its real type (spec §4.2, module doc). The
    // record's body is still fully, correctly decoded below — only the
    // final result is discarded — so a truncated erased record is still
    // TruncatedEntity, never silently accepted.
    let signed = header.type_code as i16;
    let erased = signed.is_negative();
    let type_code = signed.unsigned_abs();

    let (body, next, logical): (RecordBody, usize, u32) = match type_code {
        TYPE_LINE | TYPE_POINT | TYPE_CIRCLE | TYPE_ARC | TYPE_TEXT | TYPE_TRACE | TYPE_SOLID
        | TYPE_INSERT => {
            let (entity, next) =
                read_entity_fields(bytes, type_code, pos, pos + 4, index, version)?;
            (RecordBody::Entity(entity), next, 1)
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
            // header(4) + [nested_type, repeat_count] as two u16s (4) + the
            // nested type's own fields, with no separate 4-byte header of
            // its own — REPEAT's header stands in for it (module doc).
            // `repeat_count` is read (so a truncated file still errors) but
            // not applied: no corpus REPEAT shows a discoverable offset
            // between copies, so no repetition is synthesised.
            let nested_type = checked_u16(bytes, pos + 4, index)?;
            let _repeat_count = checked_u16(bytes, pos + 6, index)?;
            let (entity, next) =
                read_entity_fields(bytes, nested_type, pos, pos + 8, index, version)?;
            (RecordBody::Entity(entity), next, 2)
        }
        TYPE_ENDREP => {
            // header(4) + a u16 pair(4) + 2 doubles(16) = 24 bytes, fixed.
            // Read fully (so truncation still errors) but not interpreted —
            // no evidenced meaning for any of it (module doc).
            let _u1 = checked_u16(bytes, pos + 4, index)?;
            let _u2 = checked_u16(bytes, pos + 6, index)?;
            let _d1 = checked_f64(bytes, pos + 8, index)?;
            let _d2 = checked_f64(bytes, pos + 16, index)?;
            (RecordBody::Skip, pos + 24, 1)
        }
        code => return Err(DwgError::UnknownEntityType { code, at: pos }),
    };

    if erased {
        Ok((RecordBody::Skip, next, logical))
    } else {
        Ok((body, next, logical))
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
    let mut pos = meta.version.entity_start();
    let mut index: u32 = 0;
    let mut out = Vec::new();

    while pos < end {
        let (body, next, logical) = read_record_body(bytes, pos, index, meta.version)?;
        if let RecordBody::Entity(e) = body {
            out.push(e);
        }
        pos = next;
        index += logical;
    }

    // The loop above only ever exits with pos >= end (its own condition), so
    // this catches the walk landing strictly past entity_end — a record's
    // on-disk size carried it beyond the header's declared boundary — and
    // also the degenerate case where entity_end was already at or before
    // ENTITY_START (pos starts there and the loop never runs at all). Either
    // way this must be an error, not a silently accepted mismatch: the
    // module doc's evidence for POINT/TRACE/SOLID/REPEAT rests specifically
    // on the walk landing *exactly* on entity_end, and that claim is only
    // true if something checks it.
    if pos != end {
        return Err(DwgError::WalkOverran {
            pos,
            entity_end: end,
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

/// The same walk as [`read_entities`], over the same per-record decoder, but
/// tracking `BLOCK`/`ENDBLK` nesting to group each pair's entities into one
/// `Item::Block` rather than dropping the boundary. Loose entities and block
/// definitions interleave in document order (`acad_model::drawing`'s doc
/// comment), which this preserves: nothing is bucketed by kind.
///
/// The block table is flat (`acad_model::Block`'s `entities: Vec<Entity>`
/// has no room for a nested `Block`, and none is needed): a `BLOCK` record
/// encountered while another is already open defines a **sibling**, not a
/// child — evidenced by `SELEXOL`, which defines `ARROW` inside `COOLER`'s
/// own span and then `INSERT`s `ARROW` fourteen times at top level, all
/// outside `COOLER`'s span. A definition referenced from outside its
/// parent's span cannot be scoped to that parent. So open `BLOCK`s are
/// tracked as a stack: each `BLOCK` pushes, each `ENDBLK` pops the
/// *innermost* one and emits it as `Item::Block` in the position its own
/// `ENDBLK` closed — an inner block that closes before its outer one
/// therefore appears earlier in `items` than the outer one, exactly as
/// `a_block_defined_inside_another_is_a_sibling_not_a_child` (below) checks.
/// Every other record is pushed onto the innermost open block, or onto
/// `items` directly when the stack is empty.
///
/// A `BLOCK` left open when `meta.entity_end` is reached is
/// `DwgError::UnterminatedBlock`, naming the **innermost** still-open
/// block and the offset its own `BLOCK` record started at — not any
/// enclosing block, which may be fine; naming the outer one would send a
/// reader to the wrong offset. A stray `ENDBLK` with nothing open is
/// `DwgError::StrayEndblk`, naming its own offset: the stack is popped only
/// when non-empty, so this can never silently underflow.
pub fn read_items(bytes: &[u8], meta: &HeaderMeta) -> Result<Vec<Item>, DwgError> {
    let end = meta.entity_end as usize;
    let mut pos = meta.version.entity_start();
    let mut index: u32 = 0;
    let mut out = Vec::new();
    let mut open: Vec<(usize, Block)> = Vec::new();

    while pos < end {
        let (body, next, logical) = read_record_body(bytes, pos, index, meta.version)?;
        match body {
            RecordBody::Entity(e) => match open.last_mut() {
                Some((_, block)) => block.entities.push(e),
                None => out.push(Item::Entity(e)),
            },
            RecordBody::BlockStart { name, base } => {
                open.push((
                    pos,
                    Block {
                        name,
                        base,
                        entities: Vec::new(),
                    },
                ));
            }
            RecordBody::BlockEnd => match open.pop() {
                Some((_, block)) => out.push(Item::Block(block)),
                None => return Err(DwgError::StrayEndblk { at: pos }),
            },
            RecordBody::Skip => {}
        }
        pos = next;
        index += logical;
    }

    // See read_entities's identical check for why this is needed: the loop
    // only exits with pos >= end, so pos != end here means either a record
    // walked past the declared entity_end, or entity_end never gave the
    // walk anywhere valid to land in the first place.
    if pos != end {
        return Err(DwgError::WalkOverran {
            pos,
            entity_end: end,
        });
    }

    // A non-empty stack means at least one BLOCK never got its ENDBLK. The
    // innermost one (the last pushed) is the informative one to name — it's
    // the record whose own body is actually missing bytes; any enclosing
    // block may well have gone on to close correctly had the file continued.
    if let Some((at, block)) = open.pop() {
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

    /// A `REPEAT` record wrapping a nested `LINE` — no separate 4-byte
    /// header for the nested entity, since `REPEAT`'s own header and `u16`
    /// pair stand in for it (module doc). `repeat_count` is written but this
    /// crate never reads it back out; it exists so tests can show it doesn't
    /// affect decoding.
    fn raw_repeat_line(repeat_count: u16, x1: f64, y1: f64, x2: f64, y2: f64) -> Vec<u8> {
        let mut r = Vec::new();
        r.extend_from_slice(&TYPE_REPEAT.to_le_bytes());
        r.extend_from_slice(&0u16.to_le_bytes());
        r.extend_from_slice(&TYPE_LINE.to_le_bytes());
        r.extend_from_slice(&repeat_count.to_le_bytes());
        for v in [x1, y1, x2, y2] {
            r.extend_from_slice(&v.to_le_bytes());
        }
        r
    }

    /// A `REPEAT` wrapping a nested `INSERT` — evidenced by `BLIVET`'s own
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

    /// A `REPEAT` record with an unsupported nested type — nothing in the
    /// corpus needs any nested type but `LINE` or `INSERT` (module doc), so
    /// anything else must be a named error, not a guess.
    fn raw_repeat_unsupported(nested_type: u16) -> Vec<u8> {
        let mut r = Vec::new();
        r.extend_from_slice(&TYPE_REPEAT.to_le_bytes());
        r.extend_from_slice(&0u16.to_le_bytes());
        r.extend_from_slice(&nested_type.to_le_bytes());
        r.extend_from_slice(&1u16.to_le_bytes());
        r
    }

    /// A fixed 24-byte `ENDREP`: header, a `u16` pair, two doubles — none of
    /// it interpreted (module doc).
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
            vec![Item::Entity(entities[0].clone())]
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
        assert!(matches!(items[0], Item::Entity(Entity::Line { .. })));
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
    fn a_repeats_embedded_line_decodes_as_a_real_entity() {
        // BLIVET's simple case: REPEAT immediately followed by ENDREP, no
        // entities nested between them. REPEAT's own u16 pair and 4 doubles
        // are the nested LINE's fields (module doc) — values are BLIVET's
        // own first REPEAT (file offset 0x3b4).
        let bytes = records(&[
            raw_repeat_line(1, 0.01999999, 0.25, 2.23, 0.25),
            raw_endrep(1, 5, 0.0, 0.25),
        ]);
        // REPEAT counts as 2 (container + its fused nested entity), ENDREP
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
    fn a_repeats_embedded_insert_decodes_as_a_real_entity() {
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
        // FLOOR's case: REPEAT's own embedded LINE is one edge of a small
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
        // REPEAT: 2, three LINEs: 1 each, ENDREP: 1 -> 2+3+1 = 6.
        let meta = HeaderMeta {
            version: Version::Ac12,
            entity_count: 6,
            entity_end: bytes.len() as u32,
        };
        let entities = read_entities(&bytes, &meta).unwrap();
        assert_eq!(entities.len(), 4);
        assert!(entities.iter().all(|e| matches!(e, Entity::Line { .. })));
        // The rectangle closes: REPEAT's own edge starts at (7.12, 8.4), the
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
    fn an_unsupported_nested_type_inside_repeat_is_an_unknown_entity_type_error() {
        // Nothing in the corpus nests anything but LINE or INSERT inside a
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
                at: ENTITY_START,
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
    fn read_items_also_skips_erased_records() {
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
        assert_eq!(items.len(), 1);
        assert!(matches!(items[0], Item::Entity(Entity::Line { .. })));
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
}
