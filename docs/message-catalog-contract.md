# D1: shared message contract, first slice

Contract baseline: `473c90277ca6ddd9a945a639ea57aa0190958005`.
This is a design for D2/D4, subject to independent review before implementation.
It implements the message/localization direction in
[data-driven evolution](data-driven-evolution.md) and the D1–D6/D8/D9 gates in
[the agent loop plan](superpowers/plans/2026-10-06-data-driven-agent-loops.md).
It does not move geometry or input transitions into data.

## Decisions and boundaries

`acad-cmd` owns the single committed message resource and its typed formatter;
`acad-app::Session` owns locale as presentation state. `AutoCadSession` wraps
that Session, with no browser locale/catalog implementation. Standalone Editor
callers keep the existing English API and can explicitly request localized
presentation. Locale is independent of profile, palette, document and header.
Faithful defaults to English; its English text and fallback preserve the exact
baseline wording, case, punctuation and whitespace. Explicit Ukrainian selection
changes presentation only, including in Faithful; profiles must not reset locale.

The first slice contains the idle Command prompt, every LINE/CIRCLE prompt,
the two existing unknown-command messages, and file-action labels. Discovery
summaries use `command.<stable-id>.summary`, beside the existing
`command.<stable-id>.label` identity convention. D3 supplies evidenced metadata;
summary prose belongs in the message resource, never commands.json or browser JS.
Keep historical HELP content and menu macros as retained assets.

| Class | Translate? | Examples and rule |
| --- | --- | --- |
| UI prose | Yes, by stable key | Prompt instructions, action labels, discovery summaries, unknown-command guidance |
| Accepted tokens / syntax | No | LINE, CIRCLE, 2P, 3P, D, ?, Enter/Return input semantics, aliases, API method names, script lines, DXF group names |
| User/document data | No | Drawing names, paths, layer/block names, input buffer, drawing TEXT content; arguments are inserted literally |
| Legacy diagnostics/reports | Later | Pass through canonical English until their own typed migration; do not infer message identity from their prose |

Translated instructions must still show the actual accepted option tokens.
Command tokens shown inside messages are literal ASCII, not translated labels.
DOM insertion uses `textContent`/text nodes; message formatting does not emit HTML.

## Canonical resource and build contract (D2)

Add `crates/acad-cmd/resources/messages.json`, validated by the existing
`crates/acad-cmd/build.rs`. Add `crates/acad-cmd/src/messages.rs`, exported from
`src/lib.rs`. Generate identities, typed message constructors and pre-tokenized
templates into OUT_DIR; generated files are not committed. Extend the existing
build pipeline rather than add a new workspace crate or runtime JSON loader.
There are no network translation services, runtime catalogs or filesystem lookup.

Version 1 has exactly this shape (example, not a second resource):

```json
{
  "schema_version": 1,
  "messages": [
    {
      "key": "error.command.unknown",
      "args": [{"name": "command", "type": "text"}],
      "text": {"en": "unknown command: {command}"},
      "source": "473c902:crates/acad-cmd/src/dispatch.rs:command"
    }
  ]
}
```

Each entry has `key`, `args`, `text`, and nonempty `source`; no optional hidden
behavior fields. `text.en` is mandatory. `text.uk` is optional until D6, whose
declared slice requires it on every member. Add Ukrainian in this same file.
The catalog is hand-edited data with provenance, not extracted on every build.
Every referenced discovery label/summary must resolve to a no-argument entry
before that metadata is exposed as translated discovery. Reserved keys alone
are not a translated capability. D3 can initially publish summary keys as
metadata, with resolution gated until those entries land in D2/D4 integration.

Build validation must reject unknown fields and duplicate JSON object fields
(including duplicate locale properties), schema versions other than 1, duplicate
keys or argument names, invalid identity syntax, unsupported locale properties,
missing/empty English, missing provenance, and generated Rust name collisions.
Use arrays for messages/arguments and a duplicate-aware deserializer for objects;
ordinary JSON maps can silently overwrite duplicate properties.

