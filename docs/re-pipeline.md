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
`UNRECOVERED_JUMPTABLE` — MET.** 148 of 151, 98.0%. Note the population is 151
functions, not the whole binary: this measures the quality of what Ghidra found,
not how much of the 179,480 bytes it reached.

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

**3. The DWG entity record is recoverable as a coherent struct — see below.**
