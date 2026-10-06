# Data-driven foundation: validation and release scope

Date: 2026-10-07 (Europe/Kyiv). Local source candidate based on v0.4.3.
The [progress ledger](superpowers/plans/2026-10-06-data-driven-progress.json)
records exact isolated candidates, independent reviews, repairs and integration.
The [architecture overview](data-driven-evolution.md) describes the design.

## Implemented scope

- One Rust engine and drawing formats, retaining the early command-driven UI.
- Validated single-source command facts: 61 identities and 90 accepted spellings,
  typed dispatch, evidenced availability/lineage and retained help references.
- Typed messages with strict placeholders/arguments and whole-entry English
  fallback. All 28 catalog entries have Ukrainian templates: 22 initial prompt,
  diagnostic and file-action entries, plus six profile labels/statuses.
- Native/browser Ukrainian bitmap UI support, scalar-aware report wrapping,
  explicit partial-language selection, Session locale propagation, and semantic
  command-idle checks independent of translated prompt text.
- 23 hatch descriptions extracted with exact inventory and versioned provenance.
- Shared validated frozen/modern presentation profiles: existing palette presets,
  labels/statuses and badge tones. Locale and manual palette remain independent.
- A measured ARC choice pilot was rejected; its inactive prototype and independent
  parity tests remain evidence. Production transition/geometry logic stays Rust.

English remains the default and retained English catalog text is unchanged.
Profile/locale switches preserve drawing/header, view, undo, dirty state, pending
commands, input, resources and script progress. Presentation settings add no file
bits; command tokens and raw transport/script errors remain canonical English.

## Final gates

| Check | Result |
| --- | --- |
| `cargo fmt --all --check`, `git diff --check` | Passed |
| `cargo clippy --workspace --all-targets -- -D warnings` | Passed |
| `cargo clippy -p acad-wasm --target wasm32-unknown-unknown -- -D warnings` | Passed |
| `cargo test --workspace --exclude acad-oracle` | 1,052 passed, zero failures, one explicit existing ignored test |
| `node --test tools/tests/web_*.test.mjs` | 20 passed |
| `lake build` in `formal/` | Passed, eight jobs; formal source unchanged |
| Release native and wasm builds; matching wasm-bindgen browser bindings | Passed |
| Original QEMU CIRCLE comparison | One passed, zero skips |
| Original QEMU DBLIST/LIST comparison | One passed across six cases, zero skips |
| Full attached native GUI/API/MCP smoke | Passed |
| Live native Ukrainian/profile drawing workflow and visual inspection | Passed |
| Live browser workflow, visual inspection and 390×640 viewport | Passed |

The ignored test is `translated_boot_reaches_the_menu_and_accepts_exit`, which is
explicitly marked as requiring recovered CFG/original images. It is not a pass.
QEMU/System.img prerequisites were confirmed for the two selected oracle tests;
`AUTOCAD_REQUIRE_CORPUS=1` and zero-skip log checks were used. This run did not
repeat the entire historical oracle matrix or cross-compile Windows.

Live checks exercised LINE and CIRCLE, undo after profile/locale switching,
DWG/DXF save/reopen, manual palette overrides, native Main Menu task labels,
unknown-command rendering, and keyboard selection without CAD submissions.
The browser PNG hash stayed identical when locale/prompt changed; SVG excluded UI
text. Native/browser screenshots were inspected for Ukrainian glyphs. Header
controls now wrap into readable narrow-screen rows; the command area's existing
clipping behavior remains.

The native smoke harness had stale pre-retained-format expectations. Its LIST
assertions now check exact report coordinates and the separate selection status;
PAN expectations use the retained positive displacement. Existing report/view
production files and golden fixtures were not altered for these corrections.

## Explicit limits and publication state

Localization is partial. Other command families, validation errors, reports/HELP,
menu headings, tooltips and resource diagnostics retain English. Command discovery
label/summary keys exist as metadata, but their translated UI/messages are future
work. These profiles change presentation presets, not geometry policies. Unit
metadata, bindings and wider declarative rules remain future migrations.

This is a local reviewable candidate: no new tag, GitHub release, deployment,
Windows package or universal macOS bundle was published in this run. v0.4.3 remains
the published baseline. The native and browser release binaries were rebuilt.

Full logs, smoke scripts, screenshots and check summaries are retained under the
D10 artifact path recorded in the ledger. Runtime per-agent token usage is not
exposed; routing, attempts and review misses are recorded without estimated cost
or savings claims. Independent final Sol high review passed exact snapshot
`c7c79d30ae3234d1d3cca2a4a22d8d78d608587e`; no actionable findings remained.
All D0–D10 gates are verified. Only completion metadata changed after review.