Keys match `[a-z][a-z0-9_]*(\.[a-z][a-z0-9_]*)+`, at most 96 ASCII bytes;
argument names match `[a-z][a-z0-9_]*`, at most 32 bytes. Version 1 accepts only
argument type `text`, because the only first-slice argument is a command spelling.
Templates allow `{name}`, `{{` and `}}` for literal braces. Reject stray/nested
braces, unnamed/positional fields, format specifiers, selectors and expressions.
Every declared argument must appear at least once and every placeholder must be
declared, in each supplied locale. Repetition/reordering is allowed. Substituted
text is never parsed again: a command containing braces remains literal text.
Translations inherit the entry's argument types; they cannot redefine them.

Finite build bounds: resource at most 1 MiB, 1–256 entries, at most 8 arguments
per entry, each supplied template at most 4096 UTF-8 bytes. These are catalog
validation budgets, not new command/input limits. Preserve arbitrary existing
input length and user data; formatting must not truncate arguments. No recursion,
plural engine, expression evaluator, dynamic includes or unbounded template work.

Expose the following shared APIs (generated variant names may be idiomatic):

```rust
enum Locale { En, Uk }
impl Locale { fn parse(tag: &str) -> Result<Self, LocaleError>; }
enum MessageId { /* generated stable IDs */ }
enum Message { /* generated no-arg variants; UnknownCommand { command: String } */ }
impl Message {
    fn id(&self) -> MessageId;
    fn render(&self, locale: Locale) -> String;
}
pub const fn text(id: MessageId, locale: Locale) -> Result<&'static str, FormatError>;
fn resolve_key(key: &str) -> Option<MessageId>;
```

`text` is generated as a const lookup so retained public static task arrays can
use the same English source. It accepts only no-argument IDs; a parameterized ID returns
`FormatError::ArgumentsRequired`, never unresolved braces. Typed constructors
make missing, extra and wrong-type Rust arguments unrepresentable. `resolve_key`
exists for validated discovery metadata; failure is a developer/data error, not
an English sentence or raw key displayed to the user. Do not add a public generic
map-of-values formatter merely for hypothetical future needs. Emit a shared
`CATALOG_JSON` constant for inspection if needed, not a second browser resource.

## Locale, number and plural policy

`Locale::parse` accepts exactly two ASCII letters, optionally followed by `-`
and two ASCII region letters, case-insensitively. `en`, `EN-us`, `uk`, `uk-UA`
select En/Uk by language; a syntactically valid unsupported language (`fr-CA`)
selects En. Reject empty tags, whitespace, underscores, digits, trailing hyphens,
longer/extra subtags and non-ASCII with `LocaleError::MalformedTag`; no mutation
on failure. This deliberately small grammar is not a full BCP-47 parser.
Default is En, with no OS/browser-language inference. For a missing Ukrainian
entry, resolve that complete English entry with the same arguments. Never mix
template fragments, return a blank value, or fall back to a string key.

The slice has no numeric substitutions or pluralized messages, so v1 rejects
numeric argument types and plural selectors. Literal numbers in prompts (2P/3P)
stay exact. Input parsing, coordinates, geometry and DWG/DXF numeric formatting
keep their existing Rust policy independently of locale. Do not use browser Intl,
OS formatting or locale decimal commas. A later actual count/scalar message must
extend this schema with a typed numeric value and explicit, tested rounding and
en/uk plural rules; that extension is outside D2/D4. This avoids introducing
unused CLDR/ICU or count heuristics into the first slice.

## Prompt and error integration (D4)

`InputState::prompt()` currently returns static English; `Editor::prompt()` and
`Session::prompt()` return `&str`. Keep those borrowing semantics. Add
`InputState::prompt_message() -> Option<MessageId>` for the eleven migrated
states, and `Editor::prompt_for_locale(Locale) -> &str`. `Editor::prompt()` delegates
to En. Migrated arms obtain English from generated static entries; remove their
old literal source. Unmigrated states return their existing English prompt.
`Session::prompt()` uses its locale and Editor's localized accessor, retaining
the existing Main Menu prompt precedence. Do not cache formatted prompt strings
or change the entire state machine to own Strings for these static messages.

