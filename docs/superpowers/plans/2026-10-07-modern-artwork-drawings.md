# Modern artwork drawings

Date: 2026-10-07. Status: planned. Baseline: released [v0.5.0](https://github.com/dmytro-yemelianov/autocaded/releases/tag/v0.5.0).

Create a small gallery of contemporary CAD artwork: open-licensed paintings,
memes, film/video stills, modern buildings, and portraits where the subject works
as editable geometry. Keep the compact command-driven app and shared Rust engine.
The first result should be a convincing set of drawings, with a reproducible
content pipeline that can grow without embedding scene facts in Rust functions.

Interpret CC as Creative Commons, alongside public-domain/open-access material.
Use deliberate contours, colour regions, repeated forms and hatching. A subject
passes when its composition remains recognizable at the app's ordinary scale;
detailed texture is not a reason to generate thousands of nearly invisible marks.

## Subjects and first examples

| Family | Proposed treatment | First example | Acceptance |
| --- | --- | --- | --- |
| Modern buildings | Facade/elevation or drawn perspective; repeated windows, structural lines, selective fills | Original contemporary courtyard building, followed by a real building from a suitable licensed reference | Clear silhouette and rhythm; editable window/structure groups |
| Graphic painting | Few strong regions, contours and restrained hatching | Original geometric colour study; Van Gogh's *The Bedroom* as a simplified museum-source adaptation | Recognizable layout and colour relationships; no dependence on painted texture |
| Memes | Original caption panels and simplified figures; vector reinterpretation of suitable licensed templates | Original “UNDO fixes everything” two-panel CAD meme | Readable caption and immediate visual joke at 800×600 |
| Portraits | Contour drawing, silhouette, or roughly 4–8 tonal regions | One public-domain portrait chosen after a small contact sheet | Face/profile remains recognizable; eyes and major proportions survive simplification |
| Film and video stills | One selected frame, locked timestamp and crop; silhouettes and a few depth/colour planes | A clear *Big Buck Bunny* frame, avoiding the excluded cover/logo material | Recognizable subject/composition without furry texture or motion blur |

The first batch has four drawings: the original building, colour study, bedroom
adaptation and original meme. Portrait and film pilots follow only after these
four establish the geometry/export pipeline. The source portrait and exact film
frame are selected during their pilots rather than fixed without visual evaluation.

## Existing engine and output constraints

The current model supports LINE, ARC, CIRCLE, POINT, TEXT, SOLID/TRACE, INSERT and
REPEAT; use those existing types and codecs. The demo generator already drives
`Session` to produce AC1.40 drawings in
[`demo_drawings.rs`](../../../crates/acad-app/examples/demo_drawings.rs).
Numbered layers are available; semantic layer names belong in authored metadata.
ACI 256 is the Modernized palette, so recipes select indexed colours rather than
introducing RGB values into historical drawing files. Begin with 8–16 colours.

Modernized currently changes presentation presets, not geometry/resource limits.
Existing rendering uses bounded work policies, including the defaults in
[`policy.rs`](../../../crates/acad-model/src/policy.rs). An initial content envelope
is at most 5,000 stored records and 25,000 expanded primitives per drawing; these
are proposed content limits, not claims about historical AutoCAD. Actual render
work, nesting, hatch cost and diagnostic checks still decide whether a drawing fits.
Simplify content that exceeds a budget instead of raising engine limits for a demo.

Deliver an editable AC1.40 DWG, historical DXF, geometry-only SVG/PNG previews,
and source/recipe metadata. Validate text through the drawing SHP font path,
which is separate from the bitmap UI font. UI language changes must not silently
translate or edit drawing captions; optional English/Ukrainian caption variants
are explicit content variants.

## One authored source and reproducible generation

Use a versioned JSON gallery catalog and one JSON recipe per drawing, consistent
with the new fact catalogs. Choose final locations during the contract slice;
`demo/art/catalog.json` and `demo/art/recipes/` are the proposed destinations.

The catalog records stable IDs, category, title/description message keys, source
page and original creator, license URL/version, attribution, image/frame hash,
crop/timestamp where applicable, recipe path and generated output paths. Retain
the source metadata snapshot and describe the adaptation. Keep asset licenses
separate from the repository's software license. Captions are content, while
gallery labels use the shared message catalog for native/browser presentation.

Recipes contain finite geometry, numbered layers with semantic roles, indexed
palette choices, view/bounds, and generator parameters with a fixed seed when
needed. A typed compiler maps supported recipe primitives into the existing Rust
model or existing Session commands. Geometry, tracing, simplification and
triangulation remain algorithms in code. Do not turn recipes into an arbitrary
expression or transition interpreter.

Begin with hand-authored geometry and small helper generators. Once it works,
add an offline image-preparation path: choose crop, reduce colours, extract
contours or regions, simplify boundaries, and decompose fills into supported
SOLID geometry. Hatching or coarse mosaics are optional treatments, not universal
fallbacks. Make paths easier to edit rather than merely minimizing pixel error.

Regeneration emits drawing files, previews and a gallery manifest from the same
catalog. Record generator version, source hash, recipe hash, entity/layer counts,
expanded work and output hashes. Two runs with identical inputs must produce the
same drawing bytes. No remote API or model is required at application runtime.

## Source candidates

The Art Institute of Chicago API provides a public-domain filter and image IDs;
its image guidance supplies the IIIF URL pattern. The *Bedroom* record, ID 28560,
currently reports `is_public_domain: true`; preserve a snapshot and confirm the
image's licensing page when acquiring the pilot asset.
[API documentation](https://api.artic.edu/docs/) and
[artwork record](https://api.artic.edu/api/v1/artworks/28560?fields=id,title,artist_display,image_id,is_public_domain,copyright_notice).

The Met Collection API exposes `isPublicDomain` and image URLs for another
painting/portrait candidate pool. Select individual eligible objects rather than
treating all museum images as open material.
[Met Collection API](https://metmuseum.github.io/).

The *Big Buck Bunny* project describes CC BY 3.0 licensing and the credit for
reused portions of the film; cover/disc artwork and logos have exclusions.
Store the selected frame's provenance and the applicable credit in the asset
record. This supplies an initial open-film pilot, with other films considered
individually. [Project licensing](https://peach.blender.org/about/).

Original building, geometric and meme illustrations can be authored directly
for the project. Existing meme photographs/templates and modern-building photos
enter the catalog only with an identified source and usable asset license.

## Implementation sequence and agent loops

| Slice | Dependency | Work | Worker and independent reviewer |
| --- | --- | --- | --- |
| Content contract and baseline | Released v0.5.0 | Schema, supported primitive mapping, source records, palette/layer/font limits, one tiny round-trip fixture | Sol medium; Sol medium |
| Four manual pilots | Contract | Building, colour study, bedroom and meme recipes; previews and editable files | Sol medium; Sol high for visual and CAD behavior review |
| Reproducible gallery pipeline | Accepted pilots | Typed recipe compiler, deterministic regeneration/check, source/output hashes, generated manifest and shared labels | Sol high; Sol high |
| Image conversion pilot | Pipeline | One small contour/region conversion of the bedroom; compare to the manual adaptation | Sol high; Sol high |
| Portrait and film pilots | Conversion evidence | A small candidate contact sheet, one accepted portrait and one accepted frame | Sol high; Sol high |
| Native and browser gallery | Accepted assets | Existing Open selector/assets plus a modest sample-opening path for native; attribution and content metadata | Sol medium; Sol medium |

Use `gpt-6.1-sol` for the Sol assignments above. Route mechanical metadata
collection, checksum work and already-defined batch transformations to
`gpt-6-luna` low; keep visual selection and algorithms with Sol. Escalate to
`gpt-6-astra` high only for a concrete unresolved architectural problem.

Use one implementer followed by one independent reviewer. Parallelize at most two
content workers on separate recipes after the schema is stable; serialize shared
compiler, catalog, message and UI changes. Give workers narrow briefs, exact source
hashes and allowed files. Reviewers get the source thumbnail, generated preview,
recipe diff and check summary, not the full conversation or successful logs.

Each pilot gets one initial candidate and at most two focused revisions. A failed
pilot becomes simplify, change treatment, replace the source, or defer, with a
short reason. Do not keep retrying a subject that depends on unsupported effects.
Reuse deterministic scripts for metadata, counts, checksums and regeneration.
Record attempts and actual token usage if exposed; do not estimate token savings.

## Acceptance and first execution

Every accepted drawing must have a complete source record, strict recipe
validation, finite and bounded geometry, and a visually readable composition.
Native and browser open the same DWG/DXF; saving and reopening preserve geometry.
Edit an ordinary entity and undo it, change locale/profile while a command is
pending, and confirm the drawing remains usable. PNG/SVG export excludes menus,
command areas, selection highlights and other UI. Check palette and font parity,
render completeness and file-size/work limits at 800×600 and a narrow viewport.

Keep a source/preview comparison and a concise accepted/deferred record for each
pilot. Use measured native/browser timings to set a practical content budget;
provisional targets are under 100 ms per ordinary native frame and 250 ms in the
browser on recorded hardware. These are targets to calibrate, not current results.
A prettier preview does not compensate for an uneditable or incomplete drawing.

Start with the content contract and the original building/colour-study pair.
Approve the visual language through those two small scenes before spending time
on conversion tooling or assembling a large reference collection. Wider subject
coverage follows successful pilots; photorealism, video playback and a new CAD
workspace are outside this drawing-gallery plan.
