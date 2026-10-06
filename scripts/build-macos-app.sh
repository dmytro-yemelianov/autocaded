#!/usr/bin/env bash
# Build a universal Finder app (Apple Silicon + Intel) and a shareable ZIP.
# Requires macOS, Rust and Xcode Command Line Tools.
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
if [ "$(uname -s)" != Darwin ]; then
  echo "This bundle must be built on macOS." >&2
  exit 2
fi

OUTPUT="${REPO_ROOT}/target/macos"
APP="${OUTPUT}/AutoCADED.app"
ICONSET="${OUTPUT}/AutoCADED.iconset"
VERSION="$(awk '/^version = / { gsub(/[\" ]/, "", $3); print $3; exit }' "${REPO_ROOT}/crates/acad-app/Cargo.toml")"
VERSION="${AUTOCADED_BUNDLE_VERSION:-${VERSION}}"
if [[ ! "${VERSION}" =~ ^[0-9]+\.[0-9]+\.[0-9]+$ ]]; then
  echo "AUTOCADED_BUNDLE_VERSION must be MAJOR.MINOR.PATCH." >&2
  exit 2
fi

# Both slices support macOS 11+. Keep generated artifacts in this repo's target/.
export MACOSX_DEPLOYMENT_TARGET=11.0
for target in aarch64-apple-darwin x86_64-apple-darwin; do
  rustup target add "${target}"
  CARGO_TARGET_DIR="${REPO_ROOT}/target" cargo build \
    --manifest-path "${REPO_ROOT}/Cargo.toml" \
    --release -p acad-app --bin acad --target "${target}"
done

mkdir -p "${APP}/Contents/MacOS" "${APP}/Contents/Resources/System" \
  "${APP}/Contents/Resources/demo" "${ICONSET}"
lipo -create \
  "${REPO_ROOT}/target/aarch64-apple-darwin/release/acad" \
  "${REPO_ROOT}/target/x86_64-apple-darwin/release/acad" \
  -output "${APP}/Contents/MacOS/acad"
chmod +x "${APP}/Contents/MacOS/acad"
cp "${REPO_ROOT}/demo/AUTOCADED.SHP" "${APP}/Contents/Resources/System/TXT.SHP"
cp "${REPO_ROOT}/demo/AUTOCADED.MNU" "${APP}/Contents/Resources/System/ACAD.MNU"
cp "${REPO_ROOT}"/demo/*.DWG "${APP}/Contents/Resources/demo/"
cp "${REPO_ROOT}/LICENSE" "${APP}/Contents/Resources/"

for size in 16 32 128 256 512; do
  sips -z "${size}" "${size}" "${REPO_ROOT}/web/icon-512.png" \
    --out "${ICONSET}/icon_${size}x${size}.png" >/dev/null
  double=$((size * 2))
  sips -z "${double}" "${double}" "${REPO_ROOT}/web/icon-512.png" \
    --out "${ICONSET}/icon_${size}x${size}@2x.png" >/dev/null
done
iconutil -c icns "${ICONSET}" -o "${APP}/Contents/Resources/AutoCADED.icns"

python3 - "${APP}/Contents/Info.plist" "${VERSION}" <<'PY'
import plistlib
import sys
from pathlib import Path

info = {
    'CFBundleName': 'AutoCADED',
    'CFBundleDisplayName': 'AutoCADED',
    'CFBundleIdentifier': 'dev.yemelianov.autocaded',
    'CFBundleExecutable': 'acad',
    'CFBundlePackageType': 'APPL',
    'CFBundleInfoDictionaryVersion': '6.0',
    'CFBundleShortVersionString': sys.argv[2],
    'CFBundleVersion': sys.argv[2],
    'CFBundleIconFile': 'AutoCADED.icns',
    'LSMinimumSystemVersion': '11.0',
    'NSHighResolutionCapable': True,
}
Path(sys.argv[1]).write_bytes(plistlib.dumps(info))
PY

# Ad-hoc signature for local use; public distribution needs Developer ID/notarization.
codesign --force --sign - "${APP}"
codesign --verify --deep --strict "${APP}"
plutil -lint "${APP}/Contents/Info.plist"
ditto -c -k --sequesterRsrc --keepParent "${APP}" "${OUTPUT}/AutoCADED-macOS.zip"
echo "App: ${APP}"
echo "ZIP: ${OUTPUT}/AutoCADED-macOS.zip"
