# DIM Geometry Recovery Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Recover and implement the remaining DIM baseline/continue anchors, arrow-size setting, text orientation, and measured text-placement rules, with native fixtures and independently reviewable tests.

**Architecture:** Native observations come first, an executable Lean contract records only those observations second, and Rust implementation/refinement tests follow third. Keep the existing linear primitive output and command API; store any newly required DIM session state explicitly rather than recognizing arbitrary LINE/SOLID/TEXT groups as dimensions. Evidence that does not isolate a rule blocks that rule's implementation and is retained as an unresolved finding.

**Tech Stack:** Rust 2021 (workspace minimum Rust 1.88), the existing QEMU/Session/CGA oracle, AC1.40 DWG and 1983 DXF codecs, Lean 4 through the repository's Lake project, existing SHP font interpreter when supported by evidence.

**Spec:** `docs/HANDOVER-2026-09-30.md` (Known limits items 1 and 5), `docs/superpowers/specs/2026-09-28-autocad-14-rust-design.md` (DIM primitive-output scope), and the corrected Revision 5 findings in `docs/superpowers/plans/2026-09-30-dim-angular-recovery.md`. Read those sources together: the older design and original angular-plan premise have stale claims that the later handover/findings supersede.

## Global Constraints

- “For every partial command, extend the Lean contract only after QEMU has established observable facts, then add Rust refinement tests and an oracle check where the original can be driven.”
- “There is no evidence AutoCAD 1.4 (1983) has angular/radial/diameter dimensioning at all.” Do not add any of those modes or claim their absence is proven.
- Native exports contain ordinary LINE, SOLID, and TEXT records, not a dimension entity. Preserve native primitive order, layer, coordinates, TEXT rotation/value/height, and full SOLID corner order.
- Original floppy and font bytes remain local; all native runs use disposable disk copies. Small newly generated drawing fixtures may be tracked outside ignored `corpus/` with scripts and provenance.
- This plan changes no HATCH semantics, menu control-byte semantics, or SKETCH boundary. DIM code tasks must run sequentially with any HATCH/menu task that edits the same command/app modules.
- No QEMU availability means no new recovery claim. Fixture-only tests can run, but a skipped live test is not native validation.

## Review Focus

- B/C after two successful dimensions must distinguish first, second, projected, and latest origins; a prompt advancing alone is insufficient.
- Invalid/cancelled DIM and intervening ordinary drawing commands must not silently replace the recovered anchor or mutate existing geometry.
- Valid nondefault A values must have measured SOLID/LINE consequences while the setting itself adds no geometry and returns to Command.
- Translated, scaled, mirrored, and oblique inputs must expose origin-relative formulas and native quantization; do not assume affine invariance across snapping.
- Text width, fit threshold, orientation, header text size and decimal precision must be checked through exported geometry, including TEXT rotation, rather than merely entity counts.

---

## Audited starting point (main `1f38904`)

The earlier project **recovered native ABCT prompts**; it did not implement them in Rust. Do not redispatch prompt-recognition research and do not describe Rust ABCT as finished.

| Area | Current source/API | Actual behavior and limit |
|---|---|---|
| Command dispatch | `crates/acad-cmd/src/dispatch.rs:497-534`, `Editor::submit(&mut self, input: &str) -> Result<Effect, String>` | `DimFirstExtension` always calls `point(line)?`; A/B/C/T are not recognized. The linear sequence is first origin → intersection → second origin → text. |
| Input state | `crates/acad-cmd/src/input_state.rs:114-117,279-282` | Only `DimFirstExtension`, `DimIntersection(Point)`, `DimSecondExtension(Point, Point)`, `DimText(Point, Point, Point)` exist. |
| Geometry | `crates/acad-cmd/src/geometry.rs:306-471` | `dimension_geometry(first: Point, intersection: Point, second: Point, text: Option<&str>, configured_text_size: f64, units: Units) -> Result<Vec<Entity>, String>`; fixed arrow `9/64`, half-width `3/128`, coordinate snapping at `1/128`, text height `round(text_size*135)/128`; glyph `1` uses `40/63`, everything else `52/63`; rotation always zero. Narrow fitted rules only. |
| Editor/session | `crates/acad-cmd/src/lib.rs:85-175`, `editor_ops.rs:498` | No DIM style/history field. UndoSnapshot currently saves drawing, last erased items, active shape library. `cancel_command`, `accepts_mouse_point`, `submit_mouse_point` are public. |
| Lean | `formal/AutoCAD/Geometry.lean` | `advance : Phase → Input → Outcome` models only the linear DIM prompts and primitive output; no geometry calculation, style, or prior-dimension state. |
| Rust tests | `crates/acad-cmd/tests/editor.rs:600` | One linear flow test checks seven primitives, selected points, and text; no ABCT or session lifecycle checks. |
| Live oracle | `crates/acad-oracle/tests/commands.rs:49` | Two cases, DIMSHORT `DIM;1,1;5,1;3,2;blank` and DIMLONG `DIM;0,0;0,4;3,4;blank`; LINE tolerance `1e-6`, SOLID `2e-6`, TEXT origin `0.003`; TEXT rotation and layers are not compared. |
| Recovery runner | `crates/acad-oracle/examples/dim-angular-recovery.rs` | Captures each typed line with `Session::capture_editor()`, `cga::Frame::new`, `Frame::to_rgb_640x400`; current candidates stop at B/C prompt or incorrectly submit points after A. Reuse capture method; do not reuse those incomplete geometry scripts. |

