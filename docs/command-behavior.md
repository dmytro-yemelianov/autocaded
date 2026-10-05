# Native command and window behavior

Per-command behavior, coverage and evidence for the native editor, moved
verbatim from the README's former "State" and "Build" sections. The
per-command audit is the [command implementation matrix](native-command-matrix.md);
codec and oracle findings are in [codec and oracle notes](codec-oracle-notes.md);
API/MCP semantics are in [the native API notes](native-api.md).

## Editor progress

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

## DBLIST

`DBLIST` reports live entity details, including block and repeat contents, and
leaves the drawing unchanged. The original QEMU check confirms it switches to
text mode and shows a `LINE` record. The native app displays the complete report
in its window and prints it to the launching terminal. Its wrapping, paging and
controls are Rust interface policies; original screen/layout parity remains open.

## Recognized command slice: HELP, FILES, MENU, DIM, HATCH, SKETCH

The newly recognized command slice has different levels of coverage. `?` shows
the recovered command list; HELP now returns retained ACAD.HLP pages for all
57 dispatcher names, including aliases. The text is embedded in the binary;
only LINE has a retained original help-screen comparison. The pages describe
original options, some of which are still unimplemented. LINE now supports C
to close a sequence at its exact first vertex. See the
[command implementation audit](native-command-matrix.md) for per-command
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
shared session; see [the handover](HANDOVER-2026-09-30.md).
`RES` and `RESOLUTION` share
SNAP state, and `UNITS` stores its format and precision in AC1.40 DWG. Command
scripts (`SCRIPT`, `--script`, API/MCP) run `DELAY` as a nonblocking native
millisecond pause and `RESUME` continues an interrupted script; see
[the scripts contract](native-scripts.md). `DIM` writes orthogonal LINE, SOLID, and TEXT primitives; A controls
arrow size, T independently controls inside/outside text orientation, and B/C
use the preceding dimension's baseline/continuation history. UNDO restores that
history with the drawing. The Rust editor is compared offline against 29 retained
native DWGs, including fit boundaries, font ink widths and ordered primitives.
Large-arrow external text placement and other unmeasured cases remain open;
see [the DIM fixture coverage](../crates/acad-cmd/tests/fixtures/dim/README.md).
DIMARROW is supported in AC1.40 DWG and original DXF.
DIM's width calculation now uses derived TXT metrics for all 94 defined printable
characters, including letters and plus signs, rather than numeric-width guesses.
All 8,836 glyph pairs were checked against renderer strokes; this extends font
coverage without adding original DIM placement observations. See
[TXT metric coverage](../crates/acad-cmd/tests/fixtures/dim/txt-metrics.md).
HATCH reports the captured pattern list and follows the
observed pattern/scale/angle/object-selection prompts. The `LINE` hatch pattern
clips against selected closed LINE/ARC loops and circles, and is verified
against the original for default settings, scale 2 / angle 30°, circle,
nested-hole, and semicircle-plus-chord boundaries. `NET` adds a perpendicular
second family using the same clipping engine, with a shared 100,000-stroke
limit. Its default rectangle matches the retained original DWG's complete
ordered block and INSERT; scale/rotation and circular holes have additional
Rust tests. See [HATCH fixture coverage](../crates/acad-cmd/tests/fixtures/hatch/README.md).
`GRATE`, `NET3`, `PLAST`, `PLASTI` and `STEEL` now use continuous families
from the retained ACAD.PAT definitions, including each row's origin, angle and
spacing. Geometry tests cover full rectangle strokes, rotation/scale, circular
holes, DWG save/reopen, UNDO and atomic overflow. These five patterns have no
retained native exports; `PLAST`/`PLASTI` row order is checked against the in-tree original.
The other 14 patterns without zero-length dots now use the retained signed
dash/gap sequences and per-row drift. Dash phase is anchored to each definition
origin and continues through holes. Their rotation, clipping, DWG and undo
contracts have Rust tests; native output parity remains unchecked. See
[dash definition coverage](../crates/acad-cmd/tests/fixtures/hatch/dashed-patterns.md).
`MUDST` and `SACNCR` now interpret zero dash entries as POINT entities alongside
their LINE strokes. All 23 catalogue patterns have geometry. Dots share the
global phase, clipping and entity budget, save through DWG/DXF and use the
renderer’s existing screen-sized POINT marker. Their primitive representation
and boundary policy have Rust contracts, not original-export parity; see
[dot definition coverage](../crates/acad-cmd/tests/fixtures/hatch/dot-patterns.md).
Island styles (`name,N|O|I`) follow [the styles contract](native-hatch-styles.md).
`U` user patterns and external ACAD.PAT-syntax pattern files follow
[the U/PAT contract](native-hatch-user.md); the in-tree original confirms
`U` geometry, the sweep's start row and continuous-row direction, `PLAST`,
`PLASTI`, `TRANS` and `INSUL` row order, and that the original reads ACAD.PAT at run time.
`SKETCH` records mouse freehand lines through GUI motion and API/MCP `motion`,
checked against the original under a QEMU mouse oracle; see
[the SKETCH contract](native-sketch.md).
QEMU currently checks the command list/help page,
FILES menu entry, shared resolution state, and persisted Decimal precision.

