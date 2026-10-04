# Native circular ARRAY and BREAK contract

This note selects the native Rust contract for task B2 (circular ARRAY options,
BREAK of TRACE and BREAK's point-picking dialogue). It separates three kinds of
evidence:

- **HLP**: retained `ACAD.HLP` lines 55-70 (ARRAY) and 102-113 (BREAK), shipped
  as `crates/acad-cmd/resources/acad.hlp`.
- **Strings**: prompt and message strings retained in `ACAD.OVL`
  (`strings corpus/System/ACAD.OVL`).
- **In-tree runs**: the original `ACAD.EXE` executed by the in-tree 8086/DOS
  runner (`acad_oracle::generate_dwg_in_tree`) on the read-only System floppy,
  with the saved DWG decoded by `acad-dwg`. Every in-tree value cited below is
  asserted in `crates/acad-oracle/tests/array_break.rs`: `original_*` tests
  compare the original with the Rust editor, and `divergence_*` tests pin the
  original's result where the native editor deliberately differs. Like the rest
  of `acad-oracle`, they print a skip notice and pass when the floppy image is
  absent.

Where the original's measured behaviour is a screen-dependent or apparently
accidental artefact, the native editor deliberately differs and says so below.

## ARRAY, circular

HLP: "you must supply a center point, the angle between items, and the number
of items (or the angle to be covered). If only one object is being replicated
and that object is a Block, it may optionally be rotated about its insertion
point at each step."

Strings: `Center point of array:`, `Angle between items (+=CCW, -=CW):`,
`Number of items or -(degrees to fill):`, `Rotate block as it is copied?`.

Native dialogue (prompts are Rust spellings of the retained strings):

1. `ARRAY: center point`
2. `ARRAY: angle between items (+=CCW, -=CW)` — a finite angle with
   `0 < |angle| <= 360`. Zero and `|angle| > 360` are rejected with the prompt
   kept for retry. (In-tree: the original aborts the command for `0`, `400`,
   `-400`; `360` is accepted and yields coincident copies.)
3. `ARRAY: number of items or -(degrees to fill)`:
   - a positive integer `n` is the item count, the original included;
   - a value `v <= 0` is an angle to cover, `fill = -v`:
     - `fill == 0` or `fill == 360` (exact full circle) gives
       `round(360 / |angle|)` items — the full circle is **not** endpoint
       inclusive, so the last copy never lands on the original;
     - any other fill is endpoint inclusive: `round(fill / |angle|) + 1` items.
       Rounding is half-up (`45/90` gives 2 items, `44/90` gives 1).
       Fills above 360 are accepted and overlap (`1080` at `90` gives 13).
   - The count of 1 (or a fill that rounds to one item) creates no copies and
     records no undo step (the same holds for a 1x1 rectangular array).
   - Non-integer positive counts are rejected for retry (the original aborts).
   - The existing 100,000-entity output bound applies to the final count times
     the selection size and is checked before any allocation or undo entry.
   - The angle's sign gives direction (positive CCW); the fill only gives the
     magnitude.

   In-tree anchors at `angle = 90`: `-360 -> 4`, `-270 -> 4`, `-180 -> 3`,
   `-100 -> 2`, `-90 -> 2`, `-46 -> 2`, `-45 -> 2`, `-44 -> 1`, `-1 -> 1`,
   `-135 -> 3`, `-134 -> 2`, `-359 -> 5`, `-361 -> 5`, `-720 -> 9`,
   `-1080 -> 13`, `0 -> 4`, `-0 -> 4`; full circle at `100 -> 4`, `80 -> 5`,
   `70 -> 5`, `135 -> 3` (both `0` and `-360`); `angle = 360` with `3` gives
   three coincident items and with `0` one item; `angle = -90` runs clockwise.
4. Only when the selection is exactly one top-level INSERT:
   `ARRAY: rotate block as it is copied? <N>`. `Y`/`YES` rotates, `N`/`NO`/Return
   does not. Other answers are rejected for retry (the original treats any
   non-`Y` answer as No). A rotated copy `k` has its insertion point rotated
   about the center by `k * angle` and `rotation = (source + k * angle) mod 360`
   in `[0, 360)` (in-tree: `10 + 3*90 -> 280`, `10 - 90 -> 280`,
   `10 + 3*120 -> 10`). Multiple objects, a REPEAT group, or any non-INSERT
   single object never see the question (in-tree: after a two-object window
   array the next line runs as a new command).

Unchanged retained evidence: without rotation, copies are translated so the
anchor moves around the center while orientation is preserved
(`original_circular_array_rotates_copies_around_its_center`, LINE `5,3 6,3`
about `4,3`). REPEAT groups keep the translation-only policy and anchor at the
first geometric member; groups without a geometric anchor are refused before
mutation. One `UNDO` reverses the whole array. Copies keep their source layer.

## BREAK

HLP: "Point to object: <point> / Enter second point: <point>. A line, trace,
or arc can be broken into two lines, or one end can be cut off. A circle will
be changed into an arc by deleting the portion from the first point to the
second, going counterclockwise."

Strings: `Select object:`, `No object found`, `Can't break a block`,
`Need a line, trace, circle, or arc`, `Object can't be broken`,
`Enter first point:`, `Enter second point` + ` or F`.

### Dialogue

- `BREAK: point to object, or one entity number`.
  - A typed `x,y` point (or a mouse/API pick) selects the nearest live, visible
    top-level object within the pick aperture. A typed point uses an aperture of
    1% of the effective view height (the stored view height, else the LIMITS
    height, else one unit); mouse picks use the session's 6-pixel aperture; API
    `point` uses an exact (zero) aperture. The original aperture is
    screen-dependent and is not modelled. Erased objects, objects
    on OFF layers, block definitions and LOAD metadata are never picked.
    A miss reports `No object found` and keeps the prompt (the original aborts).
  - The pick point, projected onto the object, becomes the first break point.
    The dialogue continues at `BREAK: second point or F (first point)`; `F`
    asks `BREAK: first point` and then `BREAK: second point`.
  - A picked INSERT is refused (`Can't break a block`), a picked REPEAT group
    is refused (groups are never broken), and other kinds are refused with
    `BREAK needs a LINE, TRACE, CIRCLE or ARC`; the prompt is kept.
  - Because BREAK takes exactly one object, a typed comma pair at this prompt
    is a point, never an ID list. A single entity number, `L`/`LAST`, or a `W`
    window / collected-pick selection finished with Return keeps the existing
    native route: `BREAK: first point`, then `BREAK: second point` (kind
    validation happens at the second point, so the existing atomic group
    rejection is unchanged). The shared selector hands its collected owners to
    BREAK as IDs, never as re-parsed text, so a two-object selection is rejected
    with `BREAK requires exactly one entity` and the selector kept.
- Cancel at any BREAK prompt leaves the drawing and undo history unchanged.
  Rejected points keep the prompt for retry.

### Geometry

Both points are projected onto the object (in-tree: `7,1` on the LINE `0,0 10,0`
cuts at `7,0`; circle points are projected radially).

- **LINE**: projection parameter `t` is clamped to `[0, 1]`. Interior points
  remove the span between them; a point at or beyond an endpoint cuts that end
  off. Point order does not matter.
- **ARC**: positions are measured counterclockwise from the start angle. A
  position outside the sweep snaps to the nearer arc end (a tie goes to the end
  angle, as in the in-tree `270°` probe on a `0..180` arc).
- **CIRCLE**: the counterclockwise span from the first to the second point is
  removed; the retained arc runs from the second point to the first.
- **TRACE**: the stored corner order is the TRACE command's winding: `p1/p2` are
  the start edge, `p3/p4` the end edge, and the sides run `p1 -> p3` and
  `p2 -> p4`. Break points are projected onto the centerline from the start-edge
  midpoint to the end-edge midpoint and clamped like a LINE. Each cut is the
  perpendicular to the centerline through that projection, intersected with both
  sides. This reproduces the in-tree results for rectangular traces and for cuts
  outside a mitered end. The native editor **rejects atomically**:
  - traces whose corners are not in that winding (sides crossing, `p1`/`p3` not
    on the same side of the centerline, a start or end edge not crossing it,
    degenerate centerline or a side that does not advance along it);
  - a cut that falls inside a mitered end, where the perpendicular misses a side
    (the original clamps and emits a self-touching quadrilateral, e.g. the
    in-tree `3,1.25 / 6.9,1.25` probe).
  SOLID is not breakable. Picking measures TRACE and SOLID outlines in
  their stored winding (`p1-p2-p4-p3`); the earlier pick helper treated
  `p2-p3`/`p4-p1` diagonals as edges and missed the long sides.
- A span covering the whole LINE, ARC or TRACE (both points at or beyond the
  ends) erases the object, as the original does (in-tree: `0,0 10,0`,
  `10,0 0,0` and `0,0 12,0` on a LINE, the full ARC, and the full TRACE all
  leave only an erased record).
- Coincident break points are rejected without mutation. The original leaves a
  LINE unchanged, splits an ARC into two touching arcs and turns a CIRCLE into a
  full-circle ARC (`start == end`); the native editor reproduces none of these.

### Record policy (matches in-tree output)

- Two remaining pieces: the start-side piece replaces the record in place and
  the end-side piece is appended.
- Only the start-side piece remains (end cut off): replaced in place.
- Only the end-side piece remains (start cut off, including a break starting
  exactly at the first endpoint): the original record becomes an erased record
  and the remainder is appended (in-tree: `0,0 5,0`, `5,0 0,0`,
  `2,0 F 0,0 5,0`, `2,0 0,0`; a right-to-left line trims from its own start).
- No piece remains (whole span): the record becomes an erased record and nothing
  is appended.
- CIRCLE: the circle becomes an erased record and the arc is appended.

All pieces keep the source layer. One `UNDO` restores the original record list.
BREAK's erased records use the same erased-record model as ERASE but are not an
ERASE set: `OOPS` keeps restoring only the last ERASE (in-tree: `OOPS` after a
whole-span BREAK restores nothing; `ERASE L`, a whole-span BREAK of another
line, then `OOPS` restores the ERASEd line and leaves the broken one erased). Erased records persist through DWG save/reopen; DXF
carries only live entities, so a DXF round trip compares live geometry.

### Measured original artefacts not reproduced

- LINE: a second point whose X lies below the line's X range leaves the line
  unchanged (in-tree: `2,0 -1,0` and `8,0 -1,0` on `0,0 10,0`, and `2,0 -1,0`
  on `10,0 0,0`), while a point beyond the larger-X end trims normally. The
  native editor projects and trims either end symmetrically, as HLP describes.
- Coincident points (above), the inside-miter TRACE quad, and the aborts on a
  missed pick, rejected ARRAY angles/counts and non-`Y` rotate answers (above).

## Regression anchors

- `crates/acad-cmd/tests/array_break.rs`: dialogue, arithmetic, bounds,
  retry/cancel, UNDO (including no-op arrays), groups, layers, picking
  visibility, selector-to-BREAK ID handoff, whole-span erase and OOPS, TRACE
  geometry.
- `crates/acad-app/tests/array_break.rs`: Session/API route, collected
  selection handoff, cancel, one UNDO, DWG and DXF round trips.
- `crates/acad-oracle/tests/array_break.rs`: every in-tree value cited above,
  as original-vs-Rust comparisons or pinned divergences.
- `crates/acad-cmd/tests/editor.rs`: the former off-object/endpoint/outside-sweep
  BREAK rejections now assert the measured projection and end-trim results.
