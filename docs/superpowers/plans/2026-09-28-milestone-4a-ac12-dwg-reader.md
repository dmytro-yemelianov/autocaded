# Milestone ④ (early half) — `AC1.2` DWG Reader — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Read `AC1.2` DWG files into `acad-model`, proven by `SUBDIV.DWG` decoding to the same `Drawing` that `SUBDIV.DXF` decodes to.

**Architecture:** A new `acad-dwg` crate beside `acad-dxf`, same shape: a lexer-equivalent that walks fixed records, a parser that builds a `Drawing`, a typed error. The format is undocumented, so the plan is measurement-first: Task 2 builds a *tested* discovery harness that locates each known DXF entity's field values inside the DWG bytes and prints the byte layout it finds. Every later task reverses one record type using that harness and pins the result with the parallel corpus as oracle. No expectations are invented — §9 forbids it, and the parallel corpus is what lets us obey that without the emulator.

**Tech Stack:** Rust 1.88.0. No new dependencies: `acad-dwg` needs only `acad-model`, and the tests need `acad-dxf`.

**Spec:** `docs/superpowers/specs/2026-09-28-autocad-14-rust-design.md`

## Global Constraints

- Rust edition 2021, toolchain 1.88.0 (pinned by `rust-toolchain.toml`).
- `acad-dwg` is a **shipped** crate (spec §5), unlike `acad-re`. It may depend only on `acad-model`. It must not depend on `acad-dxf` — the DXF codec is a *test* oracle, so that dependency belongs in `[dev-dependencies]` only.
- 1983 policy lives in the codec, never in `acad-model` (spec §5). If a DWG field has no home in `acad-model`, say so in the ledger rather than widening `acad-model` to fit 1983.
- The corpus is gitignored; tests that need it **skip with a message**, exactly as `crates/acad-dxf/tests/roundtrip.rs` does. A fresh checkout stays green.
- `cargo fmt --all --check` and `cargo clippy --workspace --all-targets -- -D warnings` stay green; CI runs both.
- TDD: every step writes the failing test before the implementation.
- **Scope is the read direction of `AC1.2` only.** Writing DWG, the `AC1.40` version, and verification against oracle-generated drawings are all after milestone ③ (spec §8, sequencing amendment). If a task tempts you into the writer, stop and ledger it.

### Verified facts this plan builds on (spec §4.2, established 2026-09-28)

| Fact | Value |
|---|---|
| Version census | 16 `AC1.2` drawings, 5 `AC1.40` (`HOUSE`, `COLORS`, `OFFICE`, `SHUTTLE`, `DISC`); `.BAK` copies match their drawings |
| `SUBDIV.DWG` | `AC1.2`, 6,656 B on disk (`0x1A00`), sha256 `85fbd37d…` — check `corpus/manifest.toml` |
| Magic | ASCII at `+0`, `"AC1.2\0"` / `"AC1.40\0"`, then zeros to `+0x24` |
| `u32` at `+0x24` | `0x19E5` for `SUBDIV` |
| `u32` at `+0x28` | `171` — the entity record count. `SUBDIV.DXF` holds 133 `LINE` + 8 `ARC` + 8 `TEXT` + 8 `INSERT` + 2 `CIRCLE` = 159 entities, plus 6 `BLOCK` and 6 `ENDBLK` = **171**. That arithmetic is the whole reason to believe this field |
| Header doubles | `EXTENTS` min/max as 3D points from `+0x2A`; `LIMITS` min/max as 2D pairs from `+0x5A`; `DWGVIEW` centre + height from `+0x7A`; `MODERES`/`MODEGRID` as `u16`+`f64` pairs from `+0x9A`; `TXTSIZE` at `+0xB4`; `TRACEWID` at `+0xBC`. All verified against `SUBDIV.DXF`'s own header values |
| Entity start | Fixed `+0x1D8` in every `AC1.2` file, running to the `+0x24` offset |
| Entity order | **Same as the DXF.** `SUBDIV.DXF`'s entity 0 is an `ARC` whose first coordinate sits at `0x1DC`; offsets increase monotonically with DXF index, and entity 158 sits at `0x19BD`, just before `entity_end` `0x19E5` |
| Record shape | `u16` type code, `u16` of unknown meaning, then the type's fields as doubles. `LINE` is 36 bytes, `ARC` 44 |
| Type codes | 1-based index into `ACAD.EXE`'s entity table (spec §4.2): 1 `LINE`, 2 `POINT`, 3 `CIRCLE`, 4 `SHAPE`, 5 `REPEAT`, 6 `ENDREP`, 7 `TEXT`, 8 `ARC`, 9 `TRACE`, 10 `LOAD`, 11 `SOLID`, 12 `BLOCK`, 13 `ENDBLK`, 14 `INSERT` |
| Alignment | Records are **not** 8-byte aligned. Do not assume a stride across differing types |

`SUBDIV.DXF`'s header, for reference — this is the oracle for Task 1:

```
EXTENTS,1
-1.750000,19.000000,-1.750000,14.000000
LIMITS,1
-2.000000,19.000000,-2.000000,14.000000
BASE,1
0.000000,0.000000
DWGVIEW,1
8.500000,6.163380,16.326761
MODERES,1
0,0.250000
MODEGRID,1
0,0.250000
MODEORTHO,1
0
MODEFILL,1
1
TXTSIZE,1
0.200000
TRACEWID,1
0.050000
```

## Review Focus

1. **A file whose magic is `AC1.40`** — this codec reads `AC1.2`. An `AC1.40` file must be a named error saying which version was found and which is supported, never a misparse. Five corpus drawings are `AC1.40`, so a user *will* hit this. *(Task 1)*
2. **A truncated DWG** — a file shorter than the header, or whose entity count promises more records than the bytes hold, must be a named error carrying the offset, never a panic and never a half-built `Drawing`. `SHUTTLE.DXF` shows a bad floppy read is a real event on this corpus. *(Tasks 1, 3)*
3. **An entity record naming a type this codec does not know** — must name the type code and the byte offset, never be skipped silently. A silently dropped entity renders as a drawing that is quietly *wrong*, which is worse than one that fails to open. *(Task 3)*
4. **Document order** — `SUBDIV.DWG`'s records run in the **same** order as `SUBDIV.DXF`'s, and milestone ① established that document order is content, so a reader must preserve file order rather than bucket by kind. *(Task 6)* — *(Revised during execution: this line originally claimed the DWG order was reversed. It is not; see the Task 2 finding recorded in spec §4.2.)*
5. **A `Drawing` that compares equal only because both sides are empty** — every equality test against the DXF must first assert the drawing is non-trivial, or a reader that returns nothing passes the milestone. *(Task 6)*

---

### Task 1: `acad-dwg` crate and the `AC1.2` header codec

**Files:**
- Create: `crates/acad-dwg/Cargo.toml`
- Create: `crates/acad-dwg/src/lib.rs`
- Create: `crates/acad-dwg/src/error.rs`
- Create: `crates/acad-dwg/src/header.rs`
- Create: `crates/acad-dwg/tests/header_corpus.rs`
- Modify: `Cargo.toml` (workspace members and `[workspace.dependencies]`)

