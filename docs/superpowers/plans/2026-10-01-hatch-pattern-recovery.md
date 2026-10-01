# HATCH Pattern Recovery Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Recover native HATCH pattern names and primitives, then implement one useful additional pattern with native differential coverage, retaining the existing LINE cases.

**Architecture:** Native DWG/DXF exports establish the supported names, geometry, ordering, and phase before any additional geometry is shipped. Extend the executable Lean contract for the captured cases, then refine it in Rust through the current command state machine and anonymous block/INSERT representation. NET is the first candidate, subject to native acceptance and primitive recovery; this plan does not promise complete pattern parity.

**Tech Stack:** Rust 2021; existing `acad-oracle` QEMU harness, `acad-dwg`, `acad-dxf`, `acad-model`, `acad-cmd`; Lean/Lake.

**Spec:** `docs/superpowers/specs/2026-09-28-autocad-14-rust-design.md` §3, §9 and the observed command slice; `docs/HANDOVER-2026-09-30.md` item 2; `docs/oracle-qemu.md`; `formal/README.md`.

**Execution workflow:** [Recovery subagent roadmap](2026-10-01-recovery-subagent-roadmap.md) owns dispatch, isolated worktrees, shared-file serialization, review ledgers and merge order. This technical plan owns HATCH evidence and its one-pattern acceptance criteria.

## Global Constraints

- Plan audited at `main` / `1f38904`; no native probe or implementation was run while preparing it.
- The original is the oracle; pattern-file syntax and later AutoCAD conventions cannot replace observable evidence.
- Notes/specifications derive from the binaries; shipped implementation is hand-written from those specifications.
- `acad-oracle` is development infrastructure and must not become a shipped application dependency.
- Use private disposable floppy copies; never modify the corpus images.
- Preserve the recovered flat block-table representation and current public command API.
- Extend the Lean contract only after QEMU establishes observable facts; Lean is a behavioral specification, not recovered AutoCAD source or a proof of universal native equivalence.
- Keep `MAX_ARRAY_ENTITIES = 100_000`; any additional families must respect an aggregate allocation/output bound before drawing mutation.
- Existing LINE geometry and command regressions remain required. No broad `.PAT` parser, arbitrary pattern loader, or dash/dot engine is authorized by the first-pattern slice.

## Review Focus

- A pattern in `ACAD.PAT` but absent from the captured help screen: native name acceptance determines catalogue changes (Task 1).
- Translated boundaries with a fractional origin: native output determines world phase; moving the boundary must not silently reset the lattice (Tasks 1–3).
- Combined pattern angle and scale: native output determines whether definition angles compose with command angle and how spacing changes (Tasks 1–3).
- Mixed curved outer/inner boundaries, tangent contacts and arc endpoints: preserve native spans and avoid adding or discarding crossings by assumption (Tasks 1–3).
- Multiple families and repeated HATCH commands: preserve complete primitive inventory, layers, anonymous names, INSERT transforms, output ordering and undo atomicity (Tasks 2–3).

---

## Audited state and evidence limits

The project graph returned no HATCH nodes and its architecture omitted `acad-cmd`; this plan used the permitted source fallback after that incomplete discovery. Executors should try `search_graph(project="autorust", name_pattern=".*[Hh]atch.*")` first again.

`HATCH_PATTERNS` is a list of `(name, description)` pairs, not an enum or pattern-definition parser. `dispatch.rs` uppercases the whole pattern input and tests membership. `InputState` stores pattern `String`, scale and angle; defaults are 1 and 0, scale must be positive/finite, angle finite. The displayed `name,style / U` prompt does not establish support for styles or user-defined patterns. Those semantics remain outside this slice.

`Editor::add_hatch` rejects every pattern except LINE. It calls `hatch_line_geometry`, checks nonempty output, creates the first free `*Xn` block at `(0,0)` with layer-127 contents, and inserts it on the current layer at `(0,0)` with unit scales/zero rotation. It saves undo only after successful generation.

`hatch_line_geometry` accepts selected top-level closed LINE/ARC loops and positive-radius CIRCLEs. It builds undirected loops, intersects a world-origin lattice of spacing `0.125 * scale`, sorts/deduplicates crossings and pairs spans. Its output begins near the projected midpoint and sweeps upward then downward. Circles exclude tangent scanlines. General touching-loop parity, shared ARC endpoints, nested curved holes and rotated tangencies are not proved by the current fixtures.

