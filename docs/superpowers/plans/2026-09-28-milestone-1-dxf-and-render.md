# Milestone ① — DXF Codec, Corpus Integrity, First Render — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Load `SUBDIV.DXF` from the 1983 AutoCAD 1.4 sample disk, round-trip it byte-identically, and render it in a window.

**Architecture:** Four crates in a workspace. `acad-model` holds the entity model every other crate depends on. `acad-dxf` lexes the 1983 record format into `Record`s, then parses them into a `Drawing` and writes them back. `acad-render` flattens a `Drawing` into screen-space polylines (numerically testable) and rasterizes them separately (thin). `acad-app` puts the pixmap in a window. No reverse engineering is required by this milestone.

**Tech Stack:** Rust 1.88.0, `tiny-skia` 0.11 (CPU rasterizer — deterministic, so rendering is testable headless), `winit` 0.30 + `softbuffer` 0.4 (window), `sha2` 0.10 (corpus manifest), `toml` 0.8.

**Spec:** `docs/superpowers/specs/2026-09-28-autocad-14-rust-design.md`

## Global Constraints

- Rust edition 2021, toolchain 1.88.0.
- `acad-model` is the only crate other crates may universally depend on. 1983 policy lives in `acad-dxf`; never in `acad-model`.
- `acad-re` and `acad-oracle` are dev-only and out of scope for this milestone — do not create them.
- Corrupt corpus files are excluded **explicitly and by name**, never silently.
- The corpus itself is gitignored (`autocad/`, `corpus/`); the SHA-256 manifest **is** committed — it is the reproducibility record.
- DXF files are CRLF-terminated and end with a `0x1A` DOS EOF byte; bytes after `0x1A` are cluster slack and are not content.
- DXF text is Latin-1, not UTF-8. Decode explicitly.
- TDD: every step writes the failing test before the implementation.

### Record layouts (verified against `SUBDIV.DXF`)

Format is `KEYWORD,<instance-count>` followed by `instance-count × rows_per_instance(KEYWORD)` rows.

| Keyword | rows/inst | Row layout |
|---|---|---|
| `EXTENTS` `LIMITS` | 1 | `xmin,xmax,ymin,ymax` |
| `BASE` | 1 | `x,y` |
| `DWGVIEW` | 1 | `center_x,center_y,view_height` |
| `MODERES` `MODEGRID` | 1 | `flag,spacing` |
| `MODEORTHO` `MODEFILL` | 1 | `flag` |
| `TXTSIZE` `TRACEWID` | 1 | `value` |
| `LAYER` | 1 | `current_layer` |
| `LAYERC` | 8 | 16 colors per row, 128 total, `255` = unused |
| `LINE` | 1 | `x1,y1,x2,y2` |
| `CIRCLE` | 1 | `cx,cy,radius` |
| `ARC` | 1 | `cx,cy,radius,start_deg,end_deg` |
| `TEXT` | 2 | `x,y,height,rotation` then the string |
| `INSERT` | 2 | `x,y,xscale,yscale,rotation` then block name |
| `BLOCK` | 2 | `base_x,base_y` then block name |
| `ENDBLK` | 0 | — |

**`POINT`, `TRACE`, `SOLID` and `SHAPE` are out of scope for this milestone.** They exist as string constants in `ACAD.OVL` but appear in no readable sample, so their layouts are unverifiable until milestone ② or ③. The lexer must reject them with a clear error rather than guess.

## Review Focus

1. **Truncated or corrupt file mid-record** — `SHUTTLE.DXF` dies at byte 1536; the parser must return an error naming the byte offset, never a partial `Drawing`. *(Task 3)*
2. **`ARC` wrapping through 0°** — `SUBDIV` has start 78.604° → end 22.451°; sweep is counter-clockwise *through* 0, i.e. 303.8°, not a negative 56°. *(Task 6)*
3. **Instance count > 1** — every record in the corpus has count `1`; a count of `3` must consume `3 × rows_per_instance` rows and yield 3 entities, not 1. *(Task 3)*
4. **`INSERT` naming an undefined block** — must be a named error, not a panic or a silently dropped entity. *(Task 4)*
5. **Degenerate extents** — an empty drawing, or one holding a single point, gives a zero-width bounding box; the viewport must not divide by zero. *(Task 5)*

---

### Task 1: Workspace and entity model

**Files:**
- Create: `Cargo.toml`, `crates/acad-model/Cargo.toml`
- Create: `crates/acad-model/src/lib.rs`, `src/geom.rs`, `src/entity.rs`, `src/header.rs`, `src/drawing.rs`
- Test: `crates/acad-model/src/geom.rs` (inline `#[cfg(test)]`)

**Interfaces:**
- Consumes: nothing.
- Produces: `Point{x:f64,y:f64}`, `Extents{xmin,xmax,ymin,ymax:f64}` with `fn width(&self)->f64`, `fn height(&self)->f64`, `fn is_degenerate(&self)->bool`; `Entity` enum; `Block{name:String,base:Point,entities:Vec<Entity>}`; `enum Item{Entity(Entity),Block(Block)}`; `Header`; `Drawing{header:Header,items:Vec<Item>}` with `entities()`, `blocks()` and `block(name)` accessors.

- [x] **Step 1: Create the workspace**

```toml
# Cargo.toml
[workspace]
members = ["crates/acad-model", "crates/acad-dxf", "crates/acad-render", "crates/acad-app", "crates/acad-corpus"]
resolver = "2"

[workspace.package]
edition = "2021"
rust-version = "1.88"

[workspace.dependencies]
acad-model = { path = "crates/acad-model" }
acad-dxf = { path = "crates/acad-dxf" }
acad-render = { path = "crates/acad-render" }
```

```toml
# crates/acad-model/Cargo.toml
[package]
name = "acad-model"
version = "0.1.0"
edition.workspace = true
rust-version.workspace = true
```

- [x] **Step 2: Write the failing test for `Extents`**

```rust
// crates/acad-model/src/geom.rs
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extents_dimensions_from_subdiv_limits() {
        let e = Extents { xmin: -2.0, xmax: 19.0, ymin: -2.0, ymax: 14.0 };
        assert_eq!(e.width(), 21.0);
        assert_eq!(e.height(), 16.0);
        assert!(!e.is_degenerate());
    }

    #[test]
    fn single_point_extents_are_degenerate() {
        let e = Extents { xmin: 3.0, xmax: 3.0, ymin: 4.0, ymax: 4.0 };
        assert!(e.is_degenerate());
    }
}
```

- [x] **Step 3: Run it and confirm it fails**

Run: `cargo test -p acad-model`
Expected: FAIL — `cannot find type Extents in this scope`.

- [x] **Step 4: Implement the geometry types**

```rust
// crates/acad-model/src/geom.rs
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Point { pub x: f64, pub y: f64 }

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Extents { pub xmin: f64, pub xmax: f64, pub ymin: f64, pub ymax: f64 }

impl Extents {
    pub fn width(&self) -> f64 { self.xmax - self.xmin }
    pub fn height(&self) -> f64 { self.ymax - self.ymin }
    pub fn is_degenerate(&self) -> bool { self.width() <= 0.0 || self.height() <= 0.0 }
}
```

- [x] **Step 5: Run it and confirm it passes**

Run: `cargo test -p acad-model`
Expected: PASS, 2 tests.

- [x] **Step 6: Add the entity, header and drawing types**

```rust
// crates/acad-model/src/entity.rs
use crate::geom::Point;

/// Angles are degrees, counter-clockwise, as stored by AutoCAD 1.4.
#[derive(Debug, Clone, PartialEq)]
pub enum Entity {
    Line { start: Point, end: Point },
    Circle { center: Point, radius: f64 },
    Arc { center: Point, radius: f64, start_deg: f64, end_deg: f64 },
    Text { origin: Point, height: f64, rotation_deg: f64, value: String },
    Insert { origin: Point, x_scale: f64, y_scale: f64, rotation_deg: f64, name: String },
}

#[derive(Debug, Clone, PartialEq)]
pub struct Block { pub name: String, pub base: Point, pub entities: Vec<Entity> }
```

