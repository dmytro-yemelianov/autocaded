# External drawing INSERT: evidence and native contract

Status: B1 candidate; I2 adds star-form scale/rotation evidence and policy.
`INSERT name` / `INSERT *name` now accept a drawing file when `name` is not a
block of the current drawing. This document separates the retained help-file
evidence from the Rust-native policy chosen where the evidence is silent.
Original-output parity for file insertion is claimed only for the simple star
case the in-tree oracle measures (a two-entity AC1.40 file, `INSERT *D2`):
prompts and translated records. The block form, nested definitions, layers,
LOAD and conflicts have no original export.

## Retained evidence (ACAD.HLP lines 349-367)

```
INSERT  File name:  <file>
        Insertion point:  <point>
        X scale factor (default = 1):  <value>
        Y scale factor (default = 1):  <value>
        Rotation angle:  <angle>
```

- The named drawing file is copied into the current drawing "as a block which
  can later be manipulated as a whole".
- A `*` before the file name retains the individual components instead.
- The X-scale prompt also accepts an opposite corner of a box.

Previously established (design spec, QEMU/in-tree oracles for *local* blocks;
I2 re-measured the star form for blocks and files):
blank Y scale defaults to X, `INSERT *B` skips the scale and rotation prompts
and translates members by insertion point minus block base, negative scales
are stored, zero scales are rejected. External insertion reuses those prompt
contracts unchanged.

The help text does not say how a name that is both a block and a file is
resolved, what happens to blocks nested in the file, which header data is
taken from the file, or what happens on a name conflict. Everything below on
those points is **native Rust policy**, chosen to keep the drawing valid for
both codecs and never to lose or silently replace host content.

## Contract

### Name resolution

1. A name (after an optional `*`) that matches an existing block ignoring case
   inserts that block exactly as before; no file is read.
2. Otherwise the text is a drawing-file specification. The editor stays
   filesystem-free: it reports `insert_file_request`, and the application
   Session reads and decodes the file, then hands the decoded drawing back via
   `Editor::submit_insert_drawing`. Return, screen-menu GO, menu macro pieces,
   the API and MCP all take this route. An `Editor` used without a Session still
   reports `unknown block: NAME`.
3. File lookup (Session): `D:rest` uses the FILES drive mapping
   (`AUTOCAD_DRIVE_D`, `A:` defaults to the process directory); an absolute
   path is used as given; a relative path is tried in the attached document's
   directory, then the process directory. Without an extension `NAME.DWG`
   then `NAME.dwg` are tried; an explicit extension (e.g. `.DXF`) is used
   exactly. Only regular files (or symlinks to them) are candidates:
   directories, FIFOs and devices are skipped and the search continues. The
   read is bounded to 16,776,960 bytes (65,535 stored records x 256 bytes,
   the codecs' record limit); a larger file, or one that grows past the cap
   while being read, is refused. OPEN does not share this cap. The codec is
   chosen by magic bytes like OPEN: AC1.2, AC1.40, or DXF.
4. The block name is the file's base name without drive, directories or
   extension, upper-cased. It must be non-empty printable ASCII (BLOCK rule).

### What is imported

- **Block form** (`INSERT FILE`): one new definition named after the file,
  whose base is the file's stored `BASE` and whose members are the file's
  root records in document order: live entities, REPEAT groups as member
  groups, and erased ordinary root records as erased members (the inverse of
  the `INSERT *`/named-WBLOCK promotion, so `WBLOCK name` → `INSERT file` keeps
  erased history). A root *erased whole REPEAT owner* cannot be stored as a
  block member and is omitted; the status line reports the count and
  suggests `*FILE`, which keeps them. Then the usual point / X / Y / rotation prompts
  place one INSERT on the current layer.
- **Star form** (`INSERT *FILE`): the file's root records (including erased
  records, which stay erased and are never revived) are appended, translated
  by insertion point minus `BASE`; no definition named after the file is
  created and, as for `*block`, scale/rotation are not prompted (measured on
  the original for both forms, see below). The native `S` extension below
  places them scaled and rotated instead.
- **Reachable definitions only**: definitions referenced from the imported
  records (also through REPEAT members, erased records and nested blocks,
  transitively) are imported in source order before the new definition.
  Unreachable definitions in the file are dropped, like `WBLOCK *`.
- **Header**: only `BASE` and the layer table are consulted; LIMITS, VIEW,
  SNAP, current layer etc. of the file are ignored. Host EXTENTS are refreshed
  as for any edit.
- **Layers**: entity layer numbers are kept. A layer used by imported records
  (including nested definitions and REPEAT marker layers) that the host table
  lacks is added with the file's colour and, if OFF in the file, is added OFF.
  Layers already in the host keep the host colour and ON/OFF state. A layer
  absent from both tables stays absent.
- **LOAD resources**: LOAD records are retained in their stored order. In the
  host, ordered LOAD semantics apply at the insertion position (as for any
  copied record). A live root LOAD imported by the star form updates the
  SHAPE command's active library, as reopening the saved drawing would.
  After a successful commit only, the Session makes available the SHP
  libraries named by live LOAD records reachable from the imported content
  (root records and every reachable definition, including reused ones),
  found beside the source file with OPEN's search; names already available
  win. Cancelled or refused imports merge nothing, and other SHP files in
  that folder are never merged.

