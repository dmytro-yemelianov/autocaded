# Editable artwork pilots

The first two original scenes establish the artwork recipe and export pipeline.
They use the existing Rust drawing model, historical file formats and ACI palette;
the app's geometry rules and command interface are unchanged.

| Drawing | Composition | Editable parts |
| --- | --- | --- |
| [Courtyard house](generated/courtyard-house.png) | Contemporary plaster and terracotta wings, glass windows, a planted court and a flat roof | Individual facade panels, windows, mullions, paving and plant silhouettes |
| [Intervals](generated/colour-study.png) | A geometric colour study with overlapping circular and rectangular regions | Colour planes, individual SOLID sectors, line rhythms and cutouts |

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

Open a DWG or DXF in either app. Use Modernized / ACI 256 to see the intended
colours; Faithful / PC 16 deliberately maps the same indices to its historical
palette. Locale changes affect gallery metadata, not the drawing geometry.
These initial drawings contain no text and need no external font or shape library.

The original artwork is distributed under the repository's [MIT license](../../LICENSE).
Its source records state that license independently; future museum and film assets
must carry their own applicable license and attribution.

See the [content contract](../../docs/artwork-content-contract.md) and
[artwork plan](../../docs/superpowers/plans/2026-10-07-modern-artwork-drawings.md)
for validation, budgets and later subjects.