**Interfaces:**
- Consumes: `acad_model::{Extents, Point}`, `acad_model::header::{Header, DwgView, Mode}`. Note the real shapes: `Extents { xmin, xmax, ymin, ymax }` (not min_x/max_x), `DwgView { center, height }`, `Mode { on, spacing }`, and `Header` has **no `Default`** — every field must be given a value.
- Produces:
  - `acad_dwg::DwgError`
  - `acad_dwg::header::{Version, parse_header}`
  - `Version::{Ac12, Ac140}` with `Version::detect(bytes: &[u8]) -> Result<Version, DwgError>`
  - `parse_header(bytes: &[u8]) -> Result<(acad_model::Header, HeaderMeta), DwgError>`
  - `HeaderMeta { entity_count: u32, entity_end: u32 }`

- [ ] **Step 1: Create the crate and register it**

`crates/acad-dwg/Cargo.toml`:

```toml
[package]
name = "acad-dwg"
version = "0.1.0"
edition.workspace = true
rust-version.workspace = true

[dependencies]
acad-model.workspace = true

# acad-dxf is the test oracle only (spec §4.4). A shipped codec must not
# depend on another codec.
[dev-dependencies]
acad-dxf.workspace = true
```

In the workspace `Cargo.toml`, add `"crates/acad-dwg"` to `members`, and add
`acad-dwg = { path = "crates/acad-dwg" }` to `[workspace.dependencies]` beside the
existing entries — unlike `acad-re`, this one *is* meant to be depended on.

- [ ] **Step 2: Write the failing header tests**

`crates/acad-dwg/src/header.rs`, tests module only for now. The expected values come
from `SUBDIV.DXF`'s header, quoted above — not from reading the DWG and writing down
what it says.

```rust
#[cfg(test)]
mod tests {
    use super::*;

    /// A synthetic AC1.2 header: magic, the two u32s, then doubles at the
    /// offsets the spec records.
    fn header_bytes() -> Vec<u8> {
        let mut h = vec![0u8; 0x100];
        h[..6].copy_from_slice(b"AC1.2\0");
        h[0x24..0x28].copy_from_slice(&0x19e5u32.to_le_bytes());
        h[0x28..0x2c].copy_from_slice(&171u32.to_le_bytes());
        let put = |h: &mut Vec<u8>, at: usize, v: f64| {
            h[at..at + 8].copy_from_slice(&v.to_le_bytes());
        };
        // EXTENTS min (x, y, z) then max (x, y, z)
        put(&mut h, 0x2a, -1.75);
        put(&mut h, 0x32, -1.75);
        put(&mut h, 0x3a, 0.0);
        put(&mut h, 0x42, 19.0);
        put(&mut h, 0x4a, 14.0);
        put(&mut h, 0x52, 0.0);
        // LIMITS min (x, y) then max (x, y)
        put(&mut h, 0x5a, -2.0);
        put(&mut h, 0x62, -2.0);
        put(&mut h, 0x6a, 19.0);
        put(&mut h, 0x72, 14.0);
        h
    }

    #[test]
    fn detects_the_two_versions_by_magic() {
        assert_eq!(Version::detect(b"AC1.2\0rest").unwrap(), Version::Ac12);
        assert_eq!(Version::detect(b"AC1.40\0rest").unwrap(), Version::Ac140);
    }

    #[test]
    fn an_unknown_magic_is_a_named_error() {
        assert_eq!(
            Version::detect(b"AC1.50\0junk").unwrap_err(),
            DwgError::UnknownVersion { found: *b"AC1.50\0\0" }
        );
    }

    #[test]
    fn ac140_is_rejected_with_a_message_naming_both_versions() {
        // Review Focus 1: five corpus drawings are AC1.40 and this codec reads
        // AC1.2. Opening one must say so, not misparse.
        let mut b = header_bytes();
        b[..7].copy_from_slice(b"AC1.40\0");
        assert_eq!(
            parse_header(&b).unwrap_err(),
            DwgError::UnsupportedVersion {
                found: Version::Ac140,
                supported: Version::Ac12
            }
        );
    }

    #[test]
    fn reads_the_entity_count_and_end_offset() {
        let (_, meta) = parse_header(&header_bytes()).unwrap();
        assert_eq!(meta.entity_count, 171);
        assert_eq!(meta.entity_end, 0x19e5);
    }

    #[test]
    fn reads_extents_and_limits_at_the_recorded_offsets() {
        let (header, _) = parse_header(&header_bytes()).unwrap();
        assert_eq!(header.extents, Extents { xmin: -1.75, xmax: 19.0, ymin: -1.75, ymax: 14.0 });
        assert_eq!(header.limits, Extents { xmin: -2.0, xmax: 19.0, ymin: -2.0, ymax: 14.0 });
    }

    #[test]
    fn reads_the_remaining_scalar_header_fields() {
        // DWGVIEW 8.5, 6.163380, 16.326761; MODERES and MODEGRID 0.25;
        // TXTSIZE 0.2; TRACEWID 0.05 — all from SUBDIV.DXF's header.
        let mut h = header_bytes();
        let put = |h: &mut Vec<u8>, at: usize, v: f64| {
            h[at..at + 8].copy_from_slice(&v.to_le_bytes());
        };
        put(&mut h, 0x7a, 8.5);
        put(&mut h, 0x82, 6.163380);
        put(&mut h, 0x92, 16.326761);
        h[0x9a..0x9c].copy_from_slice(&0u16.to_le_bytes());
        put(&mut h, 0x9c, 0.25);
        h[0xa4..0xa6].copy_from_slice(&0u16.to_le_bytes());
        put(&mut h, 0xa6, 0.25);
        put(&mut h, 0xb4, 0.2);
        put(&mut h, 0xbc, 0.05);

        let (header, _) = parse_header(&h).unwrap();
        assert_eq!(header.view.center, Point { x: 8.5, y: 6.163380 });
        assert_eq!(header.view.height, 16.326761);
        assert_eq!(header.snap, Mode { on: false, spacing: 0.25 });
        assert_eq!(header.grid, Mode { on: false, spacing: 0.25 });
        assert_eq!(header.text_size, 0.2);
        assert_eq!(header.trace_width, 0.05);
    }

    #[test]
    fn a_file_shorter_than_the_header_is_an_error_not_a_panic() {
        // Review Focus 2.
        let short = header_bytes()[..0x40].to_vec();
        assert_eq!(
            parse_header(&short).unwrap_err(),
            DwgError::ShortHeader { len: 0x40, need: 0xc4 }
        );
    }
}
```

These field names are the real ones, checked against
`crates/acad-model/src/header.rs` and `geom.rs` while this plan was written. Read both
anyway before you start — if they have moved since, the compiler will say so, and the
assertions are what matter, not the spelling.

- [ ] **Step 3: Run and confirm they fail**

Run: `cargo test -p acad-dwg`
Expected: FAIL — `cannot find type `Version``, `cannot find function `parse_header``.

- [ ] **Step 4: Implement the error type**

`crates/acad-dwg/src/error.rs`:

