# Whole-frame render budget (RB2)

Status: RB2 candidate. RB1 bounded each owner (top-level entity or REPEAT
group) before it expands. This document adds the aggregate bound across all
owners of one frame and fixes how the frame, API and MCP report a frame the
budget cut short. It is a native Rust policy. AutoCAD 1.4 had no comparable
limit we can recover: its drawings were bounded by 640 KB of memory, not by a
renderer budget.

## Problem

Per-owner limits do not add up to a per-frame limit. A drawing with 65,535
REPEAT owners, each just inside the 100,000-generated-record preflight
(250 x 199 cells of one POINT), passed every owner check. The baseline frame
then generated about 6.5 billion primitives:

| Pathological drawing (65,535 x 49,750-cell POINT REPEAT owners) | Flatten | Rasterize 800x552 | Primitives held |
| --- | --- | --- | --- |
| Baseline, debug, 1 owner (measured) | 15.9 ms | 1.92 s | 99,500 |
| Baseline, debug, 5 owners (measured) | 43 ms | 9.74 s | 497,500 |
| Baseline, debug, 20 owners (measured) | 249 ms | 46.5 s | 1,990,000 |
| Baseline, release, 5 owners (measured) | 6.7 ms | 354 ms | 497,500 |
| Baseline, all 65,535 owners (extrapolated) | | about 42 h debug, about 78 min release | about 6.5e9 (hundreds of GB, OOM first) |
| RB2, debug, all 65,535 owners (`frame_budget.rs`) | 11.6 ms | 1.88 s | 99,500, stopped at item 2 |
| RB2, debug, `Session::frame` 800x600 (`render_budget_tests.rs`) | 0.23 s for the whole frame | | stopped at item 2 |
| RB2, release, all 65,535 owners (`frame_budget.rs`) | 1.6-1.9 ms | 71-75 ms | 99,500, stopped at item 2 |

The same gap existed for thousands of INSERTs of a heavy block, for many
large TEXT values, and for hit testing. Each of these is a regression test now.

## Contract

### Unit and limit

- `acad_render::FRAME_WORK_LIMIT = 1_000_000` work units per frame.
- One work unit is one of:
  - one stored-record visit: the render walk, hidden-owner LOAD replay, or the
    per-owner preflight;
  - one executed SHP instruction for TEXT and SHAPE, including the
    instructions of a program that then fails (`Library::text_counted` /
    `shape_counted`);
  - one REPEAT cell, so a lattice whose base draws nothing (no members, or
    only erased members) is never free;
  - one record that the visibility gate inspects beyond the visit itself:
    a REPEAT subtree, or layer wrappers;
  - 16 bytes of a drawing-supplied name hashed or compared at a lookup:
    INSERT block names (walk, hidden replay, preflight), the current font
    name for TEXT, LOAD names, and each shape-library name SHAPE scans. Names
    under 16 bytes cost nothing extra. In the preflight this also spends from
    the per-owner 100,000 budget, so an owner of huge names is refused before
    it hashes them;
  - one vertex emitted, copied into a REPEAT cell, or re-transformed by an
    INSERT level. Each primitive also costs `PRIMITIVE_SETUP_UNITS = 4`.
- Why this unit: measured rasterization costs about 16 µs per primitive plus
  about 3 µs per vertex in debug. With the 4-unit setup charge, that is about
  3.2 µs per unit for both POINT crosses (6 units per stroke) and circles
  (95 units per stroke). Release is about 30x faster. So 1,000,000 units means
  a debug raster of about 3 s and a release raster of about 0.1 s.
- The heaviest retained corpus frame, DISC.BAK, spends about 88,000 units.
  That is about 11x headroom. `shp_corpus.rs` requires every corpus drawing to
  stay below 10% of the limit.

### Enforcement

- Charges happen before allocation where the size is known. Every REPEAT is
  charged `cells x (units(base) + 1)` before any cell copy is made. When the
  base draws nothing, the cell loop is skipped after the charge. Leaf output is
  charged as it is produced and dropped if the charge fails. A refused charge
  latches `FrameBudget::exhausted`.
- The frame stops at an owner boundary, so the result is deterministic:
  - the primitives of the owner that ran out are discarded;
  - every earlier owner is complete;
  - nothing from that owner onwards is drawn.
- `RenderOutput::budget_stop` returns `BudgetStop`, which records the limit,
  `first_skipped_item` (the one-based drawing item number) and
  `skipped_owners` (the Entity/REPEAT owners from that item to the end).