```rust
// crates/acad-model/src/header.rs
use crate::geom::{Extents, Point};

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Mode { pub on: bool, pub spacing: f64 }

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DwgView { pub center: Point, pub height: f64 }

#[derive(Debug, Clone, PartialEq)]
pub struct Header {
    pub extents: Extents,
    pub limits: Extents,
    pub base: Point,
    pub view: DwgView,
    pub snap: Mode,
    pub grid: Mode,
    pub ortho: bool,
    pub fill: bool,
    pub text_size: f64,
    pub trace_width: f64,
    pub current_layer: u8,
    pub layer_colors: [u8; 128],
}
```

```rust
// crates/acad-model/src/drawing.rs
use crate::{entity::{Block, Entity}, header::Header};

/// A 1983 DXF file is a flat sequence of records in which block definitions and
/// loose entities are interleaved. `SUBDIV.DXF` has 47 lines before its first
/// block and more blocks between its inserts, so document order is content and
/// the model preserves it rather than bucketing by kind.
#[derive(Debug, Clone, PartialEq)]
pub enum Item { Entity(Entity), Block(Block) }

#[derive(Debug, Clone, PartialEq)]
pub struct Drawing { pub header: Header, pub items: Vec<Item> }

impl Drawing {
    pub fn entities(&self) -> impl Iterator<Item = &Entity> {
        self.items.iter().filter_map(|i| match i { Item::Entity(e) => Some(e), _ => None })
    }
    pub fn blocks(&self) -> impl Iterator<Item = &Block> {
        self.items.iter().filter_map(|i| match i { Item::Block(b) => Some(b), _ => None })
    }
    pub fn block(&self, name: &str) -> Option<&Block> {
        self.blocks().find(|b| b.name == name)
    }
}
```

```rust
// crates/acad-model/src/lib.rs
pub mod drawing;
pub mod entity;
pub mod geom;
pub mod header;

pub use drawing::{Drawing, Item};
pub use entity::{Block, Entity};
pub use geom::{Extents, Point};
pub use header::{DwgView, Header, Mode};
```

- [x] **Step 7: Verify and commit**

Run: `cargo test -p acad-model` → PASS.

```bash
git add Cargo.toml crates/acad-model
git commit -m "feat(model): entity model, header and geometry types"
```

---

### Task 2: Corpus extraction and integrity manifest

**Files:**
- Create: `tools/extract-corpus.sh`
- Create: `crates/acad-corpus/Cargo.toml`, `crates/acad-corpus/src/main.rs`, `src/check.rs`
- Create: `corpus/manifest.toml` (generated, committed)
- Modify: `.gitignore` (add `corpus/*` but not `corpus/manifest.toml`)
- Test: `crates/acad-corpus/src/check.rs` (inline)

**Interfaces:**
- Consumes: nothing.
- Produces: `fn classify(name: &str, bytes: &[u8]) -> Verdict` where `enum Verdict { Ok, CorruptAt(usize), Binary }`.

- [x] **Step 1: Write the extraction script**

```bash
#!/usr/bin/env bash
# tools/extract-corpus.sh — rebuilds corpus/ from the archives in autocad/
set -euo pipefail
root="$(cd "$(dirname "$0")/.." && pwd)"
out="$root/corpus"
rm -rf "$out"; mkdir -p "$out/raw"
7z x "$root/autocad/Autodesk AutoCAD 1.4 (5.25).7z" -o"$out/raw" -y >/dev/null
for img in "$out/raw"/*/*.img; do
  name="$(basename "$img" .img)"
  mkdir -p "$out/$name"
  7z x "$img" -o"$out/$name" -y >/dev/null
done
echo "corpus extracted to $out"
```

Make it executable: `chmod +x tools/extract-corpus.sh`

- [x] **Step 2: Write the failing corruption-detection test**

The detector's contract: a `.DXF` is `Ok` only if every byte up to the `0x1A` terminator is printable ASCII, CR or LF. `SHUTTLE.DXF` fails at 1536; `SUBDIV.DXF` passes.

```rust
// crates/acad-corpus/src/check.rs
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clean_text_file_is_ok() {
        let bytes = b"LINE,1\r\n1.0,2.0,3.0,4.0\r\n\x1a".to_vec();
        assert_eq!(classify("SUBDIV.DXF", &bytes), Verdict::Ok);
    }

    #[test]
    fn spliced_binary_is_reported_at_its_offset() {
        let mut bytes = b"LINE,1\r\n1.0,2.0,3.0,4.0\r\n".to_vec();
        bytes.extend_from_slice(&[0x00, 0x88, 0x06, 0x73]);
        assert_eq!(classify("SHUTTLE.DXF", &bytes), Verdict::CorruptAt(25));
    }

    #[test]
    fn slack_after_dos_eof_is_not_corruption() {
        let mut bytes = b"TREE\r\n\x1a".to_vec();
        bytes.extend_from_slice(b"50,1.00000");
        assert_eq!(classify("SUBDIV.DXF", &bytes), Verdict::Ok);
    }

    #[test]
    fn non_text_files_are_classified_binary_not_corrupt() {
        assert_eq!(classify("ACAD.EXE", &[0x4d, 0x5a, 0x00]), Verdict::Binary);
    }
}
```

- [x] **Step 3: Run it and confirm it fails**

Run: `cargo test -p acad-corpus`
Expected: FAIL — `cannot find function classify`.

- [x] **Step 4: Implement the classifier**

```rust
// crates/acad-corpus/src/check.rs
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Verdict { Ok, CorruptAt(usize), Binary }

const DOS_EOF: u8 = 0x1a;

pub fn classify(name: &str, bytes: &[u8]) -> Verdict {
    let textual = name.ends_with(".DXF") || name.ends_with(".MNU")
        || name.ends_with(".DOC") || name.ends_with(".BAT");
    if !textual { return Verdict::Binary; }

    let end = bytes.iter().position(|&b| b == DOS_EOF).unwrap_or(bytes.len());
    match bytes[..end].iter().position(|&b| !(0x20..0x7f).contains(&b) && b != b'\r' && b != b'\n') {
        Some(off) => Verdict::CorruptAt(off),
        None => Verdict::Ok,
    }
}
```

- [x] **Step 5: Run it and confirm it passes**

Run: `cargo test -p acad-corpus`
Expected: PASS, 4 tests.

- [x] **Step 6: Write the manifest generator**

```rust
// crates/acad-corpus/src/main.rs
mod check;
use check::{classify, Verdict};
use sha2::{Digest, Sha256};
use std::{fs, path::Path};

fn main() -> std::io::Result<()> {
    let root = Path::new("corpus");
    let mut rows: Vec<String> = Vec::new();
    let mut dirs: Vec<_> = fs::read_dir(root)?.filter_map(Result::ok)
        .filter(|e| e.path().is_dir() && e.file_name() != "raw").collect();
    dirs.sort_by_key(|e| e.file_name());
    for dir in dirs {
        let disk = dir.file_name().to_string_lossy().to_string();
        let mut files: Vec<_> = fs::read_dir(dir.path())?.filter_map(Result::ok).collect();
        files.sort_by_key(|e| e.file_name());
        for f in files {
            let name = f.file_name().to_string_lossy().to_string();
            let bytes = fs::read(f.path())?;
            let sha = format!("{:x}", Sha256::digest(&bytes));
            let verdict = match classify(&name, &bytes) {
                Verdict::Ok => "ok".to_string(),
                Verdict::Binary => "binary".to_string(),
                Verdict::CorruptAt(o) => format!("corrupt_at_{o}"),
            };
            rows.push(format!(
                "[[file]]\ndisk = \"{disk}\"\nname = \"{name}\"\nbytes = {}\nsha256 = \"{sha}\"\nverdict = \"{verdict}\"\n",
                bytes.len()));
        }
    }
    let body = format!("# Generated by `cargo run -p acad-corpus`. Do not edit by hand.\n\n{}", rows.join("\n"));
    fs::write(root.join("manifest.toml"), body)?;
    println!("wrote corpus/manifest.toml ({} files)", rows.len());
    Ok(())
}
```

