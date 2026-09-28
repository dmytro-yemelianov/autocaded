//! Every `AC1.2` drawing in the corpus, run end-to-end through `parse`,
//! `flatten` and `rasterize` — the same pipeline `acad-app` runs and
//! `crates/acad-render/tests/raster.rs` already checks for `SUBDIV` alone
//! (spec §8, milestone ④). This is milestone ④'s headline claim widened from
//! one drawing to the whole corpus, and it is written to be honest about
//! where that claim currently stops.
//!
//! Task 7 left 11 of the 16 rendering and 5 failing with
//! `DwgError::UnknownEntityType`: `SELEXOL` (`POINT`), `FLOOR` (`REPEAT`),
//! `BLIVET` (`TRACE`), `FLOW` (`SOLID`), and `ADDER` (a code that didn't even
//! index the entity table — turned out to be a *signed* type code marking an
//! erased entity, spec §4.2). Task 8 added all four record types plus signed
//! erasure handling (`crates/acad-dwg/src/entity.rs`'s module doc has the
//! full evidence for each), which is enough to fully fix 3 of the 5:
//! **`ADDER`, `FLOOR` and `FLOW` now parse and render**, moving `RENDERS`
//! from 11 to 14.
//!
//! `SELEXOL` and `BLIVET` still don't, but not for the reason Task 7 found —
//! `POINT`, `TRACE` and (for `BLIVET`) `REPEAT` all decode correctly now
//! (`read_entities`, the flat per-record walk with no `BLOCK` nesting
//! validation, succeeds on both — see `both_still_unsupported_files_decode_
//! cleanly_at_the_flat_record_level` below, real corpus bytes, not a
//! synthetic fixture). What actually stops them is a **separate, newly
//! discovered limitation**: both files contain a `BLOCK` definition nested
//! inside another `BLOCK` definition (`SELEXOL`'s `"HEAD"` inside
//! `"PACKTWR"`, and independently `"ARROW"` inside `"COOLER"`; `BLIVET`'s
//! `"$BCIRC"` inside a block named `"BLIVET"`), which `read_items` cannot
//! represent — its `Item::Block` is a single flat level, tracked with one
//! `Option`, not a stack, and `acad_model::Block.entities` is `Vec<Entity>`,
//! with no room for a nested `Block` inside it. Both files are perfectly
//! well-formed — every `BLOCK` they hold does have a matching `ENDBLK` —
//! so opening a second `BLOCK` while one is already open is
//! `DwgError::NestedBlock`, naming both the outer and inner block and the
//! inner one's own offset, not `UnterminatedBlock` (final review Fix 2:
//! that used to be raised here, naming only the outer block, which falsely
//! implied the file itself was malformed). This is not one of the four
//! record types Task 8's brief named, and fixing it properly means
//! widening `acad_model::Block`/`Item` to nest — a real design decision
//! with no DXF oracle for either file to verify it against, so it is left
//! unimplemented and reported here rather than guessed at.
//!
//! `SUBDIV` is still the only drawing with a DXF sibling
//! (`parallel_corpus.rs`, `entity_corpus.rs`), so `LINE`, `CIRCLE`, `ARC`,
//! `TEXT`, `BLOCK`, `ENDBLK` and `INSERT` are the only types **verified**
//! against it. `POINT`, `TRACE`, `SOLID` and `REPEAT`/`ENDREP` are
//! **inferred** — record size and whole-file-walk consistency only, cross-
//! checked across two independent files apiece where the corpus allowed it
//! (see the module doc on `crates/acad-dwg/src/entity.rs` for exactly what
//! evidence backs each). The other 10 pre-existing `RENDERS` drawings use
//! only the seven verified types, so their success is evidence the layout
//! generalises, not a second independent verification.

use acad_render::{flatten, rasterize, Viewport};

const CANVAS: u32 = 800;

/// A drawn-content floor for `known_good_drawings_parse_and_render_non_blank`.
/// Comfortably below the smallest of the 14 drawings' real pixel counts
/// (ORGATE's 2,466 on an 800x800 canvas — see that test), so it catches
/// "drew almost nothing" as well as "drew nothing", not just a bare `> 0`.
const MIN_LIT: usize = 500;

/// The corpus is extracted from archives that are deliberately not in git, so
/// a fresh checkout has none. Tests that need it skip rather than fail.
fn corpus(name: &str) -> Option<Vec<u8>> {
    let path = format!("../../corpus/Samples/{name}.DWG");
    match std::fs::read(&path) {
        Ok(b) => Some(b),
        Err(_) => {
            eprintln!("skipping: {path} absent (run ./tools/extract-corpus.sh)");
            None
        }
    }
}

/// The 14 `AC1.2` drawings that parse and render today. The first 11 use
/// only the record types verified against `SUBDIV.DXF`; `ADDER`, `FLOOR` and
/// `FLOW` are Task 8's new fixes (erasure, `REPEAT`, `SOLID` respectively —
/// see the module doc).
const RENDERS: &[&str] = &[
    "BOX", "ORGATE", "DLATCH", "SUBDIV", "ANDGATE", "NORGATE", "XORGATE", "HALFADD", "INVERTER",
    "NANDGATE", "XNORGATE", "ADDER", "FLOOR", "FLOW",
];

