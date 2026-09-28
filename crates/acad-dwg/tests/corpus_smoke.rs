//! Every `AC1.2` drawing in the corpus, run end-to-end through `parse`,
//! `flatten` and `rasterize` — the same pipeline `acad-app` runs and
//! `crates/acad-render/tests/raster.rs` already checks for `SUBDIV` alone
//! (spec §8, milestone ④). This is milestone ④'s headline claim widened from
//! one drawing to the whole corpus: **all 16 render**, `RENDERS.len() == 16`
//! and `UNSUPPORTED` is empty (see the census test below) — and it is
//! written to be honest about exactly what that claim does and does not
//! establish.
//!
//! Task 8 left 14 of the 16 rendering; `SELEXOL` and `BLIVET` failed, but
//! not on a record type — `POINT`, `TRACE` and (for `BLIVET`) `REPEAT` all
//! decoded correctly even then. What stopped them was structural: both
//! files contain a `BLOCK` definition nested inside another `BLOCK`
//! definition (`SELEXOL`'s `"HEAD"` inside `"PACKTWR"`, and independently
//! `"ARROW"` inside `"COOLER"`; `BLIVET`'s `"$BCIRC"` inside a block named
//! `"BLIVET"`), which `read_items`'s single-slot `Option` couldn't
//! represent — opening a second `BLOCK` while one was already open raised
//! `DwgError::NestedBlock`, even though both files are perfectly
//! well-formed (every `BLOCK` does have a matching `ENDBLK`).
//!
//! The nested-block-definitions plan
//! (`docs/superpowers/plans/2026-09-28-nested-block-definitions.md`) closed this in
//! two independent steps, neither of which touched `acad_model`:
//!
//! - **Read** (`read_items`, `crates/acad-dwg/src/entity.rs`): a `BLOCK`
//!   opened while another is already open defines a **sibling**, not a
//!   child — evidenced by `SELEXOL`, which defines `ARROW` inside
//!   `COOLER`'s own span and then `INSERT`s `ARROW` fourteen times at top
//!   level, all outside `COOLER`'s span, so it cannot be scoped to
//!   `COOLER`. Open `BLOCK`s are now tracked as a stack instead of one
//!   `Option`; each `ENDBLK` pops the innermost and emits it where its own
//!   closing record sits, so an inner block that closes first appears
//!   earlier in `items` than the outer block it was nested inside.
//!   `DwgError::NestedBlock` no longer exists — there is nothing left that
//!   raises it. `crates/acad-dwg/tests/entity_corpus.rs`'s
//!   `selexol_and_blivet_group_their_nested_blocks` checks both files'
//!   exact block ordering and each block's own entity count directly, so a
//!   regression that mixed an inner block's entities into its outer one
//!   would be caught there, not just here.
//! - **Render** (`flatten`, `crates/acad-render/src/flatten.rs`): grouping
//!   the blocks correctly is not the same as drawing them — an `INSERT`
//!   found *inside* a block's own body (both files now have one) still had
//!   to be expanded, recursively, composing each level's translate/scale/
//!   rotate transform with the one enclosing it. A depth cap
//!   (`MAX_INSERT_DEPTH`, ours, not a recovered 1983 constant — the corpus
//!   never nests past depth 2) stops a self-referencing `INSERT` from
//!   overflowing the stack; reaching it drops that sub-tree's geometry
//!   rather than erroring, since `flatten` has no error channel.
//!
//! `SUBDIV` is still the only drawing with a DXF sibling
//! (`parallel_corpus.rs`, `entity_corpus.rs`), so `LINE`, `CIRCLE`, `ARC`,
//! `TEXT`, `BLOCK`, `ENDBLK` and `INSERT` remain the only types **verified**
//! byte-for-byte against it. `POINT`, `TRACE`, `SOLID` and `REPEAT`/`ENDREP`
//! remain **inferred** — record size and whole-file-walk consistency only,
//! cross-checked across two independent files apiece where the corpus
//! allowed it (see the module doc on `crates/acad-dwg/src/entity.rs` for
//! exactly what evidence backs each). This plan added no oracle: `SELEXOL`
//! and `BLIVET` still have no DXF sibling, so their now-successful render is
//! evidence that the record layouts — and the sibling-block/recursive-
//! `INSERT` fix — generalise across the corpus, not a second, independent
//! verification the way a DXF comparison would be. The original 11
//! `RENDERS` drawings use only the seven verified types; their success is
//! evidence of generalisation rather than a second oracle check.

use acad_render::{flatten, rasterize, Viewport};

const CANVAS: u32 = 800;

/// A drawn-content floor for `known_good_drawings_parse_and_render_non_blank`.
/// Comfortably below the smallest of the 16 drawings' real pixel counts
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

/// All 16 `AC1.2` drawings in the corpus: the 14 that used only record types
/// already verified or inferred by Task 8 (`ADDER`, `FLOOR` and `FLOW` are
/// that task's own fixes — erasure, `REPEAT`, `SOLID` respectively, see the
/// module doc), plus `SELEXOL` and `BLIVET`, which needed the
/// nested-block-definitions plan's sibling-block read fix and recursive-
/// `INSERT` render fix (also module doc) before they would parse and draw
/// at all.
const RENDERS: &[&str] = &[
    "BOX", "ORGATE", "DLATCH", "SUBDIV", "ANDGATE", "NORGATE", "XORGATE", "HALFADD", "INVERTER",
    "NANDGATE", "XNORGATE", "ADDER", "FLOOR", "FLOW", "SELEXOL", "BLIVET",
];

/// Empty: every corpus drawing now parses and renders above `MIN_LIT`.
const UNSUPPORTED: &[&str] = &[];

#[test]
fn the_census_accounts_for_all_sixteen_ac12_drawings() {
    assert_eq!(RENDERS.len() + UNSUPPORTED.len(), 16);
    assert_eq!(RENDERS.len(), 16, "all 16 must be in the rendering list");
    assert_eq!(UNSUPPORTED.len(), 0, "no AC1.2 drawing remains unsupported");
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
        // The smallest of the 16 (ORGATE) draws 2,466 lit pixels on this
        // 800x800 canvas; MIN_LIT sits well under that so a real regression —
        // a viewport bug leaving geometry off-canvas, flatten dropping
        // everything — trips it, while never being so close to the real
        // values that anti-aliasing noise could.
        assert!(
            lit > MIN_LIT,
            "{name}: parsed but rendered near-blank ({lit} lit pixels, need > {MIN_LIT})"
        );
        if matches!(*name, "SELEXOL" | "BLIVET") {
            eprintln!("{name}: {lit} lit pixels");
        }
        ran += 1;
    }
    assert_eq!(ran, RENDERS.len());
}
