# Nested `BLOCK` Definitions — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Take `AC1.2` corpus coverage from 14 of 16 drawings to 16 of 16, by reading block definitions that appear inside another block's span.

**Architecture:** The 1983 block table is **flat** — a `BLOCK` record written inside another block's span defines a *sibling*, not a child. `acad_model` therefore needs **no change**: `Block { name, base, entities }` already models this correctly. Two things change. `acad-dwg`'s `read_items` tracks open blocks with a stack instead of a single slot, so entities land in the innermost open block and each block closes independently. And `acad-render`'s `flatten` resolves an `INSERT` found *inside* a block body, which it currently cannot, because the block-expansion path has the `Drawing` and the per-entity path does not.

**Tech Stack:** Rust 1.88.0. No new dependencies.

**Spec:** `docs/superpowers/specs/2026-09-28-autocad-14-rust-design.md`

## Global Constraints

- Rust edition 2021, toolchain 1.88.0 (pinned by `rust-toolchain.toml`).
- `acad-dwg` may depend only on `acad-model`; `acad-dxf` stays under `[dev-dependencies]`.
- **`acad_model` must not change in this plan.** If a task seems to need it, that is a signal the flat-block-table finding has been misread — stop and re-check against the evidence in Task 1 rather than widening the model.
- Every offset read in `acad-dwg` goes through the existing checked path returning `DwgError::TruncatedEntity`; a malformed file must never panic.
- Corpus tests SKIP with a message when `corpus/` is absent, never fail. A fresh checkout keeps only the tracked `corpus/manifest.toml`.
- `cargo fmt --all --check` and `cargo clippy --workspace --all-targets -- -D warnings` stay clean. 159 tests pass before this plan starts.
- Read direction of `AC1.2` only. The write direction, `AC1.40`, and oracle verification remain after milestone ③.

### The evidence this plan rests on

Established 2026-09-28 by walking `SELEXOL.DWG` directly. **`ARROW` is defined at depth 2, inside `COOLER` at `0x601`, and then `INSERT`ed at top level fourteen times** (`0xe05`, `0xe38`, `0x11ff`, `0x1232`, `0x1265`, `0x1298`, `0x12cb`, `0x1322`, `0x14e1`, `0x1514`, `0x1547`, `0x157a`, `0x15f5`, `0x1628`). A definition that is referenced from outside its parent's span is not scoped to that parent. The same holds for `HEAD`, defined inside `PACKTWR` at `0x395` and `INSERT`ed twice within `PACKTWR`'s own body.

| Fact | Value |
|---|---|
| Nesting depth | 2 in both blocked files; no corpus file exceeds it |
| `SELEXOL` | 10 `BLOCK` / 10 `ENDBLK`, balanced; walks all 167 records exactly to `entity_end` `0x1c4d` |
| `SELEXOL` nestings | `HEAD` in `PACKTWR` at `0x395`; `ARROW` in `COOLER` at `0x601` |
| `BLIVET` | 4 `BLOCK` / 4 `ENDBLK`, balanced; `$BCIRC` nested in `BLIVET` at `0x146b` |
| Block scoping | **Flat/global.** Definitions are globally referenceable regardless of where they are written |
| Current error | `DwgError::NestedBlock { outer, inner, at }`, raised in `read_items` when a `BLOCK` opens while one is open |

Both files already decode every individual record correctly — `corpus_smoke.rs`'s
`both_still_unsupported_files_decode_cleanly_at_the_flat_record_level` proves it by calling
`read_entities` on their real bytes and getting `Ok`. Only `read_items`' grouping fails.

## Review Focus

1. **A block that inserts itself, directly or through a cycle.** Task 2 makes `flatten` resolve `INSERT`s inside block bodies, which turns block expansion into recursion over a graph the file controls. `A` inserting `B` inserting `A` must terminate, not overflow the stack. A 1983 floppy with a corrupt name field is enough to produce one. *(Task 2)*
2. **An `ENDBLK` with no open block**, and a `BLOCK` left open at `entity_end`, once tracking is a stack rather than a single slot. Both must stay named errors carrying an offset — the stack makes it easy to under- or over-pop silently. *(Task 1)*
3. **An `INSERT` naming a block that does not exist.** `acad-dxf` already treats this as a named error; `acad-dwg` and `flatten` must not start silently dropping the entity instead, which renders a drawing that is quietly wrong. *(Tasks 1, 2)*
4. **Document order across a nested definition.** Milestone ① established that order is content. A block closing inside another's span must not reorder the surrounding items, and the `items` list must still round-trip through `Drawing::blocks()` in a defined order. *(Task 1)*
5. **A nested block whose parent never closes.** `SELEXOL` and `BLIVET` are well-formed, so nothing in the corpus exercises a half-open nest; a truncated file would. It must be a named error, never a panic or a partially built `Drawing`. *(Task 1)*

