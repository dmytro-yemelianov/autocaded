# acad-cmd lib.rs Module Split Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Break `crates/acad-cmd/src/lib.rs` (5,847 lines — the only outsized file in the workspace; every other file is under 2,650 lines) into focused modules by responsibility, with zero behavior change.

**Architecture:** This is a pure refactor, not new functionality — the full workspace test suite (`cargo test --workspace`, currently green) is the correctness oracle for every task, not new unit tests. Each task cuts a cohesive group of items (functions/types that already sit next to each other and share a theme) out of `lib.rs` into its own file, adjusts `mod` declarations and item visibility (`pub(crate)` where a moved item is used from more than one new module), and re-verifies the full gate before committing. Order is chosen so each extraction is independent of ones not yet done — pure/free functions first (no dependency on `Editor` internals), then `Editor`'s own non-dispatch methods, then the giant command-dispatch match last (it depends on everything else already being in place as `pub(crate)` or public items).

**Tech Stack:** Rust 2021, `acad_model` crate for domain types (`Drawing`, `Entity`, `Point`, `Item`, `Units`, etc.), workspace `cargo fmt` / `cargo clippy -D warnings` gates.

**Spec:** N/A — this is an internal code-organization refactor with no design-doc counterpart. The oracle is the existing `crates/acad-cmd` test suite (`cargo test -p acad-cmd -p acad-app`) plus the full workspace suite for the final task.

## Global Constraints

- No behavior change. No renamed public API (`Effect`, `FilesFilter`, `FilesRequest`, `Editor` and its existing `pub fn` methods keep their exact signatures and remain reachable at `acad_cmd::...` exactly as before).
- Every task must leave the tree green: `cargo test -p acad-cmd -p acad-app`, `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D warnings`.
- Run the full `cargo test --workspace` only once, in the final task — it's the expensive gate, don't re-run it after every file split.
- New files live in `crates/acad-cmd/src/`, declared via `mod name;` (private module, not `pub mod`) in `lib.rs`, since none of these groupings are meant to become new public API surface — only `pub use` re-exports if an item was previously reachable as `acad_cmd::item`. Check with `grep -rn "acad_cmd::" crates/acad-app crates/acad-oracle` before each task in case something outside the crate reaches a moved item directly (expected: nothing does, since only `Effect`, `FilesFilter`, `FilesRequest`, and `Editor`'s `pub fn`s are the crate's public surface, but verify rather than assume).
- Preserve existing doc comments and code verbatim during moves — this is a cut/paste refactor, not a rewrite. Do not "clean up" logic while moving it; that's a separate task if wanted later.
- Commit after each task (one file extraction = one commit), so a bad extraction is a single `git revert` away from the last green state.

## Review Focus

