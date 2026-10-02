# Reverse-engineering pipeline

Reproduces the Ghidra analysis of AutoCAD 1.4 from the raw disk images.
Full exports stay in ignored artifact directories. Small regression excerpts
are committed under `crates/acad-re/tests/fixtures/` with source provenance.

## Prerequisites

- Ghidra 11.3+ (developed against 12.1.3). macOS: `brew install ghidra`.
- A JDK 21. macOS: `brew install openjdk@21` — it is keg-only, so it is not on
  `PATH`; `tools/ghidra-env.sh` finds it anyway.
- Python 3.11+ for the PyGhidra venv. The wheels come from inside the Ghidra
  install, not PyPI, so the bindings always match the installed Ghidra.
- The corpus: `./tools/extract-corpus.sh`.

Check the environment with `./tools/ghidra-env.sh`. It names the first missing
prerequisite and how to install it, and exits non-zero.

## Running it

    ./tools/re-pipeline.sh

Outputs land in `build/`:

| File | What it is |
|---|---|
| `ovl-map.json` | Where every overlay region lives, from `acad-re` |
| `pyghidra-venv/` | The PyGhidra virtualenv, built on first use |
| `ghidra/acad14.gpr`, `ghidra/acad14.rep/` | The Ghidra project |
| `ast-pcode.json` | High P-Code, SSA form, keyed by function address |
| `ast-clang.json` | Clang markup, a C AST linked back to P-Code varnodes |
| `re-report.json` | The §8 decision-gate metrics |
| `analysis-map.json` | Synthetic layout preserving native 16-bit IPs |
| `recovered-cfg.json` | Instruction P-code, file offsets, inline operands, and recovered control flow |

## Why the format is decoded in Rust

`acad-re` owns the `ACAD.OVL` container format and emits `ovl-map.json`. The
PyGhidra scripts only place bytes where that file says. One implementation, one
set of tests, and the layout stays covered by `cargo test`.

## Corrected lifting — 2026-10-02

The pipeline now converts JPype instruction lengths to Python integers and
normalizes code addresses explicitly. Resident code begins at logical IP
`0x0100`, as specified by the MZ header. The synthetic image-load segment is
`0x1010`, resident CS is `0x1000`, DS/SS is `0x1E25`, and overlay analysis CS is
`0x2000`. These are analysis addresses, not measured DOS allocation addresses.
Aligning CS preserves the 16-bit relative-call wrap and saved return IP in
Ghidra's x86 P-code. The original container map remains separate.

`recovered-cfg.json` follows compiler conventions that the automatic listing
misses: inline byte/word stack sizes, embedded switch tables, and kernel-call
thunks whose operands are resident function offsets. Each instruction records
its original EXE/OVL file offset and verified bytes. Indirect flow remains an
explicit edge. A kernel thunk reads the function word and transfers through
DS:4D76/4D78; the initialized offset and resident dispatcher recover the bridge
at native IP `0x8F80`.

Entry 2's dispatch wrappers select entries 3–7, load their low code ranges, then
near-call their entry offsets under the same CS. Their writes stop before entry
2's high code range. This establishes shared helpers on that dispatch path;
DIM's inline stack size `0x82` at IP `0x0107` is data, and execution continues at
`0x0108`. Directory entries 8/9 and 10 have byte-identical code **and data**
subranges of entries 2 and 1 respectively. Recovery canonicalizes those views;
it does not invent standalone runtime states for them.

The exercised final flow pass recovered **1,054 candidate functions**, **68,631
distinct instructions**, **329 inline stack sizes**, **62 switch tables**, and
**240 kernel-call sites**. There were zero decode errors, original-byte
mismatches, cross-function instruction overlaps, or instruction/inline-data
overlaps. Six focused offline regression tests passed. The normal export entry
point was also executed on a fresh project; the final directory-view refinement
was executed against a separate copy of that normalized project.

The first rerun's 30 high-P-code boundary flags comprise 19 shared tails/entry
wrappers, ten incorrect instruction streams, and one data-segment phantom
function. They are classified in the recovery ledger. Legacy high-P-code/Clang
exports remain candidate decompiler output; their marker count is not proof of
semantic correctness. Use the reconstructed CFG and compiler operands for the
next IR work. Indirect targets, memory semantics, entity structures, and Rust
lowering still need work; the older decision gate below is historical.

Recorded runs and receipts are under
`target/acad-recovery-pipeline-20261002-01/` and
`target/acad-flow-recovery-20261002-03/`. Re-run the focused checks with:

```sh
PYTHONDONTWRITEBYTECODE=1 python3 -m unittest discover -s tools/ghidra -p test_recover_flow.py -v
```

## Bounded CFG → IR → Rust — 2026-10-02

`acad-re::ir` lowers original-byte-checked instruction Pcode to a typed integer
machine-state IR. `acad-re::rust_emit` emits standalone Rust from that IR; it does
not substitute a manually written implementation. The export must include
measured language, RAM-space ID, and userop names. `CALLOTHER` is accepted only
for the verified x86 real-mode `segment` operation. No shipped crate depends on
this code, and the historical whole-port decision below remains historical.