## Recognition count, plans and checkpoint

The recovered table contains 57 names. `TABLET`, `PLOT`, and `QPLOT` require
digitizer or plotter hardware and are excluded; all other 54 names are now
recognized by the native editor. Recognition is only a progress count: several
commands still have prompt shells or partial behavior, and each command needs
its observable prompts, effects, and file behavior (where applicable) verified
against the retained specifications and evidence before this milestone is complete.
Original-output parity gaps remain explicit. The active
[native completion plan](superpowers/plans/2026-10-03-native-editor-completion.md)
uses existing recovery artifacts without restarting DOS/oracle collection.
Remaining work runs through [bounded implementation/review loops](superpowers/plans/2026-10-04-agentic-native-completion.md)
with a [progress ledger](superpowers/plans/2026-10-04-agentic-progress.json).
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

## AXIS

AXIS accepts ON/OFF or a positive tick interval; an `X` suffix multiplies the
current SNAP spacing. It draws ruler ticks at the graphics-window edges and is
stored in AC1.40 DWG headers at `0x1e0` (enabled) and `0x1e2` (spacing).
QEMU confirms the original's ON/OFF CGA difference, the exact flag and spacing
bytes, and identical DWG output from numeric versus SNAP-relative spacing.
The 1983 DXF header has no AXIS record, so DXF export omits this setting.

## WBLOCK

`WBLOCK` supports whole-drawing (`*`), named-block, and selected-entity exports.
It writes a separate DWG snapshot; whole-drawing export keeps live entities and
transitively referenced block definitions while omitting erased records and
unreferenced blocks. All three forms are compared with AutoCAD output under
QEMU and checked through the native DWG writer's parse/write round trip. The
prompt and snapshot contract is also executable in Lean under [`formal/`](../formal/).

## Native window: command area, report viewer, MENU

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

## Accepted commands, SAVE and END

The current command loop accepts `LINE`, `CIRCLE`, `POINT`, `SOLID`, `TRACE`, `ARC`,
`TEXT`, `BLOCK` (name, base point, then `LAST`, `ALL` or entity IDs),
`INSERT` for existing blocks (insertion point, optional independent
X/Y scales and rotation, or an opposite corner point to set both scales),
`INSERT *name` to copy a block's component entities
at a new insertion point, `INSERT` of an external drawing file
([contract](native-external-insert.md)), `BREAK` on lines, arcs, circles
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
(none for `.bak` destinations or WBLOCK; [files/menu contract](native-files-menu.md)).
WBLOCK exports do not change the current document path or saved baseline.

## GRID

GRID paints world-origin dots within LIMITS, including in API frames. `0` follows
the current SNAP interval, and `nX` sets an interval of n times SNAP at command
entry. Zoomed-out grids display a coarser lattice to bound drawing work; stored
GRID/SNAP values stay unchanged. Dot color, clipping and density are Rust display
policies, without a retained original GRID screen comparison.

## CIRCLE

CIRCLE accepts a center followed by a positive numeric radius or a circumference
point; `D` selects a numeric diameter. At the first prompt, `2P` accepts diameter
endpoints and `3P` accepts three circumference points. Subsequent points support
relative/polar input; mouse points use SNAP and stay free of ORTHO projection.
Coincident/collinear or unrepresentable constructions retain their prompt for
retry and create no entity/undo entry. The new forms follow retained HLP with
Rust geometry contracts; their original export/screen parity is unestablished.

## ARC

ARC supports three points, start/center/end direction, start/center/angle or
chord, and start/end/radius, angle or starting direction. Initial `C` selects
center first; `C`/`E` at the second prompt select the other branches. Included
angles are signed (positive CCW, negative CW); negative radius/chord selects
the major CCW arc. Starting direction accepts a degree angle or a point
relative to the start. Zero/full-circle angles, impossible chords/radii,
parallel starting directions and nonfinite results remain retryable without
adding an entity or undo entry. These dialogue/sign choices are Rust contracts
guided by retained HLP; only the existing three-point path has native comparisons.

## LINE and ARC continuation

Return at LINE's first point resumes the last explicitly created LINE/ARC
endpoint. Return at ARC's first point also uses its terminal tangent and asks
only for an end point. The exact entered end survives clockwise record-angle
reordering. This history survives unrelated additions and reports, is restored
by UNDO, and is cleared when its source is changed/erased or a drawing is
opened/new. LINE continuation permits any next point; ARC continuation keeps
the tangent. This session history is not inferred from imported file order.
ARC mouse/API points use SNAP without ORTHO; continued LINE keeps ORTHO's anchor.

## SOLID

SOLID accepts Return at its fourth point for a triangle, storing `p4 = p3`.
Return at the next third-point prompt ends the sequence. Continuation always
reuses the stored third/fourth pair, including their coincidence after a
triangle; supply two new distinct points to create another nondegenerate
section from that collapsed edge. This triangle continuation policy has Rust
coverage, while the original retained export covers quadrilateral chaining.
