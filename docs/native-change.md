# CHANGE: evidence and native contract

Status: I2. CHANGE point mode now follows the original's measured dialogue
for selections of several objects of mixed kinds. Every original behaviour
below is asserted by `crates/acad-oracle/tests/insert_change.rs` (in-tree
original ACAD.EXE, prompts read from its graphics frame with its own font,
drawings from its END); native-only rules are marked **native policy**.

## Retained help (ACAD.HLP lines 114-135)

`CHANGE Select objects or Window or Last: / Intersection point/L:`. Line:
the endpoint closest to the point moves to it. Circle: the radius passes
through it. Block: moves to it, new angle may be given. Text: moves to it,
new angle and text may be given. `L` asks a new layer for every object.

## Measured original behaviour

- The original's prompt after the selection is `Intersection point or (L):`.
- Selected entities are **visited in reverse drawing order**. LINE and CIRCLE
  change silently. Every TEXT asks `New height:`, `New angle:`, `New text:`
  in turn. All INSERTs share **one** `New angle:` prompt, asked when the first
  INSERT is visited (also for INSERTs of different blocks); every INSERT
  takes that angle, and a blank answer keeps each INSERT's own angle.
- A blank intersection point keeps every location (lines, circles, inserts
  and texts stay) while the property prompts are still asked.
- A blank height/angle/text answer keeps the old value. A non-blank text
  answer — even the old value — erases the TEXT record in place (the erased
  record keeps the staged origin/height/angle and the *old* value) and
  appends a new TEXT with the new value at the end of the drawing; several
  appended texts follow the visiting order. Without a new value the TEXT is
  rewritten in place.
- ARC, POINT, TRACE and SOLID in the selection are left unchanged; no error.
- OOPS after CHANGE never revives a TEXT record that CHANGE erased; it still
  restores the set of the previous ERASE (also with the CHANGE in between).
  The native editor matches (OOPS uses only the last ERASE set).
- A REPEAT member line inside the window is edited like a top-level LINE.

## Native contract

- Prompts, visiting order, the shared INSERT angle, blank answers and the
  TEXT erase-and-append follow the measured original (the native prompt
  wording is `CHANGE: new angle`, `CHANGE TEXT: new height/angle or point/new
  text`, each "(Enter keeps current)").
- **Atomic staging (native policy)**: every answer is staged on full
  replacement records; nothing changes until the last prompt. A rejected
  answer keeps its prompt; cancel discards everything. The commit verifies the
  sources are unchanged and is one UNDO step; a CHANGE that changes nothing
  adds no undo step and leaves the document clean.
- **Answers**: angles accept a number or a point. An INSERT angle point is
  measured from the intersection point, or — after a blank intersection
  point — from the first-visited INSERT's origin (measured on the original);
  a TEXT angle point from the TEXT's staged origin (native policy); `@` relative intersection points use the first selected entity's
  anchor. Height must be positive and finite. A new value is measured in the
  font context of its append position (where the record is stored), so a
  glyph only available before a later LOAD is refused.
- **Groups (native policy, selection contract)**: a selection containing a
  REPEAT group (either root representation) is refused before any change,
  unlike the original, which edits the member lines. `CHANGE L` keeps the
  whole-owner layer rule for groups.
- Selection: the native selector's window keeps collecting until Return
  (docs/native-selection-contract.md); the original completes `W` at the
  second corner. The oracle tests therefore select with `ALL` natively.
- Only-unchanged-kind selections finish with a status message and no undo.
- API and MCP use the same prompts through the `command` route; the result
  (erased record plus appended TEXT) round-trips through AC1.2 and AC1.40 DWG.
  DXF carries live records only, so the erased TEXT is not written there.

## Limitations

- REPEAT member editing (original) is refused natively.
- SHAPE and LOAD records are treated like the other unchanged kinds; the
  original was not measured with SHAPE records in the selection.
- The original's selection messages (`4 found.`) and window completion at the
  second corner are not reproduced.
- ORTHO is not applied to the LINE endpoint change (unchanged from before).