- Worst case, the frame does at most `FRAME_WORK_LIMIT` units of work, plus
  one owner preflight (at most 100,000 records), plus one leaf's output (at
  most 100,000 SHP instructions, or one arc's 91 vertices). Diagnostic
  output is bounded separately (see Diagnostic volume). The primitives
  vector never holds more than `FRAME_WORK_LIMIT` units.
  Two costs sit outside the charge but stay bounded: each pass builds the
  block-name index once (linear in drawing items; twice per frame when a
  selection is highlighted), and an owner's visibility scan runs before it
  is charged, so it can overshoot by one owner's stored subtree, which the
  per-owner preflight already limits.
- Per-owner rules are unchanged:
  - the RB1 preflight: 100,000 generated records and stored depth 256;
  - `MAX_INSERT_DEPTH` 16;
  - 100,000 SHP instructions per TEXT or SHAPE;
  - 100,000 hidden LOAD replay visits per owner.
  An owner over its own limit is still skipped with
  `Owner N exceeds render work budget`, and the frame continues.
- LOAD context: owners before the stop keep their ordered LOAD effects. No
  later owner is drawn, so no later text can render with stale font context.

### Selection highlight

- `Session::frame` spends one `FrameBudget` across the drawing pass and then
  the selection-highlight pass
  (`flatten_with_budget` / `flatten_selected_with_budget`).
- When the highlight pass runs out, no highlight is drawn at all, never a
  partial one. The pass reports
  `Selection highlight exceeds frame render budget of 1000000 work units; highlight omitted`.
  This matches RB1: an oversized selected owner also emits no partial
  highlight.
- Highlight diagnostics are now merged (deduplicated) into the frame's
  diagnostics. They used to be dropped.

### Lookups

- Every render pass builds `Drawing::block_index()` once (O(items)) and
  resolves every INSERT through it. That covers the walk, hidden LOAD replay
  and preflight, and also `text_font_at`. `Drawing::block` scans every item,
  so the old per-INSERT cost was O(items). The index keeps `Drawing::block`'s
  semantics exactly: the first definition wins and names are case-sensitive.
  A model test checks this, including duplicate definitions.
- Pick, window and ZOOM E/A bounds resolve INSERTs through a per-pass
  `visible_bounds::Scene` that holds the same index.
- The pick walks layer wrappers before its visibility gate, and charges them.
- Shape-library stack: a LOAD of a shape library removes any earlier entry
  that resolves to the same library, then pushes the new name on top. The
  SHAPE search order, its result and the reported library name are
  unchanged, and the stack is bounded by the number of supplied libraries,
  not by the number of LOADs.
- The layer colour table and OFF set are BTreeMap/BTreeSet lookups keyed by
  `u8`, which is O(1). Library lookups are BTreeMap lookups, charged by name
  length as above.
- Command-time lookups are not per-frame and are unchanged:
  - `acad-dxf` parse;
  - the export context;
  - the TEXT command context (`acad-cmd/src/text/context.rs`).

### Diagnostic volume

- Each render pass keeps at most 64 distinct diagnostics. Deduplication uses a
  hash set. Any further distinct messages are only counted and summarized as
  `... and M more render diagnostic(s) not listed`.
- The first message of each incompleteness kind is kept even when the cap is
  full: one LOAD/recursion context failure and one per-owner skip. So a
  status of "(see diagnostics)" always has its reason in the list.
- The budget-stop message bypasses the cap and is always the last entry.
- Drawing-supplied names quoted in a diagnostic are shown unchanged up to 40
  characters. Longer names are cut to their first 40 characters plus `...`.
  This applies to INSERT block names and to font and shape-library names.
- A TEXT value quoted in a diagnostic is cut to its first 40 characters, with
  `...` appended. Diagnostic memory and time per frame are therefore bounded,
  whatever the number of failing owners or the length of their text.

### Reporting and visible degradation

- Drawing-pass diagnostic, exact text:
  `Frame render budget of 1000000 work units exceeded: drawing stopped before item K; N owner(s) not drawn`.
- `RenderOutput::incomplete` is true when any owner the pass should draw is
  missing. That covers three cases:
  - a budget stop;
  - a LOAD or block-recursion context failure that stopped the owner loop
    (RB1 behaviour);
  - an owner skipped by its per-owner budget.
  Missing fonts, shapes or blocks are content diagnostics only. They are
  reported, but they do not make the frame incomplete.
