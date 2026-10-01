# Menu Controls Recovery Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Recover native clicks on `^Snap` (byte `02`), `^Ortho` (`0f`), `^Cancel` (`03`), and `< GO >`, then implement only behavior supported by recorded native observations.

**Architecture:** First produce a reproducible QEMU evidence bundle in a private disposable session. A reviewed implementation brief freezes the observed transitions, headers, geometry, and pending-input behavior before any production changes; then bounded tasks add behavioral contracts, regression tests, and minimal Rust changes. GO has its own gate because the earlier native error does not identify its mechanism.

**Tech Stack:** Rust 2021 / Rust 1.88, existing `acad-oracle` Session/serial mouse/CGA tools, `winit`/`softbuffer`, Lean 4 via `formal/lean-toolchain`. No production dependency on `acad-oracle`.

**Spec:** `docs/HANDOVER-2026-09-30.md`, known limits item 3; `docs/superpowers/plans/2026-10-01-screen-menu-rendering.md`, recovered Task 1 findings; `corpus/System/ACAD.MNU`; `docs/oracle-qemu.md`; `formal/README.md`.

**Common execution workflow:** [Recovery subagent roadmap](2026-10-01-recovery-subagent-roadmap.md) owns model routing, fresh implementer/reviewer dispatch, ledger, worktrees, escalation, integration, and final whole-branch review. Task 3/4 implementation is accepted. Task 5 documentation is updated; serial gates passed (310 non-oracle tests; menu_controls 10/10 with one executed FSLINE and zero skips; menu_mouse 1/1; fmt, Clippy, and Lean passed). Final Astra whole-branch review and local integration remain coordinator-owned.

## Global Constraints

- Planning baseline: `main` at `1f38904`; this draft does not run QEMU, edit production code, or commit.
- Native CGA coordinates are **640×200**. `Frame::to_rgb_640x400()` doubles Y; a screenshot Y must be halved before `device_for_pixel`.
- Keep corpus images unchanged. Use `Session::boot_disposable`; copy evidence out before Session drops its private directory.
- Maintain the existing app routing: panel consumes its own clicks before drawing points or selections; blank panel slots remain consumed.
- Preserve typed command behavior and existing menu pagination, rendering, and physical minimum-size rules unless native evidence specifically requires a scoped correction.
- No guessed control semantics: typed `SNAP` and `ORTHO` enter prompts; this does not establish raw-byte click behavior.
- `< GO >` resetting to page 0 is a documented Rust simplification. The earlier native click displayed `Unknown command. Type ? for list of commands.`; the cause remains unresolved.
- Custom macro M1–M3 work (error/Quit continuation, whitespace/TEXT semantics, LOAD registration) is outside this plan.
- Lean is an observable behavioral specification, not recovered AutoCAD source or proof of unobserved native behavior.

## Review Focus

1. Control clicked while a point or scalar prompt is active: pin whether the same prompt and pending geometry survive, using a continuation after the click.
2. Cancel after one completed LINE segment and an unfinished next segment: pin exactly which entities survive and whether a later command starts cleanly.
3. Cancel with an unsubmitted app selection/input buffer: pin buffer clearing separately from editor state, and ensure the panel click cannot select a drawing entity.
4. SNAP enabled/disabled at a nondefault spacing and ORTHO enabled/disabled: inspect exported settings and a deliberate off-grid/non-axis mouse continuation; flags alone may conceal wrong point behavior.
5. GO versus a bare semicolon, Return, and a blank header slot: reproduce the prior error before attributing it to dispatch, page navigation, or a point leaking into command input.

---

## Current code audit and file boundaries

The project graph was queried first (`search_graph`, then `trace_path`/`get_code_snippet`). It lacks current Editor/Session methods and the returned `Vm.capture_editor` snippet points at unrelated FAT tests. The source reads below are the necessary fallback; do not treat graph line numbers as current.

