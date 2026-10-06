# Shared UI bitmap glyphs (D5)

The software UI font is the fixed 8×8 bitmap table and `get_glyph` definitions
in `crates/acad-app/src/bitmap.rs`. Each Unicode scalar occupies one 16×16
screen cell at the existing 2× scale. No new font package, runtime loader,
operating-system font, shaping engine, or browser font is involved.

## Coverage and fallback

All 33 composed Ukrainian letters have uppercase and lowercase definitions:

```text
АБВГҐДЕЄЖЗИІЇЙКЛМНОПРСТУФХЦЧШЩЬЮЯ
абвгґдеєжзиіїйклмнопрстуфхцчшщьюя
```

The existing table also covers printable ASCII (including space, digits,
command tokens, parentheses, slash, comma, period, colon, semicolon, question
mark and the ASCII apostrophe), plus `’` (U+2019, the same bitmap as ASCII
apostrophe), `«`, `»` and `№`. Use composed letters and these punctuation forms
for bitmap UI prose. This is finite glyph coverage, not arbitrary Unicode
support. Modifier apostrophe `ʼ` (U+02BC), ellipsis `…` (U+2026), combining marks
and emoji currently have no definitions.

`bitmap::ui_char` preserves supported non-control scalars and replaces each
unsupported scalar or control with `?`. `draw_text` applies this policy itself,
so missing glyphs consume a visible cell rather than silently disappearing.
The Main Menu text screen and report display use the same policy. The Main Menu
splits lines and clips by scalar count. Reports preserve the original API text,
ignore carriage return, expand tabs to four-column stops, and preserve newline
for wrapping. Other report controls become `?`. The command area clips prompt,
input and status by scalar count and retains its input tail and caret.

Report layout measures visual columns in scalars but retains UTF-8 byte ranges
and navigation anchors for slicing. Those offsets always fall on character
boundaries, including after wrapping, scrolling and resizing. Text screens and
reports cannot draw into the persistent command area.

## Source and license

These definitions were already shipped in this repository before D5. The
Cyrillic UI bitmap addition is recorded in commit
`8a936b6f38fac8f82b45aa4e1b8f5f7cee75b4c3` (`feat(font): Cyrillic glyphs in
AUTOCADED.SHP, UI bitmap font and DXF UTF-8 support`). D5 reuses the definitions
unchanged under the repository's [MIT license](../LICENSE). No separate upstream
font asset or external font provenance is established by these source files;
this document makes no claim of one. Preserve the repository copyright and
license notice when redistributing the definitions.

## Verification and remaining gate

Focused tests check both 33-letter alphabets, visible punctuation, distinct
Ґ/Г, Є/Е and Ї/І forms, per-cell scaled pixel placement, unsupported/control
fallback, edge clipping, and a pre-D5 printable-ASCII pixel checksum. They also
check translated command/status clipping and caret placement, Main Menu canvas
clipping, report tab normalization, odd-column wrapping, byte anchors through
paging/resize, unchanged original report text, and existing ASCII report layout.
The tests establish coverage and raster behavior; they do not establish visual
legibility of every bitmap.

Native GUI and `acad-wasm::AutoCadSession::render_rgba`/`render_png` consume
`Session::frame` and this same CPU bitmap renderer. Source sharing removes a
separate browser glyph implementation, but actual native/browser visual checks
remain the integration gate. D6 must check every actual Ukrainian template
character against this finite coverage and visually verify command/status/Main
Menu clipping before exposing locale selection. The D5 snapshot contains no
message catalog, so its tests cannot establish actual catalog coverage.

Drawing TEXT content, SHP font metrics, text styles, geometry and export fonts
have separate semantics and are unchanged by this UI renderer work.
