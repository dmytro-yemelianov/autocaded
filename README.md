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
COPY, ROTATE, SCALE, UNDO, AXIS, drawing settings, ZOOM, PAN and SAVE.
END saves the current document before exiting; an unnamed drawing asks for a
path. QUIT and window close require Y/YES to discard changes. Session tracks
the document path, detected format and unsaved changes (marked `*` in the title).
Editing selection uses canonical IDs reported by LIST, `ALL` or `LAST`: one
live top-level object or REPEAT owner, excluding LOAD and erased/definition
records. Explicit selectors may edit hidden objects; mouse/window/highlight
paths filter visible geometry. MOVE and COPY accept a displacement or base and destination points. The basic geometry creation
and view commands have retained original observations. STATUS reports drawing
extents, limits, view and mode/size settings in the window, terminal and API/MCP;
its layout is a Rust policy. STATUS, REDRAW and REGEN do not alter
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
Mouse point placement now applies SNAP on the world-origin grid and ORTHO from
the current command's anchor. The crosshair previews the exact submitted point;
menu toggles take effect during an active prompt. Typed coordinates remain exact,
and view/selection windows remain free. These mouse rules are tested Rust app
policies; original digitizer behavior has not been differentially verified.
Creating geometry updates drawing extents while preserving the current view and
LIMITS, so mouse drafting keeps a stable coordinate mapping. App workflow tests
cover keyboard commands, constrained clicks, UNDO, saving and reopening DWG/DXF
with spaces in filenames, and recovery after a failed save. DWG retains exact
entity data; historical DXF writes numeric values to six decimal places.
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
text mode and shows a `LINE` record. The native app displays the complete report
in its window and prints it to the launching terminal. Its wrapping, paging and
controls are Rust interface policies; original screen/layout parity remains open.