| File / interface at baseline | Finding / ownership |
|---|---|
| `crates/acad-app/src/main.rs:122` — `App::handle_panel_click(&mut self, width: u32, height: u32, handle_result: &mut impl FnMut(&mut Self, Result<acad_cmd::Effect, String>)) -> bool` | Single-byte controls only set an unsupported status. Plain macros use the existing splitter. NEXT and GO share `advance_menu_page`. |
| `crates/acad-app/src/main.rs:171` — `App::handle_left_click(&mut self, width: u32, height: u32, handle_result: impl FnMut(&mut Self, Result<acad_cmd::Effect, String>))` | Tests already exercise the real panel-before-drawing route without an event loop. |
| `crates/acad-app/src/main.rs:207` — `pick_mouse_entity` | Selection numbers accumulate in `App.input`; they are not submitted immediately. Keyboard Escape clears `App.input`, then calls `cancel_command`. |
| `crates/acad-app/src/main.rs:357` — `advance_menu_page(...) -> usize` | NEXT wraps; GO returns 0. `menu_panel::entry_at` only yields GO on a page with a header; page 1/2 top slots are blank. |
| `crates/acad-cmd/src/lib.rs:220` — `Editor::cancel_command(&mut self) -> Result<Effect, String>` | Clears `state`, `repeat_start`, and `status`; it does not undo completed drawing mutations. |
| `crates/acad-cmd/src/editor_ops.rs:19` — internal `cancel` | Resets only command state. Do not mechanically substitute it for `cancel_command`. |
| `crates/acad-cmd/src/lib.rs:280` — `submit_mouse_point(&mut self, point: Point) -> Result<Effect, String>` | Formats a point and calls `submit`; it currently applies no SNAP/ORTHO projection. |
| `crates/acad-cmd/src/dispatch.rs:697,711,1323,1332` | Typed SNAP/RES/RESOLUTION and ORTHO update header settings after separate prompt input. |
| `crates/acad-model/src/header.rs` | Existing settings are `Header.snap: Mode { on: bool, spacing: f64 }` and `Header.ortho: bool`; no model change is currently justified. |
| `crates/acad-oracle/src/session.rs` | Use the real Session API listed below; do not add an imagined native export/macro API. |
| `formal/AutoCAD/HelpFiles.lean`, `Units.lean`, `Geometry.lean` | Existing contracts cover MENU file input, resolution settings, and selected geometry prompts. None specifies menu-byte clicks, GO, or app mouse routing. |

Future changes are limited to: new recovery example and evidence files; `acad-cmd/src/lib.rs` or a focused `menu_controls.rs` if an editor control interface is warranted; `acad-app/src/main.rs` click routing/tests; `acad-oracle/tests/menu_controls.rs`; `formal/AutoCAD/MenuControls.lean`, `formal/lakefile.lean`, `formal/README.md`; the handover. `menu_panel.rs` changes are conditional on a demonstrated GO hit-test correction. Do not refactor DIM/HATCH, the splitter, codecs, or rendering as part of this slice.

## Task 1: Recover control clicks and isolate GO

**Files:**
- Create: `crates/acad-oracle/examples/menu-controls-recovery.rs`
- Create: `docs/recovery/2026-10-01-menu-controls/README.md`, `transcript.tsv`, `observations.tsv`
- Create: `docs/recovery/2026-10-01-menu-controls/captures/`, `drawings/` (native PNG/CGA/DWG/DXF evidence)
- Read: existing `examples/menu-pagination-recovery.rs`, `tests/menu_mouse.rs`, `tests/fillet_mouse.rs`, `src/session.rs`, and `src/qemu.rs:253-296`.

**Interfaces (existing):**
`Session::boot_disposable(system: &Path, samples: Option<&Path>, backups: &[&str]) -> Result<Session, String>`; `type_line(&mut self, &str)`, `key(&mut self, qcode: &str, down: bool)`, `mouse_pin(&mut self)`, `mouse_to(&mut self, x: i32, y: i32, buttons: u8)` all return `Result<(), String>`; `capture_editor(&mut self) -> Result<Vec<u8>, String>`; `read_system_file(&self, filename: &str) -> Result<Vec<u8>, String>`; `shutdown(&mut self) -> Result<(), String>`. `device_for_pixel(column: usize, row: usize) -> (i32, i32)` uses native coordinates.

**Produces:** A checked evidence bundle, with each input linked to a stable pre/post frame and each saved drawing linked to its native exports. Research uses a separate worktree/private image; reading, harness preparation, and offline analysis may occur independently of DIM/HATCH. Acquire the roadmap's coordinator probe lease before booting: **one native guest total**. Session's mutex is process-local and does not serialize other executables; concurrent guests can produce incomplete stable CGA captures. Do not bypass the lock or compensate by increasing timeouts.

