# Semantics Hardening — COMPLETE Progress & Remaining Tiers

**Date**: 2026-09-04
**Total Tests**: 150 passing, 0 failing
**Warnings**: 0

---

## TIER STATUS SUMMARY

| Tier | Description | Items | Done | Remaining | Status |
|------|-------------|-------|------|-----------|--------|
| Tier 1 | Quick Wins (15-30 min) | 3 | 3 | 0 | ✅ COMPLETE |
| Tier 2 | 1-Hour Fixes | 4 | 2 | 2 deferred | ⚠️ PARTIAL |
| Tier 3 | 1-2 Hour Fixes | 11 | 8 | 3 | ⚠️ IN PROGRESS |
| Tier 4 | 2-3 Hour Fixes | 5 | 5 | 0 | ✅ COMPLETE |
| Tier 5 | 3-4 Hour Fixes | 3 | 0 | 3 | ❌ NOT STARTED |
| Tier 6 | 4+ Hour Fixes | 2 | 0 | 2 | ❌ NOT STARTED |
| **TOTAL** | | **28** | **18** | **10** | **~64% done** |

---

## TIER 2 DEFERRED (2 items)

| # | Fix | File | Effort |
|---|-----|------|--------|
| 1 | NullPtr proper variant | `ir/semantic_ir.rs` | 1-2 hrs |
| 2 | PtrLiteral proper variant | `ir/semantic_ir.rs` | 1-2 hrs |

**Why deferred**: Both require adding new IR variants + updating translator + 3 backends + tests

---

## TIER 3 COMPLETED

| # | Fix | File | Status |
|---|-----|------|--------|
| 9 | UnaryOp (Not/Negate) | `frontend/ast.rs` + parser + all users | ✅ DONE |

**UnaryOp Implementation Details:**
- Added `UnaryOp` enum (Negate, Not) to AST
- Added `Expr::Unary` variant with span tracking
- Parser now creates proper Unary nodes instead of desugaring to Binary
- Semantic analyzer validates operand types (numeric for Negate, Bool for Not)
- Translator and SemanticBuilder handle Expr::Unary
- 147 tests passing, 0 warnings

---

## TIER 3 REMAINING (9 items)

| # | Fix | File | Effort |
|---|-----|------|--------|
| 1 | ~~Add Borrow/Deref/AddrOf to IR~~ | `ir/semantic_ir.rs` | ✅ DONE |
| 2 | ~~List element_type field~~ | `ir/semantic_ir.rs` | ✅ DONE |
| 3 | ~~None/Ok/Error contextual types~~ | `ir/semantic_ir.rs` | ✅ DONE |
| 4 | ~~add_instruction() callers propagate errors~~ | `semantics/control_flow.rs` + callers | ✅ DONE |
| 5 | Unify CFG verifiers | `ir/semantic_ir.rs` + `ir/cfg_verifier.rs` | 1-2 hrs |
| 6 | Reachability verification | `ir/cfg_verifier.rs` | 1-2 hrs |
| 7 | Remove TypeVar coercion | `common/types.rs` | 1-2 hrs |
| 8 | Consolidate FlowResult/BlockResult | `semantics/flow_result.rs` | 1-2 hrs |
| 9 | ~~UnaryOp (Not/Negate)~~ | `frontend/ast.rs` + parser + all users | ✅ DONE |
| 9 | ~~Module imports in Program struct~~ | `frontend/ast.rs` + parser + compiler | ✅ DONE |

---

## TIER 4 COMPLETE (5 items)

| # | Fix | File | Status |
|---|-----|------|--------|
| 1 | Branch/Return/Assign verification | `ir/semantic_verifier.rs` | ✅ DONE |
| 2 | Translator Cast/Borrow/ArrayAccess | `semantics/expr_translator.rs` | ✅ DONE |
| 3 | FlowAnalyzer no fake IDs | `semantics/flow_analyzer.rs` | ✅ DONE |
| 4 | ensure_block() Result + create_block() | `semantics/control_flow.rs` | ✅ DONE |
| 5 | Lexer stops knowing stdlib | `frontend/lexer.rs` | ✅ DONE |

---

## TIER 5 NOT STARTED (3 items)

| # | Fix | File | Effort |
|---|-----|------|--------|
| 1 | Translator If/Match/TryCatch | `semantics/expr_translator.rs` + `control_flow.rs` | 3-4 hrs |
| 2 | TypeSyntax replaces String types | `frontend/ast.rs` + parser + semantic | 3-4 hrs |
| 3 | Parser stops knowing FFI | `frontend/parser.rs` + `ffi/` | 3-4 hrs |

---

## TIER 6 NOT STARTED (2 items)

| # | Fix | File | Effort |
|---|-----|------|--------|
| 1 | VerifiedIR uses SemanticVerifier | `ir/verified_ir.rs` + `ir/semantic_verifier.rs` | 4-5 hrs |
| 2 | Unknown semantic split (Dynamic/Never/Uninferred) | `common/types.rs` + all users | 4-6 hrs |

---

## TOTAL REMAINING: 10 items, ~16-26 hours

## Verification Commands

```bash
cargo build 2>&1 | grep -c 'warning'  # Must be 0
cargo test 2>&1 | grep 'test result' | awk '{sum += \$4} END {print sum " tests passed"}'  # Must be 147+
```