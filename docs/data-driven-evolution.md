# One engine, structured facts, alternative evolution

Faithful preserves the recovered early-CAD experience. Modernized explores how
that experience could evolve today while retaining its compact, command-driven
character. It is not a roadmap to reproduce successive AutoCAD interfaces.
Both presentations use the same Rust engine and drawing formats.

The sibling NetRust project provides the architectural reference: committed,
reviewable fact catalogs with provenance; validated loading or generated typed
identities; behavior implemented in Rust. This is a separation of facts from
algorithms, not a requirement to encode every algorithm in a configuration file.

Execution is specified in the [agent loop plan](superpowers/plans/2026-10-06-data-driven-agent-loops.md),
including model assignments, dependency gates, independent review, and token discipline.

## Implemented foundation

`crates/acad-cmd/resources/commands.json` is the canonical inventory of 61
command identities and their accepted aliases. Its version-1 schema has:

- `id`: stable lowercase ASCII identity, independent of a translated label.
- `token`: canonical uppercase script spelling.
- `aliases`: additional accepted spellings, including `?` for HELP.
- `source`: versioned provenance from the pre-extraction dispatcher.

The build rejects unknown fields, unsupported schema versions, malformed names,
duplicate identities or spellings, and missing provenance. It generates
`CommandId` and `COMMANDS`; dispatch matches the generated enum exhaustively.
Adding a catalog command therefore requires a Rust behavior handler. An alias
can be added entirely in data. REDRAW and REGEN have separate identities even
though their handlers currently do the same thing.

Public Rust catalog records expose categories, availability, retained help topics,
and stable label/summary keys. The WebAssembly `command_catalog()` method returns
the same source JSON. There is no second browser catalog. These keys are metadata;
a translated discovery interface remains future work until its messages exist.
The catalog describes recognized commands, including peripheral commands that
return a device-unavailable error. It does not assert complete implementation
of every historical command option.

`resources/messages.json` is the shared, build-validated message source, with
stable typed identities, named arguments, strict template validation, and whole-entry
English fallback. The initial 22-message slice covers LINE/CIRCLE prompts, unknown
commands, and selected file-action labels; six profile labels/statuses bring the
catalog to 28 English/Ukrainian messages. Ukrainian translations, a small browser
language chooser, native `--locale`, and Session-owned locale are implemented.
Command tokens, raw API/script errors, and file bytes stay language-independent.
This is partial UI localization, with other prompts and reports retaining English.
See [coverage](ukrainian-ui-slice.md) and [the contract](message-catalog-contract.md).

The existing native/browser bitmap UI font covers Ukrainian. Scalar-aware report
wrapping preserves UTF-8 and measures character cells; unsupported characters use
a defined fallback. Drawing TEXT/font behavior remains separate. Font provenance
and punctuation limits are documented in [UI glyphs](ui-glyphs.md).

`resources/hatch-patterns.json` supplies the exact 23-pattern description table,
with versioned source locations. Build validation generates its existing consumer
records; the former Rust literal table is removed.

Menus/macros, fonts, and retained HELP already load from assets. HELP's historical
list and topic labels remain retained presentation. `resources/profiles.json` now defines the evidenced `frozen`/`modern` palette
presets and their label/status/badge policies. Native `--mode`, API, WebAssembly,
and browser use Session's shared application path. Locale and manual palette
remain separate presentation state; switching preserves the document, view,
undo, pending commands, and scripts. These profiles introduce no geometry-rule
changes. See [profile semantics](presentation-profiles.md).

The [ARC choice pilot](d9-choice-pilot-decision.md) was implemented and measured
in isolation, then rejected: removing one repeated comparison added 106 production
lines. Its inactive patch and parity evidence are retained; production transitions
remain Rust. This is a bounded decision about that family, not a general rejection
of declarative rules.

## Next migrations

The separate [modern artwork drawing plan](superpowers/plans/2026-10-07-modern-artwork-drawings.md)
uses this foundation for a data-driven gallery of editable buildings, paintings,
memes, portraits and selected film stills. It preserves the shared engine and
compact command-driven presentation; its content work does not require a new
workspace design.

1. **Messages and localization.** Expand the reviewed slice by command family,
   then cover remaining labels, errors, and reports with stable typed identities
   and named arguments. Validate placeholders, glyphs, and declared coverage.
   Preserve the recovered English output for Faithful. Command tokens, scripts,
   DXF group names, and drawing identifiers remain language-independent.
   Existing bitmap fonts also need a verified glyph path for Ukrainian and other
   languages; translating strings alone does not make them renderable.
2. **Command discovery.** Supply message entries for the existing label/summary
   keys and add evidenced argument hints. Consume the reviewed metadata for
   contextual help and suggestions in native and browser
   presentation. Keep command handlers and geometry in the shared engine.
3. **Profiles.** Encode evidenced Faithful defaults and deliberate Modernized
   choices in validated profiles: palettes, limits, bindings, menu references,
   and supported interaction policies. Profile fields must drive behavior;
   they must not merely describe it. Switch profiles without losing document,
   view, undo, or pending command state.
4. **Other static facts.** Extend the hatch extraction pattern to unit display
   metadata, bindings, and format tables where suitable. Recovered facts retain their
   source/version and evidence; newly designed choices have separate provenance.
5. **Declarative interaction rules where useful.** Move repetitive prompt,
   choice, and validation definitions into typed schemas after message extraction.
   Keep geometry, numerical algorithms, undo, I/O, and complex transitions in Rust.
   Avoid introducing a general-purpose rule interpreter without a concrete need.

Each migration replaces the previous source of those facts and routes existing
consumers through the new catalog. Check duplication, schema invariants,
native/browser consistency, historical output, and actual behavior. Automatically
extracted sources should have a regeneration/check command; this first catalog
is a one-time extraction from our dispatcher, with versioned provenance, and is
now edited directly rather than regenerated from Rust.
