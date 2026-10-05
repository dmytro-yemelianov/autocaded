# Codec and oracle notes

How the DWG/DXF codecs and the oracle reached their current state, moved
verbatim from the README's former "State" section. See also
[the oracle note](oracle-qemu.md), [font rendering](shp-rendering.md) and
[native command behavior](command-behavior.md).

## Milestone order

④'s read direction runs before ③, out of spec order, because `SUBDIV.DWG` and
`SUBDIV.DXF` are the same drawing in both formats — ground truth that needs no
emulator (spec §4.4). The QEMU oracle also supplies evidence for `AC1.40`
reading and writing.

## QEMU oracle probe

`acad-oracle` now has a QEMU-backed development probe. It boots disposable
copies of the original floppies, creates an empty `AC1.40` drawing, and asks
AutoCAD to export five `AC1.2` and five `AC1.40` sample drawings as DXF (including
`DISC.BAK`, renamed on the disposable copy). The tests compare the
empty export byte for byte and the sample exports against the DWG reader's
geometry, including `POINT`, `TRACE`, `SOLID`, and entities inside `REPEAT`.
It also creates circles, a line, a three-point arc, and rotated text, checking
geometry against the command inputs and the decoded DWG against the exported
DXF. Nondefault header settings pin BASE, fill mode, and the layer table.
Generated `LOAD B:ES` and `SHAPE` commands verify library names, definition
numbers, placement, scale, and rotation. The renderer now interprets all seven
supplied `.SHP` libraries, including ordered font changes and shape subroutines.
A CGA-memory comparison checks native text and shape strokes against the original
display. See [font rendering](shp-rendering.md) for coverage and limits.
Entity layers now survive DWG and DXF parsing and DXF writing; the renderer
resolves the layer color table into RGB strokes, and `FILL` controls the
interiors of `TRACE` and `SOLID` polygons.
A QEMU 11.0.1 branch bug requires
disabling TCG block chaining (`-d nochain`). QEMU must be installed for this
probe; the in-tree runner verifies seven DWG save cases and captures a LINE
editor frame and an ID query frame identical to QEMU's CGA memory. TEXT's computed Y extent differs
by one ULP; its cause is still being traced. See
[the oracle note](oracle-qemu.md).

## AC1.2 record types and erasure

`SUBDIV` is the corpus's only drawing with a DXF sibling on disk. Seven
`AC1.2` record types were initially verified against it: `LINE`, `CIRCLE`,
`ARC`, `TEXT`, `BLOCK`, `ENDBLK`, `INSERT`. AutoCAD's freshly generated DXF
exports now independently check `POINT`, `TRACE`, `SOLID`, and the entities
carried by `REPEAT`/`ENDREP`. The model now keeps rectangular patterns, including
the two nested in BLIVET blocks; a 2×2 original command probe verifies the
saved counts, spacing, DXF structure, and rendered positions. `ENDREP` now
accepts point inputs for both distances: the original derives column X and row
Y spacing from consecutive points, and the generated DWG matches both oracles.
The opening record's second word is 1 across generated probes with one or two
entities and different dimensions; its meaning remains unknown.
A signed type code marking an
erased entity (`ADDER`'s own finding) is confirmed the same way: reading the code
as `i16` makes `ADDER`'s walk land exactly, where reading it unsigned does not.
Erased records now remain in DWG document order and are omitted from rendering
and DXF export. QEMU verifies `ERASE L` saves a `-1` LINE record and `OOPS`
restores its positive type; Rust emits matching record bytes for both.

## AC1.2 corpus coverage

All 16 `AC1.2` drawings in the corpus have been run through the reader
(`crates/acad-dwg/tests/corpus_smoke.rs`): **all 16 render**. The pre-existing 11
use only the seven verified types (evidence the layout generalises, not a
second oracle check); `ADDER`, `FLOOR` and `FLOW` needed erasure, `REPEAT` and
`SOLID` respectively.

## Nested BLOCK definitions

The last two, `SELEXOL` and `BLIVET`, needed a structural fix rather than a new
record type. Both decoded every individual record correctly from the start
(`POINT`, `TRACE` and `REPEAT` included — checked directly against their real
bytes, not a synthetic fixture), but both also contain a `BLOCK` definition
nested inside another `BLOCK` definition (`SELEXOL`'s `"HEAD"` inside
`"PACKTWR"`, and independently `"ARROW"` inside `"COOLER"`; `BLIVET`'s
`"$BCIRC"` inside a block named `"BLIVET"`), which the reader used to track
with a single `Option`, not a stack, and so rejected as `DwgError::NestedBlock`
even though both files are otherwise perfectly well-formed (every `BLOCK` does
have a matching `ENDBLK`). The reader now tracks open `BLOCK`s as a stack: a
`BLOCK` opened while another is already open defines a **sibling**, not a
child — evidenced by `SELEXOL`'s `ARROW`, which is also `INSERT`ed fourteen
times outside `COOLER`'s own span, so it cannot be scoped to `COOLER` — and
each `ENDBLK` closes and emits the innermost open one. `DwgError::NestedBlock`
no longer exists.

This is the structural model used for the 1983 block table: block definitions
are flat entries, while the `BLOCK`/`ENDBLK` byte spans in the entity stream
group each entry's body. Nesting a definition inside another definition in
the stream does not make it a child; references resolve by name across the
table. That conclusion is grounded in the `SELEXOL` and `BLIVET` corpus
evidence above, not a claim that all possible 1983 files have been tested.

Grouping the blocks correctly still wasn't enough to draw them: both files
also `INSERT` one block from inside another block's own body, so the renderer's
`INSERT` expansion had to become recursive, composing each nesting level's own
translate/scale/rotate transform with the one enclosing it, with a depth cap
(ours, not a recovered 1983 constant — the corpus never nests past depth 2)
against a self-referencing `INSERT`.

## Render check

The 800×800 corpus render check measures 40,006 lit pixels for `SELEXOL` and
39,403 for `BLIVET`; `ORGATE`, the smallest of the 16 AC1.2 drawings, measures
2,466. The 500-pixel floor is a broad nonblank guard, not proof by itself that
nested inserts expanded correctly; recursive expansion also has direct
transform-composition tests in `acad-render`. Their newly exported DXFs
independently check decoded entity geometry, but not every rendered pixel.

## Format detection and AC1.40

`acad-app` opens either format, dispatching on the file's own magic bytes rather
than its extension, so a `.BAK` file — four corpus drawings have one — is
identified as the DWG it is rather than guessed from its name. The four
`AC1.40` drawings (`HOUSE`, `COLORS`, `OFFICE`, `SHUTTLE`) and the three
matching backups now parse and render. `DISC.BAK` also opens, preserving its
`ROMAN-S` and `ITALIC` font loads; the shuttle labels and italic STAR WARS title
now render. Entity layers, the indexed ACI palette, and filled TRACE/SOLID
rendering are preserved. AC1.40 DWG output matches original QEMU-generated
LINE, CIRCLE, and POINT record bytes and opens in AutoCAD under QEMU, with a
CGA drawing viewport matching the native reference within 0.1%. AC1.2 output
also opens, with SUBDIV's
viewport matching pixel-for-pixel. Unknown fixed-header bytes from DWG input
are carried through by the writer; their meanings remain to be recovered.