Native facts already established: A → `Dimension arrow size:` → a valid `2` returns immediately to `Command:`; an invalid coordinate pair also returns to Command with `*Invalid*`. B/C are rejected without a preceding dimension and with one ask `Second extension line origin:`. T asks whether text should be horizontal, default Y. Their exact geometric effects, lifecycle rules, and T's aligned text placement are new recovery work.

Graph-first discovery omitted current DIM symbols and returned stale `Vm.capture_editor` source; direct source verified the paths/APIs above. Document insufficient-graph fallback once at execution.

## Coordination and budget

Use [the shared roadmap](2026-10-01-recovery-subagent-roadmap.md) for worktree setup, compact task packets, fresh combined reviews, commits, and slice integration. Recovery and geometry workers/reviewers use `gpt-6.1-sol` high; concrete harness/fixtures and Lean changes use Sol medium; mechanical documentation uses Luna low. Escalate a persistent isolated ambiguity to Astra high only after two bounded failed attempts; reserve one Astra high final whole-branch review.

All DIM code tasks run sequentially with menu/HATCH edits to shared files. **One native guest total:** the coordinator grants an explicit probe lease. Session's mutex is process-local; separate processes are not protected. Other workers may prepare matrices or analyze completed artifacts while waiting. Do not bypass the mutex or overlap native tests with recovery.

## Task 1: Recover geometry with a bounded matrix and preserve evidence

**Files:**
- Create: `crates/acad-oracle/examples/dim-geometry-recovery.rs`.
- Create: `crates/acad-oracle/tests/fixtures/dim/manifest.json` and one `.inputs.txt`, `.dwg`, `.dxf` per accepted case.
- Create: `docs/recovery/dim-geometry-2026-10-01.md`.
- Create: `crates/acad-oracle/tests/dim_fixtures.rs` (native fixture integrity only at this task).
- Read/reuse: `crates/acad-oracle/examples/dim-angular-recovery.rs`, `src/session.rs`, `src/qemu.rs`, `src/cga.rs`.

**Interfaces:**
- Existing export: `acad_oracle::generate_visual_pair(system_disk: &Path, samples_disk: Option<&Path>, name: &str, editor_lines: &[&str]) -> Result<acad_oracle::VisualProbe, String>`; returns public `dwg`, `dxf`, `cga: Vec<u8>`.
- Existing per-step capture: `Session::boot_disposable(&Path, Option<&Path>, &[&str])`, `type_line(&str)`, `capture_editor() -> Result<Vec<u8>, String>`, `wait_for_text(&str, Duration)`, `shutdown()`.
- New runner CLI: `cargo run -p acad-oracle --example dim-geometry-recovery -- <anchors|style|text|lifecycle> <output-directory>`; missing image/QEMU is an error and nonzero exit, not a successful skip.
- Evidence manifest fields: `id`, `group`, `inputs` array (blank input remains `""`), `expected_terminal_prompt`, `dwg_file`, `dxf_file`, `prefix_case`, `facts`, `unresolved`, `system_sha256`, `qemu_version`, `source_commit`, `capture_files`; values describe actual observations, never Rust-generated expected geometry.

- [ ] **Step 1: Add fixture integrity tests first.** Use a test that fails until accepted native fixtures exist. Start with a complete probe `DBASE1` and its prior-only `DPRIOR` prefix; no expected coordinates are invented:

```rust
#[test]
fn dim_native_fixture_is_not_a_noop_or_a_codec_disagreement() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/dim");
    let read = |name: &str| std::fs::read(root.join(name)).unwrap();
    let prior = acad_dwg::parse(&read("DPRIOR.dwg")).unwrap();
    let after = acad_dwg::parse(&read("DBASE1.dwg")).unwrap();
    let dxf = acad_dxf::parse(&read("DBASE1.dxf")).unwrap();
    assert!(!prior.items.is_empty(), "prefix did not create a dimension");
    assert!(after.items.len() > prior.items.len(), "B produced no geometry");
    assert_eq!(&after.items[..prior.items.len()], prior.items.as_slice());
    let original_dxf = read("DBASE1.dxf");
    let eof = original_dxf.iter().position(|byte| *byte == 0x1a).unwrap() + 1;
    assert_eq!(acad_dxf::write(&after), original_dxf[..eof], "native codec serialization differs");
    assert!(!dxf.items.is_empty(), "native DXF had no entities");
}
```

Native DXF rounds fields to six decimal places: canonical writer comparison above preserves that serialization separately from Rust-versus-DWG geometry tolerances. Run: `cargo test -p acad-oracle --test dim_fixtures dim_native_fixture_is_not_a_noop_or_a_codec_disagreement -- --nocapture`. Expected failure: missing native fixture. If native rewrites the earlier prefix, preserve that fact and compare its original geometry by decoded fields instead; do not erase the guard to obtain a pass.

