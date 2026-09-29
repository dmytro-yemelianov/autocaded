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
| ③ | Oracle harness | in progress: partial 8086/DOS/BIOS core; in-tree empty, LINE, POINT, CIRCLE, ARC, SOLID, and TRACE DWGs match QEMU byte for byte, as does a LINE CGA frame |
| ④ | DWG codec — `AC1.40`, then `AC1.2` | all 21 corpus drawings read; both writers open in the original; SUBDIV's AC1.2 viewport matches pixel-for-pixel |
| ⑤ | Command loop | 32 of 57 recovered command names recognized; prompt, selection, and differential coverage remain incomplete |

`SUBDIV.DXF` round-trips byte-identically and renders. The full 1983 command set
— 57 commands — has been recovered from `ACAD.OVL`; implementation has started
with geometry creation and basic editing. The current editor supports LINE, CIRCLE,
POINT, ARC, TEXT, ID, BLOCK, INSERT, LIST, ERASE, MOVE, COPY, ROTATE, SCALE, UNDO, drawing settings,
ZOOM, PAN, SAVE, and END/QUIT. Editing selection uses the IDs reported by LIST or
`ALL`; MOVE and COPY use base and destination points. The basic geometry creation
and view commands have QEMU oracle coverage. The new edit transforms currently have
Rust model tests only: the original editor's selection prompts still need to be
driven correctly before these operations can be compared against AutoCAD.

**The §8 decision gate did not pass (1 of 3).** Transpiling the decompiler's AST
to Rust is therefore *not* adopted: `acad-re` stays an understanding tool and all
shipped Rust is hand-written. The measurements behind that are in
[`docs/re-pipeline.md`](docs/re-pipeline.md).

④'s read direction runs before ③, out of spec order, because `SUBDIV.DWG` and
`SUBDIV.DXF` are the same drawing in both formats — ground truth that needs no
emulator (spec §4.4). The QEMU oracle also supplies evidence for `AC1.40`
reading and writing.

`acad-oracle` now has a QEMU-backed development probe. It boots disposable
copies of the original floppies, creates an empty `AC1.40` drawing, and asks
AutoCAD to export five `AC1.2` and five `AC1.40` sample drawings as DXF (including
`DISC.BAK`, renamed on the disposable copy). The tests compare the
empty export byte for byte and the sample exports against the DWG reader's
geometry, including `POINT`, `TRACE`, `SOLID`, and entities inside `REPEAT`.
It also creates circles, a line, a three-point arc, and rotated text, checking
geometry against the command inputs and the decoded DWG against the exported
DXF. Nondefault header settings pin BASE, fill mode, and the layer table.
Generated `LOAD B:ES` and `SHAPE` commands verify library names, definition
numbers, placement, scale, and rotation. The renderer now interprets all seven
supplied `.SHP` libraries, including ordered font changes and shape subroutines.
A CGA-memory comparison checks native text and shape strokes against the original
display. See [font rendering](docs/shp-rendering.md) for coverage and limits.
Entity layers now survive DWG and DXF parsing and DXF writing; the renderer
resolves the layer color table into RGB strokes, and `FILL` controls the
interiors of `TRACE` and `SOLID` polygons.
A QEMU 11.0.1 branch bug requires
disabling TCG block chaining (`-d nochain`). QEMU must be installed for this
probe; the in-tree runner verifies seven DWG save cases and captures a LINE
editor frame and an ID query frame identical to QEMU's CGA memory. TEXT's computed Y extent differs
by one ULP; its cause is still being traced. See
[the oracle note](docs/oracle-qemu.md).

`SUBDIV` is the corpus's only drawing with a DXF sibling on disk. Seven
`AC1.2` record types were initially verified against it: `LINE`, `CIRCLE`,
`ARC`, `TEXT`, `BLOCK`, `ENDBLK`, `INSERT`. AutoCAD's freshly generated DXF
exports now independently check `POINT`, `TRACE`, `SOLID`, and the entities
carried by `REPEAT`/`ENDREP`. The model now keeps rectangular patterns, including
the two nested in BLIVET blocks; a 2×2 original command probe verifies the
saved counts, spacing, DXF structure, and rendered positions. The opening
record's extra word and point-specified distances still need investigation.
A signed type code marking an
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

This is the structural model used for the 1983 block table: block definitions
are flat entries, while the `BLOCK`/`ENDBLK` byte spans in the entity stream
group each entry's body. Nesting a definition inside another definition in
the stream does not make it a child; references resolve by name across the
table. That conclusion is grounded in the `SELEXOL` and `BLIVET` corpus
evidence above, not a claim that all possible 1983 files have been tested.

Grouping the blocks correctly still wasn't enough to draw them: both files
also `INSERT` one block from inside another block's own body, so the renderer's
`INSERT` expansion had to become recursive, composing each nesting level's own
translate/scale/rotate transform with the one enclosing it, with a depth cap
(ours, not a recovered 1983 constant — the corpus never nests past depth 2)
against a self-referencing `INSERT`.

