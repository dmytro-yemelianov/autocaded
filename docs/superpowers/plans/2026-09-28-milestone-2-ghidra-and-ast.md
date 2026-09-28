# Milestone ② — Ghidra Overlay Loader, Headless Pipeline, AST Export — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Turn `ACAD.EXE` + `ACAD.OVL` into a typed Rust AST, reproducibly, from the raw disk images — and measure the §8 decision gate.

**Architecture:** The 1983 overlay-container format is decoded once, in Rust, inside `acad-re`. A binary emits `ovl-map.json`; two committed PyGhidra scripts consume it to build a correctly segmented Ghidra program (2 flat blocks for the EXE, 22 overlay blocks for the OVL regions) and export a dual AST. `acad-re` then deserializes that export into typed Rust and answers the call graph, the overlay→command map, and the gate metrics. Ghidra is never a source of truth about the file format — Rust is, and the Python only places bytes where Rust says.

**Tech Stack:** Rust 1.88.0, `serde` 1 + `serde_json` 1 (dev-only crate), Ghidra 12.1.3 via Homebrew, `openjdk@21`, PyGhidra 3.1.0 on Python 3.13.

**Spec:** `docs/superpowers/specs/2026-09-28-autocad-14-rust-design.md`

## Global Constraints

- Rust edition 2021, toolchain 1.88.0 (pinned by `rust-toolchain.toml`).
- `acad-re` is **dev-only** (spec §5): nothing in `acad-model`, `acad-dxf`, `acad-render` or `acad-app` may depend on it. It is a workspace member with its own binaries; that is the whole of its integration.
- 1983 policy lives in the codecs, never in `acad-model`. `acad-re` does not touch `acad-model`.
- The corpus is gitignored; `corpus/manifest.toml` is the committed record. Tests that need corpus bytes **skip with a message**, exactly as `crates/acad-dxf/tests/roundtrip.rs` does. They never fail on a fresh checkout.
- Generated Ghidra projects and full AST exports are **gitignored**. Small hand-checked AST fixtures **are** committed, so `acad-re`'s deserializer is tested on every machine.
- TDD: every step writes the failing test before the implementation.
- `cargo fmt --all --check` and `cargo clippy --workspace --all-targets -- -D warnings` must stay green; CI runs both.
- Spec §6.2 says "`analyzeHeadless` driven by PyGhidra scripts". This plan drives PyGhidra
  directly (`pyghidra.open_program`) instead. Same headless, scripted, reproducible result;
  `analyzeHeadless` cannot run Python scripts unless Ghidra was launched through PyGhidra
  anyway, so this is one layer fewer, not a different pipeline.
- Python scripts are formatted consistently but are not linted by CI — CI has no Ghidra. The pipeline is verified locally and its outputs' *shape* is verified in CI through the committed fixtures.

### Verified facts this plan builds on (spec §4.1)

| Fact | Value |
|---|---|
| `ACAD.EXE` sha256 | `0080e343930b6fad30b588f05bacb6fb65783d7d560cdf2f116ca09fc70b45b1`, 78,848 B on disk |
| `ACAD.EXE` real image | 78,340 B = `(154-1)*512 + 4`; trailing 508 B are cluster slack |
| `ACAD.EXE` segments | `CS` = load base; `DS = SS = +0x0E15` paragraphs. Code `0x0000-0xE150`, data `0xE150-0x13204` |
| `ACAD.OVL` sha256 | `c2e1ee5b538e53df022dbb8aba6ab65a3946b8a332438bdaf39a3938a43e0da8`, 179,480 B |
| `ACAD.OVL` header | 211 bytes. `"AC1.40"` at `+0`; `u16` window size at `+11` = `0xFD00`; 11 entries × 18 B at `+13` |
| Entry layout | `{u16 dest; u16 len; u32 file_off}` ×2 (code region, then data region), then `u16 entry_point` |
| Code window | `0xFD00` B. Entry 2's code region `dest=0x3770 len=0xC590` ends at exactly `0xFD00` |
| Data window | The EXE's own data segment, `DS:0x0002-0x3552` — 13,651 B, verified all-zero in the image; string pool starts at `DS:0x3553` |
| Coverage | The 22 regions cover 99.3% of the file; all 15 gaps are pads to the next `0x80`/`0x100` boundary |

## Review Focus

1. **Truncated or non-`AC1.40` `ACAD.OVL`** — a file shorter than 211 bytes, or with the wrong magic, must be a named error carrying the offset or the bytes found, never a panic and never a half-built `Directory`. *(Task 1)*
2. **A directory entry whose region runs past end-of-file** — `file_off + len > file_len` must name the entry index and the region, because a truncated OVL is exactly what a bad floppy read produces. *(Task 1)*
3. **Ghidra or JDK 21 absent** — the pipeline must say which one is missing and how to install it, not surface a Java stack trace or a `FileNotFoundError`. A contributor without Ghidra must still get a green `cargo test`. *(Task 2)*
4. **A region reaching past the end of the window it pages into** — `ACAD.EXE` reserves exactly `window_bytes` and reads each region straight in, so a region ending past that edge writes off the end of the allocation. It must be a named error. *(Task 4)* — *(Revised during execution: this line originally asked for an intra-entry code/data overlap check, which is vacuous. Region 1 always pages into the code window and region 2 into the EXE's data segment, so the two can never collide; entry 0's dests already overlap numerically and correctly. The window overrun is the real instance of the same defect class.)*
5. **A function Ghidra fails to decompile** — must be exported as an explicit failure record with its reason, never dropped. The §8 gate measures precisely this rate, so a silent drop would inflate the score and decide the gate wrongly. *(Task 5)*

---

### Task 1: `acad-re` crate and the `ACAD.OVL` container codec

**Files:**
- Create: `crates/acad-re/Cargo.toml`
- Create: `crates/acad-re/src/lib.rs`
- Create: `crates/acad-re/src/error.rs`
- Create: `crates/acad-re/src/ovl.rs`
- Create: `crates/acad-re/src/bin/ovl-map.rs`
- Create: `crates/acad-re/tests/ovl_corpus.rs`
- Modify: `Cargo.toml` (workspace members)

**Interfaces:**
- Consumes: nothing from earlier tasks.
- Produces:
  - `acad_re::ovl::{Directory, Entry, Region}`
  - `acad_re::ovl::parse(bytes: &[u8]) -> Result<Directory, ReError>`
  - `Directory { window_bytes: u16, entries: Vec<Entry> }`
  - `Entry { index: usize, code: Region, data: Region, entry_point: u16 }`
  - `Region { dest: u16, len: u16, file_off: u32 }` with `Region::end(&self) -> u64` and `Region::is_empty(&self) -> bool`
  - `acad_re::ReError`
  - binary `ovl-map` writing `ovl-map.json`

- [ ] **Step 1: Create the crate and register it in the workspace**

`crates/acad-re/Cargo.toml`:

```toml
[package]
name = "acad-re"
version = "0.1.0"
edition.workspace = true
rust-version.workspace = true

# Dev-only (spec §5): no shipped crate may depend on this one.
[dependencies]
serde = { version = "1", features = ["derive"] }
serde_json = "1"
```

In the workspace `Cargo.toml`, add `"crates/acad-re"` to `members`. Do **not** add it to `[workspace.dependencies]` — nothing should be able to reach for it by name.

```toml
[workspace]
members = ["crates/acad-model", "crates/acad-corpus", "crates/acad-dxf", "crates/acad-render", "crates/acad-app", "crates/acad-re"]
resolver = "2"
```

- [ ] **Step 2: Write the failing tests for the container codec**

`crates/acad-re/src/ovl.rs`, tests module only for now:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    /// Build a synthetic OVL header: magic, window size, then `entries`
    /// packed at +13, padded out to the 211-byte header length.
    fn header(window: u16, entries: &[[u16; 9]]) -> Vec<u8> {
        let mut h = vec![0u8; 211];
        h[..6].copy_from_slice(b"AC1.40");
        h[11..13].copy_from_slice(&window.to_le_bytes());
        for (i, e) in entries.iter().enumerate() {
            let at = 13 + i * 18;
            for (j, w) in e.iter().enumerate() {
                h[at + j * 2..at + j * 2 + 2].copy_from_slice(&w.to_le_bytes());
            }
        }
        h
    }

    /// A body long enough that every region in `header` is in range.
    fn with_body(mut h: Vec<u8>, len: usize) -> Vec<u8> {
        h.resize(len, 0);
        h
    }

    #[test]
    fn parses_window_size_and_eleven_entries() {
        let bytes = with_body(header(0xfd00, &[]), 0x2bd18);
        let dir = parse(&bytes).unwrap();
        assert_eq!(dir.window_bytes, 0xfd00);
        assert_eq!(dir.entries.len(), 11, "the directory is always 198/18 entries");
    }

    #[test]
    fn entry_fields_decode_in_file_order() {
        // dest,len,off_lo,off_hi for code; then the same for data; then entry point.
        let e = [0x0100, 0x5ca0, 0x0100, 0x0000, 0x0002, 0x351e, 0x5e00, 0x0000, 0x0100];
        let bytes = with_body(header(0xfd00, &[e]), 0x2bd18);
        let dir = parse(&bytes).unwrap();
        let first = &dir.entries[0];
        assert_eq!(first.index, 0);
        assert_eq!(first.code, Region { dest: 0x0100, len: 0x5ca0, file_off: 0x0100 });
        assert_eq!(first.data, Region { dest: 0x0002, len: 0x351e, file_off: 0x5e00 });
        assert_eq!(first.entry_point, 0x0100);
    }

    #[test]
    fn high_word_of_the_file_offset_is_honoured() {
        // Region 1 at file offset 0x0001_d600 — above 64 KiB, so the high word matters.
        let e = [0x3770, 0xc590, 0xd600, 0x0001, 0, 0, 0, 0, 0x4d90];
        let bytes = with_body(header(0xfd00, &[e]), 0x2bd18);
        assert_eq!(parse(&bytes).unwrap().entries[0].code.file_off, 0x0001_d600);
    }

    #[test]
    fn wrong_magic_is_a_named_error() {
        let mut bytes = with_body(header(0xfd00, &[]), 0x2bd18);
        bytes[..6].copy_from_slice(b"AC1.20");
        assert_eq!(
            parse(&bytes).unwrap_err(),
            ReError::BadMagic { found: *b"AC1.20\0\0" }
        );
    }

    #[test]
    fn a_file_shorter_than_the_header_is_an_error_not_a_panic() {
        let bytes = header(0xfd00, &[])[..200].to_vec();
        assert_eq!(parse(&bytes).unwrap_err(), ReError::ShortHeader { len: 200 });
    }

    #[test]
    fn a_region_running_past_end_of_file_names_the_entry() {
        // len 0x100 at offset 0x2bd00 in a 0x2bd18-byte file overruns by 0xe8.
        let e = [0x0100, 0x0100, 0xbd00, 0x0002, 0, 0, 0, 0, 0x0100];
        let bytes = with_body(header(0xfd00, &[e]), 0x2bd18);
        assert_eq!(
            parse(&bytes).unwrap_err(),
            ReError::RegionPastEof { entry: 0, region: "code", end: 0x2be00, file_len: 0x2bd18 }
        );
    }

    #[test]
    fn an_empty_region_is_not_an_overrun() {
        // Entries 0..10 are zero-filled by `header`, so every region is len 0 at
        // offset 0. A zero-length region is absent, not a read past the start.
        let bytes = with_body(header(0xfd00, &[]), 211);
        let dir = parse(&bytes).unwrap();
        assert!(dir.entries.iter().all(|e| e.code.is_empty() && e.data.is_empty()));
    }
}
```

- [ ] **Step 3: Run the tests and confirm they fail**

Run: `cargo test -p acad-re`
Expected: FAIL — `cannot find function `parse``, `cannot find type `Region``.

- [ ] **Step 4: Implement the error type**

`crates/acad-re/src/error.rs`:

```rust
use std::fmt;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReError {
    /// The file is shorter than the 211-byte header `ACAD.EXE` reads.
    ShortHeader { len: usize },
    /// The first eight bytes are not `AC1.40\0\0`.
    BadMagic { found: [u8; 8] },
    /// A directory entry points past the end of the file.
    RegionPastEof { entry: usize, region: &'static str, end: u64, file_len: u64 },
    /// A region reaches past the end of the window it pages into.
    RegionPastWindow { entry: usize, dest: u16, end: u32, window: u16 },
}

impl fmt::Display for ReError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ShortHeader { len } => {
                write!(f, "overlay file is {len} bytes: shorter than the 211-byte header")
            }
            Self::BadMagic { found } => {
                write!(f, "overlay magic is {:?}, expected \"AC1.40\"", String::from_utf8_lossy(found))
            }
            Self::RegionPastEof { entry, region, end, file_len } => write!(
                f,
                "entry {entry} {region} region ends at {end:#x}, past end of file {file_len:#x}"
            ),
            Self::RegionsOverlap { entry, window, at } => {
                write!(f, "entry {entry}: code and data overlap in the {window} window at {at:#x}")
            }
        }
    }
}