- [ ] **Step 2: Implement the runner and a finite case list.** Completed sequences use `generate_visual_pair`; diagnostics stop at the first unexpected prompt and use the proven per-step capture method. Save raw `.cga` and PNGs under the output directory; keep only required small evidence with the accepted fixtures. Preserve whitespace and every blank input:

Follow the already working `generate_visual_pair` example pattern: save returned `dwg`, `dxf`, `cga` with the case ID as basename, convert CGA to a PNG using the old recovery example's `save_png`, and serialize the exact input array with existing serde_json. Runner prints each case before boot, labels failures by ID, refuses output overwrite, and writes `.inputs.txt` with a trailing newline per input; `.lines()` preserves a final blank line when the file has two trailing newlines. Never trim the script file. A missing prerequisite is nonzero exit.

- [ ] **Step 3: Run anchor matrix (9 exports maximum).** Keep the already known native entry/recognition facts. Use asymmetric prior points so origin/endpoint/projected-point hypotheses disagree:

| ID | Inputs | Purpose |
|---|---|---|
| `DPRIOR` | `DIM;1,1;5,1;3,2;blank` | Prior-only prefix; extension origins differ from arrow tips/projected origins. |
| `DBASE1` | prefix + `DIM;B;2,4;blank` | Isolate B's origin and inherited intersection/direction. |
| `DCONT1` | prefix + `DIM;C;2,4;blank` | Distinguish C from B and from both projected endpoints. |
| `DBASE2` | DBASE1 + `DIM;B;4,6;blank` | Identify whether baseline root or latest dimension is reused. |
| `DCONT2` | DCONT1 + `DIM;C;4,6;blank` | Identify chaining to most recent successful dimension. |
| `DBSHIFT` | `DIM;3,2;7,2;5,3;blank;DIM;B;4,5;blank` | Translate +2,+1 while preserving shapes; quantify snapping. |
| `DCSCALE` | `DIM;2,2;10,2;6,4;blank;DIM;C;4,8;blank` | Double coordinates while style remains default; separate geometry and style scale. |
| `DBOBLIQ` | `DIM;1,1;4,3;1,4;blank;DIM;B;2,5;blank` | Non-axis-aligned extension direction; distinguish copied vector from absolute intersection. |
| `DCMIRROR` | `DIM;1,1;5,1;3,0;blank;DIM;C;2,-2;blank` | Negative signed projection and side/order behavior. |

`prefix` denotes DPRIOR's exact five inputs, not an extra command. The shortened sequence after B/C is a candidate based on proven second-origin prompt. Inspect per-step capture if it does not return to Command after the text input; amend only the observed input sequence and record it before continuing exports.

Run: `cargo run -p acad-oracle --example dim-geometry-recovery -- anchors target/dim-recovery/anchors`.

Decode each native output in the runner and print LINE endpoints, all four SOLID vertices, TEXT origin/height/rotation/value, layer and file order. Compare the appended entity suffix against each hypothesis; record which inputs falsify each hypothesis. Do not choose “B first / C last” because later AutoCAD does that.

- [ ] **Step 4: Run arrow matrix (4 exports maximum).** Default geometry is DPRIOR. Execute complete fresh-drawing scripts:

```text
DAR025: DIM;A;0.25;DIM;1,1;5,1;3,2;blank
DAR050: DIM;A;0.5;DIM;1,1;5,1;3,2;blank
DAR200: DIM;A;2;DIM;1,1;5,1;3,2;blank
DARBACK: DIM;A;0.25;DIM;A;0.5;DIM;1,1;5,1;3,2;blank
```

Run: `cargo run -p acad-oracle --example dim-geometry-recovery -- style target/dim-recovery/style`.

A itself creates no entities. Capture/compare SOLID tip/base distance, half-width, extension overshoot, line fit/external-arrow choice, and TEXT changes for all values. Explicitly test whether width/overshoot follow A, remain constant, or are device-quantized. The final DARBACK must agree with the accepted 0.5 case if replacement is observed. A=2 returning to Command is already proven; probe the geometry of the separate DIM, not recognition again. One native export that differs from default is a necessary guard against a silently ignored setting; it does not establish a general scaling formula alone.

- [ ] **Step 5: Run text matrix (9 exports + at most 6 discriminating threshold exports).** All use default arrows and fresh state. These are new geometry cases, not repeated T prompt research:

```text
DT1111: DIM;0,0;0,4;3,4;1111
DT8888: DIM;0,0;0,4;3,4;8888
DTPUNCT: DIM;0,0;0,4;3,4;1.01
DTWIDE: DIM;0,0;0,4;3,4;WWWW
DTNARROW: DIM;0,0;0,4;3,4;iiii
DTYN: DIM;T;N;DIM;1,1;4,3;1,4;1.01
DTYY: DIM;T;N;DIM;T;Y;DIM;1,1;4,3;1,4;1.01
DTDEF: DIM;T;blank;DIM;1,1;4,3;1,4;1.01
DTPREC: UNITS;2;2;DIM;0,0;0,4;3,4;blank
```

