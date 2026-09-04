# ALGOL26 Frontend Hardening — Backlog

**Date**: 2026-09-04
**Status**: 6 of 10 P0 fixes complete (60%)

---

## P0 Fixes COMPLETED (6)

| # | Fix | What Changed |
|---|-----|-------------|
| 1 | Silent `=` consumption | Errors: "use ':=' or '=='" |
| 2 | Silent `!` consumption | Errors: "use '!=' or 'not'" |
| 3 | Wildcard fallback for invalid patterns | Errors: "Expected pattern" |
| 4 | `unreachable!()` in parser | Error returns instead of panic |
| 5 | "unknown" sentinel | `.unwrap_or_default()` (empty string) |
| 6 | parse_block_with_trailing fallback | `match` with error return |

---

## P0 Fixes REMAINING (4)

### P0-A: Module imports in functions[0].body

**File**: `src/frontend/parser.rs` (parse_program)
**Problem**: Top-level imports are injected into `functions[0].body`
**Impact**: Module structure is wrong — imports belong to Program, not a function
**Effort**: 2-3 hours

**Current code**:
```rust
// parse_program() collects imports, then:
functions[0].body.insert(0, Stmt::Import { path });
```

**Fix**: Create a `Program` struct:
```rust
pub struct Program {
    pub imports: Vec<ImportDecl>,
    pub functions: Vec<FunctionDecl>,
    pub traits: Vec<TraitDecl>,
    pub impls: Vec<ImplBlock>,
}
```

**Steps**:
1. Create `Program` struct in `src/frontend/ast.rs`
2. Update `parse_program` to return `Program`
3. Update compiler pipeline to use `Program`
4. Update all test files that call `parse_program`
5. Update module_loader to work with `Program`

---

### P0-B: Span (0,0) hardcoded / unreliable

**File**: `src/frontend/parser.rs`, `src/frontend/lexer.rs`
**Problem**: Hardcoded `(0, 0)` spans, inaccurate token positions
**Impact**: Diagnostics point to wrong source locations
**Effort**: 2-3 hours

**Current issues**:
```rust
// Hardcoded span:
Expr::Var(name.clone(), (0, 0))

// Inaccurate column calculation:
let col_offset = i - token_count_before;
base_column + col_offset
```

**Fix**: Record actual cursor position during tokenization
```rust
pub struct Span {
    pub start: usize,  // byte offset
    pub end: usize,    // byte offset
}
```

**Steps**:
1. Add `byte_offset` tracking to lexer
2. Replace all `(usize, usize)` with `Span`
3. Remove all hardcoded `(0, 0)` spans
4. Add source map for byte→line/column conversion
5. Update CompileError to use Span

---

### P0-C: Unary operators as Binary

**File**: `src/frontend/parser.rs` (parse_unary)
**Problem**: `-x` becomes `0 - x`, `not x` becomes `x == false`
**Impact**: Semantic lowering happening in parser, wrong AST
**Effort**: 1-2 hours

**Current code**:
```rust
// -x:
Expr::Binary { left: Number(0.0), op: Subtract, right: x }

// not x:
Expr::Binary { left: x, op: Equal, right: Bool(false) }
```

**Fix**: Add `UnaryOp` enum:
```rust
pub enum UnaryOp {
    Negate,
    Not,
    Borrow,
    MutBorrow,
    Dereference,
    AddressOf,
}

pub enum Expr {
    Unary {
        op: UnaryOp,
        operand: Box<Expr>,
    },
}
```

**Steps**:
1. Add `UnaryOp` enum to `src/frontend/ast.rs`
2. Add `Expr::Unary` variant
3. Update `parse_unary` to use `Expr::Unary`
4. Update semantic analyzer for `Expr::Unary`
5. Update semantic_builder for `Expr::Unary`
6. Update code generators (LLVM, Interpreter, WASM)

---

### P0-D: Parser knows FFI C types

**File**: `src/frontend/parser.rs`
**Problem**: Parser imports `ffi::c::CType` and has `parse_c_type`
**Impact**: Architecture violation — parser shouldn't know FFI internals
**Effort**: 2-3 hours

**Current imports**:
```rust
use crate::ffi::c::{CType, FFIInfo};
use crate::frontend::lexer::CTypeName;
```

**Fix**: Parser should produce syntax-neutral AST for FFI declarations:
```rust
pub struct ExternDecl {
    pub abi: Option<String>,
    pub name: String,
    pub params: Vec<Parameter>,
    pub return_type: Option<TypeSyntax>,
}
```

**Steps**:
1. Create FFI-neutral AST in `src/frontend/ast.rs`
2. Parser produces `ExternDecl` without knowing C types
3. Semantic/FFI analysis converts `ExternDecl` to `FFIInfo`
4. Remove `CType` import from parser
5. Remove `CTypeName` from lexer (move to FFI module)

---

## P1 Fixes (After P0)

| # | Issue | Effort |
|---|-------|--------|
| 1 | Lexer knows stdlib (Math/File/List) | 2-3 hrs |
| 2 | For/While duplication (Stmt vs Expr) | 2-3 hrs |
| 3 | MatchCase/MatchCaseExpr duplication | 1-2 hrs |
| 4 | expect_token uses discriminants | 1 hr |
| 5 | parse_identifier_stmt too heuristic | 2-3 hrs |
| 6 | defer/alloc/free fake tokens | 1 hr |

---

## Recommended Order

1. **P0-A**: Module imports in Program struct (architectural correctness)
2. **P0-C**: UnaryOp enum (semantic correctness — simplest)
3. **P0-B**: Span integration (diagnostics accuracy)
4. **P0-D**: Parser stops knowing FFI (architecture)
5. Then P1 items

---

## Verification After Each Fix

```bash
cargo build 2>&1 | grep -c 'warning'  # Should be 0
cargo test 2>&1 | grep 'test result' | tail -1  # Should be 144 passed
```

Each fix MUST keep all 144 tests passing and zero warnings.