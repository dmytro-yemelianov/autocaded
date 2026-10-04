# Group persistence evidence and proposed contract

Status: B3a independent-marker candidate plus a separately approved B3b native
erased-owner candidate. Static original REPEAT selection is source-member based;
Rust whole-owner editing/persistence is an explicit native policy. B4 retains
ordinary erased members inside live groups. E1 measured whole-group erasure after
prior member erasure on the original: its file loses the prior status too, so
DWG save writes the original's form only after an explicit question. Negative
structural subgroups and explicit owner-layer wrappers remain checked
compatibility gaps. This document
does not claim original group-edit completion.

## Retained evidence

All inspection is offline/read-only. Sources are the retained design specification,
`build/recovered-cfg.json`, and original `corpus/System/ACAD.EXE`, `ACAD.OVL`,
`corpus/Samples/FLOOR.DWG`, `BLIVET.DWG`, and `ADDER.DWG`.

- recovered CFG SHA256: `45fa698d17c0a827f542efd3b58f17e88e078c5fdeee62020874579139c1573c`
- ACAD.EXE SHA256: `0080e343930b6fad30b588f05bacb6fb65783d7d560cdf2f116ca09fc70b45b1`
- ACAD.OVL SHA256: `c2e1ee5b538e53df022dbb8aba6ab65a3946b8a332438bdaf39a3938a43e0da8`

The retained map has image load segment 0x1010, EXE_DATA segment 0x1e25,
and the MZ header is 32 paragraphs. The data file base is therefore
`32*16 + (0x1e25-0x1010)*16 = 0xe350`. This is checked against the entity
names reached by the physical decoder's descriptor table, not guessed coordinates.

### Independent physical REPEAT markers

The native reader EXE_CODE entry 0x3770 reads the signed type at DS:42be,
uses its magnitude for descriptor dispatch (0x3796: `f7d8`, NEG AX;
0x37a2: `8906a552`, store DS:52a5), reads layer DS:42c0, and indexes
DS:4426 by four times the type (0x37c5/0x37c7 shift twice;
0x37ca: `8d362644`). The raw table entry for type 5 is
`(name=0x3883, descriptor=0x4360)`: name `REPEAT start`, descriptor
words `(0,0,0)`. The descriptor's zero first word ends the body-field loop.
Thus a REPEAT opening marker occupies four bytes, with no body.

Type 6 has name `REPEAT end`, descriptor 0x4366: field selectors
`(4,1), (4,2), (2,2), (2,3)` and a terminating zero. These match the
established two u16 dimensions and two f64 spacings (24-byte ENDREP record).

The native writer EXE_CODE 0x3985 similarly uses magnitude for the field
descriptor (0x3999 `f7d8`) but writes the original signed DS:42be word
(0x39be call 0x3b98 after pushing DS:42be). Zero-body type 5 therefore
uses the same four-byte layout for either sign.

The existing Rust explanation of a fused first child is incorrect: the
supposed `nested_type` and `opaque_word` are the next ordinary child's
**type and layer header**. A flat group's opening four bytes followed by
its first ordinary record give the same bytes and count as the existing
flat layer-1 examples. Their unexplained word 1 is the child's actual layer.

An independent raw physical-record walk (without invoking Rust or translated
code) finds FLOOR 213 physical records, matching header count 213 and end
0x204f; BLIVET 149/149 and end 0x1a4c. Their seven REPEAT openings have
marker layer 1 followed by an ordinary LINE or INSERT header with child
layer 1. All patterns have depth 1; these examples do not prove nested export.

OVL02_CODE 0xc7d1 independently checks stream nesting: compares type 5 at
0xc874 (`3d0500`), increments local repeat depth at 0xc881 (`81040100`),
compares type 6 at 0xc88c (`3d0600`), decrements at 0xc899 (`812c0100`),
and requires depth zero when ENDBLK is encountered at 0xc8af (`3d0000`).
This supports an independent-marker stack parser; it does not establish
historical editor selection or maximum nesting limits.

### Erased sign: physical rule established, group scope unresolved

ADDER's independent walk finds 66/66 records ending at 0xb50 and seven
negative ordinary records: two INSERT and five LINE. Bodies and positions
are retained. No negative REPEAT or ENDREP appears in these corpus cases.