Run: `cargo run -p acad-oracle --example dim-geometry-recovery -- text target/dim-recovery/text`.

Use exports to compare numeric/punctuation/custom-text advances and explicit T N/Y/default rotation and origin. DTYN/DTYY deliberately exercise oblique geometry, so rotation-zero cannot accidentally pass. Decimal is selector 2 in the current recovered UNITS flow.

Threshold probes use `DIM;0,0;0,4;<L>,4;1111` and `...;8888`. From the first measured widths, choose one common L where fit predictions differ, then one native quantum below, at, and above a measured branch change. Maximum six additional exports; write exact L values and the origin of those choices in the evidence before running. Do not infer the inclusive/exclusive threshold from just two far-apart spans. Translation tests measure the exported delta with quantization allowance; full scale invariance is not assumed.

Default TXT's glyph interpreter already exists in `crates/acad-render/src/shp.rs`, `Library::text(&str) -> Result<Glyph, ShapeError>`, `Glyph.advance: Point`, `Library.cap_height`. Compare its advances with native placement; do not introduce an `acad-render` dependency into `acad-cmd`. Supporting other loaded fonts requires separate native evidence and a deliberate caller-supplied metric interface; it is outside this default-font matrix if no supported rule is isolated.

- [ ] **Step 6: Capture lifecycle/error probes (6 short sessions maximum).** These answer new history/validation questions. Use per-step frames to avoid submitting text into an unknown state:

1. DPRIOR, then `LINE;8,1;9,1;blank;DIM;B;2,4;blank`: does ordinary geometry replace DIM history?
2. DPRIOR, then `DIM;1,1;1,1;2,4;blank`, then B: capture the degenerate-flow rejection and determine whether the earlier anchor survives. Stop if a point remains requested; return through the observed cancellation path rather than inventing extra lines.
3. DPRIOR, then DIM first origin, cancel with native Escape through `Session::key`, then C: determine cancellation/history behavior.
4. DPRIOR, then `U`, then B: recover anchor/undo interaction; do not require history restoration before observing it.
5. Reopen saved DPRIOR via a disposable Samples drawing using the pattern in `acad_oracle::open_drawing`, then DIM B/C captures in one session: distinguish persisted geometry from in-memory history. Never reconstruct history from arbitrary primitives without proof.
6. One style-history session: `DIM;A;0.5;DIM;A;0,0`, inspect known invalid-return frame, then a separate successful DIM as DPRIOR. Compare its suffix with default/DAR050 to distinguish invalid-A reset from retention. Zero/negative/blank A acceptance is not recovered by this case and remains unclaimed. Local Rust nonfinite `NaN`/`inf` rejection is domain validation, not a native fact. Cancellation/U/reopen style changes remain unclaimed unless a separate bounded experiment replaces another case.

Run: `cargo run -p acad-oracle --example dim-geometry-recovery -- lifecycle target/dim-recovery/lifecycle`.

Total cap: 28 native exports/sessions plus at most six threshold exports, counting reopen/errors as sessions. Execute the four groups independently and stop a group at its first unresolved assumption. Do not spend the entire cap when a smaller matrix discriminates the rule. Evidence review may reject unsupported rules while accepting the independent A, T, or B/C slice.

- [ ] **Step 7: Publish and review evidence.** Move accepted DWG/DXF and exact scripts into `tests/fixtures/dim`; manifest records native-only counts/kinds, geometry facts and exclusions. Recovery document contains image/output `shasum -a 256`, QEMU version, source commit, complete scripts, prompt transcriptions, frame/PNG references, quantization/tolerance explanation and competing predictions. No native coordinates come from Rust output. Run fixture-integrity tests, receive fresh Sol high review, and commit only this evidence/harness deliverable.

**Handoff/stop gate:** Task 2 receives the reviewed rule table and fixture IDs. If an anchor, style rule, threshold, or lifecycle transition remains ambiguous, do not add its semantics to Lean/Rust. Continue independent proven rules; report the exact missing experiment. Never widen tolerances, reduce comparisons to counts, or use Rust-built expected fixtures to force acceptance.

## Task 2: Extend the executable contract from accepted observations

**Files:** Modify `formal/AutoCAD/Geometry.lean`, `formal/README.md`; test through `formal/Main.lean`'s existing imports and `lake build`.

**Interfaces:** Keep existing `advance : Phase → Input → Outcome` callers stable. Add explicit A/T/B/C constructors and phases only for accepted prompt paths. Add `dimAdvance` carrying `hasPriorDimension : Bool` if required to distinguish B/C validity; existing `advance` remains the linear/backward-compatible wrapper. A geometry contract consumes native-generated fixture constants; it does not prove ACAD.EXE source equivalence.

- [ ] **Step 1: Add executable examples for known entry/setting contracts.** Introduce constructors `.arrowSetting`, `.textOrientation`, `.baseline`, `.continueDimension`, `.validArrow`, `.invalidArrow`, `.yes`, `.no`, `.blank`; phases `.dimArrowSize`, `.dimHorizontalText`; outcomes `.command`, `.invalidCommand`; prompts `.dimensionArrowSize`, `.horizontalDimensionText`. Test examples:

