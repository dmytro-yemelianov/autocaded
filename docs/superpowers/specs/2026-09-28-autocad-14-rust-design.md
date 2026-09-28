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
| `ACAD.EXE` | MZ, 512 B header, **0 relocation entries**, `CS:IP = 0xFFF0:0x0100` (resolves to image start), `SS:SP = 0x0E15:0x51A0`. Near-empty string table — this is the kernel/loader. |
| `ACAD.OVL` | Named overlay container. Directory entries observed: `!root` (24064), `!config` (24070), `!plot` (59760), `!menu` (59766), `!IDD` (132182), `!plot` (172924), `!end` (172947), `!quit` (172952). Holds the application code and essentially all strings. |
| Drivers | Separate files per device class: display `DS*.DRV`, digitizer `DG*.DRV`, plotter `PL*.DRV`. A pre-existing device abstraction, each driver 0.5–5 KB. |

Zero relocations across a 78 KB image means the program performs its own segment
arithmetic. This is precisely the case 16-bit Ghidra handles worst, and is a named risk (§10).

### 4.2 File formats

**DWG** — magic is an ASCII version stamp: `AC1.2` and `AC1.40` both occur in the 1.4
sample corpus, so the codec must handle two versions and the corpus itself documents the
format's evolution. `ACAD.OVL` contains the literal `AC1.40` it stamps into files it writes.

**DXF (1983)** — *not* the modern group-code format. Record-oriented plain text:

```
KEYWORD,<record-count>
<comma-separated values>...
```

e.g. `LINE,1` followed by `1.012459,6.822910,1.261682,6.822910`. Undocumented but
self-describing. Files terminate with DOS EOF (`0x1A`).

**Entity and block records (11)** — `LINE`, `POINT`, `CIRCLE`, `ARC`, `TRACE`, `SOLID`, `TEXT`,
`SHAPE`, `INSERT`, plus the `BLOCK`/`ENDBLK` definition delimiters. Confirmed as string
constants in the binaries.

**Header records (12)** — `EXTENTS`, `LIMITS`, `BASE`, `DWGVIEW`, `MODERES`, `MODEGRID`,
`MODEORTHO`, `MODEFILL`, `TXTSIZE`, `TRACEWID`, `LAYER`, `LAYERC`.

**Auxiliary formats** — `.SHP` shape/font files (`TXT`, `ROMAN-S`, `ROMAN-C`, `ITALIC`,
`ES`, `PC`), `ACAD.PAT` hatch patterns (5,120 B), `ACAD.MNU` screen menu (456 B, plain text).

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
- `SUBDIV.DXF` — clean and complete, proper `0x1A` terminator. 390 lines, 177 records:
  `LINE`×133, `ARC`×8, `TEXT`×8, `INSERT`×5, `BLOCK`×3, `ENDBLK`×6, `CIRCLE`×2.

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

1. **Custom Ghidra loader** (Java) parsing the `!name` overlay directory and creating one
   memory block per overlay at its runtime segment, language `x86:LE:16:Real Mode`.
   Without this, cross-overlay calls do not resolve and every later step is degraded.
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

**③ Oracle harness.** 8086 core, scripted input, output capture, first generated
differential test.

**④ DWG codec.** `AC1.40` then `AC1.2`, verified against `SUBDIV` in both directions and
against oracle-generated drawings.

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
| Overlay loader harder than expected | Blocks all of §6 | First item of ②; timebox and reassess before building on it |
| Zero relocations, self-managed segments | Poor Ghidra output | Bochs traces to recover the segment map empirically |
| Decompiler quality on 16-bit C | Decides the §8 gate | Gate exists precisely to measure this |
| Further corpus corruption | Weakens ground truth | ① validates everything up front; oracle can regenerate drawings |
| 1983 floating-point semantics | Subtle geometry mismatches | DWG holds IEEE doubles; confirm no software-FP divergence during ④ |

## 11. Deferred decisions

- **AST consumption** — settled at the §8 gate on stated criteria.
- **Later AutoCAD releases** — separate specs; archives already in `autocad/`.
- **Rendering backend** — chosen in ① when the first window is needed; not load-bearing
  for anything above it.
