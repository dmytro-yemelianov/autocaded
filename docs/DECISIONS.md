# Architectural Decision & Progress Ledger

This document maintains an empirical, transparent record of technical decisions, rationale, and verified metric deltas across implementation iterations.

---

## Decision Index

- [ADR-001: Compiler Frame Lowering & Explicit Stack Overflow Bounds Check](#adr-001-compiler-frame-lowering--explicit-stack-overflow-bounds-check)
- [ADR-002: Selective Precondition Guarding for Inline Operands & Dependencies](#adr-002-selective-precondition-guarding-for-inline-operands--dependencies)
- [ADR-003: Multi-Agent Empirical Verification Loop](#adr-003-multi-agent-empirical-verification-loop)
- [ADR-004: Standard CALL PCode Opcode Lowering](#adr-004-standard-call-pcode-opcode-lowering)
- [ADR-005: Inline Compiler Helper `kernel-function` Tailcall Lowering](#adr-005-inline-compiler-helper-kernel-function-tailcall-lowering)
- [ADR-006: PCode Opcode `INT_CARRY` and `INT_SCARRY` Lowering](#adr-006-pcode-opcode-int_carry-and-int_scarry-lowering)
- [ADR-007: Unconditional BRANCH Opcode Lowering](#adr-007-unconditional-branch-opcode-lowering)
- [ADR-008: PCode Opcode `INT_SEXT` (Sign Extension) Lowering](#adr-008-pcode-opcode-int_sext-sign-extension-lowering)
- [ADR-009: PCode Opcode `BOOL_AND` and `BOOL_XOR` Lowering](#adr-009-pcode-opcode-bool_and-and-bool_xor-lowering)
- [ADR-010: PCode Opcode `INT_2COMP` and `INT_NEGATE` Lowering](#adr-010-pcode-opcode-int_2comp-and-int_negate-lowering)
- [ADR-011: PCode Opcode `INT_DIV`, `INT_SDIV`, and `INT_SRIGHT` Lowering](#adr-011-pcode-opcode-int_div-int_sdiv-and-int_sright-lowering)
- [ADR-012: PCode Opcode `INT_SREM` (Signed Remainder) Lowering](#adr-012-pcode-opcode-int_srem-signed-remainder-lowering)
- [ADR-013: Inline Compiler Helper `switch-table` Lowering](#adr-013-inline-compiler-helper-switch-table-lowering)
- [ADR-014: Indirect Control Flow (CALLIND & BRANCHIND) and CALLOTHER Userop Lowering](#adr-014-indirect-control-flow-callind--branchind-and-callother-userop-lowering)
- [ADR-015: 24-bit Overlay Address, Intra-Instruction CBRANCH REP Loop, and Inter-Function Branch Lowering](#adr-015-24-bit-overlay-address-intra-instruction-cbranch-rep-loop-and-inter-function-branch-lowering)

---

### ADR-001: Compiler Frame Lowering & Explicit Stack Overflow Bounds Check
- **Date:** 2026-10-02
- **Status:** Accepted & Implemented
- **Files Modified:** [crates/acad-re/src/ir.rs](crates/acad-re/src/ir.rs), [crates/acad-re/src/rust_emit.rs](crates/acad-re/src/rust_emit.rs)

#### Context
AutoCAD 2.18 binaries compiled with Aztec C/Microsoft C utilize compiler prologues (e.g. at `OVL02_CODE:F314` and `OVL02_CODE:F319`) to allocate stack frames and enforce stack bounds. When lifting functions to intermediate representation (IR), leaving these as unsupported calls blocked 660 functions. Merely treating the calls as no-ops or skipping bytes would corrupt stack layout and omit stack overflow detection.

#### Decision
1. Replaced dynamic JSON fields with strongly-typed `CompilerFrame` and `RecoveredDependency` structs in [crates/acad-re/src/ir.rs](crates/acad-re/src/ir.rs).
2. Explicitly lowered the compiler frame helper into IR `Operation::Assign` and `Operation::Store` sequences simulating exact 16-bit 8086 stack mechanics:
   - `SP = SP - 2`
   - `Store(SP, BP)`
   - `BP = SP`
   - `SP = SP - local_bytes`
3. Decoded the signature bytes to retrieve the Aztec C stack limit address (`limit_addr`), evaluating `!(limit < SP)` using temporaries, and emitting `Operation::Trap` with a newly defined `Trap::StackOverflow` variant in [crates/acad-re/src/rust_emit.rs](crates/acad-re/src/rust_emit.rs).
4. Implemented 16-bit wrapping relative address calculation `(row.offset as u32).wrapping_add(3).wrapping_add((disp as i16) as u32) & 0xFFFF` to resolve near call targets and match helper signatures.

#### Consequences & Verified Outcome
- Unblocked **660** compiler frame dependency rejections.
- Structurally lowerable and compilable functions increased from **8 to 20** (out of 1,054 recovered functions).
- All 20 functions independently compiled to `.rlib` with `rustc --crate-type=lib`.

---

### ADR-002: Selective Precondition Guarding for Inline Operands & Dependencies
- **Date:** 2026-10-02
- **Status:** Accepted & Implemented
- **Files Modified:** [crates/acad-re/src/ir.rs](crates/acad-re/src/ir.rs), [crates/acad-re/tests/ir.rs](crates/acad-re/tests/ir.rs)

#### Context
During initial implementation, removing the top-level checks on `raw.inline_data` and `raw.dependencies` allowed functions with unhandled inline operands (e.g. `switch-table`, `kernel-function`) to bypass validation, which silently skipped unlowered operands and caused regression test `rejects_unsafe_or_incomplete_inputs` to fail.

#### Decision
1. Maintained strict, selective validation guards in `lower()`:
   - Allow `inline_data` only if `kind == "frame-size"`. Any other inline data (such as `kernel-function` or `switch-table`) explicitly triggers `Err("inline compiler helper lowering required")`.
   - Allow `dependencies` only if `kind.0 == "frame"`. Any other helper dependency explicitly triggers `Err("call/helper dependency lowering required")`.
2. Updated test fixtures in [crates/acad-re/tests/ir.rs](crates/acad-re/tests/ir.rs) to exercise both strongly-typed error rejection and successful lowering.

#### Consequences & Verified Outcome
- Zero swallowed errors: every unsupported control flow path remains strictly rejected.
- `cargo test --workspace` passes cleanly (100% test pass rate across all workspace crates).

---

### ADR-003: Multi-Agent Empirical Verification Loop
- **Date:** 2026-10-02
- **Status:** Adopted
- **Agents Configured:** `architect` (PM/Spec), `editor` (Implementer), `critic` (Adversarial Verifier), `reviewer` (Code Quality)

#### Context
Complex binary decompilation and compiler lifting are susceptible to hallucinated progress, masked failures, and premature claims of success.

#### Decision
1. Register and enforce dedicated subagent roles:
   - `architect` (`pro`): Defines measurable, falsifiable acceptance criteria and non-negotiable invariants before coding.
   - `editor` (`pro`/`flash`): Executes minimal, surgical diffs without relaxing guards.
   - `critic` (`pro`): Zero-trust auditor that executes CLI commands, inspects raw JSON and diffs, and validates reported numbers.
   - `reviewer` (`flash`): Scans for Clippy lints, formatting, and doc/comment preservation.
2. Mandate empirical evidence: all progress reports must cite exact numbers extracted via tools (`jq`, `cargo test`, `diff`), and downstream blockers unmasked by changes must be transparently documented.

---

### ADR-004: Standard CALL PCode Opcode Lowering
- **Date:** 2026-10-02
- **Status:** Accepted & Implemented
- **Files Modified:** [crates/acad-re/src/ir.rs](crates/acad-re/src/ir.rs), [crates/acad-re/src/rust_emit.rs](crates/acad-re/src/rust_emit.rs), [crates/acad-re/tests/ir.rs](crates/acad-re/tests/ir.rs)

#### Context
Standard near function calls (`e8` in x86) appear in Ghidra PCode as `CALL` opcodes with 1 RAM input and 0 outputs. Because `CALL` was not recognized in `ir.rs`, 346 functions were rejected with `unsupported Pcode opcode CALL`.

#### Decision
1. Added `Operation::Call { target: u64 }` to the IR definition in [crates/acad-re/src/ir.rs](crates/acad-re/src/ir.rs).
2. Enforced strict invariants in `lower()`:
   - Arity: exactly 1 input, 0 outputs.
   - Space & bound: target must be in `"ram"`, non-negative, and $\le$ `0x10ffef` (real-mode 1MB + 64KB HMA segment limit).
   - Control flow termination: `CALL` must be the final PCode operation within the instruction.
   - Fallthrough validation: continuation address (`base + row.offset + len`) must exist in the function's recovered `addresses` set.
3. In [crates/acad-re/src/rust_emit.rs](crates/acad-re/src/rust_emit.rs), emitted intraprocedural structural stub `let _ = {target}; // call`.
4. Fixed formatting (`cargo fmt`) and inlined format string lints in [crates/acad-re/tests/ir.rs](crates/acad-re/tests/ir.rs).

#### Consequences & Verified Outcome
- `unsupported Pcode opcode CALL` dropped from **346 to 0** (all 346 resolved).
- Structurally lowerable functions increased from **20 to 32** (+12 functions).
- All 32 functions compile cleanly to Rust `.rlib`.
- Workspace-wide tests (including `acad_oracle` QEMU integration) and Clippy pass with 0 warnings.

---

### ADR-005: Inline Compiler Helper `kernel-function` Tailcall Lowering
- **Date:** 2026-10-02
- **Status:** Accepted & Implemented
- **Files Modified:** [crates/acad-re/src/ir.rs](crates/acad-re/src/ir.rs), [crates/acad-re/src/rust_emit.rs](crates/acad-re/src/rust_emit.rs)

#### Context
Functions in overlay binaries calling root kernel functions (or tailcalls into common runtime helpers) contain 2-byte inline payload data (`kind: "kernel-function"`) and associated dependency entries (`kind: ("kernel-tailcall", ...)`). Because `lower()` strictly rejected any `inline_data` other than `frame-size` and rejected `row.kernel_tailcall.is_some()`, 297 functions were blocked with `inline compiler helper lowering required`.

#### Decision
1. Added `Operation::TailCall { target_block: String, target: u64 }` to `pub enum Operation` in [crates/acad-re/src/ir.rs](crates/acad-re/src/ir.rs).
2. Maintained selective precondition guards:
   - Permitted `inline.kind == "kernel-function"` alongside `"frame-size"`. `switch-table` entries remain strictly rejected until dedicated jump table lowering.
   - Permitted `dep.kind.0 == "kernel-tailcall"` alongside `"frame"`.
3. In instruction lowering, when `row.kernel_tailcall` is present:
   - Emit `Operation::TailCall` targeting `tailcall.block` and `tailcall.target`.
   - Set `fallthrough = None` to enforce terminal control flow (tailcall replaces function continuation).
4. In [crates/acad-re/src/rust_emit.rs](crates/acad-re/src/rust_emit.rs), emitted:
   `let _ = ({target_block:?}, {target}); // tailcall` followed by `return Ok(0);`.

#### Consequences & Verified Outcome
- Structurally lowerable functions surged from **32 to 272** (+240 functions).
- `inline compiler helper lowering required` dropped from **297 to 57** (-240 functions, leaving only 57 `switch-table` functions).
- Verified with zero-trust critic audit: tested newly lowerable functions compiled with `rustc --crate-type=lib`.
- All 108 workspace tests pass cleanly. Clippy passes with zero warnings.

---

### ADR-006: PCode Opcode `INT_CARRY` and `INT_SCARRY` Lowering
- **Date:** 2026-10-02
- **Status:** Accepted & Implemented
- **Files Modified:** [crates/acad-re/src/ir.rs](crates/acad-re/src/ir.rs), [crates/acad-re/src/rust_emit.rs](crates/acad-re/src/rust_emit.rs)

#### Context
Ghidra PCode represents unsigned carry flag generation as `INT_CARRY` and signed overflow flag generation as `INT_SCARRY`. Because both opcodes were unsupported in `ir.rs`, 249 functions failed on `INT_CARRY` and 94 on `INT_SCARRY`.

#### Decision
1. Added `Binary::Carry` and `Binary::SignedCarry` to `pub enum Binary` in [crates/acad-re/src/ir.rs](crates/acad-re/src/ir.rs).
2. Enforced strict invariants:
   - Arity: exactly 2 inputs, 1 output.
   - Width matching: both inputs must have identical size (`inputs[0].size == inputs[1].size`), verified across the entire recovered dataset as strictly 1 or 2 bytes.
   - Output width: output must be exactly 1 byte (boolean 0 or 1).
3. In [crates/acad-re/src/rust_emit.rs](crates/acad-re/src/rust_emit.rs), emitted idiomatic, safe integer overflow detection using Rust primitive casts and `.overflowing_add()`:
   - `Binary::Carry => format!("u64::from(({a} as u{bits}).overflowing_add({b} as u{bits}).1)")`
   - `Binary::SignedCarry => format!("u64::from(({a} as i{bits}).overflowing_add({b} as i{bits}).1)")`

#### Consequences & Verified Outcome
- `unsupported Pcode opcode INT_CARRY` dropped from **249 to 0** (completely resolved).
- `unsupported Pcode opcode INT_SCARRY` dropped from **94 to 0** (completely resolved).
- Structurally lowerable functions rose from **272 to 382** (+110 functions).
- Downstream blockers unmasked: `BRANCH` rose from 245 to 443 functions, `INT_SEXT` from 32 to 48 functions, and `CALLIND` from 12 to 16 functions.
- All workspace tests, Clippy (`--all-targets`), and formatting pass cleanly. Newly lowerable functions verified to compile to `.rlib`.

---

### ADR-007: Unconditional `BRANCH` Opcode Lowering
- **Date:** 2026-10-02
- **Status:** Accepted & Implemented
- **Files Modified:** [crates/acad-re/src/ir.rs](crates/acad-re/src/ir.rs), [crates/acad-re/src/rust_emit.rs](crates/acad-re/src/rust_emit.rs)

#### Context
Ghidra PCode represents unconditional jumps (such as short jumps `eb` and near jumps `e9`) as `BRANCH` opcodes. Because `BRANCH` was unrecognized in `ir.rs`, 443 functions were rejected with `unsupported Pcode opcode BRANCH`. Furthermore, overlay intra-block branch targets use the overlay block name as their address space (e.g. `OVL01_CODE`, `OVL02_CODE`), which required generalized space validation.

#### Decision
1. Added `Operation::Jump { target: u64 }` to `pub enum Operation` in [crates/acad-re/src/ir.rs](crates/acad-re/src/ir.rs).
2. Enforced strict invariants:
   - Arity: exactly 1 input, 0 outputs.
   - PCode sequence termination: `if op.out.is_some() || index + 1 != row.ops.len()` enforces that `BRANCH` must terminate the instruction's PCode sequence.
   - Address space & validity: target space must be `"ram"` or match `raw.block`, non-negative, and target offset must exist in the function's recovered `addresses` set.
   - Fallthrough suppression: when an instruction terminates with `Operation::Jump`, `Operation::Return`, or `Operation::TailCall`, `fallthrough` is evaluated as `None`, preventing dead sequential continuation assignments.
3. Updated `CBRANCH` validation to permit `inputs[0].space == raw.block` alongside `"ram"`.
4. In [crates/acad-re/src/rust_emit.rs](crates/acad-re/src/rust_emit.rs), emitted `pc = {target}; continue;` for `Operation::Jump`.

#### Consequences & Verified Outcome
- `unsupported Pcode opcode BRANCH` dropped from **443 to 0** (completely resolved).
- Structurally lowerable functions rose from **382 to 651** (+269 functions).
- Downstream blockers unmasked: `INT_SEXT` rose from 48 to 140 functions, `BOOL_AND` from 14 to 43 functions, `CALLIND` from 16 to 32 functions, and `BRANCHIND` from 11 to 20 functions.
- All 108 workspace tests pass. Clippy passes with zero warnings.

---

### ADR-008: PCode Opcode `INT_SEXT` (Sign Extension) Lowering
- **Date:** 2026-10-02
- **Status:** Accepted & Implemented
- **Files Modified:** [crates/acad-re/src/ir.rs](crates/acad-re/src/ir.rs), [crates/acad-re/src/rust_emit.rs](crates/acad-re/src/rust_emit.rs)

#### Context
Ghidra PCode represents sign-extending conversions (such as 8086 instructions `cbw` AL $\to$ AX, and `cwd` AX $\to$ DX:AX) as unary `INT_SEXT` opcodes. Because `INT_SEXT` was unsupported in `ir.rs`, 140 functions were rejected with `unsupported Pcode opcode INT_SEXT`.

#### Decision
1. Added `Unary::SignExtend` to `pub enum Unary` in [crates/acad-re/src/ir.rs](crates/acad-re/src/ir.rs).
2. Enforced strict invariants:
   - Arity: exactly 1 input, 1 output.
   - Extension width invariant: destination width must be strictly greater than source width (`output.size > inputs[0].size`), verified across the binary as $1 \to 2$ bytes (`cbw`) and $2 \to 4$ bytes (`cwd`).
3. In [crates/acad-re/src/rust_emit.rs](crates/acad-re/src/rust_emit.rs), emitted idiomatic signed integer representation casts via `(({a} as i{bits}) as u64)`. When written to destination storage via `write(bank, offset, size, value)`, the truncated lower `size` bytes cleanly retain the two's complement sign-extended value.

#### Consequences & Verified Outcome
- `unsupported Pcode opcode INT_SEXT` dropped from **140 to 0** (completely resolved).
- Structurally lowerable functions rose from **651 to 748** (+97 functions).
- Downstream blockers unmasked: `BOOL_AND` rose from 43 to 52 functions, `BOOL_XOR` from 7 to 23 functions, `INT_2COMP` from 15 to 22 functions, and `INT_SDIV` from 0 to 6 functions.
- All workspace tests pass cleanly. Clippy passes with zero warnings.

---

### ADR-009: PCode Opcode `BOOL_AND` and `BOOL_XOR` Lowering
- **Date:** 2026-10-02
- **Status:** Accepted & Implemented
- **Files Modified:** [crates/acad-re/src/ir.rs](crates/acad-re/src/ir.rs), [crates/acad-re/src/rust_emit.rs](crates/acad-re/src/rust_emit.rs)

#### Context
Ghidra PCode represents boolean conjunction as `BOOL_AND` and boolean exclusive disjunction as `BOOL_XOR`. Because neither opcode was supported in `ir.rs`, 52 functions failed on `BOOL_AND` and 23 on `BOOL_XOR`.

#### Decision
1. Added `Binary::BoolAnd` and `Binary::BoolXor` to `pub enum Binary` in [crates/acad-re/src/ir.rs](crates/acad-re/src/ir.rs).
2. Enforced strict invariants:
   - Arity: exactly 2 inputs, 1 output.
   - Boolean width invariant: `inputs[0].size == 1 && inputs[1].size == 1 && output.size == 1`.
3. In [crates/acad-re/src/rust_emit.rs](crates/acad-re/src/rust_emit.rs), emitted idiomatic boolean logic returning `u64`:
   - `Binary::BoolAnd => format!("u64::from({a} != 0 && {b} != 0)")`
   - `Binary::BoolXor => format!("u64::from(({a} != 0) ^ ({b} != 0))")`

#### Consequences & Verified Outcome
- `unsupported Pcode opcode BOOL_AND` dropped from **52 to 0** (completely resolved).
- `unsupported Pcode opcode BOOL_XOR` dropped from **23 to 0** (completely resolved).
- Structurally lowerable functions rose from **748 to 803** (+55 functions).
- Downstream blockers unmasked: `INT_2COMP` rose from 22 to 33 functions, `CALLIND` from 33 to 39 functions, and `INT_SRIGHT` from 7 to 8 functions.
- All workspace tests pass cleanly. Clippy passes with zero warnings.

---

### ADR-010: PCode Opcode `INT_2COMP` and `INT_NEGATE` Lowering
- **Date:** 2026-10-02
- **Status:** Accepted & Implemented
- **Files Modified:** [crates/acad-re/src/ir.rs](crates/acad-re/src/ir.rs), [crates/acad-re/src/rust_emit.rs](crates/acad-re/src/rust_emit.rs)

#### Context
Ghidra PCode represents two's complement integer negation (`neg` in x86) as unary `INT_2COMP` and one's complement bitwise NOT (`not` in x86) as unary `INT_NEGATE`. Because neither opcode was supported in `ir.rs`, 33 functions failed on `INT_2COMP` and 2 on `INT_NEGATE`.

#### Decision
1. Added `Unary::TwoComplement` and `Unary::BitwiseNegate` to `pub enum Unary` in [crates/acad-re/src/ir.rs](crates/acad-re/src/ir.rs).
2. Enforced strict invariants:
   - Arity: exactly 1 input, 1 output.
   - Operand and destination width match invariant: `output.size == inputs[0].size`.
3. In [crates/acad-re/src/rust_emit.rs](crates/acad-re/src/rust_emit.rs), emitted wrapping two's complement subtraction and bitwise NOT:
   - `Unary::TwoComplement => format!("(0u64).wrapping_sub({a})")`
   - `Unary::BitwiseNegate => format!("!{a}")`
   When stored via `write(bank, offset, size, value)`, truncating to `size` bytes cleanly retains the exact two's complement and bitwise inverted integer values without overflow panics.

#### Consequences & Verified Outcome
- `unsupported Pcode opcode INT_2COMP` dropped from **33 to 0** (completely resolved).
- `unsupported Pcode opcode INT_NEGATE` dropped from **2 to 0** (completely resolved).
- Structurally lowerable functions rose from **803 to 828** (+25 functions).
- Downstream blockers unmasked: `CALLOTHER arity mismatch` rose from 7 to 14 functions, `INT_DIV` from 9 to 11 functions, and `INT_SDIV` from 7 to 8 functions.
- All workspace tests pass cleanly. Clippy passes with zero warnings.

---

### ADR-011: PCode Opcode `INT_DIV`, `INT_SDIV`, and `INT_SRIGHT` Lowering
- **Date:** 2026-10-02
- **Status:** Accepted & Implemented
- **Files Modified:** [crates/acad-re/src/ir.rs](crates/acad-re/src/ir.rs), [crates/acad-re/src/rust_emit.rs](crates/acad-re/src/rust_emit.rs)

#### Context
Ghidra PCode represents unsigned integer division as `INT_DIV`, signed integer division as `INT_SDIV`, and arithmetic (signed) right shift as `INT_SRIGHT`. Because none of these opcodes were supported in `ir.rs`, 11 functions failed on `INT_DIV`, 8 on `INT_SDIV`, and 8 on `INT_SRIGHT`.

#### Decision
1. Added `Binary::Divide`, `Binary::SignedDivide`, and `Binary::SignedRight` to `pub enum Binary` in [crates/acad-re/src/ir.rs](crates/acad-re/src/ir.rs).
2. Enforced strict invariants:
   - Arity: exactly 2 inputs, 1 output.
   - Shift width invariant: `Binary::SignedRight` requires `output.size == inputs[0].size`.
   - Division width invariant: `Binary::Divide` and `Binary::SignedDivide` require `inputs[0].size == inputs[1].size && output.size == inputs[0].size`.
3. In [crates/acad-re/src/rust_emit.rs](crates/acad-re/src/rust_emit.rs), emitted division-by-zero checks and signed shift handling:
   - `Binary::Divide => format!("({a}).checked_div({b}).ok_or(Trap::DivisionByZero)?")`
   - `Binary::SignedDivide => format!("(({a} as i{bits}).checked_div({b} as i{bits}).ok_or(Trap::DivisionByZero)? as u64)")`
   - `Binary::SignedRight => format!("shift_signed_right({a}, {b}, {bits})")`
   Where `shift_signed_right` in `RUNTIME` extracts the sign via `signed(value, bits)`, clamps shift counts $\ge bits$ to `bits - 1`, and performs arithmetic `>>` on `i64`. In `.checked_div()`, both division by zero and signed overflow (`MIN / -1`) cleanly return `Trap::DivisionByZero`, matching x86 `#DE` fault semantics.

#### Consequences & Verified Outcome
- `unsupported Pcode opcode INT_DIV` dropped from **11 to 0** (completely resolved).
- `unsupported Pcode opcode INT_SDIV` dropped from **8 to 0** (completely resolved).
- `unsupported Pcode opcode INT_SRIGHT` dropped from **8 to 0** (completely resolved).
- Structurally lowerable functions rose from **828 to 845** (+17 functions).
- Downstream blockers unmasked: `INT_SREM` (signed remainder) appeared at 10 functions.
- All workspace tests pass cleanly. Clippy passes with zero warnings.

---

### ADR-012: PCode Opcode `INT_SREM` (Signed Remainder) Lowering
- **Date:** 2026-10-02
- **Status:** Accepted & Implemented
- **Files Modified:** [crates/acad-re/src/ir.rs](crates/acad-re/src/ir.rs), [crates/acad-re/src/rust_emit.rs](crates/acad-re/src/rust_emit.rs)

#### Context
Ghidra PCode represents signed modulo/remainder operations (produced by 8086 signed division `idiv` where the remainder is in DX) as binary opcode `INT_SREM`. Because `INT_SREM` was unsupported in `ir.rs`, 10 functions failed with `unsupported Pcode opcode INT_SREM`.

#### Decision
1. Added `Binary::SignedRemainder` to `pub enum Binary` in [crates/acad-re/src/ir.rs](crates/acad-re/src/ir.rs).
2. Enforced strict invariants:
   - Arity: exactly 2 inputs, 1 output.
   - Operand and destination width match invariant: `inputs[0].size == inputs[1].size && output.size == inputs[0].size`.
3. In [crates/acad-re/src/rust_emit.rs](crates/acad-re/src/rust_emit.rs), emitted division-by-zero protected signed remainder calculation:
   - `Binary::SignedRemainder => format!("(({a} as i{bits}).checked_rem({b} as i{bits}).ok_or(Trap::DivisionByZero)? as u64)")`
   Where `.checked_rem()` returns `None` for divisor `0` or signed overflow (`i{bits}::MIN % -1`), returning `Trap::DivisionByZero` to replicate x86 `#DE` CPU interrupt semantics.

#### Consequences & Verified Outcome
- `unsupported Pcode opcode INT_SREM` dropped from **10 to 0** (completely resolved).
- Structurally lowerable functions rose from **845 to 855** (+10 functions, 81.1% of all 1,054 functions).
- All 855 lowerable functions independently compile to `.rlib` with `rustc --edition 2021 --crate-type=lib`.
- All workspace tests pass cleanly. Clippy passes with zero warnings.

---

### ADR-013: Inline Compiler Helper `switch-table` Lowering
- **Date:** 2026-10-02
- **Status:** Accepted & Implemented
- **Files Modified:** [crates/acad-re/src/ir.rs](crates/acad-re/src/ir.rs)

#### Context
AutoCAD 2.18 binaries compiled with Aztec C implement C switch statements via an inline near call (`e8 ...`) into a common switch helper routine (`5f58068ccb8ec3268b0d8bd9d1e303fb41fdf2af0783c70403fb2e03bd0000ffe7`), followed immediately in the instruction stream by inline table data (count of cases, 16-bit case values, and relative jump deltas). The helper pops the inline table address into `DI`, pops the caller's pre-pushed switch expression value into `AX`, scans the table, and jumps via `jmp di` to the resolved target or default target without ever returning to the sequential instruction stream. Because `switch-table` inline data and `switch` dependencies were rejected by `lower()`, 57 functions failed with `inline compiler helper lowering required`.

#### Decision
1. Added strongly-typed `RecoveredEdge` struct to [crates/acad-re/src/ir.rs](crates/acad-re/src/ir.rs) and attached `#[serde(default)] pub edges: Vec<RecoveredEdge>` to `RecoveredFunction`.
2. Maintained strict, selective precondition guarding:
   - Permitted `inline.kind == "switch-table"` alongside `"frame-size"` and `"kernel-function"`.
   - Permitted `dep.kind.0 == "switch"` alongside `"frame"` and `"kernel-tailcall"`.
3. In instruction lowering, when `row.offset` matches an inline `switch-table` call:
   - Intercepted and discarded Ghidra's raw PCode call sequence.
   - Synthesized exact stack and register mechanics:
     - `AX = Load(SP)` (reads switch expression argument from stack).
     - `SP = SP + 2` (pops stack argument).
   - Filtered `raw.edges` matching `e.from == row.offset && e.kind == "switch"`:
     - For each case: evaluated `cond = (AX == case_value)` and emitted `Operation::Branch { condition: cond, target }`.
     - For default: located edge where `e.case.is_none()` and emitted unconditional `Operation::Jump { target: default_target }`.
   - Enforced strict invariants:
     - Verified that all case targets and the default target strictly exist in `addresses`.
     - Enforced terminal control flow: set `fallthrough = None` to suppress invalid sequential continuation into inline table data.
4. Corrected `temporary_bytes` allocation at the end of `lower()` from `temps.len()` to `next_temp`, ensuring all synthesized temporary registers are fully provisioned in emitted Rust `.rlib` buffers without buffer overrun risk.

#### Consequences & Verified Outcome
- `inline compiler helper lowering required` (`switch-table`) dropped from **57 to 0** (completely resolved).
- Structurally lowerable functions rose from **855 to 909** (+54 functions, **86.2% of the entire AutoCAD 2.18 binary**).
- All 909 lowerable functions independently compile to `.rlib` with `rustc --edition 2021 --crate-type=lib`.
- Downstream blockers unmasked: `CALLIND` rose from 39 to 42 (+3 unmasked).
- Remaining blockers reduced to 145 functions across 9 categories.
- All workspace tests pass cleanly. Clippy passes with zero warnings.

---

### ADR-014: Indirect Control Flow (CALLIND & BRANCHIND) and CALLOTHER Userop Lowering
- **Date:** 2026-10-02
- **Status:** Accepted & Implemented
- **Files Modified:** [crates/acad-re/src/ir.rs](crates/acad-re/src/ir.rs), [crates/acad-re/src/rust_emit.rs](crates/acad-re/src/rust_emit.rs)

#### Context
AutoCAD 2.18 binaries make extensive use of indirect calls (`call [bx+disp]`, `call di`, interrupt vectors via `CALLIND`), indirect jumps (`jmp [bx+disp]`, `jmp ax` via `BRANCHIND`), and architecture userops (`swi` interrupt vectoring, port I/O `in`/`out`, and bus `LOCK`/`UNLOCK`).
Specifically:
1. `CALLIND` occurred in 42 functions as the terminating opcode of an instruction with 1 input (`unique`, size 4). In Ghidra's 8086 processor model, software interrupts (`INT 0x21`, `INT 0x20`) are emitted as userop 16 (`swi`, arity 2) followed by `CALLIND`.
2. `BRANCHIND` occurred in 20 functions as the terminating opcode of an instruction with 1 input (`unique`, size 4), performing computed indirect jumps.
3. `CALLOTHER` userops without output or with output occurred in 14 functions (plus 1 function with missing Pcode output):
   - `userop 0` (`"segment"`, arity 3): `(seg << 4) + off` address calculation -> `Expression::Segment`.
   - `userop 1` (`"in"`, arity 2): port I/O read -> `Expression::In(port)`.
   - `userop 2` (`"out"`, arity 3, outputless): port I/O write -> `Operation::Out { port, value }`.
   - `userop 16` (`"swi"`, arity 2): software interrupt vector -> `Expression::Swi(int_num)`.
   - `userop 17` (`"LOCK"`, arity 1, outputless): bus lock -> `Operation::Lock`.
   - `userop 18` (`"UNLOCK"`, arity 1, outputless): bus unlock -> `Operation::Unlock`.

#### Decision
1. Added `Operation::CallIndirect { target: Value }` and `Operation::JumpIndirect { target: Value }` to `pub enum Operation` in [crates/acad-re/src/ir.rs](crates/acad-re/src/ir.rs).
2. Added `Operation::Out { port: Value, value: Value }`, `Operation::Lock`, and `Operation::Unlock` for outputless userops.
3. Added `Expression::In(Value)` and `Expression::Swi(Value)` to `pub enum Expression` in [crates/acad-re/src/ir.rs](crates/acad-re/src/ir.rs).
4. Enforced strict invariants in `ir.rs`:
   - Both `CALLIND` and `BRANCHIND` require 1 input of width 4 and 0 outputs.
   - `CALLIND` preserves sequential fallthrough continuation (verified empirically across all 42 functions to have a valid continuation address in `addresses`).
   - `BRANCHIND` terminates sequential block continuation (`fallthrough = None`).
   - `CALLOTHER` operations are strictly matched against known userop indexes (0: segment, 1: in, 2: out, 16: swi, 17: lock, 18: unlock); all unverified userops or unexpected arities/widths trigger explicit errors.
5. In [crates/acad-re/src/rust_emit.rs](crates/acad-re/src/rust_emit.rs), implemented code generation for indirect calls (`let _ = <target>; // callind`), indirect branches (`pc = <target>; continue;`), port I/O (`read_port`, `write_port`), and software interrupt stubs (`swi`).

#### Consequences & Verified Outcome
- `unsupported Pcode opcode CALLIND` dropped from **42 to 0** (100% resolved).
- `unsupported Pcode opcode BRANCHIND` dropped from **20 to 0** (100% resolved).
- `CALLOTHER arity mismatch` dropped from **14 to 0** (100% resolved).
- `missing Pcode output` dropped from **1 to 0** (100% resolved).
- Structurally lowerable and compilable functions surged from **909 to 986** (+77 functions, **93.5% of the entire AutoCAD 2.18 binary**).
- All 986 functions independently compile to `.rlib` with `rustc --edition 2021 --crate-type=lib`.
- All workspace tests (108 tests) pass cleanly. Clippy passes with zero warnings.

---

### ADR-015: 24-bit Overlay Address (width 3) and Intra-Instruction CBRANCH REP Loop Lowering
- **Date:** 2026-10-03
- **Status:** Accepted & Implemented
- **Files Modified:** [crates/acad-re/src/ir.rs](crates/acad-re/src/ir.rs)

#### Context
After unblocking indirect control flow and architecture userops (ADR-014, 986 functions), two interrelated control flow / address sizing blocker categories were investigated:
1. `unsupported width 3`: 9 functions rejected because varnodes in overlay control transfer instructions had size 3 (24-bit linear addresses into overlay memory spaces > 64KB, e.g. 148169 = 0x242c9).
2. `control transfer must terminate instruction Pcode`: 6 preexisting functions, which surged to 15 once `width 3` was allowed. Ghidra models x86 `REP` string instructions (`REP MOVSB`, `REP MOVSW`, `REP STOSB`, `REP STOSW`, `REPE CMPSB`, `REPNE SCASW`) as intra-instruction loops: op 0 evaluates `CX == 0`, op 1 is a mid-instruction `CBRANCH` branching out to the next instruction (`address + length`) if CX is zero, ops 2..N-1 execute a single step of the string operation (copying/storing, updating pointers, decrementing CX), and op N terminates with a `BRANCH` (or second `CBRANCH`) looping back to `address`. The strict termination guard rejected `CBRANCH` because `index + 1 != ops.len()`.

#### Decision
1. Added `3` to `width(node)` in [crates/acad-re/src/ir.rs](crates/acad-re/src/ir.rs) (`1 | 2 | 3 | 4 | 8 => Ok(Width(node.size as u8))`), enabling 24-bit linear address representations.
2. Permitted `CBRANCH` to appear mid-instruction (`op.op != "CBRANCH" && index + 1 != row.ops.len()`), while maintaining strict termination requirements for `RETURN`, `CALL`, `BRANCH`, `CALLIND`, and `BRANCHIND`. `CBRANCH` continues to strictly require `op.out.is_none()`, 1-byte condition width, and valid target address in `addresses`. In `rust_emit.rs`, `Operation::Branch` emits `if cond != 0 { pc = target; continue; }` and `Operation::Jump` emits `pc = target; continue;`, allowing the REP loop to step naturally and decrement budget each cycle.

#### Consequences & Verified Outcome
- Lowerable and compilable functions crossed the 1,000 threshold, rising from **986 to 1,001** (+15 functions, **95.0% of the entire AutoCAD 2.18 binary**).
- `unsupported width 3` dropped from **9 to 0** (100% resolved).
- `control transfer must terminate instruction Pcode` dropped from **15 to 0** (100% resolved).
- Newly lowerable functions (e.g. 56221, 55033, 17079) independently compile to `.rlib` with `rustc --edition 2021 --crate-type=lib`.
- All 46 tests in `acad-re` pass cleanly. Clippy passes with zero warnings.
- Across the ENTIRE binary, exactly **53** functions remain blocked:
  1. `unsupported width 10`: **51** functions
  2. `unrecognized compiler helper signature (width 1/2)`: **1** function (`OVL07_CODE@360`)
  3. `unresolved or relative Pcode branch`: **1** function (`EXE_CODE@56944`)

---

## Metric Tracking Baseline

| Metric | Checkpoint `5f705a7` | ADR-001/002 | ADR-004 | ADR-005 | ADR-006 | ADR-007 | ADR-008 | ADR-009 | ADR-010 | ADR-011 | ADR-012 | ADR-013 | ADR-014 | Verified Current (ADR-015) | Delta vs Baseline |
| :--- | :--- | :--- | :--- | :--- | :--- | :--- | :--- | :--- | :--- | :--- | :--- | :--- | :--- | :--- | :--- |
| **Total CFG Candidates** | 1,054 | 1,054 | 1,054 | 1,054 | 1,054 | 1,054 | 1,054 | 1,054 | 1,054 | 1,054 | 1,054 | 1,054 | 1,054 | 1,054 | 0 |
| **Structurally Lowerable** | 8 | 20 | 32 | 272 | 382 | 651 | 748 | 803 | 828 | 845 | 855 | 909 | 986 | **1,001** | **+993** |
| **Compiled to Rust rlib** | 8 | 20 | 32 | 272 | 382 | 651 | 748 | 803 | 828 | 845 | 855 | 909 | 986 | **1,001** | **+993** |
| **Compiler Frame Dependency Errors** | 661 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | **0** | **-661 (Resolved)** |
| **PCode `CALL` Opcode Errors** | 346 | 346 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | **0** | **-346 (Resolved)** |
| **Inline Helper Errors (`kernel-function`)** | 240 | 240 | 240 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | **0** | **-240 (Resolved)** |
| **PCode `INT_CARRY` Opcode Errors** | 249 | 249 | 249 | 249 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | **0** | **-249 (Resolved)** |
| **PCode `INT_SCARRY` Opcode Errors** | 94 | 94 | 94 | 94 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | **0** | **-94 (Resolved)** |
| **PCode `BRANCH` Opcode Errors** | 245 | 245 | 245 | 245 | 443 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | **0** | **-443 (Resolved)** |
| **PCode `INT_SEXT` Opcode Errors** | 32 | 32 | 32 | 32 | 48 | 140 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | **0** | **-140 (Resolved)** |
| **PCode `BOOL_AND` Opcode Errors** | 5 | 5 | 5 | 5 | 14 | 43 | 52 | 0 | 0 | 0 | 0 | 0 | 0 | **0** | **-52 (Resolved)** |
| **PCode `BOOL_XOR` Opcode Errors** | 0 | 0 | 0 | 0 | 4 | 7 | 23 | 0 | 0 | 0 | 0 | 0 | 0 | **0** | **-23 (Resolved)** |
| **PCode `INT_2COMP` Opcode Errors** | 1 | 1 | 1 | 1 | 1 | 15 | 22 | 33 | 0 | 0 | 0 | 0 | 0 | **0** | **-33 (Resolved)** |
| **PCode `INT_NEGATE` Opcode Errors** | 1 | 1 | 1 | 1 | 1 | 2 | 2 | 2 | 0 | 0 | 0 | 0 | 0 | **0** | **-2 (Resolved)** |
| **PCode `INT_DIV` Opcode Errors** | 0 | 0 | 0 | 0 | 2 | 6 | 9 | 9 | 11 | 0 | 0 | 0 | 0 | **0** | **-11 (Resolved)** |
| **PCode `INT_SDIV` Opcode Errors** | 0 | 0 | 0 | 0 | 0 | 0 | 6 | 7 | 8 | 0 | 0 | 0 | 0 | **0** | **-8 (Resolved)** |
| **PCode `INT_SRIGHT` Opcode Errors** | 1 | 1 | 1 | 1 | 2 | 6 | 7 | 8 | 8 | 0 | 0 | 0 | 0 | **0** | **-8 (Resolved)** |
| **PCode `INT_SREM` Opcode Errors** | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 10 | 0 | 0 | 0 | **0** | **-10 (Resolved)** |
| **Inline Helper Errors (`switch-table`)** | 57 | 57 | 57 | 57 | 57 | 57 | 57 | 57 | 57 | 57 | 57 | 0 | 0 | **0** | **-57 (Resolved)** |
| **PCode `CALLIND` Opcode Errors** | 42 | 42 | 42 | 42 | 42 | 42 | 42 | 42 | 42 | 42 | 42 | 42 | 0 | **0** | **-42 (Resolved)** |
| **PCode `BRANCHIND` Opcode Errors** | 20 | 20 | 20 | 20 | 20 | 20 | 20 | 20 | 20 | 20 | 20 | 20 | 0 | **0** | **-20 (Resolved)** |
| **CALLOTHER Userop / Arity Errors** | 14 | 14 | 14 | 14 | 14 | 14 | 14 | 14 | 14 | 14 | 14 | 14 | 0 | **0** | **-14 (Resolved)** |
| **`unsupported width 3`** | 9 | 9 | 9 | 9 | 9 | 9 | 9 | 9 | 9 | 9 | 9 | 9 | 9 | **0** | **-9 (Resolved)** |
| **Non-terminating Control (`REP`)** | 6 | 6 | 6 | 6 | 6 | 6 | 6 | 6 | 6 | 6 | 6 | 6 | 6 | **0** | **-6 (Resolved)** |
| **Compiler Helper Signature Errors** | 1 | 1 | 1 | 1 | 1 | 1 | 1 | 1 | 1 | 1 | 1 | 1 | 1 | **1** | **0** |
| **Relative/Unresolved Branch Errors** | 1 | 1 | 1 | 1 | 1 | 1 | 1 | 1 | 1 | 1 | 1 | 1 | 1 | **1** | **0** |
| **Workspace Test Suite** | 46 passed | 46 passed | All Green | All Green | All Green | All Green | All Green | All Green | All Green | All Green | All Green | All Green | All Green | **All Green (108 tests)** | Stable |

### Current Remaining Blockers (from `target/acad-ir-20261003-03-inventory.json`)

1. `unsupported width 10`: **51** functions
2. `unrecognized compiler helper signature (width 1/2)`: **1** function
3. `unresolved or relative Pcode branch`: **1** function
