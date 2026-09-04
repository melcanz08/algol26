# Semantics Hardening — Session Progress

**Date**: 2026-09-04
**Total Tests**: 147 passing, 0 failing
**Warnings**: 0

---

## COMPLETED This Session

### Tier 1: Quick Wins (3 fixes)

| # | Fix | File | Status |
|---|-----|------|--------|
| 1 | Document Ptr vs Pointer(T) | `common/types.rs` | ✅ DONE |
| 2 | Remove `into_program()` from VerifiedIR | `ir/verified_ir.rs` | ✅ DONE |
| 3 | Add `is_terminator()` method | `ir/semantic_ir.rs` | ✅ DONE |

**Details**:
- `Ptr` = opaque/raw pointer (FFI void*)
- `Pointer(T)` = typed pointer (*Int)
- `into_program()` removed — guarantee cannot be broken
- `is_terminator()` = canonical definition (Return/Jump/Branch/Switch)

---

### Tier 2: Type System Fixes (2 of 4 complete)

| # | Fix | File | Status |
|---|-----|------|--------|
| 1 | Remove Unknown from `can_coerce_to` | `common/types.rs` | ✅ DONE |
| 2 | Fix List function generics | `semantics/semantic.rs` | ✅ DONE |
| 3 | NullPtr proper variant | `ir/semantic_ir.rs` | ⏳ DEFERRED |
| 4 | PtrLiteral proper variant | `ir/semantic_ir.rs` | ⏳ DEFERRED |

**Details for #1**:
- Before: `(_, Unknown) => true` made Unknown act as 'any type'
- After: Unknown = 'not yet resolved', must be resolved before coercion

**Details for #2**:
- Before: `List<Int>` couldn't pass to `List<Unknown>` parameter
- After: List parameters are generic — accept any `List<T>`

**DEFERRED — NullPtr proper variant**:
- Currently: `NullPtr → Int(0)` in translator (semantically wrong)
- Should: Add `NullPtr { pointer_type: Type }` variant to IR
- Why deferred: Requires updating translator + 3 backends + tests
- Effort: 1-2 hours

**DEFERRED — PtrLiteral proper variant**:
- Currently: `PtrLiteral → Int` in translator (semantically wrong)
- Should: Add `PtrLiteral { address, pointer_type }` variant to IR
- Why deferred: Requires updating translator + 3 backends + tests
- Effort: 1-2 hours

---

### Tier 3: Semantic Verifier (1 of 11 complete)

| # | Fix | File | Status |
|---|-----|------|--------|
| 1 | Recursive Unknown rejection | `ir/semantic_verifier.rs` | ✅ DONE |
| 2 | Add Borrow/Deref/AddrOf to IR | `ir/semantic_ir.rs` | ❌ TODO |
| 3 | List element_type field | `ir/semantic_ir.rs` | ❌ TODO |
| 4 | None/Ok/Error contextual types | `ir/semantic_ir.rs` | ❌ TODO |
| 5 | add_instruction() returns Result | `semantics/control_flow.rs` | ❌ TODO |
| 6 | Unify CFG verifiers | `ir/semantic_ir.rs` + `ir/cfg_verifier.rs` | ❌ TODO |
| 7 | Reachability verification | `ir/cfg_verifier.rs` | ❌ TODO |
| 8 | Remove TypeVar coercion | `common/types.rs` | ❌ TODO |
| 9 | Consolidate FlowResult/BlockResult | `semantics/flow_result.rs` | ❌ TODO |
| 10 | UnaryOp (Not/Negate) | `frontend/ast.rs` | ❌ TODO |
| 11 | Module imports in Program struct | `frontend/ast.rs` | ❌ TODO |

**Details for #1 (DONE)**:
- `verify_value_no_unknown()` recursively checks for Type::Unknown
- Traverses: List, Some, Ok, Error, Cast, BinaryOp, Call
- 3 new tests: rejects Unknown, accepts Int, rejects nested Unknown

---

## Session Summary

| Tier | Completed | Deferred | Remaining |
|------|-----------|----------|-----------|
| Tier 1 | 3/3 | 0 | 0 |
| Tier 2 | 2/4 | 2 (NullPtr, PtrLiteral) | 0 |
| Tier 3 | 1/11 | 0 | 10 |
| **TOTAL** | **6** | **2** | **10** |

---

## Verification Commands

```bash
cargo build 2>&1 | grep -c 'warning'  # 0
cargo test 2>&1 | grep 'test result' | awk '{sum += \$4} END {print sum " tests passed"}'  # 147
```