The newly recognized command slice has different levels of coverage. `?` shows
the recovered command list; HELP now returns retained ACAD.HLP pages for all
57 dispatcher names, including aliases. The text is embedded in the binary;
only LINE has a retained original help-screen comparison. The pages describe
original options, some of which are still unimplemented. LINE now supports C
to close a sequence at its exact first vertex. See the
[command implementation audit](docs/native-command-matrix.md) for per-command
behavior, missing options and evidence. `FILES` enters the File Utility
Menu; list-by-type, wildcard listing, delete, and rename operations now run on
the host filesystem. Drive A defaults to the current directory; set
`AUTOCAD_DRIVE_<letter>` to map a DOS drive letter to a directory (other drives
require a mapping). `MENU` parses the selected `.MNU` file and retains its
labels and exact command macro bytes. Its clickable screen panel uses recovered
repeat-marker page boundaries, wraps NEXT, and dispatches plain-text macros.
Blank panel slots consume clicks, mixed-case labels render, and loaded menus
keep the complete panel accessible through a minimum window size. GO, text
macros and the retained SNAP/ORTHO/cancel control bytes dispatch through the
shared session; see [the handover](docs/HANDOVER-2026-09-30.md).
`RES` and `RESOLUTION` share
SNAP state, and `UNITS` stores its format and precision in AC1.40 DWG. Command
scripts (`SCRIPT`, `--script`, API/MCP) run `DELAY` as a nonblocking native
millisecond pause and `RESUME` continues an interrupted script; see
[the scripts contract](docs/native-scripts.md). `DIM` writes orthogonal LINE, SOLID, and TEXT primitives; A controls
arrow size, T independently controls inside/outside text orientation, and B/C
use the preceding dimension's baseline/continuation history. UNDO restores that
history with the drawing. The Rust editor is compared offline against 29 retained
native DWGs, including fit boundaries, font ink widths and ordered primitives.
Large-arrow external text placement and other unmeasured cases remain open;
see [the DIM fixture coverage](crates/acad-cmd/tests/fixtures/dim/README.md).
DIMARROW is supported in AC1.40 DWG and original DXF.
DIM's width calculation now uses derived TXT metrics for all 94 defined printable
characters, including letters and plus signs, rather than numeric-width guesses.
All 8,836 glyph pairs were checked against renderer strokes; this extends font
coverage without adding original DIM placement observations. See
[TXT metric coverage](crates/acad-cmd/tests/fixtures/dim/txt-metrics.md).
HATCH reports the captured pattern list and follows the
observed pattern/scale/angle/object-selection prompts. The `LINE` hatch pattern
clips against selected closed LINE/ARC loops and circles, and is verified
against the original for default settings, scale 2 / angle 30°, circle,
nested-hole, and semicircle-plus-chord boundaries. `NET` adds a perpendicular
second family using the same clipping engine, with a shared 100,000-stroke
limit. Its default rectangle matches the retained original DWG's complete
ordered block and INSERT; scale/rotation and circular holes have additional
Rust tests. See [HATCH fixture coverage](crates/acad-cmd/tests/fixtures/hatch/README.md).
`GRATE`, `NET3`, `PLAST`, `PLASTI` and `STEEL` now use continuous families
from the retained ACAD.PAT definitions, including each row's origin, angle and
spacing. Geometry tests cover full rectangle strokes, rotation/scale, circular
holes, DWG save/reopen, UNDO and atomic overflow. These five patterns have no
retained native exports; `PLAST`/`PLASTI` row order is checked against the in-tree original.
The other 14 patterns without zero-length dots now use the retained signed
dash/gap sequences and per-row drift. Dash phase is anchored to each definition
origin and continues through holes. Their rotation, clipping, DWG and undo
contracts have Rust tests; native output parity remains unchecked. See
[dash definition coverage](crates/acad-cmd/tests/fixtures/hatch/dashed-patterns.md).
`MUDST` and `SACNCR` now interpret zero dash entries as POINT entities alongside
their LINE strokes. All 23 catalogue patterns have geometry. Dots share the
global phase, clipping and entity budget, save through DWG/DXF and use the
renderer’s existing screen-sized POINT marker. Their primitive representation
and boundary policy have Rust contracts, not original-export parity; see
[dot definition coverage](crates/acad-cmd/tests/fixtures/hatch/dot-patterns.md).
Island styles (`name,N|O|I`) follow [the styles contract](docs/native-hatch-styles.md).
`U` user patterns and external ACAD.PAT-syntax pattern files follow
[the U/PAT contract](docs/native-hatch-user.md); the in-tree original confirms
`U` geometry, the sweep's start row and continuous-row direction, `PLAST`,
`PLASTI`, `TRANS` and `INSUL` row order, and that the original reads ACAD.PAT at run time.
`SKETCH` records mouse freehand lines through GUI motion and API/MCP `motion`,
checked against the original under a QEMU mouse oracle; see
[the SKETCH contract](docs/native-sketch.md).
QEMU currently checks the command list/help page,
FILES menu entry, shared resolution state, and persisted Decimal precision.

