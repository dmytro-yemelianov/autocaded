# Native Rust editor completion

Date: 2026-10-03. Active plan, replacing collector-first recovery plans.
Spec: [AutoCAD 1.4 Rust design](../specs/2026-09-28-autocad-14-rust-design.md).

Execution update 2026-10-04: remaining milestones now run through
[bounded multi-agent loops](2026-10-04-agentic-native-completion.md), with
[live progress/evidence](2026-10-04-agentic-progress.json) and
[independent contracts](2026-10-04-agentic-contracts.md). This plan retains the
foundation and original-parity limits; the ledger owns current task status.

## Execution policy

Implement recovered algorithms and data in the handwritten Rust application.
Reuse existing exports offline. No DOS startup debugging, translated-runtime
implementation or new QEMU/oracle collection. Preserve unrelated `acad-re` work
and retained evidence/worktrees. Report native parity separately from Rust
behavior contracts. Keep compatibility gaps visible rather than changing
tolerances or adding observed-coordinate lookup tables.

## Confirmed foundation

- [x] Native model, AC1.2/AC1.40 DWG and historical DXF codecs, renderer and editor.
- [x] 54 recognized software command names; three plotter/tablet commands excluded.
  Recognition does not establish complete command behavior.
- [x] DIM primitives, A/T settings, B/C history, recovered TXT metrics;
  29 retained drawings compared offline. Large-arrow placement remains open.
- [x] HATCH families, signed dashes/gaps, row drift, closed LINE/ARC/circle
  boundaries and holes. LINE/default NET retain native export comparisons.
- [x] Shared mouse SNAP/ORTHO policy and view-preserving creation/UNDO.
- [x] Session API, local GUI socket, headless/attached stdio MCP, PNG/RGBA frames.
- [x] Split window adapter, session/effects/resources, command domains and geometry.
  Further extraction should follow responsibilities and concrete changes.

## 1. Close dot HATCH patterns

- [x] Transcribe MUDST/SACNCR from the retained PAT, without a runtime dependency
  on the ignored corpus file or adding names to the recovered catalogue.
- [x] Represent zero entries as real POINT entities; preserve cycle length,
  signed indices and global phase across clipped intervals and holes.
- [x] Count points and lines toward the shared 100,000-entity budget; generation
  errors must add no block, INSERT or undo step.
- [x] Check mixed primitive order, independent diagonal lattice, negative drift,
  scale/rotation, annular holes, DWG round trips and UNDO.
- [x] Verify shared API rendering, PNG, DWG/DXF save/reopen; run native regressions
  and update checkpoint/provenance. Dot representation/boundary policy is a Rust
  contract; no original dot export is available to establish native parity.

Validated: 348 native tests passed, none failed/ignored; scoped Clippy, workspace
all-target compilation, formatting and diff checks passed. A real headless MCP
process created both patterns, saved DWG and captured PNG; both frames were
visually inspected. No oracle or translated boot process was run.

## 2. Resolve remaining recovered geometry and command gaps

- [x] Audit retained DIM large-arrow notes/exports and font bounds, with
  independent raw-DWG/arithmetic review. No general external-text origin formula
  is supported; retain that implementation gap and advance other native work.
- [x] Audit the 54 command names against source and retained specifications;
  record implemented behavior, missing options and evidence/test coverage in
  one matrix. Do not equate a dispatch arm with completion.
- [x] Use retained ACAD.HLP for all HELP topics/aliases, implement LINE C closure
  and drawing STATUS reports, checking drawing/undo neutrality and shared API.
- [x] Set priorities from [the command audit](../../native-command-matrix.md).
  First: document path/dirty state and END save versus QUIT discard; GUI report
  display; GRID painting; CIRCLE options/triangular SOLID/continuation. Then
  selection/LIST/layer visibility, TEXT/CHANGE, FILLET and view forms.
- [ ] Complete HATCH styles/user patterns and DELAY/RESUME script semantics. SKETCH mouse support
  needs a deliberate native input policy; tablet/plotter drivers stay excluded.
  Each item needs a bounded behavior contract before implementation.

