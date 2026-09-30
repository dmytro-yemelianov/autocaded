# autorust

Rebuilding **AutoCAD 1.4** (1983, MS-DOS) as a native Rust application for the
original 2D drafting workflow: commands, drawing semantics, and files in a modern
window.

The original is the oracle. "Is this command right?" is answered by differential
test against the real `ACAD.EXE`, not by judgement. 1983 policy is confined to
the codecs and the command layer; `acad-model` is written to grow, so deliberate
divergence later — 3D, modern UX, an extended entity model — does not mean
discarding the verified-compatible core.

Design: [`docs/superpowers/specs/2026-09-28-autocad-14-rust-design.md`](docs/superpowers/specs/2026-09-28-autocad-14-rust-design.md)

## Finish line

The project is complete when the native app can open and render every drawing
in the AutoCAD 1.4 sample corpus; create, edit, and save supported 2D drawings
with keyboard commands and mouse-based point placement and selection; and
exchange those drawings with the original under QEMU. Every software-only 2D
command in the recovered 57-command table must be implemented and
differentially checked against the original. Plotter and digitizer hardware
commands, AutoCAD 2.x, 3D, ADS, and later releases remain outside this goal;
each recovered command outside the scope must be explicitly classified.

## State

| | Milestone | |
|---|---|---|
| ① | DXF codec, corpus integrity, first render | **done** |
| ② | Ghidra overlay loader, dual AST export, `acad-re` | **done** |
| ③ | Oracle harness | in progress: partial 8086/DOS/BIOS core; in-tree empty, LINE, POINT, CIRCLE, ARC, SOLID, and TRACE DWGs match QEMU byte for byte, as does a LINE CGA frame |
| ④ | DWG codec — `AC1.40`, then `AC1.2` | all 21 corpus drawings read; both writers open in the original; SUBDIV's AC1.2 viewport matches pixel-for-pixel |
| ⑤ | Command loop | All 54 in-scope recovered command names are recognized; full behavior and differential coverage remain |

`SUBDIV.DXF` round-trips byte-identically and renders. The full 1983 command set
— 57 commands — has been recovered from `ACAD.OVL`; implementation has started
with geometry creation and basic editing. The current editor supports LINE, CIRCLE,
POINT, ARC, TEXT, LOAD, SHAPE, ID, BLOCK, INSERT, LIST, DBLIST, STATUS, REDRAW, REGEN, ERASE, MOVE,
COPY, ROTATE, SCALE, UNDO, AXIS, drawing settings, ZOOM, PAN, SAVE, and END/QUIT.
Editing selection uses the IDs reported by LIST or
`ALL`; MOVE and COPY accept a displacement or base and destination points. The basic geometry creation
and view commands have QEMU oracle coverage. STATUS, REDRAW and REGEN do not alter
drawing data; QEMU confirms that the original accepts them. The new edit transforms currently have
Rust model tests only for ROTATE and SCALE. MOVE and COPY now follow the
original displacement, optional second point, then selection prompt order;
QEMU checks both commands with `L` (Last), including both ways to enter a
displacement.
LOAD and SHAPE use names from the available SHP libraries; LOAD also accepts an
SHP file path and makes that library available for drawing and rendering. The
RES and CAP example matches the original's exported drawing records under QEMU.
The native window now displays a crosshair, routes clicks through point prompts,
maps entity clicks to the editor's one-based selection IDs, and highlights picked
entities in yellow while a selection prompt is active. Escape cancels the current
command. The entity picker uses the current view transform; point placement and
LINE/CIRCLE picking have direct tests.
ARRAY follows the original's rectangular `R` and circular `C` prompts.
Generated 2×3 rectangular and non-origin circular drawings match the original
in both the in-tree runner and QEMU, including entity record order and the
original's orientation-preserving circular copies.
LINE also accepts relative `@dx,dy` and polar `@distance<angle` points from its
previous vertex; a generated mixed-coordinate line matches both oracles.

**The §8 decision gate did not pass (1 of 3).** Transpiling the decompiler's AST
to Rust is therefore *not* adopted: `acad-re` stays an understanding tool and all
shipped Rust is hand-written. The measurements behind that are in
[`docs/re-pipeline.md`](docs/re-pipeline.md).

The executable Lean behavioral specification is in [`formal/`](formal/). It
models WBLOCK's observed whole-drawing, named-block, and selected-entity flows
as a contract for Rust, not as a recovered source-level AutoCAD model. Run it
with `cd formal && lake build`.