The command dispatcher OVL02_CODE 0x523e calls 0x9ce0 from 0x5520.
The 0x9ce0 routine iterates selected address records, reads at that address
through 0x5100, checks DS:42be is positive, negates at 0x9d38 (`f7d8`),
stores at 0x9d3a (`8906be42`), and writes through 0x5079 at 0x9d3e.
The 0xe4cd restoration routine iterates the retained selection list at
DS:3e42, reads each address, reverses a negative type at 0xe52c (`f7d8`),
stores at 0xe52e, and writes through 0x5079 at 0xe532.
The kernel aliases resolve 0x5100 to EXE_CODE 0x775c (physical read via
0x3770), and 0x5079 to EXE_CODE 0x7788 (physical write via 0x3985).

These instructions prove per-selected-record sign reversal, with intact fields.
The independent follow-up review (`RUN/reviews/b3-erased-scope.md`) resolved the
ordinary top-level REPEAT selection path: EXE_CODE 3dcb/3e83 manipulate playback
counters/positions but never establish a marker-owned cache address. EXE_CODE
192d at 1e12–1e70 uses the current physical leaf address 58d5/58d3 when no
INSERT reference address 425e/425c is active. Cache writer 7154 and iterator
7233 carry that source address into the selection list. INSERT 43db establishes
an enclosing reference address; REPEAT does not. EXE_CODE 793b skips each
negative physical record independently, so negating only an opening marker does
not hide positive children. The follow-up independently checked 2,226 instructions
in 21 routines against original bytes. This static dataflow result is not an
executed original nested fixture or one historical numeric ID per source leaf.

Rust's whole stored Repeat owner intentionally differs. The native uniform-negative
policy below uses the established physical sign rule; it is not recovered original
whole-group selection or a new historical bit.

## Accepted B3a scope and separate B3b boundary

B3a corrects physical marker decoding/writing and bounded nested logical
assembly without invented fields. It must retain independent child layers,
LOAD order, block dependencies and source aliases. It needs an explicit model
contract for marker layer versus runtime owner: bare Rust REPEAT currently has
no owner, while Entity::OnLayer(REPEAT) has one. A file always stores a marker
layer, so silently stripping an explicit owner or inventing a sentinel is not
acceptable. Nested streams crossing block boundaries, unmatched/truncated
markers and allocation/depth excess must produce named errors.

B3b makes native whole-owner ERASE/BLOCK save/reopen usable with the explicitly
approved uniform-negative policy below. In-session OOPS/UNDO retains original
item/index pairs. No persisted editor undo/OOPS stack is promised after opening.
A named live block exports independently of erased source items, with its base,
ordered resource context and transitive block dependencies retained.

## Independent review and session history boundary

The independent read-only review (`RUN/reviews/b3-contract-review.md`) verified
650 raw instruction bytes against the original EXE/OVL and reproduced all three
physical walks. It accepts the independent-marker grammar. The later selected-address review
resolves source-member selection; owner-wrapper persistence remains unresolved. Erased ordinary members
inside a live group previously needed preservation or a named refusal: the
pre-B3 reader silently dropped those records, while B3 checked-refused them. B4
introduces the ordinary member tag and bounded preservation below. A repeat
may not cross a block boundary or be silently reparented into a different block.

Current native OOPS restores `Editor.last_erased`, a session-local list of original
item/index pairs (`acad-cmd/src/editor_ops.rs`, erase/oops). UNDO snapshots both the
drawing and that list. Opening a file initializes a fresh editor with no
`last_erased` list. Persisting `Item::Erased` alone therefore cannot promise OOPS
or prior UNDO after reopen. This is separate from retaining erased file content.

Root approved metadata-aware nested parsing and mechanical native constructor/test
updates. `Repeat.start_layer` and `end_layer` preserve physical marker metadata;
children retain their own OnLayer headers. Explicit owner wrappers remain checked
errors. New command REPEAT/ENDREP records capture the current layer at each marker. Existing flat source aliases and
strict comparisons must continue to pass; refusal-only safety fixes do not
complete B3's usable erased-group persistence objective.


## Native safety and validation policy

Both writers preflight stored records with at most 64 group levels (65 iterator
frames including the root) and a 65,535 physical-record ceiling before serialization. A huge u16 lattice is stored
compactly and does not expand during codec work. Empty groups, zero dimensions,
nonfinite spacings, explicit group owners and stacked layer wrappers are errors.
Readers enforce the same nesting/count bounds and exact physical region/count
landing. DWG retains established diagnostic order for corrupt boundary, unclosed
block/group and count mismatches. DXF limits allocated lexical records to 65,535
plus 128 header records, with a separate 65,535 body-record limit.

