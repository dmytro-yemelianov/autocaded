# Reverse-engineering pipeline

Reproduces the Ghidra analysis of AutoCAD 1.4 from the raw disk images. Nothing
it produces is committed; everything it produces is regenerable.

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

## Why the format is decoded in Rust

`acad-re` owns the `ACAD.OVL` container format and emits `ovl-map.json`. The
PyGhidra scripts only place bytes where that file says. One implementation, one
set of tests, and the layout stays covered by `cargo test`.

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
