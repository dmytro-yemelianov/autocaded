# T1 one-pass preflight: TEXT options and CHANGE text

Status: contract/risk preflight only; no implementation or verification claim. Read-only baseline `RUN/text-change`, intended baseline625176171ae0. One focused pass over retained HLP118–133/612–633 and native command/app/render source; no broad new evidence search or original execution. Exact B3a/L3 review takes precedence when candidates arrive.

## Recovered help versus native decisions

HLP612–633 specifies TEXT starting point or A/C/R, height as value/two points, angle as angle/point, and text. A uses two end points and skips height/angle prompts. C centers around a point; R right-justifies at an end point. Repeating TEXT through space/Return places the next line below with the same height, angle and alignment. It does not specify ink versus advance bounds, baseline spacing multiplier, all font-control behavior, or what happens to a repeated A span when the new string has a different width.

HLP118–133 says CHANGE moves TEXT to an intersection point and allows a new angle/text; L changes any entity's layer. It does not establish multi-TEXT prompt order, mixed-selection text replacement, blank-input defaults, exact cancellation/undo boundaries, or persistence of alignment intent. Those choices must be declared native policy rather than recovered parity.

## Representation and alignment

Current stored Text has only origin, height, angle and value. Baking alignment into these fields is a reasonable bounded geometry contract, but alignment intent is then absent after reopen and must not be inferred from a matching origin.

Measure actual local font geometry in cap-height-normalized coordinates. If choosing ink-edge alignment as native policy, use both local min_x and max_x: C translates by minus their midpoint; R by minus max_x; A translates the measured left edge to endpointA, scales height so measured width spans |B-A|, and rotates by atan2(B-A). If choosing advance alignment instead, explicitly say so and test trailing spaces/overhangs. Do not mix glyph advance, rightmost ink and width=max-min inconsistently. Apply the offset along the rotated baseline, not worldX. A needs final text before solving height. Reject coincident/nonfinite endpoints, nonpositive/unusable measured widths and overflowed derived geometry before mutation. No-ink strings (spaces), combining/control glyphs, unsupported characters and vertical advance need deliberate policies.

C/R repeated lines must retain their logical center/right anchor in session history, not just the already-shifted stored origin. Different strings have different widths; reuse of the old origin would move the apparent center/right edge. Retain effective height, angle, alignment kind and logical anchor/endpoints independently of the stored Text. A repetition has a real contract ambiguity: fixed original span plus changed string width would change height, while HLP says same height. Do not silently refit height and call it recovered behavior. Choose a documented native follow-up anchor/span policy, or keep that specific continuation unsupported until specified. Likewise “below” determines baseline-normal direction but not a justified numeric leading factor; no arbitrary multiplier should be described as retained fact.

Adding height-by-two-points and angle-by-point requires explicit input states and mouse acceptance/constraint rules; don't parse a point-shaped height answer as a number or accidentally use world-origin angle instead of the chosen baseline anchor. Typed relative-coordinate overflow needs a final finite check after arithmetic. Preserve the existing left-justified numeric path and exact raw text, including significant leading/trailing spaces; current creation passes raw `input` to TextValue while using trimmed `line` for numeric prompts.

## Runtime SHP measurement boundary

`acad-cmd/src/lib.rs::register_shape_library` currently registers names->shape numbers only. `active_shape_library` is not a reliable active TEXT font: shape-only LOAD must not replace a text font. App `Session::register_libraries` owns actual `acad_render::Libraries`. `Library::text` already executes the SHP program for the complete string, returns stroke coordinates and an advance Point, and renderer scales by height/cap_height. Reuse this semantic source through an explicit application/editor measurement interface; do not estimate width as character count, use host system fonts, or silently substitute default TXT after a missing font.

A clean boundary is a pending text-measurement request/continuation or another explicit metric-provider API: editor supplies full string plus required font/context, app measures using its registered Library, editor receives validated normalized metrics before commit. Keep state clone/debug/testability and non-GUI Editor callers considered. A per-character lookup table is not automatically equivalent to whole-string SHP execution (control characters, stateful glyph operations, advanceY, initial glyph1); if registering a compact metric table instead, restrict/document its valid string domain and verify it against `Library::text`.

Metric font resolution must agree with renderer's ordered LOAD semantics, including hidden LOAD, nested INSERT/REPEAT resource context and DOS aliases. For creation, measure in the actual append-position font context; for CHANGE, use the target record's font context rather than merely the drawing's last top-level LOAD. Resource traversal must retain existing bounds/fail-closed behavior. Missing/malformed fonts and absent glyphs should keep a metric-dependent prompt retryable and leave drawing/history unchanged. No dependency on host UI fonts or an implicit char-count fallback.