The shared marker metadata is semantic model state but has no owner-layer gate.
An explicit owner wrapper can be used by the runtime, but checked save/export
refuses it until a lossless mapping exists. Source aliases remain available at
`acad_dwg::entity::{read_entities,read_items,RecordHeader}` and both writer APIs.
The legacy DXF Vec adapter delegates to checked serialization and panics explicitly
on unsupported state; production document/API routes continue typed errors.

B3a tests include independent manually assembled records, differing opening/child/
closing layers, nested LOAD records and blocks, both revisions, historical DXF
six-decimal output, original FLOOR/BLIVET flat marker span byte comparisons,
corrupt/truncated and depth errors, command-created metadata and whole-group
edits/save/reopen/END, WBLOCK dependencies, and atomic unsupported-state recovery.

## B3b native owner policy and residuals

Every lexical DWG record inside a native erased Repeat owner, recursively, carries
the existing negative type sign. Both start/end marker layers and each child's
actual header/body are retained. Referenced INSERT block definitions are not
lexical members and remain independently live. The reader reconstructs one
`Item::Erased(Entity::Repeat)` only at a top-level Item position from a balanced,
uniformly negative subtree; nested metadata lives beneath that single owner.
Any positive marker/member in the subtree rejects. Negative structural subgroups
in live Repeat/Block bodies remain checked errors. B4 preserves ordinary negative
members under positive markers, separately from the whole-owner rule below.
Negative BLOCK/ENDBLK markers remain unsupported, except the zero-filled
top-level task 6 placeholders, which are dropped (section below). Empty groups are consistently
refused for either sign. Fields, layers, count/end, nesting and truncation checks
apply to erased records too; nonfinite ordinary fields/block bases reject before
writing or accepting malformed input. Unknown magnitude, including abs(-32768),
produces the ordinary unknown-type error.

Erased LOADs have no runtime resource effects. Historical DXF follows live-only
exchange semantics: it omits whole erased ordinary/group owners, including their
content, layers and marker metadata, with no leaked member LOAD. Both public DXF
writer adapters document that loss. The session's original erased model/OOPS/UNDO
history is unchanged by serialization, but a reopened file has no editor history.
Thus DWG retained-owner round trips and DXF live-only exports are distinct claims.

Named/selected WBLOCK context uses a dedicated bounded LOAD-only projector:
preceding hidden/unselected Repeat bodies are visited once, INSERT definitions
resolved in order with a 16-frame cycle/depth cap, 256 stored nesting depth and
100,000 stored visits. No preceding geometry or generated cells are emitted.
Unknown/deep/cyclic context rejects before output rather than selecting a stale
font. Tests compare rendered font/shape strokes after nested hidden LOADs and
both format round trips, while existing whole/selected closure tests remain.

Tests additionally assert raw uniform negative bytes/counts, every one-positive
mutation, corrupt erased layers/fields/count/end/depth, unchanged referenced block
definitions, erased LOAD suppression, ERASE/BLOCK SAVE/END/reopen, session
OOPS/UNDO and checked API pending-input behavior. Original corpus bytes and
numeric tolerances remain held. Lossless explicit owner-layer encoding and
historical source-member editing parity remain open compatibility gaps.

## B4 ordinary erased-member preservation

`Entity::Erased(Box<Entity>)` represents one erased ordinary physical record
inside a live Repeat or Block. It is separate from `OnLayer`; decoding wraps the
actual layer and unchanged ordinary fields in the erased tag. Both DWG revisions
emit the existing negative signed type, retaining child order and marker metadata.
No marker flag, new file field or owner-layer mapping is introduced. Checked
writers reject stacked erasure, layered wrappers without a unique record layer,
structural `Erased(Repeat)`, and a live drawing-root member tag (use `Item::Erased`
for a top-level erased record). Erased fields still undergo finite, layer,
string, stored-count, region/end and depth checks. Validation precedes output
allocation and applies even to content omitted by exchange export.