impl std::error::Error for ReError {}
```

- [ ] **Step 5: Implement the container codec**

`crates/acad-re/src/ovl.rs`, above the tests module:

```rust
use crate::ReError;
use serde::{Deserialize, Serialize};

/// `ACAD.EXE` freads exactly this many bytes and strcmps the magic.
pub const HEADER_LEN: usize = 211;
/// The directory begins here; `ACAD.EXE`'s chain loop starts at `&header[13]`.
pub const DIR_OFF: usize = 13;
/// `{u16 dest; u16 len; u32 file_off}` twice, then `u16 entry_point`.
pub const ENTRY_LEN: usize = 18;
/// `(211 - 13) / 18`.
pub const ENTRY_COUNT: usize = (HEADER_LEN - DIR_OFF) / ENTRY_LEN;

/// One contiguous run of file bytes paged into one window.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Region {
    /// Byte offset within the destination window.
    pub dest: u16,
    pub len: u16,
    pub file_off: u32,
}

impl Region {
    pub fn end(&self) -> u64 {
        self.file_off as u64 + self.len as u64
    }
    pub fn is_empty(&self) -> bool {
        self.len == 0
    }
}

/// One overlay: a code region, a data region, and where to call once loaded.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Entry {
    pub index: usize,
    pub code: Region,
    pub data: Region,
    pub entry_point: u16,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Directory {
    /// Bytes `ACAD.EXE` reserves for the code window, from header `+11`.
    pub window_bytes: u16,
    pub entries: Vec<Entry>,
}

fn u16_at(b: &[u8], at: usize) -> u16 {
    u16::from_le_bytes([b[at], b[at + 1]])
}

fn region_at(b: &[u8], at: usize) -> Region {
    Region {
        dest: u16_at(b, at),
        len: u16_at(b, at + 2),
        file_off: u16_at(b, at + 4) as u32 | (u16_at(b, at + 6) as u32) << 16,
    }
}

/// Decode the 211-byte header `ACAD.EXE` reads from the front of `ACAD.OVL`.
pub fn parse(bytes: &[u8]) -> Result<Directory, ReError> {
    if bytes.len() < HEADER_LEN {
        return Err(ReError::ShortHeader { len: bytes.len() });
    }
    let mut magic = [0u8; 8];
    magic.copy_from_slice(&bytes[..8]);
    if &magic[..6] != b"AC1.40" {
        return Err(ReError::BadMagic { found: magic });
    }

    let file_len = bytes.len() as u64;
    let mut entries = Vec::with_capacity(ENTRY_COUNT);
    for index in 0..ENTRY_COUNT {
        let at = DIR_OFF + index * ENTRY_LEN;
        let entry = Entry {
            index,
            code: region_at(bytes, at),
            data: region_at(bytes, at + 8),
            entry_point: u16_at(bytes, at + 16),
        };
        for (region, name) in [(entry.code, "code"), (entry.data, "data")] {
            if !region.is_empty() && region.end() > file_len {
                return Err(ReError::RegionPastEof {
                    entry: index,
                    region: name,
                    end: region.end(),
                    file_len,
                });
            }
        }
        entries.push(entry);
    }

    Ok(Directory { window_bytes: u16_at(bytes, 11), entries })
}
```

`crates/acad-re/src/lib.rs`:

```rust
//! Dev-only reverse-engineering support (spec §5): the `ACAD.OVL` container
//! codec, a typed Ghidra AST model, and the analyses built on them. No shipped
//! crate depends on this one.
pub mod error;
pub mod ovl;

pub use error::ReError;
```

- [ ] **Step 6: Run the tests and confirm they pass**

Run: `cargo test -p acad-re`
Expected: PASS, 7 tests.

- [ ] **Step 7: Write the failing corpus test against the real `ACAD.OVL`**

`crates/acad-re/tests/ovl_corpus.rs`. This is where the spike's findings become regression tests — each assertion is one thing that would be false if the layout were decoded wrongly.

```rust
use acad_re::ovl::{self, Directory};

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

fn directory() -> Option<(Vec<u8>, Directory)> {
    let bytes = corpus("System/ACAD.OVL")?;
    let dir = ovl::parse(&bytes).unwrap();
    Some((bytes, dir))
}

#[test]
fn the_real_overlay_declares_a_64768_byte_window() {
    let Some((_, dir)) = directory() else { return };
    assert_eq!(dir.window_bytes, 0xfd00);
    assert_eq!(dir.entries.len(), 11);
}

#[test]
fn the_largest_code_region_ends_exactly_at_the_window_size() {
    // Entry 2 is the big overlay: dest 0x3770 + len 0xc590 == 0xfd00. The window
    // is sized to its largest consumer, so this is the load-bearing coincidence
    // that confirms `dest` is a byte offset into the window.
    let Some((_, dir)) = directory() else { return };
    let e = dir.entries[2];
    assert_eq!(e.code.dest as u32 + e.code.len as u32, dir.window_bytes as u32);
}

#[test]
fn resident_entries_abut_in_both_windows() {
    // Entries 8 and 9 are loaded together and must not overlap: 8's code ends
    // where 9's begins, and likewise for data.
    let Some((_, dir)) = directory() else { return };
    let (a, b) = (dir.entries[8], dir.entries[9]);
    assert_eq!(a.code.dest + a.code.len, b.code.dest, "code windows abut");
    assert_eq!(a.data.dest + a.data.len, b.data.dest, "data windows abut");
}

#[test]
fn every_data_region_fits_below_the_exe_string_pool() {
    // Overlay data pages into the EXE's own data segment at DS:0x0002-0x3552;
    // DS:0x3553 is the first byte of `ACAD.EXE`'s string pool. A data region
    // reaching 0x3553 would overwrite "Not enough core for overlays".
    const STRING_POOL: u32 = 0x3553;
    let Some((_, dir)) = directory() else { return };
    for e in &dir.entries {
        assert!(
            e.data.dest as u32 + e.data.len as u32 <= STRING_POOL,
            "entry {} data region reaches {:#x}, into the string pool",
            e.index,
            e.data.dest as u32 + e.data.len as u32
        );
    }
}

#[test]
fn the_regions_cover_almost_the_whole_file_with_only_alignment_gaps() {
    let Some((bytes, dir)) = directory() else { return };
    let mut spans: Vec<(u64, u64)> = dir
        .entries
        .iter()
        .flat_map(|e| [e.code, e.data])
        .filter(|r| !r.is_empty())
        .map(|r| (r.file_off as u64, r.end()))
        .collect();
    assert_eq!(spans.len(), 22, "11 entries x 2 regions, all non-empty");
    spans.sort_unstable();

    let mut merged: Vec<(u64, u64)> = Vec::new();
    for (s, e) in spans {
        match merged.last_mut() {
            Some(last) if s <= last.1 => last.1 = last.1.max(e),
            _ => merged.push((s, e)),
        }
    }
    let covered: u64 = merged.iter().map(|(s, e)| e - s).sum();
    let total = bytes.len() as u64;
    // 178,221 of 179,480 is 99.2986%, which the spec rounds to 99.3%. Assert on
    // ten-thousandths so integer truncation does not force the bound down to a
    // slacker 99.2%: 9929 passes on today's corpus and 9930 does not.
    assert!(
        covered * 10_000 / total >= 9_929,
        "regions cover {covered} of {total} bytes ({}.{:02}%), expected >= 99.29%",
        covered * 100 / total,
        covered * 10_000 / total % 100
    );

    // The payload starts at 0x100: the 211-byte header, padded to 256. That
    // leading gap is the header, not lost content, so the scan starts after it.
    assert_eq!(merged[0].0, 0x100, "payload should start just past the padded header");

    // Every gap between regions is a pad to the next 0x80 boundary.
    let mut prev = 0x100u64;
    for (s, e) in &merged {
        if *s > prev {
            assert!(s - prev < 0x80, "gap {prev:#x}-{s:#x} is too large to be alignment padding");
        }
        prev = *e;
    }
}
```

- [ ] **Step 8: Run the corpus test**

Run: `cargo test -p acad-re --test ovl_corpus -- --nocapture`
Expected: PASS with the real corpus present; five `skipping: corpus/System/ACAD.OVL absent` lines and PASS without it. Verify the skip path too:

```bash
mv corpus/System/ACAD.OVL /tmp/ && cargo test -p acad-re --test ovl_corpus -- --nocapture; mv /tmp/ACAD.OVL corpus/System/
```

- [ ] **Step 9: Write the `ovl-map` binary**

`crates/acad-re/src/bin/ovl-map.rs`. This is the single source of truth the Python consumes — the Ghidra scripts never parse the header themselves.

```rust
//! Emits the Ghidra loader's input: where every overlay region lives in the
//! file and where it belongs in memory. The PyGhidra scripts read this and
//! never decode the 1983 container format themselves.
use acad_re::ovl;
use serde::Serialize;
use std::{fs, path::PathBuf, process::ExitCode};

/// `ACAD.EXE`'s image loads at this paragraph in Ghidra (its MZ loader default).
const IMAGE_SEG: u32 = 0x1000;
/// `DS = SS = image + 0x0E15` paragraphs (spec §4.1).
const DATA_SEG: u32 = IMAGE_SEG + 0x0e15;
/// Byte offset in the image where the data segment starts.
const DATA_OFF: u32 = 0x0e15 * 16;
/// The real image, excluding 508 bytes of cluster slack.
const IMAGE_LEN: u32 = 153 * 512 + 4;
/// The code window is allocated above the image; the first free paragraph.
const WINDOW_SEG: u32 = IMAGE_SEG + 0x1321;

#[derive(Serialize)]
struct Block {
    name: String,
    seg: u32,
    off: u32,
    len: u32,
    /// `None` for blocks filled from `ACAD.EXE`, `Some` for overlay regions.
    src_off: Option<u32>,
    overlay: bool,
}

#[derive(Serialize)]
struct Map {
    exe: PathBuf,
    ovl: PathBuf,
    image_len: u32,
    window_bytes: u16,
    blocks: Vec<Block>,
    entry_points: Vec<EntryPoint>,
}