DIM audit D1 (2026-10-04) is accepted as an evidence deliverable only; it is not
DIM completion. [Audit](../reviews/2026-10-04-dim-evidence.md) and
[independent contract review](../reviews/2026-10-04-dim-contract-review.md)
confirm large-arrow/mixed-history external text discrepancies. A prospective
inside-layout projected-extent fit policy needs separate scope and tests; it
cannot make the full PBCB drawing match while its earlier text origin differs.
The existing 29 native drawings/316 primitives retain their strict comparisons.

Audit slice validated: 359 native tests passed; scoped Clippy, workspace
all-target compilation, formatting and diff checks passed. Real stdio MCP
exercised LINE C, HATCH, STATUS, DWG save, all 57 help topics and clean quit.
Unknown HELP recovery, read-only report/undo behavior and close-without-duplicate
cases have regression coverage. The command matrix was checked against the
exact dispatcher-name sequence in the retained spec.

### Document lifecycle and GRID

- [x] Track document path, detected codec/revision and semantic saved baseline;
  expose dirty state through API and the window title, including UNDO recovery.
- [x] END saves before exit, asking for a path only for unnamed drawings; preserve
  the source codec/revision. Encoding/staging failures leave the old file and
  attachment intact. Explicit SAVE chooses format by suffix; WBLOCK stays separate.
- [x] QUIT and window close require Y/YES; default API/MCP quit shares that prompt.
  Explicit `discard:true` supports automated cleanup. Menu trailing separators
  leave confirmation/filename prompts pending.
- [x] Paint GRID in the shared frame, clipped to LIMITS with bounded density;
  support zero and SNAP-relative spacing without changing point-placement rules.
  Display style/density are Rust policy, not original screen parity.
- [x] Cover disk failures, source revisions/misleading suffixes, symlinks/modes,
  shared PNG/RGBA, GRID DWG/DXF persistence and a real stdio END/QUIT workflow.
- [x] Validate the full native suite, static checks and repeatable attached GUI
  smoke, then record the results below.

Validated: 377 native tests passed, none failed/ignored; scoped Clippy with
warnings denied, workspace all-target compilation, formatting and diff checks
passed. `tools/check_acad_gui_api.py` passed against a real macOS window through
attached MCP and direct API/CLI, including GRID PNG/RGBA at 800×600, 320×240 and
160×100, DWG/DXF reopen, UNDO, declined QUIT, END save/exit and socket removal.
The 800×600 frame was visually inspected. Retained local artifacts:
`/var/folders/d1/r2bpvclx19q4vd4c_8g42ddh0000gn/T/acad-gui-lifecycle-zkjydxbi`.
No original guest or translated runtime was launched.

### Full report viewer

- [x] Display complete HELP/STATUS/LIST/DBLIST/FILES/HATCH reports in the shared
  GUI/PNG/RGBA composer above the persistent command strip; retain API text.
- [x] Add cached wrapping, text-anchor resize behavior, bounded line/page/home/end
  navigation, keyboard/wheel and footer buttons. Skip hidden drawing rasterization
  while the viewer fills the canvas; avoid adding report work to command geometry.
- [x] Close/reopen the report without submitting editor input or altering drawing,
  dirty baseline, UNDO or pending FILES prompts; typing resumes command input.
- [x] Add typed API `report`, MCP `acad_report` and direct CLI navigation. State
  exposes viewer visibility and an opaque text anchor. API command keeps its
  existing editor-submission semantics.
- [x] Complete the bitmap font's printable ASCII punctuation; unsupported glyphs
  use `?` in the viewer while original report text stays intact for API clients.
- [x] Validate native suite, static checks, shared pixel/interaction regressions
  and attached GUI smoke; inspect first/last/narrow HELP frames and record results.

