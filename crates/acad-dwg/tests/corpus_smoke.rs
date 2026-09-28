//! Every `AC1.2` drawing in the corpus, run end-to-end through `parse`,
//! `flatten` and `rasterize` — the same pipeline `acad-app` runs and
//! `crates/acad-render/tests/raster.rs` already checks for `SUBDIV` alone
//! (spec §8, milestone ④). This is milestone ④'s headline claim widened from
//! one drawing to the whole corpus, and it is written to be honest about
//! where that claim currently stops.
//!
//! Of the 16, 11 parse and render non-blank: `BOX`, `ORGATE`, `DLATCH`,
//! `SUBDIV`, `ANDGATE`, `NORGATE`, `XORGATE`, `HALFADD`, `INVERTER`,
//! `NANDGATE`, `XNORGATE`. `SUBDIV` is the only one with a DXF sibling
//! (`parallel_corpus.rs`, `entity_corpus.rs`), so `LINE`, `CIRCLE`, `ARC`,
//! `TEXT`, `BLOCK`, `ENDBLK` and `INSERT` are verified against it; the other
//! 10 drawings use only that same, already-verified set of record types (no
//! new layout is exercised), so decoding them correctly is evidence the
//! layout generalises, not a second independent verification.
//!
//! The remaining 5 fail with `DwgError::UnknownEntityType`, naming a type
//! code this codec does not model yet. Four are real 1-based indices into
//! `ACAD.EXE`'s 14-entry entity table (`crates/acad-dwg/src/entity.rs`'s
//! module doc comment, spec §4.2) that `acad_model::Entity` has no variant
//! for: `POINT` (2, `SELEXOL`), `REPEAT`
//! (5, `FLOOR`), `TRACE` (9, `BLIVET`), `SOLID` (11, `FLOW`). Implementing
//! them is Task 8's job, which also needs an `acad-model` change this task
//! is out of scope for. `ADDER`'s failing code, 65535 (`0xFFFF`), is *not*
//! one of the 14 — it doesn't index the table at all, so it is more likely a
//! desync earlier in the walk (or an unidentified marker, e.g. a
//! deleted-entity sentinel — old AutoCAD kept erased entities in place
//! rather than removing them) than a plain missing type; that distinction is
//! left for whoever picks it up next; this test simply records what happens
//! today.
//!
//! This file lists both sets by name, the same discipline
//! `corpus/manifest.toml` uses to exclude corrupt files by name rather than
//! silently — a green test here must never be read as "all 16 render",
//! because five of them do not.

use acad_dwg::DwgError;
use acad_render::{flatten, rasterize, Viewport};

const CANVAS: u32 = 800;

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

/// The 11 `AC1.2` drawings that use only the record types verified against
/// `SUBDIV.DXF` — `LINE`, `CIRCLE`, `ARC`, `TEXT`, `BLOCK`, `ENDBLK`,
/// `INSERT` — and so parse and render today.
const RENDERS: &[&str] = &[
    "BOX", "ORGATE", "DLATCH", "SUBDIV", "ANDGATE", "NORGATE", "XORGATE", "HALFADD", "INVERTER",
    "NANDGATE", "XNORGATE",
];

/// The 5 drawings that fail today, and exactly how: the `DwgError` they
/// raise, the entity-table entry that code names (per spec — `None` for
/// `ADDER`'s 65535, which names no entry), and the offset the bad record
/// header starts at. Any change to these — a fix, a different failure, a
/// different offset — is real news about the codec and must change this
/// table, not be masked by a looser assertion.
const UNSUPPORTED: &[(&str, u16, usize, Option<&str>)] = &[
    ("SELEXOL", 2, 0x1f5, Some("POINT")),
    ("FLOOR", 5, 0x16c9, Some("REPEAT")),
    ("BLIVET", 9, 0x1d8, Some("TRACE")),
    ("FLOW", 11, 0x6da, Some("SOLID")),
    ("ADDER", 65535, 0x7de, None),
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
        let lit = pm.pixels().iter().filter(|p| p.alpha() > 0).count();
        assert!(
            lit > 0,
            "{name}: parsed but rendered blank ({lit} lit pixels)"
        );
        ran += 1;
    }
    assert_eq!(ran, RENDERS.len());
}

#[test]
fn known_unsupported_drawings_fail_with_the_recorded_type_code() {
    let mut ran = 0;
    for &(name, code, at, entity_table_entry) in UNSUPPORTED {
        let Some(bytes) = corpus(name) else { return };
        match acad_dwg::parse(&bytes) {
            Err(DwgError::UnknownEntityType {
                code: got_code,
                at: got_at,
            }) => {
                assert_eq!(got_code, code, "{name}: unexpected type code");
                assert_eq!(got_at, at, "{name}: unexpected offset");
            }
            Err(other) => panic!(
                "{name}: expected UnknownEntityType {{ code: {code}, at: {at:#x} }}, got {other}"
            ),
            Ok(_) => panic!(
                "{name}: now parses — this is progress! Move it into RENDERS \
                 (spec §4.2's entity {code} is {entity_table_entry:?}) rather \
                 than leaving it here unexercised."
            ),
        }
        ran += 1;
    }
    assert_eq!(ran, UNSUPPORTED.len());
}
