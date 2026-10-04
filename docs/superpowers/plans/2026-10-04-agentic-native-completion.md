# Native completion: agentic implementation and review loops

Authorized 2026-10-04. Scope and prior evidence:
[native completion plan](2026-10-03-native-editor-completion.md),
[command matrix](../../native-command-matrix.md).

## Goal and execution

Close the remaining documented AutoCAD 1.4 software behavior in handwritten
Rust, retaining explicit compatibility limits where the original evidence is
insufficient. No guest/collector/translated-runtime work. Preserve unrelated
dirty files, original fixtures and retained worktrees. Hardware exclusions and
pixel-exact DOS display are outside this goal.

The coordinator owns the queue, integration, checks, ledger and user updates.
Workers use the built-in collaboration API; the bounded task/handoff protocol
is adapted from the Superset orchestration skill. This is coordinator-driven
execution, not a shell daemon or unmonitored terminal automation.

Use at most three workers plus the coordinator. Initial assignments:

| Worker | Model / effort | Responsibility |
| --- | --- | --- |
| selection_loop | GPT-6.1 Sol / high | S1 canonical selection/REPEAT/LIST, then scoped follow-ups |
| layers_loop | GPT-6.1 Sol / high | L1 layer behavior, visibility and supported persistence |
| evidence_review | GPT-6 Astra / high | Evidence contracts, dependency plan and independent review |

Reuse an existing agent for follow-ups. Give later straightforward changes a
Sol medium worker; reserve Astra for ambiguous original/file semantics and
independent review. Do not launch more workers merely because a queue is long.

## Responsibility splits

The graph has no indexed nodes for the native command modules, so the coordinator
used scoped source inspection as fallback. Before final L1 integration,
`acad-render/src/flatten.rs` contains 1,189 lines, including tests, styling,
resource state and geometry traversal. L1 separates resource walking and bounded
selected-owner policy; selection separates compact visible bounds from parsing
and picks. Remaining candidates are DWG `entity.rs` (2,005 lines, much of it
tests), command `editor_ops.rs` (620) and `entity_ops.rs` (684). B3 should assess
record decoding versus structural stream assembly; subsequent edit milestones
should extract their domain operations when touched. Line counts identify
inspection candidates, not proof that every large test module is a god class.

## Isolation and handoff

Initial implementation workers have private Git snapshots of the current dirty
working files, not a checkout of stale HEAD. Their root paths and baseline commit
are in the JSON ledger. Snapshot commits exist only in private temporary repos;
the user repository is not committed, reset or stashed. Corpus links are read-only.
Workers write only their assigned private copy, use separate build target dirs,
and export an exact staged patch relative to their private baseline. Reviewer
writes only its temporary review notes. Only the coordinator changes the user
workspace, serially checking each patch against the current baseline. Preserve
overlapping changes by explicit reconciliation; never overwrite a whole file
from a stale worker snapshot. Refresh worker baselines before another milestone.

Each worker receives ID, objective, scope, dependencies, evidence requirements,
acceptance cases, verification commands, stopping rules and a handoff envelope.
Progress reports are required after contract selection, first validation and
review readiness. A worker saying done means a candidate is ready for review.

## Loop and completion gates

1. Promote a pending milestone only after dependencies are verified.
2. Read retained HLP/RE notes and source; distinguish observed behavior from a
   bounded Rust policy. Prefer graph tools; fall back when results are insufficient.
3. Implement the smallest independently reviewable behavior with meaningful
   tests for geometry/data, retries/cancel/UNDO and relevant API/file paths.
4. Run focused native tests and scoped Clippy. Report actual commands/results.
5. Independent reviewer checks code, evidence and missing cases. The coordinator
   reconciles patches and validates the combined result.
6. Feedback opens a repair pass, invalidating old check/review approvals. At most
   three repair passes per milestone; diagnose first, then change code or scope.
   A repeated failure must produce a concrete blocker or a smaller task.
7. Mark verified only after required checks pass, review passes and implementation
   is integrated. Record limitations and update the command matrix/active plan.
8. Reuse workers for the next ready milestone. Stop goal work only when the goal
   is actually achieved, the user pauses it, or the goal's blocking rule applies.

Task `blocked` records a specific local dependency/evidence problem. It does not
mean the whole goal is blocked: advance other independent work. `deferred` keeps
an unresolved compatibility gap visible and is never counted as implemented.
Do not rerun passing checks absent a code change, failure or unresolved concern.

Original fixture comparisons remain strict. No invented file bits, coordinate
lookup tables or loosened tolerances. Layer visibility must not be silently
lost on save: supported encoding or explicit failure is required. Integration
checks cover the six native packages, scoped Clippy, workspace compilation,
format/diff checks and attached GUI/MCP workflows when relevant.

## Durable progress

S2 checkpoint: shared mouse/window collection and selected LIST are integrated.
Typed IDs replace collected picks; HATCH keeps the retained second-corner window
completion. Independent exact source review passed, followed by 468 tests across
the six native packages, Clippy, workspace compilation and an attached native
window/MCP/API workflow. This validates native behavior; it is not a new original
selection or desktop keystroke measurement.

