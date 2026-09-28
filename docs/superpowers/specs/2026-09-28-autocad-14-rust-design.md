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
| `ACAD.EXE` | MZ, 512 B header, **0 relocation entries**, `CS:IP = 0xFFF0:0x0100` (resolves to image start), `SS:SP = 0x0E15:0x51A0`. Image is 78,340 B; the trailing 508 B of the 78,848 B file are cluster slack. Segment model recovered statically (2026-09-28 spike ②): **`DS = SS = 0x0E15`**, code `0x0000-0xE150` (57,680 B), data `0xE150-0x13204` (20,656 B). Not a bare kernel — it holds the overlay loader *and* a substantial string table at `DS:0x3553-0x5000` (62% non-zero): entity type names, `EREGEN` errors, shape/font parser errors, DWG paging errors, the `A U T O C A D - 8 6` / `1.40` / `03/10/84` banner. |
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
Census of the sample disk: 16 `AC1.2`, 5 `AC1.40` (`HOUSE`, `COLORS`, `OFFICE`, `SHUTTLE`,
`DISC`; the `.BAK` copies match their drawings). `SUBDIV.DWG`, the parallel-corpus file, is
`AC1.2`.

Header layout, established 2026-09-28 against `SUBDIV.DXF`'s own header values and identical
in shape in both versions: `u32` at `+0x24` = one past the last byte of entity data (`0x19E5`
for `SUBDIV`, whose file is `0x1A00` — the rest is cluster slack); `u16` at `+0x28` = the
entity record count (171 for `SUBDIV` = 159 entities + 6 `BLOCK` + 6 `ENDBLK`); then IEEE
doubles from `+0x2A` — `EXTENTS` min/max as 3D points, `LIMITS` min/max as 2D pairs at
`+0x5A`, `DWGVIEW` centre and height at `+0x7A`, `MODERES`/`MODEGRID` as `u16`+`f64` pairs at
`+0x9A`, `TXTSIZE` at `+0xB4`, `TRACEWID` at `+0xBC`. Entity records begin at a fixed `+0x1D8`,
constant across all 16 `AC1.2` drawings, and run in **the same order as the DXF**, ending at
the `+0x24` offset. Each record is a `u16` type code, a `u16` whose meaning is not yet known,
then the type's fields as IEEE doubles — so `LINE` is 36 bytes and `ARC` 44.

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
KEYWORD,<record-count>
<comma-separated values>...
```

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

**Text height** — a `TEXT` entity's height is stored differently in the two formats. The
DWG holds the font's full cell height; the DXF reports cap height. `.SHP` fonts declare the
ratio in their header — `corpus/System/TXT.SHP` opens `*0,4,Roman Simplex` / `21,7,0,0`,
meaning 21 above the baseline and 7 below — so `dwg_height = dxf_height * (21+7)/21 = 4/3`.
Verified on `SUBDIV`: the DWG holds `0.4613465` where the DXF prints `0.346010`. Every text
font the 1.4 corpus ships is 3:1 (`ITALIC`, `ROMAN-C`, `ROMAN-S` and `System/TXT.SHP` at
`21,7`; `Samples/TXT.SHP` at `6,2`), so the factor is constant across this corpus — but it is
a property of the font, not of the format, and a reader that hardcodes it says so.

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
  Block definitions and loose entities are **interleaved** — 47 lines precede the first
  block and further blocks appear between inserts — so document order is content, and a
  writer that buckets by kind cannot round-trip.

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

**⑤ Command loop.** Screen menu, command line, entity creation and editing commands,
each backed by a differential test.

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