There are two error messages, and both must survive:

* Raw `Editor::submit()` / `execute()` unknown command: `error.command.unknown`,
  with named text argument `command` containing exactly the baseline spelling
  passed to `command()`; English is `unknown command: {command}`.
* Return/GO unknown command, including empty Return without history:
  `error.command.unknown_return`, no args; English is
  `Unknown command. Type ? for list of commands.`

`dispatch.rs::submit_return` currently recognizes raw unknowns by
`starts_with("unknown command: ")`. Replace that decision with a typed
`CmdErrorKind::UnknownCommand` and message identity from the dispatch boundary.
`error.rs::CmdError::classify` currently guesses kind from English;
`execute()` maps through that classifier. Set the migrated error's kind explicitly
at creation. Keep the classifier only for unmigrated canonical-English errors;
never pass localized text into it or add Ukrainian classification substrings.

A bounded compatibility implementation may expose an optional per-submission
`CommandDiagnostic { kind: CmdErrorKind, message: Message }` from Editor alongside
the existing String result. It must be cleared at the start of every submission,
set at the unknown-command creation site, and replaced with the Return message
at the Return boundary. `execute()` uses this typed diagnostic directly for the
migrated error; successful/unrelated subsequent submissions must not reuse it.
Pass it immediately with the corresponding result into Session's effect/status
boundary, including raw macro paths. Do not reconstruct it from the error text,
status comparison or re-parsing input after dispatch. An internal typed result
is also acceptable if all current public String wrappers remain compatible.

Transport compatibility is deliberate: `submit`, `submit_return`, `execute`'s
Display/message, app API error strings, script diagnostics and wasm JsValue errors
retain their existing canonical English and kind. The shared Session's displayed
status may render a typed diagnostic with its locale; retain that Message to
re-render it on locale change. Legacy status remains English. A display cache
must be invalidated whenever a status is cleared/replaced, including API failure,
cancel and script/macro paths; it must not relabel a later error as an unknown.
Do not change success/error transport shapes or Return history, retry, cancel,
selection, undo or raw empty-input behavior for this presentation migration.

Expose `Editor::command_idle() -> bool` from InputState identity and
`Session::command_idle() -> bool` (false on the Main Menu). Add `command_idle`
to app API state and wasm state JSON without removing existing fields. The
browser zoom guard currently compares `state.prompt !== 'Command'`; replace it
with this semantic flag. Audit consumers of migrated prompt/status literals for
control flow before exposing Uk. Presentation keys are not parser tokens.

## Inventory and exact English

All entries below are no-argument except `error.command.unknown`. Prompt strings
do not include the command area's appended `: ` separator or input/caret.
Source references refer to the baseline above, not line-number stability.

