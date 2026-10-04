# Native FILLET and view policy

This implements retained HLP FILLET283–296, PAN462–478 and ZOOM686–706.
The new interaction and numerical choices below are explicit Rust policies.
Existing measured ZOOM All device constants/anchor assertions and positive
FILLET fixture geometry checks remain strict. No original nonzero FILLET
export or generalized view/fillet output parity was measured for this work.

## FILLET

FILLET asks for two top-level LINE owners or R. R opens the radius prompt;
a valid finite nonnegative answer finishes the setting command. Return keeps
the current setting. The radius starts at zero, belongs to the drawing,
affects following FILLET operations, participates in dirty comparison and
UNDO, and appears as `fillet_radius` in API state. A changed R setting has
one undo entry; a no-op has none. Retry and cancel leave drawing data intact.
Mouse/window sets use the shared selector until Return; typed IDs finish
immediately. R during a collected selector discards that pending set and
opens the setting prompt. Unsupported owners/cardinality remain retryable.

Zero radius connects both lines at their infinite-line intersection and
creates no ARC. For an interior intersection, the existing ordered ray
policy retains the first line's end and second line's start. An external
intersection extends the nearer endpoint, retaining the farther endpoint;
an intersection already at an endpoint retains the other end. New generalized
ray choices are native policy, not evidence of original digitizer-side picking.

Positive radius places tangent points and a minor CCW ARC on those retained
rays. The old crossing-lines geometric test remains unchanged in precision.
Acute/obtuse and external intersection tests check radius/tangency, rather
than merely checking an ARC exists. Parallel or near-parallel normalized
cross products at or below 1e-12, zero/nonfinite lengths, numerically collapsed
positive geometry, nonfinite results and tangent distances reaching/passing
retained endpoints are rejected before mutation. Thus very large radii do
not reverse/collapse the remaining line segments. Valid geometry commits
both endpoints and its optional ARC atomically with one undo; a zero no-op
creates none. New arcs use the current layer; source layers remain intact.

### Radius persistence boundary

The independent retained-code contract establishes AC1.40 little-endian f64
at fixed-header offset 0x1fa, final descriptor 42 (runtime DS:48fe, size 8).
AC1.40 decode/write checks finite/nonnegative radius, and writing replaces
that identified slot even when other raw header bytes are preserved.
AC1.2 ends at 0x1d8 and lacks this field. The retained historical comma-DXF
header writer/reader has no radius token. These formats import native radius 0
and permit zero export; nonzero radius is checked-refused before file staging.
No new DXF token, entity-stream bytes, implicit revision upgrade or silent
clearing is introduced. Negative zero is geometrically zero and remains
representable; omitted formats do not retain its sign.

SAVE As `.dwg` chooses AC1.40 through the existing application policy; END
keeps the attached source format/revision and can therefore refuse AC1.2/DXF
when radius is nonzero. Failed saves preserve destination, drawing, attachment,
dirty baseline and undo. WBLOCK uses its existing AC1.40 writer and preserves
the setting without attaching the export. The API Drawing operation exports
historical DXF and reports the same checked nonzero-radius limitation;
State, native SAVE/END and WBLOCK remain available.

Evidence: [independent radius mapping review](superpowers/reviews/2026-10-04-fillet-radius-contract.md)
(records retained raw-file hashes, DS/header descriptor arithmetic and audited
reader/writer paths). All eight original AC1.40 sample headers have zero in
this slot; synthetic 2.5-byte tests do not claim measured original nonzero parity.

## ZOOM and PAN

Bare positive ZOOM number fits the canonical All view and divides its height
by that number. It resets the center to All's center even after PAN; ZOOM 1 is
identical to All. A suffix X divides the current height while keeping the
current center. Repeated numeric input is absolute; repeated X input composes.
Finite positive factors whose division overflows/underflows the usable spans
are rejected without changing the view or Previous history.

All keeps the retained measured device aspect 1.5223311546840959 and lower-left
anchor. Its box is the union of explicit LIMITS and compact visible geometry
bounds. With no visible geometry it retains the measured empty EXTENTS-at-origin
contribution. Hidden geometry, erased owners and unreferenced block definitions
are excluded. Stored hidden-inclusive EXTENTS are not substituted for this
visible traversal. Preserving the measured All fitting policy means it is
independent of arbitrary application window aspect; this distinction is explicit.

