# Native command implementation audit

Audited 2026-10-03, updated 2026-10-04 against the handwritten Rust editor, shared Session/API,
retained ACAD.HLP and existing regression tests. All 57 recovered dispatcher
names appear below: 54 recognized software commands and three hardware
exclusions. No completion percentage or universal native parity is asserted.

**Evidence.** R = Rust regression coverage for the stated slice, including
common creation/editor routes. E = offline comparison against retained original
exports. H = help-file data only, not implemented command behavior. Earlier
original observations in the design spec are useful evidence but are not new
guest runs. All rows have H coverage through `tests/help.rs`, including the
excluded hardware topics. The coverage column below concerns execution.

**Shared limits.** Applicable edit/block/HATCH/ENTITYAREA/LIST commands share
ID/ALL/LAST selection, accumulated mouse picks and W/WINDOW containment. Typed
sets replace collected picks and execute immediately; general windows/picks
finish on Return, while evidenced HATCH windows finish at the second corner.
Canonical IDs address each
live top-level entity or REPEAT owner, excluding LOAD, erased records and block
definitions. Explicit IDs/ALL/LAST retain hidden records; mouse/windows/highlights
filter visible owner/member geometry. INSERT picks remain origin-based with a
visible-content gate; windows use conservative bounds. Reports are available to
the window's wrapped/paged viewer, API/MCP and the launching terminal. Viewer
navigation is independent of editor prompts and drawing/undo state; original
report screen/layout parity is not established.
Mouse SNAP/ORTHO is a native policy; typed coordinates bypass those constraints.
Stored header settings alone do not establish their visible GUI behavior.