---

### Task 1: Stack-based block tracking in `read_items`

**Files:**
- Modify: `crates/acad-dwg/src/entity.rs`
- Modify: `crates/acad-dwg/src/error.rs`
- Modify: `crates/acad-dwg/tests/entity_corpus.rs`

**Interfaces:**
- Consumes: the existing `read_items(bytes, &meta) -> Result<Vec<Item>, DwgError>` and `read_record_body`.
- Produces: the same signature, now accepting nested `BLOCK` definitions. `DwgError::NestedBlock` is **removed** — it described a limitation that no longer exists.

- [ ] **Step 1: Write the failing test for a nested definition**

Add to `crates/acad-dwg/src/entity.rs`'s tests. Build a synthetic buffer holding `BLOCK "OUTER"`, a `LINE`, `BLOCK "INNER"`, a `LINE`, `ENDBLK`, a `LINE`, `ENDBLK` — the shape `SELEXOL` actually has.

```rust
    #[test]
    fn a_block_defined_inside_another_is_a_sibling_not_a_child() {
        // SELEXOL defines ARROW inside COOLER and then INSERTs ARROW at top
        // level fourteen times, so a nested definition is globally
        // referenceable: the block table is flat.
        let bytes = nested_block_fixture();
        let meta = HeaderMeta {
            entity_count: 7,
            entity_end: bytes.len() as u32,
        };
        let items = read_items(&bytes, &meta).unwrap();

        let blocks: Vec<&str> = items
            .iter()
            .filter_map(|i| match i {
                Item::Block(b) => Some(b.name.as_str()),
                _ => None,
            })
            .collect();
        assert_eq!(blocks, vec!["INNER", "OUTER"], "INNER closes first");

        let outer = items
            .iter()
            .find_map(|i| match i {
                Item::Block(b) if b.name == "OUTER" => Some(b),
                _ => None,
            })
            .unwrap();
        assert_eq!(
            outer.entities.len(),
            2,
            "OUTER holds its own two LINEs, not INNER's"
        );
    }

    #[test]
    fn an_endblk_with_no_open_block_is_still_an_error() {
        // Review Focus 2: a stack makes it easy to over-pop silently.
        let bytes = lone_endblk_fixture();
        let meta = HeaderMeta { entity_count: 1, entity_end: bytes.len() as u32 };
        assert!(matches!(
            read_items(&bytes, &meta),
            Err(DwgError::StrayEndblk { .. })
        ));
    }

    #[test]
    fn a_nested_block_left_open_at_the_end_names_the_innermost() {
        // Review Focus 5. The innermost unterminated block is the informative
        // one: naming the outer would send a reader to the wrong offset.
        let bytes = unterminated_nest_fixture();
        let meta = HeaderMeta { entity_count: 3, entity_end: bytes.len() as u32 };
        let Err(DwgError::UnterminatedBlock { name, .. }) = read_items(&bytes, &meta) else {
            panic!("expected UnterminatedBlock");
        };
        assert_eq!(name, "INNER");
    }
```

Write the three `*_fixture()` helpers beside the existing record fixtures in that module, using the same `TYPE_*` constants and the 4-byte header + length-prefixed-name layout `BLOCK` already uses.

- [ ] **Step 2: Run them and confirm they fail**

Run: `cargo test -p acad-dwg a_block_defined_inside_another`
Expected: FAIL with `DwgError::NestedBlock` — the current code rejects exactly this.

- [ ] **Step 3: Replace the single open-block slot with a stack**

In `read_items`, change the `open: Option<Block>` to `open: Vec<Block>`:

