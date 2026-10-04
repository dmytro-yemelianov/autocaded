# Clean-Room Reimplementation Protocol for an Early CAD System
## AutoCAD-86-Inspired Behavioral Research → Formal Specification → Independent Rust Implementation

> **Purpose:** Provide an implementation agent with a legally conservative, technically rigorous process for studying an early AutoCAD-86-era binary as a behavioral reference and building an independent CAD implementation in Rust.
>
> **Important:** This document is an engineering protocol, not legal advice. Copyright, contract, anti-circumvention, and reverse-engineering rules vary by jurisdiction and by the provenance/license of the specific copy being studied. Before distributing reverse-engineering artifacts or relying on a statutory exception, obtain qualified legal review for the relevant jurisdiction.

---

## 1. Project Goal

The goal is **not** to reconstruct AutoCAD source code, translate decompiler output to Rust, or publish a recovered implementation.

The goal is to build an **independent CAD system** whose behavior is informed by:

1. black-box observation of an early CAD implementation,
2. documented input/output behavior,
3. independently written behavioral specifications,
4. formalized semantics where useful,
5. a clean-room Rust implementation derived from those specifications.

The reference binary is treated as an **oracle**, not as source material.

```text
Reference binary
    ↓
Observation / experiments
    ↓
Behavioral specification
    ↓
Formal model (Lean 4 where useful)
    ↓
Independent Rust implementation
    ↓
Conformance tests
```

---

## 2. Non-Goals

The project must **not** attempt to:

- reconstruct the original AutoCAD source tree;
- translate decompiled procedures line-for-line;
- reproduce original internal symbol names;
- preserve original implementation structure merely because it appears in the binary;
- publish decompiler output or disassembly as part of the production repository;
- distribute original AutoCAD binaries;
- patch or redistribute copy-protection bypasses;
- reproduce copyrighted UI artwork, documentation text, splash screens, icons, or other expressive assets;
- claim compatibility beyond what has actually been tested.

The implementation should reproduce **observable semantics**, not proprietary implementation expression.

---

## 3. Legal-Safety Principles

### 3.1 Lawful provenance first

Before reverse-engineering work begins, record:

- exact product/version/build;
- media or archive source;
- acquisition date;
- evidence that the copy was obtained lawfully;
- original license/EULA if available;
- hashes of the original media/binary;
- jurisdiction in which the work is being performed.

Suggested local-only structure:

```text
provenance/
  README.md
  hashes.txt
  acquisition-notes.md
  licenses/
```

Do **not** commit proprietary binaries to a public repository.

### 3.2 Use the narrowest necessary analysis

Prefer, in this order:

1. public documentation;
2. documented file formats;
3. black-box observation;
4. generated test inputs and outputs;
5. debugger/runtime observation;
6. disassembly/decompilation only where genuinely necessary.

Do not inspect more of the program than is needed to answer a defined interoperability or behavioral question.

### 3.3 Separate facts from implementation expression

Good output from reverse engineering:

```text
Given:
    LINE from (0,0) to (10,10)

When:
    MOVE by (+2,+3)

Observed result:
    endpoints become (2,3) and (12,13)

Inferred semantic rule:
    translation applies the same displacement vector to both endpoints.
```

Avoid implementation-derived descriptions such as register-level or function-level pseudocode in public specifications.

### 3.4 Clean-room separation

Where practical:

```text
RE / evidence side
    ↓
behavioral specification
------------------------------
clean-room boundary
------------------------------
formal specification
    ↓
Rust implementation
```

The Rust implementation agent should work from behavioral/specification artifacts, not raw decompiler output.

For a small team, preserve this discipline at the artifact level:

- reverse-engineering notes live outside the production source tree;
- production design documents contain only abstracted behavioral facts;
- commits must not quote or reproduce decompiled code;
- code review must reject suspiciously source-like translations.

---

## 4. Repository Structure

Recommended public repository:

```text
cad-reimplementation/
├─ README.md
├─ LICENSE
├─ SECURITY.md
├─ CONTRIBUTING.md
├─ docs/
│  ├─ architecture/
│  ├─ behavior/
│  ├─ compatibility/
│  ├─ decisions/
│  └─ legal-boundaries.md
├─ spec/
│  ├─ geometry/
│  ├─ entities/
│  ├─ commands/
│  ├─ drawing-db/
│  ├─ serialization/
│  └─ numeric-semantics/
├─ formal/
│  └─ lean/
├─ crates/
│  ├─ geometry/
│  ├─ drawing-model/
│  ├─ command-engine/
│  ├─ parser/
│  ├─ renderer/
│  ├─ cli/
│  └─ conformance/
├─ tests/
│  ├─ fixtures/
│  ├─ behavioral/
│  ├─ property/
│  └─ differential/
└─ tools/
   ├─ fixture-generator/
   ├─ test-runner/
   └─ report-generator/
```