```lean
example : advance .dimFirstExtension .arrowSetting = .prompt .dimensionArrowSize := by decide
example : advance .dimArrowSize .validArrow = .command := by decide
example : advance .dimArrowSize .invalidArrow = .invalidCommand := by decide
example : advance .dimFirstExtension .textOrientation = .prompt .horizontalDimensionText := by decide
example : advance .dimHorizontalText .blank = .command := by decide
example : dimAdvance false .dimFirstExtension .baseline = .invalidCommand := by decide
example : dimAdvance true .dimFirstExtension .baseline = .prompt .secondExtensionOrigin := by decide
example : dimAdvance true .dimFirstExtension .continueDimension = .prompt .secondExtensionOrigin := by decide
```

Run: `cd formal && lake build`. Expected FAIL until constructors and transition cases exist.

- [ ] **Step 2: Implement those transitions and encode reviewed geometry facts.** Preserve all HATCH/SKETCH examples. Use rational native fixture coordinates when exactly representable and explicit quantization functions only if discriminated by Task 1. Geometry facts can be fixture relations (selected reused origin, setting replacement, text orientation) without falsely presenting a universal recovered formula. Name facts by fixture ID and link recovery document. If baseline-root versus latest-anchor is unresolved, keep the B/C prompt contract and omit geometry claims rather than choosing an anchor.

- [ ] **Step 3: Run `cd formal && lake build`, review README support boundaries, receive fresh Sol medium review and commit the executable contract.** No HATCH/SKETCH example changes.

**Handoff:** Task 3 receives final constructor/phase names and contract examples; Tasks 4/5 receive only accepted geometry predicates and fixtures. No changing Rust behavior before this gate passes for that rule.

## Task 3: Implement single-shot A/T settings and their measured geometry

**Files:** Modify `crates/acad-cmd/src/{lib.rs,input_state.rs,dispatch.rs,geometry.rs}`, `crates/acad-cmd/tests/editor.rs`, `crates/acad-oracle/tests/dim_fixtures.rs`; create `crates/acad-oracle/tests/support/dim.rs` for the shared comparator below. Modify `editor_ops.rs` only if accepted lifecycle facts require style in undo snapshots.

**Interfaces:** Add private `DimStyle { arrow_size: f64, horizontal_text: bool }` to Editor, with native-observed defaults. Add `InputState::DimArrowSize`, `InputState::DimHorizontalText`. Extend `dimension_geometry` to receive a `DimStyle` value rather than reading Editor state. `Editor::submit`, `prompt`, `cancel_command`, and mouse APIs retain their public signatures. No persistent DWG header field is invented.

- [ ] **Step 1: Add settings tests that fail today.**

```rust
#[test]
fn dim_settings_are_single_shot_and_add_no_geometry() {
    for (option, answer, prompt) in [("A","2","DIM: dimension arrow size"),
        ("T","N","DIM: horizontal text? <Y>"),("T","Y","DIM: horizontal text? <Y>"),
        ("T","","DIM: horizontal text? <Y>")] {
        let mut editor=Editor::default(); let before=editor.drawing().clone();
        editor.submit("DIM").unwrap(); editor.submit(option).unwrap();
        assert_eq!(editor.prompt(),prompt); assert!(!editor.accepts_mouse_point());
        editor.submit(answer).unwrap(); assert_eq!(editor.prompt(),"Command");
        assert_eq!(editor.drawing(),&before);
    }
}
#[test]
fn dim_invalid_arrow_pair_returns_to_command_without_edits() {
    let mut editor=Editor::default(); let before=editor.drawing().clone();
    editor.submit("DIM").unwrap(); editor.submit("A").unwrap();
    assert!(editor.submit("0,0").is_err()); assert_eq!(editor.prompt(),"Command");
    assert_eq!(editor.drawing(),&before);
}
```

Run: `cargo test -p acad-cmd --test editor dim_ -- --nocapture`; expected FAIL on current point parsing. Add accepted A scalar-domain cases from lifecycle findings; choose post-invalid style retention/reset only from the style-history fixture. Rust nonfinite rejection must not append geometry or corrupt style. Native behavior for unaccepted Y/N values must come from evidence, not generic toggle convenience.

- [ ] **Step 2: Implement states, defaults, and measured style fields.** A parses a scalar then unconditionally exits its subcommand, including invalid result; avoid early `?` that leaves `DimArrowSize` active. T changes orientation then returns Command. Apply only Task 1's measured arrow-length/width/extension/fit relations. Add every setting-state `prompt()` match and ensure those states are excluded from point/mouse selection handling. `cancel_command()` discards incomplete settings; successful-style effects across cancellation, U, and reopen remain unclaimed unless observed.

- [ ] **Step 3: Add full fixture comparisons for DAR025/DAR050/DAR200/DARBACK and DTYN/DTYY/DTDEF.** Use Task 5's comparison schema below immediately: layers, all points, TEXT height/value/rotation, exact entity count and order. Run both fixture-only tests and focused live exports; default geometry is an explicit regression guard.

