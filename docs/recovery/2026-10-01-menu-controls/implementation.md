# Menu controls implementation brief

Task 2 implementation brief, baseline `53d156a044bd8cc2204488cedbfc27b23214c32d`.
Task 1's independent review accepted the finite observations in [README](README.md)
and [claim-index.tsv](claim-index.tsv). This document freezes the interfaces and
expected tests for Tasks 3/4; it does not claim those Rust changes already exist.
The controller's scope rulings defer mouse projection and permit the scoped editor
Return/MENU corrections below. Historical planning-only metadata is superseded.

## Interfaces and ownership

Adopt the following public editor API on the exported `acad_cmd::Editor` type
(the file ownership below places the Return implementation in `dispatch.rs`):

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MenuControl { Snap, Ortho, Cancel }
pub fn apply_menu_control(&mut self, control: MenuControl) -> Result<Effect, String>;
pub fn submit_return(&mut self, input: &str) -> Result<Effect, String>;
// Add to the existing Effect enum:
// UnloadMenu
```

Task 3 owns `MenuControl` and `apply_menu_control` in `lib.rs`, editor regression
cases in `crates/acad-cmd/tests/editor.rs`, app control routing/tests in
`crates/acad-app/src/main.rs`, and fixture tests in
`crates/acad-oracle/tests/menu_controls.rs`. A separate module is unnecessary for
three small operations. Do not feed control bytes to text `submit` or enter typed
SNAP/ORTHO prompts for these immediate operations. Snap flips only `header.snap.on`, Ortho flips only
`header.ortho`; both preserve spacing bit-for-bit, command state, pending geometry,
completed items, and the repeat marker. Return `Ok(Effect::Continue)` and replace
editor status with exactly `<Snap on>`, `<Snap off>`, `<Ortho on>`, or `<Ortho off>`.
The native output retains earlier messages in its transcript; the app displays the
latest status, so no transcript-append implementation is required.

Menu Cancel sets `state = InputState::Command`, status `*Cancel*`, and returns
`Ok(Effect::Continue)`. It retains completed drawing mutations and `repeat_start`.
It must **not call current `cancel_command`**, which clears `repeat_start` and
status. Existing keyboard Escape still clears App.input and calls that existing
method; this slice does not claim Escape equals native menu Cancel. Cancel does
not undo a completed entity, unload the menu, change its page, or supply a point.

Task 4 owns private `last_return_command: Option<String>` (initialized to `None`
in `Editor::new`) and `Effect::UnloadMenu` in `lib.rs`; the public `submit_return`
method implementation and scoped Return corrections in
`crates/acad-cmd/src/dispatch.rs` (alongside the private command dispatcher); corresponding
editor tests; shared app Return/GO submission and unload handling/tests in
`main.rs`; and GO fixture tests in the same oracle test file. Keep enum/prompt
variants in `input_state.rs` unchanged: existing Command, LineStart, LineNext,
CircleRadius, MenuFile, EditSelection(Erase), and RepeatColumns suffice. No changes
to `editor_ops.rs`, model, codecs, rendering, Session, splitter or geometry are
needed. `menu_panel.rs` keeps its current hit rules; only a stale GO comment may
be corrected in Task 4 if necessary. NEXT keeps its independent wrapping helper;
GO must leave that pagination helper and perform a submission.

`submit_return` is an explicit physical Return/GO entry point; `submit` retains
its existing macro/script policy. At Command, a successful **nonempty** Return
command dispatch records the trimmed, uppercased command in `last_return_command`.
Prompt answers, rejected unknown commands, controls, and empty submissions do not
replace it. At Command with empty Return, dispatch the remembered command once;
with no remembered command, retain Command and no items and return
`Err("Unknown command. Type ? for list of commands.")`, also setting editor status
to that exact string. An unknown nonempty Return command uses that same native
status/error and retains known history. Nonempty macro `submit` calls do not gain
history side effects; macro `submit("")` stays the existing Command no-op. Success
recording for other accepted command names is a Rust policy extension; native
history fidelity is asserted only for the observed MENU, POINT and LINE histories.
No M1–M3 custom macro compatibility claim is made.

At the following active prompts, `submit_return` implements only these scoped
corrections, using the existing dispatcher for valid nonempty answers:

| State / Return input | Result and postcondition |
|---|---|
| LineStart / empty | Continue; status `*Invalid*`; Command; no LINE |
| CircleRadius / empty | Continue; status `*Invalid*`; Command; discard pending center |
| LineNext / empty | existing finish behavior; Command; retain completed segments |
| EditSelection(Erase) / empty | existing empty-selection behavior; Command; retain seed LINE |
| MenuFile / empty | `Ok(Effect::UnloadMenu)`; Command; clear editor status |
| RepeatColumns / invalid positive integer (`REPEAT` observed) | Continue; status `*Invalid*`; Command; retain repeat marker and points |

For the confirmed single-picked-object ERASE Return/GO case, status must be
exactly `1 selected, 1 found.` after the existing erase dispatcher succeeds. Apply
that Return-specific status in `dispatch.rs`, leaving the raw macro dispatcher
and `editor_ops.rs` unchanged. General native selection-report formatting is not
recovered.

Do not generalize Invalid/Command to other prompts or every parser failure.
Menu Cancel at MenuFile returns Continue and retains the panel. Keep raw macro
MENU blank behavior unchanged through `submit`; unloading belongs to the explicit
Return path. This avoids quietly expanding the excluded macro empty-input scope.

In `main.rs`, extract the current Return behavior into exactly this testable seam:

```rust
fn submit_return_input(
    &mut self,
    handle_result: &mut impl FnMut(&mut Self, Result<acad_cmd::Effect, String>),
);
```

It takes `App.input` once with `std::mem::take`, clears app status, performs the
existing `awaiting_shape_library_name` / `resolve_shape_library` preprocessing,
then calls `editor.submit_return(&command_input)` once and invokes the callback
once. Resolution failure sets app status and performs no editor submission;
the already-taken buffer stays empty, matching current Enter extraction. The
keyboard Enter branch and actual PanelHit::Go both invoke this helper. The
production keyboard closure delegates to `handle_result(el, result)`. Existing
`submit_line` may become a thin adapter or be removed once unused. Plain macro
pieces continue to call `editor.submit(piece)` directly, with their existing
LOAD bypass and empty-piece behavior. This preserves M3 as an explicit gap.

Map only one-byte `[0x02]`, `[0x0f]`, `[0x03]` to the enum in App panel routing;
other raw controls retain unsupported status. Snap/Ortho leave App.input intact;
that preservation is a conservative app policy, not a separately observed native
partial-buffer toggle claim. Cancel clears App.input before editor cancellation,
including picked ids and paced text. Route all recovered effects through the
callback. `UnloadMenu` handling sets menu=None and menu_page=0, clears app status,
and refreshes existing window minimum-size policy through `apply_menu_window_size`
when a window exists. Tests must use a callback that actually handles UnloadMenu,
not one that merely records it, to establish the app state.

GO is exposed by `entry_at` only on the page owning the parsed header action
`b";"` (ACAD page 0); page-1/page-2 top blank slots remain consumed with no action.
All panel hits consume before drawing submission/selection. These are existing
Rust hit/consumption obligations: [BLANKHI landing](buffers/captures/BLANKHI-024-probe.png)
and [TOPROW](controls/captures/TOPROW-012-probe.png) do **not** establish true native
blank hits, so they authorize no hit-test change. Literal keyboard `;` remains
text until Return; GO is not a macro expansion of that text.

## Exact tests and native artifacts

All paths below are actual staged fixtures under this directory. There are no
`SCIDLE-before.DWG` / `SCIDLE-after.DWG` files. Read the listed `.dwg` exports with
`acad_dwg::parse`; `.decoded.txt` is the readable companion. For idle toggles use
`pilot/drawings/BASEOFF.dwg` or `controls/drawings/BASEON.dwg` as the independently
exported starting drawing ([BASEOFF settings](pilot/drawings/BASEOFF.decoded.txt),
[BASEON settings](controls/drawings/BASEON.decoded.txt)); do not invent a
same-session before export. Pending
states are recreated from the recorded typed setup, not read from a DWG. Compare
controlled flags and spacing plus full item kind/order/layer/geometry, not unrelated
whole headers or native passthrough bytes. All listed entities use layer 1.
Typed values below are exact Rust doubles; no mouse constraint tolerance is needed
for them. For selected native LINE geometry, start from CSELECT's native LINE so
Return/GO only change its wrapper; compare its coordinate bits unchanged. Live
QEMU runs are unnecessary for these fixture regressions and are not authorized by
Task 2; if future live tests retain absence guards, report skips honestly.

| Task / exact Rust test name | Setup, expected assertion, and artifact links |
|---|---|
| 3 `menu_control_snap_idle_matches_native` | BASEOFF false/.5/false → Snap true/.5/false, items []; [gate 025](pilot/captures/SCIDLE-025-probe.png), [post 027](pilot/captures/SCIDLE-027-probe.png), [SCIDLE.dwg](pilot/drawings/SCIDLE.dwg), [snapshot](pilot/drawings/SCIDLE.decoded.txt). Status `<Snap on>`. |
| 3 `menu_control_ortho_idle_matches_native` | BASEOFF → false/.5/true, []; [gate](pilot/captures/OCIDLE-025-probe.png), [post](pilot/captures/OCIDLE-027-probe.png), [OCIDLE.dwg](pilot/drawings/OCIDLE.dwg). Status `<Ortho on>`. |
| 3 `menu_control_twice_and_initial_on_match_native` | Two Snap or two Ortho clicks from BASEOFF restore false/.5/false; first/second status on/off. From BASEON true/.5/true, Snap gives false/.5/true, Ortho gives true/.5/false. [SCTWICE posts](controls/captures/SCTWICE-025-probe.png), [SCTWICE.dwg](controls/drawings/SCTWICE.dwg), [OCTWICE second](controls/captures/OCTWICE-032-probe.png), [OCTWICE.dwg](controls/drawings/OCTWICE.dwg), [SCON post](controls/captures/SCON-035-probe.png), [SCON.dwg](controls/drawings/SCON.dwg), [OCON post](controls/captures/OCON-035-probe.png), [OCON.dwg](controls/drawings/OCON.dwg). |
| 3 `menu_control_preserves_fractional_line_pending_state` | BASEOFF; LINE,2.1,3.15; Snap/Ortho retains Rust `LINE: next point (Enter to finish)` and anchor; submit 4.25,5.15 then empty; exactly LINE (2.1,3.15)→(4.25,5.15), flags true/.5/false or false/.5/true. Native `To point:` and status plus continuation: [FSLINE post](controls/captures/FSLINE-031-probe.png), [continuation](controls/captures/FSLINE-035-continuation.png), [FSLINE.dwg](controls/drawings/FSLINE.dwg), [FOLINE post](controls/captures/FOLINE-031-probe.png), [continuation](controls/captures/FOLINE-035-continuation.png), [FOLINE.dwg](controls/drawings/FOLINE.dwg). |
| 3 `menu_control_preserves_circle_radius_pending_state` | BASEOFF; CIRCLE,2,3; Snap/Ortho retains Rust `CIRCLE: radius`; submit 1.25 → Command and exactly CIRCLE center (2,3),radius 1.25. Flags as above. [SCIRCLE post](controls/captures/SCIRCLE-031-probe.png), [continuation](controls/captures/SCIRCLE-033-continuation.png), [SCIRCLE.dwg](controls/drawings/SCIRCLE.dwg), [OCIRCLE post](controls/captures/OCIRCLE-031-probe.png), [continuation](controls/captures/OCIRCLE-033-continuation.png), [OCIRCLE.dwg](controls/drawings/OCIRCLE.dwg). |
| 3 `menu_control_cancel_retains_completed_segment` | LINE,2,3,4,5; Cancel → Rust `Command`, `*Cancel*`, exactly one LINE (2,3)→(4,5); POINT,8,7 → same LINE then POINT (8,7). [CLDONE gate](pilot/captures/CLDONE-019-probe.png), [post](pilot/captures/CLDONE-021-probe.png), [continuation](pilot/captures/CLDONE-025-continuation.png), [CLDONE.dwg](pilot/drawings/CLDONE.dwg). |
| 3 `menu_control_cancel_discards_only_pending_geometry` | Idle, LINE before first point, LINE after 2,3, and CIRCLE after 2,3 each Cancel → Command/*Cancel*, zero items; POINT,8,7 → exactly POINT (8,7). [CIDLE post](cancel/captures/CIDLE-015-probe.png), [CIDLE.dwg](cancel/drawings/CIDLE.dwg), [CLSTART post](cancel/captures/CLSTART-017-probe.png), [CLSTART.dwg](cancel/drawings/CLSTART.dwg), [CLSEG post](cancel/captures/CLSEG-019-probe.png), [CLSEG.dwg](cancel/drawings/CLSEG.dwg), [CCENTER post](cancel/captures/CCENTER-019-probe.png), [CCENTER.dwg](cancel/drawings/CCENTER.dwg). |
| 3 `menu_control_cancel_retains_repeat_marker` | REPEAT;POINT;4,5;Cancel; ENDREP still prompts Rust `ENDREP: columns` (native Number of columns). Repeat marker and POINT (4,5) retained. [CREPEAT Cancel post](cancel/captures/CREPEAT-021-probe.png), [ENDREP](cancel/captures/CREPEAT-023-continuation.png), [native DXF](cancel/drawings/CREPEAT.dxf), [parse error](cancel/drawings/CREPEAT.parse-error.txt). Assert prompt/point, **not** completed group or full parsed native snapshot. |
| 3 `app_mouse_route_menu_cancel_clears_buffers_and_retains_selection` | Real handle_left_click on parsed Cancel clears App.input values `p`, `2`, `1`; corresponding idle/LINE-first/ERASE state → Command/*Cancel*, then clean POINT (8,7). [CBIDLE post](cancel-tail/captures/CBIDLE-019-probe.png), [CBIDLE.dwg](cancel-tail/drawings/CBIDLE.dwg), [CBPOINT post](cancel-tail/captures/CBPOINT-021-probe.png), [CBPOINT.dwg](cancel-tail/drawings/CBPOINT.dwg), [CBSELECT post](cancel-tail/captures/CBSELECT-021-probe.png), [CBSELECT.dwg](cancel-tail/drawings/CBSELECT.dwg). Also seed native picked LINE (1.96078431372554,3.58823529411769)→(11.7647058823534,3.58823529411769), perform a real Rust drawing pick, then Cancel: live LINE unchanged, no erasure/point leak, input empty. [CSELECT post](pilot/captures/CSELECT-036-probe.png), [CSELECT.dwg](pilot/drawings/CSELECT.dwg); pick validity control [CSELBASE post](buffers/captures/CSELBASE-033-probe.png). |
| 3 `app_mouse_route_menu_controls_preserve_prompt_and_consume_panel` | Split old unsupported-control test, retaining its ordinary POINT route. Actual Snap/Ortho panel clicks change flags/status and preserve App.input, pending LINE/CIRCLE, item count and page; callback once. Buffer preservation assertion is Rust policy; pending-state evidence is FSLINE/FOLINE/SCIRCLE/OCIRCLE above. Unknown control retains unsupported status. Blank slots remain consumed with zero callbacks/items under existing hit rules (Rust regression only). |
| 3 `app_mouse_route_cancel_preserves_later_pages` | Actual Cancel on pages 1/2: page remains 1/2, menu Some, LINE-first cleared, input empty; independent POINT (8,7). [CPAGE1 post](cancel-tail/captures/CPAGE1-022-probe.png), [CPAGE1.dwg](cancel-tail/drawings/CPAGE1.dwg), [CPAGE2 post](cancel-tail/captures/CPAGE2-027-probe.png), [CPAGE2.dwg](cancel-tail/drawings/CPAGE2.dwg). |
| 4 `menu_return_fresh_unknown_and_semicolon_are_distinct` | Fresh empty submit_return: Command, native Unknown error/status, zero items. Literal `;` buffer is still `;` before Return, then same Unknown. Raw submit("") remains Continue/no-op (Rust regression). [GOFRESH post](go/captures/GOFRESH-013-probe.png), [GOFRESH.dwg](go/drawings/GOFRESH.dwg), [RTFRESH post](go/captures/RTFRESH-010-probe.png), [RTFRESH.dwg](go/drawings/RTFRESH.dwg), [semicolon buffered](pilot/captures/SEMICOL-014-probe.png), [returned](pilot/captures/SEMICOL-018-continuation.png), [SEMICOL.dwg](pilot/drawings/SEMICOL.dwg). |
| 4 `app_mouse_route_go_and_return_repeat_menu_once_then_unload` | Submit_return MENU,ACAD and load parsed panel; empty GO/keyboard helper stops once at Rust `File name`, menu remains Some/page0, exactly one callback; second empty GO returns Command/UnloadMenu, menu None/page0, no items. [GOIDLE first](pilot/captures/GOIDLE-019-probe.png), [second gate](pilot/captures/GOIDLE-024-continuation.png), [unloaded](pilot/captures/GOIDLE-026-continuation.png), [GOIDLE.dwg](pilot/drawings/GOIDLE.dwg), [physical Return](pilot/captures/RETURN-014-probe.png), [RETURN.dwg](pilot/drawings/RETURN.dwg). |
| 4 `app_mouse_route_menu_cancel_keeps_panel` | MenuFile prompt with loaded panel; Cancel → Continue/Command/*Cancel*, menu stays Some and page0. Distinct from preceding unload test. [CMENU filename gate](controls/captures/CMENU-013-probe.png), [Cancel post](controls/captures/CMENU-020-probe.png), [CMENU.dwg](controls/drawings/CMENU.dwg). |
| 4 `menu_return_empty_active_prompts_match_native` | Explicit empty Return in LINE-first/CIRCLE-radius → Invalid/Command/no items; LINE-next after 2,3 → Command/no segment; empty ERASE → Command retaining LINE (2,3)→(4,5). Each is exercised through real GO route with empty App.input too. [GOFIRST post](go/captures/GOFIRST-017-probe.png), [GOFIRST.dwg](go/drawings/GOFIRST.dwg), [GONEXT post](go/captures/GONEXT-019-probe.png), [GONEXT.dwg](go/drawings/GONEXT.dwg), [GORADIUS post](go/captures/GORADIUS-019-probe.png), [GORADIUS.dwg](go/drawings/GORADIUS.dwg), [GOSELECT post](go/captures/GOSELECT-025-probe.png), [GOSELECT.dwg](go/drawings/GOSELECT.dwg). For GOFIRST/GONEXT, the linked later-history case pins unknown coordinates at Command. For GORADIUS, following `1.25` is unknown at Command ([continuation](go/captures/GORADIUS-021-continuation.png)), proving scalar state was lost. GOSELECT asserts seed retention only. |
| 4 `menu_return_known_history_survives_unknowns_and_prompt_answers` | Successful LINE command; empty first/next ends it; unknown 2,3/4,5 do not replace history; next empty reopens LINE-first. [GOFIRST sequence23](go/captures/GOFIRST-023-continuation.png), [GONEXT sequence23](go/captures/GONEXT-023-continuation.png). POINT,8,7 then empty GO reopens Rust `POINT: point`, saves only original POINT (8,7) regardless prior cursor location: [GOHIST post](go/captures/GOHIST-021-probe.png), [second location](go/captures/GOHIST-029-continuation.png), [GOHIST.dwg](go/drawings/GOHIST.dwg). |
| 4 `app_mouse_route_go_submits_pending_point_and_selection_once` | LINE-first App.input `2,3`; GO takes buffer → LINE-next anchor (2,3), input empty, one callback; 4,5 then empty completes exact LINE (2,3)→(4,5), no pointer point. [GOBUFPNT gate](buffers/captures/GOBUFPNT-027-probe.png), [post](buffers/captures/GOBUFPNT-029-probe.png), [GOBUFPNT.dwg](buffers/drawings/GOBUFPNT.dwg). Real drawing pick of same seed LINE as CSELECT accumulates `1`; GO and keyboard helper each erase exactly that LINE (unchanged coordinates/layer), empty buffer/Command, status `1 selected, 1 found.`. GO then POINT,8,7 gives Erased LINE followed by POINT (8,7). [GOBUFSEL post](buffers/captures/GOBUFSEL-036-probe.png), [GOBUFSEL.dwg](buffers/drawings/GOBUFSEL.dwg), [Return control](buffers/captures/CSELBASE-033-probe.png), [CSELBASE.dwg](buffers/drawings/CSELBASE.dwg). |
| 4 `menu_return_invalid_repeat_columns_retains_marker` | After Task3 CREPEAT setup, submit_return ENDREP then REPEAT → Invalid/Command, not new group; POINT,6,5 succeeds; ENDREP asks columns again. [invalid answer](cancel/captures/CREPEAT-025-continuation.png), [second ENDREP](cancel/captures/CREPEAT-031-continuation.png), [CREPEAT.dxf](cancel/drawings/CREPEAT.dxf). Decoder boundary remains `REPEAT at offset 0x202 has no ENDREP`; no codec change or unmatched-DWG equality assertion. |
| 4 `app_mouse_route_return_and_go_preserve_load_resolution` | Both shared helper callers at LOAD resolve/register available SHP before submit_return; one callback, buffer empty; unavailable/empty requested library gives existing resolution error and no editor submission. Existing LOAD policy regression, **no native LOAD/GO claim**. Macro LOAD bypass remains unchanged. |
| 4 `app_mouse_route_go_header_only_and_next_still_wraps` | Parsed ACAD header action b";": only page0 exposes GO. Pages1/2 top blanks consumed, preserve page/buffer/state, no callbacks. NEXT wraps last→0 as before. Replace old GO page-reset assertion; do not manufacture a page2 GO hit through advance_menu_page. Rust hit regression only; true blank native mapping unverified. |

Oracle fixture test names are `menu_controls_idle_native_fixtures`,
`menu_controls_pending_native_fixtures`, `menu_controls_cancel_native_fixtures`
(Task 3), and `menu_controls_go_native_fixtures` (Task 4). They cover the table's
native-backed rows using the exact staged `.dwg` paths; CREPEAT uses DXF text and
prompt artifacts only. App tests exercise `handle_left_click` and actual parsed
entries, not just enum/helper calls. Editor tests use the `menu_control` filter;
Return tests use `menu_return`; app tests use `app_mouse_route`. Future validation:
focused filters plus `cargo test -p acad-cmd -p acad-app`, oracle fixture test,
`cargo fmt --all --check`, workspace all-target Clippy with `-D warnings`, and
`cd formal && lake build`. Tasks 3/4 must record genuine red then green output.

## Finite contract and unsupported boundaries

`formal/AutoCAD/MenuControls.lean` records a partial lookup over explicit observed
before/event/after states. Prompts and pending values are structured; exact typed
and exported coordinates are symbolic decimal strings, with no floating-point or
projection arithmetic. Buffer text represents literal paced input or a symbolic selected-object id 1;
it does not recover native selection storage or prove Rust App.input. Examples check controls, continuation, Cancel retention, pages,
MENU unload versus Cancel, GO buffer submission/history and CREPEAT prompt/marker
retention. `none` means uncertified, not invalid/no-op. Rust tests must establish
refinement separately; `lake build` alone proves neither Rust routing nor native
universality. The older HelpFiles model projects MENU only to Command and has no
panel retention/unload fields; the focused model adds those observed distinctions.

Mouse Snap/Ortho parity remains a known gap: **leave `submit_mouse_point` unchanged**.
The finite [M00](controls/drawings/M00.decoded.txt),
[M10](controls/drawings/M10.decoded.txt), [M01](controls/drawings/M01.decoded.txt),
[M11](controls/drawings/M11.decoded.txt) endpoint matrix is respectively
(6.1274509803922,4.17647058823534),
(6.000000000000041,4.00000000000004),
(6.1274509803922,3.00000000000005),
(6.000000000000041,3.00000000000005) from anchor (2,3).
[MASYM](constraints/drawings/MASYM.decoded.txt) supports finite positive upper-half
and locked-coordinate/order witnesses; [MAXIS](axis/drawings/MAXIS.decoded.txt)
and [axis-analysis](axis-analysis.tsv) establish saved V,H,V for three matched pairs,
not an isolated third preview. Exact ties, origin, negative rounding, arbitrary
spacing/view/metrics/history and total ordering remain gated. These values are
retained research evidence, **not promised passing Rust projection tests** in this
slice. No general mouse rule or universal control fidelity appears in Lean.

CREPEAT has native DXF/prompt support but no complete decoded Header/Items or
completed/restarted group. True native blank-panel mapping, M1–M3, arbitrary
command-history semantics, and buffer toggles remain unsupported beyond the
explicit Rust policies above. GO's observable Return behavior is resolved for the
listed states; literal bytes/internal native mechanism are not recovered.
