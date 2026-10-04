# R2 S1 implementation review, pass 1

Read-only review of exact selection.patch SHA256 `e64461528d421f7ee9df826a0db494824f0d3f0b72dddf8c2baad75165f55401`. Verified bytes equal `git diff --cached --binary` in the private selection repository. No tests executed by reviewer; root owns integrated execution. Verdict: **needs-fixes**.

## Findings

1. **P1 — nested bounds grow exponentially, including trivial 1x1 groups.** `crates/acad-cmd/src/geometry.rs:14–31` recursively obtains all descendant points then copies the vector four times at every repeat level. One point nested under sixteen 1x1 repeats grows to 4^16 points, despite geometry being a single point. Any MOVE/COPY/SCALE or unrelated edit reaching refresh_after_edit calls this after mutation/save_undo. `selection.rs::repeat_extents_points` duplicates the same multiplicative scheme for windows. Fix by reducing each child's/group's bounds to constant-size min/max before composing parent corner offsets, or reject a bound failure before mutation. Add a nested 1x1/negative-spacing reference case; do not run an actual allocating 4^16 repro against the current implementation.

2. **P1 — CHANGE layer silently loses group changes depending on representation.** `editor_ops.rs:299–316` assigns layers to immediate members for Item::Repeat, but delegates Item::Entity(Entity::Repeat) to `geometry.rs:140 assign_layer`, which adds/replaces only an outer wrapper. `acad-dwg/src/write.rs:55–64` discards that owner layer when encoding Repeat; encode_repeat takes marker/member layer from the first encoded member. Therefore ARRAY's Entity::Repeat copies and nested groups can display a changed wrapper layer before save but reopen with unchanged child layers. Use one documented recursive-member policy for both representations or reject unsupported group CHANGE before mutation. Tests must compare both historical DWG revisions after CHANGE, not only list status or wrappers.

3. **P1 — selected WBLOCK of a dependency-bearing repeat exports dangling INSERTs.** Newly selectable REPEAT reaches `wblock_selected_entities` (`editor_ops.rs:133–148`), which clones only selected items and omits their referenced block definitions. A group containing INSERT B then saves successfully without B and loses visible geometry on reopen. This is an existing weakness for individually selected INSERT, newly exposed by S1's claim to whole-group WBLOCK support. Preserve transitive block closure, or explicitly reject such group export instead of silent content loss. Existing test uses only LINE members and does not cover this boundary.

4. **P2 — circular ARRAY silently omits groups starting with metadata.** ARRAY sources are converted from Item::Repeat to Entity::Repeat. `entity_ops.rs:177–187` anchors Entity::Repeat from first() only, while item_anchor uses find_map(). A valid group [LOAD, LINE] yields no anchor, is silently skipped at editor_ops circular_array, then adds an undo step and reports successful zero copies. Use first geometric anchor recursively or fail explicitly before mutation. Include this representation in the circular-array test.

## Positive and bounded findings

- Canonical selectable traversal excludes LOAD, definitions and erased records, includes both repeat representations and uses stable document-order owners. LIST/HATCH IDs, selected indexes, app highlighting and explicit API selectable_objects all consume it. API entities retains its previous distinct stored-record meaning.
- Repeat picking has a per-object budget and depth bound and discards partial results when budget is exceeded. Nonzero row/column offsets and negative spacing are covered. INSERT-origin picking is an explicit deferred limitation.
- Unsupported lattice ROTATE checks selected groups before save_undo/mutation. CHANGE point, FILLET, BREAK, HATCH and ENTITYAREA reject unsupported group geometry through their existing type gates. HATCH now consumes canonical IDs, preventing IDs after a repeat selecting a different boundary.
- LIST immediate dialogue remains by S1 scope. Details are bounded and escape text; selected LIST/general window dialogue are S2. No original-ID or field-layout parity is claimed.
- ERASE and BLOCK turn a repeat into Item::Erased(Entity::Repeat). Existing DWG encoder explicitly refuses grouped erased records (`write.rs:encode_erased_entity` record_count !=1). This is safe unsupported-save behavior, but S1 documentation must expose it rather than suggest save/reopen completion after group ERASE/BLOCK. OOPS/UNDO can restore encodable state.
- Highlight preserves top-level LOAD ordering but still drops LOAD effects inside preceding unselected INSERT/REPEAT owners. If testing selected text following such a group, it can use the wrong font. This older generalized highlight limitation should remain visible; integration with L1's deliberate hidden LOAD preservation warrants a follow-up. No independent native field-layout/picking parity is established.