Native regression coverage currently includes a square at defaults and scale 2 / angle 30°, a circle, an upper semicircle plus diameter (16 lines), and a nested four-LINE hole. `Geometry.lean` models prompt outcomes and documents that scope; it does not model generated endpoint calculations. The circle/rectangle tests compare ordered primitives and INSERTs; the semicircle/hole tests sort geometry and are narrower about structure. Retain all four styles of evidence and tighten new cases without weakening existing tolerances.

### Catalogue to recover

The current Rust list hardcodes these 23 names; that list is not proof of native catalogue completeness:

`EARTH ESCHER FLEX GRASS GRATE HEX HONEY HOUND INSUL LINE MUDST NET NET3 PLAST PLASTI SACNCR SQUARE STARS STEEL SWAMP TRANS TRIANG ZIGZAG`.

The textual, CRLF / DOS-EOF `corpus/System/ACAD.PAT` has 41 headers. The additional 18 are:

`ANGLE ANSI31 ANSI32 ANSI33 ANSI34 ANSI35 ANSI36 ANSI37 ANSI38 BOX BRASS BRICK CLAY CORK CROSS DASH DOLMIT DOTS`.

The existing native help test asserts only EARTH, LINE and ZIGZAG; it does not prove a complete catalogue. The Rust editor test explicitly rejects ANSI31. Catalogue expansion therefore needs native acceptance/paging evidence, not just copying the file's headers. Record the exact native System-image pattern file identity if it differs from the extracted file.

Useful observed file families (definitions are evidence inputs, not native output claims):

| Family candidate | File rows | What a native probe must settle |
| --- | --- | --- |
| NET | `0, 0,0, 0,.125` and `90, 0,0, 0,.125` | Two continuous directions, family ordering, 90° composition, phase |
| NET3 / GRATE | Three directions / unequal spacings | Oblique family handling / spacing per family |
| PLAST / STEEL | Parallel rows with different starting origins | Base-offset interpretation and transformed phase |
| SQUARE / EARTH / HONEY | Signed dash rows and, for EARTH/HONEY, translated rows | Dash clipping, row drift and row ordering |
| MUDST / SACNCR / DOTS | Zero entries alongside gaps | Native POINT versus degenerate LINE or another primitive |

NET is preferred because it adds a useful grid without requiring dashes or dots. Probe other families to produce a bounded recovery backlog; do not ship them incidentally while generalizing NET.

## File ownership and interfaces

| File | Responsibility / planned action |
| --- | --- |
| `crates/acad-oracle/examples/hatch-pattern-recovery.rs` (new) | Small one-shot capture/export tool; reuse current recovery examples and `generate_pair` |
| `docs/hatch-pattern-recovery.md` (new) | Compact evidence manifest, accepted/rejected names, exact inputs, captured geometry findings and future queue |
| `crates/acad-oracle/tests/commands.rs` | Additional complete-structure differential cases; keep current LINE regressions |
| `formal/AutoCAD/Geometry.lean` | Captured pattern/output contract with its explicit limits |
| `formal/README.md` | State precisely what the new contract checks |
| `crates/acad-cmd/src/geometry.rs` | Bounded additional pattern geometry, only after contract recovery |
| `crates/acad-cmd/src/editor_ops.rs` | Pattern dispatch and existing block/INSERT mutation path |
| `crates/acad-cmd/tests/editor.rs` | Offline refinement, rejection, aggregate bound, undo and repeat cases |
| `crates/acad-cmd/src/lib.rs`, `report.rs` | Catalogue change only if Task 1 proves missing accepted names |
| `docs/HANDOVER-2026-09-30.md`, design spec, `README.md` | Accurate completed slice and remaining limitations |

Existing APIs, verified in source:

```rust
acad_oracle::generate_pair(
    system_disk: &std::path::Path, name: &str, editor_lines: &[&str],
) -> Result<(Vec<u8>, Vec<u8>), String>

// Private command geometry and mutation interfaces:
pub(crate) fn hatch_line_geometry(
    drawing: &acad_model::Drawing, ids: &[usize], scale: f64, angle_deg: f64,
) -> Result<Vec<acad_model::Entity>, String>
pub(crate) fn add_hatch(
    &mut self, pattern: &str, scale: f64, angle_deg: f64, ids: &[usize],
) -> Result<(), String>
```

`Session::boot_disposable(&Path, Option<&Path>, &[&str]) -> Result<Self, String>`, `type_line(&str) -> Result<(), String>`, `capture_editor() -> Result<Vec<u8>, String>` and `text_screen() -> Result<String, String>` are available. Use stabilized CGA bitmap frames for editor prompts, using the `save_png` helper in `dim-angular-recovery.rs`; `text_screen` only decodes genuine alpha-text screens. Use `generate_pair` when command completion succeeds; a timeout is not evidence of rejection.