- [x] **Step 1: Prepare the isolated recovery checkout at execution time.** Follow `superpowers:using-git-worktrees`; use branch `research/menu-controls` and a separate Cargo target directory. Verify the baseline and image presence without modifying corpus files:

```bash
git rev-parse --short HEAD
git status --short
test -f 'corpus/raw/Autodesk AutoCAD 1.4 (5.25)/System.img'
command -v qemu-system-i386
```

The recovery example must report missing prerequisites and produce no claimed observations in that case. Retain the existing `-d nochain` Session launch workaround.

- [x] **Step 2: Add the observation tool, reusing the existing PNG converter and click helper.** Its optional sole argument is the evidence directory; default to `docs/recovery/2026-10-01-menu-controls`. Fail on an existing nonempty bundle to avoid mixing runs. Log case ID, every typed line/key/mouse transition, native coordinates, `Session::pointer()`, and artifact paths. Save raw 16 KiB CGA plus PNG after each stable capture; transcribe visible graphics prompts by reading PNGs, not `text_screen()`.

```rust
fn click_pixel(vm: &mut Session, column: usize, row: usize) -> Result<Vec<u8>, String> {
    let (x, y) = device_for_pixel(column, row);
    vm.mouse_to(x, y, 0)?;
    vm.capture_editor()?;
    vm.mouse_to(x, y, LEFT)?;
    std::thread::sleep(Duration::from_millis(300));
    vm.mouse_to(x, y, 0)?;
    std::thread::sleep(Duration::from_millis(300));
    vm.capture_editor()
}
```

Load `MENU`, `ACAD`; pin once the editor is active. Start at column 600; native row centers are GO 4, Snap 12, Ortho 20, Cancel 156, NEXT 164. These derive from the recovered row bands, not a new guarantee of every hit boundary. Check the PNG label before every click. On later pages Cancel remains row 19; row 0 is blank rather than GO. Verify a top-row crosshair capture because the mouse inverse unit test only covers native rows 8..161.

- [x] **Step 3: Run the finite recovery matrix in stages.** Start with idle controls, one active LINE continuation, completed/pending LINE cancellation, selection cancellation, and the GO comparison. Expand only to the remaining listed cases needed to establish the intended contract or discriminate an ambiguity; do not explore every InputState. Use separate fresh cases for destructive/cancel probes, and one case per before/after setting where END is needed. Use 1–8 uppercase/digit DOS drawing names (`SCIDLE`, `OCIDLE`, `CLSTART`, `CLSEG`, `CCENTER`, `CSELECT`, `CREPEAT`, `GOIDLE`). Record baseline exports and repeat a disputed observation in a fresh Session.

| Cases | Inputs and observation purpose |
|---|---|
| Snap idle | Typed `SNAP`, `0.5`, then `SNAP`, `OFF`; click Snap once and twice in independent saveable cases. Also begin with `SNAP`, `ON`. Record enabled bit, spacing, command prompt and status before/after. |
| Ortho idle | Typed `ORTHO`, `OFF` and separately `ORTHO`, `ON`; click once/twice, inspect exported setting and prompts. |
| Controls during active input | `LINE`, `2,3` then Snap/Ortho; capture and continue with typed `4,5`, then Return. Also `CIRCLE`, `2,3` at radius prompt. Determine whether the control consumes ordinary prompt input or acts independently. |
| Mouse continuation | At `LINE` next-point prompt, use identical off-grid/non-axis native pixels `(250,90)` for each setting combination. Capture raw point/crosshair and export complete geometry. Do not infer point coordinates from app viewport math. |
| Cancel idle / pending geometry | Idle; `LINE` before first point; `LINE`, `2,3`; `LINE`, `2,3`, `4,5`; `CIRCLE`, `2,3`. Capture then click Cancel and try an independent `POINT`, `8,7` continuation. Compare which earlier entities survive. |
| Cancel selection / REPEAT | Seed `LINE`, `2,3`, `4,5`, Return; `ERASE`, mouse-pick the line, then Cancel. Separately `REPEAT`, `POINT`, `4,5`, Cancel, then attempt `ENDREP` and a new REPEAT. Capture prompts and saved entity/group structure; do not presume cancellation is rollback. |
| Cancel pages / typed buffer | Reach each of the three menu pages, enter a point/selection prompt, click the real Cancel row. At idle and active prompts, use paced `Session::key` presses without Return to leave partial text before Cancel. Distinguish text buffering from command state. |
| GO comparison | Page-0 idle click; Return; literal semicolon key; GO during LINE first/next point, CIRCLE radius, and ERASE selection; row-0 blank click on pages 1/2. Repeat GO after moving across different drawing pixels. Capture panel/prompt changes and export any resulting entities. |