| Stable key | Exact English | Baseline source |
| --- | --- | --- |
| `prompt.command` | `Command` | acad-cmd/src/input_state.rs, Command |
| `prompt.line.start` | `LINE: first point` | same, LineStart |
| `prompt.line.next` | `LINE: next point (Enter to finish)` | same, LineNext |
| `prompt.circle.center` | `CIRCLE: center point (or 2P/3P)` | same, CircleCenter |
| `prompt.circle.radius` | `CIRCLE: radius or point (D for diameter)` | same, CircleRadius |
| `prompt.circle.diameter` | `CIRCLE: diameter` | same, CircleDiameter |
| `prompt.circle.two_point.first` | `CIRCLE 2P: first diameter endpoint` | same, CircleTwoPointFirst |
| `prompt.circle.two_point.second` | `CIRCLE 2P: second diameter endpoint` | same, CircleTwoPointSecond |
| `prompt.circle.three_point.first` | `CIRCLE 3P: first point` | same, CircleThreePointFirst |
| `prompt.circle.three_point.second` | `CIRCLE 3P: second point` | same, CircleThreePointSecond |
| `prompt.circle.three_point.third` | `CIRCLE 3P: third point` | same, CircleThreePointThird |
| `error.command.unknown` | `unknown command: {command}` | acad-cmd/src/dispatch.rs, command |
| `error.command.unknown_return` | `Unknown command. Type ? for list of commands.` | same, submit_return |
| `ui.action.new` | `New` | New presentation identity; no baseline visible control |
| `ui.action.open` | `Open` | New bare action identity; not replacement wording for Open… |
| `ui.action.save` | `Save` | New bare action identity; not replacement wording for Save as… |
| `ui.action.export` | `Export` | New presentation identity; no baseline visible control |
| `ui.file.open_picker` | `Open…` | web/index.html, open-select empty option |
| `ui.file.save_picker` | `Save as…` | web/index.html, save-select empty option |
| `ui.main_menu.new` | `Begin a NEW drawing` | acad-app/src/session/main_menu.rs, MAIN_MENU_TASKS[1] |
| `ui.main_menu.open` | `Edit an EXISTING drawing` | same, MAIN_MENU_TASKS[2] |
| `ui.main_menu.export_dxf` | `Make drawing interchange file` | same, MAIN_MENU_TASKS[5] |
| `command.<id>.label` | Existing canonical command token initially | acad-cmd/resources/commands.json; D3 integration |
| `command.<id>.summary` | D3-evidenced prose, no baseline promise | same command identity; resource is sole prose source |

The 22 explicit entries plus 61 labels and 61 summaries fit the 256-entry budget.
Do not invent New/Export toolbar controls to consume bare keys. Native historical
task entries retain their exact separate wording/order; migrate those three
entries via static English catalog lookup and localized presentation lookup,
without replacing retained task labels by modern bare action labels. Other task
entries and headings remain explicitly unmigrated. Browser picker option values,
format names and export extensions remain stable. Existing DOM labels must obtain
formatted no-arg strings through `AutoCadSession::ui_labels()` (versioned JSON
object of these fixed UI keys), delegated to Session, and refresh after locale
switching. It contains no independent translations or template formatter.

## Locale propagation, glyph gate and implementation seams

Add `Session::locale() -> Locale`, `set_locale(Locale)` and
`set_locale_tag(&str) -> Result<(), LocaleError>`. Parsing failure is atomic;
success only refreshes presentation. Add wasm `locale()` and `set_locale(&str)`
delegation, with malformed-input errors and no duplicate wasm locale field.
Add app API locale selection as a small separate consumer slice when needed for
native/headless switching; API identifiers are always locale-independent.

Session replacements must inherit presentation: `reset`/`open_drawing` in
session/mod.rs, API `Request::Open` in api.rs, `open_dropped` in lifecycle.rs,
and Main Menu `adopt` in main_menu.rs. Prefer a shared
`inherit_presentation(&Session)` helper copied before replacement, with locale
independent of palette. A new standalone `Session::open` defaults to En; replacing
an existing Session inherits its selected locale. Do not save it in Drawing,
Header, DWG/DXF or Document. D8 extends this shared presentation owner with profile
state rather than introducing a parallel browser setting. A locale/profile
switch preserves drawing/view/undo, current input, pending command and scripts.

Glyph evidence: baseline `acad-app/src/bitmap.rs::get_glyph` maps Ukrainian
uppercase/lowercase, including Ґ/ґ, Є/є, І/і, Ї/ї; its
`cyrillic_characters_have_visible_glyphs` test checks nonblank bitmaps.
`bitmap::draw_text` iterates Unicode scalar values at fixed 8x8 cells scaled 2x;
`command_line.rs` clips/measures with `chars()`. Wasm calls shared Session rendering.
However `presentation.rs::draw_text_screen` replaces every non-ASCII character
with `?`, and unknown glyphs in `draw_text` are currently skipped. D5 should first
verify every actual Uk template character/punctuation and its clipping path,
then remove the unsupported ASCII-only sanitization using glyph-aware policy.
Use composed Ukrainian letters and covered punctuation; do not assume arbitrary
Unicode shaping or combining-mark support. D5 checks provenance of the existing
bitmap asset and visual native/wasm parity before proposing any external font.
Drawing TEXT, SHP metrics, styles and export fonts are separate semantics.