Live-purpose traversals keep erasure opaque: rendering, selected highlighting,
mouse/window picks, bounds, anchors, standalone TEXT metrics and ordered LOAD
projection do not unwrap the erased tag. A live stored group retains one canonical
native ID even when all its members are erased, but it has no geometry or anchor.
`Entity::bare`-style layer helpers do not strip status. Reports count stored erased
members separately, including those in nested groups, without listing their geometry. Stored-purpose transforms
retain the tag and transform its coordinates/properties with the whole owner;
COPY/ARRAY preserve status and CHANGE layer updates the stored child header.
These whole-owner edits are native Rust policy, not historical source-leaf parity.
Stored dependency closure looks through an erased INSERT to retain its independent
referenced definitions. Resource execution does not enter that INSERT.

Uniform-negative DWG whole-owner encoding cannot distinguish a previously erased
member from a member erased by the current owner action. A whole erased owner
containing any prior erased lexical ordinary member therefore checked-refuses
direct DWG output with an explicit OOPS/UNDO recovery reason. This also applies
to the retained erased source created by BLOCK. Since E1 the SAVE/END commands
ask before refusing and can write the original's own form instead (next section). Referenced INSERT definitions are
independent lexical streams: an erased INSERT can still save when its live block
contains ordinary erased members. Named WBLOCK of a live block exports its
members as root records independently of an ambiguous erased source: a live
member stays live, and each erased direct member becomes a root `Item::Erased`
negative record with its exact layer and fields. The erased INSERT's referenced
definition is kept for its stored fields. Erased leaves inside a member REPEAT
stay inside that group. The promoted erased LOAD/INSERT has no projected or
rendered resource effect. `INSERT *name` (explode) applies the same promotion.
Erased copies are transformed like their live siblings and never revived. The
explode is one UNDO step. OOPS restores only records erased by this session's
ERASE, so it does not resurrect promoted erased copies; this matches reopened
root erased records. Whole-drawing `WBLOCK *` remains live-only and omits root
erased records.
No prior member is silently restored by saving. SAVE/API/END failure leaves the
existing destination, absent paths, document attachment and dirty state intact.
In-session OOPS restores the original partial group; UNDO restores each original
status snapshot. Reopen retains member signs, but no editor OOPS/UNDO history.

DXF follows the established live-only exchange policy: ordinary erased members
and whole erased owners are omitted. Repeat subtrees with no remaining exportable
members are pruned recursively, including marker metadata, so no unreadable empty
marker pair is emitted. A live LOAD is exportable content even without geometry;
an erased LOAD is neither exported nor executed. The source drawing and session
history are unchanged. Typed checked APIs and the explicit-failure legacy DXF
adapter preserve their existing contracts.

Constructed byte fixtures cover every ordinary signed type beneath positive
nested Repeat/Block markers, differing layers and both revisions, exact physical
record-region and re-encoded DWG bytes, independent block dependencies, negative
LOAD effects, corrupt/truncated erased fields and stored budgets. Dedicated editor,
render, session and checked API tests cover skips, transformations, report/ID
agreement, BLOCK/WBLOCK, existing/absent destination safety, END and OOPS/UNDO.
Original corpus comparisons remain unchanged. Retained static evidence proves
per-physical-record sign handling and skipping; these constructed tests do not
claim original group selection or nested editing completion.

## Task 6 placeholder records (R5)

Main Menu task 6 (Load DXF) in the original writes dead records into the
drawing. Without a reader rule, its output cannot be opened natively
(`StrayEndrep` at the erased ENDREP).

Evidence: the in-tree emulator, asserted in
`crates/acad-oracle/tests/task6_placeholders.rs`.
- For each DXF header record, task 6 writes one erased record of type
  `-(keyword index)`, with every body byte zero, at that record's place in
  the stream, even after entities. The indices are EXTENTS 1, LIMITS 2,
  BASE 3, DWGVIEW 4, DIMARROW 5, MODERES 6, MODEGRID 7, MODEORTHO 8,
  MODEFILL 9, TXTSIZE 10, TRACEWID 11, LAYER 12, LAYERC 13. The record keeps
  the layer field of the DXF record. All of these placeholders count toward
  the header's count and end.
- The original's own task 5 DXF has 12 header records, so its task 6 output
  holds -1..-13 without -5. That output includes an unpaired erased ENDREP
  (-6), an erased BLOCK with an empty name (-12) and an erased ENDBLK (-13).
  The native DXF writer puts DIMARROW directly before MODERES, so task 6 of a
  native DXF that has DIMARROW produces -5 immediately followed by the
  zero-filled -6.
- Task 6 into an existing drawing appends another run of placeholders after
  the existing records. With no header records in the DXF, no placeholders
  are written.
