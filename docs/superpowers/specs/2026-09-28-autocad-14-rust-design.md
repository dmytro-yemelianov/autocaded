# Reimplementing AutoCAD 1.4 in Rust — Design

Date: 2026-09-28
Status: approved design, pre-planning

## 1. Intent

Rebuild AutoCAD 1.4 (1983, MS-DOS) as a native Rust application: the same commands,
the same drawing semantics, the same files, running in a modern window.

**Fidelity target.** File I/O, geometry and command semantics are verified against the
original `ACAD.EXE` executing in an emulator; the display and input layer is modern
(a window, not CGA/Hercules). The original is the oracle — "is this command right?"
is answered by differential test, not by judgment.

**Long-term direction.** This is a foundation to keep building on, not a museum piece.
The architecture must allow deliberate divergence from 1983 later (3D, modern UX,
extended entity model) without discarding the verified-compatible core. Consequently
1983 policy is confined to `acad-cmd` and the codecs; `acad-model` is designed to grow.

**Success criteria.**
- Every drawing in the 1.4 sample corpus loads and renders.
- Drawings authored in Rust load correctly in the original under emulation.
- Each implemented command is backed by a differential test against `ACAD.EXE`.

## 2. Scope

**In scope:** AutoCAD 1.4 only — `ACAD.EXE` (78,848 B) and `ACAD.OVL` (179,480 B).

**Out of scope, separate specs later:** AutoCAD 2.x (introduces AutoLISP), r10/r11/r12
(an order of magnitude larger, ADS, 3D, AME). Archives for all of these are held in
`autocad/` but are not inputs to this spec.

**Non-goals:** DOS emulation as a product feature, plotter/digitizer hardware support,
pixel-exact CGA/Hercules reproduction.

## 3. Method posture

Clean-room discipline — notes and specifications derived from the binaries, implementation
written from them — is the working default, re-settled on evidence at the §8 decision gate.

## 4. Established facts

All of the following are verified against the artifacts, not assumed.

### 4.1 Binaries

| Property | Finding |
|---|---|
| Packing | None. Entropy 5.86 (`ACAD.EXE`) / 6.69 (`ACAD.OVL`) bits/byte; plaintext strings present. |
| Toolchain | C, 1982–83. `printf`-family format strings (`%-16s`, `%c%s:`); `Copyright (C) 1982,1983 %s`. |
| `ACAD.EXE` | MZ, 512 B header, **0 relocation entries**, `CS:IP = 0xFFF0:0x0100` (resolves to image start), `SS:SP = 0x0E15:0x51A0`. The MZ header declares 78,340 B, while the FAT file is 78,848 B. The QEMU DOS run leaves the full final 512-byte page resident; the trailing 508 B include a decimal conversion table that the runtime uses. Segment model recovered statically (2026-09-28 spike ②): **`DS = SS = 0x0E15`**, code `0x0000-0xE150` (57,680 B), resident data `0xE150-0x13200` (20,656 B; the declared load image ends at `0x13004`). Not a bare kernel — it holds the overlay loader *and* a substantial string table at `DS:0x3553-0x5000` (62% non-zero): entity type names, `EREGEN` errors, shape/font parser errors, DWG paging errors, the `A U T O C A D - 8 6` / `1.40` / `03/10/84` banner. |
| `ACAD.OVL` | **A container with a directory.** `AC1.40` magic at offset 0, the same stamp DWG files carry. `ACAD.EXE` `fread`s the first **211 bytes** into `DS:0x509F` and `strcmp`s the magic. That header is the directory: `u16` at `+11` = code-window size (`0xFD00`, 64,768 B), then **11 entries of 18 bytes** at `+13` (`0xD3 - 13 = 198 = 11 x 18`). Each entry is two regions of `{u16 dest; u16 len; u32 file_offset}` — region 1 into the code window, region 2 into the data window — plus a `u16` entry point at `+0x10`. The 22 regions cover **99.3%** of the file; all 15 gaps are pads to the next `0x80`/`0x100` boundary. The `!root` `!config` `!plot` `!menu` `!end` `!quit` tokens are not the directory — they are C literals in the payload, and `!root` also appears in `ACAD.EXE`'s data; they are plausibly the overlays' names, which is a hypothesis, not established. `!IDD` was a `strings` false positive over `inc sp; inc sp`. *(Corrects the first 2026-09-28 spike, which read the file as a flat image with no directory.)* |
| Drivers | Separate files per device class: display `DS*.DRV`, digitizer `DG*.DRV`, plotter `PL*.DRV`. A pre-existing device abstraction, each driver 0.5–5 KB. |