E fits compact visible geometry centered using the active drawing canvas
aspect. A single point uses height1; empty geometry is retryably refused.
W accepts corners in either order, fits their rectangle centered with height
`max(rectangle height, rectangle width / canvas aspect)`, and rejects a zero
axis. C uses a supplied center and height. L uses supplied lower-left and
height, computing center by half the canvas width/height. Standalone clients
default to the retained device aspect; `Editor::set_viewport_size` configures
a positive pixel canvas. Before this call standalone scale validation uses an
abstract one-pixel vertical canvas, without inventing original device pixels.
Session/API/GUI use physical client width and height
minus the persistent command area. Frame rendering itself remains read-only.

Bounds reuse the selection module's compact visible REPEAT/rotated INSERT
traversal: 100,000 stored visits, 256 stored levels and 16 INSERT levels; REPEAT
corners avoid enumerating cells. A global budget covers navigation across all
owners. Invalid/nonfinite or budget-exhausted traversal fails before a view
change. Circular ARC bounds remain conservative full circles; TEXT/SHAPE use
stored origin, and transformed block bounding boxes may overestimate rotated
nonrectangular geometry. Exact font/shape ink navigation bounds are residual.

PAN's first @relative point is a displacement, committed after Return at the
second prompt. Otherwise two points define `to - from`; a relative second
point is relative to the first. The view center changes by the negative of
that displacement, moving drawn geometry from the first screen location to
the second. Height stays unchanged. Absolute first points require a second
point; Return retries. Corner and navigation points bypass SNAP/ORTHO on
mouse/API routes. View-relative first ZOOM points use the current center;
PAN relative displacement uses zero as its origin. These reference/sign
choices are explicit native policy.

P swaps the current and previous views, so two P commands toggle them.
Only a successful final view changes Previous; intermediate prompts, errors
and cancel do not. Navigation retains the existing command policy of updating
saved view/dirty state without adding geometry undo entries. No new stored
viewport-aspect field is added to DWG/DXF.

Representability guard. Finite stored values alone do not establish a usable
view: a huge center can swallow its half span so both corners round to the
center (`ZOOM C 1e308,1e308 1`), and a tiny positive height can make the
pixel/world scale overflow (`ZOOM C 0,0 1e-308` on a 222-pixel canvas gives
`pixel_height / height = inf`, and `0 * inf` is NaN in rendering). One shared
check, `DwgView::validate_canvas(aspect, pixel_height)`, therefore requires:
finite center, positive finite height and half spans, finite rounded corners
strictly ordered `min < center < max` on both axes, and finite positive
pixels-per-unit and units-per-pixel for the given canvas. The editor applies
it to every final view before commit: C, L, W, E, A, absolute and relative
factors, P (the remembered view is revalidated for the current canvas) and
PAN. A refused view is an ordinary retryable error: current view, Previous,
prompt state and dirty/saved baseline are unchanged and no undo entry is
added. Measured ZOOM All constants are unchanged; A is only checked after
fitting. Standalone editors that never call `set_viewport_size` validate
against an abstract one-pixel vertical canvas, which is not an original
device measurement; any real canvas is checked again at the display seam.

The guard is a lower bound on usability, not a precision policy: corners a
few ULPs apart pass, so neighbouring pixels may still map to the same world
coordinate at extreme magnitudes. Points far outside the view can still
project to nonfinite screen coordinates; that is entity geometry, not view
validity.

Application seam. A stored view need not be usable for the current canvas:
a historical DXF without DWGVIEW parses with view height 0, a DWG may carry a
corrupt view, and a view accepted for a small canvas can become unusable after
a resize. Opening does not rewrite the stored view. For display, the Session
builds one viewport per frame/click from the drawing canvas (client width by
height minus the command area): the stored view through
`Viewport::checked_from_view` (the same `validate_canvas` check); if refused,
a fit of LIMITS (centered, height `max(LIMITS height, LIMITS width / canvas
aspect)`) under the same check; if that is refused too, a unit-height view at
the origin, which is valid for any positive canvas. Frame rendering,
left-click point mapping, pick tolerance and the crosshair preview all use
this same function, so a click maps through exactly the viewport that was
drawn. The fallback is display-only: it changes no drawing, stored view,
Previous, dirty state or undo history, and it writes no status message. The
frame, command line, prompt and screen menu therefore keep rendering.
Commands still start from the stored view, not the displayed fallback:
`numberX` and PAN keep the stored height and are refused while it is
unusable. The first `@` point of ZOOM C/L/W is relative to the stored
center; PAN's first `@` point is a displacement, and the second W corner is
relative to the first. ZOOM A, E, W, C, L or an absolute factor stores a
usable view. This fallback is a Rust display choice, not measured original
behaviour.
Viewport size is runtime context rather than drawing content.
