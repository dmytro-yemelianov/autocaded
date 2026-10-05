# AutoCADED demo assets

Original files for the public browser build (autocaded.yemelianov.dev) and
the release web archive. None of them is derived from Autodesk's AutoCAD 1.4
disks.

| File | What | Source |
|---|---|---|
| `AUTOCADED.SHP` | Monospaced stroke font, printable ASCII and Cyrillic, SHP source format | `tools/demo/gen_font.py` |
| `AUTOCADED.MNU` | Three-page screen menu: draw, edit, view and settings | written by hand |
| `WELCOME.DWG`, `BRACKET.DWG`, `FLOORPLAN.DWG`, `PALETTE.DWG` | AC1.40 demo drawings | `cargo run -p acad-app --example demo_drawings` |

The page loads the font as `TXT`, the slot AutoCAD 1.4 drawings use for
text. `scripts/build-wasm.sh --assets demo` stages these files and writes the
`assets/manifest.json` the page reads; `--assets corpus` uses a local 1.4
corpus instead.

Regenerate after changing a generator:

    python3 tools/demo/gen_font.py
    cargo run -p acad-app --example demo_drawings
