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
- Modify: `crates/acad-oracle/Cargo.toml` (added `tiny-skia = "0.11"` as a dev-dependency, to encode captured CGA framebuffers as viewable PNGs — see Step 3's Revision 2)
- Test: none (this is a one-shot recovery tool, run manually and its output recorded by hand into this plan's follow-up — it is not a `#[test]`, because its job is to print observations for a human/agent to read and write down, not to assert anything, since there is nothing yet to assert against)

**Interfaces:**
- Consumes (final, as committed — see Step 3 for how this evolved from the originally planned `text_screen()`-based approach): `acad_oracle::session::Session::boot_disposable`, `Session::type_line`, `Session::wait_for_text`, `Session::wait_until_text_gone`, `Session::capture_editor() -> Result<Vec<u8>, String>`, `Session::shutdown` (all in `crates/acad-oracle/src/session.rs`); `acad_oracle::cga::Frame::new(&[u8]) -> Result<Frame, String>` and `Frame::to_rgb_640x400() -> Vec<u32>` (in `crates/acad-oracle/src/cga.rs`); `tiny_skia::Pixmap`/`PremultipliedColorU8` for PNG encoding.
- Produces: PNG files under the OS temp directory (one per candidate, named `dim-angular-<CANDIDATE>.png`) plus printed stdout status — this task's real deliverable is the findings recorded in this plan file after reading those images, not new library code other than the example binary itself.

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

**This step went through two revisions; both are recorded here rather than silently overwritten, per this project's evidence-first discipline.**

**Revision 1 (superseded):** The originally planned live screen-probing approach did not work (see the ledger's Task 1 Ruling): `Session::text_screen()` only decodes a genuine full-screen 80x25 text page and returns byte-identical garbage once inside AutoCAD's drawing editor. The tool was rewritten to use `acad_oracle::generate_pair` (blind sequence in, decoded DWG entities out — the same method the existing linear-DIM QEMU test uses) instead. That version recorded 6 candidates, all timing out except a bare `DIM`, `A`, `""`, and drew a weakly-supported inference from the timeout pattern alone (a whole-branch review correctly flagged this reasoning as not actually distinguishing "`A` is recognized and wants more input" from "`A` is rejected and something else in the guessed sequence caused the hang").

**Revision 2 (current, visually confirmed):** Rather than keep guessing blind, the recovery tool was rewritten again to capture the AutoCAD drawing editor's raw CGA framebuffer directly — via `Session::capture_editor()`, independent of whether the drawing can later close cleanly — and render it as a PNG with the existing `acad_oracle::cga::Frame::to_rgb_640x400()` decoder (using `tiny-skia`, added as a new dev-dependency, to encode the PNG). This sidesteps the original problem entirely: the drawing editor's bottom status/command lines ARE legible bitmap text once decoded this way, no OCR or font-matching needed — they can simply be read by eye. The same 6 candidate sequences were re-run and their resulting screens captured as images.

**What was actually observed, directly, by reading the captured screens:**

| Candidate | Sequence after `DIM` | Captured screen (bottom 3 lines) |
|---|---|---|
| DIMANG3 | `A`, `""` | `Dimension arrow size:` / `*Invalid*` / `Command:` |
| DIMANG1, DIMANG2, DIMANG4, DIMANG5, DIMANG6 | `A` plus 1-5 further coordinate inputs, `""` | All five show the identical, unchanged screen: `Command:` / `DIM` / `First extension line origin or (ABCT):` — i.e. still sitting at DIM's very first prompt, as if none of the input after `DIM` had any visible effect. |

**Two solid facts, directly read, not inferred:**
1. `DIM`'s own first prompt is confirmed, by direct visual read, to be exactly `First extension line origin or (ABCT):` — this matches the existing Rust implementation's prompt string (`crates/acad-cmd/src/input_state.rs:279`) verbatim. That existing prompt text was evidently already accurate.
2. A `DIM` session that receives only `A` followed immediately by a blank line ends up back at `Command:` after passing through a screen reading `Dimension arrow size:` / `*Invalid*` — i.e. rejecting some input as invalid for a numeric prompt. Whether that `Dimension arrow size:` prompt is itself a real, separate, always-first step of native `DIM` that today's Rust implementation is simply missing, or something `A` triggers indirectly, was not resolved before this task's time budget ran out — it needs one more targeted probe (see Next step).

**One clear methodology finding, not a native-behavior finding:** the 5-candidates-identical-frozen-screen result is far more consistent with a synchronization defect in this recovery tool's own `capture_stuck_frame` helper (which calls `Session::capture_editor()` only once, after typing every line in a candidate, rather than after each individual line) than with 5 different guessed input shapes all coincidentally producing byte-identical screens. `capture_editor()`'s BIOS-keyboard-queue-drain check can pass once DOS has read a keystroke out of its buffer, which does not guarantee AutoCAD's own command loop has finished acting on it and redrawn before the *next* line is typed — so a later line can plausibly race ahead of the screen update the previous one caused. This is a tooling gap to fix, not a recovered fact about native AutoCAD.

**What was NOT recovered**: whether `A` is recognized as a real sub-mode at all (the frozen screens are inconclusive due to the synchronization issue above), what further input it would want if so, and the resulting output geometry. Lowercase `a` was not re-tested with this method.

**Recommended next step** (for whoever picks up Tasks 2+): fix `capture_stuck_frame` to call `Session::capture_editor()` after *each* typed line (not once at the end) and save a numbered PNG per step, so the screen's progression — or lack of it — after each individual keystroke is directly visible, exactly as this task's `DIM`, `A`, `""` capture already proved is possible. That is a small, mechanical change to the existing tool, not a new decoder or a separate research project — the hard part (reading AutoCAD's own on-screen bitmap text) is already solved by this task's Revision 2.

- [x] **Step 4: Commit the recovery tool and findings**

```bash
git add crates/acad-oracle/examples/dim-angular-recovery.rs docs/superpowers/plans/2026-09-30-dim-angular-recovery.md
git commit -m "research(oracle): recover native DIM angular-mode prompt sequence"
```

---

## Tasks 2+: deliberately not yet written

Per this plan's Architecture section and the project's own evidence-first rule, the `InputState` variants, the `dispatch.rs` branch recognizing `A`/`a` at the `DimFirstExtension` state, the `angular_dimension_geometry` function, the QEMU differential test case, and the `formal/AutoCAD/Geometry.lean` contract extension all depend on Task 1's recorded findings — specifically, how many points the native flow asks for and in what order, whether it reuses the existing `DimText` step, and what its output entities look like (LINE/ARC/SOLID/TEXT primitives, or something else). Writing these tasks now, before that transcript exists, would mean guessing at a 1983 CAD program's exact interaction model and presenting the guess as a plan — precisely what this project's handover doc warns against for every partial command.

**Once Task 1's Step 3 findings are recorded, extend this plan with Task 2 (add `InputState`/`dispatch.rs` recognition of the `A` sub-letter, matching the exact recovered prompt sequence), Task 3 (implement `angular_dimension_geometry` in `crates/acad-cmd/src/geometry.rs` matching the exact recovered output geometry), Task 4 (add the QEMU differential regression case to `crates/acad-oracle/tests/commands.rs`, following the existing `original_dim_exports_primitive_geometry_matched_by_rust` test's structure), and Task 5 (extend `formal/AutoCAD/Geometry.lean`'s `Phase`/`Input`/`Outcome`/`advance` to cover the new states, following the existing linear-flow entries at lines 33-36, 73-76).**
