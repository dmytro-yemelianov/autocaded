# Data-driven evolution: bounded agent loops

Date: 2026-10-06. Status: complete; all D0–D10 gates verified on 2026-10-07.
Publication followed on explicit user instruction:
[v0.5.0](https://github.com/dmytro-yemelianov/autocaded/releases/tag/v0.5.0)
was released and deployed on 2026-10-07. See the
[publication evidence](../../data-driven-validation.md#publication-follow-up-v050).
Scope: [data-driven evolution](../../data-driven-evolution.md).
Queue: [progress ledger](2026-10-06-data-driven-progress.json).

## Outcome and constraints

Keep one Rust drafting engine. Extract suitable facts and repetitive declarative
rules into structured data with stable identities, provenance, schema validation,
and native/browser consumers. Preserve Faithful's recovered English output and
file compatibility. Modernized remains an alternative evolution of early CAD.
Geometry algorithms, undo, codecs, and complex behavior remain Rust.

Execution started with a local 61-command catalog candidate that had not yet
received independent review. D0 reviewed it before the remaining migrations. Its
prior 667 command/application/wasm tests and native/wasm Clippy were baseline
evidence, not approval of later changes. Preserve the dirty foundation and unrelated
`memory/`. NetRust is a read-only architectural reference, not an edit target.

## Models and allocation

These assignments are workload choices, not guaranteed performance or savings.
They use models exposed by this session. Official guidance places Luna on focused
work, Sol on balanced complex coding, and Astra on demanding reasoning:
[OpenAI model guidance](https://developers.openai.com/api/docs/guides/latest-model).
Do not assume that more reasoning effort always produces a cheaper successful task.

| Role | Model | Effort | Use |
| --- | --- | --- | --- |
| Coordinator, in a new execution session | `gpt-6.1-sol` | medium | Contract, scheduling, integration, progress, completion gates |
| Mechanical worker | `gpt-6-luna` | low | Enumerate/extract bounded tables with explicit source and expected output |
| Default implementation worker | `gpt-6.1-sol` | medium | Catalog/schema work and migration of existing consumers |
| Complex implementation worker | `gpt-6.1-sol` | high | Message parameters, cross-platform glyphs, state-preserving profiles |
| Independent routine reviewer | `gpt-6.1-sol` | medium | Exact candidate diff, contract, and focused regressions |
| Architecture/evidence reviewer | `gpt-6-astra` | high | Initial message contract; ambiguous semantics or failed complex reviews |

This document does not change the current coordinator's model. Use explicit
`model`/`reasoning_effort` for future workers with `fork_turns="none"` and a bounded
task brief. Do not inherit the complete conversation. If a requested model is
unavailable, record the substitution and use Sol medium; do not silently run a
fleet on the strongest model.

Default: one implementer, then one independent reviewer. Allow two subordinate
agents at once only on disjoint files/contracts, at most three active agents
including the coordinator. Overlap review of one candidate with the next
independent implementation when useful. Do not parallelize shared message APIs,
dispatcher changes, profile application, or integration. Reviewers do not edit code.

## Queue and acceptance

Split each milestone into independently reviewable slices before assigning it.
Each slice changes one catalog/consumer family; aim for at most five production
files, excluding generated data and directly relevant tests/docs. This is a
planning heuristic, not an excuse to omit necessary changes.

| ID | Depends on | Worker / reviewer | Deliverable and acceptance |
| --- | --- | --- | --- |
| D0 | — | Sol medium / fresh Sol medium | Review the existing catalog. Every pre-extraction spelling retained; distinct identities preserved; malformed catalogs rejected; generated enum exhaustive; native and wasm metadata agree. Repair findings before integration. |
| D1 | D0 | Sol high / Astra high | Message contract and inventory: stable keys, named typed arguments, plural/number formatting policy, exact Faithful fallback, locale ownership and propagation. Classify UI text vs command tokens vs user data. No string keys that encode an English sentence. |
| D2 | D1 | Sol medium / Sol medium | Shared message catalog, validated loader/build generation and formatter. Reject duplicate keys and parameter/type mismatches; deterministic English fallback; malformed locale input produces a defined error. No external translation service at runtime. |
| D3 | D0 | Sol medium / Sol medium | Command discovery metadata: category, help reference, summary key and actual availability. Historical/peripheral/extension distinctions evidenced. Native and browser share the inventory; no invented feature-completeness claims. |
| D4 | D2, D3 | Sol high / Sol high | First end-to-end English slice: LINE/CIRCLE prompts, unknown-command error, and file-action labels. Existing Faithful wording, parameters, cancel/retry and scripts remain intact. Existing consumers use the catalog; remove superseded literals. Expand by command family after this slice passes. |
| D5 | D1 | Sol high / Sol high | Establish and implement the UI glyph path for localized text in native and browser presentation. Verify Ukrainian glyph coverage, shaping/measurement/clipping as applicable, and redistributable font provenance. Keep drawing TEXT/font semantics separate. Do not expose a locale whose UI cannot render. |
| D6 | D4, D5 | Sol medium / Sol medium | Ukrainian vertical slice using stable keys, complete declared slice coverage, named-argument checks and visible locale switching. Native/browser text visibly renders; command tokens/scripts/file bytes are locale-independent; switching does not edit the drawing. Do not label partial coverage as full-app i18n. |
| D7 | D0 | Luna low / Sol medium | Extract one bounded static-fact family, initially hatch descriptions. Exact inventory/provenance and existing behavior preserved; old table removed and consumers migrated. Unit metadata and bindings are separate later slices. Promote to Sol before redesigning behavior. |
| D8 | D1, D3 | Sol high / Sol high | Validated profiles for evidenced defaults and deliberate Modernized policies, applied through one engine path. Each exposed field affects behavior. Switch without losing document/view/undo/pending command; Faithful defaults stay exact. No new file bits or undocumented geometry limits. |
| D9 | D4, D8 | Sol high / Sol high; Astra for ambiguity | One repetitive prompt/choice/validation pilot selected from inventory. Typed Rust actions, bounded transitions, explicit retry/cancel. Demonstrate reduced duplication and parity, or reject the interpreter approach with evidence if the pilot adds complexity. |
| D10 | D0–D9 | Coordinator / fresh Sol high | Integrated validation, evidence/coverage docs, browser and native smoke flows. Report deferred capabilities explicitly. Prepare a release only within the user's publication authorization. |

D5 can discover that localization needs a larger renderer change. In that case,
produce a concrete decision and smaller implementation slices; D6 remains pending.
Discovery/profile changes must respect the retro interface: no toolbar/panel
redesign is implicitly authorized by this queue.

## Per-slice loop

1. **Contract.** Coordinator records ID, dependencies, exact source snapshot,
   allowed files, acceptance cases, evidence, model/effort and required checks.
   Resolve architecture once in D1; do not ask every worker to redesign it.
2. **Candidate.** Worker discovers via the knowledge graph first, using scoped
   source reads when incomplete. Extract facts, migrate real consumers, remove
   the former source, and run the smallest meaningful checks.
3. **Independent review.** Fresh reviewer receives contract, exact diff/hash,
   source evidence and check summaries. Review duplication, data fidelity,
   Faithful parity, localization parameters, and missing behavior cases. Return
   concrete findings with severity, location and a reproducer; avoid style churn.
4. **Repair.** Reuse the implementer with only findings and relevant artifacts.
   Invalidate affected checks and the previous review. Re-review the changed diff
   and interactions with already accepted code.
5. **Integrate.** Coordinator applies the reviewed candidate serially, resolves
   conflicts explicitly and validates the integrated snapshot. Approval applies
   to the exact source hash; code changes invalidate it.
6. **Advance.** Mark verified only after required checks, independent review and
   root integration. Promote tasks whose dependencies are verified.

Allow at most two repair passes per slice. Diagnose deterministic tool/build
failures without asking a model to keep regenerating code. After a repeated
reasoning failure, escalate once (Luna → Sol medium; Sol medium → Sol high;
ambiguous contract → Astra high). If still unresolved, split the task or record a
specific blocker and advance independent work. Never call deferred work complete.

## Routing calibration after D0–D4

D0 and D3 passed routine Sol medium review. D2 needed one bounded repair for
formatter argument names colliding with generated internal bindings. D4 was
promoted to Sol high for implementation and review: status replacement spans
multiple Session modules, and typed diagnostics must not survive a later status
with identical English text. This is a state-lifetime risk, not a mechanical
string extraction. D1's independent Astra contract review passed without repair.
Continue using medium for bounded translations and high for profiles/state rules.
Runtime token counts are unavailable; no cost or savings percentage is claimed.

## Token discipline

- Scripts perform extraction, regeneration, coverage counts, hashes and schema
  checks. Models decide contracts and review behavior, rather than retype tables.
- Initial task brief target: at most 1,500 tokens plus relevant source snippets.
  Provide artifact paths, not whole ledgers or logs. Routine handoff target: 400
  tokens; review findings target: 700 tokens. These are advisory output/context
  targets, not enforced total-token budgets or grounds for truncating evidence.
- Search narrowly, summarize graph nodes, and fetch individual functions. Read
  the current ledger status, not its accumulated event history. Keep successful
  logs on disk and show only command, exit status, count and artifact path.
- Reuse a worker within its domain; start a fresh brief when the domain or model
  changes. Keep independent review free of the implementer's reasoning history.
- Batch disjoint mechanical edits into one slice. Do not launch an agent for a
  tiny deterministic rename or rerun passing suites without a relevant change.
- Record actual input/output/reasoning/cached-token usage only when the runtime
  exposes it; otherwise record unavailable. Count attempts and elapsed time.
  Compare accepted slices and review misses, not tokens alone. Calibrate routing
  after D0–D4; avoid unsupported percentage/cost-saving claims.
- No `max`/`ultra` effort, multiple competing implementations, or reviewer fleets
  by default. A specific unresolved problem must justify extra inference.

## Isolation, prompts, and handoff

Before launching workers, preserve the actual dirty candidate in an isolated
snapshot with an explicit file allowlist, base commit and content hashes. A
checkout of HEAD alone omits the current foundation. Exclude `memory/`, generated
builds and credentials. Private snapshot commits are local implementation artifacts.
Never reset/stash the user workspace. Workers have separate worktrees/snapshots
and target directories; reviewers see a read-only exact candidate. The coordinator
alone integrates into the user workspace.

Worker brief template:

```text
Task ID / snapshot / model / effort:
Objective and dependencies:
Allowed files and canonical source:
Required acceptance cases / retained evidence:
Checks and artifact directory:
Do not redesign adjacent systems; prefer graph discovery.
Return: snapshot hash, patch, acceptance results, exact checks, gaps.
Stop on contract ambiguity; identify the missing fact rather than invent it.
```

Reviewer brief template:

```text
Task ID / exact candidate hash:
Contract and baseline evidence:
Diff and focused check artifacts:
Independently check correctness, single-source consumption and parity.
Return pass or concrete findings; state unverified areas. Do not edit files.
```

Handoff fields: task ID, base/candidate hashes, changed files, removed duplication,
acceptance evidence, check commands/results/log paths, findings, limitations.
Record model/effort and repair count. Worker "done" means ready for review.

## Validation and progress

For a slice, use focused tests covering genuine contracts: malformed data,
alias/identity fidelity, fallback and parameter handling, retry/cancel/undo,
locale-independent scripts and files, or state preservation as relevant. Do not
write assertions that merely repeat the new catalog as their expected values.
Run scoped Clippy; wasm Clippy when touching shared/browser bindings; browser
regressions and actual native/browser interactions when presentation changes.

At D10 run the existing CI-equivalent gate once on the integrated source:
`cargo fmt --all --check`, `git diff --check`, workspace Clippy, wasm Clippy,
`cargo test --workspace --exclude acad-oracle`, Node browser regressions, Lean
build, and native/wasm release builds. Validate actual draw → undo → save → reopen
and geometry-only exports in both modes; verify localized text visually. Strict
oracle comparisons are required where a recovered behavior is changed and the
environment is available; record unavailable/skip distinctly from pass. Do not
weaken fixtures, tolerances, or golden output to make extraction pass.

Use the existing ledger tool with the new ledger; do not alter the old run:

```sh
python3 tools/agentic_progress.py --ledger docs/superpowers/plans/2026-10-06-data-driven-progress.json status
python3 tools/agentic_progress.py --ledger docs/superpowers/plans/2026-10-06-data-driven-progress.json validate
```

The tool enforces dependency/state/check/review/integration gates and repair
counts. Model assignments, context targets, exact-hash review and reviewer
concurrency are coordinator policies, not automatically enforced by that tool.
Planning does not launch agents, change models, create an automation, or publish.