## Task 1: Native pattern and boundary evidence packet

**Owner:** `gpt-6.1-sol`, high. **Fresh combined spec/quality review:** sol high. Run native probes only under the coordinator's explicit lease for the sole QEMU guest. `Session`'s mutex is process-local; separate workers/test binaries can otherwise starve keyboard/display processing and produce incomplete CGA frames. Input preparation, artifact decoding and notes can run alongside DIM/menu work without another guest.

**Files:** Create the recovery example and evidence document above. No production/formal edits in this task.

**Consumes:** Existing oracle APIs and corpus. **Produces:** Exact native inputs, DWG/DXF artifacts, prompt frames, catalogue status and a contract-ready first pattern selection; no new production API.

- [ ] Read the handover, oracle docs and current four LINE differential tests; record baseline commit, QEMU availability and disk identity in the evidence manifest. Mark skipped/failed probes explicitly; no native parity conclusion follows from a skip.
- [ ] Build the bounded example using this export body and the existing PNG capture helper only for unclear prompt flows:

```rust
let inputs = [
    "LINE", "1,1", "5,1", "5,5", "1,5", "1,1", "",
    "HATCH", "NET", "", "", "W", "0,0", "6,6",
];
let (dwg, dxf) = acad_oracle::generate_pair(&disk, "HNETDEF", &inputs)?;
let native = acad_dwg::parse(&dwg).map_err(|e| e.to_string())?;
std::fs::write(output.join("HNETDEF.DWG"), &dwg).map_err(|e| e.to_string())?;
std::fs::write(output.join("HNETDEF.DXF"), &dxf).map_err(|e| e.to_string())?;
println!("{native:#?}");
```

Use the same missing-disk/QEMU guard as `original_hatch_line_window_matches_generated_block_geometry`; output artifacts to one task-specific OS temp directory, not tracked native binaries. Persist the exact input arrays and numerical findings in the evidence document. Keep DOS drawing names within eight uppercase alphanumeric characters.

- [ ] Capture `HATCH;?` with `Session`; preserve every help page and prompts between pages. Cross-check all 41 file headers with the displayed catalogue. Probe disputed names individually through the pattern prompt, capturing after each input; cancel/close only after observing the prompt. Do not feed scale/window inputs into an unconfirmed prompt.
- [ ] Export NET default square, scale 2 / angle 30° square, angle 90° square, and a fractional translated square `(1.0625,1.03125)`–`(5.0625,5.03125)` selected by `(0,0)`–`(6,6)`. Also compare scale `0.5` on a small square; this distinguishes physical spacing from rounding artifacts. Repeat one square with boundaries created in reversed order.
- [ ] Decode complete native output: boundary items, anonymous block/base, every entity kind/layer, ordered endpoints, current-layer INSERT/transform. Check native DXF against parsed DWG before blaming a geometry mismatch on Rust. Document the resolved direction/phase/scale/order rules with numerical witnesses from the captures.
- [ ] Capture additional LINE boundary probes: concentric circles centered `(3,3)` with radii 2 and 1; outer square `(1,1)`–`(5,5)` with inner circle `(3,3)`, radius 1; the existing semicircle/chord translated fractionally and rotated hatch angle 30°. Probe circle tangencies at lattice-aligned and half-spacing-shifted centers. A touching outer/inner curve gets its own prompt/output record and is supported only if behavior can be specified.
- [ ] Once NET has been accepted, repeat concentric-circle and mixed square/circle cases with NET. If a new clipping bug appears in LINE, make its recovery/fix a prerequisite task instead of masking it in NET.
- [ ] Run one square export for each bounded next-family representative: PLAST, STEEL, SQUARE, MUDST, HONEY. This is a family-inventory probe, not implementation scope. Capture zero-length/POINT output faithfully; do not discard it in a LINE-only extractor.
- [ ] Run `cargo run -p acad-oracle --example hatch-pattern-recovery`. The meaningful result is a complete evidence packet, not a green assertion. A native failure must identify the last confirmed prompt and input.
- [ ] Review the packet and amend downstream geometry steps with captured constants/order and exact test inputs before dispatching implementation. NET proceeds only if its recoverable native output matches the existing representable primitive model. If it does not, select the smallest demonstrated continuous family and rewrite the slice; do not guess a dash/dot representation or enlarge scope automatically.
- [ ] Commit only the example and evidence notes after review: `git add crates/acad-oracle/examples/hatch-pattern-recovery.rs docs/hatch-pattern-recovery.md` then `git commit -m "docs: recover native hatch pattern evidence"`.