Run:

```bash
cargo test -p acad-cmd --test editor dim_ -- --nocapture
cargo test -p acad-oracle --test dim_fixtures native_fixture -- --nocapture
cargo test -p acad-oracle original_dim_exports_primitive_geometry_matched_by_rust -- --nocapture --test-threads=1
```

Expected PASS for accepted A/T fixtures and unchanged two original cases. Text placement mismatch that requires Task 5 remains a named red test on the branch; do not accept Task 3's geometry deliverable until the scope it claims passes. If T placement/width cannot be separated from Task 5, combine those code edits into Task 5 rather than accepting a false T geometry claim.

- [ ] **Step 4: Fresh Sol high review; commit the accepted A/T state/style deliverable.** Include geometry, command tests and fixture comparisons in the same commit.

**Handoff:** Exact DimStyle definition/defaults, geometry signature, accepted size-domain behavior, orientation fixtures, and any explicitly moved text-placement edits go to Task 4/5. No DIM metadata serialized into drawings.

## Task 4: Implement B/C reused anchors and recovered session lifecycle

**Files:** Modify `crates/acad-cmd/src/{lib.rs,input_state.rs,dispatch.rs,geometry.rs}`, `editor_ops.rs` if undo findings require it; test `crates/acad-cmd/tests/editor.rs`, `crates/acad-oracle/tests/dim_fixtures.rs`.

**Interfaces:** Add private `DimContext { first: Point, intersection: Point, second: Point }` and `last_dimension: Option<DimContext>` to Editor. These fields hold input geometry from the last successful DIM only; the accepted evidence determines the B/C transform and whether chained dimensions update a root separately. Use a separate root field only if DBASE2 discriminates that need. A/B/C/T are recognized in `DimFirstExtension` while point input preserves the existing linear sequence.

- [ ] **Step 1: Add no-prior and native-fixture tests.**

```rust
#[test]
fn dim_no_prior_is_atomic_and_successful_continue_accepts_mouse() {
    for option in ["B","C"] {
        let mut editor=Editor::default(); let before=editor.drawing().clone();
        editor.submit("DIM").unwrap(); assert!(editor.submit(option).is_err());
        assert_eq!(editor.prompt(),"Command"); assert_eq!(editor.drawing(),&before);
    }
    let mut editor=Editor::default();
    for input in ["DIM","1,1","5,1","3,2","","DIM","C"] { editor.submit(input).unwrap(); }
    assert_eq!(editor.prompt(),"DIM: second extension line origin"); assert!(editor.accepts_mouse_point());
    editor.submit_mouse_point(Point{x:2.0,y:4.0}).unwrap(); assert_eq!(editor.prompt(),"DIM: dimension text");
}
```

Run: `cargo test -p acad-cmd --test editor dim_ -- --nocapture`; expected FAIL until B/C states exist. Add replay-based full native fixture tests DBASE1/DCONT1/DBASE2/DCONT2/DBSHIFT/DCSCALE/DBOBLIQ/DCMIRROR with changed appended primitives and the native-tested reused coordinates. Equality to actual native output is the expectation; no modern-AutoCAD anchor formula is preselected.

- [ ] **Step 2: Implement the accepted anchor construction in the shortened flow.** B/C consume `last_dimension` and directly enter second-origin input with the inherited first/intersection values recovered by Task 1. At the text step, generate every entity successfully before saving undo or updating history. Degenerate/nonfinite geometry errors append nothing and preserve accepted prior history; match the native final prompt where recovered. For plain DIM success, record original user geometry separately from snapped exported arrow vertices.

- [ ] **Step 3: Add lifecycle tests derived from accepted sessions.** Replay exact native scripts for an intervening LINE, cancellation, rejection, U, and reopened drawing history. In Rust, `cancel_command()` needs a direct test preserving the drawing, pending-point reset, and recovered prior history. Session-only histories initialize to None in `Editor::new` if native reopen resets them. If U restores history, put it in `UndoSnapshot`; if native does not restore history, retain the observed distinction. Neither persistence nor undo behavior is inferred from the fact that the drawing contains dimension-like primitives.

For geometry translation/scale use native fixtures, not exact unchecked affine formulas; compare primitives with the evidence's snapping bounds. Add a negative-sign case, not just positive coordinate magnitudes. Existing repeat/selection/undo behavior remains unchanged except explicit DIM snapshot fields.

- [ ] **Step 4: Run `cargo test -p acad-cmd --test editor dim_ -- --nocapture`, `cargo test -p acad-oracle --test dim_fixtures native_fixture -- --nocapture`, and `cargo test -p acad-oracle original_dim_exports_primitive_geometry_matched_by_rust -- --nocapture --test-threads=1`.** Require actual focused native runs under the lease. Fresh Sol high reviewer compares DBASE1/DCONT1 and both second-chain fixtures; entity counts alone do not pass. Commit accepted anchor/lifecycle behavior.

**Handoff:** Record exact context fields, update timing, cancellation/error/undo/reopen rules, and all fixtures. Do not claim lifecycle rules that the short matrix did not isolate.

