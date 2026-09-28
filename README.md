# autorust

Rebuilding **AutoCAD 1.4** (1983, MS-DOS) as a native Rust application: the same
commands, the same drawing semantics, the same files, in a modern window.

The original is the oracle. "Is this command right?" is answered by differential
test against the real `ACAD.EXE`, not by judgement. 1983 policy is confined to
the codecs and the command layer; `acad-model` is written to grow, so deliberate
divergence later — 3D, modern UX, an extended entity model — does not mean
discarding the verified-compatible core.

Design: [`docs/superpowers/specs/2026-09-28-autocad-14-rust-design.md`](docs/superpowers/specs/2026-09-28-autocad-14-rust-design.md)

## State

| | Milestone | |
|---|---|---|
| ① | DXF codec, corpus integrity, first render | **done** |
| ② | Ghidra overlay loader, dual AST export, `acad-re` | **done** |
| ③ | Oracle harness — in-tree 8086, differential tests | next |
| ④ | DWG codec — `AC1.40`, then `AC1.2` | `AC1.2` read: all 16 corpus drawings parse and render |
| ⑤ | Command loop | |

`SUBDIV.DXF` round-trips byte-identically and renders. The full 1983 command set
— 57 commands — has been recovered from `ACAD.OVL`; none are implemented yet.

**The §8 decision gate did not pass (1 of 3).** Transpiling the decompiler's AST
to Rust is therefore *not* adopted: `acad-re` stays an understanding tool and all
shipped Rust is hand-written. The measurements behind that are in
[`docs/re-pipeline.md`](docs/re-pipeline.md).

④'s read direction runs before ③, out of spec order, because `SUBDIV.DWG` and
`SUBDIV.DXF` are the same drawing in both formats — ground truth that needs no
emulator (spec §4.4). Its write direction, `AC1.2`, and oracle verification wait
for ③.

