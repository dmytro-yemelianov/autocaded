# Dash/gap HATCH definition coverage

The retained ACAD.PAT identity is the same as in
[continuous coverage](continuous-patterns.md). The constants in `hatch_pattern.rs`
preserve definition row order, angles, world origins, perpendicular spacing,
parallel drift and every signed dash/gap length for these existing catalogue names:

EARTH, ESCHER, FLEX, GRASS, HEX, HONEY, HOUND, INSUL, SQUARE, STARS, SWAMP,
TRANS, TRIANG and ZIGZAG.

The application now has geometry for all 23 listed patterns. MUDST and SACNCR
have zero-length dot entries covered separately in [dot coverage](dot-patterns.md).
Their POINT representation is a Rust contract without a retained native export.
No catalogue expansion or external
PAT loader was introduced; the app and tests run without the ignored corpus file.

For signed row index `n`, the sweep's normal offset is
`scale * (dot(origin,definition_normal) + n * spacing)`. Its along-line dash
phase is `scale * (dot(origin,definition_direction) + n * drift)`.
Command angle rotates origin and both axes together about world (0,0).
Positive entries draw, negative entries advance through gaps. The cycle length is
the sum of absolute entry lengths, multiplied by scale. Phase is shared by every
clipped interval on that row, so a hole does not restart the sequence. Continuous
rows within a dashed pattern still emit full clipped intervals.

Definition family order and center-out sweep order reuse the LINE/NET policy;
dash strokes advance along each family's direction. Boundary intervals clip
partial strokes at either end. Negative indices are supported for rows and cycles.
The existing 100,000 emitted-entity budget counts every dash across all families
and split intervals. Separate sweep and cycle-span work limits prevent excessive
enumeration. Dash cycle indices must lie strictly between -2^53 and 2^53 to
preserve adjacent integer indices. Nonfinite/underflowed dash lengths and lost
precision while accumulating a cycle fail explicitly. Failures add no block,
INSERT or undo snapshot.

`hatch_dashed.rs` checks complete ordered SQUARE/TRANS/INSUL strokes; signed EARTH
row drift; ZIGZAG's translated vertical origin and drift; partial dashes and phase
through an annular hole; all 14 patterns under scale 2 / angle 30°, DWG save/reopen,
annular clipping and UNDO; and atomic stroke-limit, cycle-index and work-limit
failures. Dash-engine unit tests cover leading gaps, negative cycles, multiple
unequal positive lengths and invalid numerical ranges. Existing continuous and
retained-export HATCH regressions remain required.

These 14 patterns have no retained native exports. Tests validate the Rust
interpretation of the file definitions, not original output order, floating-point
behavior, style semantics or complete native parity. No guest was launched.
