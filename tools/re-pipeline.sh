#!/usr/bin/env bash
# tools/re-pipeline.sh — raw disk images to a typed AST, reproducibly.
set -euo pipefail
root="$(cd "$(dirname "$0")/.." && pwd)"
cd "$root"

[ -f corpus/System/ACAD.OVL ] || { echo "corpus missing; run ./tools/extract-corpus.sh" >&2; exit 1; }

mkdir -p build
# shellcheck source=/dev/null
source tools/ghidra-env.sh

cargo run --quiet -p acad-re --bin ovl-map -- \
  corpus/System/ACAD.EXE corpus/System/ACAD.OVL build/ovl-map.json

# export_ast starts from a fresh Ghidra project, so no stale overlay blocks or
# analysis can carry over from a previous run.
"$PYGHIDRA_PYTHON" tools/ghidra/export_ast.py build/ovl-map.json build/ghidra build

cargo run --quiet -p acad-re --bin re-report -- \
  build/ast-pcode.json build/ast-clang.json build/re-report.json