The recovered table contains 57 names. `TABLET`, `PLOT`, and `QPLOT` require
digitizer or plotter hardware and are excluded; all other 54 names are now
recognized by the native editor. Recognition is only a progress count: several
commands still have prompt shells or partial behavior, and each command needs
its observable prompts, effects, and file behavior (where applicable) verified
against the retained specifications and evidence before this milestone is complete.
Original-output parity gaps remain explicit. The active
[native completion plan](docs/superpowers/plans/2026-10-03-native-editor-completion.md)
uses existing recovery artifacts without restarting DOS/oracle collection.
Remaining work runs through [bounded implementation/review loops](docs/superpowers/plans/2026-10-04-agentic-native-completion.md)
with a [progress ledger](docs/superpowers/plans/2026-10-04-agentic-progress.json).
Canonical selection, multiple picks/windows and selected LIST are verified, as
are LAYER ON/OFF/COLOR/? and signed OFF persistence for defined layers/colors
1..127 in DWG AC1.2/AC1.40 and historical DXF. Unsupported OFF edge states still
fail before file replacement. Retained static code establishes the signed mapping;
no new original OFF export was measured. Independent review, native regressions
and attached GUI/API/MCP save/reopen checks passed. Nested live and native
uniform-erased REPEAT/BLOCK DWG persistence, live DXF export and bounded WBLOCK
resource-context preservation are verified. Erased ordinary members inside live
groups and blocks are retained in DWG; a whole erased owner with prior member
erasure still checked-refuses DWG output. TEXT C/R/A uses runtime SHP ink metrics;
point height/angle, repeated lines and single-text CHANGE are integrated.
Alignment/history are session-local native policies. The combined checkpoint
passed 520 native tests, Clippy, workspace compilation and independent source
review; actual GUI/MCP text, group and DWG/DXF save/reopen workflows passed.
Rendering preflights each owner's expansion and preserves bounded ordered LOAD
context. The resource walker and selection/budget policies are separate modules.

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

Startup loads the shipped `ACAD.MNU` screen menu alongside the requested drawing.
The bottom command area shows the current prompt, text as you type, and the latest
status or error. Type a command or response and press Return; menu GO uses the
same submission path. Backspace edits input and Escape cancels the active command.
Long input scrolls to its end and caret. Clicks in this area do not place points or
select entities. HELP, STATUS, LIST, DBLIST, FILES and HATCH pattern reports open
a full-canvas text viewer above this strip. Use Up/Down, Page Up/Down, Home/End
or the mouse wheel; click PREV/NEXT/CLOSE (UP/DN/X in narrow frames). Escape or
an empty Return closes the viewer without answering the pending command prompt.
Typing resumes command input. Reports also print to the launching terminal.
The viewer wraps long rows and keeps a text anchor across width changes. It
preserves drawing/undo state; body clicks do not place or select drawing entities.
The bitmap viewer covers printable ASCII and displays other characters as `?`;
the complete original UTF-8 report remains available through API/MCP.

Use `MENU`, then a menu filename (for example `ACAD`), to load or reload a menu;
Return at its filename prompt unloads the panel while leaving the command area
visible. A missing menu file is reported in the status line and keyboard commands
remain available. The menu loader searches the existing current-path and corpus
locations. The window minimum accommodates all ACAD rows, NEXT, and the command
area (160×380 physical client pixels); these interface improvements do not claim
native pixel fidelity or command-history support.

The current command loop accepts `LINE`, `CIRCLE`, `POINT`, `SOLID`, `TRACE`, `ARC`,
`TEXT`, `BLOCK` (name, base point, then `LAST`, `ALL` or entity IDs),
`INSERT` for existing blocks (insertion point, optional independent
X/Y scales and rotation, or an opposite corner point to set both scales),
`INSERT *name` to copy a block's component entities
at a new insertion point, `INSERT` of an external drawing file
([contract](docs/native-external-insert.md)), `BREAK` on lines, arcs, circles
and traces, rectangular and
circular `ARRAY`, line-to-line
`FILLET`,
`CHANGE` (move selected LINE/CIRCLE/INSERT geometry, edit a single TEXT, or
assign a layer),
`DIST`, `ID`, point-by-point `AREA`,
`ENTITYAREA` for circles, quadrilaterals, and closed line loops (a Rust extension),
`OOPS` (restore the last ERASE),
`UNDO`, `BASE`,
`SNAP`, `GRID`, `ORTHO`, `FILL`, `LIMITS`, `LAYER`,
`COLOR`, numeric-factor `ZOOM`, `ZOOM E` (Extents), `ZOOM W` (Window),
`ZOOM C` (Center), `ZOOM P` (Previous), coordinate-based `PAN`, `SAVE`
(then an output path), and `END`/`QUIT`. SAVE output paths ending in `.dxf`
use the DXF writer;
other paths use the AC1.40 DWG writer.
END preserves the detected source codec/revision, including AC1.2 or a file
with a misleading suffix; unsupported encoding fails with the document still
open. SAVE can select another destination/format. Complete output is staged
beside the destination and renamed after successful encoding/writing, preserving
old file bytes on failure; existing symlinks are followed and permissions retained.
SAVE/END replacing an existing file keeps its previous bytes as a `.BAK` backup
(none for `.bak` destinations or WBLOCK; [files/menu contract](docs/native-files-menu.md)).
WBLOCK exports do not change the current document path or saved baseline.

