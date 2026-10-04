# Independent D1 contract review

Date 2026-10-04. Read-only review of reviews/dim-evidence.md and retained source/fixtures. No implementation, original guest, new collector, translated runtime, tolerance change or fixture mutation. This is separate from R2 selection/layer review passes.

## Decision

**D1 evidence audit: pass.** The audit accurately separates retained finite observations, current Rust choices, and unresolved external-origin arithmetic. Its decision not to claim a general large-arrow origin rule is supported.

**Production fit-candidate contract: insufficient as currently stated for implementation plus all-passing held full-fixture acceptance.** There is enough evidence to motivate a separately named native projected-extent policy experiment, but two conditions must be resolved first: choose the prospective inside orientation without circular dependence, and acknowledge that the fit change alone cannot make complete PBCB pass. It would be misleading to approve full PBCB parity, omit its external text record, or treat primitive subsets as complete-fixture acceptance.

No new general external-origin implementation is justified by this audit. Keep DAR050/DAR200/PTEXT50 and mixed-history external-origin discrepancies explicit; no stored-coordinate lookup or widened tolerance can replace the missing formula.

## Independent verification

Read native `crates/acad-cmd/src/dimension.rs`, all29 manifest scripts and the complete-record comparator in `tests/dimension_native.rs`; retained supplemental README/native-facts, PBCB/PBCC input arrays, and the font-bounds plan under `.worktrees/dim-geometry`. Native comparator checks all ordered entity kinds/layers, LINE endpoints, four SOLID corners and TEXT origin/height/rotation/value at absolute1e-10; it also checks DIMARROW/units and DWG round trip.

A separate read-only Python walker parsed little-endian raw DWGs for DAR050, DAR200, PBCB, PBCC and PTEXT50 from offset0x202 using LINE/SOLID/TEXT record layouts. All ended exactly at header entity-end, with counts7,7,28,28,14 respectively. Independent SHA256 values matched the audit's three quoted values:
- DAR050: f49c96ff095109bb8d087e39656d006968688e45667f27644357b72c4d0b21ce
- DAR200: 0549d7f28ac9160371039056e513a8a467aa2c77f64139b4bb058466ae92247e
- PTEXT50: 67adfda3abae5274dbbe794e0d093c1088b077f8a043c3e5242b312003edf03d

Additional independently read hashes:
- PBCB: fe90b53d0aa7fa5cfcddf6253cbc642a33a63441bc52352bed0c474616328583
- PBCC: c31b3e92ec67a07c9b94c72382756c0bd1ede6092a556fb7e03b9cf3d090f8f1

Raw binary64 TEXT origins confirm DAR050(3.5,3.5), DAR200(5,8), both PTEXT50 dimensions(6.5,3.5), PBCB second(5.75,5.5), third(8.142857142857142,7.5), fourth(11,5.625). PBCB fourth SOLID tips at(12.5,4),(12.5,8) and bases at y4.5/y7.5 independently establish internal arrows.

Exact Fraction arithmetic independently confirms:
- DAR050 centered x=45/14, actual residual2/7.
- DAR200 centered x=-15/7, actual residual50/7.
- PBCB final label4.0000 at h3/4 and ink104/cap21 gives W26/7, current fit threshold47/7, projected-height candidate threshold15/4. Span4 passes candidate and fails current rule; span3 remains external under candidate.

The translation control supports a repeated residual at+3x and agreement between automatic/explicit labels in that state. It does not identify the generating arithmetic. The fit observation is one mixed-history branch bracket, not a measured continuous vertical threshold.

## Required contract clarification before a fit implementation

`dimension_geometry` currently computes:
1. internal = length >= width+6A;
2. horizontal = inside_horizontal_text if internal else outside_horizontal_text;
3. rotate and gap_width from horizontal and dimension axis.

Thus moving gap_width above step1 is not a defined operation: gap_width depends on the outcome it would decide. A reasonable **new native policy** is to test the prospective inside layout: use h for a vertical dimension line when inside_horizontal_text is true; use W otherwise, then choose the actual inside/outside rotation after fit. This is geometrically defensible but requires independent Y/N, N/Y, Y/Y, N/N tests in the threshold interval between h+6A and W+6A. Outside orientation must not inadvertently choose the inside comparator unless that alternative is expressly justified. Existing finite PORIENT long/short cases do not distinguish these policies near the threshold.

The existing Rust-only alphabetic fit test uses vertical span1.65/default A with horizontal text. Prospective-height policy changes its external/internal result. Such a local-policy expectation may change with an explicit explanation; retained native comparators must not change or shrink to accommodate the new policy.

## Acceptance boundaries

- Retain all29 native scripts and316 primitives at unchanged1e-10, full ordered comparisons and existing metadata/round-trip checks.
- Replay complete PBCB/PBCC from the original input arrays; report every remaining field discrepancy. Under the candidate, PBCB's second external B origin remains5.75 native vs79/14 centered. Therefore do not add complete PBCB to the passing native manifest or claim its full comparison passes after a fit-only change.
- A focused test of PBCB's final seven records may establish finite **branch-specific** support after replaying the whole history, provided its scope and the preceding origin failure are explicit. It cannot substitute for or be labeled full-fixture parity. A complete diagnostic comparison should still retain/report the mismatch.
- Independent geometry tests for the candidate should cover both axes, signs, inside/outside orientation combinations, equal/below/above proposed threshold, varying labels, and finite overflow rejection. These are Rust policy checks, not newly recovered parity.
- New full passing PBCB parity requires a separately evidenced origin rule. Full DAR050/DAR200/PTEXT50 remain blocked by that same unresolved class of placement behavior.

The audit needs no factual correction. Its phrase “moving/reusing gap_width” should be read as a research suggestion, not a complete implementation contract. Root can schedule a bounded fit-only policy milestone once the orientation and acceptance distinctions above are recorded; D1 alone does not approve that implementation.