Validated: 388 native tests passed, none failed/ignored; the affected app suite
passed again after reducing report bars so 160×100 frames retain a text row.
Scoped Clippy with warnings denied, workspace all-target compilation, formatting
and diff checks passed. Attached GUI smoke exercised HELP first/end/home, resize,
footer navigation, STATUS/LIST/DBLIST/FILES, CLI `report` and unchanged geometry/
dirty state, followed by the existing GRID/files/UNDO/QUIT/END checks. First,
last and narrow HELP frames were visually inspected. Final local artifacts:
`/var/folders/d1/r2bpvclx19q4vd4c_8g42ddh0000gn/T/acad-gui-lifecycle-bv43dqcu`.
No original guest or translated runtime was launched.

### CIRCLE options and triangular SOLID

- [x] Implement retained HLP CIRCLE radius points, numeric D, 2P diameter endpoints
  and 3P circumference points, including absolute/relative/polar typed coordinates.
- [x] Extract the circle dialogue and pure finite circle construction into separate
  modules. Use translated/scaled circumcircle math and overflow-resistant diameter
  construction; reject unrepresentable bounds and degenerate inputs before add.
- [x] Share mouse/click/API routes, SNAP without ORTHO for curves, pending-state
  retry/cancel behavior and one undo entry per completed entity. Keep existing
  Return-at-radius invalid behavior; no remembered-radius policy is inferred.
- [x] Return at SOLID's fourth point stores p4=p3, then keeps the existing stored
  third/fourth continuation pair. Triangle continuation/storage is Rust policy,
  not established by the retained native quadrilateral export. Relative coordinate
  overflow fails before advancing SOLID prompts or adding geometry.
- [x] Check independent circle construction across point orders/scales/translations,
  malformed/degenerate retry, layer/view/limits preservation, UNDO, both DWG revisions,
  shared pixel/point input, DXF workflow and triangle FILL ON/OFF rendering.
- [x] Validate the native suite/static checks and extended real GUI/MCP workflow;
  inspect the saved frame and update the checkpoint with the actual results.

Validated: 400 native tests passed, none failed/ignored; the creation tests also
passed after adding an independent oblique-circle/order case. Scoped Clippy with
warnings denied, workspace all-target compilation, formatting and diff checks
passed. Extended attached GUI/MCP/API smoke created numeric-D/2P/3P circles and
a filled triangle, saved/reopened DWG/DXF, exercised report/grid/UNDO routes,
declined QUIT, saved/exited via END, and reopened seven entities with clean dirty
state in a fresh process. The 800×600 drawing frame was visually inspected.
Final local artifacts:
`/var/folders/d1/r2bpvclx19q4vd4c_8g42ddh0000gn/T/acad-gui-lifecycle-n9uuj2do`.
No guest or translated runtime was launched; new input forms retain Rust-only
compatibility contracts.

### LINE/ARC alternatives and continuation

- [x] Implement retained HLP center-first, start/center/end direction,
  start/center/angle or chord, start/end/radius, angle or direction. Keep
  dialogue and geometry in separate ARC modules; reuse stable circumcircle
  math for 3P rather than leaving ARC creation in editing geometry.
- [x] Store the last explicitly created LINE/ARC's exact endpoint and terminal
  tangent. Initial Return resumes LINE at that endpoint or asks for ARC's
  tangent-constrained endpoint. Clockwise records still encode CCW angles.
  Unrelated creations/reports preserve history, UNDO restores it, and source
  mutation/erasure or new/open invalidates it. Imported history is not inferred.
- [x] Define bounded Rust sign/input rules: signed included angles, negative
  radius/chord for major CCW arcs; direction accepts angle/point. Reject zero,
  full-circle, collinear, parallel-direction, impossible and nonfinite results
  before add/undo. Relative overflow is retryable before prompt advancement.
- [x] Exercise SNAP without ARC ORTHO, continued LINE's ORTHO anchor, exact
  endpoints, closing edges, cancellation, retry, history invalidation/restoration,
  both DWG revisions, DXF, shared API clicks/points and PNG/RGBA.
- [x] Extend attached GUI/MCP lifecycle smoke with tangent ARC/LINE continuation
  and center/angle ARC; inspect its shared frame and reopen final DXF cleanly.

