# ALGOL26 Architecture Contract

**Effective**: v0.8.0

## Architectural Invariants

The following invariants MUST hold at all times. Architecture tests should enforce these.

### Invariant 1: AST Never Reaches a Backend

Backends (LLVM, WASM, Interpreter) receive only `SemanticProgram`.
They never import from `frontend::ast`, `frontend::parser`, or `frontend::lexer`.

### Invariant 2: Backend Never Performs Semantic Analysis

Type checking, borrow checking, and ownership analysis happen in `semantics/`.
Backends assume the IR is already semantically valid.

### Invariant 3: Semantic IR Is Backend-Independent

`ir::semantic_ir` contains no LLVM types, WASM types, or interpreter-specific data.
No target-specific lowering in the canonical IR.

### Invariant 4: Optimization Preserves Observable Behavior

`execute(original_ir) == execute(optimized_ir)` must always hold.
No optimization may change what the program outputs or its side effects.

### Invariant 5: Every Lowering Pass Preserves Semantics

Loop desugaring, defer lowering, and monomorphization must not change program meaning.
Each pass has a documented contract (see `ir-pass-contracts.md`).

### Invariant 6: Unsafe Operations Are Explicitly Represented

Unsafe blocks, raw pointers, and FFI calls must be explicit in the AST and IR.
No implicit unsafe behavior.

### Invariant 7: Diagnostics Preserve Source Locations

All compiler errors and warnings must include source location (line, column).
No diagnostic without a location.

### Invariant 8: Module Loading Is Independent of Code Generation

`frontend::module_loader` handles imports.
Code generation never performs filesystem traversal.

### Invariant 9: Backends Cannot Depend on One Another

LLVM backend must not import from WASM backend or Interpreter.
Each backend is a completely independent consumer of `SemanticProgram`.

### Invariant 10: Verified IR Is the Only Backend Input

Backends receive IR that has passed `verify()`.
No backend should receive unverified IR.

---

## Enforcement Status

| Invariant | Enforced By | Status |
|-----------|-------------|--------|
| 1. AST never reaches backend | backend_independence.rs | ✅ Tested |
| 2. Backend never performs analysis | backend_trait_test.rs | ✅ Tested |
| 3. IR backend-independent | backend_independence.rs | ✅ Tested |
| 4. Optimization preserves behavior | release_hardening.rs | ✅ Tested |
| 5. Lowering preserves semantics | defer_lowering_test.rs | ✅ Tested |
| 6. Unsafe explicitly represented | AST design | ✅ By design |
| 7. Diagnostics preserve locations | CompileError struct | ⚠️ Partial |
| 8. Module loading independent | module_loader.rs | ✅ By design |
| 9. Backends independent | backend_trait_test.rs | ✅ Tested |
| 10. Verified IR only input | verify() in pipeline | ✅ By design |
### Invariant 11: List Printing Preserves Interpreter Format

`print(List<Float>)` in LLVM must match interpreter oracle:
`[%.1f, %.1f,...]\n` via `emit_print_list_*`.

Verified by `oracle_test::test_oracle_print_list_var`.
