# Semantic Verifier — Progress & Backlog

**Date**: 2026-09-04
**Status**: Foundation created — 2 of 10 verification rules implemented

---

## COMPLETED (This Session)

### Created: `src/ir/semantic_verifier.rs`

Separate module for comprehensive IR verification.
Does NOT modify existing `SemanticProgram::verify()` — adds verification on top.

| # | Rule | What It Checks | Status |
|---|------|---------------|--------|
| 1 | Branch condition | `condition.type_of() == Type::Bool` | ✅ Done |
| 2 | Return type | `Return.type_ != Type::Unknown` | ✅ Done |
| 3 | Return value | `value.type_of() != Type::Unknown` | ✅ Done |

---

## REMAINING (Next Sessions)

### P0: Recursive Unknown Verification

**Goal**: Reject ANY Type::Unknown anywhere in validated IR

```rust
fn verify_value_no_unknown(value: &TypedIRValue, context: &str) -> Result<(), String> {
    if value.type_of() == Type::Unknown {
        return Err(format!("{}: Type::Unknown", context));
    }
    // Recursively check children...
}
```

**Why**: Currently `None`, `Ok(Unknown)`, `Error(Unknown)`, `List([])` can pass.
**Effort**: 1-2 hours

---

### P0: Instruction Verification Rules

| # | Instruction | Rule | Status |
|---|-------------|------|--------|
| 3 | `Assign` | value type matches target type | ❌ TODO |
| 4 | `ArrayAssign` | array=List, index=Int | ❌ TODO |
| 5 | `Declare` | value type matches declared type | ❌ TODO |
| 6 | `Call` | args match params, return type correct | ❌ TODO |
| 7 | `MethodCall` | receiver type correct, args match | ❌ TODO |
| 8 | `ArrayAccess` | array=List<T>, index=Int, element=T | ❌ TODO |
| 9 | `BinaryOp` | operands valid for operator | ❌ TODO |
| 10 | `Switch` | patterns match value type | ❌ TODO |

**Effort**: 2-3 hours

---

### P0: VerifiedIR Integration

**Goal**: `VerifiedIR::new()` uses `SemanticVerifier::verify()` instead of just `program.verify()`

```rust
// Before:
program.verify()?;  // Only structural

// After:
SemanticVerifier::verify(&program)?;  // Structural + Types
```

**Effort**: 15 minutes (after rules 3-10 are done)

---

## Architecture (Current vs Target)

```
CURRENT:
SemanticProgram
    │
    ├── verify()          → structural only (block IDs, jumps)
    │
    └── SemanticVerifier  → 2 type rules (Branch, Return)

TARGET:
SemanticProgram
    │
    └── SemanticVerifier::verify()
         ├── structural (block IDs, jumps, terminators)
         ├── types (recursive Unknown rejection)
         ├── instructions (Assign, Call, ArrayAccess, etc.)
         ├── functions (return type matches)
         └── values (recursive type check)
```

---

## Verification Commands

```bash
cargo build 2>&1 | grep -c 'warning'  # Must be 0
cargo test 2>&1 | grep 'test result' | tail -1  # Must be 144 passed
```

Each new verification rule MUST keep all 144 tests passing.
If a rule breaks tests, those tests were using invalid IR (that's a bug).