# Shared presentation profiles (D8)

`crates/acad-cmd/resources/profiles.json` is the canonical, build-validated
catalog. Stable IDs remain `frozen` and `modern`, preserving browser storage
values. Generated `ProfileId` values select `Session::apply_profile`; native,
API and wasm/browser adapters use that same path.

The evidenced behavior is presentation: `frozen` selects the existing PC 16
palette, and `modern` selects ACI 256. The catalog preserves the existing
green/red badge-dot tones through the browser consumer. Exact English chooser, badge and mode
message wording is recovered from the prior browser controls, including
“Modernized (concept)”. Those six entries and their Ukrainian translations
live in the shared message catalog. “Concept” does not promise additional
geometry policies. Neither profile changes geometry limits, input bindings,
menus, fonts, drawing headers or file formats.

Native launch accepts `acad --mode frozen|modern`; default launch remains
Faithful with PC 16 and English. An explicit `--palette pc16|aci256` overrides
the chosen preset, independent of argument order. The typed app API accepts
`{"method":"set_mode","params":{"id":"modern"}}`. API/wasm state exposes
`mode` and the separate current `palette`. Unknown/case-mismatched profile IDs
return an error and preserve all session state, including status.

The existing browser mode chooser reads resolved titles, badges and status
messages from `AutoCadSession::presentation_profiles()`. Locale switching
re-renders these labels, including Faithful Ukrainian, independently of the
selected profile or current palette. Focused locale, mode and palette choosers
retain native select keyboard navigation without forwarding keys into CAD input.

Applying a preset changes only its ID and current rendering palette. Selecting
a palette manually keeps the selected profile ID; the palette chooser shows
the actual current palette. Browser startup restores `autocaded.mode` first,
then `autocaded.palette` as a separate manual override. Explicitly selecting a
profile removes the prior saved manual override. New, open, dropped-open and
Main Menu adopt routes preserve profile, locale and any manual palette override.
Wasm keeps the SVG export font library on the same palette as Session rendering.

The resource rejects duplicate object fields/IDs, unknown fields/IDs/palettes/badge tones,
missing provenance, unsupported schema versions, invalid counts/budget excess,
and unresolved/parameterized or untranslated message references. Data adds no
runtime JSON policy interpreter. Tests cover schema fixtures, exact English,
Ukrainian resolution, atomic rejection, pending input/undo/view/document bytes,
scripts/macros/resources, replacement routes and actual browser handlers.
Native/browser visual legibility and narrow-screen layout remain the D10 gate.