The first supported subset has 1/2/4/8-byte integer values, byte-addressed
registers preserving aliases, temporary storage with checked definitions,
conditional branches, LOAD/STORE, and returns. All instruction targets must
resolve within the selected function. Unsupported operations, compiler inline
data, call dependencies, or incomplete flow fail before emission. `re-inventory`
checks original bytes for every candidate and reports the first lowering
rejection for each; it does not certify behavior.

Generated `run` takes a Ghidra register bank (`[u8; 1024]`), mutable linear RAM,
and an instruction budget. Memory accesses are little-endian and bounds checked;
segment addresses are `(segment << 4) + offset`. There is no implicit A20 wrap or
DOS relocation claim. Register and pointer arithmetic retain their Pcode widths.
A budget or memory trap can occur after earlier effects; execution is not a
transaction. Calls, indirect transfers, x87 values, and general microbranches
are still unsupported.

A fresh read-only recovery pass on a copied normalized project completed in
13.27 seconds, preserving the 1,054 candidates and adding architecture metadata.
Artifacts are in `target/acad-ir-20261002-01/`: `export/recovered-cfg.json`,
`execution.json`, `ir-inventory-final.json`, `translation-execution.json`, and
`functions/`. Eight functions were lowered and compiled as standalone Rust
libraries with warnings denied. Four copies have independent instruction-level
reference checks:

- `OVL04_CODE:1778` and `OVL01_CODE:4F38`: shift DX:AX left by unsigned CX,
  capped at 32; 76,544 cases per copy, including every CX value.
- `OVL02_CODE:F332` and `EXE_CODE:B12A`: test the word at DS:(SP+8) against
  `7FF0`, conditionally XOR the byte at DS:(SP+9) with `80`; 65,581 cases per
  copy, including every word value and segment/offset boundaries.

Checks compare complete register banks, flags, return addresses and memory.
They compile and run the emitted Rust; the reference never evaluates Pcode or
IR. Committed fixtures retain original CFG/image hashes and only the referenced
userop name. The other four accepted functions have compilation validation
only. No fresh native differential execution or DIM geometry parity is claimed.

Replay a committed slice against the preserved original images:

```sh
cargo run -p acad-re --bin re-lower -- \
  crates/acad-re/tests/fixtures/dim-shift-cfg.json OVL04_CODE 1778 \
  target/acad-ir-20261002-01/input target/dim-shift-rust-replay
rustc --edition=2021 --crate-type lib -D warnings \
  target/dim-shift-rust-replay/function.rs -o target/dim-shift-rust-replay/function.rlib
cargo test -p acad-re --test ir
```

The output directory must be new. For a complete inventory:

```sh
cargo run -p acad-re --bin re-inventory -- \
  target/acad-ir-20261002-01/export/recovered-cfg.json \
  target/acad-ir-20261002-01/input target/ir-inventory-replay.json
```

The report file must be new. Next work is compiler helper/call lowering, followed
by unresolved control flow and the integer/x87 operations required by the chosen
geometry routines. Structural inventory counts do not replace semantic tests.

## Results — 2026-09-28

Ghidra 12.1.3, `openjdk@21`, PyGhidra 3.1.0, Python 3.13. Reproduced from a
clean `corpus/` and `build/`: `rm -rf corpus build && ./tools/extract-corpus.sh
&& ./tools/re-pipeline.sh` gives the same numbers.

```
functions        151 (148 clean, 3 marked, 0 failed)
clean ratio      98.0%  (§8 threshold 70%: MET)
non-code         17 functions outside any code block, 11 marked
call edges       346
cross-overlay    0 edges, 0 to named targets
clang AST        168 functions, 0 failures
```

`ACAD.EXE` contributes 82 functions and the 11 overlays 69, across all 11 code
blocks. Seeding each overlay's entry point from the directory is what makes that
possible: without it Ghidra finds functions in only 4 of the 11 blocks.

The 17 non-code functions are in `EXE_DATA`, the EXE's data segment, where the
overlay data windows page in. 11 of the 17 carry `halt_baddata` or
`UNRECOVERED_JUMPTABLE`, against 3 of 151 in code — decompiling data as code.
They are reported separately rather than counted in the gate.

### The full command set — 57 commands

Recovered from a single NUL-terminated, space-separated table at file offset
`0x2b4f4`, inside entry 2's data region, which the dispatcher indexes:

```
LINE POINT CIRCLE SHAPE REPEAT ENDREP TEXT ARC TRACE LOAD SOLID LIST INSERT BASE
ORTHO LAYER GRID LIMITS ID RES RESOLUTION ZOOM PAN MOVE ERASE MENU REDRAW STATUS
REGEN DBLIST DIST CHANGE END QUIT ? AREA OOPS TABLET PLOT DELAY RESUME COPY BLOCK
DIM QPLOT SNAP FILL HELP UNITS ARRAY WBLOCK AXIS HATCH FILLET BREAK SKETCH FILES
```

