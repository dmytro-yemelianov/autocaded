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
parses `.MNU` labels and preserves macro bytes. The app implements panel
rendering, repeat-boundary pagination, and plain-text mouse macros; a QEMU differential case compares a real native click with Rust
dispatch. This Lean model does not prove mouse dispatch or pagination.
`AutoCAD.MenuControls` adds finite, artifact-linked observations for immediate
Snap/Ortho toggles, pending typed geometry, menu Cancel retention and buffers,
REPEAT opener retention, MENU blank unload versus Cancel, and GO's Return-like
history/buffer behavior. Its partial lookup returns `none` for unrecorded inputs;
that means uncertified, not a native error/no-op. Decimal coordinates and spacing
are symbolic tokens; Rust tests must establish refinement separately. The
[implementation brief](../docs/recovery/2026-10-01-menu-controls/implementation.md)
pins actual staged fixture names and exact expected results; accepted Rust tests
refine these finite cases, including actual parsed app entries and the shared
Return/GO path. Snap/Ortho change flags and preserve pending typed prompts, while
typed coordinates in the observed cases bypass constraints. Menu Cancel clears
app input and pending command state while retaining completed entities, repeat
marker, loaded panel and page. GO shares physical Return submission: it submits
the current buffer once and repeats observed MENU/POINT/LINE histories; empty
MENU filename unloads and Cancel retains the panel. These Rust tests do not prove
native mouse dispatch generally or desktop/DPI behavior.
The native GO byte mechanism, true blank-panel hit mapping, general command
history, and mouse Snap/Ortho projection remain open. `submit_mouse_point` is
unchanged; exact ties, origin, negative rounding, arbitrary spacing, axis choice,
projection order, and view rules are unrecovered. CREPEAT has prompt/native-DXF evidence only: no full decoded group/header.
Rust implements FILES listing, deletion, and rename
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
remains dependent on an input device. That `unsupportedDevice` outcome is
superseded: a QEMU mouse oracle shows that the original accepts a mouse, and
`docs/native-sketch.md` specifies the implemented mouse SKETCH. The Lean model
still covers only the increment prompt.

The original behavior still needs the QEMU oracle. Lean checks the stated model's
obligations. No Rust refinement proof is supplied by this package; Rust regression tests establish correspondence separately. Lean
cannot supply missing facts about AutoCAD by itself.