- `Frame::complete` and the API/MCP `frame` result field `"complete"` are
  `false` whenever either pass (drawing or highlight) is incomplete. The
  `diagnostics` array then contains the reason. A cut-short frame is never
  reported as complete.
- On screen, the drawing canvas gets a 3-pixel amber border (`0xFF8000`). It
  is drawn last, over the menu panel and the crosshair. The command-area
  status line is prefixed with
  one of:
  - `RENDER BUDGET: N owner(s) from item K not drawn`
  - `RENDER BUDGET: selection highlight omitted`
  - `RENDER INCOMPLETE: some owners not drawn (see diagnostics)`, for a
    context failure or a per-owner skip
  - `RENDER INCOMPLETE: selection highlight incomplete (see diagnostics)`
- The GUI prints each new diagnostic once to stderr (`render: ...`), the same
  as other render diagnostics.

### Other whole-drawing traversals

| Traversal | Bound |
| --- | --- |
| Hit testing (pick, BREAK pick) | `MAX_DRAWING_HIT_TEST_VISITS = 1_000_000` stored-record visits per pick, across all owners, on top of the unchanged 100,000-member per-owner limit. On exhaustion the whole pick fails with `selection exceeds the drawing hit-test work budget; select objects by number`. It never returns a hit from a partial scan. `Editor::try_pick_entity_at` returns that error. `pick_entity_at` keeps its `Option` signature and returns `None`. Interactive picks (`pick_selection_at`, BREAK) put the error in the status line and leave the prompt as it was. |
| Window selection | The same 1,000,000-visit budget across all owners' compact bounds. On exhaustion the window fails with the same message, instead of selecting a partial set. |
| ZOOM E/A visible bounds (`drawing_bounds`) | Already one shared budget: 100,000 stored visits across all owners. REPEAT bounds are compact and do not expand cells. Navigation fails with an error instead of using partial bounds. Unchanged. |
| GRID | At most 65,536 dots per frame, with a coarser displayed sublattice. Unchanged. |
| AXIS ticks | At most width + height ticks per frame. Unchanged. |
| SKETCH preview | Bounded by the recorded SKETCH segment count, which the user creates. Unchanged. |
| `text_font_at` (TEXT/CHANGE font lookup) | Runs at command time, not per frame. The 100,000-visit per-owner LOAD replay bounds it, and it is not aggregated. This is a documented limitation. |

## Verification

- `crates/acad-render/tests/frame_budget.rs` covers:
  - the 65,535-owner pathological frame, with timing printed, a 30-second
    load-tolerant bound, and the memory bound checked;
  - 5,000 heavy INSERTs;
  - SHP instruction charging with zero strokes;
  - hidden-owner and preflight charging;
  - that the highlight pass shares the budget and is never partial;
  - that the stop is deterministic;
  - review repair 1:
    - empty-member and erased-only 65,535-owner lattices: stop at item 11;
      about 0.06 ms release, about 0.4 ms debug (before the repair, 14-15.6 s
      release with no stop);
    - 8,000 failing 40,000-character TEXT owners: charged, stop at item 10,
      at most 66 short diagnostics; 2 ms release, 42 ms debug (before the
      repair, 29.8 s);
    - 65,535 per-owner-rejected owners: 65 diagnostics; 2.9 ms release
      (before the repair, 2.1 s);
    - a context failure, which marks the pass incomplete;
  - review repair 2:
    - h2, block lookups: 190 owners × 2,000 nested INSERTs of a block
      defined after 65,335 filler records;
    - h3, the shape stack: 32,767 LOADs followed by 32,767 SHAPEs;
    - unchanged SHAPE search order;
    - h1, reasons kept outside the cap: 64 missing blocks followed by a
      context failure or a per-owner skip;
    - long block names: charged, and shortened in diagnostics;
  - the acad-cmd `hit_tests_resolve_blocks_through_a_per_pass_index` test
    covers the pick and window path for h2.
- `crates/acad-app/src/session/render_budget_tests.rs` covers
  `Session::frame`: the pathological frame bound, `complete`, the border and
  status indicator, the API `frame` JSON, and `complete: false` with the
  border for a context-failure frame.
- `crates/acad-cmd/src/selection.rs` tests cover the aggregate pick and window
  budgets.
- `shp_corpus.rs` checks corpus headroom. `tools/check_acad_corpus_api.py`
  checks that all 25 corpus inputs still render with no diagnostics.
