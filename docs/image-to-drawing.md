# Image-to-drawing conversion v1

An offline Rust tool converts a pinned PNG into ordinary editable CAD entities.
The portrait and architectural photo are conversion pilots, alongside the four
manually authored artwork samples. Browser demo builds stage all six in Open.
The compact command interface and renderer budgets are unchanged.

## Run

```sh
cargo run -p acad-app --example image_recipe -- \
  demo/art/sources/self-portrait-conversion.json demo/art/recipes/self-portrait.json
cargo run -p acad-app --example image_recipe -- \
  demo/art/sources/self-portrait-conversion.json demo/art/recipes/self-portrait.json --check
cargo run -p acad-app --example art_drawings -- demo/art/catalog.json --check
```

`image_recipe CONFIG OUTPUT [--check]` writes the derived recipe, then prints a
conversion report. `--check` compares bytes without writing. The artwork exporter
recomputes each catalog entry with `conversion` settings before writing any output;
a changed image hash or stale derived recipe rejects the entire batch.
The manifest records configuration SHA-256, input SHA-256, dimensions, palette,
region/contour counts and renderer work. Existing manual recipe bytes are retained.

## Settings

The strict JSON object rejects unknown fields:

| Field | Meaning |
| --- | --- |
| `schema_version`, `id` | Version 1 and the existing catalog/recipe identity |
| `image`, `image_sha256` | Ordinary relative path from the configuration directory and exact lowercase input SHA-256 |
| `columns` | 4–128 requested sample columns; never upsamples a smaller input |
| `colors` | 1–15 requested region colours; effective palette may be smaller |
| `contours` | Include merged axis-aligned region boundary LINEs on a separate layer |
| `contour_color` | ACI index 1–254 for those lines |
| `contrast_threshold` | RGB Euclidean distance 0–442 required between adjacent quantized colours; the outer frame is always included when contours are enabled |
| `matte` | Three RGB bytes used to composite transparency before sampling |

Rows follow the source aspect ratio, rounded to the nearest grid row, with a
128-row limit. Images are sampled by integer area averages in byte RGB, without
an implicit gamma transformation. Fixed integer clustering (eight iterations,
stable seeds/ties) chooses colour centers, mapped to the shared ACI palette.
Duplicate ACI choices are collapsed. Colour 255 is unavailable in the historical
layer table; index 7 supplies white and 250 supplies black.

Equal-colour cells merge into disjoint filled rectangles. Their union covers the
whole grid exactly; image top maps to CAD top. Collinear contour segments merge
into LINEs. All parts remain ordinary editable SOLIDs/LINEs on numbered layers.
Recipe layer roles identify colour groups and contours, not semantic objects.
This deliberately blocky treatment does not reconstruct windows, faces, circles,
perspective, diagonal edges or photorealistic textures.

## Bounds and provenance

Inputs are still PNGs (RGB, grayscale, indexed and alpha forms expanded to 8-bit).
JPEG decoding, animation, cropping and interactive import are outside v1. Inputs
are limited to 16 MiB, 4096 pixels per axis and 16 megapixels, with bounded decoder
allocation. Grid resolution and output complexity are explicit; conversion rejects
more than 5,000 entities or an incomplete renderer frame rather than truncating.
Coordinates use the historical six-decimal precision, including saved bounds.
DWG/DXF/SVG/PNG generation uses the existing codecs and shared renderer.

The portrait source is Van Gogh's 1887 Self-Portrait, Art Institute of Chicago
1954.326. The retained museum API confirms public-domain status; the museum
reproduction's Commons page identifies CC0. The architectural source is Detroit
Publishing Co.'s circa-1930 Chrysler Building photograph, Library of Congress
LC-DIG-det-4a25712. Its collection rights statement is **no known copyright
restrictions**, recorded as such; no CC0 dedication is claimed.

Each source record retains the image URL, rights/credit, original JPEG hash and
normalized PNG hash. Normalization used `sips -Z 1024 -s format png`; the resulting
PNG bytes are committed, so regeneration needs neither that utility nor a network.
Source/rights records remain independent of the software license. For a new input,
retain its source record and rights evidence, pin its PNG hash, add settings and
EN/UK gallery metadata, generate a candidate, then inspect and validate it before
adding it to Open.
