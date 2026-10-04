# Retained native DIM fixtures

These are untouched DWG exports and exact input arrays copied from
`recovery/dim-geometry` at `d6e06f01e0cef98ff283ae4574f215f4a2fb0fec`.
The original bundles remain in that worktree under
`crates/acad-oracle/tests/fixtures/{dim,dim-supplement}`. `manifest.json`
records each source bundle and SHA-256. No guest or new collector was run.

`dimension_native.rs` replays the 29 scripts through the native Rust Editor,
then reads the original DWGs with `acad-dwg`. It compares all 316 ordered
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

`DPRIOR`, `DAR025`, `DAR050`, and `DAR200` DWG/DXF pairs also supply the
DWG/DXF codec regression tests. DAR050 and DAR200 are **codec fixtures only**:
they are not included in the passing geometry replay manifest.

## Implementation limits

Recovered geometry uses arrow length/overshoot A, half-width A/6, text height
1.5A, and stroke bounds from the retained default TXT metrics (cap height21).
The exact representable PFTLOW/AT/HI cases establish the tested width+6A
boundary, including the internal result at equality. Generalizing that
comparator and dominant-axis selection beyond the sampled inputs is local
Rust policy, not a proof of every original DIM path.

The original large-arrow external TEXT origins remain unexplained. At A=.5,
DAR050's native origin is (3.5,3.5), while stroke centering gives
(3.2142857142857144,3.5); A=2 and the mixed nondefault B/C histories also
need the missing placement rule. Rust uses stroke centering in that domain
and does not claim native parity there. No coordinate lookup table or wider
tolerance substitutes for the missing algorithm.

Retained native text observations cover numeric/punctuation glyphs, E, W and i.
DIM now has derived horizontal metrics for all 94 printable characters defined
by default TXT.SHP, checked offline against the renderer's complete ordered
strokes for all 8,836 defined character pairs. See [metric coverage](txt-metrics.md).
Characters absent from TXT keep the numeric-width fallback. Other fonts, arbitrary
directions, native style persistence and unobserved lifecycle paths remain
outside the comparison claim. B's internal horizontal text offset of3A is
observed in the retained default-style baseline chains; other B styles are
not verified. The local UNDO policy restores geometry and DIM history while
leaving session style settings intact.

DIMARROW is written to the drawing header when a dimension is committed;
A/T setting dialogs alone leave the drawing and undo stack unchanged.
AC1.40 and original DXF support the field. AC1.2 has no recovered mapping
and rejects an explicit DIMARROW rather than silently dropping it.