Literal semicolon is unsupported by `Session::type_line`; use `key("semicolon", true)`, release with `false`, capture, then separately press/release `"ret"`. For control-key comparisons use `key("ctrl", true)`, press/release `"b"`, `"o"`, or `"c"`, then release Ctrl. These are comparison inputs, not assumed equivalents. Avoid leaving a modifier pressed. Native pending selection is not automatically identical to Rust's `App.input` buffer.

- [x] **Step 4: Save exports without pretending END works at an active prompt.** First capture the observed result. Continue or cancel only after the screenshot confirms the current prompt; log cleanup separately. Reuse the native main-menu export flow once at Command:

```rust
vm.type_line("END")?;
vm.wait_for_text("Enter selection:", Duration::from_secs(60))?;
vm.type_line("5")?;
vm.wait_for_text("Enter NAME of drawing (default", Duration::from_secs(60))?;
vm.type_line(name)?;
vm.wait_for_text("Drawing interchange file complete.", Duration::from_secs(60))?;
vm.shutdown()?;
let dwg = vm.read_system_file(&format!("{name}.DWG"))?;
let dxf = vm.read_system_file(&format!("{name}.DXF"))?;
let drawing = acad_dwg::parse(&dwg)?;
```

Persist `dwg`, `dxf`, decoded `Header`/`Item` debug output and file hashes while Session still exists. Parse errors require string conversion in a `Result<(), String>` example. Do not substitute Rust-generated DXF for native DXF. An unsaveable case still needs frames/transcript and a clear statement that no header snapshot was obtained.

- [x] **Step 5: Run and classify observations.** This is research, not a passing compatibility test:

```bash
cargo run -p acad-oracle --example menu-controls-recovery -- docs/recovery/2026-10-01-menu-controls
cargo fmt --all --check
cargo clippy -p acad-oracle --all-targets -- -D warnings
```

`observations.tsv` columns: `case`, `before_prompt`, `after_prompt`, `before_snap_on`, `after_snap_on`, `before_spacing`, `after_spacing`, `before_ortho`, `after_ortho`, `pending_continuation`, `saved_entities`, `artifact`, `certainty`. Mark observed / inferred / unresolved separately. README states exact image hash, baseline commit, QEMU version, artifact index, cleanup inputs, and any repeat discrepancies. No expected Boolean, geometry, or error value is supplied by this plan.

- [x] **Step 6: Fresh evidence review.** Reviewer checks click landings, actual transcript, all Review Focus cases, native headers/entities, and cleanup contamination. GO is resolved only if comparison probes distinguish mechanisms sufficiently to define observable behavior; otherwise produce a precise narrowed ambiguity for escalation. Commit only reviewed tool/evidence files, without production code.

## Task 2: Freeze the implementation brief and observed contracts

**Files:** Create `docs/recovery/2026-10-01-menu-controls/implementation.md`; conditionally create `formal/AutoCAD/MenuControls.lean` and modify `formal/lakefile.lean`, `formal/README.md`.

**Consumes:** Task 1 bundle and evidence review. **Produces:** A concrete brief naming each observed transition, fixture, test name, Rust API, exact affected files, and unresolved cases. This is an execution gate, not permission to implement guessed transitions.

