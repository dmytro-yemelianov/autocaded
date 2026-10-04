# Native TEXT layout, continuation and CHANGE policy

Retained `crates/acad-cmd/resources/acad.hlp` lines 612–633 describe A/C/R,
height by value/two points, angle by angle/point, and repeated lines below the
previous line. Lines 118–133 describe TEXT location, angle and content CHANGE.
The following placement, prompt and history choices are native Rust contracts;
no retained original aligned/repeated TEXT export establishes pixel/layout parity.
DIM's compiled metrics, 29 strict fixtures and comparison tolerances are unchanged.

## Baked layout and font measurement

TEXT accepts a starting point, C center anchor, R right endpoint, or A two ink
endpoints. C/R then accept positive height (numeric or distance between two
finite distinct points) and numeric angle or a distinct point relative to the
logical anchor. A skips those prompts and solves uniform height and baseline
angle from its finite distinct endpoints once the complete string is known.
Typed relative/polar points use each prompt's anchor and reject arithmetic
overflow; mouse/API point placement retains the existing SNAP policy without an
ORTHO projection anchor for these text prompts.

C uses the midpoint of whole-string local ink xmin/xmax; R uses xmax; A positions
xmin at the first endpoint and scales the ink width to the endpoint distance.
Offsets follow the rotated baseline. Actual SHP execution supplies these bounds,
normalized by that library's cap height. Glyph advance and trailing spaces do
not replace ink bounds; leading spaces, overhangs and all stored text characters
remain significant. A needs nonzero ink width, and A/C/R reject no-ink strings.
Left placement retains the existing raw text acceptance, including spaces; an
empty new string is rejected. Metric-dependent operations support one line
without control characters or vertical advance and fail on unavailable glyphs.

The command crate's `TextMetricProvider` knows only drawing, top-level item
position, complete string and normalized bounds. The app supplies runtime SHP
libraries and uses the renderer's bounded ordered LOAD traversal for append or
target-record font context, including hidden records, REPEAT and nested INSERT.
Unused block definitions and erased records do not affect that context. Shape-only
LOAD adds shapes without changing the font. Missing fonts retain their requested
name and fail rather than measuring with stale TXT. Stored/context budget or
block recursion exhaustion fails before geometry changes. App library resolution
refreshes the provider whenever a SHP program is registered.

Standalone command clients without a provider may use the existing verified
compiled TXT printable-ASCII metrics only in startup TXT context without a LOAD.
This limited fallback rejects missing glyphs and unclassified LOAD contexts;
it never estimates width from character count. Its iterator-frame context scan
uses O(depth) pending state, with 100,000 stored visits, 256 stored levels and
16 INSERT levels. Cyclic INSERTs, missing block references and exhausted budgets
fail closed before sibling work can accumulate. Runtime libraries are required
for other font-dependent placement. DIM remains independent of this provider.

## Repeated lines and input routes

Physical Return, screen-menu GO, and an idle physical Space repeat the last
successful command keyword. Space in an editable TEXT value remains a literal
space. API `command` shares the Return route; raw script/menu macro `submit`
retains its existing separate blank-input policy. Raw and Return TEXT/CHANGE
payloads preserve leading/trailing spaces, while numeric answers are trimmed.

Repeating TEXT opens the value prompt directly. Session history retains logical
anchor/alignment, effective height and angle. The next anchor moves 1.5 times
cap height along the negative baseline normal: this leading factor is an
unmeasured native policy. C/R remeasure each new string at their retained logical
anchor. Repeated A preserves effective height/angle and uses the displaced first
ink endpoint; it does not refit the old span to a different-width string. This
resolves the help's same-height requirement as an explicit native choice.

History advances only after successful placement and validates its unchanged,
still-live top-level source. Erase/CHANGE/transform/grouping can invalidate that
source; UNDO restores the prior history with the drawing snapshot. Cancel and
metric/input failure retain earlier committed history. Fresh explicit TEXT opens
a new placement dialogue. New/open never infer interactive history from stored
records. Generic last-command repetition still applies: a physical UNDO becomes
the last command, while raw UNDO leaves Return-command history unchanged.

## Atomic CHANGE properties

CHANGE point mode supports a single top-level TEXT, preserving layer and height.
It stages new origin, optional numeric/point angle, then optional raw text. Empty
angle/content answers keep their old values; a spaces-only content answer stores
spaces. New content validates against the target's font context rather than the
append font. Alignment intent is absent from stored Text, so CHANGE uses its
stored origin as its point anchor and does not reconstruct centering/right intent.

Mixed or multiple-TEXT property selections reject before mutation. The existing
multi-LINE/CIRCLE/INSERT point route remains available. Selections containing
INSERT now stage every selected replacement through the optional angle answer;
Escape or invalid answers cannot leave moved INSERT/ordinary geometry behind.
No-op properties and no-op stored layer assignments create no undo snapshot.
A successful changed property set commits once; retry/cancel preserves drawing,
dirty baseline and undo history. CHANGE L retains common whole-owner layer policy.

## Persistence and validation limits

Existing origin/height/angle/value fields contain the baked geometry; no alignment
or history disk fields were invented. Constructed tests roundtrip both DWG
revisions and historical DXF. DXF's existing six-decimal field rounding is checked
explicitly; no retained comparison tolerance changed. AC1.2's existing 4/3 height
conversion remains the corpus-established codec policy, not newly measured
runtime-font parity. Interactive alignment/history cannot be recovered on reopen.

`acad-cmd/tests/text_change.rs`, `acad-app/tests/text_change.rs` and
`acad-render/tests/text_context.rs` cover rotated variable-width ink, trailing
spaces, A repetition without refitting, two-point/relative/API/MCP prompts,
font switches/shape-only/hidden/group/INSERT LOAD, target-font editing, missing
fonts/glyphs/context budgets, still-live history, Space/Return/raw routes,
retry/cancel/no-op/UNDO, clean-document atomicity and baked-field roundtrips.