Zero relocations across a 78 KB image means the program performs its own segment
arithmetic — the case 16-bit Ghidra handles worst. The spike resolved it statically rather than
empirically: histogramming which 16-bit immediates in the image are used as string pointers
identifies `DS = 0x0E15` at 27/32 hits against 20 for the runner-up, and `DS = SS` follows from the
MZ header. No emulator trace was needed (§10).

### 4.2 File formats

**DWG** — magic is an ASCII version stamp: `AC1.2` and `AC1.40` both occur in the 1.4
sample corpus, so the codec must handle two versions and the corpus itself documents the
format's evolution. `ACAD.OVL` contains the literal `AC1.40` it stamps into files it writes.
Census of the sample disk: 16 `AC1.2` drawings, four `AC1.40` DWGs (`HOUSE`, `COLORS`,
`OFFICE`, `SHUTTLE`), and a fifth AC1.40 drawing available only as `DISC.BAK`.
HOUSE/COLORS/OFFICE have matching backups. `SUBDIV.DWG`, the parallel-corpus file, is
`AC1.2`.

Header layout, established 2026-09-28 against `SUBDIV.DXF`'s own header values and identical
in shape in both versions: `u32` at `+0x24` = one past the last byte of entity data (`0x19E5`
for `SUBDIV`, whose file is `0x1A00` — the rest is cluster slack); `u16` at `+0x28` = the
entity record count (171 for `SUBDIV` = 159 entities + 6 `BLOCK` + 6 `ENDBLK`); then IEEE
doubles from `+0x2A` — `EXTENTS` min/max as 3D points, `LIMITS` min/max as 2D pairs at
`+0x5A`, `DWGVIEW` centre and height at `+0x7A`, `MODERES`/`MODEGRID` as `u16`+`f64` pairs at
`+0x9A`, `TXTSIZE` at `+0xB4`, `TRACEWID` at `+0xBC`. Entity records begin at a fixed `+0x1D8`,
constant across all 16 `AC1.2` drawings, and end at the `+0x24` offset. Entity
order within each block agrees with AutoCAD's exported DXF, but block
definitions can have a different order. Each record starts with a signed
`i16` type code and a `u16` layer index, followed by the type's fields as
IEEE doubles — so `LINE` is 36 bytes and `ARC` 44. The layer word matches
the entity suffixes in original DXF exports and is retained by the drawing
model.

Generated QEMU drawings now establish `AC1.40`'s entity start at `0x202`,
42 bytes later than `AC1.2`. The scalar offsets above are shared. Nondefault
commands locate BASE x/y at `0x0C`/`0x14`, ORTHO at `0xAE`, and FILL at
`0xB2` (correcting the earlier `0xB0` assumption, concealed by both words
being 1 in the samples). Patching only SUBDIV's `0xB2` word to 0 in a copied
disk makes the original export FILL=0, independently confirming the AC1.2
offset. Current layer is a `u16` at `0xC4`; the layer table is 128 `u16`
colors at `0xC8`, with 255 denoting an unused slot. The model retains these
header fields and each entity's layer; DXF writing preserves the entity suffix.

**Block definitions are globally scoped.** In the `AC1.2` corpus, a `BLOCK`
record can open inside another block's `BLOCK`/`ENDBLK` span. It defines a
sibling in a flat block table, not a child of the surrounding definition:
`SELEXOL` defines `ARROW` inside `COOLER`, then `INSERT`s `ARROW` fourteen
times at top level. `SELEXOL` and `BLIVET` both have balanced nested
definitions; the greatest observed definition nesting depth is 2. The reader
uses a stack to match delimiters, but `acad-model::Block` needs no recursive
block structure. The corpus does not establish whether a 1983 writer emitted
such nesting deliberately or these files acquired it through later editing.