```rust
use std::fmt;

use crate::header::Version;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DwgError {
    /// The first bytes are neither `AC1.2` nor `AC1.40`.
    UnknownVersion { found: [u8; 8] },
    /// A version this codec knows of but cannot read yet.
    UnsupportedVersion { found: Version, supported: Version },
    /// The file is shorter than the fixed header.
    ShortHeader { len: usize, need: usize },
    /// An entity record runs past the end of the file.
    TruncatedEntity { index: u32, at: usize, len: usize },
    /// An entity type code this codec does not know.
    UnknownEntityType { code: u16, at: usize },
    /// The header promises more entities than the bytes hold.
    EntityCountMismatch { want: u32, got: u32 },
}

impl fmt::Display for DwgError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnknownVersion { found } => write!(
                f,
                "not a DWG: magic is {:?}, expected \"AC1.2\" or \"AC1.40\"",
                String::from_utf8_lossy(found)
            ),
            Self::UnsupportedVersion { found, supported } => write!(
                f,
                "this is a {found} drawing; only {supported} can be read so far"
            ),
            Self::ShortHeader { len, need } => {
                write!(f, "file is {len} bytes: the header alone needs {need}")
            }
            Self::TruncatedEntity { index, at, len } => write!(
                f,
                "entity {index} at offset {at:#x} runs past end of file {len:#x}"
            ),
            Self::UnknownEntityType { code, at } => {
                write!(f, "unknown entity type {code:#x} at offset {at:#x}")
            }
            Self::EntityCountMismatch { want, got } => {
                write!(f, "header promises {want} entities, found {got}")
            }
        }
    }
}

impl std::error::Error for DwgError {}
```

- [ ] **Step 5: Implement the header codec**

`crates/acad-dwg/src/header.rs`, above the tests module. Offsets are named constants
because they are recovered facts, not arbitrary numbers, and a reader should be able
to check each against the spec table.

```rust
use crate::DwgError;
use acad_model::{
    header::{DwgView, Header, Mode},
    Extents, Point,
};
use std::collections::BTreeMap;
use std::fmt;

/// Header field offsets, established against `SUBDIV.DXF` (spec §4.2).
const OFF_ENTITY_END: usize = 0x24;
const OFF_ENTITY_COUNT: usize = 0x28;
const OFF_EXTENTS: usize = 0x2a;
const OFF_LIMITS: usize = 0x5a;
const OFF_VIEW: usize = 0x7a;
const OFF_VIEW_HEIGHT: usize = 0x92;
const OFF_SNAP_FLAG: usize = 0x9a;
const OFF_SNAP: usize = 0x9c;
const OFF_GRID_FLAG: usize = 0xa4;
const OFF_GRID: usize = 0xa6;
const OFF_ORTHO: usize = 0xae;
const OFF_FILL: usize = 0xb0;
const OFF_TXTSIZE: usize = 0xb4;
const OFF_TRACEWID: usize = 0xbc;
/// The last field this codec reads, plus its width.
const HEADER_MIN: usize = OFF_TRACEWID + 8;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Version {
    Ac12,
    Ac140,
}

impl fmt::Display for Version {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Ac12 => "AC1.2",
            Self::Ac140 => "AC1.40",
        })
    }
}

impl Version {
    pub fn detect(bytes: &[u8]) -> Result<Version, DwgError> {
        if bytes.starts_with(b"AC1.40") {
            return Ok(Version::Ac140);
        }
        if bytes.starts_with(b"AC1.2") {
            return Ok(Version::Ac12);
        }
        let mut found = [0u8; 8];
        let n = bytes.len().min(8);
        found[..n].copy_from_slice(&bytes[..n]);
        Err(DwgError::UnknownVersion { found })
    }
}

/// What the header says about the entity region, which the entity reader needs
/// and `acad_model::Header` has no place for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HeaderMeta {
    pub entity_count: u32,
    pub entity_end: u32,
}

fn f64_at(b: &[u8], at: usize) -> f64 {
    f64::from_le_bytes(b[at..at + 8].try_into().unwrap())
}

fn u32_at(b: &[u8], at: usize) -> u32 {
    u32::from_le_bytes(b[at..at + 4].try_into().unwrap())
}

pub fn parse_header(bytes: &[u8]) -> Result<(Header, HeaderMeta), DwgError> {
    let version = Version::detect(bytes)?;
    if version != Version::Ac12 {
        return Err(DwgError::UnsupportedVersion {
            found: version,
            supported: Version::Ac12,
        });
    }
    if bytes.len() < HEADER_MIN {
        return Err(DwgError::ShortHeader {
            len: bytes.len(),
            need: HEADER_MIN,
        });
    }

    // EXTENTS is two 3D points — min (x, y, z) then max (x, y, z), so the
    // maxima are 24 and 32 bytes in, not 16 and 24. LIMITS is two 2D pairs.
    let extents = Extents {
        xmin: f64_at(bytes, OFF_EXTENTS),
        ymin: f64_at(bytes, OFF_EXTENTS + 8),
        xmax: f64_at(bytes, OFF_EXTENTS + 24),
        ymax: f64_at(bytes, OFF_EXTENTS + 32),
    };
    let limits = Extents {
        xmin: f64_at(bytes, OFF_LIMITS),
        ymin: f64_at(bytes, OFF_LIMITS + 8),
        xmax: f64_at(bytes, OFF_LIMITS + 16),
        ymax: f64_at(bytes, OFF_LIMITS + 24),
    };

    let header = Header {
        extents,
        limits,
        // BASE is 0,0 in SUBDIV, so its offset is not yet pinned — a field
        // whose only sample is zero cannot be located by searching for it.
        // Task 7 revisits this against a drawing with a non-zero BASE.
        base: Point { x: 0.0, y: 0.0 },
        view: DwgView {
            center: Point {
                x: f64_at(bytes, OFF_VIEW),
                y: f64_at(bytes, OFF_VIEW + 8),
            },
            height: f64_at(bytes, OFF_VIEW_HEIGHT),
        },
        snap: Mode {
            on: u16_at(bytes, OFF_SNAP_FLAG) != 0,
            spacing: f64_at(bytes, OFF_SNAP),
        },
        grid: Mode {
            on: u16_at(bytes, OFF_GRID_FLAG) != 0,
            spacing: f64_at(bytes, OFF_GRID),
        },
        ortho: u16_at(bytes, OFF_ORTHO) != 0,
        fill: u16_at(bytes, OFF_FILL) != 0,
        text_size: f64_at(bytes, OFF_TXTSIZE),
        trace_width: f64_at(bytes, OFF_TRACEWID),
        // The layer table follows the scalars and is reversed in Task 7, where
        // SUBDIV.DXF's LAYERC record is the oracle. Until then a drawing reads
        // as layer 0 with no table, which renders correctly because milestone
        // ① does not colour by layer yet.
        current_layer: 0,
        layers: BTreeMap::new(),
    };
    Ok((header, meta_of(bytes)))
}

fn meta_of(bytes: &[u8]) -> HeaderMeta {
    HeaderMeta {
        entity_count: u32_at(bytes, OFF_ENTITY_COUNT),
        entity_end: u32_at(bytes, OFF_ENTITY_END),
    }
}

fn u16_at(b: &[u8], at: usize) -> u16 {
    u16::from_le_bytes(b[at..at + 2].try_into().unwrap())
}
```