A strict superset of `ACAD.MNU`'s 30 (spec §4.3), which is the check the tests
make. Which overlay *implements* each command is not recovered — that needs the
dispatcher that indexes this table.

### §8 decision gate

**1. ≥70% of `ACAD.OVL` functions decompile without `halt_baddata` or
`UNRECOVERED_JUMPTABLE` — MET.** 148 of 151, 98.0%.

No framing of the population decides this. Taking the spec's words literally —
`ACAD.OVL` functions only — it is 68 of 69, 98.6%. Counting every function
including the 17 non-code ones, it is 154 of 168, 91.7%. The reported figure sits
between them. All three clear 70%, so the choice to exclude the non-code
functions, which is the one that flatters the number, does not change the answer.

What the criterion does *not* measure is how much of the 179,480 bytes Ghidra
reached: 151 functions is what it found, not what is there.

**2. Cross-overlay calls resolve to named targets — NOT DEMONSTRATED, and the
criterion is mis-specified.** There are no cross-overlay calls: every overlay
call goes to its own overlay (51 edges) or nowhere resolvable. That is what the
design predicts — the overlays share one 64,768-byte window, so a direct call
from one to another could not work at runtime; the chain runs through
`ACAD.EXE`, which loads the next overlay and far-calls its entry point.

The substantive question behind the criterion is whether *overlay-to-kernel*
calls resolve, and there the answer is no: **0** edges reach `EXE_CODE`, while
106 land on the invented functions in `EXE_DATA`. `ACAD.EXE` carries the string
`UNINITIALIZED FCN PTR` at `DS:0x4DA8`, and the overlay dispatcher at image
offset `0x8e61` ends in an indirect call through a variable. Overlays appear to
reach the kernel through a function-pointer table in the data segment, which
Ghidra has not resolved. Resolving it is the next piece of work on this track.

**3. The DWG entity record is recoverable as a coherent struct — NOT MET.**

The record could not be recovered, for a structural reason rather than a missing
step. `acad-re::analysis::trace_stores` reads a layout off `STORE`s whose pointer
operand is a constant, which is how a struct write looks in flat code. In 16-bit
segmented code there are none — not one, in any of the 151 gate functions, nor in
all 168. Every store address is built by `SEGMENTOP(space, segment, offset)`, so
the pointer is always a computed `unique` varnode: of the 459 stores in code
blocks, 407 have a `SEGMENTOP`-defined pointer whose constant operand is the
*segment* (`0x1E15`), not a displacement. (Over all 168 functions it is 5,099 of
5,314, but 4,855 of those stores belong to the 17 non-code functions excluded
above, so the code-block figure is the one that bears on this.) Walking one level
further reaches a `PTRADD` whose constant is the element size, not a field
offset. `crates/acad-re/tests/dataflow.rs` pins this, so an improvement trips
it.

Nor could the writer be located by name. `ACAD.EXE` holds `EREAD`, `EWRITE`,
`EWRITE error`, `** ILLEGAL TCODE VALUE %d IN %s` and
`BAD ENTITY TYPE %d PASSED TO EREGEN` at `DS:0x36B3`–`DS:0x36FB`, and the entity
type table at `DS:0x38EE`. None appears in the 4.7 MB of decompiled C: Ghidra
resolves only 13 string symbols across the whole program, and every function is
named `FUN_*`. The same holds for the geometry markers,
`Improper argument (%g) to SQRT.` at `DS:0x3942` and `ACOS undefined for %g` at
`DS:0x397A`, so **arc tessellation and `FILLET` were not located either** — the
other half of §6.4 that this track owes.

This is not the overlay layout's fault, and it is not an unset segment register
either — the two hypotheses worth ruling out, and both were.

Loading three ways — `ACAD.EXE` alone, plus code overlays, plus all overlays —
gives identical string resolution, so the 22 overlay blocks neither help nor hurt
here. They do add 16 phantom functions in `EXE_DATA`, which is where the 17
non-code functions above come from.

Separately, the overlay code blocks originally got no segment-register context at
all: `CS`/`DS`/`SS`/`ES` were set over `EXE_CODE` only, so 69 of the 151 functions
were analysed with an unknown `DS`. Setting them over every `OVLxx_CODE` block
too more than tripled the resolved data references (`DAT_` symbols went from
1,039 to 3,513) and changed **none** of the numbers above: same 151/148/3, same
346 edges, same zero cross-overlay edges, same zero constant-offset stores, and
`EWRITE` still does not appear. So the structural claim in this section is a
measurement, not an inference from a gap.

### Verdict

**The gate does not pass.** One of three criteria is met. Per spec §8, that
settles the deferred decision: transpiling the AST to Rust is **not** adopted,
`acad-re` remains an understanding tool, and all shipped Rust stays hand-written.
The §3 clean-room posture stands unchanged.

What would move it: resolving the kernel's function-pointer table so
overlay-to-kernel calls land on named targets (criterion 2), and improving string
and data-reference resolution so the entity writer can be found by name
(criterion 3). Both are tractable and both are about Ghidra's data-reference
analysis in segmented real mode, not about the overlay format, which is settled.