Private/local-only research area:

```text
research-private/
├─ provenance/
├─ binaries/
├─ debugger-notes/
├─ decompiler-projects/
├─ screenshots/
└─ raw-observations/
```

Never publish `research-private/` by default.

---

## 5. Behavioral Specification Format

Each observed feature should have a structured spec.

Example:

```yaml
id: cmd.move.line.basic
feature: MOVE
entity: LINE
preconditions:
  - valid_line
input:
  line:
    start: [0.0, 0.0]
    end: [10.0, 10.0]
  displacement: [2.0, 3.0]
expected:
  line:
    start: [2.0, 3.0]
    end: [12.0, 13.0]
properties:
  - preserves_length
  - preserves_direction
source:
  kind: behavioral_observation
  evidence_id: obs-000143
```

A spec entry should describe:

- preconditions;
- input;
- observable output;
- error behavior;
- numeric tolerances;
- state changes;
- invariants;
- confidence level;
- evidence identifier.

---

## 6. Suggested CAD Scope

Start with a deliberately small early-CAD kernel rather than attempting full AutoCAD compatibility.

### Phase 1 geometry

```text
Point2
Vector2
Line
Ray
Segment
Circle
Arc
AABB
AffineTransform2
```

Core operations:

```text
distance
length
translate
rotate
scale
intersection
projection
bounding_box
contains
closest_point
```

### Phase 2 drawing entities

```text
EntityId
LineEntity
CircleEntity
ArcEntity
PolylineEntity
TextEntity
BlockReference
Layer
```

Use stable immutable IDs where possible.

### Phase 3 drawing database

```text
Drawing
├─ Header / settings
├─ Layers
├─ Blocks
├─ Entities
└─ Metadata
```

Important properties:

- deterministic iteration where practical;
- explicit ownership;
- no hidden global mutable state;
- serializable state;
- stable entity identifiers;
- versioned schema.

### Phase 4 command engine

Initial commands:

```text
LINE
CIRCLE
ARC
ERASE
MOVE
COPY
ROTATE
SCALE
ZOOM
PAN
```

Later:

```text
TRIM
EXTEND
OFFSET
FILLET
CHAMFER
BLOCK
INSERT
EXPLODE
```

Each command should behave as a state transition:

```text
Command + DrawingState → Result<DrawingState, CommandError>
```

---

## 7. State-Transition Architecture

Prefer deterministic command evaluation.

```rust
pub trait Command {
    fn apply(
        &self,
        state: &DrawingState,
    ) -> Result<CommandOutcome, CommandError>;
}
```

Possible outcome representation:

```rust
pub struct CommandOutcome {
    pub next_state: DrawingState,
    pub created: Vec<EntityId>,
    pub modified: Vec<EntityId>,
    pub removed: Vec<EntityId>,
    pub diagnostics: Vec<Diagnostic>,
}
```

A more scalable approach uses events/deltas:

```text
Command
   ↓
Validated operation
   ↓
Drawing events
   ↓
State reducer
```

Example events:

```text
EntityCreated
EntityModified
EntityDeleted
LayerCreated
LayerChanged
BlockCreated
SettingChanged
```

Benefits:

- undo/redo;
- deterministic replay;
- saved simulations;
- testing;
- auditing;
- future collaboration;
- formal reasoning.

---

## 8. Numeric Semantics

Do not assume modern floating-point behavior exactly matches the reference.

Document experimentally:

- coordinate precision;
- rounding;
- comparison tolerance;
- angle normalization;
- intersection edge cases;
- degenerate geometry handling;
- overflow/underflow where relevant;
- serialization precision.

Use an explicit numeric policy where needed:

```rust
pub struct NumericPolicy {
    pub epsilon: f64,
    pub angle_epsilon: f64,
}
```

Do not introduce arbitrary tolerances merely to make tests pass.

---

## 9. Lean 4 Formalization

Lean is not required for every routine.

Use Lean where the property is:

- foundational;
- reusable;
- safety/consistency critical;
- straightforward to state mathematically;
- valuable for preventing entire bug classes.

Good candidates:

```text
translate preserves line length
rotation preserves distance
translation composition
identity transformation
inverse transformation properties
bounding-box containment
command invariants
```

Example command invariant:

```text
MOVE:
    entity count unchanged
    IDs unchanged
    geometry translated
```

The purpose of Lean is to define and verify the **new system's semantics**, not to prove that AutoCAD's implementation was correct.

---

## 10. Rust Design Principles

Recommended baseline:

