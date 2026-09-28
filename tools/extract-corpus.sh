#!/usr/bin/env bash
# tools/extract-corpus.sh — rebuilds corpus/ from the archives in autocad/
set -euo pipefail
root="$(cd "$(dirname "$0")/.." && pwd)"
out="$root/corpus"
rm -rf "$out"; mkdir -p "$out/raw"
7z x "$root/autocad/Autodesk AutoCAD 1.4 (5.25).7z" -o"$out/raw" -y >/dev/null
for img in "$out/raw"/*/*.img; do
  name="$(basename "$img" .img)"
  mkdir -p "$out/$name"
  7z x "$img" -o"$out/$name" -y >/dev/null
done
echo "corpus extracted to $out"
