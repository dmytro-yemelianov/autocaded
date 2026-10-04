# D1 read-only DIM evidence audit

Date: 2026-10-04. Three bounded evidence passes: main arrow exports/current
native geometry; retained supplemental/font/translation observations; independent
raw DWG/hash/arithmetic checks. No implementation, evidence mutation, guest,
collector, translated runtime or tolerance change.

## Decision

Available observations do **not** establish a general original large-arrow
external TEXT origin algorithm. Keep DAR050/DAR200 out of passing native
geometry claims. A separate axis-aware arrow-fit candidate is suggested by a
held mixed-history case; it does not solve the external origin gap and should
not be implemented without a separately reviewed bounded contract.

## Sources and provenance

Native sources read in the private L1 copy: `crates/acad-cmd/src/dimension.rs`,
`tests/dimension_native.rs`, `tests/fixtures/dim/{README.md,txt-metrics.md}`,
DAR050/DAR200 DWG/DXF and current recovery plan. Graph discovery for acad-cmd
DIM symbols returned no matching nodes; scoped source/evidence reads followed.

Retained evidence root (read-only):
`/Users/dmytro/github/autorust/.worktrees/dim-geometry`.

- `docs/recovery/dim-geometry-2026-10-01.md` gives original scripts, geometry,
  provenance and unresolved hypotheses. Its historical initial red codec gate
  is not confused with the subsequently accepted fixture guards/current native
  tests.
- `crates/acad-oracle/tests/fixtures/dim/{DAR050,DAR200,DARBACK,DAINVAL}` supplies
  exact native script/snapshot/export evidence; replacement/invalid-setting
  cases repeat the same .5 geometry and are not new discriminatory geometry.
- `.../fixtures/dim-supplement/{README.md,native-facts.json,PBCB,PBCC}` supplies
  current-versus-prior arrow size and mixed B/C observations.
- `.../fixtures/dim-text-origin/{README.md,PTEXT50.inputs.json,
  PTEXT50.native-decoded.json,plans/font-bounds.json}` supplies translated,
  automatic-versus-explicit text and matching-header controls.
- `docs/recovery/dim-default-txt-metrics-2026-10-01/README.md`, derived labels,
  and `crates/acad-oracle/examples/dim-default-txt-metrics.rs` distinguish font
  metric derivation from native placement evidence. The offline script has no
  general native DIM-origin formula. Its census explicitly retains all 38
  exports/62 TEXT occurrences, including PTEXT50.

Read-only Python SHA256 checks matched the retained hash indexes for DPRIOR,
DAR025, DAR050, DAR200, PBCB, PBCC and PTEXT50. A separate direct little-endian
DWG walker read every LINE/SOLID/TEXT in those seven drawings and landed exactly
at each native entity-end field. TEXT origins below therefore come directly
from DWG binary64 values, not reduced-precision DXF or lossy JSON parsing.

DAR050 DWG SHA256:
`f49c96ff095109bb8d087e39656d006968688e45667f27644357b72c4d0b21ce`.
DAR200:
`0549d7f28ac9160371039056e513a8a467aa2c77f64139b4bb058466ae92247e`.
PTEXT50:
`67adfda3abae5274dbbe794e0d093c1088b077f8a043c3e5242b312003edf03d`.

## What the exports establish

DPRIOR/DAR025/DAR050/DAR200 all use first (1,1), intersection (5,1), second
(3,2), horizontal automatic `1.0000`. Dimension line is x5, span1; external
arrows. All retain four LINE, two SOLID and one TEXT, in that order, layer1.
Arrow length/overshoot A, half-width A/6, text height h=1.5A agree for the four
saved sizes. External TEXT y=2+3A agrees throughout.

Retained TXT has cap21 and `1.0000` ink bounds x0..100, advance106. Thus current
Rust ink width W=100h/21, and its horizontal external origin is x5-W/2.

| Case | A | Native TEXT origin | Rust centered x | Native x minus centered x |
| --- | ---: | --- | ---: | ---: |
| DPRIOR | 9/64 | (4.497767857142857,2.421875) | 4.497767857142857 | rounding-scale residual only |
| DAR025 | 1/4 | (4.107142857142857,2.75) | 4.107142857142857 | rounding-scale residual only |
| DAR050 | 1/2 | (3.5,3.5) | 45/14 = 3.2142857142857144 | 2/7 |
| DAR200 | 2 | (5,8) | -15/7 = -2.142857142857143 | 50/7 |