The render smoke test lights 40,006 pixels for `SELEXOL` and 35,699 for
`BLIVET` on an 800×800 canvas. Their newly exported DXFs independently
check the decoded entity geometry. They do not verify every pixel or the
renderer's transform composition.

`acad-app` opens either format, dispatching on the file's own magic bytes rather
than its extension, so a `.BAK` file — four corpus drawings have one — is
identified as the DWG it is rather than guessed from its name. The four
`AC1.40` drawings (`HOUSE`, `COLORS`, `OFFICE`, `SHUTTLE`) and the three
matching backups now parse and render. `DISC.BAK` also opens, preserving its
`ROMAN-S` and `ITALIC` font loads; the shuttle labels and italic STAR WARS title
now render. Entity layers, the indexed ACI palette, and filled TRACE/SOLID
rendering are preserved. AC1.40 DWG output matches original QEMU-generated
LINE, CIRCLE, and POINT record bytes and opens in AutoCAD under QEMU, with a
CGA drawing viewport matching the native reference within 0.1%. AC1.2 output
also opens, with SUBDIV's
viewport matching pixel-for-pixel. Unknown fixed-header bytes from DWG input
are carried through by the writer; their meanings remain to be recovered.

## Build

Rust 1.88.0, pinned by `rust-toolchain.toml`.

    cargo test --workspace
    cargo run -p acad-app        # renders SUBDIV.DXF in a window
    cargo run -p acad-app -- corpus/Samples/SUBDIV.DWG   # or the DWG sibling
    cargo run -p acad-app -- corpus/Samples/HOUSE.DWG    # AC1.40
    cargo run -p acad-app -- corpus/Samples/DISC.BAK     # fonts included
    cargo run -p acad-render --example render-png -- corpus/Samples/DISC.BAK /tmp/disc.png

The current command loop accepts `LINE`, `CIRCLE`, `POINT`, `SOLID`, `TRACE`, three-point `ARC`,
`TEXT`, `BLOCK` (name, base point, then `LAST`, `ALL` or entity IDs),
`INSERT` for existing blocks (insertion point, optional independent
X/Y scales and rotation, or an opposite corner point to set both scales),
`INSERT *name` to copy a block's component entities
at a new insertion point, `BREAK` on lines, arcs and circles, `ARRAY`, line-to-line
`FILLET`,
`CHANGE` (assign selected entities to a layer), `DIST`, `ID`, point-by-point `AREA`,
`ENTITYAREA` for circles, quadrilaterals, and closed line loops (a Rust extension),
`OOPS` (restore the last ERASE),
`UNDO`, `BASE`,
`SNAP`, `GRID`, `ORTHO`, `FILL`, `LIMITS`, `LAYER`,
`COLOR`, numeric-factor `ZOOM`, `ZOOM E` (Extents), `ZOOM W` (Window),
`ZOOM C` (Center), `ZOOM P` (Previous), coordinate-based `PAN`, `SAVE`
(then an output path), and `END`/`QUIT`. Output paths ending in `.dxf`
use the DXF writer;
other paths use the AC1.40 DWG writer.

The app and PNG renderer accept additional font directories after the drawing
(after the output path for PNG). Explicit directories take precedence; the
extracted corpus uses `System/TXT.SHP` as AutoCAD 1.4's startup font and finds
other libraries beside the drawing. Missing libraries/glyphs are reported.

Tests that need the corpus skip when it is absent, so a fresh checkout is green
without it.

## Crates

| Crate | |
|---|---|
| `acad-model` | Entity model. The one crate everything depends on, and the one written for the future rather than for 1983. |
| `acad-dxf` | The 1983 `KEYWORD,n` DXF codec (entity suffixes denote layers) — not the modern group-code format. |
| `acad-dwg` | The 1983 binary DWG reader/writer for `AC1.2` and `AC1.40`, including LOAD/SHAPE records. Depends only on `acad-model`. |
| `acad-render` | Viewport fit, entity/block transforms, SHP font/shape interpretation, then rasterisation. |
| `acad-app` | Window, via `winit` + `softbuffer`. Opens either `.DXF` or `.DWG`, chosen by the file's magic bytes. |
| `acad-corpus` | Generates `corpus/manifest.toml`, the integrity record. |
| `acad-re` | **Dev only.** `ACAD.OVL` container codec, typed Ghidra AST, call graph, command recovery, gate metrics. Nothing shipped depends on it. |
| `acad-oracle` | **Dev only.** Partial 8086 real-mode core, MZ loader, DOS/BIOS services, and QEMU probe for the original. QEMU comparisons require the extracted floppies. |

Still to come, per spec §5: broader command coverage and remaining precision work in the
in-tree oracle, then the remaining command/menu behavior.

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