## Task 5: Replace fitted text heuristics with measured placement rules

**Files:** Modify `crates/acad-cmd/src/geometry.rs`; tests `crates/acad-cmd/tests/editor.rs`, `crates/acad-oracle/tests/dim_fixtures.rs`, `crates/acad-oracle/tests/commands.rs`, shared helper `crates/acad-oracle/tests/support/dim.rs`. If evidence requires font-specific caller metrics, make that a separately reviewed interface change across `lib.rs`, `crates/acad-app/src/main.rs`, and existing `acad-render` font setup after graph discovery; do not silently add renderer dependency or font I/O to the command crate.

**Interfaces:** Preserve the Task 3 geometry/style API. For the observed default TXT font, introduce focused geometry helpers for measured text advance, origin/rotation, and fit decision; names and equations are chosen from the accepted evidence and remain private. An unmeasured glyph is not assigned `52/63` merely to make it render. Full loaded-font parity remains excluded unless separately evidenced.

- [ ] **Step 1: Add fixture replay tests for the 9 text cases and accepted threshold cases.** These compare real native fields, and fail against the current all-but-1-equal-width and horizontal-text assumptions. Task 3 creates the shared `pub fn compare_dim_items(actual: &[Item], native: &[Item])` in `tests/support/dim.rs`; both test binaries use `#[path="support/dim.rs"] mod dim_support;` and `use dim_support::compare_dim_items;`. It preserves layer and rotation that the old live test ignores:

```rust
pub fn compare_dim_items(actual: &[acad_model::Item], native: &[acad_model::Item]) {
    use acad_model::{Entity, Item, Point};
    assert_eq!(actual.len(), native.len()); assert!(!native.is_empty());
    let close = |a:f64,b:f64,t:f64| { assert!(a.is_finite() && b.is_finite()); assert!((a-b).abs()<=t,"{a} != {b}"); };
    let point = |a:Point,b:Point,t:f64| { close(a.x,b.x,t); close(a.y,b.y,t); };
    for (index,(a,b)) in actual.iter().zip(native).enumerate() {
        let (Item::Entity(Entity::OnLayer{layer:al,entity:a}),Item::Entity(Entity::OnLayer{layer:bl,entity:b}))=(a,b)
            else { panic!("item {index}: non-layered primitive"); };
        assert_eq!(al,bl);
        match (a.as_ref(),b.as_ref()) {
            (Entity::Line{start:a,end:b},Entity::Line{start:c,end:d}) => { point(*a,*c,1e-6); point(*b,*d,1e-6); }
            (Entity::Solid{p1:a,p2:b,p3:c,p4:d},Entity::Solid{p1:e,p2:f,p3:g,p4:h}) => {
                for (a,b) in [(*a,*e),(*b,*f),(*c,*g),(*d,*h)] { point(a,b,2e-6); }
            }
            (Entity::Text{origin:a,height:ah,rotation_deg:ar,value:av},Entity::Text{origin:b,height:bh,rotation_deg:br,value:bv}) => {
                point(*a,*b,0.003); close(*ah,*bh,1e-6); close(*ar,*br,1e-6); assert_eq!(av,bv);
            }
            _ => panic!("item {index}: primitive mismatch {a:?} / {b:?}"),
        }
    }
}
```

The `0.003` origin tolerance is the inherited existing bound, not permission to increase it. The evidence should explain its cause and tighten it when native precision supports doing so. Rotation equality uses actual native convention; no modulo-equivalence normalization hides incorrect stored angles.

Fixture replay uses the exact native `.inputs.txt`, including final blank text input:

```rust
fn replay_fixture(root: &std::path::Path, id: &str) {
    let inputs=std::fs::read_to_string(root.join(format!("{id}.inputs.txt"))).unwrap();
    let native=acad_dwg::parse(&std::fs::read(root.join(format!("{id}.dwg"))).unwrap()).unwrap();
    let mut rust=acad_cmd::Editor::default();
    for input in inputs.lines() { rust.submit(input).unwrap(); }
    assert_eq!(rust.prompt(),"Command","{id}: unfinished script");
    compare_dim_items(&rust.drawing().items,&native.items);
}
```

Special error/lifecycle fixtures use manifest-declared expected error steps; never `.unwrap()` every submission in an error script or discard all errors indiscriminately. Name every deterministic integrity/replay test with `native_fixture` (for example `native_fixture_text_matrix`), and every live test with `original_dim_`. Enumerate fixture IDs from the reviewed manifest; require accepted nonempty ID lists and primitive kinds. The fixture-only filter must execute no QEMU guests.

Run: `cargo test -p acad-oracle --test dim_fixtures native_fixture -- --nocapture`; expected FAIL specifically on new text/threshold fixtures before helper changes.

- [ ] **Step 2: Implement the recovered formulas and measured font advances.** Replace broad “every other character” approximation only with accepted default-font measurements or a proven glyph interpreter integration. Cover punctuation and alphabetic differences tested in the matrix. Respect configured text-size quantization, decimal precision, text-aligned rotation and coordinate placement, negative direction, inside/outside arrow ordering, and the discriminated threshold inequality. Document each equation's evidence cases beside the helper rather than deriving it from a desired Rust image.

