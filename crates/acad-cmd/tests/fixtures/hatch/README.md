# Retained HATCH exports

The DWG/DXF files and exact editor input arrays are copied unchanged from
the preserved `recovery/hatch-pattern` collection. Source commit, local
collection path and SHA-256 values are recorded in `manifest.json`.
No new guest or collector was run.

`HNETDEF` is the original default NET hatch of a rectangle from(1,1) to(5,5).
It has four boundary LINEs, an anonymous `*X1` block with64 layer127 LINEs,
and a layer1 INSERT at origin with unit scales and zero rotation. The first32
strokes are horizontal, then32 vertical. Each family uses spacing1/8 and
world-origin phase. Sweeps start at the projected boundary center, advance
along the family's normal, then return below the center. This explains the
opposite coordinate order of the vertical family rather than sorting strokes.

`HLINCIRC` is the original LINE hatch of concentric circles (3,3), radii2/1.
Its block has46 strokes, including the segments around the inner hole.

The offline command test compares every ordered item, block name/base, stroke
endpoint/layer, boundary circle and INSERT field with the retained DWG using
fixed absolute1e-10 coordinate tolerance. Generated drawings are written and
opened again and compared to the same geometry. Re-encoding the original
DWGs must also reproduce their entire byte arrays exactly. The original DXFs
are retained as source evidence; this test does not assert that Rust-created
drawing headers match the original's saved extents or DXF text byte for byte.

Separate Rust tests verify scale2/angle30 by transforming the original NET
geometry, clipping both families around circular holes, open-boundary
rejection, safe grid-index bounds, and the aggregate100,000-stroke limit,
including strokes split by holes. Failures add no partial block or undo step.
These are Rust geometric/behavioral checks, not new original-app observations.

Only default rectangular NET has direct retained native coverage here. The
other preserved NET variant requests were not executed and are not evidence.
GRATE, NET3, PLAST, PLASTI and STEEL are implemented from retained continuous
PAT definitions and checked separately in `hatch_continuous.rs`. They are not
additional native export fixtures; see [definition coverage](continuous-patterns.md).
The 14 listed dash/gap patterns without dots also have definition-based Rust
coverage; see [dash coverage](dashed-patterns.md). No native exports were added.
MUDST/SACNCR dots also have definition-based Rust contracts in
[dot coverage](dot-patterns.md), including API, rendering and DWG/DXF workflows.
All 23 catalogue patterns have geometry; external PAT loading and styles remain open.
The continuous-family implementation reuses LINE's existing closed
LINE/ARC-loop and circle clipping; no native parity claim is made for unmeasured
NET angles, scales, boundaries or style combinations.
