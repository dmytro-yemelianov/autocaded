# Editable artwork pilots

Four manual scenes establish the artwork recipe and export pipeline.
They use the existing Rust drawing model, historical file formats and ACI palette;
the app's geometry rules and command interface are unchanged.

| Drawing | Composition | Editable parts |
| --- | --- | --- |
| [Courtyard house](generated/courtyard-house.png) | Contemporary plaster and terracotta wings, glass windows, a planted court and a flat roof | Individual facade panels, windows, mullions, paving and plant silhouettes |
| [Intervals](generated/colour-study.png) | A geometric colour study with overlapping circular and rectangular regions | Colour planes, individual SOLID sectors, line rhythms and cutouts |

| [The Bedroom — after Van Gogh](generated/bedroom.png) | Simplified public-domain painting with room perspective and furniture | Bed, chairs, window, floor and colour regions |
| [UNDO fixes everything](generated/undo-meme.png) | Original two-panel CAD joke | House geometry and editable SHP-font captions |

The authored source is `catalog.json`, `recipes/*.json` and the pinned original
source records in `sources/`. Titles and descriptions resolve through the shared
English/Ukrainian message catalog. Layer roles stay in recipe metadata; drawing
files retain historical numbered layers and indexed colours.

Generate from the repository root:

```sh
cargo run -p acad-app --example art_drawings -- demo/art/catalog.json
cargo run -p acad-app --example art_drawings -- demo/art/catalog.json --check
```

`generated/` contains AC1.40 DWG, historical DXF, geometry-only SVG and PNG previews,
plus a manifest with provenance, hashes and measured drawing/render counts.
The check command regenerates in memory and compares the committed files.

Demo browser builds include all four drawings in the Open menu;
the entries are staged from the catalog, with shared English/Ukrainian titles.
Open a DWG or DXF in either app. Use Modernized / ACI 256 to see the intended
colours; Faithful / PC 16 deliberately maps the same indices to its historical
palette. Locale changes affect gallery metadata, not the drawing geometry.
The meme uses the canonical AUTOCADED SHP font. Keep generated `TXT.SHP` beside
its DWG/DXF when opening it in the native app. Browser staging verifies the same
font hash. The other drawings need no external font or shape library.

The original artwork is distributed under the repository's [MIT license](../../LICENSE).
Source records state asset licenses independently. The Bedroom reference is a
CC0 museum reproduction of public-domain artwork, credited to the Art Institute
of Chicago; its source image and museum metadata are retained with hashes.
The manually authored CAD adaptation is MIT licensed.

See the [content contract](../../docs/artwork-content-contract.md) and
[artwork plan](../../docs/superpowers/plans/2026-10-07-modern-artwork-drawings.md)
for validation, budgets and later subjects.