| Name | Current executable behavior | Remaining scope / compatibility limits | Coverage |
| --- | --- | --- | --- |
| LINE | Successive segments, relative points, C closure, session LINE/ARC endpoint continuation, undo | Imported-file continuation history is not inferred | R, `line_close.rs`, `arc_options.rs` |
| POINT | Current-layer point placement | Original screen point style not matched | R, codecs |
| CIRCLE | Center plus numeric/point radius, numeric D, 2P diameter endpoints and 3P circumference points | New forms follow retained HLP with Rust numeric/input contracts; no retained original exports for these forms | R, `creation_options.rs`, shared API/MCP |
| SHAPE | Named loaded SHP shape, origin/height/angle | Broader original library acceptance not established | R, renderer |
| REPEAT | Opens following-entity group; canonical whole-group edits; nested live/uniform-erased DWG persistence, ordinary erased-member retention, live DXF pruning | Whole-owner save with prior member erasure, negative structural subgroups, explicit owner-layer encoding and original leaf-selection parity; some group transforms | R, `selection_groups.rs`, group codecs/API/MCP |
| ENDREP | Columns/rows and numeric or two-point spacing; nested persistence; bounded owner expansion in rendering | Aggregate whole-drawing rendering budget; original group-edit parity | R, codecs, `expansion_budget.rs` |
| TEXT | A/C/R ink layout, height value/two points, angle value/point, literal text and repeated lines | Original layout/leading parity; alignment/history absent after reopen | R, `text_change.rs`, runtime SHP/API/MCP |
| ARC | Three points; center/end/angle/chord/radius/direction forms; tangent continuation; undo | Extra forms, sign and session-history choices lack native export parity | R, E: existing 3P; `arc_options.rs`, app API/GUI smoke |
| TRACE | Width, vertex chain, mitered quadrilaterals | Whole-chain undo policy and broader bends not established | R |
| LOAD | Host SHP resolution, library registration and LOAD record | Original file search/error semantics not fully established | R, app |
| SOLID | Chained four-point sections; blank fourth point commits a triangle with p4=p3 | Triangle uses the existing third/fourth continuation pair, now coincident; that continuation/storage policy lacks native parity evidence | R, `creation_options.rs`, fill raster/API/MCP |
| LIST | Selected canonical owners via IDs/picks/windows; bounded detailed properties in shared viewer/API/MCP | Original field layout and general selection dialogue parity | R, app reports, `selection_dialogue.rs` |
| INSERT | Existing block, independent scales/angle, box scales, star explode (erased members become root erased records, never revived); external AC1.2/AC1.40/DXF drawing files as a block at the stored BASE or star-exploded, with reachable nested definitions, new-layer colour/OFF import, post-commit SHP libraries for reachable LOAD names, regular-file/byte-capped reads, whole-drawing record-budget preflight, explicit refusal of name conflicts/cycles/undefined references, staged commit as one UNDO (Session/API/MCP/menu macro) | External-file policies are native choices without original-output parity (`docs/native-external-insert.md`); star form ignores scale/rotation; erased whole REPEAT owners omitted in block form; other source header state not merged | R, `erased_member_promotion.rs`, `external_insert.rs` (acad-cmd, acad-app) |
| BASE | Updates stored insertion base | General header-setting undo policy not uniform | R, `status.rs`, codecs |
| ORTHO | Toggle/header, shared mouse constraint | Typed-point constraint policy differs from original description | R, mouse/API |
| LAYER | Numeric current layer, ON/OFF lists, COLOR/? prompted/inline; shared visibility/undo/API | Signed OFF persistence for defined layers/colors 1..127; native-edge OFF refusal and original policy parity remain open | R, `layers.rs`, rendering/API/GUI |
| GRID | Toggle, numeric/zero/SNAP-relative spacing, world-origin dots in GUI/PNG/RGBA | Limits clipping, density and color are Rust display policies; no native screen comparison | R, `grid.rs`, API/codecs |
| LIMITS | Validated lower-left/upper-right bounds | General header-setting undo policy not uniform | R, codecs |
| ID | Point coordinate report in selected units | Original field layout not retained | R |
| RES | Alias of SNAP | Same limits as SNAP | R |
| RESOLUTION | Alias of SNAP | Same limits as SNAP | R |
| ZOOM | A/E/C/L/P/W, All-relative numeric and current-relative X; visible bounds, viewport-aspect E/W/L | New center/aspect policy, representability guard and display-only fallback for unusable stored views are native; text/shape ink bounds and new original-output parity remain unmeasured | R, retained All fixtures, `fillet_views.rs`, `native-view-policy.md` |
| PAN | Relative displacement plus Return or two-point from/to, unchanged height, Previous swap | Displacement sign and point/layout choices are explicit native policy | R, `fillet_views.rs`, shared API |
| MOVE | Displacement or from/to then selection | Common selection limits | R |
| ERASE | Ordinary/member erasure retained in DWG, stable native owner selection and undo | Whole-owner save with prior member erasure checked-refuses; original leaf-selection parity | R, erased-record codecs, `erased_member_lifecycle.rs` |
| MENU | MNU pages (`*`, `*[label]`), blank slots, GO, macros with per-space/`;` Returns and `\` one-input pauses (cancel abandons), SNAP/ORTHO/cancel controls; regular-file 64 KiB-capped load | No INS keyboard menu cursor; `+`/`$S=`/other control bytes unimplemented (absent from retained menus); pick-at-selection-pause and script-item resume are unmeasured native policy; original display parity | R, `menu_macros.rs` oracle, retained ACAD/OFFICE/SUBDIV/SHUTTLE.MNU, [files/menu](native-files-menu.md) |
| REDRAW | Shared native frame composition, no data mutation | Original device behavior deliberately modernized | R, app |
| STATUS | Extents, limits, base, view, layer, modes, units and sizes report in GUI/API | Rust report layout; no retained original output | R, `status.rs`, API |
| REGEN | Shared native regeneration, no data mutation | Original device behavior deliberately modernized | R, app |
| DBLIST | Live entity/block/repeat database report, shared wrapped/paged viewer | Original field layout/paging not matched | R, app reports |
| DIST | Two-point distance in selected units | Broader original field layout not retained | R |
| CHANGE | LINE endpoint, CIRCLE radius, staged INSERT point/angle, single TEXT origin/angle/value, entity layer | Multiple/mixed TEXT property edits; original prompt/layout parity | R, `text_change.rs` |
| END | Saves attached document before exit; unnamed drawing asks for output path; replacing an existing file keeps its previous bytes as `.BAK` (none for new files or `.bak` destinations) with staged, fsynced (files and directory), destination-safe order; locked (macOS immutable) destinations refused up front | Source codec/revision preserved by Rust policy; host backup naming/symlink/permission policy is native; no Main Menu, task-1 replace question or disk-full parity | R, `files_backup.rs` oracle, `backups.rs`, `corpus_backups.rs`, app `lifecycle.rs`, real MCP/GUI |
| QUIT | Y/YES discard confirmation, shared with window close and default API quit; writes no drawing or backup (oracle) | Explicit API `discard:true` bypass for automation; no native screen/layout parity | R, `exit.rs`, app `lifecycle.rs`, `files_backup.rs` oracle, real MCP/GUI |
| ? | Command query/list and retained topic pages in shared viewer | Original screen/paging not matched | R, `help.rs`, app reports |
| AREA | Point-polygon area; ENTITYAREA extension handles selected geometry | Original AREA entity-selection syntax is not inferred | R |
| OOPS | Restores last erased records in place, undoable | Unmeasured original lifecycle paths remain open | R |
| TABLET | Excluded digitizer configuration | Hardware outside native 2D scope | H only |
| PLOT | Excluded plotter driver | Hardware outside native 2D scope | H only |
| DELAY | Signed 16-bit integer count; nonblocking Session deadline in native milliseconds inside a command script (native SCRIPT command, `--script`, API/MCP `script`); negative = no pause; inert outside scripts ([contract](native-scripts.md)) | Original is a CPU-bound loop (~730 8086 instructions/unit) with 16-bit wrap; native unit and out-of-range refusal are policy | R + in-tree original: `scripts.rs` (cmd/oracle), app `session/script_tests.rs` |
| RESUME | Continues an interrupted script (error, input, cancel) at its exact next unread item; remaining DELAY discarded; inert inside or without a script | Mouse/API/cancel interruption and no Main Menu are native; keys the GUI ignores (Space/Tab/arrows) do not interrupt; screen echo not compared | R + in-tree original: `scripts.rs` (cmd/oracle), app `session/script_tests.rs` |
| COPY | Displacement or from/to then selection | Common selection limits | R |
| BLOCK | Named selection/base, flat block table | Common selection limits | R |
| DIM | Orthogonal primitives, A/T settings, B/C history, TXT metrics, undo | Large-arrow external text rule, other fonts and unmeasured cases | R + E: 29 drawings |
| QPLOT | Excluded quick plotter output | Hardware outside native 2D scope | H only |
| SNAP | Toggle/positive spacing, mouse grid snapping | Broader original acceptance and typed-point constraint policy | R, mouse/API |
| FILL | Header toggle and filled/outlined SOLID/TRACE rendering | Broader original display parity | R, renderer |
| HELP | All dispatcher topics, aliases, retained text, unknown-topic recovery, shared viewer | Original screen/paging not matched; pages describe options still absent in execution | R, exact retained LINE page, app reports |
| UNITS | Four formats/precision, persisted AC1.40 fields and measurement display | Broader original formatting thresholds not all retained | R, codecs |
| ARRAY | Rectangular/circular copies, numeric or point spacing, bounded output; circular angle-to-cover (`-degrees`, endpoint inclusive, exact 0/360 full circle, half-up rounding), angle validation, single-INSERT rotate-copies choice with normalized rotation | Original aborts (native retries) on zero/oversized angle and fractional counts; other rotate answers retry (original: No); original's large-array confirmation prompt not reproduced | R + in-tree original, `docs/native-array-break.md` |
| WBLOCK | Whole/live, named (erased members as root negative records) or selected group export, transitive block closure, bounded ordered nested LOAD context, base | Explicit owner-layer encoding; full save of erased source with prior member erasure; broader path semantics; replaces an existing destination (staged, no backup) without the original `Y` question | R, app/files, `group_exports.rs`, `selection_groups.rs`, `files_backup.rs` oracle |
| AXIS | Toggle, numeric/SNAP-relative spacing and visible rulers | Original pixel style modernized; historical DXF cannot carry AXIS | R, header/raster |
| HATCH | All 23 built-in names, phase/drift/dashes/dots, closed loops/circles/holes, undo; `name,N/O/I` island styles (`docs/native-hatch-styles.md`); `U[,style]` angle/spacing (number or two points)/double prompts; external ACAD.PAT-syntax files through the Session (`NAME.PAT`, then `ACAD.PAT`; regular files, 262,144-byte cap, line-numbered errors, row/dash/line limits, 10,000,000 row+dash-cycle work bound; rows visit only boundary edges whose offset range they cross) (`docs/native-hatch-user.md`); sweep start row and continuous-row direction follow the original | Native retries (Return/zero/negative spacing, other double answers) where the original exits or accepts; built-in names never read a file; dashed-row direction and patterns with non-zero delta-x are not oracle-checked; non-default styles lack original comparison | R + E: LINE/default NET; R + in-tree original (`docs/native-hatch-user.md`): `U` geometry/prompts, sweep start/orientation, PLAST/PLASTI/TRANS/INSUL order, ACAD.PAT run-time catalogue and ANSI31/ANSI37/BRICK via the file route |
| FILLET | R remembered initial-zero radius; zero intersection/line extension; positive tangent fillet; AC1.40 radius persistence | AC1.2/DXF nonzero radius checked refusal; new nonzero original export and generalized ray choices unmeasured | R, independent retained descriptor audit, `fillet_radius.rs`, `fillet_views.rs` |
| BREAK | Point-to-object pick (typed/mouse/API) as first point, F first-point re-entry, projected points, end cut-off, whole-span erase, LINE/ARC/CIRCLE/TRACE with original record policy (erased source + appended remainder), undo, DWG/DXF | Mitered-end TRACE cuts and non-TRACE corner windings refused; missed picks retry (original aborts); original LINE below-X-range no-op, coincident-point ARC/CIRCLE splits and screen-dependent aperture not reproduced | R + in-tree original, `docs/native-array-break.md` |
| SKETCH | Mouse freehand on GUI motion and API/MCP `motion`. Click or P toggles the pen. Vertices are taken at the world-unit increment by one-axis (Chebyshev) distance; pen-up/R/X record the tail; collinear runs merge. R/X/Q/Return/E (cut back through the nearest vertex)/C (connect within the increment)/`.` follow the original's messages. SNAP/ORTHO apply (the ORTHO pen-up/R/X tail is the observed L, and R continues from the pointer). Temporary strokes are frame-only (cyan) until R/X; Q/Esc/Ctrl+C discard them, other GUI chords are ignored; at most 10 000 temporary segments; one UNDO per record batch; DWG/DXF round trip (`docs/native-sketch.md`) | Original CGA-pixel quantization and polling cadence not reproduced. Unobserved cases (other sub-mode keys, connect metric, non-axis and cross-stroke merging, pen-up ORTHO, segment bound, UNDO granularity) are native policy; no remembered increment default | R + QEMU original mouse oracle (`sketch_mouse.rs`, 9 tests, O1–O14) |
| FILES | Host file utility list/wildcard/delete/rename with drive mapping, shared viewer; rename never replaces any existing entry (incl. dangling symlink) | Broader DOS path/layout semantics; original File Utility screen | R, app/files/reports, [files/menu](native-files-menu.md) |

## Source ownership and regression anchors

- `crates/acad-cmd/src/dispatch.rs` and `dispatch/{creation,blocks,settings,editing,measurement,files,dimensions,hatch}.rs`
  own routing and prompts; `input_state.rs` defines pending command state.
- `selection.rs`, `editor_ops.rs`, `entity_ops.rs` and `geometry/` own selectable
  entities, mutations and algorithms. These files were inspected for the audit.
- `dispatch/creation/circle.rs` owns the retained CIRCLE prompt variants;
  `geometry/circle.rs` owns finite bounds, stable diameter midpoint/radius and
  translated/scaled circumcircle construction. CIRCLE mouse points snap without
  ORTHO projection; typed absolute/relative/polar points remain exact.
- `tests/editor.rs`, `tests/mouse_constraints.rs`, `tests/line_close.rs`,
  `tests/help.rs`, `tests/status.rs`, `tests/exit.rs`, `tests/grid.rs` and `entity_ops.rs` unit tests cover the
  listed Rust slices. `tests/dimension_native.rs` and `tests/hatch_native.rs`
  replay the retained original fixtures; `acad-oracle/tests/hatch_user.rs`
  compares HATCH U, sweep order and ACAD.PAT patterns with the in-tree
  original; other HATCH tests are data contracts.
- `crates/acad-app/src/session/`, `presentation.rs` and `tests/api.rs` own the
  shared user/API workflow. `tests/combined_workflow.rs` drives one drawing
  through creation, window MOVE/COPY/ERASE/OOPS, BLOCK/INSERT (including an
  external file), DIM, HATCH styles/`U`, circular ARRAY, BREAK, FILLET,
  motion SKETCH, LAYER OFF, UNDO chains and a SCRIPT DELAY on an injected
  clock over the in-process MCP route, then saves AC1.40/AC1.2 DWG and DXF
  with `.BAK` backups and compares reopened drawings. Report effects expose complete text to API state;
  the window renders complete reports above its persistent prompt/status strip.
- `report_view/{mod,layout,paint}.rs` owns visibility, source-anchored navigation,
  cached wrapping and painting. `tests/reports.rs` covers all viewer transports,
  tiny/changed frames, footer clicks, pending prompts and drawing/undo neutrality.
  The bitmap font covers printable ASCII; other glyphs use `?` in the viewer
  while API report text remains intact.
- `session/lifecycle.rs` owns document path/baseline and END save; `document/`
  stages replacement files and keeps `.BAK` backups ([files/menu](native-files-menu.md)). `tests/lifecycle.rs` covers encoding/staging failures,
  source revisions, permissions/symlinks and WBLOCK independence. `grid.rs` owns
  bounded display painting; `tools/check_acad_gui_api.py` exercises the attached
  window with scratch copies, frames, save/reopen and exit behavior.
- `crates/acad-dwg/tests/ac140_corpus.rs` already opens/rasterizes all 21 valid
  corpus drawings (16 AC1.2 and five AC1.40, including DISC.BAK) plus backups.
  Corpus-present tests also cover nested blocks. Tests return early if corpus
  is absent; a successful corpus-free run is not a corpus validation claim.
- `tools/check_acad_corpus_api.py` additionally requires manifest hashes and all
  25 Session/MCP inputs: the 21 drawings, three other Samples backups and
  SUBDIV.DXF. It checks PNG/RGBA diagnostics, scratch Save As/END revisions,
  exact DWG canvas and canonical DXF text after fresh-process reopen. Missing
  corpus fails. Historical DXF pixel differences are recorded; no original
  screen parity is inferred. This supplementary workflow passed all 25 inputs.

## Next implementation order

Current execution is tracked in the [agentic ledger](superpowers/plans/2026-10-04-agentic-progress.json).
Every ledger task except Q1 is verified: selection/LIST (S1/S2), layers and
signed OFF persistence (L1/L2/L3), TEXT/CHANGE (T1), FILLET/ZOOM/PAN (V1),
HATCH styles and U/pattern files (H1/H2), external INSERT (B1), circular
ARRAY and BREAK (B2), REPEAT persistence and erased members (B3/B4), owner
rendering bounds (RB1), scripts (X1), mouse SKETCH (K1), files/backups/menu
macros (F1) and the DIM evidence audit (D1). The latest integration passed
738 six-package native tests plus the in-tree/QEMU oracle suites, Clippy,
workspace checks and the attached GUI/MCP/API harness; the Q1 candidate
passes 742 with the corpus required. These are scoped Rust
contracts; remaining compatibility limits are stated in the table and ledger.

1. Q1, combined workflows, performance and release checks, is the final
   milestone and awaits review: the combined Session/API/MCP workflow test,
   the corpus API check (all 25 inputs), the HATCH boundary-edge index
   (dense boundaries no longer cost rows times edges) and CI that compiles
   and tests a clean checkout without the corpus (corpus menu files are read
   at run time; their tests skip visibly), shows oracle skip messages and
   builds release binaries. The whole-document aggregate
   rendering budget noted at RB1 remains a follow-up.
2. Keep each row's native-parity limits explicit; the open gaps include
   whole-owner save after prior member erasure, explicit owner-layer
   encoding, nonzero FILLET radius outside AC1.40, view ink bounds and
   original screen/report layouts.
3. DIM's large-arrow external text rule stays open: the D1 audit found no
   retained-evidence formula, so existing native comparisons stay strict and
   the gap stays explicit.