## Task 2: Captured contract and failing refinement cases

**Owner:** sol medium for bounded fixture/harness work; sol high if the captured contract includes ambiguous geometry. **Fresh combined review:** sol high. Depends on approved Task 1 findings. Do not begin Rust production changes yet.

**Files:** Modify `formal/AutoCAD/Geometry.lean`, `formal/README.md`, `crates/acad-oracle/tests/commands.rs`, `crates/acad-cmd/tests/editor.rs`; update evidence notes with fixture provenance.

**Consumes:** Task 1's exact primitive/layer/ordering/phase results. **Produces:** An executable bounded contract and offline/native failures that the next task must satisfy.

- [ ] Extend the Lean geometry contract with a separately scoped captured-pattern contract, retaining the existing prompt model. Encode captured default/nonorigin NET spans using fixed decimal coordinates from native DXF as integer millionths; keep rotated/radical coordinates under an explicit comparison tolerance in Rust. State that this checks finite observed outputs, not continuous clipping or general Float equivalence. Do not merely rename `hatchLineBlock` or add a pattern-name constructor and call that a geometry proof.
- [ ] Add `by decide` examples for the captured primitive inventory, block/INSERT metadata and the bounded endpoint/phase cases. Derive their literals from Task 1's artifact, with native input and provenance next to each example. Do not invent expected endpoint values before the capture exists.
- [ ] Run `cd formal && lake build`; expected PASS for the captured contract. Review whether this model actually distinguishes a shifted lattice or missing second family; if not, add the captured witness that does.
- [ ] Add native differential tests adjacent to current HATCH tests, using `generate_pair`, `acad_dwg::parse`, and `Editor::submit` exactly as those tests do. For accepted NET, the minimal failing Rust witness is:

```rust
let mut rust = acad_cmd::Editor::default();
for input in [
    "LINE", "1,1", "5,1", "5,5", "1,5", "1,1", "",
    "HATCH", "NET", "", "", "W", "0,0", "6,6",
] {
    rust.submit(input).unwrap();
}
```

At the audited baseline it fails on final window submission with `HATCH pattern NET is listed but its geometry is not implemented`. Native assertions must check total item count, unchanged boundaries, exact block name/base, complete primitive inventory and layers, endpoint direction/order, and complete INSERT transform/current layer. Compare each corresponding endpoint coordinate with `abs(actual - expected) <= 1e-12`, as existing tests do; enlarge a tolerance only with recorded native rounding evidence. For curved cases that establish geometric equality but not order, compare canonical spans separately and explicitly record that limit.

- [ ] Add offline assertions from captured outputs for default, scale/angle, translated phase and nested boundaries; do not derive expected values by calling the production generator. Add a repeated-HATCH/current-layer case for `*Xn` collision avoidance and a failed-generation case proving drawing/undo remain unchanged.
- [ ] If Task 1 established missing accepted names, add an offline catalogue regression and a native acceptance regression before adjusting the 23-name list. Preserve the ANSI31 rejection test unless native evidence disproves it; then replace it with the recovered acceptance behavior and a genuinely unknown-name test.
- [ ] Run `cargo test -p acad-cmd hatch_ -- --nocapture --test-threads=1` and, under the coordinator's sole native-guest lease, `cargo test -p acad-oracle --test commands original_hatch_ -- --nocapture --test-threads=1`. Record the intended new failures separately from existing regression failures; an oracle skip is not a passing recovery check.
- [ ] Commit reviewed contract/tests separately: `git commit -m "test: specify recovered hatch pattern geometry"` with only this task's named files staged.

## Task 3: First pattern implementation and regression gate

**Owner:** sol high. **Fresh combined review:** sol high. **Final whole-slice review:** one `gpt-6-astra`, high, after checks; use astra earlier only for a persistent ambiguous geometric recovery. Depends on Tasks 1–2 and on DIM/menu implementation releasing shared files.

**Files:** Modify `geometry.rs`, `editor_ops.rs`, relevant tests, and only evidence-backed catalogue files; update status documents when tests pass.

**Consumes:** Existing private APIs plus captured contract. **Produces:** One additional supported pattern through unchanged `Editor::submit`; all unimplemented accepted names remain explicitly unsupported.

