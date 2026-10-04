# R2 final merged implementation review — pass 3

**Verdict: pass for the scoped source review; root integrated execution remains a separate acceptance gate.** No remaining blocking source finding in S1/L1, nested-output guards, or their visibility/resource integration. This completes the maximum three source-review passes; do not interpret it as complete native AutoCAD parity.

Reviewed final exact patch:
`/var/folders/d1/r2bpvclx19q4vd4c_8g42ddh0000gn/T/autorust-native-loops-20261004-548e_3pv/merged-final.patch`

SHA256: `c097b98196220df41b16b8c6112b3593e546168c04adc88db46ccb01e79db02a`.

Verified byte-identical to `git diff --cached --binary` in `review-final`. Initial pass3 hash e9b2ba42f48fff2e832dbdba96db87c85cfafef04be9b69c73b8957cecb79b8c was replaced during review by a documentation-only clarification of explicit hidden-ID selection. Reviewed that clarification. All217 files in `merged-native-sha256.json` matched both review-final and `/Users/dmytro/github/autorust`, with zero mismatches. No code edits or tests executed by reviewer. Worker counts are not independently certified here.

## Closure of prior findings

- P1 layer-default disagreement: Header fallback is layer1, consistent with model and both codecs; bare repeat markers have no owner layer. Explicit wrapped owners and member layers gate geometry. The added bare LINE/INSERT/repeat-child rendering and both-version reopen checks establish the specific correction.
- P1 nested bounds amplification: command bounds iterate stored descendants with cumulative lattice minimum/maximum offsets and return two points. Visibility-specific windows aggregate bounded descendant boxes and repeat extremes without generating cells. Deep/mixed-sign assertions test geometry, not simply a count.
- P1 CHANGE layer mismatch: both repeat representations recursively change descendant layers. Flat group DWG reopen assertions check actual leaf layers for both revisions; nested in-memory changes are correctly distinguished from unsupported persistence.
- P1 selected WBLOCK missing dependencies: selected exporter retains top-level ordered LOAD records and resolves transitive INSERT block closure; B->C fixture verifies both DWG revisions. Prior unselected-owner resource-context caveat is not falsely treated as solved for every export.
- P2 metadata-first circular ARRAY: recursive first-geometric-anchor lookup skips LOAD, and no-anchor selections fail before mutation/undo.
- P1 newly reachable unreadable nested save: both checked codecs now reject lexical nested repeat groups before byte serialization/staging. This closes the active-REPEAT/ARRAY corruption path without pretending to implement nested-group persistence.

## Exact safety and integration inspection

**Codec grammar and caller coverage.** `acad-dwg/src/write.rs::validate_repeat_structure` and `acad-dxf/src/write.rs::validate_repeat_structure` unwrap layer wrappers, start each block body as an independent root, and reject a Repeat while already inside a Repeat. Top-level Entity/Erased and Item::Repeat are covered. INSERT references are not followed as lexical nesting, so a repeat containing an INSERT whose independent block contains a flat repeat remains writable. Iterator stack never pushes a second repeat level. DWG encode/encode_version and public write/write_version route through the guard. DXF try_write routes through it and rejects OFF state; legacy public write delegates to try_write and fails explicitly. No production file/API route found still using the panic adapter.

`acad-app/src/document.rs::Format::save` encodes before `storage::replace`. Session save updates attachment/baseline only after successful save; END returns success only after save. The new lifecycle regression executes the source-derived active-REPEAT/ARRAY repro from AC1.2, AC1.40 and DXF baselines, then checks SAVE, API save/export and END errors, original bytes/attachment/revision, no staging files, pending API prompt, dirty state and undo-to-baseline. This is meaningful end-to-end command coverage for the failure gate, not just an encoder unit assertion.