GRID paints world-origin dots within LIMITS, including in API frames. `0` follows
the current SNAP interval, and `nX` sets an interval of n times SNAP at command
entry. Zoomed-out grids display a coarser lattice to bound drawing work; stored
GRID/SNAP values stay unchanged. Dot color, clipping and density are Rust display
policies, without a retained original GRID screen comparison.

CIRCLE accepts a center followed by a positive numeric radius or a circumference
point; `D` selects a numeric diameter. At the first prompt, `2P` accepts diameter
endpoints and `3P` accepts three circumference points. Subsequent points support
relative/polar input; mouse points use SNAP and stay free of ORTHO projection.
Coincident/collinear or unrepresentable constructions retain their prompt for
retry and create no entity/undo entry. The new forms follow retained HLP with
Rust geometry contracts; their original export/screen parity is unestablished.

ARC supports three points, start/center/end direction, start/center/angle or
chord, and start/end/radius, angle or starting direction. Initial `C` selects
center first; `C`/`E` at the second prompt select the other branches. Included
angles are signed (positive CCW, negative CW); negative radius/chord selects
the major CCW arc. Starting direction accepts a degree angle or a point
relative to the start. Zero/full-circle angles, impossible chords/radii,
parallel starting directions and nonfinite results remain retryable without
adding an entity or undo entry. These dialogue/sign choices are Rust contracts
guided by retained HLP; only the existing three-point path has native comparisons.

Return at LINE's first point resumes the last explicitly created LINE/ARC
endpoint. Return at ARC's first point also uses its terminal tangent and asks
only for an end point. The exact entered end survives clockwise record-angle
reordering. This history survives unrelated additions and reports, is restored
by UNDO, and is cleared when its source is changed/erased or a drawing is
opened/new. LINE continuation permits any next point; ARC continuation keeps
the tangent. This session history is not inferred from imported file order.
ARC mouse/API points use SNAP without ORTHO; continued LINE keeps ORTHO's anchor.

SOLID accepts Return at its fourth point for a triangle, storing `p4 = p3`.
Return at the next third-point prompt ends the sequence. Continuation always
reuses the stored third/fourth pair, including their coincidence after a
triangle; supply two new distinct points to create another nondegenerate
section from that collapsed edge. This triangle continuation policy has Rust
coverage, while the original retained export covers quadrilateral chaining.

The app and PNG renderer accept additional font directories after the drawing
(after the output path for PNG). Explicit directories take precedence; the
extracted corpus uses `System/TXT.SHP` as AutoCAD 1.4's startup font and finds
other libraries beside the drawing. Missing libraries/glyphs are reported.

Tests that need the corpus skip when it is absent, so a fresh checkout is green
without it.

## Native API and MCP

The native GUI and automation share `acad_app::Session` and the same CPU frame
composer. The window owns its session; attached API requests run on the window's
event loop, so commands, mouse input and captured frames use one drawing.

Start a window with its opt-in local API (Unix/macOS/Linux):

