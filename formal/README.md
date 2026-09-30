# AutoCAD behavioral model

This Lean 4 package records observable contracts for the native Rust rebuild.
It does not claim to reconstruct the original program's source-level design.
The current Ghidra report cannot map table entries to command handlers or
recover the DWG entity record as a coherent structure. The WBLOCK model is
therefore a specification of observed behavior and Rust refinement obligations.

Run it with:

```sh
cd formal
lake build
```

`AutoCAD.Wblock` models the prompt order and output snapshots for whole-drawing,
named-block, and selected-entity WBLOCK. Names are represented by normalized
numeric symbols after input parsing; paths are opaque tokens. Examples checked
by Lean cover transitive block closure, erased records, orphan definitions,
selection numbering, base-point handling, and each prompt transition.

`AutoCAD.Dblist` models the read-only traversal of live top-level, block, and
repeat entities for DBLIST. QEMU evidence currently pins the original's
text-mode switch and presence of a LINE record; full output formatting and
pagination remain open.

`AutoCAD.HelpFiles` models the observed command-list/help prompt, the exact
captured LINE help page, MENU cancellation flow, and FILES menu entry. Rust
parses `.MNU` labels and preserves macro bytes, but menu rendering and click
dispatch remain open. Rust implements FILES listing, deletion, and rename
against mapped host directories; the Lean model does not specify their
filesystem semantics. Other named help pages remain open.

`AutoCAD.Geometry` records the observed states for `DIM`, `HATCH`, and
`SKETCH`. DIM takes first extension origin, dimension-line intersection,
second extension origin, then text; the observed linear flow writes ordinary
LINE, SOLID, and TEXT entities. HATCH lists pattern names, then asks for the
pattern scale, angle, and boundary objects. The LINE pattern's clipped output
for closed LINE/ARC loops and circles is represented as an anonymous block and
insert, verified against native DWG for default settings, scale 2 / angle 30°,
and circle boundaries, a nested line-loop hole, and an upper semicircle closed
by a diameter. Other listed patterns remain outside this model. SKETCH takes a record increment before awaiting a digitizer, so it
remains dependent on an input device.

The original behavior still needs the QEMU oracle. Lean proves that the Rust
implementation matches this model only when the behavior is represented here;
it cannot supply missing facts about AutoCAD by itself.
