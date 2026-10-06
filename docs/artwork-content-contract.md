# Artwork content contract v1

The offline generator compiles strict typed JSON into the existing CAD model and
codecs. Run `cargo run -p acad-app --example art_drawings -- demo/art/catalog.json`.
Append `--check` to regenerate in memory and compare all existing output bytes and
`generated/manifest.json`. No network or image conversion is involved.

All objects reject unknown fields. The only supported schema version is `1`.
IDs contain 1–64 lowercase ASCII letters, digits or hyphens. Paths are relative to
the catalog directory and contain only ordinary components (no traversal or
absolute paths or backslashes). Paths use forward slashes; output and authored
input collision checks ignore case for portable generation. Output paths must be
unique and use their declared suffix.

## Catalog

`Catalog` is `{schema_version,entries}`. Each entry has `id`, `category`
(`building` or `colour_study`), `title_key`, `description_key`, `recipe`, `source`
and `outputs`. Label keys resolve through the shared typed EN/UK message catalog
and must require no arguments. Labels never become drawing captions.

`source` has `kind` (currently only `original`), `creator`, `license_url`,
`license_version`, `attribution`, `adaptation`, `snapshot` and `sha256`.
The snapshot is a retained local provenance file; its exact bytes must match the
lowercase SHA-256 string. Source license metadata is independent of the software
license. Snapshot payloads remain authored metadata, not executable recipes.
`outputs` has `dwg`, `dxf`, `svg` and `png` relative paths.

## Recipe

`Recipe` has `schema_version`, `id`, `bounds` (`[xmin,ymin,xmax,ymax]`), `layers`
and `primitives`. Bounds have positive dimensions and finite coordinates within
±1,000,000 drawing units. Every primitive lies within those bounds, including the
full circle radius. Bounds determine the saved view and preview framing.

There are 1–16 semantic layers. A layer has `number` (1–127), `color` (1–254 ACI)
and a nonempty `role`. Numbers must be unique. The AC1.40 layer table does not
support color 255; use color 7 for white. Previews use Aci256; palette choice is
presentation state and is not encoded as RGB in the historical drawing.

Each primitive has a `type` tag and `layer`:

| Type | Additional fields | Compiled geometry |
| --- | --- | --- |
| `line` | `start`, `end` coordinate pairs | One nonzero LINE |
| `circle` | `center`, positive `radius` | One CIRCLE |
| `solid` | `points`: four coordinate pairs in CAD order | One convex SOLID |
| `rectangle` | `min`, `max`, `filled` boolean | SOLID or four LINEs |
| `grid` | `origin`, `columns`, `rows`, `spacing`, `size`, `filled` | Finite rectangular grid expanded to ordinary entities |

SOLID points are file order p1,p2,p3,p4; perimeter order is p1,p2,p4,p3.
Triangles use repeated p3=p4. Zero-area, concave and crossing fills are rejected.
Coordinates are quantized to the historical DXF writer’s six decimal places;
positive dimensions are at least 0.000001 units. This also stabilizes floating-point
helper sums across the two file codecs. Header bounds are also quantized; every
emitted entity must stay inside both the authored and quantized bounds. Grid counts are positive integers; spacing and size are positive finite values,
except unused spacing may be zero on an axis with count one. Helper expansion
preserves authored document order and produces individually editable entities.

There are no expressions, commands, blocks, arbitrary generators, seed-dependent
algorithms, text or font loading in v1. Text and additional source families need
explicit contract extensions and their own font/provenance validation.

## Outputs and validation

Generation validates all entries and renders all artifacts before writing. DWG
is AC1.40 and DXF is the existing historical comma format. Both codecs must
round-trip the exact entity sequence. Each drawing permits at most 5,000 ordinary
stored records and 25,000 shared renderer primitives. The existing whole-frame
work budget remains in force; incomplete rendering or any renderer diagnostic
rejects generation.

SVG serializes the shared renderer primitives. PNG uses the existing rasterizer.
Both are 800×600, black background, drawing geometry only, fitted to the authored extents. The saved editor view has an 8% margin.
No menus, selection highlights, cursor or command text are included.

The generated manifest records schema/generator versions, source and recipe
hashes, the exact catalog hash, resolved EN/UK label text and keys, entity/layer counts, expanded primitive
count, actual renderer work units, output paths and output SHA-256 hashes.
Identical inputs produce identical drawing/preview/manifest bytes. `--check`
compares all bytes without writing. Timing/browser/narrow-view acceptance and
visual judgment are separate pilot checks, not claims made by this compiler.
