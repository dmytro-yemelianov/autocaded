# Phase 2 scoped integration review — pass 1

Exact staged snapshot: `RUN/review-phase2`, baseline `625176171ae0`; independently matched its staged diff and `RUN/phase2-merged.patch` to SHA256 **`59f5f18b7b23fee95a8e731c3ddcd07bc67471df4d6d1dda377d56e40dc50428`**. No unstaged changes were present during inspection.

Scope only: L3 merge reconciliation, root render budget changes, GUI harness extension. This is not whole-B3 approval. B3a named-WBLOCK resource-context P2 and B3b remain separately open.

## Verdicts

- **L3 merged signed persistence: PASS.** DXF shared group preflight and new UnsupportedGroup error coexist with signed table validation. Before output allocation, both checked writers reject OFF slot 0, absent slots, indices above 127, zero color and colors above 127; table validation retains positive colors 0..254 and undefined sentinel 255. Signed decoding uses checked negation, accepts only magnitudes 1..127 in slots 1..127, and never conflates 255 with OFF. Repeated DXF table slots replace prior visibility state. DWG header encoding writes the original signed value, and header passthrough is overwritten for all color slots. No L3 behavior was lost in B3a reconciliation.
- **Root rendering limit: NEEDS FIXES, P2 below.** Normal-owner and public geometry-only entry points now preflight before expanding repeat cells; rejected owners retain ordered LOAD effects without multiplying them by lattice cells. Hidden INSERT exhaustion explicitly fails resource context. Subsequent owners stop when context becomes unresolved, while selected highlights clear the earlier partial result. These paths otherwise match the intended conservative contract.
- **GUI harness source: PASS.** The new flow checks negative slot 1 in raw DWG and DXF, reopened OFF/color/clean state, UNDO returning visible geometry and a dirty changed saved baseline, and saves that restored baseline before constructing color-0/OFF failure. `LAYER COLOR 0` correctly changes current layer 1. Both UNDOs restore the clean baseline. Failure checks preserve old destination bytes, attachment and a running GUI; missing target remains absent. Positive nested save/reopen compares serialized drawing and selectable counts, then reopens the earlier flat baseline. These assertions use the attached GUI API/MCP path; no desktop-key-event or executed-by-reviewer claim is made.

## P2 — zero-sized outer repeat bypasses the work estimate

`crates/acad-render/src/selection_policy.rs:66–77` computes `generated = copies * cells`. If a public/native model has an outer Repeat with zero rows or columns, recursive copies becomes zero and each expensive inner Repeat/leaf is charged only one record through `max(1)`. Both actual walkers compute their child base before entering the outer cell loop (`resource_walker.rs:27–31`, `flatten.rs:115–121`). Therefore outer rows=0 containing inner rows=columns=u16::MAX passes the cheap estimate and attempts the huge inner expansion anyway.

The codecs refuse zero dimensions, which limits file exposure; it does not protect the public `flatten_entity` or direct native Drawing/Repeat APIs whose safety guard is being added. Reject zero-sized repeats in the preflight or charge base work with nonzero effective copies/cells. Add the zero-outer/huge-inner case to full rendering and `flatten_entity`; both must finish with no inner allocation and, for full rendering, a budget diagnostic while preserving ordered LOAD context.

## Test evidence assessed

The four new expansion-budget tests meaningfully exercise huge flat/nested positive lattices, continued rendering of a following point, ordered font LOAD after a rejected owner, excessive wrapper depth and cyclic INSERT context. They do not cover zero-sized ancestor multiplication. Existing L3 lifecycle tests still cover AC1.2/AC1.40/DXF positive persistence and failure atomicity. Reviewer inspected source/assertions and exact hashes, without rerunning builds/tests or treating worker/root logs as independent proof.

## Narrow repair review — pass 2

Repaired exact staged diff and exported patch both independently hash to **`f45b082236f7876fdca4749786e2ec9d79c81486e17ad46c1e471fc2db899244`**; no unstaged changes present. **Root render guard P2 CLOSED; scoped render source verdict PASS.** `repeat_cost` now rejects either zero dimension before computing or propagating copies. That prevents child-base allocation in both full-owner and public geometry-only routes. The new regression constructs both zero-row and zero-column outer patterns with a u16::MAX-by-u16::MAX inner pattern, asserts the following point still renders, checks the budget diagnostic, and checks empty geometry-only output. Resource traversal remains stored-only on rejection.

**Harness delta PASS.** After reopening the flat baseline, `MENU` followed by blank input unloads the new session menu before 160-pixel captures. This restores the existing small-canvas test precondition without changing saved drawing state or weakening assertions. Actual GUI execution remains the coordinator's verification, not a reviewer claim.

L3 approval remains unchanged. B3a named-WBLOCK P2 and B3b are still outside this scoped approval; no whole-B3 completion is asserted.
