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
| ③ | Oracle harness — in-tree 8086, differential tests | in progress: QEMU command and sample-export comparisons |
| ④ | DWG codec — `AC1.40`, then `AC1.2` | both versions read; 20 corpus drawings render; font LOAD/SHAPE and writing remain |
| ⑤ | Command loop | |

`SUBDIV.DXF` round-trips byte-identically and renders. The full 1983 command set
— 57 commands — has been recovered from `ACAD.OVL`; none are implemented yet.

**The §8 decision gate did not pass (1 of 3).** Transpiling the decompiler's AST
to Rust is therefore *not* adopted: `acad-re` stays an understanding tool and all
shipped Rust is hand-written. The measurements behind that are in
[`docs/re-pipeline.md`](docs/re-pipeline.md).

④'s read direction runs before ③, out of spec order, because `SUBDIV.DWG` and
`SUBDIV.DXF` are the same drawing in both formats — ground truth that needs no
emulator (spec §4.4). The QEMU oracle now also supplies evidence for `AC1.40`
reading; the write direction remains future work.

`acad-oracle` now has a QEMU-backed development probe. It boots disposable
copies of the original floppies, creates an empty `AC1.40` drawing, and asks
AutoCAD to export five `AC1.2` and four `AC1.40` sample drawings as DXF. The tests compare the
empty export byte for byte and the sample exports against the DWG reader's
geometry, including `POINT`, `TRACE`, `SOLID`, and entities inside `REPEAT`.
It also creates circles, a line, a three-point arc, and rotated text, checking
geometry against the command inputs and the decoded DWG against the exported
DXF. Nondefault header settings pin BASE, fill mode, and the layer table.
A QEMU 11.0.1 branch bug requires
disabling TCG block chaining (`-d nochain`). QEMU must be installed for this
probe; the spec's in-tree, CI-independent oracle remains future work. See
[the oracle note](docs/oracle-qemu.md).

`SUBDIV` is the corpus's only drawing with a DXF sibling on disk. Seven
`AC1.2` record types were initially verified against it: `LINE`, `CIRCLE`,
`ARC`, `TEXT`, `BLOCK`, `ENDBLK`, `INSERT`. AutoCAD's freshly generated DXF
exports now independently check `POINT`, `TRACE`, `SOLID`, and the entities
carried by `REPEAT`/`ENDREP`. The repeat construct's full semantics remain
unmodelled. A signed type code marking an
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

The render smoke test lights 21,439 pixels for `SELEXOL` and 31,729 for
`BLIVET` on an 800×800 canvas. Their newly exported DXFs independently
check the decoded entity geometry. They do not verify every pixel or the
renderer's transform composition.

`acad-app` opens either format, dispatching on the file's own magic bytes rather
than its extension, so a `.BAK` file — four corpus drawings have one — is
identified as the DWG it is rather than guessed from its name. The four
`AC1.40` drawings (`HOUSE`, `COLORS`, `OFFICE`, `SHUTTLE`) and the three
matching backups now parse and render. `DISC.BAK` remains unsupported at its
font `LOAD` record (`ROMAN-S`); the reader reports the type and byte offset.

## Build

Rust 1.88.0, pinned by `rust-toolchain.toml`.

    cargo test --workspace
    cargo run -p acad-app        # renders SUBDIV.DXF in a window
    cargo run -p acad-app -- corpus/Samples/SUBDIV.DWG   # or the DWG sibling
    cargo run -p acad-app -- corpus/Samples/HOUSE.DWG    # AC1.40

Tests that need the corpus skip when it is absent, so a fresh checkout is green
without it.

## Crates

| Crate | |
|---|---|
| `acad-model` | Entity model. The one crate everything depends on, and the one written for the future rather than for 1983. |
| `acad-dxf` | The 1983 `KEYWORD,n` DXF codec (entity suffixes denote layers) — not the modern group-code format. |
| `acad-dwg` | The 1983 binary DWG reader for `AC1.2` and `AC1.40`. Font LOAD/SHAPE and writing remain unsupported. Depends only on `acad-model`. |
| `acad-render` | Viewport fit and entity flattening (numerically testable), then rasterisation. |
| `acad-app` | Window, via `winit` + `softbuffer`. Opens either `.DXF` or `.DWG`, chosen by the file's magic bytes. |
| `acad-corpus` | Generates `corpus/manifest.toml`, the integrity record. |
| `acad-re` | **Dev only.** `ACAD.OVL` container codec, typed Ghidra AST, call graph, command recovery, gate metrics. Nothing shipped depends on it. |
| `acad-oracle` | **Dev only.** QEMU probe for original AutoCAD exports. Requires QEMU and the extracted floppies. |

Still to come, per spec §5: `acad-cmd` and the in-tree 8086 oracle core.

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
