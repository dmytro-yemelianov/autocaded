# DIM Angular-Mode Recovery Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Recover, from the native AutoCAD 1.4 oracle under QEMU, the exact prompt sequence and output geometry for `DIM`'s `A` (angular) sub-letter, and add one QEMU differential regression case proving the Rust editor reproduces it — the project's next-smallest step per `docs/HANDOVER-2026-09-30.md`'s "Known limits and next work" item 1.

**Architecture:** This project's own rule (stated in `docs/HANDOVER-2026-09-30.md` and `formal/README.md`) is: recover observable facts under QEMU first, implement the Rust refinement second, extend the Lean formal spec last. `DIM`'s prompt string already advertises `"DIM: first extension line origin or (ABCT)"` (`crates/acad-cmd/src/input_state.rs:279`), but the editor's dispatch (`crates/acad-cmd/src/dispatch.rs:423`) only ever parses the first line as a point — it never checks for `A`/`B`/`C`/`T` before parsing, so today `DIM` followed by `A` returns a parse error instead of entering angular mode. **This plan is intentionally split into a recovery phase (Task 1, fully specified below) and an implementation phase that is scoped only after Task 1's findings exist** — writing the `InputState` variants and geometry formula for angular dimensioning before observing the real native prompt sequence would be exactly the undisciplined guess this project's evidence-first rule exists to prevent. Task 1 is complete, concrete, and immediately executable; Tasks 2+ are deliberately left as a follow-up to be written from Task 1's output, not invented now.