/// The 2 drawings that still fail, and exactly how: both are
/// `DwgError::NestedBlock`, naming the outer `BLOCK` that was still open,
/// the inner `BLOCK` that opened inside it, and the offset the inner one
/// itself started at — see the module doc's explanation of why. Any change
/// to these — a fix, a different failure, a different offset — is real news
/// about the codec and must change this table, not be masked by a looser
/// assertion.
const UNSUPPORTED: &[(&str, &str, &str, usize)] = &[
    ("SELEXOL", "PACKTWR", "HEAD", 0x395),
    ("BLIVET", "BLIVET", "$BCIRC", 0x146b),
];

#[test]
fn the_census_accounts_for_all_sixteen_ac12_drawings() {
    assert_eq!(RENDERS.len() + UNSUPPORTED.len(), 16);
}

#[test]
fn known_good_drawings_parse_and_render_non_blank() {
    let mut ran = 0;
    for name in RENDERS {
        let Some(bytes) = corpus(name) else { return };
        let drawing = acad_dwg::parse(&bytes)
            .unwrap_or_else(|e| panic!("{name}: expected to parse, got {e}"));
        let vp = Viewport::fit(&drawing.header.limits, CANVAS, CANVAS);
        let pm = rasterize(&flatten(&drawing, &vp), CANVAS, CANVAS);
        // The background `rasterize` fills is opaque black and every stroke
        // it draws is opaque white, so alpha is 255 everywhere regardless of
        // what was drawn (see raster.rs's own
        // an_empty_drawing_rasterizes_to_uniform_opaque_background, which
        // asserts exactly that for an empty scene). Content must be told
        // apart by colour, the way
        // background_is_opaque_so_strokes_are_visible_when_composited
        // already does, not by alpha.
        let lit = pm
            .pixels()
            .iter()
            .filter(|p| p.red() > 0 || p.green() > 0 || p.blue() > 0)
            .count();
        // The smallest of the 11 (ORGATE) draws 2,466 lit pixels on this
        // 800x800 canvas; MIN_LIT sits well under that so a real regression —
        // a viewport bug leaving geometry off-canvas, flatten dropping
        // everything — trips it, while never being so close to the real
        // values that anti-aliasing noise could.
        assert!(
            lit > MIN_LIT,
            "{name}: parsed but rendered near-blank ({lit} lit pixels, need > {MIN_LIT})"
        );
        ran += 1;
    }
    assert_eq!(ran, RENDERS.len());
}

// known_unsupported_drawings_fail_with_the_recorded_type_code used to live
// here, asserting that SELEXOL and BLIVET fail to parse with
// DwgError::NestedBlock. The nested-block-definitions plan's Task 1 (see
// .superpowers/sdd/2026-09-28-nested-block-definitions/) removed that
// variant: read_items now tracks open BLOCKs with a stack instead of one
// slot, so a BLOCK nested inside another's span is grouped as a sibling, not
// rejected — both files now parse. A test asserting they fail is simply
// wrong now, regardless of what the RENDERS/UNSUPPORTED tables above still
// say; that plan's Task 3 moves both files into RENDERS and rewrites this
// module's doc comment and census tables to match. Left as a removal rather
// than a rewrite here because Task 1's own scope is read_items, not this
// file's bookkeeping.

/// The record-level evidence behind the module doc's claim that `SELEXOL`
/// and `BLIVET` are stopped by nested `BLOCK` definitions, not by anything
/// left over from `POINT`/`TRACE`/`REPEAT`: `read_entities` — the flat walk
/// that decodes every entity-shaped record but does no `BLOCK`/`ENDBLK`
/// nesting validation at all (unlike `read_items`, which `parse` uses) —
/// succeeds on both files' real bytes. A regression in any of Task 8's new
/// record types would fail *this* test even though the two files never reach
/// `RENDERS`.
#[test]
fn both_still_unsupported_files_decode_cleanly_at_the_flat_record_level() {
    let mut ran = 0;
    // (file, expected flat entity count including block-interior ones —
    // BLOCK/ENDBLK themselves are walked over and contribute nothing, per
    // read_entities's own doc).
    for (name, expected_count) in [("SELEXOL", 147usize), ("BLIVET", 131usize)] {
        let Some(bytes) = corpus(name) else { return };
        let (_, meta) = acad_dwg::header::parse_header(&bytes)
            .unwrap_or_else(|e| panic!("{name}: expected the header to parse, got {e}"));
        let entities = acad_dwg::entity::read_entities(&bytes, &meta).unwrap_or_else(|e| {
            panic!(
                "{name}: expected the flat record walk to succeed (only BLOCK nesting \
                 is unsupported, not the record types themselves), got {e}"
            )
        });
        assert_eq!(entities.len(), expected_count, "{name}: flat entity count");
        ran += 1;
    }
    assert_eq!(ran, UNSUPPORTED.len());
}