#[derive(Serialize)]
struct EntryPoint {
    entry: usize,
    seg: u32,
    off: u32,
}

fn main() -> ExitCode {
    let mut args = std::env::args_os().skip(1);
    let (Some(exe), Some(ovl_path), Some(out)) = (args.next(), args.next(), args.next()) else {
        eprintln!("usage: ovl-map <ACAD.EXE> <ACAD.OVL> <out.json>");
        return ExitCode::FAILURE;
    };
    let (exe, ovl_path, out) = (PathBuf::from(exe), PathBuf::from(ovl_path), PathBuf::from(out));

    let bytes = match fs::read(&ovl_path) {
        Ok(b) => b,
        Err(e) => {
            eprintln!("cannot read {}: {e}", ovl_path.display());
            return ExitCode::FAILURE;
        }
    };
    let dir = match ovl::parse(&bytes) {
        Ok(d) => d,
        Err(e) => {
            eprintln!("{}: {e}", ovl_path.display());
            return ExitCode::FAILURE;
        }
    };

    let mut blocks = vec![
        Block { name: "EXE_CODE".into(), seg: IMAGE_SEG, off: 0, len: DATA_OFF, src_off: None, overlay: false },
        Block { name: "EXE_DATA".into(), seg: DATA_SEG, off: 0, len: IMAGE_LEN - DATA_OFF, src_off: None, overlay: false },
    ];
    let mut entry_points = Vec::new();
    for e in &dir.entries {
        if !e.code.is_empty() {
            blocks.push(Block {
                name: format!("OVL{:02}_CODE", e.index),
                seg: WINDOW_SEG,
                off: e.code.dest as u32,
                len: e.code.len as u32,
                src_off: Some(e.code.file_off),
                overlay: true,
            });
        }
        if !e.data.is_empty() {
            blocks.push(Block {
                name: format!("OVL{:02}_DATA", e.index),
                seg: DATA_SEG,
                off: e.data.dest as u32,
                len: e.data.len as u32,
                src_off: Some(e.data.file_off),
                overlay: true,
            });
        }
        entry_points.push(EntryPoint { entry: e.index, seg: WINDOW_SEG, off: e.entry_point as u32 });
    }

    let map = Map { exe, ovl: ovl_path, image_len: IMAGE_LEN, window_bytes: dir.window_bytes, blocks, entry_points };
    match fs::write(&out, serde_json::to_string_pretty(&map).unwrap()) {
        Ok(()) => {
            println!("wrote {} ({} blocks)", out.display(), map.blocks.len());
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("cannot write {}: {e}", out.display());
            ExitCode::FAILURE
        }
    }
}
```

- [ ] **Step 10: Run it against the real overlay**

```bash
mkdir -p build
cargo run -p acad-re --bin ovl-map -- corpus/System/ACAD.EXE corpus/System/ACAD.OVL build/ovl-map.json
```

Expected: `wrote build/ovl-map.json (24 blocks)` — 2 EXE blocks plus 22 overlay regions. Confirm:

```bash
python3 -c "import json;m=json.load(open('build/ovl-map.json'));print(len(m['blocks']), m['window_bytes'], len(m['entry_points']))"
```

Expected: `24 64768 11`.

- [ ] **Step 11: Gitignore build output and commit**

Append to `.gitignore`:

```
# Generated RE pipeline output: reproducible from the corpus, not history
/build
```

```bash
cargo fmt --all
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
git add Cargo.toml Cargo.lock .gitignore crates/acad-re
git commit -m "feat(re): ACAD.OVL container codec and ovl-map emitter"
```

---

### Task 2: Ghidra environment bootstrap

**Files:**
- Create: `tools/ghidra-env.sh`
- Create: `docs/re-pipeline.md`

**Interfaces:**
- Consumes: nothing.
- Produces: `tools/ghidra-env.sh`, which when **sourced** exports `GHIDRA_INSTALL_DIR`, `JAVA_HOME`, and `PYGHIDRA_PYTHON` (the venv interpreter with `pyghidra` importable), and when **run** prints a diagnosis and exits non-zero if anything is missing.

- [ ] **Step 1: Write the script**

`tools/ghidra-env.sh`. PyGhidra ships as a wheel inside the Ghidra install; installing from there rather than PyPI pins the bindings to this exact Ghidra. Python 3.13 is required because the bundled JPype has no `cp39` arm64 wheel.

```bash
#!/usr/bin/env bash
# tools/ghidra-env.sh — locate Ghidra + JDK 21 and build the PyGhidra venv.
#
#   source tools/ghidra-env.sh   # exports GHIDRA_INSTALL_DIR, JAVA_HOME, PYGHIDRA_PYTHON
#   ./tools/ghidra-env.sh        # diagnose only; non-zero if anything is missing
set -euo pipefail
root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
venv="$root/build/pyghidra-venv"

die() { echo "ghidra-env: $*" >&2; return 1; }

find_ghidra() {
  if [ -n "${GHIDRA_INSTALL_DIR:-}" ] && [ -d "$GHIDRA_INSTALL_DIR/Ghidra" ]; then
    echo "$GHIDRA_INSTALL_DIR"; return
  fi
  for c in /opt/homebrew/Cellar/ghidra/*/libexec /usr/local/Cellar/ghidra/*/libexec \
           /opt/ghidra /usr/share/ghidra "$HOME/ghidra"; do
    [ -d "$c/Ghidra" ] && { echo "$c"; return; }
  done
  die "Ghidra not found. Install it (macOS: brew install ghidra) or set GHIDRA_INSTALL_DIR."
}

find_jdk() {
  for c in "${JAVA_HOME:-}" /opt/homebrew/opt/openjdk@21 /usr/local/opt/openjdk@21 \
           /usr/lib/jvm/java-21-openjdk; do
    [ -n "$c" ] && [ -x "$c/bin/java" ] && { echo "$c"; return; }
  done
  die "JDK 21 not found. Ghidra 12 needs it (macOS: brew install openjdk@21) or set JAVA_HOME."
}

find_python() {
  for c in python3.13 python3.12 python3.11; do
    command -v "$c" >/dev/null && { command -v "$c"; return; }
  done
  die "Python 3.11+ not found. The bundled JPype has no wheel for older versions."
}

GHIDRA_INSTALL_DIR="$(find_ghidra)"
JAVA_HOME="$(find_jdk)"
py="$(find_python)"
export GHIDRA_INSTALL_DIR JAVA_HOME

dist="$GHIDRA_INSTALL_DIR/Ghidra/Features/PyGhidra/pypkg/dist"
[ -d "$dist" ] || die "no PyGhidra wheels at $dist — is this Ghidra 11.3 or newer?"

if [ ! -x "$venv/bin/python" ]; then
  echo "ghidra-env: creating $venv" >&2
  "$py" -m venv "$venv"
  "$venv/bin/pip" install --quiet --upgrade pip
  "$venv/bin/pip" install --quiet --no-index --find-links "$dist" pyghidra
fi
export PYGHIDRA_PYTHON="$venv/bin/python"

# Sourced or run? `return` succeeds only when sourced.
(return 0 2>/dev/null) || {
  echo "GHIDRA_INSTALL_DIR = $GHIDRA_INSTALL_DIR"
  echo "JAVA_HOME          = $JAVA_HOME ($("$JAVA_HOME/bin/java" -version 2>&1 | head -1))"
  echo "PYGHIDRA_PYTHON    = $PYGHIDRA_PYTHON"
  "$PYGHIDRA_PYTHON" -c "import pyghidra; print('pyghidra', pyghidra.__version__)"
}
```

- [ ] **Step 2: Make it executable and run the diagnosis**

```bash
chmod +x tools/ghidra-env.sh
./tools/ghidra-env.sh
```

Expected, on a machine with Ghidra installed:

```
GHIDRA_INSTALL_DIR = /opt/homebrew/Cellar/ghidra/12.1.3/libexec
JAVA_HOME          = /opt/homebrew/opt/openjdk@21 (openjdk version "21.0.12.1" ...)
PYGHIDRA_PYTHON    = .../build/pyghidra-venv/bin/python
pyghidra 3.1.0
```

- [ ] **Step 3: Confirm the missing-tool path is actionable**

This is Review Focus item 3 — verify the message rather than assuming it.

```bash
GHIDRA_INSTALL_DIR=/nonexistent bash -c 'cd /Users/dmytro/github/autorust && ./tools/ghidra-env.sh'; echo "exit=$?"
```

Expected: the script falls back to the search paths and still succeeds. Now force a real failure:

```bash
env -i PATH=/usr/bin:/bin HOME="$HOME" bash -c 'cd /Users/dmytro/github/autorust && ./tools/ghidra-env.sh'; echo "exit=$?"
```

Expected: a single line naming what is missing and how to install it, and a non-zero exit — not a stack trace.

- [ ] **Step 4: Document the pipeline**

`docs/re-pipeline.md`:

```markdown
# Reverse-engineering pipeline

Reproduces the Ghidra analysis of AutoCAD 1.4 from the raw disk images. Nothing
it produces is committed; everything it produces is regenerable.

## Prerequisites

- Ghidra 11.3+ (developed against 12.1.3). macOS: `brew install ghidra`.
- A JDK 21. macOS: `brew install openjdk@21` — it is keg-only, so it is not on
  `PATH`; `tools/ghidra-env.sh` finds it anyway.
- Python 3.11+ for the PyGhidra venv. The wheels come from inside the Ghidra
  install, not PyPI, so the bindings always match the installed Ghidra.
- The corpus: `./tools/extract-corpus.sh`.

Check the environment with `./tools/ghidra-env.sh`.

## Running it

    ./tools/re-pipeline.sh

Outputs land in `build/`:

| File | What it is |
|---|---|
| `ovl-map.json` | Where every overlay region lives, from `acad-re` |
| `acad.gpr`, `acad.rep/` | The Ghidra project |
| `ast-pcode.json` | High P-Code, SSA form, keyed by function address |
| `ast-clang.json` | Clang markup, a C AST linked back to P-Code varnodes |
| `re-report.json` | The §8 decision-gate metrics |

## Why the format is decoded in Rust

`acad-re` owns the `ACAD.OVL` container format and emits `ovl-map.json`. The
PyGhidra scripts only place bytes where that file says. One implementation, one
set of tests, and the layout stays covered by `cargo test`.
```

- [ ] **Step 5: Commit**

```bash
git add tools/ghidra-env.sh docs/re-pipeline.md
git commit -m "feat(re): Ghidra and PyGhidra environment bootstrap"
```

---

### Task 3: Ghidra loader — the `ACAD.EXE` blocks and segment registers

**Files:**
- Create: `tools/ghidra/load_acad.py`
- Create: `tools/ghidra/ghidra_common.py`

**Interfaces:**
- Consumes: `build/ovl-map.json` from Task 1; `PYGHIDRA_PYTHON` etc. from Task 2.
- Produces: `tools/ghidra/ghidra_common.py` exposing `open_or_create(project_dir, project_name, exe_path)` and `seg_addr(program, seg, off)`; `load_acad.py` runnable as `$PYGHIDRA_PYTHON tools/ghidra/load_acad.py build/ovl-map.json build/ghidra`.

- [ ] **Step 1: Write the shared helpers**

`tools/ghidra/ghidra_common.py`:

```python
"""Helpers shared by the PyGhidra scripts. Import after pyghidra.start()."""
import os
import pyghidra


def start():
    """Boot the JVM against the Ghidra located by tools/ghidra-env.sh."""
    pyghidra.start(install_dir=os.environ["GHIDRA_INSTALL_DIR"])


