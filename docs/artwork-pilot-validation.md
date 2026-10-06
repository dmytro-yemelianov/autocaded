# Artwork pilots: validation and remaining scope

Date: 2026-10-07. Scope: the artwork plan's first execution—strict content contract
and the original courtyard building/colour-study pair. These assets extend the
released v0.5.0 foundation; they are not a new application release.

## Accepted drawings

| Drawing | Stored entities | Layers | Expanded primitives | Renderer work units |
| --- | ---: | ---: | ---: | ---: |
| Courtyard house | 271 | 12 | 510 | 5,307 |
| Intervals | 175 | 8 | 342 | 3,579 |

Both PNG previews were inspected by the implementer and an independent Sol high
reviewer. The building's silhouette, window rhythm and court read clearly; the
colour study retains deliberate overlap and negative space. All geometry remains
ordinary editable entities. Neither drawing needs an external font or image.

The [catalog](../demo/art/catalog.json) pins original provenance and MIT asset
licensing independently. Recipes carry numbered layer roles and indexed colours.
Gallery title/description keys resolve through four shared EN/UK messages; they
do not become drawing captions or silently change drawing content.

## Checks and evidence

Generation and `--check` reproduce both AC1.40 DWGs, historical DXFs, geometry-only
SVGs/PNGs and the manifest byte for byte. The manifest retains catalog, source,
recipe and output hashes. Codec round-trips preserve the exact entity sequence;
generation rejects renderer diagnostics, incomplete frames and over-budget content.

The native app and command suites passed, as did all 10 wasm tests, eight focused
artwork/compiler/generator regressions and 23 browser regression tests. Formatting,
workspace/all-target Clippy and wasm-target Clippy passed. CI now runs the generator
regression tests and committed-artifact regeneration check.

Native Session checks open each DWG and DXF, move an existing entity, undo, preserve
pending LINE input across locale/profile changes, and save/reopen geometry. Frames
complete without diagnostics at 800×600 and 390×640. Release-mode frames measured
2.1–7.8 ms on an arm64 Apple M5 Mac. These are local smoke timings, not a benchmark
or a performance promise for other devices.

A real browser UI session opened the same files, edited/undid, saved/reopened,
switched presentation while LINE was pending, and exported PNG/SVG without UI.
Decoded browser PNG pixels matched the generated preview pixels exactly for both
drawings. Browser 800×600 PNG exports took approximately 10 ms on this Mac.
Desktop and 390×640 screenshots were inspected. The existing screen menu overlays
the right of the drawing; the narrow composition check used ordinary zoom/pan to
place the whole scene beside it. Geometry-only exports fit the complete drawing.

Independent review found four boundary defects in two focused revisions: output
path aliases, a manifest/source collision, case-only collisions on macOS, and
geometry escaping its bounds after coordinate rounding. All were corrected with
regression coverage before acceptance. The generator rejects portable path aliases
and authored-input collisions before writing; emitted geometry is checked after
historical six-decimal quantization as well as against authored bounds.

Local evidence is retained in `/tmp/autorust-art-*`: native test/release smoke logs,
browser smoke JSON/scripts, inspected screenshots, and the independent review note.
Committed artifacts and reproducible checks are the durable evidence.

## Next slice

This completes the bounded first execution. The four-drawing manual batch still
needs the museum painting and original meme; text requires a verified SHP font
extension. Image conversion, portrait/film pilots and native/browser gallery
integration remain later slices of the [approved plan](superpowers/plans/2026-10-07-modern-artwork-drawings.md).
The current app's menus, viewport policy, geometry limits and runtime asset loading
are unchanged. Open these committed files through the existing file-opening path.