Validated: the six-package suite passed 411 tests with no failures/ignored tests;
the final focused ARC suite passed all ten tests after one additional closing-edge
regression (412 distinct native tests now covered). Scoped Clippy with warnings
denied, including the final test additions, workspace all-target compilation,
formatting and diff checks passed. Attached GUI/MCP/API/CLI smoke saved/reopened
nine entities in DWG/DXF, exercised report/grid/UNDO and END/QUIT cleanup, then
opened the final ten-entity DXF with clean dirty state in a fresh process.
The 800×600 frame was visually inspected. Final local artifacts:
`/var/folders/d1/r2bpvclx19q4vd4c_8g42ddh0000gn/T/acad-gui-lifecycle-0yzalaxd`.
New ARC forms/signs and continuation remain Rust contracts; existing native
comparisons passed without tolerance changes. No guest/translated runtime ran.

### Canonical selection and layer visibility — S1/L1

- [x] Shared live top-level owner IDs for immediate detailed LIST, edits,
  REPEAT, mouse picks, windows, highlights and API selectable_objects. Explicit
  hidden IDs remain editable; mouse/windows/highlights filter visible geometry.
- [x] LAYER ON/OFF/COLOR/? with atomic validation, UNDO/dirty/API state and
  owner/member rendering. Preserve ordered LOAD effects in selected highlights.
- [x] Reject unsupported OFF-state and lexical nested REPEAT before file staging;
  protect attachment, destinations and UNDO through SAVE/END/export failures.
- [x] Extract renderer resource walker, selected work budget and compact visible
  bounds. Native policy/limits are in the selection/layer contracts and ledger.

Accepted final exact source review, 455 native tests (none failed/ignored), scoped
Clippy, workspace all-target compilation, 44 changed Rust files' format/diff,
and actual attached GUI/MCP/API/CLI smoke. Visible/OFF800×600 frames inspected;
final fresh DXF contains12 raw records/11 selectable owners, clean saved baseline.
Artifacts: `/var/folders/d1/r2bpvclx19q4vd4c_8g42ddh0000gn/T/acad-gui-lifecycle-k7vlixyh`.
Safe refusal does not complete OFF/erased/nested persistence. Selected WBLOCK's
arbitrary preceding unselected-owner LOAD context and full-render responsiveness
also remain explicit. Existing original comparisons/tolerances unchanged.

Next: selected LIST/general selection dialogue and REPEAT persistence, then
TEXT/CHANGE, FILLET and missing view inputs. Keep native parity separate from Rust contracts.

## 3. Exercise complete drawing workflows

- [x] Existing codec/raster tests open/render all 21 valid corpus drawings plus
  backups when corpus is present. Shared Session/MCP corpus workflows remain
  part of the next item; a corpus-free passing suite is not corpus evidence.
- [ ] Automate create/select/edit/block/dimension/hatch/undo/save/reopen through
  Session/API/MCP, checking complete drawing data and frames at useful points.
- [x] Retain `tools/check_acad_gui_api.py` for attached-GUI/socket/MCP smoke,
  including scratch copies, cleanup, multiple dimensions and END/QUIT lifecycle.

## 4. Prepare the native application for use

- [ ] Finish file/new/open/save UX, preserving supported drawing data and clear
  errors for unsupported behavior. Complete command report viewing is delivered
  above; original file/dialogue and backup semantics remain separate work.
- [ ] Check responsiveness on corpus drawings and bounded dense hatches.
- [ ] Add native regression checks and packaging/launch documentation to CI.
- [ ] Release against the spec's 2D criteria with a stated compatibility matrix.
  Later formats, 3D and extended CAD features require separate scope.

## Validation

Run the six native packages (`acad-model`, `acad-dwg`, `acad-dxf`, `acad-render`,
`acad-cmd`, `acad-app`), scoped Clippy with warnings denied, workspace all-target
compilation, formatting and diff checks. Use the existing original fixtures
offline; no guest launch is required. Do not repeat checks after they pass
unless another edit or unresolved failure warrants it.
