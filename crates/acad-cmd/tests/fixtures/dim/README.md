# Retained native DIM fixtures

These are untouched DWG exports and exact input arrays copied from
`recovery/dim-geometry` at `d6e06f01e0cef98ff283ae4574f215f4a2fb0fec`.
The original bundles remain in that worktree under
`crates/acad-oracle/tests/fixtures/{dim,dim-supplement,dim-text-origin}`. `manifest.json`
records each source bundle and SHA-256. No guest or new collector was run.

`dimension_native.rs` replays the 32 scripts through the native Rust Editor,
then reads the original DWGs with `acad-dwg`. It compares all 344 ordered
entities: kinds, layers, every LINE endpoint and SOLID corner, and TEXT
origin, height, rotation and value. Coordinates use the fixed absolute
1e-10 geometry tolerance, independently of six-decimal DXF serialization.
No entity or field is omitted from a passing case.

The cases cover default and A=.25 dimensions, A=7/32 fit boundaries, TXT
numeric/punctuation/W/i ink widths, independent inside/outside T settings,
blank defaults, B chains, C chains, reflected and translated inputs, C after
internal arrows, ordinary LINE between dimensions, and decimal precision.
The additional local history test checks UNDO, cancellation and opening a
drawing without fabricating DIM session history from its primitives.

The large-arrow cases `DAR050` (A=.5) and `DAR200` (A=2) and the translated
`PTEXT50` (copied with its inputs from the retained `dim-text-origin`
bundle) joined the replay after the crossing-text rule was measured
([native DIM](../../../../../docs/native-dim.md)). They are retained-export
confirmations, not held-out predictions: the DAR050/DAR200 text origins were
already known from the D1 audit, and the DAR050 and PTEXT50 geometry was
re-measured on the in-tree original in rounds 1 and 2. `DPRIOR`, `DAR025`, `DAR050`, and `DAR200` DWG/DXF pairs also
supply the DWG/DXF codec regression tests.

## Implementation limits

Recovered geometry uses arrow length/overshoot A, half-width A/6, text height
1.5A, and stroke bounds from the retained default TXT metrics (cap height21).
The exact representable PFTLOW/AT/HI cases establish the tested width+6A
boundary for horizontal dimension lines, including the internal result at equality. Generalizing that
comparator and dominant-axis selection beyond the sampled inputs is local
Rust policy, not a proof of every original DIM path.

Horizontal text across a vertical dimension line follows the measured
crossing-text rule x = L + σ·max(0, W/2 − R) − W/2 with reach
R = min(|F − L|, |S − L|) − A (see [native DIM](../../../../../docs/native-dim.md)).
It explains DAR050/DAR200/PTEXT50 and the B chains' internal 3A offset
(R = 3A there). The original arrow-fit comparator for vertical dimension
lines with horizontal text is not W+6A and remains undetermined, so the
retained PBCB/PBCC histories are not in this replay set.

Retained native text observations cover numeric/punctuation glyphs, E, W and i.
DIM now has derived horizontal metrics for all 94 printable characters defined
by default TXT.SHP, checked offline against the renderer's complete ordered
strokes for all 8,836 defined character pairs. See [metric coverage](txt-metrics.md).
Characters absent from TXT keep the numeric-width fallback. Other fonts, arbitrary
directions, native style persistence and unobserved lifecycle paths remain
outside the comparison claim. The local UNDO policy restores geometry and DIM history while
leaving session style settings intact.

DIMARROW is written to the drawing header when a dimension is committed;
A/T setting dialogs alone leave the drawing and undo stack unchanged.
AC1.40 and original DXF support the field. AC1.2 has no recovered mapping
and rejects an explicit DIMARROW rather than silently dropping it.
