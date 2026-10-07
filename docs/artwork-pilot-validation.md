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

## Painting and meme extension

The manual batch now also includes The Bedroom (after Van Gogh) and the original
UNDO meme. The painting is a deliberately simplified geometric treatment of the
1889 Chicago version. Its public-domain museum record, CC0 reference image,
credit and source hashes are retained. The meme has original MIT provenance.

The meme uses editable TEXT entities backed by the canonical AUTOCADED SHP font,
with its hash pinned in the recipe. Generation emits a TXT.SHP sidecar for native
opening; browser staging verifies the same font. Unsupported glyphs and text
outside recipe bounds are rejected. Captions remain authored English drawing
content; EN/UK gallery titles come from the message catalog.

An independent reviewer accepted both previews and the focused compiler/font
checks, and confirmed that the original pair's drawing artifacts stayed byte
identical. Native checks cover all four drawings in both codecs, editing/undo,
pending commands across presentation changes and complete desktop/narrow frames.

Browser acceptance opens both new samples through Open, edits/saves/reopens the
meme caption and undoes it, preserves pending input across locale/profile changes,
and checks geometry-only export. Decoded PNG pixels match native previews exactly
for both new drawings. English/Ukrainian Open labels resolve from the catalog.
Workspace/all-target and wasm Clippy, formatting, wasm tests and staging checks pass.

## Image conversion extension

The offline converter and two pilots are implemented: Self-Portrait has 2,634
editable colour regions; Chrysler Building has 2,599 regions and 1,471 contour
lines (4,070 entities). Both use pinned PNG/settings and reproducible recipes.
Native checks open both codecs, edit/undo and preserve pending commands across
presentation changes, with complete desktop and narrow frames. Tests cover exact
cell coverage, orientation, transparency, codec parity, fractional bounds, resource
limits, pinned-input failures and read-only regeneration. See
[image conversion](image-to-drawing.md) for source records and settings.

Browser Open checks pass for both new samples, including editing/undo, save/reopen,
pending input across locale/profile changes and EN/UK titles. Decoded PNG pixels
match native previews exactly. On the same arm64 Apple M5, native release frames
measured 11.6–15.4 ms and browser 800×600 PNG exports 27.2–29.9 ms. These are local
smoke observations, not portable performance guarantees. All 16 previous drawing
and preview files remain byte identical.

## Next slice

Conversion refinement and additional portrait/film pilots can build on the
deterministic region and contour pipeline. Native gallery integration and broader
help/i18n remain later work in the [approved plan](superpowers/plans/2026-10-07-modern-artwork-drawings.md).