### Conflict and failure policy (explicit)

All name comparisons ignore case, preserving the host invariant (BLOCK already
refuses case-insensitive duplicates). Host definitions are never redefined.

| Situation | Result |
| --- | --- |
| Bare name equals a host block | Host block is inserted (rule 1); file ignored |
| Path/extension-qualified file whose block name equals a host block | Refused: `block NAME already exists` |
| Reachable file definition with the same exact name and identical base/members as a host definition | Host definition reused, not duplicated |
| Reachable file definition whose name differs from a host definition only in case | Refused: `differs from existing block NAME in name case` (`and content` when the members differ too) |
| Reachable file definition with the same exact name and different content | Refused: `block NAME already exists with different content` |
| Two reachable file definitions with the same name | Refused (ambiguous) |
| File references an undefined block | Refused |
| Cycle among reachable definitions, or file content inserting the new block name | Refused (`cyclic`) |
| Missing file / unreadable file / codec error | Refused with the path and reason |
| File with no root records to insert | Refused |
| Missing / non-regular file, or file over the byte cap | Refused (`not found` / `byte drawing file limit`) |
| Imported records violate the checked codec limits, or host plus import exceed the whole-drawing 65,535-record budget | Refused before any change (an import never makes a savable drawing unsavable; a host that already fails the checks is only checked for the imported records) |

### Atomicity

The file is read, decoded, closed over and validated when the name is
entered; the result is staged in the prompt state only. Nothing in the drawing
changes until the final prompt (rotation for the block form, insertion point
for the star form). The commit is a single UNDO step. An error at any prompt
keeps that prompt for retry; cancel at any prompt discards the staged import.
Every refusal leaves drawing, undo history, dirty state and the block table
unchanged; no definition is left behind.

## Star-form scale and rotation (I2)

### Original evidence (`crates/acad-oracle/tests/insert_change.rs`)

- `INSERT *B1` / `INSERT *D2` (block and drawing file) return to `Command:`
  directly after `Insertion point:`; a plain `INSERT B1` asks
  `X scale factor (default=1) or Corner:` there. Numbers typed after the
  star point are commands (`Unknown command.`), and the members are only
  translated by insertion point minus base. The native default flow is the
  same (no prompts, identical records).
- Answering `S` at the star `Insertion point:` makes the original print
  `*Invalid*` and abort INSERT, so no original script can depend on it.

### Native extension (native policy)

`S` at the star insertion-point prompt asks `X scale (Enter for 1)`,
`Y scale (Enter for X scale)` and `rotation angle (Enter for 0)` (numbers;
zero or non-finite scales are refused), then returns to the insertion point,
which commits as before (one UNDO; file imports stay staged until then).
Members are placed by `origin + R(rotation) · diag(X, Y) · (p − base)`, the
same transform the renderer applies to a block INSERT, so the exploded
geometry draws the same picture as `INSERT name` with the same answers
(asserted in `acad-app/tests/insert_change.rs`). The identity answers keep the
exact evidenced translation.

| Member | Representable when | Stored result |
| --- | --- | --- |
| LINE, POINT, TRACE, SOLID | always | transformed points |
| CIRCLE | \|X\| = \|Y\| | radius × \|X\| |
| ARC | \|X\| = \|Y\| | radius × \|X\|, mapped angles; a mirror (X·Y < 0) swaps start/end |
| TEXT, SHAPE | X = Y (a negative uniform scale is a half turn) | height × \|X\|, angle + rotation (+180) |
| nested INSERT | the combined linear part has orthogonal columns (uniform scale, or a nested angle that is a multiple of 90°) | scales/angle of R(ψ)·diag(x′, y′), keeping the decomposition whose angle is nearest the turned angle |
| REPEAT | a lattice (more than one cell) needs rotation ≡ 0 or 180°; one cell always | members placed, spacings × the diagonal factors |
| LOAD | always | unchanged |

Erased members are placed the same way and promoted to root erased records,
never revived (B4/E1 promotion). Any unrepresentable member — live or erased —
refuses the whole placement at the rotation prompt (`insert as a block
instead`), which stays open for retry. Placed geometry that leaves the finite
range depends on the origin and is refused at the insertion-point prompt
instead (`placed geometry exceeds the finite range`), which also stays open.
Cancel or a refusal changes nothing. TEXT/SHAPE angles are stored wrapped into
[0, 360), like ARC and INSERT angles. A 90°/270° turn of a REPEAT lattice
(representable by swapping rows and columns) stays refused as a conservative
native choice.

## Limitations

- Apart from the measured simple star case, the external-insertion policies
  above are native choices, not recovered behaviour.
- The original star form never scales or rotates (now evidenced for blocks
  and files); the `S` placement is a native extension without original
  parity, and refuses non-uniform scales of circles/arcs/text, mirrored
  text/shapes, skewing nested INSERTs and rotated REPEAT lattices.
- Erased whole REPEAT owners in the file are omitted in the block form.
- Only drawing records and the layer table are imported; other header state
  of the source file is not merged.
- SHP libraries merged after a commit are session resources: a later UNDO
  does not withdraw them (as for LOAD).
- Imported content inherits the host's ordered LOAD context at its position;
  no LOAD is synthesized to reproduce the source drawing's initial context.
