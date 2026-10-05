# Native DIM text placement

Contract for DIM text across a vertical dimension line (agentic row D2:
large-arrow external text). Original behaviour cited under "Evidence" was
measured on the original ACAD.EXE by the in-tree 8086/DOS runner and is
asserted in `crates/acad-oracle/tests/dim_arrows.rs`. That test skips visibly
without the extracted `System.img` and fails when `AUTOCAD_REQUIRE_CORPUS` is
set; a skipped run is not evidence. Retained exports replayed by
`crates/acad-cmd/tests/dimension_native.rs` are listed in the
[fixture README](../crates/acad-cmd/tests/fixtures/dim/README.md). Everything
under "Native policy" is Rust policy.

Notation: A = arrow size (DIM A, DIMARROW), h = 1.5A text height, W = TXT
ink width of the label at h. "X extensions" means the extension lines are
horizontal and the dimension line is vertical at x = L, the first origin at
x = F, the second at x = S, and sign σ = +1 when the extension lines run in
+x (F < L), −1 otherwise.

## Evidence: measured rule

Three measurement rounds (logs `target/agentic-artifacts/d2-round{1,2,3}.log`):

0. An unlogged reproduction run of the retained DAR025, DAR050 and DAR200
   scripts matched their saved records (x 4.107142…, 3.5 and 5).
1. Arrow sweep A = .3, .35, .4, .45, .6, .75, 1, 1.5 at first (1,1), line
   x5, second (3,2) with label `1.0000`: x is centred (5 − W/2) up to
   A = .4 and equals 3 + A from A = .45. Translation in x or y keeps the
   offset; ZOOM .25 and ZOOM 4 do not change it (no display dependence);
   S = 2, 4, 6, 8 give centred, 4.5, 4.5, centred.
2. Discriminators: a nearer first origin (F = 4, S = 1) decides instead of
   S; S = 5.25 (reach below zero) gives 5.25; A = 3 gives 6; text below the
   line (negative offset) keeps the x; σ = −1 (F = 9) gives 2.928571…, i.e.
   the shift reverses; Y extensions and 90° rotated text are not shifted;
   B and C after an external dimension follow the same rule.
3. Falsification: σ = −1 with labels `88888888`, `WWWW`, `1` and A = 2
   (the width enters only when σ = −1), F/S both near the line, a σ = −1
   B chain, internal arrows at span 7 (x = 3.5, so internal text follows
   the same rule).

**Derived rule.** For X extensions with horizontal (unrotated) text, internal
or external, let the reach R = min(|F − L|, |S − L|) − A. The text is centred
on the dimension line unless W/2 > R; the excess pushes the centre along the
extension direction:

    x = L + σ · max(0, W/2 − R) − W/2

The y coordinate and every LINE and SOLID record are unchanged from the
existing native geometry. Alternatives rejected by the oracle test
(`falsified_alternative_rules_contradict_original_results`): pure centring,
reach from S only, reach from F only, an unsigned push, and a left edge
clamped at S + A. The test also checks the derived rule on each observation.

Confirmations run in the first D2 pass, after the rule was fixed, all match
every record at 1e-10: the first three dimensions of retained `PBCB` (its B
origin 5.75, already known from the D1 audit, is R = (5.25 → 7.5) − .5 =
1.75); an internal dimension with R = 0 (F = 4.5); and a mirrored (σ = −1)
internal B chain at the default arrow size. The retained DAR050, DAR200 and
PTEXT50 exports now pass the replay, but are confirmations with known
targets, not held-out predictions (their geometry was in rounds 0–2). The
retained B "internal 3A offset" is this rule's case R = 4A − A, because B
starts at the previous first extension end, A past the old line, and
advances 5A.

**Held-out set (repair pass 1).** Added after the rule was fixed and after
an independent review: seven scripts absent from every measurement log and
from the review, whose predicted TEXT origins were written down
(`target/agentic-artifacts/d2-repair1-predictions.log`) and committed in
`HELD_OUT` before the original ran them. They cover A = .42 just below the
A = .4375 threshold, a second origin beyond the line with unequal reaches,
a 15-character label at σ = −1, negative coordinates (LIMITS first) with
text below the line, a second origin closer than A at σ = −1, internal
arrows with R = .6, and a σ = −1 B chain.
`held_out_original_matches_predicted_crossing_text` asserts the predictions
on both editors and full record parity at 1e-10.

`parity_original_crossing_text_matches_native_records` compares all ordered
LINE/SOLID/TEXT records (kinds, layers, points, TEXT origin, height,
rotation, value) for 54 scripts against the native editor at absolute 1e-10.

## Arrow fit

For X extensions with horizontal inside text the original keeps arrows
inside where the former native comparator `length ≥ W + 6A` did not: span
6.5 at A = .5 (`6.5000`, and round 2's span 6 `6.0000`) is internal with
text x 3.5, and retained `PBCB`'s final B (span 4, A = .5) is internal while
its span-3 B is external. So W + 6A is falsified for this orientation.

Native policy (owner decision 2026-10-05, from the D1 projected-extent
candidate and its contract review): fit compares the span with the
**prospective inside layout's** text extent along the dimension line, plus 6A:

    internal = length ≥ E + 6A
    E = h  for X extensions (vertical line) with inside horizontal text
    E = W  otherwise (Y extensions, or rotated inside text)

The inside/outside text orientation is chosen after the fit, so the outside
T setting never selects the comparator. This keeps the exact PFTLOW/AT/HI
W + 6A boundary for Y extensions, puts the bracket (3, 4] at A = .5 on the
right side (h + 6A = 3.75), and agrees with the original on spans 6 and 6.5
(`crates/acad-oracle/tests/dim_arrows.rs`,
`original_and_native_fit_inside_by_projected_text_height`). It is not a
recovered comparator: the equality case for X extensions, a continuous
vertical threshold and inside/outside T variants inside the bracket were not
measured. Rust policy tests cover both axes, both signs, all four T
combinations below, at and above the thresholds, varying labels and
non-finite sizes (`dimension.rs`, `fit_policy_tests`). Full `PBCB` now
replays in the passing set; `PBCC` keeps a separate two-record
extension-line gap ([fixtures](../crates/acad-cmd/tests/fixtures/dim/README.md)).

## Native policy

- The derived rule is implemented in `crates/acad-cmd/src/dimension.rs`
  (`crossing_text_x`). Y extensions and rotated text keep the existing
  centred placement; that matches every measured case.
- Fit uses the prospective inside text extent plus 6A ([Arrow fit](#arrow-fit)).
- Fonts: the [command matrix](native-command-matrix.md) lists no STYLE
  command among the recovered dispatcher names, and the in-tree runner
  mounts only the System disk's `TXT.SHP`. Other `.SHP` files on the Samples disk
  were not measured as DIM fonts. Labels use the TXT ink metrics; characters
  absent from TXT keep the numeric-width fallback with no parity claim.
- Oblique intersections keep the dominant-axis policy (retained `DBOBLIQ`);
  the push was measured only for orthogonal inputs.