```sh
cargo run -p acad-app --bin acad -- corpus/Samples/SUBDIV.DXF --api-socket /tmp/acad-rust.sock
python3 tools/acad_api.py --socket /tmp/acad-rust.sock state
python3 tools/acad_api.py --socket /tmp/acad-rust.sock command --params '{"input":"LINE"}'
python3 tools/acad_api.py --socket /tmp/acad-rust.sock point --params '{"x":1,"y":2}'
python3 tools/acad_api.py --socket /tmp/acad-rust.sock point --params '{"x":5,"y":4}'
python3 tools/acad_api.py --socket /tmp/acad-rust.sock command --params '{"input":""}'
python3 tools/acad_api.py --socket /tmp/acad-rust.sock frame --output /tmp/acad-frame.png
```

Each socket connection sends one newline-delimited JSON request, for example
`{"method":"command","params":{"input":"LINE"}}`, and receives
`{"result":...}` or `{"error":"..."}`. The socket has mode `0600`, refuses an
existing path, and is removed on normal shutdown. Submit one prompt answer per
call and inspect `state`; request errors retain the drawing. A timed-out mutation
must not be automatically retried because it may already have run.

Build and launch the MCP stdio adapter in either mode:

```sh
cargo build -p acad-app --bins
# Control the already-open window:
target/debug/acad-mcp --socket /tmp/acad-rust.sock
# Own a session without opening a window:
target/debug/acad-mcp --drawing corpus/Samples/SUBDIV.DXF --fonts corpus/System
# Or start with an empty drawing:
target/debug/acad-mcp --fonts corpus/System
```

