#!/usr/bin/env bash
# Build the browser bundle into web/: pkg/ (wasm + JS bindings) and assets/
# (font, menu and samples copied from the local corpus).
#
#   scripts/build-wasm.sh              # full local bundle; needs corpus/
#   scripts/build-wasm.sh --no-assets  # pkg/ only, e.g. for a release archive
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "${SCRIPT_DIR}/.." && pwd)"

STAGE_ASSETS=1
for arg in "$@"; do
  case "${arg}" in
    --no-assets) STAGE_ASSETS=0 ;;
    *) echo "usage: $0 [--no-assets]" >&2; exit 2 ;;
  esac
done

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

if [ "${STAGE_ASSETS}" -eq 1 ]; then
  # The 1983 corpus stays out of git; the page fetches these at runtime.
  echo "==> Staging corpus assets into web/assets/..."
  mkdir -p "${REPO_ROOT}/web/assets"
  for f in System/TXT.SHP System/ACAD.MNU \
    Samples/SUBDIV.DXF Samples/HOUSE.DWG Samples/OFFICE.DWG Samples/COLORS.DWG; do
    src="${REPO_ROOT}/corpus/${f}"
    if [ ! -f "${src}" ]; then
      echo "error: ${src} missing; extract the corpus first (or pass --no-assets)" >&2
      exit 1
    fi
    cp "${src}" "${REPO_ROOT}/web/assets/"
  done
fi

echo "==> WebAssembly build complete: web/pkg/acad_wasm.js and web/pkg/acad_wasm_bg.wasm; serve web/ over HTTP"
