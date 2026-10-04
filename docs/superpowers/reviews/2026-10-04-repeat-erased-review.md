# B3b and B3a resource repair — exact source review

Verdict: **PASS for this incremental source candidate**; B3a named-WBLOCK P2 closed. Final merged source/integration gate remains required; this private candidate does not contain the coordinator's L3/render guards or T1 changes.

Independently matched exported `RUN/b3b.patch` and staged `RUN/repeat-codec` diff to SHA256 **`a6bbadf1591358249205ea49fc334f2e8a40cff832be734a1e87f090121e6615`**, against immutable B3a `a5cdbc0fbfcb37bb17932e8abba15db6d6a772c1`. Nineteen staged files, no unstaged changes at inspection. This is the B3a repair follow-up/source pass 2, plus first B3b incremental inspection. No blocking finding.

## Erased-owner policy

`acad-dwg/src/write.rs` propagates a single erased flag through each lexical Repeat start, every ordinary child/nested Repeat, and each matching ENDREP. BLOCK definitions use their independent live encoding; INSERT references do not propagate erasure into them. Record bodies and marker/child layers are retained, physical record counting stays unchanged, and the old one-record temporary erased encoder is removed.

`entity/record.rs` decodes magnitude with `unsigned_abs`, retains erased flags on Repeat markers, validates fields/layers, and keeps negative BLOCK/ENDBLK unsupported. `entity/stream.rs` accepts uniformly signed repeat nesting only: positive ordinary children in a negative owner fail; negative leaves in a live Repeat/Block fail; parent/child opening signs and closing signs must match; negative roots beneath a block fail. A balanced all-negative top-level subtree becomes one Item::Erased(Entity::Repeat), with nested members represented beneath that owner. Empty/zero-dimension groups, nonfinite fields, depth/count/end/truncation violations remain errors for either sign. The flat geometry API additionally validates structural assembly whenever negative markers occur, preventing a malformed signed owner from leaking positive geometry.

The shared model preflight now permits native erased groups while retaining explicit owner-wrapper/stacked-wrapper refusal, record/depth limits, and finite ordinary fields/block bases. DWG decoding also checks post-conversion ordinary fields; AC1.2 TEXT writing checks overflow after its 4/3 conversion. These checks prevent this policy from accepting or producing nonfinite body fields under a negative sign.

DXF remains an explicitly documented live-only export. It skips the entire erased owner, including all internal LOAD effects and marker metadata. Both public writer adapters document this history loss. Session serialization preserves the in-memory erased model and restoration history; reopening does not manufacture OOPS/UNDO history. This is an approved native whole-owner policy using the recovered physical sign rule, not original source-member selection parity.

## B3a P2 closure: WBLOCK resources

`acad-cmd/src/export_context.rs` replaces the direct-LOAD filter with an ordered LOAD-only projection. It traverses stored Repeat bodies once, resolves INSERT bodies without generated cells, preserves LOAD layers, and includes hidden owners' resource effects. It ignores erased owners and unused definitions. Work is bounded by 100,000 visits, 256 stored depth, and 16 INSERT frames with active-name cycle detection; missing references and unresolved context fail explicitly. Output size is bounded by visits; no fanout work-stack accumulation occurs.

Named export projects prior document resource effects before copying the selected block's body; selected export projects unselected owners in document order around retained selected owners. Borrowing group validation runs before recursive owner/definition cloning and dependency collection. Named/selected/whole helpers propagate Result through file dispatch, so context refusal produces no SaveDrawing effect, editor mutation, or destination write. Transitive dependency closure and named block base are retained.

The dedicated regression compares exported font/shape primitives against the selected source after hidden nested Repeat/INSERT LOADs, requires no unrelated resource geometry/definition export, and repeats comparisons after both DWG revisions and DXF reopen. Private tests clear OFF state only for the old private codec baseline; merged L3 must preserve it independently. Missing/cyclic/deep/oversized context tests check drawing/prompt/undo stability. These assertions meaningfully close the reported direct-LOAD-only omission.

## Evidence assessed and residuals

Independently read source and new assertions rather than treating worker's 475-test/Clippy report as verification. The manually assembled all-negative nested fixture verifies exact record bytes/count/marker and child layers; each of seven physical signs is individually flipped to require refusal. Tests also cover invalid layers, truncated closes, zero dimensions, nonfinite fields, unknown signed magnitude, live-block erased subgroups, depth, 65,535-record boundary, revision-specific overflow, erased LOAD suppression and unchanged block definitions. Session/API tests exercise ERASE/BLOCK SAVE/END/reopen, pending input, in-session OOPS/UNDO and the absent reopened restoration stack. Existing original corpus spans and tolerances were not weakened. No tests/builds or external/guest tools were run by reviewer.

Still explicit compatibility gaps: erased ordinary members or subgroups inside live groups/blocks, negative block markers, and lossless file encoding for runtime OnLayer(Repeat) owners. These require separate model/format work; do not call this original group-edit parity. The reviewed root ordinary-render guard and L3 signed-layer validations must survive merge. Final root integration/GUI and exact merged source approval remain outstanding.