`DBLIST` reports live entity details, including block and repeat contents, and
leaves the drawing unchanged. The original QEMU check confirms it switches to
text mode and shows a `LINE` record. The native app prints the report to its
launching terminal; matching the original's complete text layout and paging
remains open.

The newly recognized command slice has different levels of coverage. `?` shows
the recovered command list; `HELP LINE` returns the captured native help page,
while other named help pages remain unrecovered. `FILES` enters the File Utility
Menu, but its list/delete/rename operations are not implemented. `MENU` prompts
for a file name but does not yet parse menu files. `RES` and `RESOLUTION` share
SNAP state, and `UNITS` stores its format and precision in AC1.40 DWG. `DELAY`
validates an interval but has no script queue to delay; `RESUME` is a no-op
without one. `DIM` writes LINE, SOLID, and TEXT primitives for the observed
linear dimension flow. HATCH reports the captured pattern list and follows the
observed pattern/scale/angle/object-selection prompts. The `LINE` hatch pattern
clips against selected closed LINE/ARC loops and circles, and is verified
against the original for default settings, scale 2 / angle 30°, circle,
nested-hole, and semicircle-plus-chord boundaries; other listed patterns
remain open. `SKETCH` stops after its
increment because it needs a digitizer.
QEMU currently checks the command list/help page,
FILES menu entry, shared resolution state, and persisted Decimal precision.

The recovered table contains 57 names. `TABLET`, `PLOT`, and `QPLOT` require
digitizer or plotter hardware and are excluded; all other 54 names are now
recognized by the native editor. Recognition is only a progress count: several
commands still have prompt shells or partial behavior, and each command needs
its observable prompts, effects, and file behavior (where applicable) verified
against AutoCAD under QEMU before this milestone is complete.

AXIS accepts ON/OFF or a positive tick interval; an `X` suffix multiplies the
current SNAP spacing. It draws ruler ticks at the graphics-window edges and is
stored in AC1.40 DWG headers at `0x1e0` (enabled) and `0x1e2` (spacing).
QEMU confirms the original's ON/OFF CGA difference, the exact flag and spacing
bytes, and identical DWG output from numeric versus SNAP-relative spacing.
The 1983 DXF header has no AXIS record, so DXF export omits this setting.

`WBLOCK` supports whole-drawing (`*`), named-block, and selected-entity exports.
It writes a separate DWG snapshot; whole-drawing export keeps live entities and
transitively referenced block definitions while omitting erased records and
unreferenced blocks. All three forms are compared with AutoCAD output under
QEMU and checked through the native DWG writer's parse/write round trip. The
prompt and snapshot contract is also executable in Lean under [`formal/`](formal/).

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
saved counts, spacing, DXF structure, and rendered positions. `ENDREP` now
accepts point inputs for both distances: the original derives column X and row
Y spacing from consecutive points, and the generated DWG matches both oracles.
The opening record's second word is 1 across generated probes with one or two
entities and different dimensions; its meaning remains unknown.
A signed type code marking an
erased entity (`ADDER`'s own finding) is confirmed the same way: reading the code
as `i16` makes `ADDER`'s walk land exactly, where reading it unsigned does not.
Erased records now remain in DWG document order and are omitted from rendering
and DXF export. QEMU verifies `ERASE L` saves a `-1` LINE record and `OOPS`
restores its positive type; Rust emits matching record bytes for both.

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

The 800×800 corpus render check measures 40,006 lit pixels for `SELEXOL` and
39,403 for `BLIVET`; `ORGATE`, the smallest of the 16 AC1.2 drawings, measures
2,466. The 500-pixel floor is a broad nonblank guard, not proof by itself that
nested inserts expanded correctly; recursive expansion also has direct
transform-composition tests in `acad-render`. Their newly exported DXFs
independently check decoded entity geometry, but not every rendered pixel.

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
at a new insertion point, `BREAK` on lines, arcs and circles, rectangular and
circular `ARRAY`, line-to-line
`FILLET`,
`CHANGE` (move selected LINE/CIRCLE/INSERT geometry or assign it to a layer),
`DIST`, `ID`, point-by-point `AREA`,
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

Still to come, per spec §5: implement and differentially verify the remaining
command behavior, complete the in-tree oracle, and finish menu and hardware
boundaries.

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