def seg_addr(program, seg, off):
    """A real-mode seg:off address in the program's default space."""
    return program.getAddressFactory().getAddress("%04x:%04x" % (seg, off))


def block_by_name(program, name):
    for b in program.getMemory().getBlocks():
        if b.getName() == name:
            return b
    return None
```

- [ ] **Step 2: Write the failing loader check**

Write `tools/ghidra/load_acad.py` so that it ends in a self-check, and run it before the block-building code exists. The self-check is the test: it asserts that a string whose address is known from the spec reads back correctly, which is only true if the segment layout is right.

```python
#!/usr/bin/env python
"""Build a correctly segmented Ghidra program for ACAD.EXE.

Blocks and addresses come from build/ovl-map.json, which acad-re generates.
This script decodes no 1983 file formats of its own.
"""
import json
import os
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import ghidra_common as gc

gc.start()
import pyghidra
from java.math import BigInteger


def load(map_path, project_dir):
    with open(map_path) as fh:
        m = json.load(fh)

    exe_code = next(b for b in m["blocks"] if b["name"] == "EXE_CODE")
    exe_data = next(b for b in m["blocks"] if b["name"] == "EXE_DATA")

    with pyghidra.open_program(m["exe"], project_location=project_dir,
                               project_name="acad14", analyze=False) as api:
        program = api.getCurrentProgram()
        mem = program.getMemory()

        # Ghidra's MZ loader makes one flat block. Split it so that code and
        # data are separately addressable, then tell the disassembler what DS
        # is — without that, every string reference resolves to the wrong place.
        flat = gc.block_by_name(program, "CODE_0")
        if flat is not None:
            split_at = gc.seg_addr(program, exe_data["seg"], 0)
            mem.split(flat, split_at)
            mem.getBlock(flat.getStart()).setName("EXE_CODE")
            mem.getBlock(split_at).setName("EXE_DATA")

        ctx = program.getProgramContext()
        code = gc.block_by_name(program, "EXE_CODE")
        for reg_name, value in (("CS", exe_code["seg"]),
                                ("DS", exe_data["seg"]),
                                ("SS", exe_data["seg"]),
                                ("ES", exe_data["seg"])):
            reg = ctx.getRegister(reg_name)
            if reg is not None:
                ctx.setValue(reg, code.getStart(), code.getEnd(),
                             BigInteger.valueOf(value))

        check(program, exe_data["seg"])
        return program


def check(program, data_seg):
    """The layout is right only if known strings read back at known addresses.

    DS:0x3553 is the first byte of ACAD.EXE's string pool and DS:0x3597 is the
    overlay filename format (spec §4.1). Both are load-bearing: if the data
    segment were off by even one paragraph, neither would match.
    """
    for off, want in ((0x3553, b"Not enough core for overlays"),
                      (0x3597, b"%c:%s.OVL")):
        addr = gc.seg_addr(program, data_seg, off)
        got = bytes(program.getMemory().getBytes(addr, len(want)))
        assert got == want, "DS:%04x is %r, expected %r" % (off, got, want)
    print("segment layout verified: DS:3553 and DS:3597 read back correctly")


if __name__ == "__main__":
    load(sys.argv[1], sys.argv[2])
    print("loaded ACAD.EXE")
```

- [ ] **Step 3: Run it and confirm the check is real**

```bash
source tools/ghidra-env.sh
"$PYGHIDRA_PYTHON" tools/ghidra/load_acad.py build/ovl-map.json build/ghidra
```

Expected: `segment layout verified: ...` then `loaded ACAD.EXE`.

Then confirm the check would catch a wrong layout — change `DATA_SEG` in `ovl-map.rs` to `IMAGE_SEG + 0x0e14`, regenerate, rerun, and expect an `AssertionError` naming the bytes found. Restore `0x0e15` afterwards and regenerate.

- [ ] **Step 4: Commit**

```bash
git add tools/ghidra/ghidra_common.py tools/ghidra/load_acad.py
git commit -m "feat(re): Ghidra loader for ACAD.EXE with recovered segment layout"
```

---

### Task 4: Ghidra loader — the 22 overlay blocks

**Files:**
- Modify: `tools/ghidra/load_acad.py`
- Modify: `crates/acad-re/src/ovl.rs` (add overlap validation)
- Modify: `crates/acad-re/src/error.rs` is already written in Task 1; no change needed

**Interfaces:**
- Consumes: `Directory` from Task 1, `load()` from Task 3.
- Produces: `acad_re::ovl::parse` additionally rejecting intra-entry window overlap; `load_acad.py` creating one Ghidra overlay block per region.

- [ ] **Step 1: Write the failing window-overrun tests**

Region 1 of every entry pages into the code window and region 2 into the EXE's own
data segment, so code and data can never collide — entry 0's dests overlap numerically
(`0x0100..0x5DA0` against `0x0002..0x3520`) and that is correct. The real instance of
Review Focus 4 is a region reaching past the edge of the window it pages into: the
loader would write off the end of the allocation. Add to `crates/acad-re/src/ovl.rs`'s
tests module:

```rust
    #[test]
    fn a_code_region_overrunning_the_declared_window_is_an_error() {
        let e = [0xfd00, 0x0100, 0x0100, 0x0000, 0, 0, 0, 0, 0x0100];
        let bytes = with_body(header(0xfd00, &[e]), 0x2bd18);
        assert_eq!(
            parse(&bytes).unwrap_err(),
            ReError::RegionPastWindow { entry: 0, dest: 0xfd00, end: 0xfe00, window: 0xfd00 }
        );
    }

    #[test]
    fn a_code_region_ending_exactly_at_the_window_edge_is_allowed() {
        // Entry 2 of the real overlay does exactly this: 0x3770 + 0xc590 =
        // 0xfd00. An off-by-one here would reject the shipping file.
        let e = [0x3770, 0xc590, 0x0100, 0x0000, 0, 0, 0, 0, 0x4d90];
        let bytes = with_body(header(0xfd00, &[e]), 0x2bd18);
        assert!(parse(&bytes).is_ok());
    }
```

- [ ] **Step 2: Run them and confirm they fail**

Run: `cargo test -p acad-re overrunning`
Expected: FAIL — `no variant named `RegionPastWindow` found for enum `error::ReError``.

- [ ] **Step 3: Implement the window check**

Replace the `RegionsOverlap` variant in `error.rs` with `RegionPastWindow { entry, dest, end, window }`
and its `Display` arm. In `parse`, hoist `let window = u16_at(bytes, 11);` above the entry
loop and add, before `entries.push(entry)`:

```rust
        // `ACAD.EXE` reserves exactly `window` bytes above its image for the
        // code window and reads each region straight into it. A region ending
        // past that edge would write off the end of the allocation. Computed in
        // u32 because dest + len overflows u16 near the top.
        let code_end = entry.code.dest as u32 + entry.code.len as u32;
        if !entry.code.is_empty() && code_end > window as u32 {
            return Err(ReError::RegionPastWindow {
                entry: index,
                dest: entry.code.dest,
                end: code_end,
                window,
            });
        }
```

- [ ] **Step 4: Run and confirm green**

Run: `cargo test -p acad-re`
Expected: PASS, 9 unit tests plus the 5 corpus tests. The real overlay must still
parse — entry 2 ends exactly at the window edge, so a `>=` here would reject it.

- [ ] **Step 5: Add overlay block creation to the loader**

In `tools/ghidra/load_acad.py`, add before `check(...)`:

```python
def add_overlay_blocks(program, m, monitor):
    """One Ghidra overlay block per region.

    The 11 entries page into the same two windows, so their bytes cannot
    coexist in one flat address space. Ghidra overlay blocks are exactly this:
    several blocks claiming one address range, each in its own space.
    """
    from ghidra.program.model.mem import MemoryConflictException

    mem = program.getMemory()
    with open(m["ovl"], "rb") as fh:
        ovl = fh.read()

    made = 0
    for b in m["blocks"]:
        if not b["overlay"]:
            continue
        start = gc.seg_addr(program, b["seg"], b["off"])
        try:
            block = mem.createInitializedBlock(
                b["name"], start, b["len"], 0, monitor, True)
        except MemoryConflictException as exc:
            raise SystemExit("overlay block %s conflicts: %s" % (b["name"], exc))
        chunk = ovl[b["src_off"]:b["src_off"] + b["len"]]
        if len(chunk) != b["len"]:
            raise SystemExit("%s wants %d bytes at %#x, file has %d"
                             % (b["name"], b["len"], b["src_off"], len(chunk)))
        mem.setBytes(block.getStart(), chunk)
        block.setExecute(b["name"].endswith("_CODE"))
        made += 1
    print("created %d overlay blocks" % made)
    return made
```

Wire it into `load()`, after the segment registers are set:

```python
        from ghidra.util.task import ConsoleTaskMonitor
        made = add_overlay_blocks(program, m, ConsoleTaskMonitor())
        assert made == 22, "expected 22 overlay regions, made %d" % made
```

- [ ] **Step 6: Run the loader and verify the overlay bytes**

```bash
rm -rf build/ghidra
source tools/ghidra-env.sh
"$PYGHIDRA_PYTHON" tools/ghidra/load_acad.py build/ovl-map.json build/ghidra
```

Expected: `created 22 overlay blocks`, then the segment-layout line, then `loaded ACAD.EXE`.

Add a second self-check to `load_acad.py`'s `check()` proving the overlay bytes landed where the directory says. `!root` is at file offset `0x5e00`, which is entry 0's data region start, so it must appear at the start of `OVL00_DATA`:

```python
    block = gc.block_by_name(program, "OVL00_DATA")
    got = bytes(program.getMemory().getBytes(block.getStart(), 5))
    assert got == b"!root", "OVL00_DATA starts with %r, expected b'!root'" % got
    print("overlay payload verified: OVL00_DATA begins at the !root token")
```

Rerun and expect the new line.

- [ ] **Step 7: Commit**

```bash
cargo fmt --all && cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace
git add crates/acad-re tools/ghidra/load_acad.py
git commit -m "feat(re): 22 Ghidra overlay blocks, and reject overlapping regions"
```

---

### Task 5: Analysis and dual AST export

**Files:**
- Create: `tools/ghidra/export_ast.py`
- Create: `tools/re-pipeline.sh`

**Interfaces:**
- Consumes: the loaded program from Task 4.
- Produces: `build/ast-pcode.json` and `build/ast-clang.json`, each an object `{"functions": [...], "failures": [...]}` keyed by function address string `"seg:off"`; `tools/re-pipeline.sh` running the whole chain.

- [ ] **Step 1: Write the exporter**

`tools/ghidra/export_ast.py`. Per spec §6.3 both forms are exported, keyed by function address. Failures are recorded, never dropped — that is Review Focus item 5 and the §8 gate depends on it.

```python
#!/usr/bin/env python
"""Export both AST forms from the analysed program (spec §6.3).

  ast-pcode.json  high P-Code, SSA form, language-neutral
  ast-clang.json  Clang markup, a C AST whose nodes link back to P-Code varnodes

Functions Ghidra cannot decompile are recorded in `failures` with the reason.
Dropping them would inflate the §8 gate's success rate and decide it wrongly.
"""
import json
import os
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import ghidra_common as gc
import load_acad

gc.start()

BAD_MARKERS = ("halt_baddata", "UNRECOVERED_JUMPTABLE")


def varnode(vn):
    if vn is None:
        return None
    return {
        "space": vn.getAddress().getAddressSpace().getName(),
        "offset": vn.getOffset(),
        "size": vn.getSize(),
        "unique": vn.isUnique(),
    }