D2 changes messages.json, build.rs, messages.rs and lib.rs, with focused catalog
tests. D4 prompt/error integration touches input_state.rs, dispatch.rs, error.rs,
lib.rs and Session commands/effects/presentation where typed status is handed off.
File labels and transport/DOM consumers are separate bounded follow-up slices
in main_menu.rs, api.rs, acad-wasm/src/lib.rs and web/index.html. No consumer may
retain a second English literal as its runtime source after migration; tests may
keep exact expected strings as evidence. Do not combine these seams into a new
interaction interpreter. D9 can later select one actual repeated transition pilot.

## Acceptance and deferred coverage

* D2 rejects the validation cases above, argument/placeholder mismatch in either
  locale, malformed tags and budget excess; same input gives identical output
  on native and wasm. Tests cover fallback, brace escaping, repeated/reordered
  fields, literal argument braces and no-arg lookup of a parameterized ID.
* D4 snapshots all eleven English prompts and both error routes. Test LINE finish,
  CIRCLE radius/diameter/2P/3P, invalid-answer retry, cancel, unknown Return with
  and without history, raw submission and script/macro history. `execute` retains
  UnknownCommand kind and exact English even with Uk display; no stale diagnostic.
* Locale tests cover En/Uk, region/case, unsupported valid fallback, malformed
  atomic failure, fallback per whole entry, pending command/status rerender, and
  preservation across reset, every existing open/adopt route and profile switch.
  Compare drawing/header, serialization, input and undo before/after switching.
* D5/D6 require actual Ukrainian templates to pass glyph coverage, visible native
  and browser command/status/Main Menu clipping checks, and browser zoom behavior
  with translated Command. Do not expose Uk selection until glyph checks and
  complete declared-slice translations pass. D2 can validate Uk formatting earlier.
* Discovery validates all referenced message keys and no-arg shape in integration;
  English prose is only in messages.json. Browser/native labels resolve identically.

Unmigrated: other command families/validation errors, `*Invalid*`, reports/HELP,
configuration/Main Menu headings and other tasks, menu labels/macros, resource
diagnostics, file-operation toasts/tooltips and picker format descriptions, mode
badges, accessibility prose, transport errors and drawing TEXT/fonts. Explicit
slice coverage is required; this is not a full-application i18n claim.

Inventory discovery used project `autorust` graph searches and
`get_code_snippet(InputState.prompt)` first. The graph snippet pointed to the live
checkout and omitted newer error/bitmap symbols; exact scoped snapshot reads
confirmed the source facts above. No implementation or tests were changed/run
for this documentation-only contract.

## D4 consumer implementation

The first consumer slice now resolves the eleven prompts and both unknown-command
routes from the shared catalog. `Session::status()` retains canonical transport
text; `display_status()` supplies rendered status to the shared frame and title.
Every production status replacement clears its diagnostic through `set_status`;
app API and mutable wasm operations use `with_error_status` so unrelated failures
replace the status without guessing message identity from prose. Script context
messages remain canonical English after replacing the captured command status.

Session owns locale across reset/open/drop/Main Menu adoption. Headless callers
can use `set_locale` with `{"tag":"en"}` or `{"tag":"uk"}`; wasm callers use
`set_locale(tag)`. Missing Uk entries currently fall back to complete English
entries. No locale chooser or translations are added in D4. Picker labels come
from the versioned `ui_labels()` JSON and are refreshed via `textContent` during
redraw; native pan and browser zoom/touch navigation use semantic `command_idle`.

## D6 Ukrainian selection

The declared 22-key slice now supplies complete Ukrainian templates in the same
resource. Native `--locale en|uk` and the browser's partial-Ukrainian header
chooser delegate to Session; native API state also reports the active locale.
The chooser synchronizes from shared state after success or rejection.
[Coverage and limits](ukrainian-ui-slice.md) document the English text remaining
outside this slice. D10 owns actual native/browser visual checks.
