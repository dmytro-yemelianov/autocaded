#!/usr/bin/env bash
# Build the browser bundle into web/: pkg/ (wasm + JS bindings) and assets/
# (font, screen menu, sample drawings and the manifest.json the page reads).
#
#   scripts/build-wasm.sh                  # corpus/ assets if present, else demo/
#   scripts/build-wasm.sh --assets demo    # AutoCADED's own font, menu, drawings
#   scripts/build-wasm.sh --assets corpus  # a local AutoCAD 1.4 corpus
#   scripts/build-wasm.sh --assets none    # pkg/ only
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "${SCRIPT_DIR}/.." && pwd)"

ASSETS=auto
while [ $# -gt 0 ]; do
  case "$1" in
    --assets) ASSETS="${2:-}"; shift 2 ;;
    --no-assets) ASSETS=none; shift ;;
    *) echo "usage: $0 [--assets demo|corpus|none]" >&2; exit 2 ;;
  esac
done
if [ "${ASSETS}" = auto ]; then
  if [ -f "${REPO_ROOT}/corpus/System/TXT.SHP" ]; then ASSETS=corpus; else ASSETS=demo; fi
fi
case "${ASSETS}" in demo|corpus|none) ;; *) echo "unknown --assets ${ASSETS}" >&2; exit 2 ;; esac

# The CLI must match the wasm-bindgen crate in Cargo.lock exactly.
BINDGEN_VERSION="$(awk '/^name = "wasm-bindgen"$/ { getline; gsub(/[^0-9.]/, ""); print; exit }' \
  "${REPO_ROOT}/Cargo.lock")"

echo "==> Building acad-wasm (wasm32-unknown-unknown, release)..."
rustup target add wasm32-unknown-unknown 2>/dev/null || true

cargo build --manifest-path "${REPO_ROOT}/crates/acad-wasm/Cargo.toml" \
  --target wasm32-unknown-unknown \
  --release

if ! wasm-bindgen --version 2>/dev/null | grep -qF "${BINDGEN_VERSION}"; then
  echo "==> Installing wasm-bindgen-cli ${BINDGEN_VERSION} (matches Cargo.lock)..."
  cargo install wasm-bindgen-cli --version "${BINDGEN_VERSION}" --locked
fi

echo "==> Generating JavaScript bindings into web/pkg/..."
mkdir -p "${REPO_ROOT}/web/pkg"
wasm-bindgen "${REPO_ROOT}/target/wasm32-unknown-unknown/release/acad_wasm.wasm" \
  --out-dir "${REPO_ROOT}/web/pkg" \
  --target web \
  --no-typescript

# stage FILE... : copy into web/assets/, failing on a missing source.
stage() {
  for src in "$@"; do
    if [ ! -f "${src}" ]; then
      echo "error: ${src} missing" >&2
      exit 1
    fi
    cp "${src}" "${REPO_ROOT}/web/assets/"
  done
}

rm -rf "${REPO_ROOT}/web/assets"
if [ "${ASSETS}" != none ]; then
  echo "==> Staging ${ASSETS} assets into web/assets/..."
  mkdir -p "${REPO_ROOT}/web/assets"
fi
case "${ASSETS}" in
  demo)
    # AutoCADED's own font, menu and drawings (demo/README.md).
    D="${REPO_ROOT}/demo"
    stage "$D/AUTOCADED.SHP" "$D/AUTOCADED.MNU" \
      "$D/WELCOME.DWG" "$D/BRACKET.DWG" "$D/FLOORPLAN.DWG" "$D/PALETTE.DWG"
    cat > "${REPO_ROOT}/web/assets/manifest.json" <<'JSON'
{
  "font": "AUTOCADED.SHP",
  "menu": "AUTOCADED.MNU",
  "start": "WELCOME.DWG",
  "samples": [
    { "file": "WELCOME.DWG", "label": "Welcome" },
    { "file": "BRACKET.DWG", "label": "Bracket" },
    { "file": "FLOORPLAN.DWG", "label": "Floor plan" },
    { "file": "PALETTE.DWG", "label": "Palette" }
  ]
}
JSON
    ;;
  corpus)
    # The 1983 corpus stays out of git; the page fetches these at runtime.
    C="${REPO_ROOT}/corpus"
    stage "$C/System/TXT.SHP" "$C/System/ACAD.MNU" "$C/Samples/SUBDIV.DXF" \
      "$C/Samples/HOUSE.DWG" "$C/Samples/OFFICE.DWG" "$C/Samples/COLORS.DWG"
    cat > "${REPO_ROOT}/web/assets/manifest.json" <<'JSON'
{
  "font": "TXT.SHP",
  "menu": "ACAD.MNU",
  "start": "SUBDIV.DXF",
  "samples": [
    { "file": "SUBDIV.DXF", "label": "SUBDIV (1983)" },
    { "file": "HOUSE.DWG", "label": "HOUSE (1983)" },
    { "file": "OFFICE.DWG", "label": "OFFICE (1983)" },
    { "file": "COLORS.DWG", "label": "COLORS (1983)" }
  ]
}
JSON
    ;;
esac

echo "==> WebAssembly build complete: web/pkg/acad_wasm.js and web/pkg/acad_wasm_bg.wasm; serve web/ over HTTP"
