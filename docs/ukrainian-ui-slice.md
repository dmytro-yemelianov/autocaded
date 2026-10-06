# Ukrainian UI slice (D6)

The browser's existing header offers **English** and **Українська (частково)**.
Native launch uses `acad --locale en|uk`; English remains the default.
Both select the shared Session locale. Headless consumers can use
`set_locale` and read the active `locale` in state. Two-letter language tags
and optional two-letter regions are accepted case-insensitively; a valid
unsupported language selects English, and malformed tags leave locale intact.

The original 22 UI keys in the shared message catalog have Ukrainian templates: the idle
prompt, LINE and CIRCLE prompts (including D, 2P and 3P), both unknown-command
diagnostics, four bare file-action identities, the two browser picker labels,
and three historical Main Menu tasks. This is partial UI coverage. Other
commands, validation errors, reports/HELP, menu headings/tasks, tooltips, format
descriptions, resource diagnostics and transport errors retain English.
Bare action identities do not add new controls. D8 adds six translated profile
chooser/badge/status keys in the same catalog; see [presentation profiles](presentation-profiles.md).

Accepted commands, aliases and options remain ASCII. User arguments are inserted
literally, including braces. Raw Editor/app/wasm errors and script context remain
canonical English; the shared rendered unknown-command status uses the selected
language and re-renders when it changes. A later status replacement clears its
typed identity. Drawing TEXT and font semantics are independent of UI locale.

A switch preserves the drawing/header, view, undo history, pending command,
input buffer, scripts, palette, resource libraries and dirty state. Locale is
presentation state and is not serialized into DWG/DXF. Existing reset/open/adopt
routes inherit it. English catalog text retains exact prior wording.

Native templates use the composed Ukrainian bitmap alphabet and supported
punctuation documented in [UI glyphs](ui-glyphs.md); only DOM picker labels use
ellipsis. The longest translated Main Menu task plus its number fits the
640-pixel text screen. Automated tests cover every template's glyphs/placeholders,
tokens, typed-status re-render/clearing, locale atomicity, pending input/undo,
replacement routes and identical file bytes. D10 also checked live native/browser legibility and narrow-screen clipping.
The command area retains its existing clipping behavior; browser header controls
wrap into readable rows on narrow screens. See [validation evidence](data-driven-validation.md).