- a `BLOCK` record pushes a new `Block`;
- an `ENDBLK` pops the innermost and emits it as `Item::Block`, or returns `DwgError::StrayEndblk { at }` if the stack is empty;
- every other entity is pushed to `open.last_mut()`, or to `items` when the stack is empty;
- after the loop, a non-empty stack returns `DwgError::UnterminatedBlock` naming the **innermost** block and its offset.

Delete the `NestedBlock` variant from `error.rs` and its `Display` arm. It described a limitation this task removes; leaving it would be a second way to describe a state that can no longer occur.

- [ ] **Step 4: Run and confirm green**

Run: `cargo test -p acad-dwg`
Expected: PASS. The existing `a_block_opened_before_the_previous_one_closes_is_a_nested_block_error` test asserts the old behaviour and must be **replaced**, not deleted — it becomes the new nesting test if it is not already covered by Step 1.

- [ ] **Step 5: Add the corpus assertions**

Extend `crates/acad-dwg/tests/entity_corpus.rs`:

```rust
#[test]
fn selexol_and_blivet_group_their_nested_blocks() {
    for (file, count, nested) in [
        ("SELEXOL", 167usize, [("HEAD", "PACKTWR"), ("ARROW", "COOLER")].as_slice()),
        ("BLIVET", 149, [("$BCIRC", "BLIVET")].as_slice()),
    ] {
        let Some(bytes) = corpus(&format!("Samples/{file}.DWG")) else {
            return;
        };
        let (_, meta) = parse_header(&bytes).unwrap();
        assert_eq!(meta.entity_count as usize, count, "{file} record count");

        let items = read_items(&bytes, &meta)
            .unwrap_or_else(|e| panic!("{file} should now group: {e}"));
        let names: Vec<&str> = items
            .iter()
            .filter_map(|i| match i {
                Item::Block(b) => Some(b.name.as_str()),
                _ => None,
            })
            .collect();
        for (inner, outer) in nested {
            assert!(names.contains(inner), "{file}: {inner} is a block in its own right");
            assert!(names.contains(outer), "{file}: {outer} survived the nesting");
        }
    }
}
```

- [ ] **Step 6: Commit**

```bash
cargo fmt --all && cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace
git add crates/acad-dwg
git commit -m "feat(dwg): nested BLOCK definitions are siblings, not children"
```

---

### Task 2: Resolve `INSERT`s inside block bodies

**Files:**
- Modify: `crates/acad-render/src/flatten.rs`

**Interfaces:**
- Consumes: `acad_model::{Drawing, Entity, Item}`, `Viewport`.
- Produces: `flatten(d, vp)` expanding an `INSERT` found inside a block body. `flatten_entity(e, vp)` keeps its signature and keeps returning `Vec::new()` for `Insert` — the recursion lives in `flatten`, which has the `Drawing`.

`crates/acad-render/src/flatten.rs:33` is `flatten_entity(e, vp)`, which has no `Drawing` and so answers `Entity::Insert { .. } => Vec::new()`. `flatten` expands top-level `INSERT`s only, by looking the block up and calling `flatten_entity` on each of its entities. So a block that inserts another block draws nothing — and `SELEXOL`'s `PACKTWR` inserts `HEAD` twice, `COOLER` inserts `ARROW`. Task 1 alone would make `SELEXOL` parse and render incompletely.

- [ ] **Step 1: Write the failing test**