**Tech Stack:** Rust 2021, the existing `acad-oracle` QEMU harness (`crates/acad-oracle/src/session.rs`'s `Session` type), the AutoCAD 1.4 System floppy at `corpus/raw/Autodesk AutoCAD 1.4 (5.25)/System.img`.

**Spec:** `docs/HANDOVER-2026-09-30.md` ("Known limits and next work", item 1) and `formal/AutoCAD/Geometry.lean` (the current DIM contract, which only models the linear 3-point flow: `command --dim--> dimFirstExtension --point--> dimIntersection --point--> dimSecondExtension --point--> dimText --text--> dimensionEntities`).

## Global Constraints

- Recovery first: do not write any `InputState` variant, `dispatch.rs` branch, geometry formula, or Lean contract extension for angular dimensioning until Task 1's native observation is captured and recorded. A plausible-looking guess is not evidence.
- The existing linear DIM flow and its two QEMU regression cases (`DIMSHORT`, `DIMLONG` in `crates/acad-oracle/tests/commands.rs:59-61`) must keep passing unchanged — this is additive, not a rewrite of the linear path.
- Drawing names passed to `acad_oracle::generate_pair`/`Session` must be 1-8 uppercase ASCII letters or digits (DOS 8.3 filename limit, enforced in `crates/acad-oracle/src/qemu.rs:110-120` for the sibling `generate_wblock` function — the same disk format applies here).
- QEMU-dependent code must self-skip (not fail) when `qemu-system-i386` or the extracted `System.img` are absent, matching the existing pattern in every oracle test (`if !disk.exists() || !acad_oracle::available() { eprintln!(...); return; }`).
- Any new differential test must compare geometry with the same tolerance discipline as the existing DIM test (`crates/acad-oracle/tests/commands.rs:82-112`: 1e-6 for line endpoints, 2e-6 for solid vertices, ~0.003+1e-6 for text position/height, exact string match for dimension text) — do not invent a looser tolerance to make a test pass.

## Review Focus

- **Empty or whitespace-only input at the `DIM` first-extension prompt after entering angular mode**: a reasonable user backing out with a blank line should return to command input the same way the existing linear flow's blank-text-prompt case does, not panic or leave the editor in a stuck state. Task 1's recovery script should observe what the native does here as part of the same session.
- **Case sensitivity of the `A` sub-letter**: native AutoCAD's command-line letters are typically case-insensitive (see the existing `"W"`/`"WINDOW"` pattern at `crates/acad-cmd/src/dispatch.rs:502-503` using `eq_ignore_ascii_case`) — Task 1's recovery must check whether lowercase `a` is also accepted by the native editor, not assume it mirrors the `W` convention without checking.
- **Collinear or degenerate angle-defining points**: the existing linear `dimension_geometry` already guards against distinct/non-collinear points (`crates/acad-cmd/src/geometry.rs:317-330`) and returns a `Result<_, String>` error rather than panicking on degenerate input — whatever angular implementation follows from Task 1 must carry the same discipline; note this as a required property for the eventual Task 2, not something Task 1 itself has to resolve.
- **Interaction with the `T` (text override) and `C`/`B` (continue/baseline) sub-letters**: Task 1 should note, if the native prompt reveals it, whether angular mode still accepts a text override the same way the linear flow's final blank-or-typed line does — this affects how much of the existing `DimText` handling (`crates/acad-cmd/src/dispatch.rs:435-456`) can be reused versus needs its own state.
- **QEMU session flakiness**: the existing oracle tests already treat a missing disk/QEMU binary as a skip, not a failure — Task 1's recovery script must follow the same self-skip convention so it never becomes a false CI failure on a machine without the corpus.

---

## Task 1: Recover the native `DIM` `A` (angular) prompt sequence and output geometry

**Files:**
- Create: `crates/acad-oracle/examples/dim-angular-recovery.rs`
- Test: none (this is a one-shot recovery tool, run manually and its output recorded by hand into this plan's follow-up — it is not a `#[test]`, because its job is to print observations for a human/agent to read and write down, not to assert anything, since there is nothing yet to assert against)

**Interfaces:**
- Consumes: `acad_oracle::session::Session::boot_in_place(a: &Path, b: Option<&Path>) -> Result<Self, String>`, `Session::type_line(&mut self, line: &str) -> Result<(), String>`, `Session::text_screen(&mut self) -> Result<String, String>`, `Session::wait_for_text(&mut self, needle: &str, timeout: Duration) -> Result<(), String>`, `Session::shutdown(&mut self) -> Result<(), String>` (all already defined in `crates/acad-oracle/src/session.rs`).
- Produces: printed stdout observations only — this task produces recorded findings (written into this plan file's Task 2 placeholder section below, by hand, after running it), not new library code other than the example binary itself.

- [x] **Step 1: Write the recovery example**

Create `crates/acad-oracle/examples/dim-angular-recovery.rs`:

```rust
//! One-shot recovery tool: drives native AutoCAD 1.4 under QEMU through
//! `DIM` `A` (angular mode) and prints the text screen after every typed
//! line, so the exact prompt sequence and output can be read off by hand.
//! Not a test — there is nothing to assert yet. Run with:
//! `cargo run -p acad-oracle --example dim-angular-recovery`

use acad_oracle::session::Session;
use std::path::Path;
use std::time::Duration;

fn main() {
    let disk = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../corpus/raw/Autodesk AutoCAD 1.4 (5.25)/System.img");
    if !disk.exists() || !acad_oracle::available() {
        eprintln!("skipping: extracted System.img or qemu-system-i386 absent");
        return;
    }

    let mut vm = Session::boot_in_place(&disk, None).expect("boot");
    vm.wait_for_text("Enter selection:", Duration::from_secs(60))
        .expect("boot to main menu");
    vm.type_line("1").expect("choose create new drawing");
    vm.wait_for_text("Enter NAME of drawing:", Duration::from_secs(60))
        .expect("name prompt");
    vm.type_line("DIMANGL").expect("drawing name");
    vm.wait_until_text_gone("Enter NAME of drawing:", Duration::from_secs(60))
        .expect("enter editor");
    std::thread::sleep(Duration::from_millis(500));

    // Draw two reference lines first so there is real geometry to pick an
    // angle between (arbitrary right-angle corner at the origin).
    for line in ["LINE", "0,0", "5,0", "", "LINE", "0,0", "0,5", ""] {
        vm.type_line(line).expect("draw reference lines");
    }

    // Now drive DIM, then the angular sub-letter, printing the screen after
    // each line so the exact prompt text is legible. Adjust/extend this
    // input sequence interactively (re-run after editing) until a complete
    // angular dimension is produced or the native editor errors out —
    // that error message is itself a recovered fact worth recording.
    let probe_inputs = ["DIM", "A", "0,0", "5,0", "0,0", "0,5", "3,3", ""];
    for input in probe_inputs {
        vm.type_line(input).expect("type probe input");
        std::thread::sleep(Duration::from_millis(300));
        let screen = vm.text_screen().expect("read screen");
        println!("--- after typing {input:?} ---\n{screen}\n");
    }

    // Also probe whether lowercase `a` and a blank line at the first
    // extension prompt behave the same as `A` and the linear flow's blank
    // line, per this plan's Review Focus.
    vm.type_line("U").ok(); // undo the (possibly failed/partial) angular attempt
    for input in ["DIM", "a"] {
        vm.type_line(input).expect("type lowercase probe");
        std::thread::sleep(Duration::from_millis(300));
        let screen = vm.text_screen().expect("read screen");
        println!("--- lowercase probe after {input:?} ---\n{screen}\n");
    }

    vm.shutdown().ok();
}
```

- [x] **Step 2: Run it and capture the transcript**

Run: `cargo run -p acad-oracle --example dim-angular-recovery 2>&1 | tee /tmp/dim-angular-recovery.log`

Expected: either the tool self-skips with `skipping: extracted System.img or qemu-system-i386 absent` (if this machine lacks the corpus/QEMU — acceptable, it means this task's Step 3 cannot be completed on this machine and must be handed to one that has them), or it prints one `--- after typing ... ---` block per probed input showing the native screen's text content at each step.

- [x] **Step 3: Record the findings in this plan file**

**The originally planned live screen-probing approach did not work** (see the ledger's Task 1 Ruling): `Session::text_screen()` only decodes a genuine full-screen 80x25 text page and returns byte-identical garbage once inside AutoCAD's drawing editor, which uses a graphics/mixed-mode screen. The recovery tool was rewritten to use this codebase's own proven blind-sequence-in/decoded-DWG-out technique (`acad_oracle::generate_pair`, the same method `original_dim_exports_primitive_geometry_matched_by_rust` already uses for linear DIM) instead of reading the live screen.

**What was actually observed**, from 6 candidate input sequences run against the real AutoCAD 1.4 oracle (all starting from two reference lines, `0,0`-`5,0` and `0,0`-`0,5`, then `DIM`, `A`):

| Sequence after `DIM`, `A` | Result |
|---|---|
| `""` (immediate blank) | Succeeds cleanly, returns to the drawing menu; **no new entity is added** — only the 2 reference lines exist afterward. |
| `"0,0"`, `""` | **Hangs** — `generate_pair` times out (60s) waiting for the session to return to the "Current drawing:" menu screen. |
| `"0,0"`, `"5,0"`, `""` | Hangs, same timeout. |
| `"0,0"`, `"5,0"`, `"0,0"`, `""` | Hangs, same timeout. |
| `"0,0"`, `"5,0"`, `"0,0"`, `"0,5"`, `"3,3"`, `""` | Hangs, same timeout. |
| `"1,0"`, `"0,1"`, `"3,3"`, `""` | Hangs, same timeout. |

**Interpretation** (this is inference from the pattern above, not a directly-read fact — flagged as such): `A` is very likely recognized as a real sub-mode (not silently rejected — if it were, a bare `""` afterward should have been "swallowed" by whatever `A`'s error left behind rather than cleanly closing the drawing), and that sub-mode expects a specific, currently-unknown number and shape of further inputs before it can cleanly terminate. Every guessed shape tried (1 point, 2 points, 3 points, 4 points + text, or a 2-point-pick-plus-location variant) left the session mid-prompt when the test harness tried to close the drawing, hence the timeout — meaning every one of these guesses was wrong (too few inputs, or the wrong kind of input, e.g. maybe it wants a keyword or a different coordinate convention, not bare `x,y` points).

**What was NOT recovered**: the exact prompt text, the exact number of points/inputs the native `A` sub-mode requires, and therefore the output geometry it produces. Lowercase `a` was not re-tested with this method (the finding from the original garbled screen-probe run is discarded as unreliable, since that whole run's readings were uninterpretable garbage).

**Recommended next step** (for whoever picks up Tasks 2+): blind bisection has diminishing returns past this point — each further guess costs a full QEMU boot (~30-60s) and narrows very little. The next productive step is almost certainly to reuse the CGA graphics-mode decoder already built for the interactive `acad-qemu` example (`acad_oracle::cga::{Mode, ModeTracker, Frame, ...}`, see `crates/acad-oracle/examples/acad-qemu.rs`) to actually read AutoCAD's live status-line prompt text while single-stepping through `DIM`, `A`, one input at a time — either by a human driving that interactive window directly, or by teaching a new recovery tool to decode frames with that same machinery instead of the plain-text-only `Session::text_screen()`. This is a larger task than this plan's Task 1 scoped for (it requires graphics-mode text extraction, not just blind sequence probing), so it is out of this plan's scope and should be its own follow-up plan.

- [x] **Step 4: Commit the recovery tool and findings**

```bash
git add crates/acad-oracle/examples/dim-angular-recovery.rs docs/superpowers/plans/2026-09-30-dim-angular-recovery.md
git commit -m "research(oracle): recover native DIM angular-mode prompt sequence"
```

---

## Tasks 2+: deliberately not yet written

Per this plan's Architecture section and the project's own evidence-first rule, the `InputState` variants, the `dispatch.rs` branch recognizing `A`/`a` at the `DimFirstExtension` state, the `angular_dimension_geometry` function, the QEMU differential test case, and the `formal/AutoCAD/Geometry.lean` contract extension all depend on Task 1's recorded findings — specifically, how many points the native flow asks for and in what order, whether it reuses the existing `DimText` step, and what its output entities look like (LINE/ARC/SOLID/TEXT primitives, or something else). Writing these tasks now, before that transcript exists, would mean guessing at a 1983 CAD program's exact interaction model and presenting the guess as a plan — precisely what this project's handover doc warns against for every partial command.

**Once Task 1's Step 3 findings are recorded, extend this plan with Task 2 (add `InputState`/`dispatch.rs` recognition of the `A` sub-letter, matching the exact recovered prompt sequence), Task 3 (implement `angular_dimension_geometry` in `crates/acad-cmd/src/geometry.rs` matching the exact recovered output geometry), Task 4 (add the QEMU differential regression case to `crates/acad-oracle/tests/commands.rs`, following the existing `original_dim_exports_primitive_geometry_matched_by_rust` test's structure), and Task 5 (extend `formal/AutoCAD/Geometry.lean`'s `Phase`/`Input`/`Outcome`/`advance` to cover the new states, following the existing linear-flow entries at lines 33-36, 73-76).**