All four actionable findings were sent to the worker. Root should not accept this hash; await a refreshed exact patch. Maximum three source-review passes for S1.

## Repaired exact candidate review, pass 2

Reviewed selection.patch SHA256 `0b36cd247704c30ac772cd38b3861dc43a06715a1cf5e219c55a428802748f48`, verified byte-identical to the private selection repository's staged binary diff. Inspected repair source, new assertions and worker test/lint log tails. No tests run by reviewer.

Original findings 1–4 are corrected at their stated boundaries:
- Bounds now use an explicit iterator stack over stored descendants, aggregate cumulative lattice min/max offsets and return two extrema. Nested 64-level mixed-sign test checks independently calculable extents; window path reuses the strict version. No repeated descendant-vector multiplication.
- CHANGE now applies recursively to leaf records for both group representations; tests reopen flat groups through AC1.2 and AC1.40 and compare actual member layers. Nested recursive policy is independently tested in memory. This does not establish nested codec support.
- Selected WBLOCK now retains block definitions while deriving transitive INSERT closure using the whole-drawing exporter and preserves top-level LOAD records in document order. Test establishes B->C closure, removes UNUSED, and reopens both DWG revisions. LOAD effects inside *unselected* group/INSERT owners remain outside this test and still require a future resource-context policy; the new implementation only retains top-level LOAD and dependencies of selected objects.
- Circular ARRAY now seeks first geometric descendant after LOAD and rejects missing-anchor selections before allocation/undo. Both positive metadata-prefix and atomic no-anchor tests added.

**Verdict remains needs-fixes: P1 unsafe nested-group output newly reachable through the added operations.** Merely documenting writer/reader mismatch is insufficient because save can report success and replace a valid destination with a file the application cannot reopen.

Concrete newly enabled route (source-derived, not executed by reviewer): create a flat REPEAT containing POINT; start another REPEAT; ARRAY the existing group (rectangular1x2); ENDREP. S1 ARRAY now appends Item::Entity(Entity::Repeat), unlike the previously unselectable top-level group. `dispatch/blocks.rs:158–179` accepts all Item::Entity records after the marker, including the new nested repeat, and wraps them into Item::Repeat. `acad-dwg/src/write.rs::encode_repeat` recursively writes the nested record; DWG reader cannot read a fused nested REPEAT and also explicitly rejects ordinary nested markers. `acad-dxf/src/write.rs::entity` emits nested REPEAT/ENDREP and `acad-dxf/src/parse.rs:205–208` rejects an already-open repeat. No save error occurs before staging.

Minimal command sequence following existing prompts:
```
REPEAT; POINT; 1,1; ENDREP; 2; 1; 3; 0;
REPEAT; ARRAY; 1; R; 1; 2; 0; 10; ENDREP; 1; 1; 0; 0;
```
Then SAVE or END in either format can emit unreadable content. A bounded acceptable fix is checked codec rejection of nested REPEAT before staging, covering nested repeats in block entities as well as top-level forms. Alternatively reject such construction before mutation, but existing in-memory nested models still need an explicit export policy. Root may own the checked guard during merge, in which case approval must identify both the S1 patch and the guard's final code/hash. Do not mark this exact patch alone approved.

Worker/root notified immediately. This was pass 2 of the maximum three source-review passes; next pass should verify a concrete guard, not repeat the same evidence search.