These commands are server entry points for an MCP client, not interactive terminal
prompts. The adapter uses the
[MCP 2025-11-25 initialization and tool protocol](https://modelcontextprotocol.io/specification/2025-11-25/basic/lifecycle)
over stdio; older handshake versions are also negotiated. Reports go to stderr
and structured tool results, keeping stdout exclusively for protocol messages.
Example client configuration for an attached window:

```json
{
  "mcpServers": {
    "acad": {
      "command": "/absolute/path/to/autorust/target/debug/acad-mcp",
      "args": ["--socket", "/tmp/acad-rust.sock"]
    }
  }
}
```

| MCP tool / API method | Arguments and behavior |
|---|---|
| `acad_new` / `new` | Empty drawing, retaining loaded fonts/libraries and menu |
| `acad_open` / `open` | `path`, optional `directories` for SHP fonts/libraries |
| `acad_command` / `command` | `input`: command or one prompt answer; blank string means Return |
| `acad_point` / `point` | World `x`, `y`, using SNAP/ORTHO; exact typed points use `command` |
| `acad_click` / `click` | Physical client `x`, `y`, optional `width`, `height`; menu/selection routes |
| `acad_motion` / `motion` | Physical client `x`, `y`, optional `width`, `height`; pointer motion without a click (SKETCH sampling, see `docs/native-sketch.md`) |
| `acad_state` / `state` | Prompt, input, status/full report, `sketch` (pen, mode, temporary line count, or null), `script` status, `report_view` visibility/text anchor, counts, view, limits, layers/OFF layers, FILLET radius, SNAP/GRID/ORTHO and document `path`, `format`, `dirty` |
| `acad_drawing` / `drawing` | Current geometry as historical DXF text, without writing a file |
| `acad_save` / `save` | `path`; `.dxf` is case-insensitive, other extensions write DWG |
| `acad_cancel` / `cancel` | Cancel the current prompt |
| `acad_report` / `report` | `action`: `up`, `down`, `page_up`, `page_down`, `home`, `end`, `close`, `open`; optional frame `width`, `height` |
| `acad_frame` / `frame` | Optional `width`, `height`, `format`: `png` (default) or `rgba` |
| `acad_quit` / `quit` | Enter QUIT confirmation; optional `discard: true` explicitly exits without saving |
| `acad_script` / `script` | `path`: start a command script (`.SCR` added without an extension); runs at most 64 due items |
| `acad_script_status` / `script_status` | Read-only script state, next line/offset, remaining DELAY and interrupt cause |
| `acad_script_tick` / `script_tick` | Run due script items (at most 64, never waits); repeat while running or after a DELAY |
| `acad_script_stop` / `script_stop` | Discard the current or interrupted script |

Reply to QUIT with `command {"input":"Y"}` or `YES`; other answers or `cancel`
keep the session open. Automated teardown can use `quit {"discard":true}`.
To save and exit, submit `command {"input":"END"}` (then a path if unnamed).
Explicit API `new`/`open` replace the current drawing without a confirmation
dialogue. Successful `save` attaches its path and clears `dirty`; failed saves
retain the previous attachment and baseline. Dirty state compares drawing data,
so UNDO back to the saved drawing clears it; reports and frame requests do not
make a drawing dirty.

Report navigation changes only the viewer. `close` retains the complete text
for `open`; the next editor effect or `new`/`open` clears it. `cancel` cancels
the editor prompt and clears the report. Supply the dimensions of the frame
being viewed for matching page sizes. GUI empty Return closes a visible viewer;
API `command` always submits editor input, so automation should use `report` to
close or navigate. `report_view.anchor` is an opaque display-text position, not
an offset to slice the original report.

Frames include the client drawing, menu, command area, selection and cursor;
OS title bars are excluded. PNG is returned as an MCP image. Raw frames contain
base64 RGBA8 bytes, top-down rows, opaque alpha and stride `width * 4`. Poll
`frame` for successive buffers; no video encoder or push-stream is bundled.
Frame/click dimensions default to the live window, or 800×600 when headless.
Transport frames are limited to 4096 per dimension and 4,194,304 pixels; provide
smaller explicit dimensions for a larger Retina window. The GUI composer retains
support for larger physical windows. `tools/acad_api.py --output` decodes frame
bytes directly into a PNG or raw file. Font diagnostics accompany captured frames.

The public Rust API exposes `Session::new/open`, `command`, `point`, `click`,
`drawing`, `prompt`, `status`, `save`, `document_path`, `document_format`,
`is_dirty`, `request_quit`, `report_text`, `report_visible`, `report_action` and
`frame`; `Frame` provides RGB words for the
window plus `rgba()` and `png()` for callers. The app is split into session
commands/resources/effects/mouse modules, document I/O, bitmap painting,
presentation, a separate report viewer with cached wrapping and painting,
window adapter, typed API, socket bridge and MCP adapter. Command
prompt handling is split by domain; HATCH clipping, area metrics and editing
geometry are separate modules. Existing native-export parity tests still run
against the same recovered algorithms.

Run the attached-window regression on a Unix desktop with a graphical session:

```sh
cargo build -p acad-app --bins
python3 tools/check_acad_gui_api.py
```

It creates scratch drawings, exercises the real GUI through both socket/API and
MCP, captures PNG/RGBA at three sizes, checks DWG/DXF reopen and UNDO, declines
QUIT, then saves/exits with END and checks socket cleanup. Logs, frames and the
reopened state remain in the printed temporary directory. It also checks report
pages, resize, mouse paging, the CLI report method and drawing/undo neutrality.
The drawing includes numeric-D, 2P and 3P CIRCLE forms, a triangular SOLID,
LINE/ARC continuation and a center/angle ARC.
It does not send
desktop mouse/keyboard events or write source fixtures.

Run the retained corpus through headless Session/MCP with the same binaries:

```sh
python3 tools/check_acad_corpus_api.py
```

This requires all 21 drawings, three additional Samples backups and SUBDIV.DXF,
with manifest hashes intact; missing files fail the check. It captures frames,
checks Save As AC1.40 and END's attached revision, and reopens scratch DWG/DXF
files in fresh processes. DWG canvas pixels and canonical DXF text must remain
stable. Historical DXF rounds to six decimal places and omits AXIS, so its pixel
differences are recorded separately. Results, PNGs and outputs remain in the
printed temporary directory. This workflow passed all 25 inputs at the current
native checkpoint; broader file/command compatibility work remains open.

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