`Header` has no `Default`, so every field is given a value above. Two are placeholders
with stated reasons — `base`, whose only sample is `0,0` and so cannot be located by
searching, and the layer table — and both are Task 7's work. Do not let them stay
silent: the corpus test below compares only the fields this task claims.

`crates/acad-dwg/src/lib.rs`:

```rust
//! The 1983 DWG binary codec. `AC1.2` read support; `AC1.40` and the write
//! direction come after milestone ③ (spec §8).
pub mod error;
pub mod header;

pub use error::DwgError;
```

- [ ] **Step 6: Run and confirm green**

Run: `cargo test -p acad-dwg`
Expected: PASS, 6 tests.

- [ ] **Step 7: Write the corpus header test**

`crates/acad-dwg/tests/header_corpus.rs`. This is where the parallel corpus first
earns its keep: the DWG header must agree with the DXF header, field for field, with
no numbers written down by hand.

```rust
use acad_dwg::header::{parse_header, Version};

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
fn the_dwg_header_agrees_with_the_dxf_header() {
    let (Some(dwg), Some(dxf)) = (
        corpus("Samples/SUBDIV.DWG"),
        corpus("Samples/SUBDIV.DXF"),
    ) else {
        return;
    };
    let (from_dwg, _) = parse_header(&dwg).unwrap();
    let from_dxf = acad_dxf::parse(&dxf).unwrap().header;
    assert_eq!(from_dwg.extents, from_dxf.extents, "EXTENTS");
    assert_eq!(from_dwg.limits, from_dxf.limits, "LIMITS");
    assert_eq!(from_dwg.view, from_dxf.view, "DWGVIEW");
    assert_eq!(from_dwg.snap, from_dxf.snap, "MODERES");
    assert_eq!(from_dwg.grid, from_dxf.grid, "MODEGRID");
    assert_eq!(from_dwg.text_size, from_dxf.text_size, "TXTSIZE");
    assert_eq!(from_dwg.trace_width, from_dxf.trace_width, "TRACEWID");
    // `base`, `current_layer` and `layers` are Task 7's, and are deliberately
    // not compared here — see parse_header.
}

#[test]
fn the_entity_count_matches_what_the_dxf_holds() {
    // 171 = 159 entities + 6 BLOCK + 6 ENDBLK. Deriving it from the DXF rather
    // than hardcoding 171 is what makes this a check and not a restatement.
    let (Some(dwg), Some(dxf)) = (
        corpus("Samples/SUBDIV.DWG"),
        corpus("Samples/SUBDIV.DXF"),
    ) else {
        return;
    };
    let (_, meta) = parse_header(&dwg).unwrap();
    let drawing = acad_dxf::parse(&dxf).unwrap();
    // A Drawing is a flat Vec<Item> preserving document order; a Block counts
    // as its own entities plus the BLOCK and ENDBLK delimiters.
    let from_dxf = drawing.entities().count()
        + drawing.blocks().map(|b| b.entities.len() + 2).sum::<usize>();
    assert_eq!(meta.entity_count as usize, from_dxf);
}

#[test]
fn every_ac12_drawing_in_the_corpus_has_a_readable_header() {
    let mut seen = 0;
    for name in [
        "BOX", "FLOW", "ADDER", "FLOOR", "ORGATE", "DLATCH", "SUBDIV", "BLIVET",
        "ANDGATE", "NORGATE", "XORGATE", "HALFADD", "SELEXOL", "INVERTER",
        "NANDGATE", "XNORGATE",
    ] {
        let Some(bytes) = corpus(&format!("Samples/{name}.DWG")) else {
            return;
        };
        assert_eq!(Version::detect(&bytes).unwrap(), Version::Ac12, "{name}");
        parse_header(&bytes).unwrap_or_else(|e| panic!("{name}: {e}"));
        seen += 1;
    }
    assert_eq!(seen, 16, "the census says 16 AC1.2 drawings");
}

#[test]
fn an_ac140_drawing_is_refused_by_name() {
    // Review Focus 1, against a real file rather than a synthetic one.
    let Some(bytes) = corpus("Samples/HOUSE.DWG") else {
        return;
    };
    assert_eq!(acad_dwg::header::Version::detect(&bytes).unwrap(), Version::Ac140);
    let msg = parse_header(&bytes).unwrap_err().to_string();
    assert!(msg.contains("AC1.40") && msg.contains("AC1.2"), "message was: {msg}");
}
```

`Drawing` holds `items: Vec<Item>` where `Item` is `Entity(Entity)` or `Block(Block)`,
with `entities()` and `blocks()` as iterator **methods** — there are no `entities` or
`blocks` fields. That flat, ordered list is deliberate: milestone ① found that block
definitions and loose entities interleave, so document order is content.

- [ ] **Step 8: Run the corpus tests**

Run: `cargo test -p acad-dwg --test header_corpus -- --nocapture`
Expected: PASS with the corpus present. If `the_dwg_header_agrees_with_the_dxf_header`
fails, the offsets are wrong — re-derive them by scanning the DWG for the DXF's header
values, as spec §4.2 records, rather than adjusting the assertion.

Then confirm the skip path:

```bash
mv corpus/Samples/SUBDIV.DWG /tmp/ && cargo test -p acad-dwg -- --nocapture; mv /tmp/SUBDIV.DWG corpus/Samples/
```

- [ ] **Step 9: Commit**

```bash
cargo fmt --all
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
git add Cargo.toml Cargo.lock crates/acad-dwg
git commit -m "feat(dwg): acad-dwg crate and the AC1.2 header codec"
```

---

### Task 2: The record discovery harness

**Files:**
- Create: `crates/acad-dwg/src/discover.rs`
- Create: `crates/acad-dwg/src/bin/dwg-discover.rs`
- Modify: `crates/acad-dwg/src/lib.rs`

**Interfaces:**
- Consumes: `parse_header`, `HeaderMeta` from Task 1; `acad_dxf::parse` in the binary.
- Produces:
  - `acad_dwg::discover::{find_f64, FieldHit}`
  - `find_f64(bytes: &[u8], needle: f64, tolerance: f64) -> Vec<FieldHit>`
  - `FieldHit { offset: usize, value: f64 }`
  - binary `dwg-discover`, which prints where each DXF entity's coordinates live in the DWG

This is the tool every later task uses to reverse one record type. It is production
code in a shipped crate rather than a throwaway script because it is the method, and a
method that is not tested is a method you cannot trust the third time you use it.

- [ ] **Step 1: Write the failing search tests**