def pcode_ops(high):
    ops = []
    it = high.getPcodeOps()
    while it.hasNext():
        op = it.next()
        ops.append({
            "seq": str(op.getSeqnum().getTarget()),
            "op": op.getMnemonic(),
            "out": varnode(op.getOutput()),
            "in": [varnode(op.getInput(i)) for i in range(op.getNumInputs())],
        })
    return ops


def clang_nodes(markup):
    """Flatten the C AST, keeping each node's type and its P-Code linkage."""
    out = []

    def walk(node, depth):
        entry = {"depth": depth, "class": type(node).__name__, "text": str(node)}
        op = getattr(node, "getPcodeOp", lambda: None)()
        if op is not None:
            entry["pcode"] = str(op.getSeqnum().getTarget())
        out.append(entry)
        for i in range(node.numChildren()):
            walk(node.Child(i), depth + 1)

    walk(markup, 0)
    return out


def export(program, out_dir):
    from ghidra.app.decompiler import DecompInterface
    from ghidra.util.task import ConsoleTaskMonitor

    dec = DecompInterface()
    dec.toggleCCode(True)
    dec.toggleSyntaxTree(True)
    dec.openProgram(program)
    monitor = ConsoleTaskMonitor()

    pcode = {"functions": [], "failures": []}
    clang = {"functions": [], "failures": []}

    for func in program.getFunctionManager().getFunctions(True):
        key = str(func.getEntryPoint())
        block = program.getMemory().getBlock(func.getEntryPoint())
        common = {
            "address": key,
            "name": func.getName(),
            "block": block.getName() if block is not None else None,
        }
        res = dec.decompileFunction(func, 120, monitor)
        if not res.decompileCompleted():
            reason = res.getErrorMessage() or "decompile did not complete"
            pcode["failures"].append(dict(common, reason=reason))
            clang["failures"].append(dict(common, reason=reason))
            continue

        c_text = res.getDecompiledFunction().getC()
        markers = [m for m in BAD_MARKERS if m in c_text]

        high = res.getHighFunction()
        if high is None:
            pcode["failures"].append(dict(common, reason="no HighFunction"))
        else:
            pcode["functions"].append(dict(common, markers=markers, ops=pcode_ops(high)))

        markup = res.getCCodeMarkup()
        if markup is None:
            clang["failures"].append(dict(common, reason="no C markup"))
        else:
            clang["functions"].append(dict(common, markers=markers, nodes=clang_nodes(markup)))

    for name, doc in (("ast-pcode.json", pcode), ("ast-clang.json", clang)):
        path = os.path.join(out_dir, name)
        with open(path, "w") as fh:
            json.dump(doc, fh, indent=1)
        print("wrote %s (%d functions, %d failures)"
              % (path, len(doc["functions"]), len(doc["failures"])))
    return pcode, clang


if __name__ == "__main__":
    map_path, project_dir, out_dir = sys.argv[1], sys.argv[2], sys.argv[3]
    prog = load_acad.load(map_path, project_dir)
    export(prog, out_dir)
```

- [ ] **Step 2: Add analysis to the loader**

`load_acad.load()` currently passes `analyze=False`. Analysis must run after the overlay blocks exist, or Ghidra never sees the overlay code. Add, immediately before `check(program, ...)`:

```python
        api.analyzeAll(program)
        print("analysis complete: %d functions"
              % program.getFunctionManager().getFunctionCount())
```

- [ ] **Step 3: Write the pipeline driver**

`tools/re-pipeline.sh`:

```bash
#!/usr/bin/env bash
# tools/re-pipeline.sh — raw disk images to a typed AST, reproducibly.
set -euo pipefail
root="$(cd "$(dirname "$0")/.." && pwd)"
cd "$root"

[ -f corpus/System/ACAD.OVL ] || { echo "corpus missing; run ./tools/extract-corpus.sh" >&2; exit 1; }

mkdir -p build
# shellcheck source=/dev/null
source tools/ghidra-env.sh

cargo run --quiet -p acad-re --bin ovl-map -- \
  corpus/System/ACAD.EXE corpus/System/ACAD.OVL build/ovl-map.json

rm -rf build/ghidra
"$PYGHIDRA_PYTHON" tools/ghidra/export_ast.py build/ovl-map.json build/ghidra build

cargo run --quiet -p acad-re --bin re-report -- \
  build/ast-pcode.json build/ast-clang.json build/re-report.json
```

`re-report` does not exist until Task 8; until then the last command fails and that is expected.

- [ ] **Step 4: Run the export**

```bash
chmod +x tools/re-pipeline.sh
source tools/ghidra-env.sh
"$PYGHIDRA_PYTHON" tools/ghidra/export_ast.py build/ovl-map.json build/ghidra build
```

Expected: `analysis complete: N functions` where N is in the hundreds, then two `wrote build/ast-*.json` lines. Sanity-check that overlay functions are present, not just EXE ones:

```bash
python3 -c "
import json,collections
d=json.load(open('build/ast-pcode.json'))
print('functions', len(d['functions']), 'failures', len(d['failures']))
print(collections.Counter(f['block'] for f in d['functions']).most_common())"
```

Expected: blocks named `OVL00_CODE` … `OVL10_CODE` appear alongside `EXE_CODE`. If only `EXE_CODE` appears, analysis ran before the overlay blocks were created — check the ordering in Step 2.

- [ ] **Step 5: Commit**

```bash
git add tools/ghidra/export_ast.py tools/re-pipeline.sh
git commit -m "feat(re): dual AST export and end-to-end pipeline driver"
```

---

### Task 6: Typed Rust AST model

**Files:**
- Create: `crates/acad-re/src/ast.rs`
- Create: `crates/acad-re/tests/fixtures/ast-pcode-sample.json`
- Create: `crates/acad-re/tests/fixtures/ast-clang-sample.json`
- Create: `crates/acad-re/tests/ast.rs`
- Modify: `crates/acad-re/src/lib.rs`

**Interfaces:**
- Consumes: `build/ast-pcode.json`, `build/ast-clang.json` from Task 5.
- Produces:
  - `acad_re::ast::{PcodeExport, ClangExport, Function, Failure, PcodeOp, Varnode, ClangNode}`
  - `PcodeExport::from_json(&str) -> Result<PcodeExport, serde_json::Error>` and the same on `ClangExport`
  - `Function { address: String, name: String, block: Option<String>, markers: Vec<String>, .. }`
  - `PcodeExport::decompiled_cleanly(&self) -> impl Iterator<Item = &Function>` — functions with no markers

- [ ] **Step 1: Create the committed fixtures**

The full exports are tens of megabytes and gitignored, so the deserializer needs small committed inputs or it is untested on a fresh checkout. Cut them from the real export — never hand-write them, or they will drift from what Ghidra actually emits.

```bash
python3 - <<'PY'
import json
for kind in ("pcode", "clang"):
    d = json.load(open(f"build/ast-{kind}.json"))
    key = "ops" if kind == "pcode" else "nodes"
    # Two clean functions, one with a bad marker if any exists, and one failure.
    clean = [f for f in d["functions"] if not f["markers"]][:2]
    dirty = [f for f in d["functions"] if f["markers"]][:1]
    small = [dict(f, **{key: f[key][:6]}) for f in clean + dirty]
    out = {"functions": small, "failures": d["failures"][:1]}
    path = f"crates/acad-re/tests/fixtures/ast-{kind}-sample.json"
    json.dump(out, open(path, "w"), indent=1)
    print(path, len(small), "functions,", len(out["failures"]), "failures")
PY
```

Read both files afterwards. They are the contract between the Python and the Rust, so they must be inspected, not assumed.

- [ ] **Step 2: Write the failing deserialization tests**

`crates/acad-re/tests/ast.rs`:

```rust
use acad_re::ast::{ClangExport, PcodeExport};

const PCODE: &str = include_str!("fixtures/ast-pcode-sample.json");
const CLANG: &str = include_str!("fixtures/ast-clang-sample.json");

#[test]
fn pcode_export_deserializes() {
    let e = PcodeExport::from_json(PCODE).unwrap();
    assert!(!e.functions.is_empty());
    let f = &e.functions[0];
    assert!(!f.address.is_empty(), "every function is keyed by address");
    assert!(!f.body.ops.is_empty(), "a clean function has P-Code ops");
}

#[test]
fn clang_export_deserializes() {
    let e = ClangExport::from_json(CLANG).unwrap();
    assert!(!e.functions.is_empty());
    assert!(!e.functions[0].body.nodes.is_empty());
}

#[test]
fn failures_survive_the_round_trip() {
    // Review Focus 5: a dropped failure inflates the gate's success rate.
    let e = PcodeExport::from_json(PCODE).unwrap();
    assert_eq!(e.failures.len(), 1);
    assert!(!e.failures[0].reason.is_empty(), "a failure always says why");
}

#[test]
fn marked_functions_are_excluded_from_the_clean_set() {
    let e = PcodeExport::from_json(PCODE).unwrap();
    let clean: Vec<_> = e.decompiled_cleanly().collect();
    assert!(clean.iter().all(|f| f.markers.is_empty()));
    assert!(clean.len() <= e.functions.len());
}