Library presently exposes cap_height but not the font's below-baseline metric even though it parses definition0. If line spacing uses font-cell metrics, add an explicit exposed metric and documented leading policy; don't mislabel cap height as full line height. General TEXT runtime metrics must remain separate from DIM's compiled `txt_metrics` policy.

## Return history and repeated lines

`submit_return` currently remembers the last command keyword, while raw `submit` is used by scripts/menu macro pieces and does not update that history. Implement TEXT continuation specifically at physical Return/repeat routing. A fresh explicit TEXT should still start a new layout dialogue. A blank raw macro/script command must retain its existing policy and must not accidentally place the next line because it happens to use the same command dispatcher. The application sends literal character spaces into the editable buffer; adding space-as-command-repeat must not consume spaces inside TEXT payloads. If space behavior is deferred, state that limit explicitly.

Update successful-text history only after a valid commit. Failed metrics, cancelled placement/property prompts, unrelated command repetition, open/new and UNDO need explicit history behavior. Prefer restoring text-history with undo or invalidating it when its source edit is undone; do not continue relative to an undone/cancelled line. Reopened drawings must not manufacture interactive last-TEXT history from the final stored text without evidence. Tests should distinguish submit_return from submit and screen-menu GO from raw macro tokens.

## Atomic CHANGE text

Current `dispatch/editing.rs::ChangeIntersection` calls `change_point` immediately. `editor_ops.rs::change_point` validates supported types, saves undo, mutates geometry and refreshes before an optional INSERT-angle prompt. Simply allowing TEXT there and adding later angle/value states would leave its moved origin committed on Escape or invalid text—the main implementation risk.

For a TEXT property edit, capture targets and prospective replacements in pending state; validate all requested geometry/layer/font/text changes, then checkpoint and apply exactly once on the final successful answer. Invalid angle/value/metrics must be retryable without moving the entity or consuming undo. Cancel must discard the entire pending TEXT edit, including origin. If a selection mixes text with LINE/CIRCLE/INSERT, either stage every affected record through the whole continuation or reject the unsupported mixture before any mutation; do not partially commit ordinary geometry while awaiting text answers. Multi-text edits need an explicit replacement/order/default contract; a clearly bounded single-TEXT property path is preferable to accidentally applying one string to every selected object. Keep CHANGE L's existing separate all-entity behavior.

Preserve each target's layer and unchanged height; angle/content changes should update only specified fields. An empty optional answer may mean keep old value if that is the chosen native contract, which differs from new TEXT rejecting an empty string; implement those cases explicitly. Existing groups remain subject to their declared unsupported CHANGE point contract rather than descending or stripping marker/owner metadata implicitly.

## Focused acceptance before later review

- Variable-width strings and trailing spaces, actual SHP font switches and shape-only LOAD, C/R at0/90/arbitrary angles, A with unequal endpoints, and metric/render agreement. Constructed geometry policy tests are not original A/C/R parity fixtures.
- Multiple different-length repeated lines preserve the chosen logical alignment, height and angle; both physical Return and raw submit paths; failure/cancel/undo/new/open history cases.
- Height two-point and angle-point prompts through typed/mouse/API/MCP routes, finite/degenerate inputs and significant text whitespace.
- CHANGE origin/angle/value stay uncommitted through errors/cancel, one final undo, no-op policy, layer preserved, mixed unsupported selection atomic, and clean-document dirty-state checks.
- DWG/DXF save/open preserves baked fields and resulting geometry; no claim that hidden alignment/session history roundtrips. Remember AC1.2's existing cap-height conversion is corpus-specific and must not be broadened silently by new runtime fonts.
- Keep DIM implementation, compiled TXT metrics,29 strict retained fixtures, tolerances and evidence files unchanged. General TEXT feature work must not route DIM through newly variable runtime metrics or alter the separate evidence-limited dimension placement behavior.

AGENT_REVIEW task:T1-preflight verdict:contracts-ready-with-decisions findings: runtime measurement boundary and ordered font context required; alignment/repeat policy ambiguities explicit; CHANGE must stage before commit evidence: retained HLP and scoped current source only next: root specifies bounded native policies before Sol implementation; exact B3a/L3 reviews remain priority notes:this file.