`crates/acad-dwg/src/discover.rs`, tests module only:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    fn bytes_with(values: &[(usize, f64)], len: usize) -> Vec<u8> {
        let mut b = vec![0u8; len];
        for (at, v) in values {
            b[*at..*at + 8].copy_from_slice(&v.to_le_bytes());
        }
        b
    }

    #[test]
    fn finds_a_double_at_an_unaligned_offset() {
        // The format is not 8-byte aligned — a clean 1.0 sits at 0x19CD in
        // SUBDIV.DWG — so the search must step one byte at a time.
        let b = bytes_with(&[(0x0d, 6.822910)], 0x40);
        let hits = find_f64(&b, 6.822910, 1e-6);
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].offset, 0x0d);
    }

    #[test]
    fn finds_every_occurrence_not_just_the_first() {
        let b = bytes_with(&[(0x10, 2.5), (0x30, 2.5)], 0x40);
        let offsets: Vec<usize> = find_f64(&b, 2.5, 1e-6).iter().map(|h| h.offset).collect();
        assert_eq!(offsets, vec![0x10, 0x30]);
    }

    #[test]
    fn tolerance_accounts_for_the_dxf_printing_six_decimals() {
        // The DXF prints 1.012459; the DWG holds the full double. A search that
        // demanded equality would miss it, which is how the first probe of this
        // file lost the LINE's x1.
        let b = bytes_with(&[(0x08, 1.0124585)], 0x20);
        assert_eq!(find_f64(&b, 1.012459, 1e-6).len(), 1);
        assert_eq!(find_f64(&b, 1.012459, 1e-12).len(), 0);
    }

    #[test]
    fn zero_is_not_reported_because_padding_is_full_of_it() {
        // A file is mostly zero bytes, so every 8-byte window of padding reads
        // as 0.0. Reporting those would bury every real hit.
        let b = bytes_with(&[], 0x40);
        assert!(find_f64(&b, 0.0, 1e-6).is_empty());
    }

    #[test]
    fn a_needle_near_the_end_does_not_read_past_it() {
        let b = bytes_with(&[(0x18, 7.0)], 0x20);
        assert_eq!(find_f64(&b, 7.0, 1e-6).len(), 1);
        assert!(find_f64(&b, 9.0, 1e-6).is_empty());
    }
}
```

- [ ] **Step 2: Run and confirm they fail**

Run: `cargo test -p acad-dwg discover`
Expected: FAIL — `cannot find function `find_f64``.

- [ ] **Step 3: Implement the search**

```rust
//! Locating known values inside an undocumented binary.
//!
//! The DXF sibling of a DWG says exactly which coordinates the drawing holds.
//! Searching the DWG for those values shows where each field lives, which is
//! how every record layout in this crate was recovered.

/// One place a searched-for value appears.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FieldHit {
    pub offset: usize,
    pub value: f64,
}

/// Every offset at which `needle` appears as a little-endian `f64`.
///
/// Steps one byte at a time: the records are not 8-byte aligned. `0.0` is never
/// reported — a DWG is mostly zero padding, so it would bury every real hit.
pub fn find_f64(bytes: &[u8], needle: f64, tolerance: f64) -> Vec<FieldHit> {
    if needle == 0.0 {
        return Vec::new();
    }
    let mut hits = Vec::new();
    for offset in 0..bytes.len().saturating_sub(7) {
        let value = f64::from_le_bytes(bytes[offset..offset + 8].try_into().unwrap());
        if (value - needle).abs() <= tolerance {
            hits.push(FieldHit { offset, value });
        }
    }
    hits
}
```

Add `pub mod discover;` to `crates/acad-dwg/src/lib.rs`.

- [ ] **Step 4: Run and confirm green**

Run: `cargo test -p acad-dwg discover`
Expected: PASS, 5 tests.

- [ ] **Step 5: Write the discovery binary**

`crates/acad-dwg/src/bin/dwg-discover.rs`. It reports, for each entity the DXF
declares, where that entity's coordinates sit in the DWG — which is what makes the
record layout visible.

```rust
//! Prints where each DXF entity's coordinates live inside the DWG sibling.
//!
//!     cargo run -p acad-dwg --bin dwg-discover -- \
//!         corpus/Samples/SUBDIV.DWG corpus/Samples/SUBDIV.DXF
//!
//! Reading its output is how a record layout gets recovered; nothing here is
//! part of the shipped read path.
use acad_dwg::discover::find_f64;
use acad_model::Entity;
use std::{fs, process::ExitCode};

/// The DXF prints six decimals, so a hit can be off by half of the last place.
const TOLERANCE: f64 = 5e-7;

/// `Entity` is an enum of struct variants — `Entity::Line { start, end }`, not
/// `Entity::Line(line)` — with exactly five kinds. Spelling is American:
/// `center`, `start_deg`, `end_deg`.
fn coords(e: &Entity) -> Vec<(&'static str, f64)> {
    match e {
        Entity::Line { start, end } => vec![
            ("x1", start.x), ("y1", start.y), ("x2", end.x), ("y2", end.y),
        ],
        Entity::Circle { center, radius } => {
            vec![("cx", center.x), ("cy", center.y), ("r", *radius)]
        }
        Entity::Arc { center, radius, start_deg, end_deg } => vec![
            ("cx", center.x), ("cy", center.y), ("r", *radius),
            ("start", *start_deg), ("end", *end_deg),
        ],
        Entity::Text { origin, height, rotation_deg, .. } => vec![
            ("x", origin.x), ("y", origin.y), ("h", *height), ("rot", *rotation_deg),
        ],
        Entity::Insert { origin, x_scale, y_scale, rotation_deg, .. } => vec![
            ("x", origin.x), ("y", origin.y),
            ("xs", *x_scale), ("ys", *y_scale), ("rot", *rotation_deg),
        ],
    }
}

fn main() -> ExitCode {
    let mut args = std::env::args().skip(1);
    let (Some(dwg_path), Some(dxf_path)) = (args.next(), args.next()) else {
        eprintln!("usage: dwg-discover <file.DWG> <file.DXF>");
        return ExitCode::FAILURE;
    };
    let dwg = fs::read(&dwg_path).expect("read DWG");
    let dxf = acad_dxf::parse(&fs::read(&dxf_path).expect("read DXF")).expect("parse DXF");

    for (i, entity) in dxf.entities().enumerate() {
        let fields = coords(entity);
        if fields.is_empty() {
            continue;
        }
        println!("DXF entity {i}: {}", entity_name(entity));
        for (label, value) in fields {
            let hits = find_f64(&dwg, value, TOLERANCE);
            let where_ = hits
                .iter()
                .map(|h| format!("{:#06x}", h.offset))
                .collect::<Vec<_>>()
                .join(" ");
            println!("    {label:<6} {value:<14} -> {}", if where_.is_empty() { "—".into() } else { where_ });
        }
        if i >= 4 {
            println!("... (stopping after 5; enough to see the stride)");
            break;
        }
    }
    ExitCode::SUCCESS
}

