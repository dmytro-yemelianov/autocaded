# Dot HATCH definition coverage

Source: retained System/ACAD.PAT, SHA-256
`8ea70d7a5501ce829ff10d4846b009907a964b4057f1fdfe012df4f7f1c7a4d2`,
as in [continuous coverage](continuous-patterns.md). No new guest was run.
Production constants and tests do not require that ignored file.

Definitions transcribed unchanged:

```
*mudst,Mud and sand
0, 0,0, .5,.25, .25,-.25,0,-.25,0,-.25
*sacncr,Concrete
45, 0,0, 0,.09375
45, .066291261,0, 0,.09375, 0,-.09375
```

Zero entries emit POINT at the current cycle offset without advancing it.
Positive entries emit LINE and negative entries advance gaps. A repeat must
have a positive finite period, computed from nonzero absolute lengths times
scale. All-zero repeats and invalid numerical ranges fail explicitly.
Origins, signed row drift, scale and rotation follow the existing family engine.
Clipped intervals share a global phase across holes.

Dots use half-open boundary intervals (lower endpoint included, upper excluded),
with the sweep's existing 1e-10 boundary tolerance to stabilize rotated endpoint
projection. This is a Rust boundary contract, not an observed original rule.
Every POINT counts toward the shared 100,000-entity budget, alongside LINE;
overflow creates no partial block, INSERT or undo snapshot. Both types are stored
on layer 127 in the existing anonymous block and INSERT representation.

`hatch_dots.rs` checks mixed MUDST primitive order, negative row indices,
an independently enumerated SACNCR diagonal lattice, transformed ordered
geometry, DWG round trips, annular holes, phase and atomic UNDO/overflow.
Dash unit tests check zero-length emissions and negative cycles.
`acad-app/tests/api.rs` checks commands, visible dot pixels and black gaps,
PNG export, undo and API DWG/DXF save/reopen. DXF comparisons retain its
historical six-decimal precision. POINT uses the existing fixed-pixel plus
marker; no native screen style is claimed.

Neither dot pattern has a retained original export. POINT representation,
boundary inclusion and output ordering are therefore unverified against
original AutoCAD. These contracts cover the Rust interpretation of retained
data, not native parity. LINE/NET export regressions remain unchanged.