PTEXT50 translates DAR050's inputs by +3x while retaining A=.5, the same
LIMITS/view/header settings and the exact same TXT source. Both its automatic
blank and explicit `1.0000` dimensions produce the same complete seven-item
block, TEXT(6.5,3.5), height.75, rotation0. Centering predicts87/14; the residual
is again2/7. This excludes an automatic/explicit explanation in the observed
state and excludes a fixed absolute x-origin explanation. It does not recover
what original bounds/placement state caused the displacement. Its retained
translation inventory preserves one-ULP SOLID corner differences; no universal
exact translation invariance is claimed.

Mixed-history evidence rejects a universal fixed 3A x-offset as well as a
universal centered offset. At A=.5, PBCB external B with span3/label3.0000 has
x7.5 and native origin5.75 (effective half-width1.75), while centered x is
79/14=5.642857142857143. Its following external C with span2/label2.0000
centers correctly at57/7=8.142857142857142. PBCC's external C with that same
span/label centers at79/14. These labels have the same104-unit ink width.
History/orientation/placement branches are not interchangeable.

## Additional bounded fit gap and candidate

PBCB's final B has A=.5, h=.75, span4, horizontal `4.0000` on a vertical
dimension line x12.5. Native arrows are internal; the full four LINE/two SOLID
records establish this independently of TEXT placement. Native text origin is
(11,5.625), compatible with the existing internal baseline x-offset3A.

Current Rust decides every axis using `length >= W+6A`. Here W=26/7, so the
threshold47/7 exceeds span4 and Rust incorrectly chooses external arrows.
The retained same-A external B at span3 and internal B at span4 bracket a
branch; all horizontal-line threshold probes use glyph width and do not
resolve the vertical-line comparator.

A geometrically motivated candidate is to use the text's extent projected
along the dimension line: h for a vertical line with horizontal text, W for
horizontal lines or text aligned to a vertical line. Candidate threshold is
projected_extent+6A. For this held vertical case it is3.75, yielding internal;
span3 remains external. It preserves the established horizontal PFTLOW/AT/HI
width premise. This is compatible finite evidence, not a recovered universal
comparator: history and line position also differ, and no continuous vertical
threshold was measured.

The code already computes this projected quantity as `gap_width` after the
fit decision. Moving/reusing it for fit is a possible smaller future native
policy change, but should be reviewed independently and must compare every
ordered primitive and metadata field against held PBCB/PBCC cases. It does
not justify changing DAR050/DAR200's origin or claiming those cases pass.

## Missing facts and acceptance requirements

Missing original information is the construction/control flow around external
TEXT placement: which bounds are measured, where/when they are initialized,
how font scale and orientation enter, what conditional thresholds use A/span,
and whether B/C/history changes those intermediates. Documentation's static
OVL04:14B0 -> EXE:8DDF font-bounds call only confirms an X-bounds subtraction;
it does not supply the later placement branch or its scalar intermediates.
No reviewed general rule follows from fitting the finite saved coordinates.

A supported future origin derivation must come from retained static arithmetic
and branch/state evidence, with named intermediates and a falsifiable formula.
Use DAR050 for development and hold DAR200/PTEXT50 plus PBCB/PBCC suffixes out
for independent acceptance. Compare complete ordered DWG entities (all LINE
endpoints, all four SOLID corners, TEXT origin/height/rotation/value, layers and
counts) at the unchanged absolute1e-10 geometry tolerance. Keep all29 existing
native scripts/316 primitives passing. Repeated DARBACK/DAINVAL are useful
setting-history checks but cannot substitute for held geometry discrimination.

No coordinate tables, widened tolerances, rendered-pixel substitutes, partial
field comparisons or newly authorized guest collection are proposed. Until
reviewed static evidence supplies that rule, the precise gap remains original
large-A external text placement, plus the separately identified vertical fit
choice and nondefault mixed histories.
