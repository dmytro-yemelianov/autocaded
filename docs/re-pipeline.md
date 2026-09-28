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