- When the original opens such a drawing and ENDs it, edited or not, only the
  live records remain (`task6-line-ended.dwg`). QUIT leaves the file
  unchanged. The original's task 5 output for the raw file is identical to
  its output for the ENDed file.
- The original also drops an ordinary record ERASEd in an earlier session
  when a later session ENDs. Native B3/B4 policy keeps such records, and that
  difference is unchanged here.

Decision: the placeholders are split by whether the model can represent
them.
- **Ordinary placeholders (1-4, 7-11) are preserved.** They are erased
  LINE/POINT/CIRCLE/SHAPE/TEXT/ARC/TRACE/LOAD/SOLID records with zero
  fields, and nothing distinguishes them from a record erased natively at
  the origin. A native BLOCK of a POINT at (0,0) leaves exactly such a
  record, and the existing tests require it to survive. They stay
  top-level `Item::Erased` under the B4 policy: they never render, select,
  count as entities or reach DXF. Saving re-emits them byte for byte, in
  both revisions.
- **Structural placeholders are normalised away.** These are the erased
  ENDREP (6), the erased BLOCK with an empty name (12), the erased ENDBLK
  (13), and the erased REPEAT start (5) that pairs with the zero-filled
  ENDREP. They have no model form, and no native or original record of that
  shape carries meaning: an unpaired erased ENDREP or ENDBLK, or a BLOCK
  with no name. They are dropped, as the original's END drops them.

`read_items` drops a record as a structural placeholder only when all of
these hold:
- it is at the top level (no BLOCK or REPEAT open);
- it is an erased ENDREP, BLOCK or ENDBLK;
- its layer is at most 255;
- every body byte is zero.

A dropped record is counted and checked against the header's count and end
like any other record. Native writers never emit one. A native rewrite of the
fixture keeps the nine ordinary placeholders and the LINE (10 records), not
the original END's single record. This is the same retained-erased-record
difference as above.

An erased REPEAT start is always body-less, so a lone one cannot be told from
a malformed erased owner. It is a placeholder only when the next record is a
zero-filled erased ENDREP. A genuine erased owner can never close with that
record, because zero dimensions are invalid.

These cases keep their existing errors:
- a stray ENDREP with any non-zero byte, or one that is not erased
  (`StrayEndrep`);
- a zero-filled erased ENDREP that closes an open erased or live REPEAT
  (`InvalidGroupStream`);
- a lone erased REPEAT start, or one followed by any other record
  (`UnterminatedRepeat`, `InvalidGroupStream`; the B3b mixed-sign rules are
  unchanged);
- a named erased BLOCK, any erased BLOCK or ENDBLK inside an open block, and
  an erased ENDBLK whose layer is above 255 (`UnsupportedErasedStructure`);
- a placeholder pair inside a block (`InvalidGroupStream`);
- count and end mismatches.

A zero-filled erased record inside a live group is still a B4 member.

Tests: `crates/acad-dwg/tests/task6_placeholders.rs` covers the fixtures in
both revisions, each placeholder alone, the DIMARROW pair, every refusal
listed above, and byte-exact rewrites. `crates/acad-app/tests/task6_placeholders.rs`
covers Session::open (used by `acad DRAWING` and `acad-mcp --drawing`),
Main Menu task 2, API `open`, the real `acad-mcp --drawing` binary, render
and selection equality with the ENDed file, END (exact kept records, the
backup holding the original bytes, and a stable second END), and native
task 6 appending to such a drawing.

Remaining limit: when a DIMARROW record is not directly followed by MODERES
(only possible with a hand-made DXF), the original writes a lone -5. That
output is still refused, and the oracle test asserts the refusal.

## E1 whole-owner erasure after prior member erasure

### Original evidence (in-tree emulator, `crates/acad-oracle/tests/erased_owner.rs`)

