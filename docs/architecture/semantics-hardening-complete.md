# ALGOL26 Semantics Hardening — Complete Backlog

**Date**: 2026-09-04
**Source**: Outsider's Semantic IR/Type System Review (34 issues)
**Status**: 0 of 34 issues complete

---

## P0 — CRITICAL (Must Fix Before VerifiedIR Is Real)

### 1. VerifiedIR verification is too weak

**File**: `src/ir/semantic_ir.rs`, `src/ir/verified_ir.rs`
**Problem**: `verify()` only checks block IDs and jump targets. Does NOT verify types.
**Fix**: Integrate `SemanticVerifier` into `VerifiedIR::new()`.
**Effort**: 2-3 hours (after rules below are implemented)

---

### 2. Recursive Type::Unknown rejection missing

**File**: `src/ir/semantic_verifier.rs`
**Problem**: `None`, `Ok(Unknown)`, `Error(Unknown)`, `List([])` can pass verification.
**Fix**: Add `verify_value_no_unknown()` recursive check.
**Effort**: 1-2 hours

---

### 3. Instruction type verification missing

**File**: `src/ir/semantic_verifier.rs`
**Problem**: Assign, Call, ArrayAccess, BinaryOp, Switch not verified.
**Fix**: Add verification rules for all 8 instruction types.
**Effort**: 2-3 hours

---

### 4. Translator silently discards information

**File**: `src/semantics/expr_translator.rs`
**Problems**:
- `Cast` → inner expression (target_type discarded)
- `Borrow`/`MutBorrow`/`Deref`/`AddrOf` → inner expression
- `ArrayAccess` → `Float(0.0)`
- `If` → then branch only
- `Match` → first case only
- `TryCatch` → try branch only
- `NullPtr` → `Int(0)`
- `PtrLiteral` → `Int`
**Fix**: These must preserve semantic meaning or return errors.
**Effort**: 3-4 hours

---

### 5. FlowAnalyzer fabricates fake block ID

**File**: `src/semantics/flow_analyzer.rs`
**Problem**: `FlowResult::Reachable(0)` — placeholder block ID.
**Fix**: Flow analysis should not allocate CFG IDs. Separate analysis from construction.
**Effort**: 2-3 hours

---

### 6. add_instruction() and ensure_block() silently fail

**File**: `src/semantics/control_flow.rs`
**Problem**: Invalid block ID → instruction silently dropped or block auto-created.
**Fix**: Return `Result<(), String>` on invalid block ID. No silent block creation.
**Effort**: 1-2 hours

---

## P1 — Type System Hardening

### 7. Type::Unknown does too many jobs

**File**: `src/common/types.rs`
**Problem**: Unknown means: not inferred, invalid, missing, empty list, None inner, etc.
**Fix**: Introduce distinct types: `Dynamic`, `Never`, `Uninferred`.
**Effort**: 2-3 hours

---

### 8. None/Ok/Error don't preserve contextual type

**File**: `src/ir/semantic_ir.rs`
**Problem**: `None` returns `Option<Unknown>`, `Ok(42)` returns `Result<Int, Unknown>`.
**Fix**: Add explicit type fields: `None { option_type: Type }`, `Ok { result_type: Type }`.
**Effort**: 2-3 hours

---

### 9. List element type not preserved for empty lists

**File**: `src/ir/semantic_ir.rs`
**Problem**: `List([])` returns `List<Unknown>`.
**Fix**: Add `element_type: Type` to List variant.
**Effort**: 1-2 hours

---

### 10. Unknown coerces to anything

**File**: `src/common/types.rs`
**Problem**: `(_, Type::Unknown) => true` makes Unknown act as `Any`.
**Fix**: Remove Unknown from can_coerce_to. Unknown must be resolved before coercion.
**Effort**: 1 hour

---

### 11. TypeVar coerces to anything

**File**: `src/common/types.rs`
**Problem**: TypeVar can coerce to/from any type without unification.
**Fix**: TypeVar compatibility must come from unification, not coercion.
**Effort**: 2 hours

---

### 12. Ptr vs Pointer(T) unclear

**File**: `src/common/types.rs`
**Problem**: Both exist but semantic difference not documented.
**Fix**: Document: `Ptr` = opaque/raw, `Pointer(T)` = typed pointer.
**Effort**: 30 minutes (documentation only)

---

## P1 — IR Representation

### 13. Borrow/Deref/AddrOf missing from TypedIRValue

**File**: `src/ir/semantic_ir.rs`
**Problem**: IR cannot represent these operations — translator discards them.
**Fix**: Add variants: `Borrow`, `MutBorrow`, `Deref`, `AddrOf`.
**Effort**: 2-3 hours

---

### 14. NullPtr and PtrLiteral not preserved

**File**: `src/ir/semantic_ir.rs`
**Problem**: NullPtr → Int(0), PtrLiteral → Int. Wrong semantics.
**Fix**: Add proper variants with pointer type information.
**Effort**: 1-2 hours

---

## P1 — Architecture

### 15. TWO CFG verifiers exist

**File**: `src/ir/semantic_ir.rs`, `src/ir/cfg_verifier.rs`
**Problem**: `SemanticProgram::verify()` and `SemanticCFGVerifier` duplicate logic.
**Fix**: One public verification entry point. CFG-specific logic in cfg_verifier.rs.
**Effort**: 2 hours

---

### 16. is_terminator() not unified

**File**: Multiple files
**Problem**: Terminator list duplicated in verifier and translator.
**Fix**: Add `SemanticInstruction::is_terminator()` method. Use everywhere.
**Effort**: 1 hour

---

### 17. Reachability verification missing

**File**: `src/ir/cfg_verifier.rs`
**Problem**: Dead blocks pass verification.
**Fix**: Add reachability analysis from entry block.
**Effort**: 2 hours

---

### 18. Empty blocks accepted

**File**: `src/ir/cfg_verifier.rs`
**Problem**: Block with no terminator and no instructions passes.
**Fix**: Every reachable block must end in exactly one terminator.
**Effort**: 1 hour

---

### 19. BlockResult vs FlowResult duplication

**File**: `src/semantics/control_flow.rs`, `src/semantics/flow_result.rs`
**Problem**: Two overlapping concepts.
**Fix**: Consolidate into one flow representation.
**Effort**: 2 hours

---

### 20. into_program() breaks VerifiedIR guarantee

**File**: `src/ir/verified_ir.rs`
**Problem**: Can extract mutable SemanticProgram, losing verification guarantee.
**Fix**: Remove `into_program()` or document that guarantee is lost.
**Effort**: 15 minutes

---

### 21. Type::from_str() as semantic parser

**File**: `src/common/types.rs`
**Problem**: Semantic types reconstructed from strings instead of TypeSyntax AST.
**Fix**: Introduce `TypeSyntax` in frontend, resolve to `Type` in semantics.
**Effort**: 3-4 hours

---

## Verification Commands

```bash
cargo build 2>&1 | grep -c 'warning'  # Must be 0
cargo test 2>&1 | grep 'test result' | tail -1  # Must be 144+ passed
```

---

## Recommended Order

1. Rules 1-3: Complete SemanticVerifier (type checking)
2. Rules 4-6: Fix translator information loss
3. Rules 7-12: Type system hardening
4. Rules 13-14: IR representation completeness
5. Rules 15-21: Architecture consolidation

---

## Total Estimated Effort: 35-45 hours