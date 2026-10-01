# Remaining command recovery: subagent execution roadmap

Date: 2026-10-01
Status: execution authorized; menu integrated locally through `55f7c81`; DIM and HATCH evidence/implementation pending
Baseline: local `main` at `1f38904`, with screen-menu rendering and fresh drawing defaults merged

**Goal:** Close the next three bounded gaps with native evidence, Lean behavioral contracts, Rust implementation, and differential tests: menu controls, DIM geometry, and a first additional HATCH pattern.

**Authority:** [AutoCAD 1.4 design](../specs/2026-09-28-autocad-14-rust-design.md) and [current handover](../../HANDOVER-2026-09-30.md). This roadmap does not claim that completing these slices completes the overall AutoCAD rebuild.

## Plans and order

| Slice | Deliverable | Technical plan |
|---|---|---|
| 1. Menu | Recover and implement `^Snap`, `^Ortho`, `^Cancel`, and the observed GO behavior; distinguish flag changes from mouse-point constraints | [Menu controls](2026-10-01-menu-controls-recovery.md) |
| 2. DIM | Implement recovered style/history state and primitive geometry for baseline/continue, arrow size, and text orientation/placement | [DIM geometry](2026-10-01-dim-geometry-recovery.md) |
| 3. HATCH | Establish the native catalogue and implement the first evidence-backed pattern beyond LINE, with phase/scale/angle and boundary checks | [HATCH pattern recovery](2026-10-01-hatch-pattern-recovery.md) |

Read only the current slice's plan and its referenced evidence when executing it. Old completed menu and DIM-recognition tasks must not be redispatched.

## Model allocation

Use explicit model overrides and `fork_turns="none"` for every new worker. Give it a task brief, worktree, allowed paths, evidence inputs, output report path, and the no-subagents rule. These choices concern reasoning requirements; they are not quoted price or token-usage estimates.

| Role | Model / reasoning | Selection rule |
|---|---|---|
| Native recovery and geometry implementation | `gpt-6.1-sol` / high | Default for uncertain native semantics, DIM anchors, pattern phase/clipping, and numerical comparisons |
| Bounded integration and test harness changes | `gpt-6.1-sol` / medium | Use after the evidence and interfaces are concrete; avoid sending unresolved geometry to a cheaper model |
| Mechanical documentation/fixture transcription | `gpt-6-luna` / low | Only when the exact content and schema are supplied; coordinator batches related edits |
| Task review: spec compliance plus code quality | `gpt-6.1-sol` / medium or high | Medium for small clear diffs, high for geometry/state-machine changes; reviewer is independent of implementer |
| Scoped fix re-review | `gpt-6.1-sol` / medium | Review the findings and fix diff, without repeating the whole branch review |
| Final whole-branch review | `gpt-6-astra` / high | One review per independently completed slice, as required by the selected SDD workflow |
| Escalation | `gpt-6-astra` / high | Fresh agent for persistent native ambiguity, a broken contract, or exhausted ordinary fix rounds; include what already failed |

Do not route ordinary docs, fixture copying, or routine command execution to Astra. Do not use Luna for independent semantic review. Most coding work remains on Sol because additional failed turns can erase nominal savings.

## Parallelism and ownership

Four slots are available including the coordinator. The maximum initial recovery wave is:

```text
coordinator
  + menu recovery worker
  + DIM recovery worker
  + HATCH recovery worker
```

Each recovery worker owns a separate worktree, branch, evidence directory, session directory, and disposable floppy copy. No shared interactive session or writable corpus is permitted. Run at most one QEMU guest at a time: `Session` deliberately serializes guests because concurrent guests can starve keyboard/display processing and produce incomplete CGA frames. Its mutex is process-local, so separate agents/test binaries do not share that protection. The coordinator grants one explicit probe lease; other workers prepare input matrices, harness changes, or decode completed artifacts while they wait. Do not remove or bypass the harness lock to manufacture parallelism.

After menu recovery finishes, its slot becomes the menu implementer or reviewer seat. DIM/HATCH preparation and offline analysis can finish independently; actual native probes obey the single-guest lease. Only one designated implementation worker may change source, including recovery-example/harness code. Other workers can read code, prepare input matrices and write their own evidence notes, or run already-built probes with the lease. Never run multiple implementation workers at once: all three slices may change `acad-cmd/src/dispatch.rs`, `input_state.rs`, and `geometry.rs`. Never let a worker spawn its own helpers/reviewer. The coordinator alone dispatches the next worker and integrates commits.