- [x] **Step 1: Write the brief from evidence.** Specify whether Snap/Ortho are immediate toggles, prompt inputs, or another observed operation; exact spacing preservation; prompt/pending-geometry behavior; Cancel entity retention, repeat behavior, and app-buffer rule; status behavior; GO's behavior and where it is clickable. Pin follow-up inputs that prove state rather than merely asserting the prompt string.
- [x] **Step 2: Define a focused editor interface only where warranted.** Proposed signature, to be accepted or replaced in the reviewed brief: `pub enum MenuControl { Snap, Ortho, Cancel }` and `pub fn apply_menu_control(&mut self, control: MenuControl) -> Result<Effect, String>`. This API does **not** exist at baseline. Keep byte-to-enum mapping in app routing. Reuse `cancel_command` only if its observed retention/reset behavior agrees; do not feed control bytes to text `submit` or route immediate controls through typed SNAP/ORTHO prompts without evidence.
- [x] **Step 3: Add Lean only for confirmed behavioral transitions.** New module `AutoCAD.MenuControls` models observed flags, spacing, prompt/pending state, cancellation retention, and GO only if resolved. Define explicit input/output states and fixture-linked `example` obligations from the brief; no unobserved universal transition rule. Add its root to the existing `lean_lib AutoCAD` list. This is a behavioral contract; Rust tests separately establish refinement. If Task 1 supports only UI pixels with no meaningful state contract, record why no Lean addition is warranted.
- [x] **Step 4: Build and review the brief/contract.** Run `cd formal && lake build`; fresh reviewer verifies every native claim links to an artifact and every promised test has an exact expected value from that artifact. Unresolved GO cannot block shipping separately recovered controls, but prevents a GO compatibility claim.

## Task 3: Implement the recovered controls with red/green tests

**Files:** Modify `crates/acad-cmd/src/lib.rs` (or add `src/menu_controls.rs` and module wiring), `crates/acad-cmd/tests/editor.rs`, `crates/acad-app/src/main.rs`; create `crates/acad-oracle/tests/menu_controls.rs`. Do not modify `dispatch.rs` unless the brief demonstrates a necessary state transition there.

**Consumes:** Reviewed Task 2 interface and expected observations. **Produces:** Observed editor controls and the real app mouse route, with native/fixture regressions. Code tasks run sequentially with a fresh implementer and fresh reviewer; do not share production-file ownership with DIM/HATCH tasks.

- [x] **Step 1: Write concrete failing editor, app, and native tests from the brief.** Update the old app no-op test `app_mouse_route_dispatches_point_and_consumes_control_entries`; its unsupported-status assertion will cease to describe the observed controls. Preserve its POINT-route coverage in a separate test. Add actual `menu_app`/`click` route cases for every control and pending `App.input` selection, not only a helper unit test.

Use native fixture comparisons, not assumed toggles. This skeleton uses the proposed Task 2 interface only if adopted; before dispatching this task replace the skeleton with the brief's complete cases and assertions:

```rust
use std::path::Path;

#[test]
fn snap_idle_matches_recorded_native_header() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../docs/recovery/2026-10-01-menu-controls/drawings");
    let before = acad_dwg::parse(&std::fs::read(root.join("SCIDLE-before.DWG")).unwrap()).unwrap();
    let expected = acad_dwg::parse(&std::fs::read(root.join("SCIDLE-after.DWG")).unwrap()).unwrap();
    let mut editor = acad_cmd::Editor::new(before);
    editor.apply_menu_control(acad_cmd::MenuControl::Snap).unwrap();
    assert_eq!(editor.drawing().header.snap, expected.header.snap);
    assert_eq!(editor.drawing().header.ortho, expected.header.ortho);
    assert_eq!(editor.drawing().items, expected.items);
}
```

Fixture names are the recovery tool's normalized output interface; native guest drawing names remain ≤8 characters. Require captured native before/after files to exist before running this fixture test. For live oracle cases retain existing absence guards and label skipped runs honestly. Compare full entity kinds/order/layers/geometry; use the codec's existing precision policy with tolerances justified by exported evidence. Do not impose full-header equality when unrelated native metadata differs.

- [x] **Step 2: Run the focused tests and inspect genuine failures.** Commands: `cargo test -p acad-cmd --test editor menu_control -- --nocapture`; `cargo test -p acad-app app_mouse_route -- --nocapture`; `cargo test -p acad-oracle --test menu_controls -- --nocapture`. The brief defines test names to match the editor filter. Compile failure for the new interface is an acceptable first red state; after compiling, verify behavioral assertions fail on baseline behavior.
- [x] **Step 3: Implement the minimal editor transitions and app dispatch.** Only bytes `02`, `0f`, `03` enter the recovered path; other controls keep their documented unsupported status. Route results through the existing handler. Apply the brief's input-buffer rule explicitly; ensure any Cancel clearing affects `App.input` as well as editor state when observed. Preserve completed entities according to evidence. Do not use `drawing_mut()` as the production control dispatch shortcut.
- [x] **Step 4: If mouse constraints were observed, bound the geometry change.** Flags alone do not reproduce an off-grid/orthogonal continuation. The brief must define native origin, rounding/tie rule, axis choice, and ordering when both are enabled before changing `submit_mouse_point`; if the matrix cannot distinguish these, extend recovery rather than invent formulas. Keep typed coordinates unchanged unless observed otherwise. Limit the first supported refinement to the proven prompts and disclose broader gaps.
- [x] **Step 5: Verify once, review, then commit the tested change.** Run focused tests above plus `cargo test -p acad-cmd -p acad-app`, `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D warnings`, and `cd formal && lake build`. Reviewer checks all five Review Focus lines and regression scope. Commit exact affected files after resolving review findings.