```toml
# crates/acad-corpus/Cargo.toml
[package]
name = "acad-corpus"
version = "0.1.0"
edition.workspace = true
rust-version.workspace = true

[dependencies]
sha2 = "0.10"
```

- [x] **Step 7: Generate the manifest and confirm the known corruption is caught**

```bash
./tools/extract-corpus.sh
cargo run -p acad-corpus
grep -A5 'SHUTTLE.DXF' corpus/manifest.toml
```

Expected: `SHUTTLE.DXF` carries `verdict = "corrupt_at_1536"`, and `SUBDIV.DXF` carries `verdict = "ok"`. If either differs, stop — the corpus on disk is not the one this plan was written against.

- [x] **Step 8: Update `.gitignore` and commit**

```
# .gitignore — add
corpus/*
!corpus/manifest.toml
```

```bash
git add .gitignore tools/extract-corpus.sh crates/acad-corpus corpus/manifest.toml Cargo.toml
git commit -m "feat(corpus): extraction script and SHA-256 integrity manifest"
```

---

### Task 3: DXF lexer

**Files:**
- Create: `crates/acad-dxf/Cargo.toml`, `crates/acad-dxf/src/lib.rs`, `src/error.rs`, `src/lex.rs`
- Test: `crates/acad-dxf/src/lex.rs` (inline)

**Interfaces:**
- Consumes: nothing from earlier tasks.
- Produces: `struct Record { pub keyword: String, pub rows: Vec<String>, pub line: usize }` (`line` is 1-based) — **one `Record` per instance**, so a header of `LINE,3` yields three `Record`s; `fn lex(bytes: &[u8]) -> Result<Vec<Record>, DxfError>`; `enum DxfError { Corrupt{offset:usize}, UnknownKeyword{keyword:String, line:usize}, Truncated{keyword:String, line:usize}, BadHeader{line:usize} }` deriving `Debug, PartialEq` and implementing `std::fmt::Display` and `std::error::Error`; `fn rows_per_instance(keyword:&str) -> Option<usize>`.

- [x] **Step 1: Write the failing lexer tests**

These cover Review Focus items 1 and 3.

```rust
// crates/acad-dxf/src/lex.rs
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lexes_a_single_line_record() {
        let src = b"LINE,1\r\n8.800000,5.700000,19.000000,1.899999\r\n\x1a";
        let recs = lex(src).unwrap();
        assert_eq!(recs.len(), 1);
        assert_eq!(recs[0].keyword, "LINE");
        assert_eq!(recs[0].rows, vec!["8.800000,5.700000,19.000000,1.899999"]);
    }

    #[test]
    fn layerc_takes_eight_rows_for_one_instance() {
        let mut src = b"LAYERC,1\r\n".to_vec();
        for _ in 0..8 { src.extend_from_slice(b"0,15,255,255,255,255,255,255,255,255,255,255,255,255,255,255\r\n"); }
        src.push(0x1a);
        let recs = lex(&src).unwrap();
        assert_eq!(recs.len(), 1);
        assert_eq!(recs[0].rows.len(), 8);
    }

    #[test]
    fn instance_count_above_one_yields_that_many_records() {
        let src = b"LINE,3\r\n0,0,1,1\r\n1,1,2,2\r\n2,2,3,3\r\n\x1a";
        let recs = lex(src).unwrap();
        assert_eq!(recs.len(), 3);
        assert_eq!(recs[2].rows, vec!["2,2,3,3"]);
    }

    #[test]
    fn text_record_takes_two_rows() {
        let src = b"TEXT,1\r\n9.624130,12.295160,0.346010,0.000000\r\nA\r\n\x1a";
        let recs = lex(src).unwrap();
        assert_eq!(recs[0].rows, vec!["9.624130,12.295160,0.346010,0.000000", "A"]);
    }

    #[test]
    fn endblk_takes_no_rows() {
        let src = b"ENDBLK,1\r\nLINE,1\r\n0,0,1,1\r\n\x1a";
        let recs = lex(src).unwrap();
        assert_eq!(recs.len(), 2);
        assert!(recs[0].rows.is_empty());
        assert_eq!(recs[1].keyword, "LINE");
    }

    #[test]
    fn spliced_binary_is_an_error_naming_the_offset() {
        let mut src = b"LINE,1\r\n8.8,5.7,19.0,1.9\r\n".to_vec();
        src.extend_from_slice(&[0x00, 0x88, 0x06, 0x73]);
        assert_eq!(lex(&src), Err(DxfError::Corrupt { offset: 25 }));
    }

    #[test]
    fn unimplemented_entity_is_rejected_not_guessed() {
        let src = b"SOLID,1\r\n0,0,1,1\r\n\x1a";
        assert_eq!(lex(src), Err(DxfError::UnknownKeyword { keyword: "SOLID".into(), line: 1 }));
    }

    #[test]
    fn record_running_past_end_of_file_is_truncated() {
        let src = b"TEXT,1\r\n9.6,12.2,0.3,0.0\r\n\x1a";
        assert_eq!(lex(src), Err(DxfError::Truncated { keyword: "TEXT".into(), line: 1 }));
    }
}
```

- [x] **Step 2: Run them and confirm they fail**

Run: `cargo test -p acad-dxf`
Expected: FAIL — `cannot find function lex`.

- [x] **Step 3: Implement the error type**

```rust
// crates/acad-dxf/src/error.rs
use std::fmt;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DxfError {
    Corrupt { offset: usize },
    UnknownKeyword { keyword: String, line: usize },
    Truncated { keyword: String, line: usize },
    BadHeader { line: usize },
    BadNumber { row: String, line: usize },
    UndefinedBlock { name: String },
}

impl fmt::Display for DxfError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Corrupt { offset } =>
                write!(f, "non-text byte at offset {offset}: file is corrupt"),
            Self::UnknownKeyword { keyword, line } =>
                write!(f, "line {line}: unsupported record `{keyword}`"),
            Self::Truncated { keyword, line } =>
                write!(f, "line {line}: record `{keyword}` runs past end of file"),
            Self::BadHeader { line } =>
                write!(f, "line {line}: expected `KEYWORD,<count>`"),
            Self::BadNumber { row, line } =>
                write!(f, "line {line}: cannot parse numbers from `{row}`"),
            Self::UndefinedBlock { name } =>
                write!(f, "INSERT references undefined block `{name}`"),
        }
    }
}

impl std::error::Error for DxfError {}
```

- [x] **Step 4: Implement the lexer**