```rust
    #[test]
    fn an_insert_inside_a_block_body_is_expanded() {
        // SELEXOL's PACKTWR inserts HEAD; COOLER inserts ARROW. Without this,
        // those parts of the drawing are silently missing.
        let d = Drawing {
            header: test_header(),
            items: vec![
                Item::Block(Block {
                    name: "INNER".into(),
                    base: Point { x: 0.0, y: 0.0 },
                    entities: vec![Entity::Line {
                        start: Point { x: 0.0, y: 0.0 },
                        end: Point { x: 1.0, y: 1.0 },
                    }],
                }),
                Item::Block(Block {
                    name: "OUTER".into(),
                    base: Point { x: 0.0, y: 0.0 },
                    entities: vec![Entity::Insert {
                        origin: Point { x: 2.0, y: 2.0 },
                        x_scale: 1.0,
                        y_scale: 1.0,
                        rotation_deg: 0.0,
                        name: "INNER".into(),
                    }],
                }),
                Item::Entity(Entity::Insert {
                    origin: Point { x: 0.0, y: 0.0 },
                    x_scale: 1.0,
                    y_scale: 1.0,
                    rotation_deg: 0.0,
                    name: "OUTER".into(),
                }),
            ],
        };
        assert_eq!(
            flatten(&d, &vp()).len(),
            1,
            "the LINE inside INNER, reached through OUTER, must be drawn"
        );
    }

    #[test]
    fn a_block_that_inserts_itself_terminates() {
        // Review Focus 1: the file controls this graph. A corrupt name field is
        // enough to make a cycle, and unbounded recursion is a stack overflow,
        // not an error message.
        let d = Drawing {
            header: test_header(),
            items: vec![
                Item::Block(Block {
                    name: "LOOP".into(),
                    base: Point { x: 0.0, y: 0.0 },
                    entities: vec![Entity::Insert {
                        origin: Point { x: 1.0, y: 1.0 },
                        x_scale: 1.0,
                        y_scale: 1.0,
                        rotation_deg: 0.0,
                        name: "LOOP".into(),
                    }],
                }),
                Item::Entity(Entity::Insert {
                    origin: Point { x: 0.0, y: 0.0 },
                    x_scale: 1.0,
                    y_scale: 1.0,
                    rotation_deg: 0.0,
                    name: "LOOP".into(),
                }),
            ],
        };
        // The assertion is that this returns at all.
        let _ = flatten(&d, &vp());
    }
```

`flatten`'s tests have never built a `Drawing` — they only exercise `flatten_entity`, which
takes an `&Entity`. So both tests above need a `Header`, which has twelve required fields and
no `Default`. `acad-model` has a `test_header()` but it is private to its own `#[cfg(test)]`
module and unreachable from here, and this plan must not change `acad-model`. Add a local copy
beside the existing `vp()` at `flatten.rs:143`; both tests above already call it:

```rust
    use acad_model::{
        header::{DwgView, Header, Mode},
        Block, Extents, Item,
    };

    /// `Header` has no `Default` and `acad-model`'s own `test_header` is private
    /// to its test module. `flatten` reads only `Drawing::items`, so every field
    /// here is a zero that no assertion depends on.
    fn test_header() -> Header {
        let zero = Extents { xmin: 0.0, xmax: 0.0, ymin: 0.0, ymax: 0.0 };
        Header {
            extents: zero,
            limits: zero,
            base: Point { x: 0.0, y: 0.0 },
            view: DwgView { center: Point { x: 0.0, y: 0.0 }, height: 0.0 },
            snap: Mode { on: false, spacing: 0.0 },
            grid: Mode { on: false, spacing: 0.0 },
            ortho: false,
            fill: false,
            text_size: 0.0,
            trace_width: 0.0,
            current_layer: 0,
            layers: Default::default(),
        }
    }
```

The `vp()` helper already in that module supplies the `Viewport`; it does not read the header,
so the zeros are harmless. If that stops being true, the test will fail loudly rather than
quietly — `Viewport::fit` on a zero-sized extent is already covered by
`degenerate_extents_do_not_divide_by_zero` in `viewport.rs`.

- [ ] **Step 2: Run and confirm failure**

Run: `cargo test -p acad-render an_insert_inside_a_block_body`
Expected: FAIL asserting `0 == 1`. Run `a_block_that_inserts_itself_terminates` separately and expect it to **hang or overflow the stack** — that is the failure, and seeing it is the point.

- [ ] **Step 3: Make block expansion recursive, with a depth cap**

Extract the existing `INSERT` expansion body into a function that takes the `Drawing`, the `Insert`'s fields, the `Viewport`, and a remaining-depth counter; recurse into it when a block's entity is itself an `Insert`; and return `Vec::new()` when the counter reaches zero.

```rust
/// A block may insert another block; `SELEXOL` nests two deep. The graph comes
/// from the file, so a cycle is possible — a corrupt name field is enough — and
/// unbounded recursion would be a stack overflow rather than a missing shape.
/// AutoCAD 1.4 itself carried a nesting limit; this is ours.
const MAX_INSERT_DEPTH: u32 = 16;
```

Keep `flatten_entity` as it is, including its `Insert => Vec::new()` arm and its comment — it remains the per-entity path used by everything that has no `Drawing`.

- [ ] **Step 4: Run and confirm green**

Run: `cargo test -p acad-render`
Expected: PASS, including the cycle test returning promptly.