- [ ] For NET, first try composing the existing line helper at the two recovered direction angles and preserving its family order. The existing helper already expresses a phase-zero continuous line family; no general `.PAT` engine is needed if Task 1 proves NET shares that contract. Use `hatch_line_geometry(&self.drawing, ids, scale, angle_deg)` and its recovered second-angle call. If native order differs, amend the generator step from the evidence packet before changing code; geometry-set equality alone does not justify inventing serialization order.
- [ ] Keep all generated entities local until both families validate; enforce an aggregate preflight bound before allocation and a complete primitive-output bound before `save_undo`. One helper's 100,000-line check is insufficient for two families or multiple spans across holes. Use a narrowly scoped internal helper only if the preflight or family phase needs it; name/signature belongs in the reviewed Task 1 continuation, not a speculative public API.
- [ ] Update `add_hatch` to select the recovered supported pattern while keeping the existing block naming/layer/INSERT/undo path. Preserve the exact rejection of unimplemented names. Boundary fixes required by Task 1 must have their own captured tests and review before pattern composition.
- [ ] Run the new tests to PASS, including aggregate bound/error atomicity, repeated names/current layer and nonorigin phase. Re-run all existing `original_hatch_` tests; the semicircle witness remains 16 native-matching lines.
- [ ] Follow the shared roadmap's slice integration checks: `cargo test --workspace --exclude acad-oracle -- --test-threads=1`, `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D warnings`, and `cd formal && lake build`; run only the focused real native suite serially under the coordinator's sole guest lease. The coordinator runs one full `cargo test --workspace -- --test-threads=1` with real QEMU after all accepted roadmap slices integrate. Repeat additional relevant checks only for new failures or changed shared behavior; report skips honestly.
- [ ] Update handover/spec/README to name the additional implemented pattern, evidence cases and remaining curved/pattern limits. Keep remaining names listed as future recovery work, not parity. Commit the reviewed implementation/doc slice with `git commit -m "feat: add recovered hatch grid pattern"` if NET was the selected pattern; choose a title matching the actual recovered pattern otherwise.

## Evidence handoff and future work

The native packet must contain: native version/disk identity, file/header catalogue versus observed acceptance, exact scripts and prompt frames, artifact paths, complete primitive inventory/order/layers, numerical phase/scale witnesses, rejected/ambiguous inputs, and the selected first-pattern contract. Retain successful captures so reviewers do not need to reboot QEMU to rediscover every fact. Pass only relevant packet sections and exact touched functions to each worker; avoid full repository dumps.

Future continuous-family recovery: GRATE, NET3, PLAST, PLASTI and STEEL. Future signed-dash/row-drift recovery: EARTH, ESCHER, FLEX, GRASS, HEX, HONEY, HOUND, INSUL, SQUARE, STARS, SWAMP, TRANS, TRIANG and ZIGZAG. Future zero-entry recovery: MUDST and SACNCR. The other 18 file headers require catalogue/acceptance decisions first; DASH/DOTS may supply isolated primitive evidence if accepted. LINE remains supported; none of these queue entries imply current geometry support. Styles, U, custom pattern loading, malformed files and complete HATCH parity are separate specifications.

Curved future work follows observed failures rather than an unlimited combinatorial matrix: shared arc endpoints, nested curved holes, touching loops, more-than-two island depths, winding/order variation and boundary degeneracy. This slice owns only captured cases and any clipping correction necessary for its first pattern.

Shared `geometry.rs`, `editor_ops.rs`, `dispatch.rs`, `input_state.rs`, `Geometry.lean` and `commands.rs` make production implementation serial with DIM/menu. Independent planning, probe preparation and artifact decoding can be parallel; native execution always uses one QEMU guest total through the coordinator's explicit lease. Documentation consolidation can use luna low after geometry evidence is final. Do not use a cheap model to interpret unresolved native geometry.

## Self-review and execution handoff

- [ ] Before execution, check that recovery outputs precede Lean and Rust claims, every primitive asserted was actually captured, and the selected first-pattern branch has exact constants/order rather than assumptions.
- [ ] After Task 1, fill the continuation by replacing contingent geometry instructions with its observed contract; review that amendment before Task 2. This is an evidence dependency, not an authorization request to run wider work.
- [ ] Verify each Review Focus row has its indicated native/offline tests and catalogue expansion has acceptance evidence.
- [ ] Require the independent final slice review after scoped checks; resolve findings before merge. Keep implementation serial with sibling slices.

Planning is complete. Implementation starts only after the parent planning request's review handoff; no QEMU, source edits or commits were performed by the planning worker.