```rust
// crates/acad-dxf/src/lex.rs
use crate::error::DxfError;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Record { pub keyword: String, pub rows: Vec<String>, pub line: usize }

const DOS_EOF: u8 = 0x1a;

pub fn rows_per_instance(keyword: &str) -> Option<usize> {
    Some(match keyword {
        "ENDBLK" => 0,
        "LAYERC" => 8,
        "TEXT" | "INSERT" | "BLOCK" => 2,
        "EXTENTS" | "LIMITS" | "BASE" | "DWGVIEW" | "MODERES" | "MODEGRID"
        | "MODEORTHO" | "MODEFILL" | "TXTSIZE" | "TRACEWID" | "LAYER"
        | "LINE" | "CIRCLE" | "ARC" => 1,
        _ => return None,
    })
}

pub fn lex(bytes: &[u8]) -> Result<Vec<Record>, DxfError> {
    let end = bytes.iter().position(|&b| b == DOS_EOF).unwrap_or(bytes.len());
    let body = &bytes[..end];
    if let Some(off) = body.iter().position(
        |&b| !(0x20..0x7f).contains(&b) && b != b'\r' && b != b'\n') {
        return Err(DxfError::Corrupt { offset: off });
    }
    // Latin-1: every byte here is already ASCII, so a direct map is exact.
    let text: String = body.iter().map(|&b| b as char).collect();
    let lines: Vec<&str> = text.split("\r\n").filter(|l| !l.is_empty()).collect();

    let mut out = Vec::new();
    let mut i = 0usize;
    while i < lines.len() {
        let (kw, count) = lines[i].rsplit_once(',')
            .and_then(|(k, n)| n.parse::<usize>().ok().map(|n| (k.to_string(), n)))
            .ok_or(DxfError::BadHeader { line: i + 1 })?;
        let per = rows_per_instance(&kw)
            .ok_or_else(|| DxfError::UnknownKeyword { keyword: kw.clone(), line: i + 1 })?;
        let mut cursor = i + 1;
        for _ in 0..count {
            if cursor + per > lines.len() {
                return Err(DxfError::Truncated { keyword: kw.clone(), line: i + 1 });
            }
            out.push(Record { keyword: kw.clone(), line: i + 1,
                rows: lines[cursor..cursor + per].iter().map(|s| s.to_string()).collect() });
            cursor += per;
        }
        i = cursor;
    }
    Ok(out)
}
```

```toml
# crates/acad-dxf/Cargo.toml
[package]
name = "acad-dxf"
version = "0.1.0"
edition.workspace = true
rust-version.workspace = true

[dependencies]
acad-model.workspace = true
```

```rust
// crates/acad-dxf/src/lib.rs
pub mod error;
pub mod lex;
pub use error::DxfError;
pub use lex::{lex, rows_per_instance, Record};
```

- [x] **Step 5: Run and commit**

Run: `cargo test -p acad-dxf` → PASS, 8 tests.

```bash
git add crates/acad-dxf Cargo.toml
git commit -m "feat(dxf): 1983 record lexer with corruption and truncation errors"
```

---

### Task 4: DXF parser — records to `Drawing`

**Files:**
- Create: `crates/acad-dxf/src/parse.rs`
- Modify: `crates/acad-dxf/src/lib.rs` (add `pub mod parse; pub use parse::parse;`)
- Test: `crates/acad-dxf/src/parse.rs` (inline), `crates/acad-dxf/tests/subdiv.rs`

**Interfaces:**
- Consumes: `lex::{lex, Record}`, `DxfError`, and all of `acad-model`.
- Produces: `fn parse(bytes: &[u8]) -> Result<Drawing, DxfError>`.

- [x] **Step 1: Write the failing parser tests**

Review Focus item 4 is the `undefined_block_is_a_named_error` test.

```rust
// crates/acad-dxf/src/parse.rs
#[cfg(test)]
mod tests {
    use super::*;
    use acad_model::{Entity, Item, Point};

    fn with_header(body: &str) -> Vec<u8> {
        let mut s = String::from(
            "EXTENTS,1\r\n-1.750000,19.000000,-1.750000,14.000000\r\n\
             LIMITS,1\r\n-2.000000,19.000000,-2.000000,14.000000\r\n\
             BASE,1\r\n0.000000,0.000000\r\n\
             DWGVIEW,1\r\n8.500000,6.163380,16.326761\r\n\
             MODERES,1\r\n0,0.250000\r\nMODEGRID,1\r\n0,0.250000\r\n\
             MODEORTHO,1\r\n0\r\nMODEFILL,1\r\n1\r\n\
             TXTSIZE,1\r\n0.200000\r\nTRACEWID,1\r\n0.050000\r\nLAYER,1\r\n1\r\n");
        s.push_str(body);
        let mut b = s.into_bytes();
        b.push(0x1a);
        b
    }

    #[test]
    fn parses_header_fields_from_subdiv_values() {
        let d = parse(&with_header("")).unwrap();
        assert_eq!(d.header.limits.xmin, -2.0);
        assert_eq!(d.header.view.height, 16.326761);
        assert!(!d.header.snap.on);
        assert!(d.header.fill);
        assert_eq!(d.header.text_size, 0.2);
        assert_eq!(d.header.current_layer, 1);
    }

    #[test]
    fn parses_the_four_scalar_entity_kinds() {
        let d = parse(&with_header(
            "LINE,1\r\n8.800000,5.700000,19.000000,1.899999\r\n\
             CIRCLE,1\r\n16.464680,12.562830,0.420620\r\n\
             ARC,1\r\n3.037728,2.906527,2.339596,78.604100,22.450900\r\n\
             TEXT,1\r\n9.624130,12.295160,0.346010,0.000000\r\nA\r\n")).unwrap();
        let es: Vec<&Entity> = d.entities().collect();
        assert_eq!(es.len(), 4);
        assert_eq!(*es[0], Entity::Line {
            start: Point { x: 8.8, y: 5.7 }, end: Point { x: 19.0, y: 1.899999 } });
        assert_eq!(*es[2], Entity::Arc {
            center: Point { x: 3.037728, y: 2.906527 },
            radius: 2.339596, start_deg: 78.6041, end_deg: 22.4509 });
        assert_eq!(*es[3], Entity::Text {
            origin: Point { x: 9.62413, y: 12.29516 },
            height: 0.34601, rotation_deg: 0.0, value: "A".into() });
    }

    #[test]
    fn document_order_is_preserved_across_blocks_and_entities() {
        // SUBDIV.DXF interleaves them; a writer that buckets by kind cannot round-trip.
        let d = parse(&with_header(
            "LINE,1\r\n0,0,1,1\r\n\
             BLOCK,1\r\n0,0\r\nB1\r\nCIRCLE,1\r\n0,0,1\r\nENDBLK,1\r\n\
             INSERT,1\r\n1,1,1,1,0\r\nB1\r\n")).unwrap();
        assert!(matches!(d.items[0], Item::Entity(Entity::Line { .. })));
        assert!(matches!(d.items[1], Item::Block(_)));
        assert!(matches!(d.items[2], Item::Entity(Entity::Insert { .. })));
    }

    #[test]
    fn block_definition_collects_entities_until_endblk() {
        let d = parse(&with_header(
            "BLOCK,1\r\n8.500000,3.799999\r\nHOUSEA\r\n\
             LINE,1\r\n0,0,1,1\r\nENDBLK,1\r\n\
             INSERT,1\r\n10.437350,12.218680,1.000000,1.000000,0.000000\r\nHOUSEA\r\n")).unwrap();
        let blocks: Vec<_> = d.blocks().collect();
        assert_eq!(blocks.len(), 1);
        assert_eq!(blocks[0].name, "HOUSEA");
        assert_eq!(blocks[0].entities.len(), 1);
        assert_eq!(d.entities().count(), 1); // the INSERT only
    }

    #[test]
    fn undefined_block_is_a_named_error() {
        let err = parse(&with_header(
            "INSERT,1\r\n1.0,2.0,1.0,1.0,0.0\r\nGHOST\r\n")).unwrap_err();
        assert_eq!(err, DxfError::UndefinedBlock { name: "GHOST".into() });
    }
}
```

- [x] **Step 2: Run them and confirm they fail**

Run: `cargo test -p acad-dxf`
Expected: FAIL — `cannot find function parse`.

- [x] **Step 3: Implement the parser**

