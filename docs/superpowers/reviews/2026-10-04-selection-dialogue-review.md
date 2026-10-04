# S2 exact candidate source review — pass 1

Verdict: **PASS (source review); no blocking correctness finding in the scoped candidate.** Reviewed patch SHA256 `bfad5b3021b285f4b7d594777603756a32f79cebe7b927caa88a07b86cc9689f`, `RUN/s2.patch`, byte-identical to the staged binary diff in `RUN/selection-dialogue`. Baseline `2148439c6bd4ebab8bc466536013c6c3f0827e94`; no unstaged changes at review completion. This verdict does not cover subsequent B3/L2 integration edits or claim actual desktop GUI execution.

## Independent source findings

- `acad-cmd/src/selection/dialogue.rs` stores the command continuation and sorted deduplicated canonical owner IDs in editor state. Picks do not execute the command; misses preserve state. W/WINDOW retains previous picks, asks two corners, then collects until Return. Corner validation occurs before replacing stored state; empty windows and malformed points remain retryable.
- Typed nonempty ID/ALL/LAST input replaces the pending collection rather than unioning it. `finish_selection` forwards the validated set to the existing command-specific handler with its captured parameters, preserving MOVE/COPY displacement and BLOCK/WBLOCK/HATCH context. Immediate cardinality/geometry errors restore the pending collection. Errors at later domain prompts retain those existing domain handlers' behavior; this change does not pretend to make every edit atomic or historically identical.
- HATCH keeps its previous pattern/scale/angle input and window completion at the second corner. Its former dedicated window states are replaced by the shared selector with explicit HATCH completion/prompt branches. Existing retained fixture inputs were not modified to accommodate a new Return.
- LIST now enters a selector and reports only requested owners while retaining their global IDs. Report summary and details use the same selected-ID set and existing byte/object limits. Empty initial LIST Return cancels; explicit hidden IDs remain allowed, while picks/windows reuse the existing visibility-aware geometry functions. The report does not alter drawing/undo state.
- App world points and pixel clicks accumulate through the same editor method. World hits use documented zero tolerance; pixel hits retain viewport tolerance. Window corner mouse inputs bypass SNAP/ORTHO. Session effect handling mirrors retained IDs after retry errors, and cancel clears the editor continuation and command input. Existing report storage/paging remains independent of the completed LIST prompt.

No P0/P1/P2 issue was identified in these paths. No external or original runtime was used; review was read-only outside the review notes.

## Validation assessed, not rubber-stamped

Read the newly added command/app/MCP tests and surrounding implementation. The tests cover mixed visible/hidden geometry, duplicate picks, multiple windows, partial-window exclusion, typed replacement, retry, cancellation with undo checks, all shared command continuations, HATCH geometry rejection, selected reports, clean-document paging and MCP point collection. App tests call the real API/click/session/frame routes and check rendered highlight pixels. They do **not** constitute actual native-window keyboard/mouse execution. I did not rerun builds/tests or write implementation files; root owns combined execution.

Remaining integration checks: actual keyboard Return and screen-menu GO with mirrored selection text, Escape during each window phase, LIST report paging/close followed by the next command, and revalidation after B3 modifies Repeat metadata. Root should retain complete original HATCH fixture checks without changing inputs/tolerances. Existing conservative INSERT bounds/origin picking, nested-group save limitations, historical selection-unit differences and native report formatting limits remain documented prior gaps, not new S2 completion claims.

AGENT_REVIEW task:R3-S2 verdict:pass findings:no blocking scoped source findings evidence: exact staged SHA256 above; shared selector/dispatch/report/app source and meaningful new test assertions independently read next: root integrated tests and actual GUI; new combined changes need exact review notes:this file.