#[test]
fn an_unknown_field_does_not_break_deserialization() {
    // The exporter will grow fields; older Rust must still read newer JSON.
    let json = r#"{"functions":[{"address":"1000:0003","name":"f","block":"EXE_CODE",
                   "markers":[],"ops":[],"future_field":42}],"failures":[]}"#;
    assert_eq!(PcodeExport::from_json(json).unwrap().functions.len(), 1);
}
```

- [ ] **Step 3: Run and confirm failure**

Run: `cargo test -p acad-re --test ast`
Expected: FAIL — `unresolved import `acad_re::ast``.

- [ ] **Step 4: Implement the model**

`crates/acad-re/src/ast.rs`:

```rust
//! The Ghidra export, as typed Rust. Mirrors `tools/ghidra/export_ast.py`;
//! the committed fixtures in `tests/fixtures/` are the contract between them.
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Varnode {
    pub space: String,
    pub offset: i64,
    pub size: u32,
    pub unique: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PcodeOp {
    pub seq: String,
    pub op: String,
    pub out: Option<Varnode>,
    #[serde(rename = "in")]
    pub inputs: Vec<Option<Varnode>>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ClangNode {
    pub depth: u32,
    pub class: String,
    pub text: String,
    /// The P-Code op this C AST node links back to, when it has one.
    #[serde(default)]
    pub pcode: Option<String>,
}

/// A function Ghidra could not decompile. Recorded, never dropped.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Failure {
    pub address: String,
    pub name: String,
    pub block: Option<String>,
    pub reason: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Function<Body> {
    pub address: String,
    pub name: String,
    pub block: Option<String>,
    /// `halt_baddata` / `UNRECOVERED_JUMPTABLE` found in the C text.
    #[serde(default)]
    pub markers: Vec<String>,
    #[serde(flatten)]
    pub body: Body,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PcodeBody {
    #[serde(default)]
    pub ops: Vec<PcodeOp>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ClangBody {
    #[serde(default)]
    pub nodes: Vec<ClangNode>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Export<Body> {
    #[serde(default)]
    pub functions: Vec<Function<Body>>,
    #[serde(default)]
    pub failures: Vec<Failure>,
}

pub type PcodeExport = Export<PcodeBody>;
pub type ClangExport = Export<ClangBody>;

impl<Body: serde::de::DeserializeOwned> Export<Body> {
    pub fn from_json(s: &str) -> Result<Self, serde_json::Error> {
        serde_json::from_str(s)
    }
    /// Functions with no `halt_baddata` and no `UNRECOVERED_JUMPTABLE` — the
    /// numerator of the §8 gate's 70% criterion.
    pub fn decompiled_cleanly(&self) -> impl Iterator<Item = &Function<Body>> {
        self.functions.iter().filter(|f| f.markers.is_empty())
    }
}
```

`#[serde(flatten)]` puts `ops` and `nodes` at the top level of the JSON, matching what
`export_ast.py` writes, while keeping them off `Function`'s own fields — which is why the tests
in Step 2 reach them through `f.body`.

Add to `crates/acad-re/src/lib.rs`:

```rust
pub mod ast;
```

- [ ] **Step 5: Run and confirm green**

Run: `cargo test -p acad-re`
Expected: PASS, all unit tests plus 5 AST tests plus the corpus tests.

- [ ] **Step 6: Commit**

```bash
cargo fmt --all && cargo clippy --workspace --all-targets -- -D warnings
git add crates/acad-re
git commit -m "feat(re): typed Rust model for the dual Ghidra AST export"
```

---

### Task 7: Call graph and overlay→command map

**Files:**
- Create: `crates/acad-re/src/analysis.rs`
- Modify: `crates/acad-re/src/lib.rs`
- Modify: `crates/acad-re/tests/ast.rs`
- Create: `crates/acad-re/tests/commands.rs`

**Interfaces:**
- Consumes: `PcodeExport` from Task 6, `Directory` from Task 1.
- Produces:
  - `acad_re::analysis::CallGraph` with `from_pcode(&PcodeExport) -> CallGraph`, `callers_of(&str) -> Vec<&str>`, `callees_of(&str) -> Vec<&str>`, `cross_overlay_edges(&self) -> Vec<(&str, &str)>`
  - `acad_re::analysis::commands(ovl_bytes: &[u8], dir: &Directory) -> BTreeMap<usize, Vec<String>>` — overlay index to the command names whose text lives in that entry's regions

- [ ] **Step 1: Write the failing call-graph tests**

Append to `crates/acad-re/tests/ast.rs`:

```rust
use acad_re::analysis::CallGraph;

#[test]
fn call_graph_links_caller_to_callee() {
    let json = r#"{"functions":[
      {"address":"1000:0003","name":"main","block":"EXE_CODE","markers":[],
       "ops":[{"seq":"1000:0011","op":"CALL","out":null,
               "in":[{"space":"ram","offset":65955,"size":2,"unique":false}]}]},
      {"address":"1000:01a3","name":"open_ovl","block":"EXE_CODE","markers":[],"ops":[]}
    ],"failures":[]}"#;
    let g = CallGraph::from_pcode(&acad_re::ast::PcodeExport::from_json(json).unwrap());
    assert_eq!(g.callees_of("1000:0003"), vec!["1000:01a3"]);
    assert_eq!(g.callers_of("1000:01a3"), vec!["1000:0003"]);
}

#[test]
fn a_call_to_an_unexported_address_is_dropped_not_invented() {
    // A CALL whose target is not a function we exported must not create a node,
    // or the graph grows phantom vertices that no analysis can explain.
    let json = r#"{"functions":[
      {"address":"1000:0003","name":"main","block":"EXE_CODE","markers":[],
       "ops":[{"seq":"1000:0011","op":"CALL","out":null,
               "in":[{"space":"ram","offset":9999999,"size":2,"unique":false}]}]}
    ],"failures":[]}"#;
    let g = CallGraph::from_pcode(&acad_re::ast::PcodeExport::from_json(json).unwrap());
    assert!(g.callees_of("1000:0003").is_empty());
}

#[test]
fn cross_overlay_edges_are_identified_by_block() {
    let json = r#"{"functions":[
      {"address":"2321:0100","name":"a","block":"OVL00_CODE","markers":[],
       "ops":[{"seq":"2321:0110","op":"CALL","out":null,
               "in":[{"space":"ram","offset":144400,"size":2,"unique":false}]}]},
      {"address":"2321:0200","name":"b","block":"OVL01_CODE","markers":[],"ops":[]}
    ],"failures":[]}"#;
    let g = CallGraph::from_pcode(&acad_re::ast::PcodeExport::from_json(json).unwrap());
    assert_eq!(g.cross_overlay_edges(), vec![("2321:0100", "2321:0200")]);
}
```

The `offset` values are the linear addresses Ghidra reports in varnodes, so they are `seg * 16 + off`:
`0x1000 * 16 + 0x01a3 = 65955` and `0x2321 * 16 + 0x0200 = 144400`. `linear()` in Step 3 computes the
same thing from the address string; if these two disagree the graph silently has no edges, which is
why the first test asserts a specific callee rather than a non-empty list.

- [ ] **Step 2: Run and confirm failure**

Run: `cargo test -p acad-re --test ast`
Expected: FAIL — `unresolved import `acad_re::analysis``.

- [ ] **Step 3: Implement the call graph**

`crates/acad-re/src/analysis.rs`:

```rust
//! Analyses over the exported AST: who calls whom, and which overlay owns
//! which command.
use crate::ast::{Function, PcodeBody, PcodeExport};
use crate::ovl::Directory;
use std::collections::{BTreeMap, BTreeSet};

/// A directed call graph over exported function addresses (`"seg:off"`).
#[derive(Debug, Default, Clone)]
pub struct CallGraph {
    edges: BTreeSet<(String, String)>,
    blocks: BTreeMap<String, String>,
}

/// `"1000:01a3"` -> `0x101a3`, the linear address Ghidra reports in varnodes.
fn linear(addr: &str) -> Option<i64> {
    let (seg, off) = addr.split_once(':')?;
    let seg = i64::from_str_radix(seg, 16).ok()?;
    let off = i64::from_str_radix(off, 16).ok()?;
    Some(seg * 16 + off)
}

impl CallGraph {
    pub fn from_pcode(export: &PcodeExport) -> Self {
        let by_linear: BTreeMap<i64, &str> = export
            .functions
            .iter()
            .filter_map(|f| linear(&f.address).map(|l| (l, f.address.as_str())))
            .collect();

        let mut graph = Self::default();
        for f in &export.functions {
            if let Some(b) = &f.block {
                graph.blocks.insert(f.address.clone(), b.clone());
            }
        }
        for f in &export.functions {
            for target in call_targets(f) {
                // Only edges to functions we actually exported. A CALL into
                // unanalysed bytes is a fact about Ghidra, not about the program.
                if let Some(callee) = by_linear.get(&target) {
                    graph.edges.insert((f.address.clone(), (*callee).to_string()));
                }
            }
        }
        graph
    }

    pub fn callees_of<'a>(&'a self, addr: &str) -> Vec<&'a str> {
        self.edges
            .iter()
            .filter(|(from, _)| from == addr)
            .map(|(_, to)| to.as_str())
            .collect()
    }

    pub fn callers_of<'a>(&'a self, addr: &str) -> Vec<&'a str> {
        self.edges
            .iter()
            .filter(|(_, to)| to == addr)
            .map(|(from, _)| from.as_str())
            .collect()
    }

    /// Edges whose endpoints live in different overlay blocks. The §8 gate asks
    /// whether these resolve to named targets; this is how they are counted.
    pub fn cross_overlay_edges(&self) -> Vec<(&str, &str)> {
        self.edges
            .iter()
            .filter(|(from, to)| match (self.blocks.get(from), self.blocks.get(to)) {
                (Some(a), Some(b)) => a != b && a.starts_with("OVL") && b.starts_with("OVL"),
                _ => false,
            })
            .map(|(from, to)| (from.as_str(), to.as_str()))
            .collect()
    }

    pub fn edge_count(&self) -> usize {
        self.edges.len()
    }
}

fn call_targets(f: &Function<PcodeBody>) -> Vec<i64> {
    f.body
        .ops
        .iter()
        .filter(|op| op.op == "CALL" || op.op == "CALLIND")
        .filter_map(|op| op.inputs.first().and_then(|v| v.as_ref()).map(|v| v.offset))
        .collect()
}
```

- [ ] **Step 4: Run the call-graph tests**

Run: `cargo test -p acad-re --test ast`
Expected: PASS, 8 tests.

- [ ] **Step 5: Write the failing command-recovery test**

Spec §4.3 gives the screen menu verbatim, so the recovered command set has a falsifiable lower bound: it must contain all 30 of them. `crates/acad-re/tests/commands.rs`:

```rust
use acad_re::{analysis, ovl};

fn corpus(rel: &str) -> Option<Vec<u8>> {
    match std::fs::read(format!("../../corpus/{rel}")) {
        Ok(b) => Some(b),
        Err(_) => {
            eprintln!("skipping: corpus/{rel} absent (run ./tools/extract-corpus.sh)");
            None
        }
    }
}

/// ACAD.MNU's screen menu, verbatim (spec §4.3). The full set is a superset.
const MENU: &[&str] = &[
    "LINE", "ARC", "CIRCLE", "TEXT", "INSERT", "MOVE", "COPY", "CHANGE", "FILLET", "ERASE",
    "OOPS", "BREAK", "REDRAW", "POINT", "TRACE", "SOLID", "ARRAY", "HATCH", "SKETCH", "DIM",
    "ZOOM", "LIST", "DIST", "AREA", "STATUS", "TABLET", "FILES", "PLOT", "LIMITS", "GRID",
];

#[test]
fn the_recovered_command_set_covers_the_screen_menu() {
    let Some(bytes) = corpus("System/ACAD.OVL") else { return };
    let dir = ovl::parse(&bytes).unwrap();
    let map = analysis::commands(&bytes, &dir);

    let found: std::collections::BTreeSet<&str> =
        map.values().flatten().map(String::as_str).collect();
    let missing: Vec<&str> = MENU.iter().copied().filter(|c| !found.contains(c)).collect();
    assert!(missing.is_empty(), "screen-menu commands not recovered: {missing:?}");
}

#[test]
fn the_full_set_is_a_superset_of_the_menu() {
    let Some(bytes) = corpus("System/ACAD.OVL") else { return };
    let dir = ovl::parse(&bytes).unwrap();
    let map = analysis::commands(&bytes, &dir);
    let total: usize = map.values().map(Vec::len).sum();
    assert!(total > MENU.len(), "recovered {total} commands, expected more than {}", MENU.len());
}

#[test]
fn every_command_is_attributed_to_an_overlay_that_exists() {
    let Some(bytes) = corpus("System/ACAD.OVL") else { return };
    let dir = ovl::parse(&bytes).unwrap();
    for entry in analysis::commands(&bytes, &dir).keys() {
        assert!(*entry < dir.entries.len(), "overlay {entry} is not in the directory");
    }
}
```

- [ ] **Step 6: Run and confirm failure**

Run: `cargo test -p acad-re --test commands`
Expected: FAIL — `cannot find function `commands` in module `analysis``.

- [ ] **Step 7: Implement command recovery**

Append to `crates/acad-re/src/analysis.rs`. Command names are NUL-terminated uppercase ASCII literals in the overlays' string pools; attributing each to the entry whose regions contain it gives the overlay→command map.

```rust
/// Uppercase ASCII words of 2..=8 characters, NUL-terminated, are how the
/// 1983 toolchain stored command names. Filtering on that shape and then on
/// which entry's regions contain the bytes gives the overlay→command map.
fn candidates(bytes: &[u8], from: usize, to: usize) -> Vec<String> {
    let mut out = Vec::new();
    let mut start = None;
    for i in from..to.min(bytes.len()) {
        let b = bytes[i];
        match (b.is_ascii_uppercase(), start) {
            (true, None) => start = Some(i),
            (true, Some(_)) => {}
            (false, Some(s)) => {
                // A command literal is NUL-terminated and preceded by a NUL or
                // by the start of the pool; anything else is a substring of
                // some longer message.
                let preceded = s == 0 || bytes[s - 1] == 0;
                if b == 0 && preceded && (2..=8).contains(&(i - s)) {
                    out.push(String::from_utf8_lossy(&bytes[s..i]).into_owned());
                }
                start = None;
            }
            (false, None) => {}
        }
    }
    out
}

/// Which overlay owns which command names.
pub fn commands(ovl_bytes: &[u8], dir: &Directory) -> BTreeMap<usize, Vec<String>> {
    let mut map: BTreeMap<usize, Vec<String>> = BTreeMap::new();
    for entry in &dir.entries {
        let mut names: BTreeSet<String> = BTreeSet::new();
        for region in [entry.code, entry.data] {
            if region.is_empty() {
                continue;
            }
            let from = region.file_off as usize;
            let to = region.end() as usize;
            names.extend(candidates(ovl_bytes, from, to));
        }
        if !names.is_empty() {
            map.insert(entry.index, names.into_iter().collect());
        }
    }
    map
}
```

Add `pub mod analysis;` to `crates/acad-re/src/lib.rs`.

- [ ] **Step 8: Run, then tighten if the menu is not covered**

Run: `cargo test -p acad-re --test commands -- --nocapture`

If `the_recovered_command_set_covers_the_screen_menu` fails, the filter is wrong, not the spec — §4.3 lists these names as verified string constants. Print what was found near a missing name and adjust the shape rule (the likely culprits are the length bounds and the preceding-NUL rule). Do not relax the test to match the code.

If `the_full_set_is_a_superset_of_the_menu` passes with an implausibly large number, the filter is too loose and is admitting ordinary uppercase words from message text; tighten it and record the final count in the commit message — that count is the "full command set recovered" deliverable.

- [ ] **Step 9: Commit**

```bash
cargo fmt --all && cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace
git add crates/acad-re
git commit -m "feat(re): call graph and overlay-to-command map"
```

---

### Task 8: Decision-gate report

**Files:**
- Create: `crates/acad-re/src/bin/re-report.rs`
- Modify: `crates/acad-re/src/analysis.rs`
- Modify: `crates/acad-re/tests/ast.rs`
- Modify: `docs/re-pipeline.md`

**Interfaces:**
- Consumes: everything above.
- Produces: `acad_re::analysis::Gate` with `measure(&PcodeExport, &CallGraph) -> Gate` and fields `total`, `clean`, `marked`, `failed`, `clean_ratio`, `cross_overlay_edges`, `cross_overlay_named`; binary `re-report` writing `build/re-report.json` and a human summary to stdout.

- [ ] **Step 1: Write the failing gate tests**

Append to `crates/acad-re/tests/ast.rs`:

```rust
use acad_re::analysis::Gate;

#[test]
fn the_gate_counts_failures_against_the_total() {
    // Review Focus 5 again, at the level that decides the milestone: a function
    // that failed to decompile is neither clean nor absent — it is a denominator.
    let json = r#"{"functions":[
      {"address":"1000:0003","name":"a","block":"EXE_CODE","markers":[],"ops":[]},
      {"address":"1000:0100","name":"b","block":"EXE_CODE","markers":["halt_baddata"],"ops":[]}
    ],"failures":[{"address":"1000:0200","name":"c","block":"EXE_CODE","reason":"timeout"}]}"#;
    let e = acad_re::ast::PcodeExport::from_json(json).unwrap();
    let g = Gate::measure(&e, &CallGraph::from_pcode(&e));
    assert_eq!(g.total, 3, "two exported plus one failed");
    assert_eq!(g.clean, 1);
    assert_eq!(g.marked, 1);
    assert_eq!(g.failed, 1);
    assert!((g.clean_ratio - 1.0 / 3.0).abs() < 1e-9);
}

#[test]
fn an_empty_export_does_not_divide_by_zero() {
    let e = acad_re::ast::PcodeExport::from_json(r#"{"functions":[],"failures":[]}"#).unwrap();
    let g = Gate::measure(&e, &CallGraph::from_pcode(&e));
    assert_eq!(g.total, 0);
    assert_eq!(g.clean_ratio, 0.0);
}
```

- [ ] **Step 2: Run and confirm failure**

Run: `cargo test -p acad-re --test ast`
Expected: FAIL — `cannot find type `Gate``.

- [ ] **Step 3: Implement the gate measurement**

Append to `crates/acad-re/src/analysis.rs`:

```rust
use serde::Serialize;

/// The §8 decision-gate metrics. Transpilation of the AST to Rust is adopted
/// only if `clean_ratio >= 0.70`, cross-overlay calls resolve to named targets,
/// and the DWG entity record is recoverable as a coherent struct. This type
/// measures the first two; the third is a judgement recorded alongside.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Gate {
    /// Exported functions plus functions that failed to decompile.
    pub total: usize,
    /// Decompiled with no `halt_baddata` and no `UNRECOVERED_JUMPTABLE`.
    pub clean: usize,
    pub marked: usize,
    pub failed: usize,
    pub clean_ratio: f64,
    pub cross_overlay_edges: usize,
    /// Cross-overlay edges whose callee has a name Ghidra assigned, rather
    /// than a bare `FUN_` placeholder.
    pub cross_overlay_named: usize,
}

impl Gate {
    pub fn measure(export: &PcodeExport, graph: &CallGraph) -> Self {
        let clean = export.decompiled_cleanly().count();
        let marked = export.functions.len() - clean;
        let failed = export.failures.len();
        let total = export.functions.len() + failed;
        let named: BTreeMap<&str, &str> = export
            .functions
            .iter()
            .map(|f| (f.address.as_str(), f.name.as_str()))
            .collect();
        let edges = graph.cross_overlay_edges();
        let cross_overlay_named = edges
            .iter()
            .filter(|(_, to)| named.get(to).is_some_and(|n| !n.starts_with("FUN_")))
            .count();
        Self {
            total,
            clean,
            marked,
            failed,
            clean_ratio: if total == 0 { 0.0 } else { clean as f64 / total as f64 },
            cross_overlay_edges: edges.len(),
            cross_overlay_named,
        }
    }

    /// The §8 threshold on decompiler quality.
    pub fn meets_decompile_threshold(&self) -> bool {
        self.clean_ratio >= 0.70
    }
}
```

- [ ] **Step 4: Run and confirm green**

Run: `cargo test -p acad-re --test ast`
Expected: PASS, 10 tests.

- [ ] **Step 5: Write the report binary**

`crates/acad-re/src/bin/re-report.rs`:

```rust
//! Measures the §8 decision gate from the exported AST.
use acad_re::{
    analysis::{CallGraph, Gate},
    ast::{ClangExport, PcodeExport},
};
use std::{fs, process::ExitCode};

fn main() -> ExitCode {
    let mut args = std::env::args().skip(1);
    let (Some(pcode), Some(clang), Some(out)) = (args.next(), args.next(), args.next()) else {
        eprintln!("usage: re-report <ast-pcode.json> <ast-clang.json> <out.json>");
        return ExitCode::FAILURE;
    };

    let pcode = match fs::read_to_string(&pcode).map_err(|e| e.to_string())
        .and_then(|s| PcodeExport::from_json(&s).map_err(|e| e.to_string()))
    {
        Ok(e) => e,
        Err(e) => {
            eprintln!("{pcode}: {e}");
            return ExitCode::FAILURE;
        }
    };
    let clang = match fs::read_to_string(&clang).map_err(|e| e.to_string())
        .and_then(|s| ClangExport::from_json(&s).map_err(|e| e.to_string()))
    {
        Ok(e) => e,
        Err(e) => {
            eprintln!("{clang}: {e}");
            return ExitCode::FAILURE;
        }
    };

    let graph = CallGraph::from_pcode(&pcode);
    let gate = Gate::measure(&pcode, &graph);

    println!("functions        {} ({} clean, {} marked, {} failed)",
             gate.total, gate.clean, gate.marked, gate.failed);
    println!("clean ratio      {:.1}%  (§8 threshold 70%: {})",
             gate.clean_ratio * 100.0,
             if gate.meets_decompile_threshold() { "MET" } else { "NOT MET" });
    println!("call edges       {}", graph.edge_count());
    println!("cross-overlay    {} edges, {} to named targets",
             gate.cross_overlay_edges, gate.cross_overlay_named);
    println!("clang AST        {} functions, {} failures",
             clang.functions.len(), clang.failures.len());

    match fs::write(&out, serde_json::to_string_pretty(&gate).unwrap()) {
        Ok(()) => {
            println!("wrote {out}");
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("cannot write {out}: {e}");
            ExitCode::FAILURE
        }
    }
}
```

- [ ] **Step 6: Run the whole pipeline end to end**

```bash
rm -rf build/ghidra build/ast-*.json build/re-report.json
./tools/re-pipeline.sh
```

Expected: `ovl-map.json` written, 22 overlay blocks created, both self-checks pass, analysis completes, both AST files written, then the gate summary. Record the printed numbers — they are the milestone's headline result.

- [ ] **Step 7: Verify reproducibility from the raw images**

Spec §8 says the milestone is done when *the pipeline reproduces from raw images on a clean machine*. Prove it:

```bash
rm -rf corpus build
./tools/extract-corpus.sh
./tools/re-pipeline.sh
```

Expected: identical gate numbers. If they differ, something outside version control is leaking in — find it before claiming the milestone.

- [ ] **Step 8: Record the result**

Append to `docs/re-pipeline.md` a "Results" section with the date, the Ghidra version, and the gate numbers as printed. Then state plainly whether the §8 gate is met on each of its three criteria, with the third (DWG entity record recoverable as a coherent struct) answered from the AST by inspection and written out as a short paragraph with the function addresses that support it.

- [ ] **Step 9: Confirm the whole workspace is green**

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

Expected: all green. Confirm the corpus-absent path too:

```bash
mv corpus /tmp/corpus-hold && cargo test --workspace; mv /tmp/corpus-hold corpus
```

Expected: still green, with skip messages.

- [ ] **Step 10: Commit**

```bash
git add crates/acad-re docs/re-pipeline.md
git commit -m "feat(re): decision-gate report and milestone ② results"
```

---

### Task 9: DWG record data flow and the geometry algorithms

Spec §6.4 requires `acad-re` to answer four questions. Tasks 6 and 7 answer two — the call
graph and the overlay→command mapping. This task answers the other two: **the DWG record
writer's data flow** and **the geometry algorithms (arc tessellation and `FILLET`)**. The
first is also the §8 gate's third criterion, so the milestone cannot be called done without it.

The reusable analysis is fully specified and tested against synthetic P-Code. Which function
writes DWG records is not yet known — it is discovered here, and the discovery is then pinned
by a regression test so it cannot silently rot.

**Files:**
- Modify: `crates/acad-re/src/analysis.rs`
- Modify: `crates/acad-re/tests/ast.rs`
- Create: `crates/acad-re/tests/dataflow.rs`
- Modify: `docs/re-pipeline.md`

**Interfaces:**
- Consumes: `PcodeExport` (Task 6), `CallGraph` and `commands` (Task 7).
- Produces:
  - `acad_re::analysis::StoreSite { seq: String, offset: Option<i64>, size: u32 }`
  - `acad_re::analysis::trace_stores(&PcodeExport, addr: &str) -> Vec<StoreSite>` — the
    `STORE` ops of one function in P-Code order, each with its constant offset when it has one
  - `acad_re::analysis::record_layout(&[StoreSite]) -> Vec<(i64, u32)>` — offset/size pairs,
    deduplicated and sorted, i.e. the struct as the writer lays it out

- [ ] **Step 1: Write the failing data-flow tests**

Append to `crates/acad-re/tests/ast.rs`:

```rust
use acad_re::analysis::{record_layout, trace_stores, StoreSite};

#[test]
fn stores_are_returned_in_pcode_order_with_their_offsets() {
    let json = r#"{"functions":[
      {"address":"1000:0003","name":"w","block":"EXE_CODE","markers":[],"ops":[
        {"seq":"1000:0010","op":"STORE","out":null,
         "in":[{"space":"const","offset":0,"size":4,"unique":false},
               {"space":"const","offset":0,"size":2,"unique":false},
               {"space":"register","offset":0,"size":2,"unique":false}]},
        {"seq":"1000:0018","op":"COPY","out":null,"in":[]},
        {"seq":"1000:0020","op":"STORE","out":null,
         "in":[{"space":"const","offset":0,"size":4,"unique":false},
               {"space":"const","offset":2,"size":2,"unique":false},
               {"space":"register","offset":0,"size":8,"unique":false}]}
      ]}],"failures":[]}"#;
    let e = acad_re::ast::PcodeExport::from_json(json).unwrap();
    let sites = trace_stores(&e, "1000:0003");
    assert_eq!(
        sites,
        vec![
            StoreSite { seq: "1000:0010".into(), offset: Some(0), size: 2 },
            StoreSite { seq: "1000:0020".into(), offset: Some(2), size: 8 },
        ]
    );
}

#[test]
fn a_store_through_a_computed_pointer_has_no_constant_offset() {
    // The offset is only meaningful when the pointer operand is a constant.
    // Reporting a register's number as a struct offset would invent a layout.
    let json = r#"{"functions":[
      {"address":"1000:0003","name":"w","block":"EXE_CODE","markers":[],"ops":[
        {"seq":"1000:0010","op":"STORE","out":null,
         "in":[{"space":"const","offset":0,"size":4,"unique":false},
               {"space":"register","offset":8,"size":2,"unique":false},
               {"space":"register","offset":0,"size":2,"unique":false}]}
      ]}],"failures":[]}"#;
    let e = acad_re::ast::PcodeExport::from_json(json).unwrap();
    assert_eq!(trace_stores(&e, "1000:0003")[0].offset, None);
}

#[test]
fn an_unknown_function_address_traces_to_nothing() {
    let e = acad_re::ast::PcodeExport::from_json(r#"{"functions":[],"failures":[]}"#).unwrap();
    assert!(trace_stores(&e, "1000:9999").is_empty());
}

#[test]
fn record_layout_sorts_and_deduplicates_offsets() {
    // A writer may store the same field twice; the struct still has one slot.
    let sites = vec![
        StoreSite { seq: "a".into(), offset: Some(4), size: 8 },
        StoreSite { seq: "b".into(), offset: Some(0), size: 2 },
        StoreSite { seq: "c".into(), offset: Some(4), size: 8 },
        StoreSite { seq: "d".into(), offset: None, size: 2 },
    ];
    assert_eq!(record_layout(&sites), vec![(0, 2), (4, 8)]);
}
```

- [ ] **Step 2: Run and confirm failure**

Run: `cargo test -p acad-re --test ast`
Expected: FAIL — `cannot find function `trace_stores` in module `analysis``.

- [ ] **Step 3: Implement the data-flow analysis**

Append to `crates/acad-re/src/analysis.rs`. In Ghidra's P-Code a `STORE` takes three inputs:
the address space id (a constant), the pointer, and the value. A constant pointer is a struct
offset; anything else is computed and says nothing about the layout.

```rust
/// One `STORE` in a function, as the writer emitted it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct StoreSite {
    pub seq: String,
    /// The pointer operand, when it is a constant — i.e. a field offset.
    pub offset: Option<i64>,
    /// Bytes written: 2 for a 16-bit int, 8 for an IEEE double.
    pub size: u32,
}

/// Every `STORE` in one function, in P-Code order.
pub fn trace_stores(export: &PcodeExport, addr: &str) -> Vec<StoreSite> {
    let Some(f) = export.functions.iter().find(|f| f.address == addr) else {
        return Vec::new();
    };
    f.body
        .ops
        .iter()
        .filter(|op| op.op == "STORE")
        .filter_map(|op| {
            let ptr = op.inputs.get(1)?.as_ref()?;
            let val = op.inputs.get(2)?.as_ref()?;
            Some(StoreSite {
                seq: op.seq.clone(),
                offset: (ptr.space == "const").then_some(ptr.offset),
                size: val.size,
            })
        })
        .collect()
}

/// The record as the writer lays it out: offset/size pairs, sorted, deduplicated.
/// Stores through computed pointers are excluded — they carry no layout information.
pub fn record_layout(sites: &[StoreSite]) -> Vec<(i64, u32)> {
    let mut fields: Vec<(i64, u32)> =
        sites.iter().filter_map(|s| s.offset.map(|o| (o, s.size))).collect();
    fields.sort_unstable();
    fields.dedup();
    fields
}
```

- [ ] **Step 4: Run and confirm green**

Run: `cargo test -p acad-re --test ast`
Expected: PASS, 14 tests.

- [ ] **Step 5: Discover the DWG record writer and the geometry functions**

This step produces findings, not code. Run the pipeline first (`./tools/re-pipeline.sh`), then
work from `build/ast-pcode.json` and `build/ast-clang.json`.

The evidence available, in order of strength:

   All `DS:` offsets below are verified against the bytes; `DS:0000` is file offset `0xE350`.

1. **`ACAD.EXE`'s own strings** name the writer's neighbourhood: `EREAD` at `DS:0x36B3`,
   `EWRITE` at `DS:0x36B9`, `EWRITE error` at `DS:0x36CC`,
   `** ILLEGAL TCODE VALUE %d IN %s` at `DS:0x36D9`, and
   `BAD ENTITY TYPE %d PASSED TO EREGEN` at `DS:0x36FB`. Find the functions referencing them;
   `EWRITE` is the entity writer. `MWRITE failed: %d,%ld` at `DS:0x3B3F` is the layer below —
   the raw page write, not the record encoder.
2. **The entity type table.** `DS:0x38EE` holds `LINE POINT CIRCLE SHAPE REPEAT ENDREP TEXT ARC
   TRACE LOAD SOLID BLOCK ENDBLK INSERT` as consecutive NUL-terminated literals — the `TCODE`
   dispatch order, and the writer switches on it. Do not confuse it with the *display* names at
   `DS:0x386B` (`REPEAT start`, `BLOCK DEFINITION`, …), which are for `LIST` output.
3. **Geometry.** `Improper argument (%g) to SQRT.` is at `DS:0x3942` and `ACOS undefined for %g`
   at `DS:0x397A`. Arc tessellation is among their callers. `FILLET` is reachable from the
   command map (Task 7) via the entry that owns the `FILLET` literal.

Write a short throwaway script against the JSON to list, for each candidate, its address,
block, and `record_layout(&trace_stores(..))`. Record in `docs/re-pipeline.md`:

- the address and block of the DWG entity writer;
- its `record_layout` output as an offset/size table, read as a struct;
- whether that table is coherent — fields at increasing offsets, no overlaps, sizes of 2 or 8
  (16-bit ints and IEEE doubles, per spec §10's note that DWG holds IEEE doubles);
- the addresses of the arc tessellation and `FILLET` functions, with a paragraph each on what
  the decompiled C does.

The coherence judgement is the §8 gate's third criterion. State it as a yes or a no, with the
offset table as the evidence, not as an impression.

- [ ] **Step 6: Pin the findings with a regression test**

`crates/acad-re/tests/dataflow.rs`. Replace the three `const` addresses with the ones found in
Step 5 — the test is worthless until they are real, and it is the thing that stops a later
Ghidra upgrade quietly changing the answer.

```rust
use acad_re::analysis::{record_layout, trace_stores};
use acad_re::ast::PcodeExport;

/// Addresses recorded in docs/re-pipeline.md, from the milestone ② analysis.
const DWG_ENTITY_WRITER: &str = "REPLACE_IN_STEP_5";
const ARC_TESSELLATION: &str = "REPLACE_IN_STEP_5";
const FILLET: &str = "REPLACE_IN_STEP_5";

/// The full export is gitignored — it is regenerable and tens of megabytes.
fn export() -> Option<PcodeExport> {
    match std::fs::read_to_string("../../build/ast-pcode.json") {
        Ok(s) => Some(PcodeExport::from_json(&s).unwrap()),
        Err(_) => {
            eprintln!("skipping: build/ast-pcode.json absent (run ./tools/re-pipeline.sh)");
            None
        }
    }
}

#[test]
fn the_three_analysed_functions_are_still_present_and_clean() {
    let Some(e) = export() else { return };
    for addr in [DWG_ENTITY_WRITER, ARC_TESSELLATION, FILLET] {
        let f = e.functions.iter().find(|f| f.address == addr);
        let f = f.unwrap_or_else(|| panic!("{addr} is no longer in the export"));
        assert!(f.markers.is_empty(), "{addr} now decompiles with {:?}", f.markers);
    }
}

#[test]
fn the_dwg_entity_record_is_a_coherent_struct() {
    // The §8 gate's third criterion, as an assertion rather than an impression:
    // fields at distinct increasing offsets, none overlapping its successor,
    // every size a 16-bit int or an IEEE double.
    let Some(e) = export() else { return };
    let layout = record_layout(&trace_stores(&e, DWG_ENTITY_WRITER));
    assert!(!layout.is_empty(), "the entity writer stores no constant-offset fields");
    for w in layout.windows(2) {
        let ((off, size), (next, _)) = (w[0], w[1]);
        assert!(off + size as i64 <= next, "field at {off} (size {size}) overlaps {next}");
    }
    for (off, size) in &layout {
        assert!(matches!(size, 2 | 8), "field at {off} has size {size}, not 2 or 8");
    }
}
```

- [ ] **Step 7: Run both the pinned test and the whole suite**

```bash
cargo test -p acad-re --test dataflow -- --nocapture
cargo test --workspace
```

Expected: PASS with `build/ast-pcode.json` present, skip messages without it.

If `the_dwg_entity_record_is_a_coherent_struct` fails, that is a **result, not a bug** — it
means the §8 gate's third criterion is not met and transpilation is off the table. Record the
failing layout in `docs/re-pipeline.md`, mark the test `#[ignore]` with a comment pointing at
that section, and say so plainly in the commit message. Do not weaken the assertions to make
it pass.

- [ ] **Step 8: Commit**

```bash
cargo fmt --all && cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace
git add crates/acad-re docs/re-pipeline.md
git commit -m "feat(re): DWG record data flow and geometry analyses"
```

---

## Milestone ② exit criteria

- `cargo test --workspace` passes, with and without the corpus present.
- `./tools/re-pipeline.sh` runs from a clean `corpus/` and `build/` and produces `ast-pcode.json`, `ast-clang.json` and `re-report.json`.
- The Ghidra program has 2 flat blocks and 22 overlay blocks; both loader self-checks pass.
- The recovered command set contains all 30 screen-menu commands and is a strict superset.
- `acad-re` answers all four §6.4 questions: call graph, overlay→command map, the DWG record
  writer's data flow, and the arc tessellation and `FILLET` algorithms.
- `docs/re-pipeline.md` records the gate numbers, the DWG entity record's offset/size table, and
  states whether each of the three §8 criteria is met — including a plain no where one is not.

The §8 decision gate is then settled on those numbers, and milestone ③ (oracle harness) gets its own plan from the same spec.