- [ ] **Step 5: Commit**

```bash
cargo fmt --all && cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace
git add crates/acad-render
git commit -m "feat(render): expand INSERTs inside block bodies, with a depth cap"
```

---

### Task 3: 16 of 16, and say so honestly

**Files:**
- Modify: `crates/acad-dwg/tests/corpus_smoke.rs`
- Modify: `README.md`

**Interfaces:**
- Consumes: Tasks 1 and 2.
- Produces: a smoke test with an empty unsupported list.

- [ ] **Step 1: Move `SELEXOL` and `BLIVET` into the rendering list**

`corpus_smoke.rs` keeps two lists, `RENDERS` and `UNSUPPORTED`, and a
`the_census_accounts_for_all_sixteen_ac12_drawings` test that they sum to 16. Move both
files across and delete `known_unsupported_drawings_fail_with_the_recorded_type_code`,
whose whole subject is gone.

Keep the census test. An empty `UNSUPPORTED` is a fact worth asserting, and the census is
what stops a file being quietly dropped from both lists.

- [ ] **Step 2: Run it**

Run: `cargo test -p acad-dwg --test corpus_smoke -- --nocapture`
Expected: all 16 parse and render above `MIN_LIT`. Record `SELEXOL`'s and `BLIVET`'s lit-pixel
counts; if either is close to the floor, say so rather than lowering it — a drawing that
barely renders is a finding about Task 2's expansion, not about the threshold.

- [ ] **Step 3: Update the README**

Its milestone ④ row and its long paragraph both describe the 14/16 state and the
`NestedBlock` limitation in detail. Rewrite them for 16/16. **Keep the verified-versus-inferred
distinction exactly as it is** — this plan does not add an oracle for `POINT`, `TRACE`, `SOLID`
or `REPEAT`, and `SELEXOL` and `BLIVET` still have no DXF sibling, so their rendering is
evidence that the layouts generalise, not a second independent check. Say that.

- [ ] **Step 4: Commit**

```bash
git add crates/acad-dwg/tests/corpus_smoke.rs README.md
git commit -m "feat(dwg): all 16 AC1.2 corpus drawings load and render"
```

---

### Task 4: Record the flat block table in the spec

**Files:**
- Modify: `docs/superpowers/specs/2026-09-28-autocad-14-rust-design.md`

**Interfaces:**
- Consumes: the evidence in this plan's header.
- Produces: §4.2 recording block scoping as a format fact.

- [ ] **Step 1: Add the finding to §4.2**

Beside the existing entity-record paragraphs, record that the block table is flat: a `BLOCK`
record inside another block's span defines a sibling, and the evidence is that `SELEXOL`
defines `ARROW` inside `COOLER` and then `INSERT`s it at top level fourteen times. Note the
observed maximum nesting depth of 2, and that `acad_model` needs no recursion as a result.

Also note what is **not** established: whether a 1983 writer would ever emit a nested
definition deliberately, or whether this is an artifact of how these two drawings were edited.
The corpus cannot distinguish those, and the reader does not need to.

- [ ] **Step 2: Note the round-trip limitation**

`acad-dxf`'s parser rejects a nested `BLOCK` outright (`parse.rs:161`), and its writer emits
blocks in `items` order. So a DWG with nested definitions, written back out as DXF, produces a
*flat* file whose block table is identical but whose nesting is gone. That is correct — the
block table is what the format means — but it means DWG→DXF is not byte-preserving for these
two files, and the spec should say so before milestone ④'s write direction assumes otherwise.

- [ ] **Step 3: Commit**

```bash
git add docs/superpowers/specs/2026-09-28-autocad-14-rust-design.md
git commit -m "docs(spec): the 1983 block table is flat"
```

---

## Exit criteria

- `cargo test --workspace` passes, with the corpus present and with only `corpus/manifest.toml`.
- All 16 `AC1.2` corpus drawings parse and render above `MIN_LIT`; `UNSUPPORTED` is empty.
- `acad_model` is unchanged by this plan.
- A block that inserts itself terminates rather than overflowing the stack.
- `DwgError::NestedBlock` no longer exists, and `StrayEndblk` / `UnterminatedBlock` still name an offset.
- The README claims 16/16 without weakening the verified-versus-inferred distinction.