## Task 4: Correct GO only after its separate evidence gate

**Files:** Conditional modifications to `crates/acad-app/src/main.rs`, `src/menu_panel.rs`, `crates/acad-oracle/tests/menu_controls.rs`, and `formal/AutoCAD/MenuControls.lean`.

**Consumes:** Resolved GO observations and Task 2 brief. **Produces:** Proven GO behavior or an explicit remaining research boundary; never call unresolved behavior native-compatible.

- [x] **Step 1: Inspect the GO decision record.** If unresolved, use the root escalation path for cross-layer/native ambiguity. Do not replace page-reset simplification with a guessed unknown-command error. Controls may complete independently.
- [x] **Step 2: When resolved, freeze exact tests before code.** Test idle and active prompt clicks, page-0 header versus page-1/page-2 blank slots, visible status/prompt, continuation, exported entities, and page state. Exercise `handle_left_click` with the real parsed header action (`b";"`); a direct `advance_menu_page(...Go)` helper call from page 2 is not proof that the UI exposes that click.
- [x] **Step 3: Run red tests, minimally adjust GO routing, run green tests.** Keep NEXT assertions intact when replacing `advance_menu_page_wraps_next_and_resets_go_to_page_zero`. Command/status effects belong in app/editor routing, rather than smuggling them into a pagination-only helper. Change hit-testing only if Task 1 proved its current slot rules wrong.
- [x] **Step 4: Gate and fresh review.** Run `cargo test -p acad-app`, `cargo test -p acad-oracle --test menu_controls -- --nocapture`, format, workspace Clippy, and Lean build if affected. Commit the independently reviewed correction; unresolved GO has no production commit.

## Task 5: Report verified coverage and integrate sequentially

**Files:** Modify `docs/HANDOVER-2026-09-30.md`, `formal/README.md`, evidence README and this plan's checkboxes.

- [x] **Step 1: Update documentation from the final evidence.** Name the recovered controls, idle/active/pending cases tested, actual geometry coverage, and GO's resolved behavior or precise open question. Preserve M1–M3 limits. Do not claim Lean proves native mouse dispatch or untested constraint geometry.
- [x] **Step 2: Run integration gates on the final slice.** `cargo test --workspace --exclude acad-oracle -- --test-threads=1`; focused real-QEMU `cargo test -p acad-oracle --test menu_controls -- --nocapture` and existing `--test menu_mouse`; `cargo fmt --all --check`; `cargo clippy --workspace --all-targets -- -D warnings`; `cd formal && lake build`. Record skips separately from real passes. Retest after fixes only as needed.
- [ ] **Step 3: Final whole-branch review and merge handoff.** Follow the roadmap's model/workflow policy, including the required single Astra final whole-branch review for this independently completed slice. Reviewer gets baseline, diff, brief, evidence index, and checks. Merge sequentially with other slices; no parallel writers on app/editor/formal shared files.

## Self-review / known evidence boundaries

The plan covers all three shipped control bytes, Cancel on all pages, active point/scalar prompts, pending geometry and selections, nondefault settings, mouse continuation, and GO's unresolved mechanism. It supplies current APIs, artifact paths, native coordinate conversion, export flow, tests/gates, and a mandatory evidence-backed brief before code dispatch. The deferred mouse-refinement gate is explicit: `submit_mouse_point` is unchanged, mouse Snap/Ortho projection is unimplemented, and origin/ties/negative rounding/arbitrary spacing/axis/order/view rules remain unresolved. True native blank-slot mapping and M1–M3 remain explicit limits.
