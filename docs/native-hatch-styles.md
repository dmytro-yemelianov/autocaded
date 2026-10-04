# HATCH island styles (N/O/I)

Status: HLP contract plus Rust geometry policy. No retained original export
exercises a style suffix, so none of this is measured original parity.

## Evidence (acad.hlp lines 308-329)

- Prompt: `HATCH  Pattern (name,style / U / ?): <enter name and style>`.
  The style is a suffix on the pattern name, separated by a comma.
- Styles: `N = Normal (the default)`, `O = Outermost area only`,
  `I = Ignore internal structure`.
- A standard pattern is then followed by scale and angle prompts. `?` lists
  the patterns; `U` defines a pattern on the fly (`docs/native-hatch-user.md`).

The HLP does not define how nesting is determined, how curves participate,
or what happens with touching or crossing boundaries. Those are Rust policy.

## Rust policy

- Reply syntax: `name[,style]`, case-insensitive, whitespace around either
  part ignored. A missing suffix is `N`. Any other suffix (empty after the
  comma, `X`, `OI`, ...) is rejected at the pattern prompt and returns to
  `Command` with the drawing unchanged, like an unknown pattern name. `U,style`
  and file patterns take the same suffix (`docs/native-hatch-user.md`).
- Boundary objects are unchanged: the selected LINE/ARC edges must form
  closed, unbranched loops; each positive-radius CIRCLE is its own loop.
- Nesting depth (`O` and `I`): for each sweep line, the depth of a segment is
  the number of selected loops enclosing it, toggled by each loop's own
  crossings. Crossings use one half-open vertex rule: a LINE edge counts a
  vertex on the sweep line only as its lower end, and an ARC is split at its
  extrema in the sweep-normal direction into monotone pieces that follow the
  same `[low, high)` rule. So a pass-through vertex (line/line or arc/line)
  crosses once, a local minimum twice and a local maximum not at all, and
  every loop has an even count. There is no per-loop merging of crossings.
  Crossings within 1e-9 (of any loops) toggle together before the depth is
  tested, so touching loops and a row that only touches a vertex never
  produce zero-length strokes or split a stroke. If a loop still has an odd
  count on a row (possible when a junction's two endpoints differ by more
  than the 1e-10 row snap but within the 1e-8 loop-closing tolerance), HATCH
  fails atomically with "boundary crossings are inconsistent" rather than
  emitting a corrupt row. CIRCLE loops keep the strict tangency rule (no
  crossing at the extremes), which is also even.
  - `N`: the existing parity rule, unchanged byte for byte. All crossings from
    all loops are merged within 1e-9, using the original closed-tolerance arc
    endpoint test, and then paired. For non-touching nested loops whose
    vertices avoid sweep rows this is odd depth.
  - `O`: depth exactly 1, the area between an outermost loop and its first
    level of islands. Deeper islands are left empty.
  - `I`: depth at least 1. Islands are ignored, and each run of segments is
    emitted as one span, so dashes and dots keep the global pattern phase.
- Disconnected loops: each outermost loop applies the style to its own
  islands independently, since depth is per point.
- Overlapping (crossing, not nested) loops are not rejected. They follow the
  same depth count: `O` hatches areas covered by exactly one loop and `I`
  hatches their union. This is a defined native rule, not evidence.
- Known pre-existing `N` limitations, kept unchanged because the retained
  original LINE/NET comparisons depend on this path:
  - Vertex rows: a concave local-minimum vertex lying exactly on a sweep row
    is merged into a single crossing, which can mis-pair that row. Arc/line
    junctions at an arc extremum on a row have the same problem. Example: an
    upper semicircle closed above by lines hatches the segment under the
    arc on the row through its endpoints.
  - Touching islands: an island touching the outer loop on a row (an
    inscribed diamond meeting a 4x4 square at (0,2) and (4,2), or a radius-2
    circle tangent inside it) merges crossings, so `N` hatches the full row
    `[(0,4)]` across the island. `O` and `I` handle these cases.
- Pattern families, scale, angle, signed dash phase, row drift, dots, the
  aggregate 100,000-stroke budget, the `*Xn` block on layer 127, and the
  single undo snapshot are shared with the unstyled path. Generation errors
  leave the drawing, undo history and document dirty state unchanged, and the
  selection prompt stays active for a retry or cancel.

## Contracts

`crates/acad-cmd/tests/hatch_styles.rs` and
`crates/acad-app/tests/api.rs::api_styled_hatches_share_the_command_route_budget_undo_and_files`:

- concentric three-level squares give different N/O/I spans;
- for all 23 patterns at scale 1/angle 0 and scale 2/angle 30, `O` over three
  levels equals the parity hatch of the outer two, and `I` equals the outer
  loop alone (order, dash phase, signed drift and dots included);
- `N` and a missing suffix equal the existing output for every pattern;
- rotated/scaled NET follows the boundary transform for each style;
- disjoint loops with islands, two-ARC and CIRCLE curved islands;
- O/I vertex rows: an island local-minimum notch on a row (counts twice), the
  mirrored local maximum (no crossing), an arc/line extremum junction
  (nothing hatched outside the region) and an arc/line pass-through junction
  (one crossing);
- invalid suffixes, budget exhaustion with retry and cancel, one UNDO, DWG
  and DXF save/open through the shared API. The `NET,I` budget cases fail at
  the shared pre-sweep or aggregate check. Separately,
  `api_outermost_stroke_budget_fails_atomically_where_ignore_fits` exhausts
  the budget inside the styled stroke path: `LINE,O` at scale 0.008 over ten
  islands fails atomically (drawing, undo and dirty state unchanged), while
  `LINE,I` on the same input fits with 12,000 strokes.

The retained original LINE/default NET comparisons (`hatch_native.rs`) and
the DIM fixtures are untouched and keep their strict tolerances.