The L2 static audit establishes signed negative layer colors in the retained
DWG/DXF code. L3 bounded persistence for layers/colors1..127 is verified, including
actual attached GUI/MCP save/reopen and zero-color failure recovery. Malformed or
unrepresentable native edge states retain checked errors. RB1 per-owner rendering
budgets passed independent review,46 renderer tests and actual GUI integration;
whole-document aggregate work remains a Q1 performance check. B3 corrects the
independent REPEAT markers and child headers, preserving marker layers as metadata.
Original REPEAT picking resolves source members, while native selection uses one
group owner. Whole-owner erased DWG persistence therefore has an explicitly native
all-negative lexical-record policy, now verified. DXF retains
its live export semantics and omits erase history. Partial erased members inside
live groups remain a separate compatibility limit tracked by B4.

B3/T1 final checkpoint: 520 six-package native tests passed with no failures or
ignored cases; scoped Clippy, workspace all-target compilation, changed-file
formatting and diff checks passed. The independent final merged-source review
accepted SHA `576694866be892912ac7abe4a945bade609022f242aad091a3cbe1d29820bc3b`.
Actual GUI/API/MCP workflows passed TEXT C/R/A, repeated lines, point height,
atomic CHANGE, erased/nested REPEAT, OOPS, BLOCK/WBLOCK and DWG/DXF reopen.
The text PNG was visually inspected. Native files matched the 238-file snapshot;
the 41 protected files remained unchanged. GUI artifacts:
`/var/folders/d1/r2bpvclx19q4vd4c_8g42ddh0000gn/T/acad-gui-lifecycle-v_l_jgxb`.
V1 FILLET/views and B4 erased group members run in separate private snapshots,
with R4 independently auditing radius encoding and reviewing both candidates.

V1/B4 checkpoint: the coordinator resumed in a Claude Code session after the
Codex workers ended; the same snapshots and review protocol continued. V1 needed
three source passes: representable view corners and finite render scale on every
ZOOM/PAN/P commit, then a display-only LIMITS/unit fallback so unusable stored
views (historical DXF without DWGVIEW) never blank the GUI. B4 needed two: INSERT
`*name` explode and named WBLOCK promote erased direct members to top-level erased
records, and LIST counts nested erased members. The final merged-source review
accepted the integration delta. 564 six-package tests passed with no failures;
Clippy, workspace all-target compilation, formatting and diff checks passed; the
attached GUI/MCP/API harness passed, including a new erased-member step (DWG
byte-identical resave, DXF omission, ambiguous whole-owner save refusal, OOPS).
The 41 protected files are unchanged. macOS purged files from the temporary run
directory during this phase, so artifacts are mirrored to
`target/agentic-artifacts/` and new snapshots live in `target/agentic-snapshots/`.
H1, B1 and B2 start from snapshot `ed5812b` of these accepted working files.

H1/B1/B2 checkpoint: each passed independent review (H1 two source passes plus
a test-only addendum, B1 two, B2 two) and was integrated serially by three-way
merge. H1 adds `name,N|O|I` styles with a half-open depth-count rule for O/I and
unchanged N output. B1 adds external drawing INSERT with regular-file-only lookup,
a byte cap, a whole-drawing record check and post-commit library merge. B2 adds
circular ARRAY fill/rotate and point-picked BREAK including TRACE; the user
accepted the in-tree original emulator as B2 evidence, and every original-behaviour
claim is asserted by committed `acad-oracle` tests (9 passed on the real System.img).
Final combined state: 634 six-package tests with no failures, Clippy, workspace
compilation, formatting and diff checks passed, and the attached GUI harness gained
external INSERT, HATCH N/O/I and ARRAY/BREAK steps and passed. Protected files are
unchanged. H2, X1 and K1 start from the next snapshot.

Final checkpoint: all 23 milestones of this loop are verified. X1/H2/F1/K1 and Q1
were reviewed and integrated serially; the final merged-source review found one
coordinator merge defect (macro resume after a pattern-file answer), fixed with a
regression test. Q1 adds a combined Session/MCP workflow test, a HATCH edge index
(dense 2,000-square case 20.4 s to 0.04 s, byte-identical output), CI that compiles
and passes on a clean checkout without the corpus (corpus tests skip visibly and
fail under `AUTOCAD_REQUIRE_CORPUS=1`), and a matrix/README audit. Final run:
1,061 workspace tests with the corpus required and none skipped, Clippy and
formatting clean, release build, GUI harness and the 25-input corpus API check.

Follow-up queue (user decision 2026-10-04): plotting stays out of scope. M1 adds
the native Main Menu tasks (New, Edit existing, Configure, Make/Load DXF, File
Utilities, Exit) with oracle-backed prompts. W1 replaces WBLOCK's silent overwrite
with the original's confirmation and protects the open drawing. RB2 adds the
whole-document rendering budget left open by RB1. All three start after Q1 is
integrated and the accepted work is committed.

[Independent evidence/contracts and suggested queue](2026-10-04-agentic-contracts.md)
is accepted as an audit deliverable, not as approval of implementation patches.

[Machine-readable ledger](2026-10-04-agentic-progress.json) records stable IDs,
dependencies, agent/model/workspace, status, acceptance, check evidence, artifacts,
review verdict, integration and append-only events. Only the coordinator writes it.

```sh
python3 tools/agentic_progress.py status
python3 tools/agentic_progress.py validate
python3 tools/agentic_progress.py update S1 --status review \
  --check focused_tests=passed --note 'Candidate checked; independent review pending'
```

The updater checks dependencies, transitions and verification gates before an
atomic file replacement. Read-only reports and log/artifact paths survive
context compaction. Reacquire agent status using collaboration.list_agents and
read the ledger rather than infer completion from an idle agent.