```rust
// crates/acad-dxf/src/parse.rs
use crate::{error::DxfError, lex::{lex, Record}};
use acad_model::{Block, Drawing, DwgView, Entity, Extents, Header, Item, Mode, Point};

fn nums(rec: &Record, row: usize) -> Result<Vec<f64>, DxfError> {
    rec.rows[row].split(',').map(|t| t.trim().parse::<f64>()
        .map_err(|_| DxfError::BadNumber { row: rec.rows[row].clone(), line: rec.line }))
        .collect()
}

fn default_header() -> Header {
    let zero = Extents { xmin: 0.0, xmax: 0.0, ymin: 0.0, ymax: 0.0 };
    Header {
        extents: zero, limits: zero, base: Point { x: 0.0, y: 0.0 },
        view: DwgView { center: Point { x: 0.0, y: 0.0 }, height: 0.0 },
        snap: Mode { on: false, spacing: 0.0 }, grid: Mode { on: false, spacing: 0.0 },
        ortho: false, fill: false, text_size: 0.0, trace_width: 0.0,
        current_layer: 0, layer_colors: [255; 128],
    }
}

pub fn parse(bytes: &[u8]) -> Result<Drawing, DxfError> {
    let records = lex(bytes)?;
    let mut header = default_header();
    let mut items: Vec<Item> = Vec::new();
    let mut open_block: Option<Block> = None;

    for rec in &records {
        let entity = match rec.keyword.as_str() {
            "EXTENTS" | "LIMITS" => {
                let v = nums(rec, 0)?;
                let e = Extents { xmin: v[0], xmax: v[1], ymin: v[2], ymax: v[3] };
                if rec.keyword == "EXTENTS" { header.extents = e } else { header.limits = e }
                continue;
            }
            "BASE" => { let v = nums(rec, 0)?; header.base = Point { x: v[0], y: v[1] }; continue }
            "DWGVIEW" => { let v = nums(rec, 0)?;
                header.view = DwgView { center: Point { x: v[0], y: v[1] }, height: v[2] }; continue }
            "MODERES" | "MODEGRID" => {
                let v = nums(rec, 0)?;
                let m = Mode { on: v[0] != 0.0, spacing: v[1] };
                if rec.keyword == "MODERES" { header.snap = m } else { header.grid = m }
                continue;
            }
            "MODEORTHO" => { header.ortho = nums(rec, 0)?[0] != 0.0; continue }
            "MODEFILL" => { header.fill = nums(rec, 0)?[0] != 0.0; continue }
            "TXTSIZE" => { header.text_size = nums(rec, 0)?[0]; continue }
            "TRACEWID" => { header.trace_width = nums(rec, 0)?[0]; continue }
            "LAYER" => { header.current_layer = nums(rec, 0)?[0] as u8; continue }
            "LAYERC" => {
                for (r, row) in rec.rows.iter().enumerate() {
                    for (c, tok) in row.split(',').enumerate() {
                        header.layer_colors[r * 16 + c] =
                            tok.trim().parse::<u8>().map_err(|_| DxfError::BadNumber {
                                row: row.clone(), line: rec.line })?;
                    }
                }
                continue;
            }
            "BLOCK" => {
                let v = nums(rec, 0)?;
                open_block = Some(Block { name: rec.rows[1].clone(),
                    base: Point { x: v[0], y: v[1] }, entities: Vec::new() });
                continue;
            }
            "ENDBLK" => {
                if let Some(b) = open_block.take() { items.push(Item::Block(b)) }
                continue;
            }
            "LINE" => { let v = nums(rec, 0)?;
                Entity::Line { start: Point { x: v[0], y: v[1] }, end: Point { x: v[2], y: v[3] } } }
            "CIRCLE" => { let v = nums(rec, 0)?;
                Entity::Circle { center: Point { x: v[0], y: v[1] }, radius: v[2] } }
            "ARC" => { let v = nums(rec, 0)?;
                Entity::Arc { center: Point { x: v[0], y: v[1] }, radius: v[2],
                    start_deg: v[3], end_deg: v[4] } }
            "TEXT" => { let v = nums(rec, 0)?;
                Entity::Text { origin: Point { x: v[0], y: v[1] }, height: v[2],
                    rotation_deg: v[3], value: rec.rows[1].clone() } }
            "INSERT" => { let v = nums(rec, 0)?;
                Entity::Insert { origin: Point { x: v[0], y: v[1] }, x_scale: v[2],
                    y_scale: v[3], rotation_deg: v[4], name: rec.rows[1].clone() } }
            other => return Err(DxfError::UnknownKeyword { keyword: other.into(), line: rec.line }),
        };
        match open_block.as_mut() {
            Some(b) => b.entities.push(entity),
            None => items.push(Item::Entity(entity)),
        }
    }

    let drawing = Drawing { header, items };
    for e in drawing.entities() {
        if let Entity::Insert { name, .. } = e {
            if drawing.block(name).is_none() {
                return Err(DxfError::UndefinedBlock { name: name.clone() });
            }
        }
    }
    Ok(drawing)
}
```

- [x] **Step 4: Run and confirm the unit tests pass**

Run: `cargo test -p acad-dxf` → PASS.

- [x] **Step 5: Add the real-corpus integration test**

```rust
// crates/acad-dxf/tests/subdiv.rs
use acad_dxf::parse;
use acad_model::Entity;

fn subdiv() -> Vec<u8> {
    std::fs::read("../../corpus/Samples/SUBDIV.DXF")
        .expect("run ./tools/extract-corpus.sh first")
}

#[test]
fn subdiv_parses_with_the_counts_recorded_in_the_spec() {
    let d = parse(&subdiv()).unwrap();
    let count = |f: fn(&Entity) -> bool| d.entities().chain(
        d.blocks().flat_map(|b| b.entities.iter())).filter(|e| f(e)).count();
    assert_eq!(count(|e| matches!(e, Entity::Line { .. })), 133);
    assert_eq!(count(|e| matches!(e, Entity::Arc { .. })), 8);
    assert_eq!(count(|e| matches!(e, Entity::Text { .. })), 8);
    assert_eq!(count(|e| matches!(e, Entity::Insert { .. })), 8);
    assert_eq!(count(|e| matches!(e, Entity::Circle { .. })), 2);
    assert_eq!(d.blocks().count(), 6);
    // 78 loose entities + 6 block definitions; the 12 header records are not items.
    assert_eq!(d.items.len(), 84);
}

#[test]
fn shuttle_dxf_is_rejected_as_corrupt_at_1536() {
    let bytes = std::fs::read("../../corpus/Samples/SHUTTLE.DXF").unwrap();
    let err = parse(&bytes).unwrap_err();
    assert_eq!(format!("{err}"), "non-text byte at offset 1536: file is corrupt");
}
```

- [x] **Step 6: Run, then commit**

Run: `cargo test -p acad-dxf` → PASS.

If the `SUBDIV` counts disagree, do not adjust the assertions — they come from the spec's verified corpus analysis. Investigate the parser first.

```bash
git add crates/acad-dxf
git commit -m "feat(dxf): parse records into the entity model"
```

---

### Task 5: DXF writer and byte-identical round-trip

**Files:**
- Create: `crates/acad-dxf/src/write.rs`
- Modify: `crates/acad-dxf/src/lib.rs` (add `pub mod write; pub use write::write;`)
- Test: `crates/acad-dxf/tests/roundtrip.rs`

**Interfaces:**
- Consumes: `acad_model::Drawing`.
- Produces: `fn write(drawing: &Drawing) -> Vec<u8>` — emits CRLF line endings, `%.6}` fixed-point formatting, and a trailing `0x1A`.

- [x] **Step 1: Write the failing round-trip test**

```rust
// crates/acad-dxf/tests/roundtrip.rs
use acad_dxf::{parse, write};

fn subdiv() -> Vec<u8> {
    std::fs::read("../../corpus/Samples/SUBDIV.DXF")
        .expect("run ./tools/extract-corpus.sh first")
}

/// The on-disk file carries cluster slack after its 0x1A terminator;
/// content is everything up to and including that byte.
fn content(bytes: &[u8]) -> &[u8] {
    let end = bytes.iter().position(|&b| b == 0x1a).unwrap();
    &bytes[..=end]
}

#[test]
fn subdiv_round_trips_byte_identically() {
    let original = subdiv();
    let rewritten = write(&parse(&original).unwrap());
    assert_eq!(rewritten, content(&original));
}

#[test]
fn parse_of_our_own_output_is_stable() {
    let once = parse(&subdiv()).unwrap();
    let twice = parse(&write(&once)).unwrap();
    assert_eq!(once, twice);
}
```