If Task 1 leaves loaded-font effects or arbitrary glyphs unisolated, retain a precise documented support boundary and stop new semantics for those cases. A fixture lookup keyed by the full submitted script is not an implementation of the geometry rule.

- [ ] **Step 3: Strengthen existing live differential and add focused live cases.** Reuse `compare_dim_items` for the old DIMSHORT/DIMLONG oracle rather than keeping its rotation/layer blind spots. Add live tests `original_dim_baseline_continue_geometry_matches_fixtures` and `original_dim_arrow_and_text_geometry_matches_fixtures` in `dim_fixtures.rs`; for DPRIOR/DBASE1/DCONT1/DAR025/DAR200/DTYN/DTWIDE run `generate_pair` and compare both recorded native fixtures and Rust replay. Missing QEMU keeps ordinary repository skip convention, but completion requires logs proving these selected cases actually ran.

Run:

```bash
cargo test -p acad-cmd --test editor dim_ -- --nocapture
cargo test -p acad-oracle --test dim_fixtures native_fixture -- --nocapture
cargo test -p acad-oracle original_dim_ -- --nocapture --test-threads=1
```

Expected PASS for native fixture replay and live cases; assertion messages show case ID, primitive index, field/value. Add meaningful nonfinite/degenerate atomic rejection tests: set `editor.drawing_mut().header.text_size` to `f64::NAN` or `f64::INFINITY`, submit the four DPRIOR point/text inputs after DIM, require an error with unchanged items and preserved context; include finite `0.4` as a local configured-size case. These are Rust domain checks, not native font/size parity. Use the private geometry API for inputs public dispatch cannot express. Do not require fabricated native geometry for a nonfinite Rust input.

- [ ] **Step 4: Fresh Sol high review; commit measured text placement and fit rules with the strengthened comparison tests.**

**Handoff:** Accepted metrics/equations, remaining unsupported fonts/glyphs, exact threshold fixtures, comparison tolerances and their evidence, and live-run availability logs.

## Task 6: Integrate and document the accepted slice

**Files:** `docs/HANDOVER-2026-09-30.md`, `docs/superpowers/specs/2026-09-28-autocad-14-rust-design.md`, `docs/oracle-qemu.md`, `formal/README.md`, `README.md` where DIM claims changed; this plan's execution checkboxes.

**Interfaces:** Documentation consumes accepted task handoffs and distinguishes native facts, implemented cases, unresolved semantics and fixture versus live verification.

- [ ] **Step 1: Refresh support claims with exact case IDs.** Record A/T/B/C geometry and history/undo/reopen scope only where accepted. Remove stale angular-mode premise. Preserve no-evidence status of angular/radial/diameter and unmeasured loaded-font behavior.
- [ ] **Step 2: Run the shared roadmap's integration checks once after code changes:** `cargo fmt --all --check`, `cargo test --workspace --exclude acad-oracle -- --test-threads=1`, `cargo clippy --workspace --all-targets -- -D warnings`, `cd formal && lake build`; under native lease run `cargo test -p acad-oracle --test dim_fixtures native_fixture -- --nocapture` and `cargo test -p acad-oracle original_dim_ -- --nocapture --test-threads=1`. Record native execution versus skips. Do not repeat slow native checks for documentation-only edits.
- [ ] **Step 3: One fresh Astra high whole-branch review.** Provide the spec, accepted evidence, final diff, test logs and manifest. Reviewer checks native→Lean→Rust ordering, asymmetric/chained anchors, state/errors/translation/scale, exact primitive fields and no-op guards; correct findings and repeat only affected checks.
- [ ] **Step 4: Commit documentation and deliver integration handoff.** Pass commit, shared files, new Editor/UndoSnapshot/InputState fields, tests actually run, evidence links and unresolved rules to the next menu/HATCH owner. Shared-file integration remains sequential.

## Execution stop rules and plan self-review

Planning performs no QEMU run, code edit, or commit. During execution a blocked native geometry question is a recovery boundary, not permission to use a weaker test. Keep established ABCT prompts and independent accepted rules progressing; return competing predictions and the smallest missing experiment before implementation of the uncertain rule.

Coverage audit: B/C first/second/latest/projected anchors are discriminated by asymmetric and chained fixtures; A settings are isolated from separate drawing creation and have nondefault shape guards; T includes known prompt facts and new oblique rotation; text matrix covers glyph classes, configured size safety, precision, and threshold; lifecycle matrix covers cancel/error/intervening commands/undo/reopen; fixture/live checks cover layer/order/SOLID corners/LINE/TEXT fields. Angular/radial/diameter and unmeasured loaded-font rules remain explicit exclusions. No expected native anchor or font equation is fabricated in this plan.

Execution uses subagent-driven delivery if the user confirms this draft captures the requested scope; that execution method is already the requested token-effective approach. No additional choice of method is required. Read the subagent-driven-development skill before execution, use the saved plan as the compact task packet, and wait for the planning review before creating the execution worktree.