- A moved free function that was `fn` (private) and is called from a group staying behind in `lib.rs` (or from a different new module) must become `pub(crate) fn`, not silently left private and breaking the build — verify by building, not by eyeballing.
- `impl Editor` can legally be split across multiple files in the same crate (inherent impls aren't required to be in one place) — confirm this compiles after Task 7 before assuming the pattern is safe to repeat in Task 8.
- The `#[cfg(test)] mod tests` block (lines 4202–5847, ~1,645 lines) tests both pure free functions (e.g. `number()`, `hatch_line_geometry()`) directly by name and `Editor` behavior through its public API — a test calling a private helper must move into that helper's new module (`use super::*` still resolves it there), not stay behind in `lib.rs` where the helper no longer exists.
- `Transform` (line 37) is used both by `input_state.rs`'s `InputState` variants and by `Editor::transform`/`transform_entity` in `entity_ops.rs` — decide its home (input_state.rs, since `InputState` is its heavier consumer) and make it `pub(crate)` from there, not duplicated.
- After the last task, re-run `cargo doc -p acad-cmd --no-deps` (not part of the existing gate list) to confirm no broken intra-doc links resulted from moved items — cheap insurance the existing gates don't already cover.

---

## File Map (target end state)

```
crates/acad-cmd/src/
  lib.rs           – mod decls, Effect/FilesFilter/FilesRequest, Editor struct + Default impl,
                     first impl Editor block (state accessors: new, register_shape_library,
                     awaiting_shape_library_name, drawing, drawing_mut, prompt, status,
                     axis_spacing, cancel_command, accepts_mouse_point, submit_mouse_point,
                     accepts_mouse_selection, pick_entity_at)   [~600 lines]
  menu.rs          – unchanged
  input_state.rs   – Transform, InputState, EditCommand, RepeatDistanceInput,
                     ArraySpacingInput, impl InputState                [~290 lines]
  parse.rs         – number, parse_feet_inches, parse_fraction, format_measurement,
                     format_feet_inches, gcd, point, point_from, sin_cos_degrees,
                     layer_index, color_index, parse_toggle, parse_mode,
                     positive_count, positive_word                     [~330 lines]
  geometry.rs      – HatchEdge, hatch_sweep_deg, hatch_line_geometry, dimension_geometry,
                     repeat_points, entity_points, trace_quads, area_perimeter,
                     checked_area_metrics, line_loop_area, polygon_metrics,
                     three_point_arc, rotate_point, BreakGeometry, break_entity_geometry,
                     break_circle_angles, break_arc_geometry, LineBreakParts,
                     break_line_geometry, set_line_points, FilletGeometry,
                     fillet_geometry, line_points, entity_layer, assign_layer [~1,050 lines]
  selection.rs     – entities_in_window, selection_extents_points, selectable_count,
                     selection, selected_item_indexes, entity_pick_distance  [~160 lines]
  entity_ops.rs    – collect_insert_names, transform_entity, bare, normalize_library_name,
                     load_library_name, entity_anchor, item_anchor, can_change_point,
                     apply_change_point, set_insert_angle                     [~140 lines]
  report.rs        – help_report, hatch_pattern_report, list_entities,
                     database_listing                                        [~90 lines]
  editor_ops.rs    – second `impl Editor` block: cancel, add, ensure_layer, set_view,
                     create_block, wblock_entire_drawing, wblock_named_block,
                     wblock_selected_entities, explode_block, erase, oops, array,
                     circular_array, change_layer, change_point, fillet, break_entity,
                     measure_area, add_hatch, save_undo, transform, refresh_limits,
                     refresh_after_edit                                       [~600 lines]
  dispatch.rs      – third `impl Editor` block: submit, command                [~1,290 lines]
```

Tests currently in `lib.rs`'s trailing `mod tests` (lines 4202–5847) get redistributed: each test that only calls `Editor::new`/`Editor::submit`/other `pub fn`s moves to `crates/acad-cmd/tests/editor.rs` (a new integration test file, since those are all reachable through the public API); each test that calls a specific private free function directly (e.g. `number("3/4")`, `hatch_line_geometry(...)`) moves into a `#[cfg(test)] mod tests { use super::*; ... }` block at the bottom of that function's new file.

---

## Task 1: Extract `input_state.rs`

**Files:**
- Create: `crates/acad-cmd/src/input_state.rs`
- Modify: `crates/acad-cmd/src/lib.rs:1-4` (add `mod input_state;` and adjust imports), `crates/acad-cmd/src/lib.rs:37-326` (remove — moved out)

**Interfaces:**
- Consumes: `acad_model::Point` (already imported in lib.rs)
- Produces: `pub(crate) enum Transform`, `pub(crate) enum InputState`, `pub(crate) enum EditCommand`, `pub(crate) enum RepeatDistanceInput`, `pub(crate) enum ArraySpacingInput`, each with their existing variants and derives unchanged. `impl InputState` methods keep their existing signatures.

- [ ] **Step 1: Cut lines 37–326 of `lib.rs`** (the `Transform` enum through the end of `impl InputState`) into a new `crates/acad-cmd/src/input_state.rs`. Change `enum Transform` → `pub(crate) enum Transform`, `enum InputState` → `pub(crate) enum InputState`, `enum EditCommand` → `pub(crate) enum EditCommand`, `enum RepeatDistanceInput` → `pub(crate) enum RepeatDistanceInput`, `enum ArraySpacingInput` → `pub(crate) enum ArraySpacingInput`. Add at the top of the new file:
  ```rust
  use acad_model::Point;
  ```
- [ ] **Step 2: In `lib.rs`**, add `mod input_state;` under `pub mod menu;` and add `use input_state::{ArraySpacingInput, EditCommand, InputState, RepeatDistanceInput, Transform};` near the top-of-file imports. Delete the now-duplicated `use std::collections::{BTreeMap, BTreeSet};` only if no longer needed in `lib.rs` after this cut — check with a build, don't guess.
- [ ] **Step 3: Build.** Run: `cargo build -p acad-cmd 2>&1 | tail -60`. Fix any `unused import` or `private type escapes` errors by adjusting `pub(crate)` on the specific item the compiler flags — do not blanket-`pub` everything.
- [ ] **Step 4: Test.** Run: `cargo test -p acad-cmd -p acad-app`. Expected: same pass count as the pre-refactor baseline (record the baseline pass count before Task 1 starts by running this once on the unmodified tree).
- [ ] **Step 5: Gates.** Run: `cargo fmt --all --check` and `cargo clippy --workspace --all-targets -- -D warnings`. Fix any new warnings the move introduced (e.g. now-unused imports in `lib.rs`).
- [ ] **Step 6: Commit.**
  ```bash
  git add crates/acad-cmd/src/lib.rs crates/acad-cmd/src/input_state.rs
  git commit -m "refactor(cmd): extract InputState machine into input_state.rs"
  ```

## Task 2: Extract `parse.rs`

**Files:**
- Create: `crates/acad-cmd/src/parse.rs`
- Modify: `crates/acad-cmd/src/lib.rs` (remove moved items at former lines 3127–3141 and 4004–4201; add `mod parse;`)

**Interfaces:**
- Consumes: `acad_model::{Point, UnitFormat, Units, Mode}` as needed (check each function's actual signature before copying imports — `parse_mode` takes `acad_model::Mode`, most others take/return only `f64`/`String`/`Point`).
- Produces: `pub(crate) fn number`, `parse_feet_inches`, `parse_fraction`, `format_measurement`, `format_feet_inches`, `gcd`, `point`, `point_from`, `sin_cos_degrees`, `layer_index`, `color_index`, `parse_toggle`, `parse_mode`, `positive_count`, `positive_word` — all keep existing signatures.

- [ ] **Step 1:** Cut `positive_count` and `positive_word` (former lines 3127–3141) and `number` through `three_point_arc`'s preceding block, i.e. `number`..`parse_mode` (former lines 4004–4201, stop before `three_point_arc` which belongs in `geometry.rs`), into `crates/acad-cmd/src/parse.rs`. Mark each `pub(crate)`.
- [ ] **Step 2:** In `lib.rs`, add `mod parse;` and `use parse::{color_index, format_feet_inches, format_measurement, gcd, layer_index, number, parse_feet_inches, parse_fraction, parse_mode, parse_toggle, point, point_from, positive_count, positive_word, sin_cos_degrees};` (trim to only what `lib.rs` itself still calls directly — other new modules that need these import from `crate::parse::...` directly instead of re-exporting through `lib.rs`).
- [ ] **Step 3: Build, test, gate, commit** as in Task 1 Steps 3–6, with commit message `refactor(cmd): extract parsing helpers into parse.rs`.

## Task 3: Extract `geometry.rs`

**Files:**
- Create: `crates/acad-cmd/src/geometry.rs`
- Modify: `crates/acad-cmd/src/lib.rs`

**Interfaces:**
- Consumes: `crate::parse` is NOT needed here (geometry functions take numeric/`Point`/`Entity` args directly); needs `acad_model::{Entity, Item, Point, Repeat}` and `std::f64::consts` as the existing code already uses.
- Produces: `pub(crate) enum HatchEdge`, `pub(crate) struct BreakGeometry`, `pub(crate) struct LineBreakParts`, `pub(crate) struct FilletGeometry`, and `pub(crate) fn` for `hatch_sweep_deg`, `hatch_line_geometry`, `dimension_geometry`, `repeat_points`, `entity_points`, `trace_quads`, `area_perimeter`, `checked_area_metrics`, `line_loop_area`, `polygon_metrics`, `three_point_arc`, `rotate_point`, `break_entity_geometry`, `break_circle_angles`, `break_arc_geometry`, `break_line_geometry`, `set_line_points`, `fillet_geometry`, `line_points`, `entity_layer`, `assign_layer` — this is the largest extraction (~1,050 lines) but every item is a pure function/data-holder already grouped together in the source.

- [ ] **Step 1:** Cut former lines 2565–2879 (`HatchEdge` through `dimension_geometry`), 3045–3086 (`repeat_points`, `entity_points`), 3492–3701 (`rotate_point` through `entity_layer`, but stop before `BreakGeometry` — include it), 3701–4003 (`BreakGeometry` through `fillet_geometry`), and `three_point_arc` (former lines 4174–4201) into `geometry.rs`, in that reading order, each marked `pub(crate)`.
- [ ] **Step 2:** In `lib.rs`, add `mod geometry;` and import only what `lib.rs`'s remaining code (mainly the future `dispatch.rs`/`editor_ops.rs`, which will instead `use crate::geometry::...` directly) still needs at the `lib.rs` level — likely nothing, since these are all called from `Editor` methods that are about to move too. If `lib.rs` itself calls none of them directly after Tasks 7–8 are also done, skip adding a `use` here and let each consuming file import what it needs when it's extracted.
- [ ] **Step 3: Build, test, gate, commit**, commit message `refactor(cmd): extract geometry helpers into geometry.rs`.

## Task 4: Extract `selection.rs`

**Files:**
- Create: `crates/acad-cmd/src/selection.rs`
- Modify: `crates/acad-cmd/src/lib.rs`

**Interfaces:**
- Consumes: `acad_model::{Drawing, Entity, Point}`, `std::collections::BTreeSet`, and `crate::geometry::selection_extents_points`'s sibling helpers as needed (check actual bodies — `entity_pick_distance` likely calls geometry helpers like `line_points`, in which case add `use crate::geometry::line_points;`).
- Produces: `pub(crate) fn entities_in_window`, `selection_extents_points`, `selectable_count`, `selection`, `selected_item_indexes`, `entity_pick_distance`.

- [ ] **Step 1:** Cut former lines 2504–2564 (`entities_in_window`, `selection_extents_points`) and 3086–3218 (`selectable_count` through `entity_pick_distance`) into `selection.rs`, each `pub(crate)`.
- [ ] **Step 2:** Add `mod selection;` to `lib.rs`; resolve any cross-module calls the compiler flags (e.g. if `entity_pick_distance` calls a `geometry.rs` function, add the matching `use crate::geometry::...;` in `selection.rs`).
- [ ] **Step 3: Build, test, gate, commit**, commit message `refactor(cmd): extract selection helpers into selection.rs`.

## Task 5: Extract `entity_ops.rs`

**Files:**
- Create: `crates/acad-cmd/src/entity_ops.rs`
- Modify: `crates/acad-cmd/src/lib.rs`

**Interfaces:**
- Consumes: `acad_model::{Entity, Item, Point}`, `std::collections::BTreeSet`, `crate::input_state::Transform` (for `transform_entity`), `crate::geometry::rotate_point` (for `set_insert_angle`/rotation logic, if the body calls it — verify).
- Produces: `pub(crate) fn collect_insert_names`, `transform_entity`, `bare`, `normalize_library_name`, `load_library_name`, `entity_anchor`, `item_anchor`, `can_change_point`, `apply_change_point`, `set_insert_angle`.

- [ ] **Step 1:** Cut former lines 3282–3491 (`collect_insert_names` through `set_insert_angle`) into `entity_ops.rs`, each `pub(crate)`.
- [ ] **Step 2:** Add `mod entity_ops;` to `lib.rs`, and `use crate::input_state::Transform;` inside `entity_ops.rs` for `transform_entity`'s signature.
- [ ] **Step 3: Build, test, gate, commit**, commit message `refactor(cmd): extract entity mutation helpers into entity_ops.rs`.

## Task 6: Extract `report.rs`

**Files:**
- Create: `crates/acad-cmd/src/report.rs`
- Modify: `crates/acad-cmd/src/lib.rs`

**Interfaces:**
- Consumes: `acad_model::Drawing`, `std::collections::BTreeSet` (for `collect_insert_names` call inside `database_listing`/`list_entities` if present — import `crate::entity_ops::collect_insert_names` if so).
- Produces: `pub(crate) fn help_report`, `hatch_pattern_report`, `list_entities`, `database_listing`.

- [ ] **Step 1:** Cut former lines 2460–2503 (`help_report`, `hatch_pattern_report`) and 3218–3281 (`list_entities`, `database_listing`) into `report.rs`, each `pub(crate)`.
- [ ] **Step 2:** Add `mod report;` to `lib.rs`.
- [ ] **Step 3: Build, test, gate, commit**, commit message `refactor(cmd): extract reporting helpers into report.rs`.

## Task 7: Extract `editor_ops.rs` (second `impl Editor` block)

**Files:**
- Create: `crates/acad-cmd/src/editor_ops.rs`
- Modify: `crates/acad-cmd/src/lib.rs`

**Interfaces:**
- Consumes: `crate::Editor` (the struct stays defined in `lib.rs`; this file adds a second `impl Editor { ... }` block for it — confirm Rust allows this by building after Step 1, per Review Focus item 2), plus `crate::geometry::*`, `crate::entity_ops::*`, `crate::input_state::Transform`, `acad_model::{Block, Drawing, Entity, Extents, Header, Item, Point}` as each moved method's body requires.
- Produces: `Editor::cancel`, `add`, `ensure_layer`, `set_view`, `create_block`, `wblock_entire_drawing`, `wblock_named_block`, `wblock_selected_entities`, `explode_block`, `erase`, `oops`, `array`, `circular_array`, `change_layer`, `change_point`, `fillet`, `break_entity`, `measure_area`, `add_hatch`, `save_undo`, `transform`, `refresh_limits`, `refresh_after_edit` — all keep existing signatures and remain private (`fn`, not `pub fn`) since only `dispatch.rs` (Task 8) calls them.

- [ ] **Step 1:** Cut former lines 1863–2459 (`cancel` through `refresh_after_edit`, everything inside `impl Editor` after `pick_entity_at`'s block ends and before `submit` starts... note `submit` itself, lines 583–1862, stays behind for Task 8) into a new `impl Editor { ... }` block in `editor_ops.rs`. These methods stay non-`pub` — they're only called from `submit`/`command`, which move to `dispatch.rs` next, and Rust methods in a second `impl Editor` block in a sibling module of the same crate are visible to `dispatch.rs` as `self.method_name(...)` without any `pub(crate)` needed, since privacy in Rust is scoped to modules, not files — **verify this specific claim by building after this step, since `editor_ops.rs` as a *module* still needs its items visible to whatever module calls them.** If the build reports `method is private`, mark the specific flagged methods `pub(crate) fn` rather than guessing up front.
- [ ] **Step 2:** Add `mod editor_ops;` to `lib.rs`.
- [ ] **Step 3: Build, test, gate, commit**, commit message `refactor(cmd): extract Editor mutation methods into editor_ops.rs`.

## Task 8: Extract `dispatch.rs` (third `impl Editor` block — the command interpreter)

**Files:**
- Create: `crates/acad-cmd/src/dispatch.rs`
- Modify: `crates/acad-cmd/src/lib.rs`

**Interfaces:**
- Consumes: everything from Tasks 1–7 (`crate::input_state::*`, `crate::parse::*`, `crate::geometry::*`, `crate::selection::*`, `crate::entity_ops::*`, `crate::report::*`), plus `acad_model::*` types the match arms construct (`Entity`, `Item`, etc.), plus `crate::{Effect, FilesFilter, FilesRequest}` from `lib.rs`.
- Produces: `Editor::submit` (`pub fn`, existing signature `fn submit(&mut self, input: &str) -> Result<Effect, String>`) and `Editor::command` (private, existing signature `fn command(&mut self, command: &str) -> Result<Effect, String>`).

- [ ] **Step 1:** Cut former lines 583–1862 (`submit` through the end of `command`) into a third `impl Editor { ... }` block in `dispatch.rs`. Keep `submit` as `pub fn`; `command` stays private.
- [ ] **Step 2:** Add `mod dispatch;` to `lib.rs`, and the `use crate::{geometry::..., parse::..., selection::..., entity_ops::..., report::...};` lines dispatch.rs needs — resolve each by building and adding exactly what the compiler names, rather than importing every item from every module speculatively.
- [ ] **Step 3: Build, test, gate, commit**, commit message `refactor(cmd): extract command dispatch into dispatch.rs`.

## Task 9: Redistribute the test module

**Files:**
- Create: `crates/acad-cmd/tests/editor.rs`
- Modify: `crates/acad-cmd/src/{input_state,parse,geometry,selection,entity_ops,report}.rs` (each gets its own `#[cfg(test)] mod tests { use super::*; ... }` appended), `crates/acad-cmd/src/lib.rs` (remove the now-fully-redistributed former `mod tests` block, former lines 4202–5847)

**Interfaces:**
- Consumes: N/A (test-only reshuffle)
- Produces: same test functions, same assertions, verbatim — only their file location and, where a test used a private item that's now `pub(crate)` in a different module, their `use` statements change.

- [ ] **Step 1:** For each `#[test] fn` in the original `mod tests` block (former lines 4202–5847), grep its body for calls to any of the now-relocated private free functions (`number(`, `hatch_line_geometry(`, `selection(`, `transform_entity(`, `help_report(`, etc. — the full names list is in the File Map above). A test that calls one or more of these moves into that function's new module's own `#[cfg(test)] mod tests` block with `use super::*;` (if it calls functions from two different new modules, put it in the module of the primary function under test, importing the other via `use crate::other_module::other_fn;`).
- [ ] **Step 2:** Every remaining test — one that only constructs `Editor::new(...)`, calls `.submit(...)`, `.pick_entity_at(...)`, or other `pub fn` methods, and asserts on `Effect`/`.drawing()`/`.prompt()`/`.status()` — moves verbatim into `crates/acad-cmd/tests/editor.rs` as a Cargo integration test, with `use acad_cmd::{Editor, Effect, FilesFilter, FilesRequest};` (or whichever subset each test needs) at the top in place of the old `use super::*;`.
- [ ] **Step 3:** Delete the emptied `mod tests { ... }` block from `lib.rs` entirely.
- [ ] **Step 4: Build.** Run: `cargo build -p acad-cmd --tests 2>&1 | tail -80`. Fix any test that reached for something not `pub`/`pub(crate)` by widening that one item's visibility — don't widen anything not flagged.
- [ ] **Step 5: Test.** Run: `cargo test -p acad-cmd -p acad-app`. Expected: identical pass count to the Task 1 baseline (no test lost, none newly skipped).
- [ ] **Step 6: Gates.** Run `cargo fmt --all --check` and `cargo clippy --workspace --all-targets -- -D warnings`.
- [ ] **Step 7: Commit.**
  ```bash
  git add crates/acad-cmd/src crates/acad-cmd/tests
  git commit -m "refactor(cmd): redistribute lib.rs tests to their new modules"
  ```

## Task 10: Final full-workspace verification

**Files:** none (verification only)

- [ ] **Step 1:** Run: `cargo test --workspace 2>&1 | tail -150`. Expected: same aggregate pass count as the pre-refactor full-suite run recorded in the current handover doc (`docs/HANDOVER-2026-09-30.md` — the run that already passed before this refactor started), 0 failed.
- [ ] **Step 2:** Run: `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D warnings`, `cd formal && lake build && cd ..` — all three must be clean, matching the existing handover gate list.
- [ ] **Step 3:** Run: `cargo doc -p acad-cmd --no-deps 2>&1 | tail -60` and confirm no new warnings about unresolved doc links from moved items (Review Focus item 4).
- [ ] **Step 4:** Run `find crates/acad-cmd/src -name '*.rs' | xargs wc -l | sort -rn` and confirm no single file in the crate exceeds roughly 1,300 lines (the `dispatch.rs` ceiling from the File Map) — this is the plan's own success signal, not a new automated gate.
- [ ] **Step 5: Update `docs/HANDOVER-2026-09-30.md`** (or the then-current handover doc) to note the `acad-cmd` module split and the new file layout, so the next session doesn't rediscover a 5,847-line `lib.rs` that no longer exists.
- [ ] **Step 6: Commit.**
  ```bash
  git add docs/HANDOVER-2026-09-30.md
  git commit -m "docs: record acad-cmd module split in handover"
  ```