`SUBDIV` is the corpus's only drawing with a DXF sibling, so it is the only
independent check there is. Seven `AC1.2` record types decode through it and are
**verified**, byte for byte, against `SUBDIV.DXF`: `LINE`, `CIRCLE`, `ARC`, `TEXT`,
`BLOCK`, `ENDBLK`, `INSERT`. Four more — `POINT`, `TRACE`, `SOLID`, and the
`REPEAT`/`ENDREP` pair — have no DXF sibling anywhere in the corpus, so they are
**inferred** from record size and whole-file consistency alone: the file's
record-by-record walk lands exactly on the header's own `entity_end` with a
record count matching `entity_count`, cross-checked against two independent
corpus files apiece where one was available (see `crates/acad-dwg/src/entity.rs`'s
module doc for the full evidence behind each). A signed type code marking an
erased entity (`ADDER`'s own finding) is confirmed the same way: reading the code
as `i16` makes `ADDER`'s walk land exactly, where reading it unsigned does not.

All 16 `AC1.2` drawings in the corpus have been run through the reader
(`crates/acad-dwg/tests/corpus_smoke.rs`): **all 16 render**. The pre-existing 11
use only the seven verified types (evidence the layout generalises, not a
second oracle check); `ADDER`, `FLOOR` and `FLOW` needed erasure, `REPEAT` and
`SOLID` respectively.

The last two, `SELEXOL` and `BLIVET`, needed a structural fix rather than a new
record type. Both decoded every individual record correctly from the start
(`POINT`, `TRACE` and `REPEAT` included — checked directly against their real
bytes, not a synthetic fixture), but both also contain a `BLOCK` definition
nested inside another `BLOCK` definition (`SELEXOL`'s `"HEAD"` inside
`"PACKTWR"`, and independently `"ARROW"` inside `"COOLER"`; `BLIVET`'s
`"$BCIRC"` inside a block named `"BLIVET"`), which the reader used to track
with a single `Option`, not a stack, and so rejected as `DwgError::NestedBlock`
even though both files are otherwise perfectly well-formed (every `BLOCK` does
have a matching `ENDBLK`). The reader now tracks open `BLOCK`s as a stack: a
`BLOCK` opened while another is already open defines a **sibling**, not a
child — evidenced by `SELEXOL`'s `ARROW`, which is also `INSERT`ed fourteen
times outside `COOLER`'s own span, so it cannot be scoped to `COOLER` — and
each `ENDBLK` closes and emits the innermost open one. `DwgError::NestedBlock`
no longer exists.

Grouping the blocks correctly still wasn't enough to draw them: both files
also `INSERT` one block from inside another block's own body, so the renderer's
`INSERT` expansion had to become recursive, composing each nesting level's own
translate/scale/rotate transform with the one enclosing it, with a depth cap
(ours, not a recovered 1983 constant — the corpus never nests past depth 2)
against a self-referencing `INSERT`.

**This plan adds no oracle.** `SELEXOL` and `BLIVET` still have no DXF sibling,
so their render — 21,439 and 31,729 lit pixels respectively, on the smoke
test's 800×800 canvas, well clear of `SUBDIV`'s 15,660 and `ORGATE`'s
smallest-of-the-corpus 2,466 — is evidence that the record layouts and this
nesting fix generalise across the corpus, not a second, independent
verification the way a DXF comparison would be. The verified-versus-inferred
split above is unchanged by any of this: `POINT`, `TRACE`, `SOLID` and
`REPEAT`/`ENDREP` remain inferred, not verified, exactly as before.

`acad-app` opens either format, dispatching on the file's own magic bytes rather
than its extension, so a `.BAK` file — four corpus drawings have one — is
identified as the DWG it is rather than guessed from its name. Every `.BAK` in
this corpus happens to be `AC1.40`, so today that means each is correctly
refused with a message naming both versions, not silently misread as DXF.

## Build

Rust 1.88.0, pinned by `rust-toolchain.toml`.

    cargo test --workspace
    cargo run -p acad-app        # renders SUBDIV.DXF in a window
    cargo run -p acad-app -- corpus/Samples/SUBDIV.DWG   # or the DWG sibling

Tests that need the corpus skip when it is absent, so a fresh checkout is green
without it.

## Crates

| Crate | |
|---|---|
| `acad-model` | Entity model. The one crate everything depends on, and the one written for the future rather than for 1983. |
| `acad-dxf` | The 1983 `KEYWORD,count` DXF codec — not the modern group-code format. |
| `acad-dwg` | The 1983 binary DWG codec. `AC1.2` read support so far; `AC1.40` and the write direction come after milestone ③. Depends only on `acad-model` — a shipped codec must not depend on another codec. |
| `acad-render` | Viewport fit and entity flattening (numerically testable), then rasterisation. |
| `acad-app` | Window, via `winit` + `softbuffer`. Opens either `.DXF` or `.DWG`, chosen by the file's magic bytes. |
| `acad-corpus` | Generates `corpus/manifest.toml`, the integrity record. |
| `acad-re` | **Dev only.** `ACAD.OVL` container codec, typed Ghidra AST, call graph, command recovery, gate metrics. Nothing shipped depends on it. |

Still to come, per spec §5: `acad-cmd`, `acad-oracle`.

## The corpus

The 1.4 sample disks are a used 1983 working floppy and are **partly corrupt**.
`corpus/manifest.toml` records all 83 files with SHA-256 and a verdict; it is
committed, and it is the reproducibility record. The bytes themselves are not in
git.

    ./tools/extract-corpus.sh    # rebuilds corpus/ from the archives in autocad/

`SHUTTLE.DXF` is valid for 1,536 bytes and then has another file's data spliced
in at a sector boundary. It is excluded explicitly and by name, never silently.

## Reverse engineering

    ./tools/re-pipeline.sh       # raw disk images -> typed AST -> gate report

Needs Ghidra 11.3+, a JDK 21, and Python 3.11+; `./tools/ghidra-env.sh` finds
them and says what is missing. `acad-re` owns the `ACAD.OVL` format and emits
`build/ovl-map.json`; the PyGhidra scripts only place bytes where it says. One
implementation, one set of tests. See [`docs/re-pipeline.md`](docs/re-pipeline.md).

## Scope

AutoCAD 1.4 only — `ACAD.EXE` (78,848 B) and `ACAD.OVL` (179,480 B). Archives for
2.x through r12 are held locally but are not inputs to this design. DOS emulation
is test infrastructure, not a product feature; there is no plan for pixel-exact
CGA/Hercules reproduction or for plotter and digitizer hardware.