Merge completed slices locally in menu → DIM → HATCH order, following the previously selected local integration workflow. Rebase the next owned branch onto the resulting `main`, resolve overlaps, and rerun covering tests before implementation/review. Do not push: this repository has no configured remote. Leave the unrelated detached worktrees intact.

## Evidence gate

1. Reuse existing proven native facts. Recover only the missing observable behavior, using small asymmetric cases that distinguish competing explanations.
2. Save exact input sequences, before/after CGA captures where prompts matter, generated DWG/DXF data, numeric observations, tool conditions, and a claim-to-artifact mapping. Exclude proprietary floppy/font bytes from tracked artifacts.
3. A recovery task completes with either a demonstrated contract or an explicitly unresolved result and its discriminating next probe. An unresolved behavior is not an implemented feature.
4. Review the recovery deliverable before coding. Write the concrete implementation brief from accepted observations, including exact expected values, state transitions, and test assertions. The native experiment must not be replaced with a guessed expected output.
5. Extend the Lean observable model for facts it can represent, then implement and test the Rust refinement. Lean proves the represented model; it does not recover native facts or automatically prove arbitrary Rust.

Native graphics prompts require `Session::capture_editor()`/CGA inspection. `wait_for_text` is for startup/text menus. Native mouse coordinates use 640×200 space, while saved displayed captures double the Y dimension to 640×400. Retain the existing QEMU `-d nochain` workaround.

## Task lifecycle and token discipline

Use the installed `superpowers:subagent-driven-development` workflow at implementation time, with its `using-git-worktrees` setup and plan-specific SDD ledger.

- [ ] Verify each plan's ledger and commits before dispatch; completed tasks and accepted evidence are reused after compaction.
- [ ] Extract one task brief to a file. Include only requirements, changed interfaces, and the evidence pointers the worker needs.
- [ ] Record base/head commits, tests run, native execution versus skips, findings, and rulings in the ledger.
- [ ] Dispatch one fresh implementer, then one fresh task reviewer covering both spec compliance and quality.
- [ ] Reuse the implementer for early fix rounds; escalate with fresh context only when required. Scoped re-review checks the fix and regressions in that diff.
- [ ] After the slice is complete, run one Astra whole-branch review, one combined fix wave if needed, then one scoped re-review.
- [ ] Update the handover with completed behavior and remaining limits; merge locally only after the required checks and review gates pass.

Reports are files, with a short return message containing status, commits, tests, and concerns. Do not paste the entire session or all previous reports into fresh agents. Batch same-shape mechanical changes. Independent reviewers must see the relevant evidence, not just the implementer's interpretation.

## Verification policy

Before claiming native verification, check prerequisites in the execution worktree:

```sh
command -v qemu-system-i386
test -f 'corpus/raw/Autodesk AutoCAD 1.4 (5.25)/System.img'
```

Oracle tests can exit successfully by skipping; inspect their output and record actual native runs separately from compilation or skips. Worktree setup must make the existing corpus inputs available without modifying them.

Run each new focused native test and nearby native regressions serially. Run `acad-cmd` and `acad-app` covering tests after code changes. At each slice integration, verify:

```sh
cargo test --workspace --exclude acad-oracle -- --test-threads=1
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
```

If formal sources change, run `lake build` from `formal`. Use focused native checks for per-task iterations. Once all accepted slices are integrated, run one full `cargo test --workspace -- --test-threads=1` with QEMU and available corpus inputs. A new failure or changed shared behavior justifies additional relevant runs; routine repetition does not. Serial execution also avoids the recorded temporary-directory test collision.

The final report distinguishes native prompts/state/geometry checks from OS-window/DPI checks. The current menu minimum-size code has no new desktop/DPI runtime verification; do not imply these recovery tasks verify it.

## What remains after this roadmap

Additional HATCH catalogue entries and unprobed boundaries, menu custom-macro limits M1–M3, broader HELP/DBLIST parity, nondegenerate ZOOM extent behavior, and the documented SKETCH digitizer boundary remain separate work. Hardware exclusions and the broader command-by-command fidelity target remain as stated in the design and handover.