Each claim below is asserted by that test against original ACAD.EXE runs on
the retained System floppy (skipped visibly without `System.img`, failing under
`AUTOCAD_REQUIRE_CORPUS`). Fixture: `REPEAT`, LINE A (1,1)-(2,1), LINE B
(1,3)-(2,3), `ENDREP` 2 columns, 1 row, spacing 5. "Erase A" is
`ERASE W 0,0 3,2` (A's source instance); "erase the group" is `ERASE W 0,0 10,5`.

- Erasing A alone writes positive REPEAT/ENDREP markers, negative A, positive B.
- Erasing the whole group after A writes **byte-identical files** to erasing
  the whole group with no prior erasure. The markers stay positive (type 5 and
  6); each source member is negative, so a whole erased group is a live group
  whose members are all erased, and its file cannot tell an earlier erased
  member from one erased with the group. That markers are never negated by
  editor selection in general rests on the B3 static analysis above (source
  leaf addresses only; `RUN/reviews/b3-erased-scope.md`, retained as
  `docs/superpowers/reviews/2026-10-04-repeat-erased-scope-review.md`), which this depth-1
  fixture confirms by execution.
- Selection is per source member: a window holding only the repeated column
  (`5,0 10,5`) writes the same file. After A was erased the group window
  reports `3 found.` (A is found again through its repeated instance) against
  `4 found.` without it.
- In-session OOPS after that whole-group erase revives A as well: the file is
  byte-identical to erase-then-OOPS with no prior erasure. Control: two ungrouped LINEs
  with the same windows keep A erased after OOPS.
- BLOCK by the same window leaves the source in that form (positive markers,
  every member negative) and copies one live record per found instance into the
  definition (`B, A, B` after the prior erasure, `B, A, B, A` without); the
  erased A is copied live. OOPS after BLOCK revives A in the source.
- After reopening either file, OOPS answers `*Invalid*`. END after reopening
  writes only the REPEAT/ENDREP marker pair (count 2): erased members are not
  kept. The native reader refuses that empty pair (`REPEAT needs members`).

The original therefore loses the distinction itself, in the file and even in
its own session. No original bit distinguishes the cases, so none is invented.

### Native policy

The native model, ERASE/BLOCK, OOPS and UNDO are unchanged: the session keeps
`Item::Erased(Repeat)` with the prior member tag and OOPS/UNDO restore the exact
prior partial group. Direct DWG encoding (`acad_dwg::write*`, the API `save`
request shared by MCP, `Session::save`) still checked-refuses with the OOPS/UNDO
reason, which now also names the SAVE/END question. DXF stays live-only and never asks.

SAVE, END (attached or unnamed path) to a DWG destination with such an owner
asks first instead of refusing: `SAVE: Lose earlier member erasure? <N>` (or
`END: ...`), short enough for an 800-pixel command line, while the status row
explains `Erased REPEAT group holds earlier erased members; Y writes every member
erased, as AutoCAD 1.4 does (their earlier erasure is lost); anything else writes
nothing`. As at the WBLOCK replace question, an answer starting with `Y`
confirms; Return, `N`, anything else or cancel writes nothing and keeps the
session, dirty state and attachment. The question is native only, so scripts and
menu macros written for the original carry no answer for it: when a command-script
item or menu-macro piece answers with anything but `Y`, the decline is an error.
It interrupts the script (cause `error`, the item's line reported, RESUME
continues after it) or drops the macro's remaining steps, and its "nothing
written" message stays on the status row (`docs/native-scripts.md`). An explicit
`Y` item or piece still confirms. A confirmed save uses
`group_codec::original_member_erasure`: each ambiguous top-level erased owner is
written as a live Repeat (outer and nested markers positive, marker layers
kept) whose every ordinary member is erased, with earlier erased members kept as
one tag. Erased owners without prior member erasure keep the native
uniform-negative form. For the fixture, opening the original's erase-A file,
`ERASE 1` and the confirmed save write exactly the original's record bytes for
the whole-group erasure; BLOCK's retained source reopens equal to the original's
BLOCK source. Both revisions round-trip; END keeps the attached codec/revision.
The status reports the number of groups written in the original form and that
earlier member erasure was not kept. The saved baseline is the session drawing,
so it is not dirty; reopening gives the live all-erased group (one selectable
owner without geometry) and no OOPS history, as in the original. Write failures
are atomic like any save (destination, attachment and dirty state unchanged).

Remaining gaps: the original's END-after-reopen purge (empty marker pair) is not
reproduced and its output is not readable natively (ledger task R6: read the
original's empty REPEAT/ENDREP pair); the original's per-instance BLOCK copies
and OOPS revival of earlier erased members are not reproduced. Unmeasured:
nested original groups (the nested positive-marker form written on `Y` is a
native extrapolation of the static marker rule) and point picks (typed-point
selection did not complete in the in-tree runner); only window selection of the
depth-1 fixture is executed evidence.