- [x] **Step 2: Run it and confirm it fails**

Run: `cargo test -p acad-dxf --test roundtrip`
Expected: FAIL — `cannot find function write`.

- [x] **Step 3: Implement the writer**

```rust
// crates/acad-dxf/src/write.rs
use acad_model::{Drawing, Entity, Item, Point};
use std::fmt::Write as _;

fn f(v: f64) -> String { format!("{v:.6}") }
fn pt(p: &Point) -> String { format!("{},{}", f(p.x), f(p.y)) }

fn entity(out: &mut String, e: &Entity) {
    match e {
        Entity::Line { start, end } =>
            { let _ = write!(out, "LINE,1\r\n{},{}\r\n", pt(start), pt(end)); }
        Entity::Circle { center, radius } =>
            { let _ = write!(out, "CIRCLE,1\r\n{},{}\r\n", pt(center), f(*radius)); }
        Entity::Arc { center, radius, start_deg, end_deg } =>
            { let _ = write!(out, "ARC,1\r\n{},{},{},{}\r\n",
                pt(center), f(*radius), f(*start_deg), f(*end_deg)); }
        Entity::Text { origin, height, rotation_deg, value } =>
            { let _ = write!(out, "TEXT,1\r\n{},{},{}\r\n{}\r\n",
                pt(origin), f(*height), f(*rotation_deg), value); }
        Entity::Insert { origin, x_scale, y_scale, rotation_deg, name } =>
            { let _ = write!(out, "INSERT,1\r\n{},{},{},{}\r\n{}\r\n",
                pt(origin), f(*x_scale), f(*y_scale), f(*rotation_deg), name); }
    }
}

pub fn write(d: &Drawing) -> Vec<u8> {
    let mut s = String::new();
    let h = &d.header;
    let _ = write!(s, "EXTENTS,1\r\n{},{},{},{}\r\n",
        f(h.extents.xmin), f(h.extents.xmax), f(h.extents.ymin), f(h.extents.ymax));
    let _ = write!(s, "LIMITS,1\r\n{},{},{},{}\r\n",
        f(h.limits.xmin), f(h.limits.xmax), f(h.limits.ymin), f(h.limits.ymax));
    let _ = write!(s, "BASE,1\r\n{}\r\n", pt(&h.base));
    let _ = write!(s, "DWGVIEW,1\r\n{},{}\r\n", pt(&h.view.center), f(h.view.height));
    let _ = write!(s, "MODERES,1\r\n{},{}\r\n", h.snap.on as u8, f(h.snap.spacing));
    let _ = write!(s, "MODEGRID,1\r\n{},{}\r\n", h.grid.on as u8, f(h.grid.spacing));
    let _ = write!(s, "MODEORTHO,1\r\n{}\r\n", h.ortho as u8);
    let _ = write!(s, "MODEFILL,1\r\n{}\r\n", h.fill as u8);
    let _ = write!(s, "TXTSIZE,1\r\n{}\r\n", f(h.text_size));
    let _ = write!(s, "TRACEWID,1\r\n{}\r\n", f(h.trace_width));
    let _ = write!(s, "LAYER,1\r\n{}\r\n", h.current_layer);
    s.push_str("LAYERC,1\r\n");
    for row in h.layer_colors.chunks(16) {
        let cells: Vec<String> = row.iter().map(|c| c.to_string()).collect();
        let _ = write!(s, "{}\r\n", cells.join(","));
    }
    // Document order, not bucketed by kind: SUBDIV.DXF interleaves the two.
    for item in &d.items {
        match item {
            Item::Block(b) => {
                let _ = write!(s, "BLOCK,1\r\n{}\r\n{}\r\n", pt(&b.base), b.name);
                for e in &b.entities { entity(&mut s, e); }
                s.push_str("ENDBLK,1\r\n");
            }
            Item::Entity(e) => entity(&mut s, e),
        }
    }
    let mut bytes = s.into_bytes();
    bytes.push(0x1a);
    bytes
}
```

- [x] **Step 4: Run the round-trip and reconcile**

Run: `cargo test -p acad-dxf --test roundtrip`

If bytes differ, diff them and fix the **writer**, not the test:

```bash
cargo test -p acad-dxf --test roundtrip -- --nocapture 2>&1 | head -40
```

Two things make this pass, and the writer above already does both. Header records are emitted in the order `SUBDIV.DXF` uses, which is the order of the spec's layout table. Blocks and entities are emitted by walking `d.items` in sequence, because the original interleaves them — 47 loose lines precede the first block, and two more blocks appear between inserts. If you find yourself sorting or bucketing anything in this function, that is the bug.

- [x] **Step 5: Commit**

```bash
git add crates/acad-dxf
git commit -m "feat(dxf): writer with byte-identical SUBDIV round-trip"
```

---

### Task 6: Render — flatten a `Drawing` to screen-space polylines

**Files:**
- Create: `crates/acad-render/Cargo.toml`, `crates/acad-render/src/lib.rs`, `src/viewport.rs`, `src/flatten.rs`
- Test: `crates/acad-render/src/viewport.rs` and `src/flatten.rs` (inline)

**Interfaces:**
- Consumes: `acad_model::{Drawing, Entity, Extents, Point}`.
- Produces: `struct Viewport { pub width: u32, pub height: u32, scale: f64, offset: Point }` with `fn fit(extents: &Extents, width: u32, height: u32) -> Viewport` and `fn to_screen(&self, p: Point) -> Point`; `enum Prim { Polyline(Vec<Point>) }`; `fn flatten(drawing: &Drawing, vp: &Viewport) -> Vec<Prim>`.

- [x] **Step 1: Write the failing viewport tests**

Review Focus item 5 is `degenerate_extents_do_not_divide_by_zero`.

```rust
// crates/acad-render/src/viewport.rs
#[cfg(test)]
mod tests {
    use super::*;
    use acad_model::{Extents, Point};

    #[test]
    fn fit_centres_and_flips_y() {
        let e = Extents { xmin: 0.0, xmax: 10.0, ymin: 0.0, ymax: 10.0 };
        let vp = Viewport::fit(&e, 100, 100);
        let origin = vp.to_screen(Point { x: 0.0, y: 0.0 });
        let top = vp.to_screen(Point { x: 0.0, y: 10.0 });
        assert!(origin.y > top.y, "world +y must map to screen -y");
        assert!((origin.y - 100.0).abs() < 1.0);
        assert!((top.y - 0.0).abs() < 1.0);
    }

    #[test]
    fn fit_preserves_aspect_ratio() {
        let e = Extents { xmin: 0.0, xmax: 20.0, ymin: 0.0, ymax: 10.0 };
        let vp = Viewport::fit(&e, 200, 200);
        let a = vp.to_screen(Point { x: 0.0, y: 0.0 });
        let b = vp.to_screen(Point { x: 20.0, y: 10.0 });
        assert!(((b.x - a.x).abs() - 200.0).abs() < 1.0);
        assert!(((b.y - a.y).abs() - 100.0).abs() < 1.0);
    }

    #[test]
    fn degenerate_extents_do_not_divide_by_zero() {
        let e = Extents { xmin: 3.0, xmax: 3.0, ymin: 4.0, ymax: 4.0 };
        let vp = Viewport::fit(&e, 100, 100);
        let p = vp.to_screen(Point { x: 3.0, y: 4.0 });
        assert!(p.x.is_finite() && p.y.is_finite());
    }
}
```

- [x] **Step 2: Run and confirm failure**

Run: `cargo test -p acad-render`
Expected: FAIL — `cannot find type Viewport`.