The 1983 DXF parser rejects a nested `BLOCK`, and the DXF writer emits each
block from the flat `items` list in closure order. Writing either drawing as
DXF would therefore emit flat definitions and lose the original nesting;
DWG→DXF is not byte-preserving for these files. A later QEMU probe asked
the original to export `SELEXOL`, `BLIVET`, `FLOW`, `FLOOR`, and `ADDER` to
DXF. Comparing those fresh exports with the DWG reader now verifies the
geometry and per-block entity order for `POINT`, `TRACE`, `SOLID`, and
entities inside `REPEAT`. Subsequent original command probes established
the rectangular pattern's columns, rows and spacing; the model now preserves
top-level patterns and patterns inside blocks. The original DXFs also establish that an entity
header suffix (for example, `LINE,20`) is a layer number, not a record count.
The AC1.2 and AC1.40 writers emit the model's entity types and blocks. AC1.40
output matches original QEMU-generated LINE, CIRCLE, and POINT record bytes
and opens in the original under QEMU with a CGA drawing viewport within 16
pixels of the native reference. An AC1.2 rewrite of SUBDIV opens with a
pixel-identical drawing viewport. The model retains otherwise-uninterpreted
fixed-header bytes from DWG input and the writer carries them through; their
individual semantics remain open. A drawing constructed without DWG source
metadata uses zero values for those fields. The writer serializes the model's
flat block order, which may differ from the source DWG's original physical
order.

**The type code is a 1-based index into the entity type table recovered from `ACAD.EXE`**
(`DS:0x38EE`: `LINE POINT CIRCLE SHAPE REPEAT ENDREP TEXT ARC TRACE LOAD SOLID BLOCK ENDBLK
INSERT`). Checked against first records: `SUBDIV` = 8 = `ARC`, and its DXF's first entity is
an `ARC`; `ADDER` = 12 = `BLOCK`; `FLOOR` and `BOX` = 1 = `LINE`; `BLIVET` = 9 = `TRACE`.
Milestone ②'s string-table recovery therefore decodes milestone ④'s binary format.

*(Corrects a 2026-09-28 reading that had the order reversed. That reading searched the DWG
for `1.012459,6.822910,1.261682,6.822910`, quoted as a `LINE` example in §4.2 above — a value
that does not appear in `SUBDIV.DXF` at all. Two of its numbers matched unrelated entities
near the end of the file, which looked like a reversal. The DXF's first entity is an `ARC`,
and its coordinates sit at `0x1DC`, immediately after the header.)*

The entity-record count was first established as a `u32`, which reads correctly for
`SUBDIV` only by coincidence: `SUBDIV`'s `EXTENTS` xmin is `-1.75`, an exactly representable
double whose low mantissa bytes (at `+0x2A`/`+0x2B`, immediately following the count) are
zero, so a 4-byte read at `+0x28` picked up two extra zero bytes and happened to still equal
171. Corpus files whose xmin isn't a clean fraction (`ADDER`, `FLOOR`, `BLIVET`, `ANDGATE`,
`HALFADD`, `NANDGATE`) exposed the error: their low mantissa bytes are non-zero, so the `u32`
read returns nonsense (billions, or implausibly small counts) while the `u16` read gives a
count consistent with the file's size and `SUBDIV`'s bytes-per-entity ratio. The field is a
`u16`.

**DXF (1983)** — *not* the modern group-code format. Record-oriented plain text:

```
KEYWORD,<n>
<comma-separated values>...
```

