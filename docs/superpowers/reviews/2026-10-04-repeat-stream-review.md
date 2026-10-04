# B3a independent exact-source review — pass 1

Verdict: **needs-fixes**, narrowly for named WBLOCK resource context. Marker/codec portion has no blocking finding in this pass. Integrated acceptance also requires the coordinator-owned ordinary-render expansion guard. Parent B3 remains open for B3b.

Reviewed immutable private commit `a5cdbc0fbfcb37bb17932e8abba15db6d6a772c1`, baseline `457764aa07321b7c44f4a08890f39c79d774a14b`. Its complete diff independently hashes to exported `b3a.patch` SHA256 **`7ade74d468653d3a444c1eb7b5f97e4c54970035e1ee59019a4bc26985376015`**. Mutable B3b working-tree changes were excluded by reading `git show` of this commit.

## Finding

**P2 — Named WBLOCK drops preceding nested LOAD effects.** `crates/acad-cmd/src/editor_ops.rs:128–131` copies only preceding top-level `Item::Entity(LOAD)` records. Rendering carries the same mutable resource state through `Repeat` children (`acad-render/src/resource_walker.rs:21–31`) and INSERT bodies (`:209–227`), including hidden resource traversal. A drawing with a preceding Repeat containing `LOAD ALT`, followed by block GROUP containing TEXT, then an INSERT of GROUP, therefore uses ALT; named WBLOCK GROUP drops that LOAD and reopens with default TXT. Shape-library lookup has the analogous failure. This is a source-derived reproduction, not an executed GUI claim. The added `group_exports.rs` test covers FIRST/SECOND direct root LOADs and INNER within the exported body; it does not cover the omitted preceding nested context.

Fix with bounded ordered resource-effect collection through preceding Repeat/INSERT owners (retain LOAD effects without exporting their geometry); fail explicitly if traversal cannot establish the context. A safe refusal can bound this export subcase while actual general support remains a follow-up. Test both nested REPEAT and INSERT LOAD context, existing destination/drawing preservation on refusal, and a successful reopened text/shape rendering comparison when supported.

## Verified source properties

- REPEAT is emitted/read as an independent four-byte marker, ENDREP as its own record. The next leaf keeps its own ordinary type/layer header. `Repeat.start_layer/end_layer` preserve marker metadata independently of runtime visibility ownership. Constructor updates capture REPEAT and ENDREP command layers; ordinary edit clones retain metadata.
- Shared iterative writer validation rejects explicit OnLayer(REPEAT), stacked layer wrappers, erased groups in this B3a candidate, empty/zero-dimension/nonfinite groups, depth above 64 and stored records above 65,535 before recursive serialization. Referenced INSERT definitions are independent roots, not lexically nested streams.
- DWG logical assembly uses separate bounded repeat/block stacks, checks cross-block group boundaries, unmatched/unclosed/truncated markers, exact end/count landing, and rejects erased structural records/member states it cannot represent. Existing public `RecordHeader`, `read_entities`, `read_items` aliases remain available. The geometry-only view intentionally stays a flat physical-leaf view.
- Historical DXF preserves start/end marker layers and ordered nested members, applies structural depth/count bounds and checks nested INSERT references. The checked public writer and legacy panic adapter share validation; no silent owner-wrapper normalization was introduced.
- Named WBLOCK retains transitive referenced definitions and base point; direct preceding LOAD order is preserved. The broader context gap is the finding above.
- New manually assembled DWG records independently exercise mismatched marker/child layers and nested LOAD bodies in both revisions. Existing original FLOOR/BLIVET marker spans are compared byte-for-byte; moved entity unit tests retain their assertions, with the unknown child offset correctly changed by four bytes for the independent opening marker. Lifecycle tests now demonstrate successful nested SAVE/END/reopen and preserve atomic failure checks using nonfinite derived spacing. Worker test-count/lint claims were not substituted for this source review; no builds/tests were rerun by reviewer.

## Integration and remaining limits

The exact candidate still permits ordinary full rendering to expand a u16 lattice without a per-owner work bound (`acad-render/src/flatten.rs:277–299`; `resource_walker.rs:21–49`). Root already owns the merged guard; inspect its exact source and LOAD-state behavior before integrated approval. Do not treat codec depth/count limits as expansion limits.

B3b erased-owner persistence is a separate incremental native policy, not established original ERASE/OOPS group parity. Partial erased member states and after-open editor history remain distinct limitations. L3 signed layer header changes are also outside this B3a hash and need merge reconciliation; this candidate's old OFF-layer refusal is not a regression relative to its private baseline. No guest/oracle/translated code, retained evidence, or implementation files were modified.