fn entity_name(e: &Entity) -> &'static str {
    match e {
        Entity::Line { .. } => "LINE",
        Entity::Circle { .. } => "CIRCLE",
        Entity::Arc { .. } => "ARC",
        Entity::Text { .. } => "TEXT",
        Entity::Insert { .. } => "INSERT",
    }
}
```

`acad-model` models exactly five entity kinds today — `Line`, `Circle`, `Arc`, `Text`,
`Insert` — because those are the five `SUBDIV.DXF` contains. Spec §4.2 lists eleven
record types, so the other corpus drawings may hold `POINT`, `SOLID`, `TRACE` or
`SHAPE`, which the model has no home for. That is Task 8's problem, not this one's.

- [ ] **Step 6: Run it and record what the layout is**

```bash
cargo run -p acad-dwg --bin dwg-discover -- corpus/Samples/SUBDIV.DWG corpus/Samples/SUBDIV.DXF
```

Expected: for DXF entity 0 (`LINE 1.012459,6.822910,1.261682,6.822910`), `y1`, `x2`
and `y2` resolve to `0x19e4`, `0x19ec`, `0x19f4` — the values the spec already records
— and `x1` to `0x19dc`. Subsequent entities appear at *decreasing* offsets, confirming
the reversed order.

Write into the commit message: the stride between consecutive entities' first fields,
and the byte distance from an entity's last coordinate to the next entity's first.
Those two numbers are the record size and the inter-record header size, and Task 3
needs both.

- [ ] **Step 7: Commit**

```bash
cargo fmt --all && cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace
git add crates/acad-dwg
git commit -m "feat(dwg): record discovery harness over the parallel corpus"
```

---

### Task 3: The `LINE` record and the entity walk

**Files:**
- Create: `crates/acad-dwg/src/entity.rs`
- Modify: `crates/acad-dwg/src/lib.rs`
- Create: `crates/acad-dwg/tests/entity_corpus.rs`

**Interfaces:**
- Consumes: `parse_header`, `HeaderMeta` (Task 1); the offsets Task 2 printed.
- Produces:
  - `acad_dwg::entity::{RecordHeader, read_entities}`
  - `read_entities(bytes: &[u8], meta: &HeaderMeta) -> Result<Vec<acad_model::Entity>, DwgError>`, returning entities in file order, which **is** DXF order — do not reverse
  - `read_items(bytes: &[u8], meta: &HeaderMeta) -> Result<Vec<acad_model::Item>, DwgError>` — the same walk, but keeping `BLOCK`/`ENDBLK` grouping as `Item::Block`. Task 3 implements `read_entities` and Task 5 adds `read_items` on top of it once block delimiters are understood; until then `read_items` wraps every entity as `Item::Entity`.

- [ ] **Step 1: Write the failing `LINE` test**

Use the offsets Task 2 printed. Build a synthetic buffer holding exactly one record at
the layout Task 2 found, then assert the parse.

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use acad_model::Entity;

    /// One LINE record, laid out as Task 2's discovery run showed: the type
    /// code and flags, then x1, y1, x2, y2 as little-endian doubles.
    fn one_line(x1: f64, y1: f64, x2: f64, y2: f64) -> Vec<u8> {
        let mut r = Vec::new();
        r.extend_from_slice(&TYPE_LINE.to_le_bytes());
        for v in [x1, y1, x2, y2] {
            r.extend_from_slice(&v.to_le_bytes());
        }
        r
    }

    #[test]
    fn reads_a_line_record() {
        // TYPE_LINE is 1: the type code indexes ACAD.EXE's entity name table,
        // whose first entry is LINE.
        let bytes = one_line(1.012459, 6.822910, 1.261682, 6.822910);
        let meta = HeaderMeta { entity_count: 1, entity_end: bytes.len() as u32 };
        let entities = read_entities(&bytes, &meta).unwrap();
        assert_eq!(entities.len(), 1);
        let Entity::Line { start, end } = &entities[0] else {
            panic!("expected a LINE, got {:?}", entities[0]);
        };
        assert_eq!(start.x, 1.012459);
        assert_eq!(end.y, 6.822910);
    }

    #[test]
    fn an_unknown_type_code_names_the_code_and_the_offset() {
        // Review Focus 3: a silently skipped entity renders as a drawing that
        // is quietly wrong, which is worse than one that fails to open.
        let mut bytes = one_line(0.0, 0.0, 1.0, 1.0);
        bytes[..2].copy_from_slice(&0x00ffu16.to_le_bytes());
        let meta = HeaderMeta { entity_count: 1, entity_end: bytes.len() as u32 };
        assert_eq!(
            read_entities(&bytes, &meta).unwrap_err(),
            DwgError::UnknownEntityType { code: 0x00ff, at: 0 }
        );
    }

    #[test]
    fn a_record_running_past_the_end_is_an_error_not_a_panic() {
        // Review Focus 2.
        let mut bytes = one_line(0.0, 0.0, 1.0, 1.0);
        bytes.truncate(bytes.len() - 4);
        let meta = HeaderMeta { entity_count: 1, entity_end: bytes.len() as u32 };
        assert!(matches!(
            read_entities(&bytes, &meta),
            Err(DwgError::TruncatedEntity { index: 0, .. })
        ));
    }

    #[test]
    fn fewer_records_than_the_header_promises_is_an_error() {
        let bytes = one_line(0.0, 0.0, 1.0, 1.0);
        let meta = HeaderMeta { entity_count: 5, entity_end: bytes.len() as u32 };
        assert_eq!(
            read_entities(&bytes, &meta).unwrap_err(),
            DwgError::EntityCountMismatch { want: 5, got: 1 }
        );
    }
}
```

- [ ] **Step 2: Run and confirm they fail**

Run: `cargo test -p acad-dwg entity`
Expected: FAIL — `cannot find function `read_entities``.

- [ ] **Step 3: Implement the walk and the `LINE` record**

Write `crates/acad-dwg/src/entity.rs` with:

- `TYPE_LINE = 1`, from `ACAD.EXE`'s entity table (see the facts table above);
- `read_entities`, which walks the entity region from `ENTITY_START = 0x1D8` to
  `meta.entity_end` and dispatches on the `u16` type code. **Do not reverse it** — file
  order already is DXF order;
- `read_items`, which for now is
  `read_entities(..).map(|v| v.into_iter().map(Item::Entity).collect())`. Task 5
  replaces its body once `BLOCK`/`ENDBLK` are understood. It exists from this task so
  that Task 6's `parse` has a stable name to call.

Every offset arithmetic goes through a checked helper returning
`DwgError::TruncatedEntity` rather than indexing directly, or Review Focus 2 fails on
the first truncated file.

The entity region starts at a fixed `0x1D8` in every `AC1.2` file — verified across six of
them, each showing a plausible `u16` type code there. The header's scalars end at `0xC4` and
the layer table fills `0xC4..0x1D8`; Task 7 reverses the table itself, but its length is
settled, so define `ENTITY_START = 0x1D8` as a named constant rather than deriving it.

Add `pub mod entity;` to `crates/acad-dwg/src/lib.rs`.

- [ ] **Step 4: Run and confirm green**

Run: `cargo test -p acad-dwg`
Expected: PASS — Task 1's 6, Task 2's 5, and these 4.

- [ ] **Step 5: Write the corpus test for `LINE`**

`crates/acad-dwg/tests/entity_corpus.rs`:

```rust
use acad_dwg::{entity::read_entities, header::parse_header};
use acad_model::Entity;

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
fn every_line_in_the_dxf_appears_in_the_dwg_in_the_same_order() {
    let (Some(dwg), Some(dxf)) = (
        corpus("Samples/SUBDIV.DWG"),
        corpus("Samples/SUBDIV.DXF"),
    ) else {
        return;
    };
    let (_, meta) = parse_header(&dwg).unwrap();
    let from_dwg = read_entities(&dwg, &meta).unwrap();
    let from_dxf = acad_dxf::parse(&dxf).unwrap();

    fn lines<'a>(it: impl Iterator<Item = &'a Entity>) -> Vec<(f64, f64, f64, f64)> {
        it.filter_map(|e| match e {
            Entity::Line { start, end } => Some((start.x, start.y, end.x, end.y)),
            _ => None,
        })
        .collect()
    }
    let a = lines(from_dwg.iter());
    let b = lines(from_dxf.entities());
    assert_eq!(a.len(), 133, "SUBDIV holds 133 LINEs (spec §4.4)");

    // The DXF prints six decimals, so compare to that precision rather than
    // demanding the bits agree.
    assert_eq!(a.len(), b.len());
    for (i, (p, q)) in a.iter().zip(b.iter()).enumerate() {
        for (x, y) in [(p.0, q.0), (p.1, q.1), (p.2, q.2), (p.3, q.3)] {
            assert!((x - y).abs() < 5e-7, "LINE {i}: {x} vs {y}");
        }
    }
}
```

- [ ] **Step 6: Run and reconcile**

Run: `cargo test -p acad-dwg --test entity_corpus -- --nocapture`

If the count is right but the order is reversed, `read_entities` is not reversing —
fix it there, not in the test; Review Focus 4 is the reason. If the count is wrong, the
record size is wrong: rerun `dwg-discover` and re-read the stride.

- [ ] **Step 7: Commit**

```bash
cargo fmt --all && cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace
git add crates/acad-dwg
git commit -m "feat(dwg): LINE records and the entity walk, in DXF order"
```

---

### Task 4: `CIRCLE` and `ARC`

**Files:**
- Modify: `crates/acad-dwg/src/entity.rs`
- Modify: `crates/acad-dwg/tests/entity_corpus.rs`

**Interfaces:**
- Consumes: `read_entities` and the record walk from Task 3.
- Produces: the same signature, now returning these variants too.

`CIRCLE` and `ARC` are the other two entity kinds `SUBDIV` contains, and both already
have a home in `acad_model::Entity`. The remaining record types spec §4.2 lists —
`POINT`, `SOLID`, `TRACE`, `SHAPE` — have no model representation and are Task 8.

Reverse one type at a time, each as its own RED→GREEN cycle. For each:

- [ ] **Step 1: Run the harness for that type**

```bash
cargo run -p acad-dwg --bin dwg-discover -- corpus/Samples/SUBDIV.DWG corpus/Samples/SUBDIV.DXF
```

`SUBDIV` holds 2 `CIRCLE` and 8 `ARC` (spec §4.4), and `dwg-discover` prints where
their centres, radii and angles sit.

- [ ] **Step 2: Write the failing synthetic test for the record**

Same shape as Task 3's `reads_a_line_record`, with the field order the harness showed.

- [ ] **Step 3: Run it and confirm it fails**

Expected: FAIL with `UnknownEntityType` carrying that type's code.

- [ ] **Step 4: Add the arm to the dispatch and implement the record**

- [ ] **Step 5: Run and confirm green**

- [ ] **Step 6: Add the corpus assertion**

Extend `entity_corpus.rs` with the same DXF-comparison shape Task 3 used, asserting the
counts spec §4.4 records: 2 `CIRCLE` and 8 `ARC`. `ARC`'s angles are the place to be
careful — milestone ① found `SUBDIV`'s arc sweeps counter-clockwise *through* 0° (start
78.604° → end 22.451°, a 303.8° sweep, not a negative 56°), and `acad_model` documents
its angles as "degrees, counter-clockwise, as stored by AutoCAD 1.4". So assert
`start_deg` and `end_deg` equal the DXF's stored values, not any normalised form.

- [ ] **Step 7: Commit after each type**

```bash
git commit -m "feat(dwg): <TYPE> records"
```

---

### Task 5: `BLOCK`, `ENDBLK`, `INSERT` and `TEXT`

**Files:**
- Modify: `crates/acad-dwg/src/entity.rs`
- Create: `crates/acad-dwg/src/text.rs`
- Modify: `crates/acad-dwg/tests/entity_corpus.rs`

**Interfaces:**
- Consumes: Task 4's dispatch.
- Produces: `read_entities` returning block delimiters and inserts; `acad_dwg::text::decode_latin1(bytes: &[u8]) -> String`

These are the records with variable or non-numeric content, which is why they are last.

- [ ] **Step 1: Write the failing Latin-1 test**