For entity records, `n` is the layer number, not a record count;
e.g. `LINE,1` followed by `8.800000,5.700000,19.000000,1.899999`. Undocumented but
self-describing. Files terminate with DOS EOF (`0x1A`). *(The example previously given here,
`1.012459,6.822910,1.261682,6.822910`, appears in no corpus file and caused a wrong reading
of the DWG entity order; the replacement is `SUBDIV.DXF`'s first `LINE`, verified present.)*

**Entity and block records (11)** — `LINE`, `POINT`, `CIRCLE`, `ARC`, `TRACE`, `SOLID`, `TEXT`,
`SHAPE`, `INSERT`, plus the `BLOCK`/`ENDBLK` definition delimiters. Confirmed as string
constants in the binaries.

**Header records (12)** — `EXTENTS`, `LIMITS`, `BASE`, `DWGVIEW`, `MODERES`, `MODEGRID`,
`MODEORTHO`, `MODEFILL`, `TXTSIZE`, `TRACEWID`, `LAYER`, `LAYERC`.

**Erased entities** — the record type code is a **signed** `i16`, and a negative value marks
an erased entity whose magnitude is its real type. `ADDER.DWG` holds 7 of them, including
`-1` (an erased `LINE`) and `-14` (an erased `INSERT`); read as unsigned they appear as the
nonsensical types 65535 and 65522. The record keeps its full body, which is presumably how
`OOPS` restores the last erase. Treating the code as signed and skipping negatives makes
`ADDER` walk exactly 66 records to `entity_end`, matching its header count; read as unsigned
the walk dies at record 42. `SUBDIV` has none and is unaffected.
The current DWG model retains ordinary erased records in place as `Item::Erased`;
the geometry iterator, renderer, and DXF writer omit them.

**Text height** — an `AC1.2` `TEXT` entity stores the font's full cell height;
the DXF reports cap height. `.SHP` fonts declare the
ratio in their header — `corpus/System/TXT.SHP` opens `*0,4,Roman Simplex` / `21,7,0,0`,
meaning 21 above the baseline and 7 below — so `dwg_height = dxf_height * (21+7)/21 = 4/3`.
Verified on `SUBDIV`: the DWG holds `0.4613465` where the DXF prints `0.346010`. Every text
font the 1.4 corpus ships is 3:1 (`ITALIC`, `ROMAN-C`, `ROMAN-S` and `System/TXT.SHP` at
`21,7`; `Samples/TXT.SHP` at `6,2`), so the factor is constant across this corpus — but it is
a property of the font, and a reader that hardcodes it says so. In `AC1.40`,
the binary height is already the DXF height: HOUSE/OFFICE exports and an
original TEXT command with height 0.75 confirm this. Applying the AC1.2
factor to AC1.40 makes the independent export comparison fail.

**Auxiliary formats** — `.SHP` shape/font files (`TXT`, `ROMAN-S`, `ROMAN-C`, `ITALIC`,
`ES`, `PC`), `ACAD.PAT` hatch patterns (5,120 B), `ACAD.MNU` screen menu (456 B, text with
embedded control codes — `[^Snap]\x02`, `[^Ortho]\x0f`, `\x03` for Cancel — which are
the literal bytes each menu item sends, not corruption).

### 4.3 Command set

`ACAD.MNU` gives the screen menu verbatim: `LINE ARC CIRCLE TEXT INSERT MOVE COPY CHANGE
FILLET ERASE OOPS BREAK REDRAW POINT TRACE SOLID ARRAY HATCH SKETCH DIM ZOOM LIST DIST
AREA STATUS TABLET FILES PLOT LIMITS GRID`, plus `SNAP`/`ORTHO` toggles and `ZOOM`
sub-options (`All`, `Win`, `Pre`). The full command set is a superset and is to be
recovered from `ACAD.OVL` in milestone ②.

### 4.4 Corpus integrity

The samples disk is a used 1983 working floppy and is **partly corrupt**:

- `SHUTTLE.DXF` — valid for 1,536 bytes, then another file's binary data is spliced in at
  an exact 512-byte sector boundary. Broken FAT cluster chain. **Unusable.**
- `SUBDIV.DXF` — clean and complete, proper `0x1A` terminator. 389 content lines, 183 records:
  `LINE`×133, `ARC`×8, `TEXT`×8, `INSERT`×8, `CIRCLE`×2, `BLOCK`/`ENDBLK`×6 pairs.
  Block definitions and loose entities are **interleaved** — 53 `LINE` records precede the
  first block (61 records counting the `ARC`s interleaved among them) and further blocks
  appear between inserts — so document order is content, and a writer that buckets by kind
  cannot round-trip. (Final review fix pass: this previously said "47 lines precede the
  first block", which was simply wrong — 53 and 61 are what
  `crates/acad-dwg/tests/entity_corpus.rs` verifies directly against real bytes.)

`SUBDIV.DWG` + `SUBDIV.DXF` is therefore the parallel corpus — the same drawing in binary
and text form — and is the primary lever for reversing DWG without an emulator in the loop.
Integrity validation of the whole corpus is a milestone ① deliverable, not an afterthought.

## 5. Architecture

```
acad-model    entity model; grows to 3D later
acad-dxf      1983 KEYWORD,count codec
acad-dwg      AC1.2 / AC1.40 binary codec
acad-cmd      1983 command semantics       <- compatibility boundary
acad-render   geometry -> frame
acad-app      window, input, command line
acad-re       (dev) Ghidra AST model and analyses
acad-oracle   (dev) in-tree 8086; runs real ACAD.EXE under cargo test
```

`acad-model` is the only crate every other crate depends on, and the only one written for
the future rather than for 1983. Divergence from the original happens by adding crates
beside `acad-cmd`, never by editing it.

`acad-re` and `acad-oracle` are dev-only: they are test and analysis infrastructure and
must not appear in the dependency graph of a shipped binary.

## 6. Static RE track

1. **Map `ACAD.OVL` — settled.** The EXE pages regions in and out, so the block layout
   follows the table it reads (§4.1). Language is `x86:LE:16:Real Mode`. The loader script
   therefore builds:
   - two flat blocks for `ACAD.EXE` — code at segment base, data at `+0x0E15` paragraphs;
   - **22 Ghidra overlay blocks**, one per region of the 11 directory entries, because the
     entries share window addresses and cannot coexist in one flat space. Code regions map
     into the 64,768 B window above the image; data regions map into the EXE's own data
     segment at `DS:0x0002-0x3552`, which the EXE reserves (13,651 bytes, verified all-zero
     in the image, with the string pool starting immediately after at `DS:0x3553`).

   The load protocol, for the record: `sprintf("%c:%s.OVL", drive, "ACAD")` → `fopen "rb"` →
   `fread` 211 B → magic check → close; reserve `(hdr.u16@11 + 15) >> 4` paragraphs or die with
   `"Not enough core for overlays"`; then a chain loop from entry 0 in which each overlay's
   return value selects the next and `0` quits, paging via FCB random block reads and failing
   to `"Overlay load error"`. The FCB mechanics come from the decompiler, not from bytes, and
   are to be re-confirmed when the loader is written.
2. **Headless and reproducible.** `analyzeHeadless` driven by PyGhidra scripts committed
   to the repo. The pipeline runs from the raw `.img` files; no project state lives outside
   version control.
3. **Dual AST export**, both to JSON keyed by function address:
   - high P-Code — `HighFunction.getPcodeOps()`, SSA-form, language-neutral
   - Clang markup — `DecompileResults.getCCodeMarkup()`, a C AST whose nodes link back to
     P-Code varnodes
4. **`acad-re`** deserializes both into a typed Rust AST and answers: the call graph, the
   overlay-to-command mapping, the DWG record writer's data flow, and the geometry
   algorithms (notably arc tessellation and `FILLET`).

Cross-checks: Bochs' internal debugger for deterministic instruction traces; rizin as a
second disassembler where Ghidra's segmented 16-bit output is doubtful.

## 7. Oracle track

`acad-oracle` embeds an 8086 core so `cargo test` boots the real `ACAD.EXE`, scripts
keyboard input, and captures the `.DWG` it writes. In-tree rather than DOSBox because tests
must be deterministic, dependency-free and CI-runnable.

Ground truth is **generated, not curated**: a test asks the original to draw a circle of
radius *r* and diffs its output against the Rust implementation's, for any *r*.

## 8. Milestones

**① DXF codec, corpus integrity, first render.** Validate every file on all three disks and
publish a manifest of what is intact; implement the 1983 DXF codec; render `SUBDIV.DXF` in
a window. No RE required. *Done when* `SUBDIV.DXF` round-trips byte-identically and renders.

**② Ghidra loader and AST export.** Overlay loader, headless pipeline, dual AST export,
`acad-re` with call graph and overlay→command map. *Done when* the pipeline reproduces from
raw images on a clean machine and the full command set is recovered.

**Decision gate (end of ②).** Transpilation of the AST to Rust is adopted only if all three
hold: ≥70% of `ACAD.OVL` functions decompile without `halt_baddata` or
`UNRECOVERED_JUMPTABLE`; cross-overlay calls resolve to named targets; the DWG entity
record is recoverable as a coherent struct. Otherwise `acad-re` remains an understanding
tool and all shipped Rust is hand-written. `acad-re` itself is unchanged either way — only
the consumer of its output differs. This gate also re-settles the §3 posture on evidence.

**Gate outcome (2026-09-28): not passed, 1 of 3.** Measured in `docs/re-pipeline.md`.
98.0% of functions decompile cleanly, so the first criterion is met. There are no
cross-overlay calls at all — the overlays share one window, so a direct call between them
could not work at runtime — and no overlay call resolves into `EXE_CODE`, so the second is
not demonstrated. No `STORE` in any analysed function has a constant pointer operand, because
every store address is built by `SEGMENTOP`, so the DWG entity record is not recoverable as a
struct and the third fails. **Transpilation is therefore not adopted**: `acad-re` remains an
understanding tool, all shipped Rust is hand-written, and the §3 clean-room posture stands.

The gate is settled and is not reopened to chase transpilation. It decided the *consumer* of
`acad-re`'s output, not `acad-re`'s usefulness — so the unresolved kernel function-pointer
table is pursued only when a specific question in ④ or ⑤ needs it, on demand rather than
speculatively.

**③ Oracle harness.** 8086 core, scripted input, output capture, first generated
differential test.

**Oracle implementation status.** QEMU-backed development tests generate DWG,
DXF, and CGA references from the original executable. The in-tree `Cpu8086`
provides a 1 MiB real-mode bus and the instruction/8087 subset exercised by
startup and an empty drawing save. The DOS machine supplies the PSP,
environment, paragraph allocation, BIOS video subset, keyboard polling,
interrupt vectors, floppy free-space query, FCB file operations, and DTA.

The MZ header declares 78,340 bytes inside a 78,848-byte FAT file. An aligned
QEMU debugger trace identified an in-tree loader mismatch: the C runtime's
`0123456789ABCDEF` conversion table at `DS:508Bh` is in the final 508 bytes,
beyond the declared MZ length. QEMU has it in memory; the former in-tree
loader discarded it. With the table absent, identical `sprintf` arguments
`(1, '-', 1, 106)` produced `00-000000` in-tree instead of QEMU's
`01-010106`. The resulting mismatch entered a memory-checksum routine at
`2ABB:5975`; its zero low-three-bit result led to a far copy over the live
stack. Loading the full final 512-byte page restores the conversion table,
produces the QEMU string, and avoids that path. The earlier physical-memory
snapshot differences were real but were not the cause of this divergence.

The runner now handles FCB random write and rename. Single-record FCB random
reads/writes keep the caller's random-record pointer; block operations advance
it. This matches the original DOS behavior observed here and documented in
the MS-DOS FCB reference. The public `generate_dwg_in_tree` runner accepts
scripted editor lines. Empty, LINE, POINT, CIRCLE, ARC, SOLID, and TRACE
`AC1.40` DWGs match QEMU byte for byte. TEXT differs only in the computed
maximum Y extent at header offset `0x4a` by one ULP. The 8087 core now retains
low bits through register arithmetic and rounds to the control-word precision,
but this particular mismatch persists; its arithmetic path remains to be traced. The public
`generate_visual_dwg_in_tree` captures CGA memory before END; LINE and ID
screens each match QEMU across the entire 16 KiB frame. Broader command coverage,
full DOS/BIOS services, and complete 8087 precision remain open.

**④ DWG codec.** `AC1.40` then `AC1.2`, verified against `SUBDIV` in both directions and
against oracle-generated drawings.

**Sequencing amendment (2026-09-28).** ④'s *read* direction runs before ③. §4.4 names
`SUBDIV.DWG` + `SUBDIV.DXF` as the primary lever for reversing DWG "without an emulator in
the loop", and §9's reason for putting the oracle first — that it supplies the expected side
so tests do not invent expectations — is already satisfied for this half by the parallel
corpus: `SUBDIV.DWG` must decode to the `Drawing` that `SUBDIV.DXF` decodes to.

**This inverts ④'s internal order: `AC1.2` comes first, not `AC1.40`.** `SUBDIV.DWG` is
`AC1.2` — a magic census of the sample disk gives 16 `AC1.2` drawings against 5 `AC1.40`
(`HOUSE`, `COLORS`, `OFFICE`, `SHUTTLE`, `DISC`), and the only file with DXF ground truth is
`AC1.2`. Reversing `AC1.40` first would mean inventing expectations for the one format that
cannot be checked, which is what §9 exists to prevent.

Scope of the early half: decode `AC1.2` into `acad-model`, verified by that equality and by
every `AC1.2` drawing in the corpus loading and rendering. `AC1.40`, the write direction, and
verification against oracle-generated drawings stay after ③.

Current progress: the QEMU bootstrap now generates the expected side for
`AC1.40`. The reader passes command-generated DWG/DXF comparisons and five
sample exports, including `DISC.BAK`. LOAD is a length-prefixed name in DWG
and one name row in DXF. SHAPE is x/y/scale/radians as four doubles, then a
u16 definition number; DXF is one x,y,scale,degrees,number row. Original
`LOAD B:ES` plus `SHAPE RES`/`SHAPE CAP` commands verify the layouts and
definition IDs 129/130 from ES.SHP. Both records retain document order in
the model, including block bodies. Their AC1.2 layouts lack independent
evidence because no AC1.2 corpus file uses them. `.SHP` rendering now supports
the instructions used by all seven supplied libraries, with font changes,
subshapes, spacing, and placement through nested block transforms. A generated
CGA-memory test compares Roman AA and an ES resistor against native strokes.
See `docs/shp-rendering.md` for evidence and limits. Entity layer fidelity,
repeat semantics, and colors/fills are implemented. Both DWG writers open in
the original under QEMU; an AC1.2 SUBDIV rewrite matches its original drawing
viewport pixel-for-pixel. The meanings of several retained header fields and
source block-definition physical ordering remain open.

**⑤ Command loop.** Screen menu, command line, entity creation and editing commands,
each backed by a differential test. The first command engine slice now supports
LINE, CIRCLE, POINT, three-point ARC, TEXT, UNDO, SAVE, END/QUIT, and numeric
factor and Previous ZOOM. Its LINE/ARC/TEXT entity output and ZOOM's saved
view height/Previous behavior are checked against the original under QEMU.
The window accepts keyboard input, redraws after edits, and honors the
drawing's saved view; numeric, Previous, Extents, Window and Center ZOOM modes
update the saved view; coordinate-based PAN changes its center while keeping
the current view height. Remaining commands and the full AutoCAD menu/command
behavior are still open.

The command engine also has LIST plus ERASE, MOVE, COPY, ROTATE and SCALE over
one-based IDs (or `ALL`), with model tests for geometry changes and undo.
QEMU checks the original's `LAST` selection for ERASE, MOVE, and COPY; BLOCK
and INSERT have further original-command probes below. Point and window
selection and the screen-menu flow still need differential coverage.

`INSERT` now places an existing block by name, with an insertion point,
independent X/Y scales, and rotation; blank scale and rotation prompts use
1/1/0 defaults. Block lookup ignores case and stores the block's canonical
name, and unknown blocks or zero scales leave the drawing unchanged.
The locally defined block flow is now compared against the original under
both in-tree execution and QEMU: explicit scales 2/3 and 30° rotation produce
the same insertion, allowing for DWG radians-to-degrees rounding. A blank Y
scale after X=2 becomes 2 in the original, despite `ACAD.HLP` saying its
default is 1; Rust follows the observed runtime behavior. `INSERT *B1` skips
the scale and rotation prompts and copies B1's component entities, translating
them by insertion point minus block base. The Rust result and its saved DWG
match the original's decoded entities, and the original in-tree DWG matches
QEMU byte for byte. External drawing-file insertion remains open.
At the X-scale prompt, an opposite corner point also sets both scales: the
original saves the coordinate differences from the insertion point as X and Y
scales, then asks for rotation. A generated `(3,3)` → `(5,6)` example saves
2/3 in both oracles and the Rust command. Negative numeric X/Y scales are
accepted by the original and Rust for mirrored inserts; zero does not create
an insert in the original probe and remains invalid in Rust.

`BLOCK` now creates a named definition from selected top-level entities.
The original's `LAST` flow was captured with a nonzero base point: it
uppercases the name, retains the members' drawing coordinates, marks their
source records erased in place, and appends the block after those records. The
Rust command reproduces that item list; its AC1.40 save reopens with the same
items, and the in-tree original's DWG matches QEMU byte for byte. Rust also
accepts the existing ID/`ALL` selection interface, which remains to be
compared with the original's point and window selection behavior.

`OOPS` restores the last `ERASE` at the original item positions, including
around block definitions and `LOAD` records. `UNDO` can reverse an `OOPS` and
then restore its erased set again. QEMU now verifies `ERASE L` and `OOPS` on a
LINE: the original saves type `-1` after erasure and `+1` after restoration,
and Rust writes the same entity bytes.

Rectangular `ARRAY` copies a selected set into a row/column grid with signed
row and column spacing. After selection it asks for rectangular or circular
mode; Rust currently supports `R`. The original asks for rows, columns, row
spacing, then column spacing and writes the copies in column-major order.
The in-tree runner and QEMU agree on a generated 2×3 example, and the Rust
item list matches both. One `UNDO` reverses the complete operation. The
implementation rejects dimensions that overflow or generate over 100,000
entities. Circular mode and point-specified spacing remain open.

`CHANGE` assigns selected top-level entities to an existing or new layer,
preserves their geometry and other entities, and is reversible with one
`UNDO`. The command and header-layer update are model-tested; differential
verification against AutoCAD remains open.

`FILLET` accepts two top-level `LINE` entities whose segments intersect and a
positive radius. It uses the nearest endpoint on each line to choose the
corner branch, trims both lines to the tangent points, and adds the circular
arc on the current layer. Parallel lines, non-line entities, non-intersecting
segments, and radii that exceed the available segment lengths are rejected
without mutation. Right-angle geometry, rejection cases, and undo are
model-tested; differential verification against AutoCAD remains open.

`BREAK` removes the interval between two interior points on a selected line,
leaving the two ordered remainder segments on the original layer. On a circle,
it removes the counter-clockwise arc between two circumference points and keeps
the complementary arc on the original layer. Off-geometry, endpoint, and
coincident points are rejected. Line, circle and arc splits, layer
preservation, undo, wraparound angles, and rejection cases are model-tested.
Oracle comparison remains open.

The QEMU oracle shows `DIST` reporting `Distance=5.0000` for `(0,0)` to
`(3,4)`. It shows `AREA` accepting polygon vertices `(0,0)`, `(4,0)`,
`(4,3)`, then a blank line, and reporting `Area = 6.0000`. The Rust command
loop now follows those input and output formats. `ENTITYAREA` is a separate
Rust extension for measuring circles, `TRACE`/`SOLID` quadrilaterals, and
selected closed loops of `LINE` entities; open or unsupported selections
return an error. Model tests cover the reported values and rejected open loops.
Further query edge cases remain unverified against the original.

`ID` now accepts a point, reports X and Y to four decimal places, and returns
to the command prompt without changing the drawing. Its prompt and coordinate
display were checked on a captured original editor screen after `ID`, `3,4`;
the in-tree capture of that screen matches QEMU byte for byte. The Rust command
has a model test for the reported coordinates and unchanged drawing.

`SOLID` takes four points for the first quadrilateral. It then reuses the
third and fourth points as the first two points of the next quadrilateral,
accepting another third/fourth pair until a blank line. A two-solid QEMU
export establishes both the file-order corners and this chaining behavior;
the Rust command's item list is compared directly against that export.

`TRACE` accepts a width and a chain of centerline points. At a bend it
miters both sides: the QEMU export of a width-0.5 path `(1,1)` → `(4,1)` →
`(4,4)` places the shared corners at `(3.75,1.25)` and `(4.25,0.75)`, and
saves `TRACEWID=0.5`. The Rust command reproduces both trace records and the
header value. Command-generated DXF establishes that `TRACE` and `SOLID`
place the four corners on two coordinate rows, which the writer now matches.

## 9. Testing

TDD throughout. For `acad-cmd` and the codecs the oracle supplies the expected side, so
tests are written before implementation without inventing expectations. `acad-model`,
`acad-render` and `acad-app` are tested conventionally. The corpus manifest from ① is a
fixture: corrupt files are excluded explicitly and by name, never silently.

## 10. Risks

| Risk | Impact | Response |
|---|---|---|
| ~~Mapping `ACAD.OVL` correctly~~ | ~~Blocks all of §6~~ | **Retired.** The directory is decoded and tiles 99.3% of the file; the layout is static data, not behaviour (§4.1) |
| ~~Zero relocations, self-managed segments~~ | ~~Poor Ghidra output~~ | **Retired.** `DS = SS = 0x0E15` recovered statically; no Bochs trace needed. Bochs remains available for §6 cross-checks |
| Overlay blocks obscure cross-overlay calls | Weakens the §8 gate's "cross-overlay calls resolve to named targets" criterion | Entry points are known (`+0x10` per entry); resolve call targets against the directory rather than against Ghidra's flat address space |
| Decompiler quality on 16-bit C | Decides the §8 gate | Gate exists precisely to measure this |
| Further corpus corruption | Weakens ground truth | ① validates everything up front; oracle can regenerate drawings |
| 1983 floating-point semantics | Subtle geometry mismatches | DWG holds IEEE doubles; confirm no software-FP divergence during ④ |

## 11. Deferred decisions

- **AST consumption** — settled at the §8 gate on stated criteria.
- **Later AutoCAD releases** — separate specs; archives already in `autocad/`.
- **Rendering backend** — chosen in ① when the first window is needed; not load-bearing
  for anything above it.