- [x] **Step 3: Implement the viewport**

```rust
// crates/acad-render/src/viewport.rs
use acad_model::{Extents, Point};

#[derive(Debug, Clone, Copy)]
pub struct Viewport { pub width: u32, pub height: u32, scale: f64, offset: Point }

impl Viewport {
    pub fn fit(e: &Extents, width: u32, height: u32) -> Self {
        // A degenerate box has no extent to fit; fall back to 1:1 so the
        // transform stays finite and the drawing is simply centred.
        let (w, h) = (e.width().max(f64::EPSILON), e.height().max(f64::EPSILON));
        let scale = if e.is_degenerate() { 1.0 }
                    else { (width as f64 / w).min(height as f64 / h) };
        let cx = (e.xmin + e.xmax) / 2.0;
        let cy = (e.ymin + e.ymax) / 2.0;
        Self { width, height, scale, offset: Point { x: cx, y: cy } }
    }

    pub fn to_screen(&self, p: Point) -> Point {
        Point {
            x: (p.x - self.offset.x) * self.scale + self.width as f64 / 2.0,
            y: self.height as f64 / 2.0 - (p.y - self.offset.y) * self.scale,
        }
    }
}
```

- [x] **Step 4: Run and confirm the viewport tests pass**

Run: `cargo test -p acad-render` → PASS, 3 tests.

- [x] **Step 5: Write the failing flatten tests**

Review Focus item 2 is `arc_sweeps_counter_clockwise_through_zero`.

```rust
// crates/acad-render/src/flatten.rs
#[cfg(test)]
mod tests {
    use super::*;
    use acad_model::{Entity, Item, Point};

    fn vp() -> Viewport {
        Viewport::fit(&acad_model::Extents { xmin: 0.0, xmax: 10.0, ymin: 0.0, ymax: 10.0 }, 100, 100)
    }

    #[test]
    fn line_becomes_a_two_point_polyline() {
        let prims = flatten_entity(&Entity::Line {
            start: Point { x: 0.0, y: 0.0 }, end: Point { x: 10.0, y: 10.0 } }, &vp());
        let Prim::Polyline(pts) = &prims[0];
        assert_eq!(pts.len(), 2);
    }

    #[test]
    fn circle_is_closed() {
        let prims = flatten_entity(&Entity::Circle {
            center: Point { x: 5.0, y: 5.0 }, radius: 2.0 }, &vp());
        let Prim::Polyline(pts) = &prims[0];
        let (first, last) = (pts[0], *pts.last().unwrap());
        assert!((first.x - last.x).abs() < 1e-9 && (first.y - last.y).abs() < 1e-9);
    }

    #[test]
    fn arc_sweeps_counter_clockwise_through_zero() {
        // SUBDIV's arc: start 78.6041 deg, end 22.4509 deg.
        // CCW from 78.6 to 22.45 wraps through 360/0, a sweep of 303.847 deg.
        assert!((sweep_deg(78.6041, 22.4509) - 303.8468).abs() < 1e-3);
    }

    #[test]
    fn a_full_turn_is_used_when_start_equals_end() {
        assert!((sweep_deg(90.0, 90.0) - 360.0).abs() < 1e-9);
    }
}
```

- [x] **Step 6: Implement flatten**

```rust
// crates/acad-render/src/flatten.rs
use crate::viewport::Viewport;
use acad_model::{Drawing, Entity, Item, Point};

#[derive(Debug, Clone, PartialEq)]
pub enum Prim { Polyline(Vec<Point>) }

/// Counter-clockwise sweep from `start` to `end`, in degrees, always positive.
/// AutoCAD 1.4 stores arcs CCW, so an end angle below the start wraps through 0.
pub fn sweep_deg(start: f64, end: f64) -> f64 {
    let d = (end - start).rem_euclid(360.0);
    if d == 0.0 { 360.0 } else { d }
}

fn arc_points(c: Point, r: f64, start: f64, sweep: f64, vp: &Viewport) -> Vec<Point> {
    let steps = ((sweep / 4.0).ceil() as usize).max(8);
    (0..=steps).map(|i| {
        let a = (start + sweep * (i as f64 / steps as f64)).to_radians();
        vp.to_screen(Point { x: c.x + r * a.cos(), y: c.y + r * a.sin() })
    }).collect()
}

pub fn flatten_entity(e: &Entity, vp: &Viewport) -> Vec<Prim> {
    match e {
        Entity::Line { start, end } =>
            vec![Prim::Polyline(vec![vp.to_screen(*start), vp.to_screen(*end)])],
        Entity::Circle { center, radius } =>
            vec![Prim::Polyline(arc_points(*center, *radius, 0.0, 360.0, vp))],
        Entity::Arc { center, radius, start_deg, end_deg } =>
            vec![Prim::Polyline(arc_points(*center, *radius, *start_deg,
                sweep_deg(*start_deg, *end_deg), vp))],
        // TEXT needs the .SHP font files, which milestone ① does not decode.
        Entity::Text { .. } => Vec::new(),
        Entity::Insert { .. } => Vec::new(), // expanded by `flatten` below
    }
}

pub fn flatten(d: &Drawing, vp: &Viewport) -> Vec<Prim> {
    let mut out = Vec::new();
    for e in d.entities() {
        match e {
            Entity::Insert { origin, x_scale, y_scale, rotation_deg, name } => {
                let Some(b) = d.block(name) else { continue };
                let (sin, cos) = rotation_deg.to_radians().sin_cos();
                for inner in &b.entities {
                    for prim in flatten_entity(inner, vp) {
                        let Prim::Polyline(pts) = prim;
                        // Re-place in world space, then re-project.
                        out.push(Prim::Polyline(pts.into_iter().map(|sp| {
                            let w = unproject(vp, sp);
                            let (dx, dy) = (w.x - b.base.x, w.y - b.base.y);
                            let (sx, sy) = (dx * x_scale, dy * y_scale);
                            vp.to_screen(Point {
                                x: origin.x + sx * cos - sy * sin,
                                y: origin.y + sx * sin + sy * cos })
                        }).collect()));
                    }
                }
            }
            other => out.extend(flatten_entity(other, vp)),
        }
    }
    out
}

fn unproject(vp: &Viewport, p: Point) -> Point { vp.to_world(p) }
```

Add the inverse transform to `Viewport`:

```rust
// crates/acad-render/src/viewport.rs — add to impl Viewport
pub fn to_world(&self, p: Point) -> Point {
    Point {
        x: (p.x - self.width as f64 / 2.0) / self.scale + self.offset.x,
        y: (self.height as f64 / 2.0 - p.y) / self.scale + self.offset.y,
    }
}
```

```toml
# crates/acad-render/Cargo.toml
[package]
name = "acad-render"
version = "0.1.0"
edition.workspace = true
rust-version.workspace = true

[dependencies]
acad-model.workspace = true
tiny-skia = "0.11"
```

```rust
// crates/acad-render/src/lib.rs
pub mod flatten;
pub mod raster;
pub mod viewport;
pub use flatten::{flatten, flatten_entity, sweep_deg, Prim};
pub use raster::rasterize;
pub use viewport::Viewport;
```

- [x] **Step 7: Run and commit**

Run: `cargo test -p acad-render` → PASS, 7 tests.

```bash
git add crates/acad-render Cargo.toml
git commit -m "feat(render): viewport transform and entity flattening"
```

---

### Task 7: Rasterize to a pixmap

**Files:**
- Create: `crates/acad-render/src/raster.rs`
- Test: `crates/acad-render/tests/raster.rs`

**Interfaces:**
- Consumes: `Prim`, `Viewport`, `acad_model::Drawing`.
- Produces: `fn rasterize(prims: &[Prim], width: u32, height: u32) -> tiny_skia::Pixmap`.

- [x] **Step 1: Write the failing rasterization test**

