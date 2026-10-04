//! The entity record walk (spec §4.2, §4.4).
//!
//! Entity records sit after the header and layer table, from the
//! version-specific start (`AC1.2`: 0x1D8; `AC1.40`: 0x202) to
//! `meta.entity_end`. Each record is a signed `u16` type code, a `u16` layer
//! index, then the type's own fields. The second word matches entity layer
//! suffixes in original AutoCAD DXF exports, including the complete SUBDIV
//! stream. The type code is a
//! 1-based index into the entity name table recovered from `ACAD.EXE`:
//! 1 `LINE`, 2 `POINT`, 3 `CIRCLE`, 4 `SHAPE`, 5 `REPEAT`, 6 `ENDREP`,
//! 7 `TEXT`, 8 `ARC`, 9 `TRACE`, 10 `LOAD`, 11 `SOLID`, 12 `BLOCK`,
//! 13 `ENDBLK`, 14 `INSERT`. All fourteen codes are decoded. `LOAD` names
//! a font or shape library and stays in document order; `SHAPE` references
//! a numeric definition with position, scale and rotation. Their layouts
//! are verified by original AC1.40 commands and DISC.BAK's font loads;
//! no AC1.2 corpus drawing exercises those two codes.
//!
//! **Erased entities (spec §4.2, Part A of Task 8).** The type code is a
//! **signed** `i16`; a negative value marks an erased entity whose magnitude
//! is its real type (`ADDER.DWG`'s `-1` is an erased `LINE`, `-14` an erased
//! `INSERT` — read as unsigned they are the nonsensical 65535 and 65522).
//! The record keeps its full body — presumably how `OOPS` restores the last
//! erase — so the byte walk decodes it exactly like the live version of the
//! same type. `read_items` keeps ordinary erased records in place as
//! `Item::Erased`, while the geometry-only `read_entities` omits them.
//! Confirmed on `ADDER`, the
//! corpus's only file with any: reading the sign makes it walk exactly 66
//! records to `entity_end` (matching its header's `entity_count`, which
//! counts all 7 erased records too), with all 7 landing on `LINE` or
//! `INSERT`. `SUBDIV` and the other 15 `AC1.2` drawings have none.
//!
//! **Task 6 placeholders.** The original's Load DXF (Main Menu task 6)
//! writes one zero-filled erased record per DXF header record, typed by the
//! keyword's index (1..=13, including unpaired erased ENDREP, BLOCK and
//! ENDBLK). At the top level `read_items` counts the structural ones toward
//! the header's count/end and drops them, as the original's own END does;
//! the ordinary ones are ordinary erased records. Any other erased structure
//! keeps its named error (docs/native-group-persistence.md).
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
//! **REPEAT/ENDREP.** Retained native physical-reader/writer descriptors prove
//! REPEAT is an independent four-byte marker with no body. What older notes
//! called a fused first child is the next ordinary entity's type/layer header.
//! Every marker and leaf counts once. ENDREP has two u16 dimensions and two
//! f64 spacings. See docs/native-group-persistence.md for exact static bytes,
//! independent corpus walks, and the explicit native erased-owner policy.
//! Marker layers are retained separately from children and do not imply a
//! Rust visibility owner. Logical grouping uses a bounded stack; child layers
//! and ordered LOAD records are preserved. Explicit runtime owner wrappers and
//! mixed-sign subtrees and erased subgroups in live groups produce errors
//! instead of losing metadata; erased ordinary members in live groups are
//! retained (B4). A uniformly negative top-level Repeat reconstructs one
//! native erased owner, with all nested fields retained. This is native policy,
//! not original whole-group selection parity. A group without members (the
//! original's own empty REPEAT/ENDREP pair, R6) is kept as an empty `Repeat`.
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
//! flat, file-order `Vec<Entity>` as a geometry-only convenience view.
//! `read_items` groups both block definitions and repeat patterns, preserving
//! their document order and the patterns nested in block definitions.

mod record;
mod stream;
pub use record::RecordHeader;
pub use stream::{read_entities, read_items};
#[cfg(test)]
mod tests;
