# D9 ARC choice pilot: retain the Rust dispatcher

Decision: reject the declarative center-choice extraction for this family.
D9 permits a measured rejection when a pilot adds complexity. This is evidence
for that decision, not a claim that an interaction interpreter was implemented.
No production code, command tokens, prompts, translations, or geometry changed.

## Inventory and bounded experiment

Baseline: `34fe7fe213165d2108a4f9b57aed3321fca426c9`.
Discovery used `search_graph(submit_arc|submit_circle)` and the ARC source snippet;
the graph reads the live checkout, so scoped snapshot reads confirmed the source.
The [D9 acceptance](superpowers/plans/2026-10-06-data-driven-agent-loops.md),
[message contract](message-catalog-contract.md), and
[command discovery](command-discovery.md) constrain this experiment.

`submit_arc` contains eight context/token mappings across four contexts:

| Context | Tokens | Existing fallback |
| --- | --- | --- |
| `ArcStart` | C | Empty input continues the previous tangent; otherwise parse a point |
| `ArcMiddle` | C, E | Parse the middle point and reject coincident points |
| `ArcCenterChoice` | A, L | Parse a direction point and build center/end geometry |
| `ArcEndChoice` | R, A, D | Exact English error and retry |

Those mappings use nine token literal occurrences in executable choice handling:
C repeats across two contexts, and A is tested twice in `ArcCenterChoice`.
`ArcEndChoice` already has a closed three-way match. CIRCLE has just three
mappings across two contexts. Presentation prompts contain option tokens, but
are not a second parser. ARC prompts remain outside the migrated message slice.

The smallest useful candidate was `ArcCenterChoice`: two choices and one repeated
A test. The complete [prototype patch](experiments/d9-arc-center-choice-prototype.patch)
is retained as non-active experimental evidence. Its SHA-256 is
`5ae92b8a610fcc613c220cd998ed508e9a031f897b19a2f07d5952e98c28c76b`.
It applies to the baseline and the final candidate with `git apply --check`.

The prototype moves syntax to a two-row JSON resource, validates it at build
time, emits a static typed lookup, and migrates the actual dispatcher consumer.
It rejects unknown/missing/duplicate fields, unsupported schema/context/action,
non-uppercase ASCII tokens, duplicate context/token or action, blank/control
provenance, incomplete action coverage, and resource overflow. Its closed Rust
`CenterChoice` match has no wildcard: unknown runtime actions cannot execute.
Unrecognized input still takes the existing point parser. It introduces no
runtime resource loader, arbitrary action, expression evaluator, or transition
interpreter. English prompts and errors are unchanged.

## Baseline/prototype comparison

| Measure | Baseline | Prototype |
| --- | --- | --- |
| Center-choice syntax literals in dispatcher | A twice, L once | None; A/L each once in resource |
| Repeated token comparison removed | — | One (the second A test) |
| Center-choice dispatcher file line delta | — | Zero (including import) |
| New production support | None | 72 build-helper lines, 17 JSON lines, 13 module lines, 4 build/module integration lines |
| Net production line delta | — | +106 lines across six files |
| Additional schema test | None | 41 lines |
| Applicable existing geometry/transitions | Rust | Same Rust |

The center-choice body becomes one line shorter, while its new import offsets
that reduction. The extraction removes actual duplicate syntax, but leaves the
typed transition branching and all geometry responsibilities in place. The new
schema and build/runtime seam are much larger than the one repeated comparison
removed. The closed endpoint match does not contain a comparable repeated check.
Expanding scope solely to amortize the mechanism would exceed this pilot.

Retain the compact Rust branch. Reconsider extraction only when an inventory
shows a substantially repeated rule with enough consumers to justify the shared
schema. This decision does not reject existing command/message/profile catalogs,
and does not establish parity for hypothetical future declarative families.

## Behavior evidence and checks

The same three independent `arc_choice_parity` tests passed on baseline,
prototype, and restored baseline. They exercise ASCII A/L case variants and
submission whitespace, exact English prompts, En/Uk prompt access, absolute and
relative point fallback, unknown/context-inappropriate/non-ASCII tokens, numeric
fallback errors, empty physical Return, invalid geometry/number retry without
mutation, cancellation, undo, and prior curve continuation. Geometry comparisons
use the canonical choice of the same mode; chord floating-point rounding is not
assumed identical to angle geometry.

All ten existing `arc_options` tests also passed on both versions, covering the
other ARC alternatives, signed/major sweeps, mouse input, finite geometry,
continuation/history, invalid intermediate points, undo, and DWG round trips.
The prototype's strict-schema test passed its malformed-input cases.

Commands used with separate `CARGO_TARGET_DIR=../target-d9`:

```sh
cargo test -p acad-cmd --test arc_choice_parity --test arc_options
# Baseline and restored final: 13 passed.
cargo test -p acad-cmd --test arc_choice_parity --test arc_options --test arc_choice_catalog
# Applied prototype only: 14 passed.
cargo clippy -p acad-cmd --all-targets -- -D warnings
# Prototype and restored final: passed.
cargo fmt --all -- --check
git diff --check
git apply --check docs/experiments/d9-arc-center-choice-prototype.patch
```

To reproduce the experiment, apply the patch in a disposable checkout of this
candidate, run the three test targets and Clippy, then discard that checkout.
The final candidate retains only this decision, the non-active patch, and the
independent parity tests. The production source matches the stated baseline.
Native/browser visual checks and integrated profile switching belong to D10;
this rejected parser experiment does not add UI or profile state and makes no
new visual or full-app localization claim.