```rust
// crates/acad-render/tests/raster.rs
use acad_model::{Extents, Point};
use acad_render::{rasterize, Prim, Viewport};

#[test]
fn a_diagonal_line_marks_pixels_on_both_ends() {
    let vp = Viewport::fit(&Extents { xmin: 0.0, xmax: 10.0, ymin: 0.0, ymax: 10.0 }, 64, 64);
    let prims = vec![Prim::Polyline(vec![
        vp.to_screen(Point { x: 1.0, y: 1.0 }),
        vp.to_screen(Point { x: 9.0, y: 9.0 })])];
    let pm = rasterize(&prims, 64, 64);
    let lit = pm.pixels().iter().filter(|p| p.alpha() > 0).count();
    assert!(lit > 40, "expected a drawn diagonal, got {lit} lit pixels");
}

#[test]
fn an_empty_drawing_rasterizes_to_a_blank_pixmap() {
    let pm = rasterize(&[], 32, 32);
    assert!(pm.pixels().iter().all(|p| p.alpha() == 0));
}
```

- [x] **Step 2: Run and confirm failure**

Run: `cargo test -p acad-render --test raster`
Expected: FAIL — `cannot find function rasterize`.

- [x] **Step 3: Implement the rasterizer**

```rust
// crates/acad-render/src/raster.rs
use crate::flatten::Prim;
use tiny_skia::{Paint, PathBuilder, Pixmap, Stroke, Transform};

pub fn rasterize(prims: &[Prim], width: u32, height: u32) -> Pixmap {
    let mut pm = Pixmap::new(width, height).expect("non-zero pixmap dimensions");
    let mut paint = Paint::default();
    paint.set_color_rgba8(255, 255, 255, 255);
    paint.anti_alias = true;
    let stroke = Stroke { width: 1.0, ..Stroke::default() };

    for Prim::Polyline(pts) in prims {
        if pts.len() < 2 { continue }
        let mut pb = PathBuilder::new();
        pb.move_to(pts[0].x as f32, pts[0].y as f32);
        for p in &pts[1..] { pb.line_to(p.x as f32, p.y as f32); }
        if let Some(path) = pb.finish() {
            pm.stroke_path(&path, &paint, &stroke, Transform::identity(), None);
        }
    }
    pm
}
```

- [x] **Step 4: Run and confirm it passes**

Run: `cargo test -p acad-render --test raster` → PASS.

- [x] **Step 5: Add an end-to-end smoke test over the real corpus**

```rust
// crates/acad-render/tests/raster.rs — append
#[test]
fn subdiv_renders_a_non_blank_image() {
    let bytes = std::fs::read("../../corpus/Samples/SUBDIV.DXF")
        .expect("run ./tools/extract-corpus.sh first");
    let d = acad_dxf::parse(&bytes).unwrap();
    let vp = Viewport::fit(&d.header.limits, 800, 600);
    let pm = rasterize(&acad_render::flatten(&d, &vp), 800, 600);
    let lit = pm.pixels().iter().filter(|p| p.alpha() > 0).count();
    assert!(lit > 5000, "SUBDIV should draw substantial geometry, got {lit}");
}
```

Add `acad-dxf.workspace = true` under a new `[dev-dependencies]` section in `crates/acad-render/Cargo.toml`.

- [x] **Step 6: Run and commit**

Run: `cargo test -p acad-render` → PASS.

```bash
git add crates/acad-render
git commit -m "feat(render): tiny-skia rasterizer with corpus smoke test"
```

---

### Task 8: Window the result

**Files:**
- Create: `crates/acad-app/Cargo.toml`, `crates/acad-app/src/main.rs`
- Test: manual — this task's deliverable is a visible window.

**Interfaces:**
- Consumes: `acad_dxf::parse`, `acad_render::{flatten, rasterize, Viewport}`.
- Produces: the `acad` binary — `cargo run -p acad-app -- <file.dxf>`.

- [x] **Step 1: Declare the crate**

```toml
# crates/acad-app/Cargo.toml
[package]
name = "acad-app"
version = "0.1.0"
edition.workspace = true
rust-version.workspace = true

[[bin]]
name = "acad"
path = "src/main.rs"

[dependencies]
acad-dxf.workspace = true
acad-model.workspace = true
acad-render.workspace = true
winit = "0.30"
softbuffer = "0.4"
```

- [x] **Step 2: Write the viewer**

```rust
// crates/acad-app/src/main.rs
use acad_render::{flatten, rasterize, Viewport};
use std::{num::NonZeroU32, rc::Rc};
use winit::{application::ApplicationHandler, event::WindowEvent,
    event_loop::{ActiveEventLoop, EventLoop}, window::{Window, WindowId}};

struct App { drawing: acad_model::Drawing, state: Option<(Rc<Window>, softbuffer::Surface<Rc<Window>, Rc<Window>>)> }

impl ApplicationHandler for App {
    fn resumed(&mut self, el: &ActiveEventLoop) {
        let attrs = Window::default_attributes().with_title("AutoCAD 1.4");
        let window = Rc::new(el.create_window(attrs).unwrap());
        let context = softbuffer::Context::new(window.clone()).unwrap();
        let surface = softbuffer::Surface::new(&context, window.clone()).unwrap();
        self.state = Some((window, surface));
    }

    fn window_event(&mut self, el: &ActiveEventLoop, _: WindowId, event: WindowEvent) {
        let Some((window, surface)) = self.state.as_mut() else { return };
        match event {
            WindowEvent::CloseRequested => el.exit(),
            WindowEvent::RedrawRequested => {
                let size = window.inner_size();
                let (Some(w), Some(h)) = (NonZeroU32::new(size.width), NonZeroU32::new(size.height))
                    else { return };
                surface.resize(w, h).unwrap();
                let vp = Viewport::fit(&self.drawing.header.limits, size.width, size.height);
                let pm = rasterize(&flatten(&self.drawing, &vp), size.width, size.height);
                let mut buffer = surface.buffer_mut().unwrap();
                for (dst, src) in buffer.iter_mut().zip(pm.pixels()) {
                    *dst = ((src.red() as u32) << 16) | ((src.green() as u32) << 8) | src.blue() as u32;
                }
                buffer.present().unwrap();
            }
            _ => {}
        }
    }
}

fn main() {
    let path = std::env::args().nth(1)
        .unwrap_or_else(|| "corpus/Samples/SUBDIV.DXF".to_string());
    let bytes = std::fs::read(&path).unwrap_or_else(|e| panic!("{path}: {e}"));
    let drawing = acad_dxf::parse(&bytes).unwrap_or_else(|e| panic!("{path}: {e}"));
    println!("{}: {} entities, {} blocks", path,
        drawing.entities().count(), drawing.blocks().count());
    let el = EventLoop::new().unwrap();
    el.run_app(&mut App { drawing, state: None }).unwrap();
}
```

- [x] **Step 3: Run it**

```bash
cargo run -p acad-app -- corpus/Samples/SUBDIV.DXF
```

Expected: a window showing the subdivision plan — property boundary lines, arcs on the cul-de-sac, and the five inserted `HOUSEA` blocks. Text will not appear; `.SHP` font decoding is milestone ⑤.

- [x] **Step 4: Confirm the whole workspace is green**

Run: `cargo test --workspace`
Expected: PASS across all crates.

- [x] **Step 5: Commit**

```bash
git add crates/acad-app Cargo.toml
git commit -m "feat(app): winit viewer rendering SUBDIV.DXF"
```

---

## Milestone ① exit criteria

- `cargo test --workspace` passes.
- `corpus/manifest.toml` is committed and records `SHUTTLE.DXF` as `corrupt_at_1536`.
- `SUBDIV.DXF` round-trips byte-identically through parse → write.
- `cargo run -p acad-app` displays the drawing.

Milestone ② (Ghidra overlay loader and AST export) gets its own plan and begins from the same spec.