- Rust stable;
- Cargo workspace;
- minimal dependencies in the geometry kernel;
- `serde` for versioned serialization;
- `thiserror` or equivalent for structured errors;
- `proptest` for property-based testing;
- deterministic data structures when iteration order is observable;
- no unsafe code initially unless justified.

Suggested workspace:

```text
crates/
  cad-math
  cad-geometry
  cad-model
  cad-events
  cad-commands
  cad-serialize
  cad-conformance
  cad-cli
```

Keep geometry/model crates independent of UI and file-format concerns.

---

## 11. Testing Strategy

### 11.1 Unit tests

```text
line length
circle bounds
matrix composition
arc normalization
```

### 11.2 Property-based tests

```text
translate by zero == identity

distance(a,b) == distance(b,a)

rotate(p, 0) == p

move(move(entity, a), b)
    == move(entity, a+b)
```

### 11.3 Behavioral conformance tests

Generated from the behavioral specification.

### 11.4 Differential tests

Where legally and operationally appropriate:

```text
fixture
   ├─ run against reference
   └─ run against new implementation
          ↓
normalized result comparison
```

Only normalized observable output should enter the public test corpus.

---

## 12. Reference-Oracle Workflow

For each behavior:

1. Define one precise question.
2. Construct the smallest possible drawing/input.
3. Run the reference application.
4. Record observable inputs and outputs.
5. Repeat with boundary cases.
6. Infer the smallest behavioral rule consistent with observations.
7. Write a spec entry.
8. Create independent tests.
9. Implement from the spec.
10. Compare results.

Good question:

```text
How does MOVE behave when the displacement is zero?
```

Avoid:

```text
How is MOVE implemented internally?
```

---

## 13. Evidence IDs and Audit Trail

Use IDs:

```text
OBS-000001
OBS-000002
...
```

Public specs may reference an evidence ID while raw evidence remains private.

Maintain ADRs:

```text
docs/decisions/
  ADR-0001-coordinate-type.md
  ADR-0002-command-event-model.md
  ADR-0003-numeric-tolerance.md
```

Each ADR:

```text
Context
Decision
Alternatives
Consequences
Evidence
```

---

## 14. Serialization and Save/Restore

The new implementation should have its own independent format unless interoperability with a legacy format is an explicit requirement.

Recommended internal representation:

```text
versioned MessagePack / CBOR / JSON
```

Example:

```rust
pub struct SavedDrawing {
    pub format_version: u32,
    pub drawing: DrawingState,
}
```

Requirements:

- explicit format version;
- migration path;
- deterministic IDs;
- corruption detection where useful;
- reproducible save/load round trips.

Property:

```text
decode(encode(state)) ≈ state
```

Define the equivalence relation explicitly.

---

## 15. Undo / Redo

Prefer command/event history or reversible deltas.

```rust
pub trait ReversibleEvent {
    fn apply(&self, state: &mut DrawingState);
    fn revert(&self, state: &mut DrawingState);
}
```

---

## 16. UI Separation

Do not begin by reproducing the historical UI.

Build first:

```text
geometry → model → commands → CLI → tests
```

Then optional:

```text
desktop GUI
web/WASM
TUI
MCP
headless server
```

CLI should be the first interface because it is deterministic, testable, scriptable, and agent-friendly.

Example:

```bash
cadctl new example.cadx
cadctl line example.cadx --from 0,0 --to 10,10
cadctl move example.cadx --entity 1 --by 2,3
cadctl inspect example.cadx
```

---

## 17. Agent-Friendly Interface

Expose machine-readable operations.

Request:

```json
{
  "op": "move",
  "entities": ["E42"],
  "displacement": [2.0, 3.0]
}
```

Response:

```json
{
  "status": "ok",
  "modified": ["E42"],
  "events": [
    {
      "type": "entity_modified",
      "entity": "E42"
    }
  ]
}
```

This supports:

- LLM agents;
- MCP servers;
- CI;
- formal verification harnesses;
- simulation environments;
- fuzzers.

---

## 18. Security and Sandboxing

Treat the historical binary and old file formats as untrusted.

Use:

- isolated VM where practical;
- no unnecessary network access;
- read-only reference media;
- hashes;
- VM snapshots;
- restricted shared folders.

---

## 19. Prohibited Production-Repository Content

Do not include, unless separately cleared:

- original Autodesk binaries;
- disk/archive images containing proprietary software;
- cracked or patched binaries;
- license keys;
- copy-protection bypass code;
- IDA/Ghidra databases;
- decompiler output;
- disassembly listings;
- mechanically translated source-like pseudocode;
- copied documentation;
- proprietary fonts/assets/icons.

When in doubt, convert research into an abstract behavioral statement.

---

## 20. Allowed Public Artifacts

Appropriate independently created artifacts include:

- behavioral specifications;
- mathematical definitions;
- entity schemas;
- interoperability-oriented file-format descriptions;
- original conformance tests;
- clean-room Rust source;
- Lean proofs;
- benchmark results;
- architecture documents;
- compatibility matrices;
- independently created examples.

---

## 21. Implementation Phases

### Phase 0 — Legal/provenance boundary

Deliverables:

```text
provenance record
private research directory
legal-boundaries.md
evidence-ID system
```

### Phase 1 — Minimal CAD mathematics

Implement:

```text
Point2
Vector2
Line
Circle
Arc
Transform2
AABB
```

### Phase 2 — Drawing model

Implement:

```text
DrawingState
EntityId
Entity
Layer
Block
```

Add serialization.

### Phase 3 — Command kernel

Implement:

```text
LINE
CIRCLE
ERASE
MOVE
COPY
ROTATE
SCALE
```

Represent modifications as events/deltas.

### Phase 4 — Behavioral conformance framework

Build:

```text
spec parser
fixture runner
normalized comparison
report generation
```

### Phase 5 — Reference characterization

Suggested order:

1. coordinates;
2. LINE;
3. CIRCLE;
4. ARC;
5. MOVE;
6. COPY;
7. ROTATE;
8. SCALE;
9. layers;
10. blocks.

### Phase 6 — Advanced editing

```text
TRIM
EXTEND
OFFSET
FILLET
CHAMFER
POLYLINE
BLOCK/INSERT
```

### Phase 7 — Interfaces

```text
CLI
JSON RPC / REST
MCP adapter
WASM
optional GUI
```

---

## 22. Definition of Done for Each Feature

- [ ] behavioral specification exists;
- [ ] provenance/evidence reference exists if based on reference behavior;
- [ ] semantics are independent of decompiler representation;
- [ ] Rust implementation exists;
- [ ] unit tests exist;
- [ ] boundary cases exist;
- [ ] property-based tests exist where appropriate;
- [ ] formal invariant exists where justified;
- [ ] conformance fixture exists;
- [ ] deterministic replay works;
- [ ] serialization behavior is defined if relevant;
- [ ] documentation lists known divergences;
- [ ] no prohibited reverse-engineering artifact entered the production repository.

---

## 23. Compatibility Policy

Use a matrix instead of vague compatibility claims:

| Feature | Reference observed | Implemented | Conformance | Notes |
|---|---:|---:|---:|---|
| LINE | yes | yes | exact | |
| CIRCLE | yes | yes | exact | |
| MOVE | yes | yes | exact | |
| ROTATE | yes | yes | tolerance | numeric difference |
| TRIM | partial | no | n/a | research pending |

Compatibility must be evidence-based.

---

## 24. Architecture Principle

> **The specification is authoritative for the new implementation; the reference binary is evidence used to improve the specification.**

Never make the Rust source structurally dependent on recovered implementation details.

If legacy behavior is strange, document separately:

```text
Observed legacy behavior
Normative behavior selected for the new implementation
Compatibility mode, if any
```

Do not automatically reproduce bugs.

---

## 25. Recommended First Milestone

Build the smallest complete independent system:

```text
DrawingState
    +
Line / Circle
    +
LINE / CIRCLE / MOVE / ERASE
    +
event log
    +
save/load
    +
CLI
    +
property tests
    +
behavioral fixture runner
```

Acceptance scenario:

```text
1. create drawing
2. LINE 0,0 → 100,0
3. CIRCLE center=50,50 radius=10
4. MOVE line by 10,20
5. save
6. reload
7. verify entity IDs
8. verify geometry
9. replay command/event sequence
10. verify identical final state
```

---

## 26. Guidance to the Implementation Agent

1. **Do not request or use decompiled AutoCAD code as implementation input.**
2. Work from `spec/`, `docs/behavior/`, and independently created fixtures.
3. Keep geometry code mathematically explicit and dependency-light.
4. Model editing as deterministic state transitions.
5. Prefer event/delta outputs for modifying commands.
6. Record architectural decisions.
7. Add property tests before optimizing.
8. Formalize high-value invariants rather than attempting to prove everything.
9. Do not reproduce legacy bugs without an explicit compatibility requirement.
10. Flag any task that appears to require copying proprietary implementation expression.
11. Keep all compatibility claims testable.
12. Optimize only after correctness and replayability are established.

---

## 27. Core Principle

The project should be defensible as:

> **A new CAD implementation derived from independently documented behavior and mathematical semantics, not a reconstruction of Autodesk source code.**

The desired result is not “AutoCAD-86 rewritten in Rust.”

The desired result is:

```text
historical CAD behavior
        ↓
knowledge
        ↓
explicit specification
        ↓
formal semantics
        ↓
modern independent CAD kernel
```

That distinction should remain visible in the architecture, repository history, documentation, and development process.