DXF text is Latin-1, not UTF-8 (milestone ①'s Global Constraints), and DWG is a binary
format, so the same applies with no escaping to hide behind.

```rust
#[test]
fn text_decodes_as_latin1_not_utf8() {
    // 0xE9 is é in Latin-1 and an invalid UTF-8 lead byte. Decoding as UTF-8
    // would either fail or produce a replacement character.
    assert_eq!(decode_latin1(&[b'c', b'a', b'f', 0xe9]), "café");
}

#[test]
fn a_nul_terminates_a_text_field() {
    assert_eq!(decode_latin1(&[b'h', b'i', 0, b'j', b'u', b'n', b'k']), "hi");
}
```

- [ ] **Step 2: Run and confirm failure, then implement**

```rust
/// DWG text is Latin-1: every byte is its own code point. Decoding as UTF-8
/// would reject perfectly good 1983 text.
pub fn decode_latin1(bytes: &[u8]) -> String {
    bytes
        .iter()
        .take_while(|&&b| b != 0)
        .map(|&b| b as char)
        .collect()
}
```

- [ ] **Step 3: Reverse `BLOCK` / `ENDBLK` / `INSERT` with the harness**

`SUBDIV` holds 6 `BLOCK`/`ENDBLK` pairs and 8 `INSERT` (spec §4.4), and milestone ①
established that block definitions and loose entities are **interleaved** — document
order is content. Assert that the DWG's block boundaries fall at the same points in
the entity sequence as the DXF's, not merely that the same blocks exist.

- [ ] **Step 4: Add the corpus assertions**

Assert the counts spec §4.4 records: 8 `TEXT`, 8 `INSERT`, 6 `BLOCK`/`ENDBLK` pairs, and
that `blocks()` yields the same names in the same order as the DXF's.

- [ ] **Step 5: Commit**

```bash
git commit -m "feat(dwg): BLOCK, INSERT and TEXT records"
```

---

### Task 6: Whole-drawing equality against the parallel corpus

**Files:**
- Create: `crates/acad-dwg/src/lib.rs` public `parse`
- Create: `crates/acad-dwg/tests/parallel_corpus.rs`

**Interfaces:**
- Consumes: everything above.
- Produces: `acad_dwg::parse(bytes: &[u8]) -> Result<acad_model::Drawing, DwgError>` — the crate's one entry point, mirroring `acad_dxf::parse`.

- [ ] **Step 1: Write the failing equality test**

This is the milestone's deliverable, so it is worth stating plainly.

```rust
use acad_dwg::DwgError;

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
fn subdiv_decodes_to_the_same_drawing_from_either_format() {
    let (Some(dwg), Some(dxf)) = (
        corpus("Samples/SUBDIV.DWG"),
        corpus("Samples/SUBDIV.DXF"),
    ) else {
        return;
    };
    let from_dwg = acad_dwg::parse(&dwg).unwrap();
    let from_dxf = acad_dxf::parse(&dxf).unwrap();

    // Review Focus 5: two empty drawings are equal, and a reader that returns
    // nothing would otherwise pass the milestone.
    assert_eq!(from_dxf.entities().count(), 159, "the DXF oracle itself must be whole");
    assert_eq!(from_dxf.blocks().count(), 6);

    assert_eq!(from_dwg.entities().count(), from_dxf.entities().count(), "entity count");
    assert_eq!(from_dwg.blocks().count(), from_dxf.blocks().count(), "block count");

    // Review Focus 4: items, not entities. Comparing the flat item list is what
    // catches an ordering difference, because a Block in the wrong place is
    // still the right Block.
    assert_eq!(from_dwg.items.len(), from_dxf.items.len(), "item count");
    for (i, (a, b)) in from_dwg.items.iter().zip(from_dxf.items.iter()).enumerate() {
        assert_eq!(
            std::mem::discriminant(a),
            std::mem::discriminant(b),
            "item {i} is a different kind — document order differs between the formats"
        );
    }
    assert_eq!(
        from_dwg.blocks().map(|b| b.name.as_str()).collect::<Vec<_>>(),
        from_dxf.blocks().map(|b| b.name.as_str()).collect::<Vec<_>>(),
        "block names, in order"
    );
}
```

- [ ] **Step 2: Run, implement `parse`, and reconcile**

```rust
/// Read an `AC1.2` drawing.
pub fn parse(bytes: &[u8]) -> Result<acad_model::Drawing, DwgError> {
    let (header, meta) = header::parse_header(bytes)?;
    // `Drawing` is a flat `Vec<Item>` in document order; `read_entities`
    // returns items already reversed into DXF order (Task 3).
    let items = entity::read_items(bytes, &meta)?;
    Ok(acad_model::Drawing { header, items })
}
```

Reconcile field-by-field rather than relaxing the assertions. A coordinate that differs
in the last bits is the DXF's six-decimal printing, and belongs in a tolerance; a
coordinate that differs in the first digits is a layout bug.

- [ ] **Step 3: Commit**

```bash
git commit -m "feat(dwg): acad_dwg::parse, verified against the SUBDIV parallel corpus"
```

---

### Task 7: Every `AC1.2` drawing loads and renders

**Files:**
- Create: `crates/acad-dwg/tests/corpus_smoke.rs`
- Modify: `crates/acad-app/src/main.rs`
- Modify: `README.md`

**Interfaces:**
- Consumes: `acad_dwg::parse`.
- Produces: `acad-app` opening `.DWG` as well as `.DXF`.

- [ ] **Step 1: Write the failing corpus smoke test**

Mirror `crates/acad-render/tests/raster.rs`'s existing shape — read it first and follow
it. For each of the 16 `AC1.2` drawings: parse, flatten, rasterise, and assert the
image is not blank. A drawing that parses but renders empty is the failure this catches.

- [ ] **Step 2: Run it and fix what it finds**

Expected: 16 drawings parse and produce non-blank images. A file that fails here is a
real finding — record which, and whether the cause is an unimplemented record type
(`DwgError::UnknownEntityType` names it) or a layout bug.

If a type appears only in drawings with no DXF sibling, its layout is inferred rather
than verified; say so in the commit message and in the README rather than letting the
green test imply otherwise.

- [ ] **Step 3: Teach `acad-app` to open DWG**

Dispatch on the file extension, or better on the magic — a `.BAK` file is a DWG too,
and the corpus has six of them.

- [ ] **Step 4: Update the README**

Change ④'s row from "read half starts first" to what is actually true, and say which
`AC1.2` record types are verified against the parallel corpus and which are inferred.

- [ ] **Step 5: Final verification**

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
mv corpus /tmp/corpus-hold && cargo test --workspace; mv /tmp/corpus-hold corpus
```

Expected: green both with and without the corpus.

- [ ] **Step 6: Commit**

```bash
git commit -m "feat(dwg): all 16 AC1.2 corpus drawings load and render"
```

---

### Task 8: Grow `acad-model` for the record types the corpus actually uses

**Files:**
- Modify: `crates/acad-model/src/entity.rs`
- Modify: `crates/acad-dwg/src/entity.rs`
- Modify: `crates/acad-render/src/flatten.rs`
- Modify: `crates/acad-dwg/tests/corpus_smoke.rs`

**Interfaces:**
- Consumes: the `DwgError::UnknownEntityType` codes Task 7 reported.
- Produces: new `acad_model::Entity` variants, and `acad-render` flattening them.

Spec §4.2 names eleven record types; `acad_model::Entity` has five, because those are
the five `SUBDIV.DXF` contains. The other fifteen `AC1.2` drawings may use `POINT`,
`SOLID`, `TRACE` or `SHAPE`. **Add only the ones Task 7 actually reported** — this task
is evidence-driven, and a variant no corpus file uses is a variant with no test.

Widening `acad-model` is allowed and expected: spec §5 calls it "the only one written
for the future rather than for 1983". The constraint is that the *model* stays
1983-agnostic. A `Solid` is four points, not "a 1983 SOLID record"; a `Trace` is four
points and a width. Anything that is 1983 policy — fixed field widths, record codes,
the fill toggle's meaning — stays in the codec.

- [ ] **Step 1: List what Task 7 reported**

From Task 7's output, list each `UnknownEntityType { code, at }` with the drawing it
came from. That list is this task's scope; if it is empty, close the task and say so.

- [ ] **Step 2: For each type, write the failing model test**

In `crates/acad-model/src/entity.rs`'s tests, assert the variant exists and holds what
the geometry needs — for `Point`, an origin; for `Solid` and `Trace`, four corners.

- [ ] **Step 3: Add the variant, then make `acad-render` flatten it**

Adding a variant breaks every exhaustive `match` in the workspace, which is the
compiler telling you where the decision belongs. `acad-render/src/flatten.rs` is the
one that matters: a new entity that flattens to nothing renders as a silently missing
shape, which Review Focus 3 exists to prevent.

- [ ] **Step 4: Add the DWG record, and re-run Task 7's smoke test**

Run: `cargo test -p acad-dwg --test corpus_smoke`
Expected: the drawing that previously failed now parses and renders non-blank.

- [ ] **Step 5: Commit per type**

```bash
git commit -m "feat(model): <TYPE> entity, and read it from AC1.2"
```

---

## Milestone ④ (early half) exit criteria

- `cargo test --workspace` passes, with and without the corpus present.
- `SUBDIV.DWG` decodes to the same `Drawing` as `SUBDIV.DXF`: same entity count, same kinds in the same order, coordinates equal to the DXF's printed precision.
- All 16 `AC1.2` drawings in the corpus parse and render non-blank.
- Any `acad-model` variant added along the way is geometric, not a 1983 record shape, and `acad-render` flattens it.
- An `AC1.40` file is refused with a message naming both versions.
- The README says which record types are verified against the parallel corpus and which are inferred from record size alone.

`AC1.40`, the write direction, and differential verification against oracle-generated drawings all wait for milestone ③, which gets its own plan from the same spec.