**Selection visibility and numbering.** `acad-cmd/src/selection.rs` keeps canonical owner numbering independent of visibility. Explicit IDs/ALL/LAST may still edit hidden owners by documented policy; picks and windows filter geometry. `selection/visible_bounds.rs` gates all owner wrappers, uses default layer1 only for bare ordinary records, ignores LOAD as geometry and independently filters repeat/block children. INSERT-origin hits require bounded visible block geometry; transformed conservative bounds serve window containment. Missing/all-hidden/empty contents give no origin hit. This preserves the existing origin-pick limitation rather than claiming full transformed-geometry picking.

Picking is capped per owner at100000 visited member instances and32 repeat levels; exhausted scans discard partial hits. Window bounds use100000 stored visits/256 levels/16 INSERT references and return no partial selection on failure. Canonical traversal itself does not renumber when layers toggle. New API click tests check hidden miss, visible stable ID, highlight pixels, read-only discovery and explicit hidden-ID ERASE/UNDO.

**Resource state and highlighting.** `acad-render/src/flatten.rs::flatten_owners`, `resource_walker.rs` and `selection_policy.rs` separate owner emission from ordered LOAD effects. Unselected/hidden repeat and INSERT owners still advance the same font/shape state used by the ordinary renderer. Selected-owner preflight prevents generated-cell expansion over the documented record-work budget; incomplete LOAD traversal sets context_failed and clears selected primitives instead of returning stale-font highlights. Tests use distinguishable ALT/TXT strokes behind a hidden INSERT and repeat, and separately force context-budget failure. The cap counts record visits, not font strokes; documentation now states that precise boundary. Per-owner caps are not a claim of globally bounded ordinary-frame rendering.

**LIST/API/application integration.** Immediate LIST scope is retained and clearly distinguished from selected LIST/general selection dialogue. Report details use canonical IDs and bounded text; API adds selectable_objects while preserving prior entities semantics. HATCH uses canonical owner IDs and rejects unsupported repeat boundaries atomically. App highlight maps IDs to original indexes and calls the resource-aware selected renderer. The refreshed policy paragraph now agrees with the explicit hidden-ID policy.

**GUI workflow source.** `tools/check_acad_gui_api.py` adds a flat repeat, canonical count/COPY/UNDO checks, OFF/ON RGBA comparison, OFF-state save refusal for existing and absent destinations, live-process END failure, nested command-built output refusal, unchanged file bytes/attachment, and undo recovery before subsequent save/reopen/lifecycle flows. These are actual attached API/MCP operations in the harness, not merely static flags. Reviewer inspected the harness; root must report actual execution and visual inspection independently.

## Remaining gaps and claim limits

- L2 OFF-layer persistence remains unsupported; safe refusal does not satisfy it.
- B3 nested repeats and erased multi-record REPEAT DWG persistence remain unsupported; BLOCK of a repeat can leave an erased group that triggers the documented refusal. Guarding file replacement is interim safety.
- S2 selected LIST/general selection dialogue, transformed geometry picking, precise text/shape bounds and visible-only ZOOM fitting remain open. Explicit ID semantics and wrapper ownership are native Rust policies, not recovered original parity.
- Ordinary full-frame rendering can still expand very large repeats; selected-owner record caps do not establish global responsiveness or font-stroke caps. This requires its separate performance milestone.
- Selected WBLOCK retains top-level LOAD and referenced block closure; it does not reconstruct font state contributed by arbitrary preceding unselected group/INSERT owners. Keep the export/resource-context compatibility gap visible if broader guarantees are claimed.
- DIM D1 has its own independent audit verdict in dim-contract-review.md. Neither this review nor passing existing fixtures resolves general large-arrow external text placement or the prospective projected-extent fit contract.

No tolerance changes, new guest collection, translated runtime changes or evidence mutation were used to justify this approval. Root owns full six-package tests, Clippy, workspace compilation, attached GUI execution and durable ledger/status updates.

Root execution update received after source verdict:455 native tests passed, zero failed/ignored; six-package all-target Clippy with warnings denied passed;44 changed Rust files' formatting, diff and Python syntax checks passed;217 native and41 protected hashes matched. These are root-reported results, not reviewer-executed checks. Workspace all-target compilation and actual GUI smoke were still running at this update.
