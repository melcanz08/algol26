# ALGOL26 Changelog

## [0.8.0-hardened] - 2026-09-05 - Level 3 Production Hardened - Lenovo G560 (Opol)

### Hardening - Level 3 Complete

**From 240 prod unwraps / 148 clippy errors → 0 prod unwraps, clippy -D warnings CLEAN, 150/150 tests green.**

#### Enforcement Added - `src/lib.rs`
```rust
#![deny(clippy::panic)]
#![deny(clippy::unwrap_used)]
#![allow(clippy::expect_used)] // LLVM builder ICE only
```

#### Panic Surface Reduction
- **Before:** 240x `unwrap()` / `expect()` / `panic!()` in prod paths
- **After:** 13 ICE-only `expect("LLVM IR build failed - ICE")` in `src/backends/ir_codegen.rs`
- `cfg_verifier.rs:126` - `assert_valid` now `#[allow(clippy::panic)]` - intentional ICE
- All `parser.rs` / `verified_ir.rs` panics confined to `#[cfg(test)]`

#### Critical Fixes (G560)
- `lexer.rs:375-390` - `procedure`/`proc`/`function` decl: `strip_prefix().unwrap()` → safe len calc (manual_strip)
- `lexer.rs:683` - `handle_operator`: `chars.next().unwrap()` → `else { return Ok(()) }` (EOF crash - fuzz found)
- `lexer.rs` - `*indent_stack.last().unwrap_or(&0)` → `.last().copied().unwrap_or(0)`
- `ast.rs:308` - `List<Int>`: `s.find('<').unwrap()` → `let Some(open_pos) = s.find('<') else { return Unknown }`
- `semantics/semantic.rs:513` - `trimmed.chars().next().unwrap().is_uppercase()` → `is_some_and(|c| c.is_uppercase())`
- `common/types.rs, semantic_builder.rs, monomorphize.rs` - all prod `unwrap()` → `is_some_and` / `if let Some`

#### Verification
```
cargo clippy --lib -- -D warnings → LEVEL 3 CLEAN ✅
cargo test -- --test-threads=1 → 150 passed, 0 failed (43 unit + 16 backends + 14 differential + 2 frontend + 4 fuzz + 13 integration + 24 ir + 5 property + 29 semantics)
./tools/hardening_audit.sh → Count: 13 (ICE only)
git tag v0.8.0-hardened pushed to origin
```

#### What Level 3 Means
- No user input can trigger `panic!()` / `unwrap()` in prod paths
- Remaining `expect()`s are LLVM builder invariants - only fire on compiler bugs (ICE)
- Fuzz harness `test_fuzz_compiler_no_panic` passes 3.37s random input

#### Commit
`e1db260 - hardening Level 3 final: deny panic/unwrap_used, fix parse_type_param is_some_and, 150 tests green on G560`

---

## v1.3 – Standard Library Foundation (Current)

### Added
- `Math.sqrt(x)` registration
- `Math.pow(x, y)` registration
- `Math.sin(x)` registration
- `Math.cos(x)` registration
- `Math.abs(x)` registration
- Standard library module (`src/stdlib.rs`)

### Tests
- 18 unit tests passing
- 13 conformance programs passing
- Zero build warnings

### Known Limitations
- Math function calls compile and link, but return value extraction is not yet implemented (returns 0.0 placeholder)

## v1.2 – Type System 1.0

### Added
- `Option<T>` type (`Some(value)`, `None`)
- `Result<T, E>` type (`Ok(value)`, `Error(value)`)
- Type inference for Option/Result
- Semantic analysis for Option/Result

### Tests
- Type system tests passing
- Semantic validation tests passing

## v1.1 – Semantic Hardening

### Added
- Differential testing (LLVM output verification)
- Historical lineage documentation
- Honest status system (🟢🟡🔵🔴)
- Unique temp files for parallel tests

### Tests
- Differential tests passing
- Semantic validation tests passing
- Conformance suite 13/13 passing

## v1.0 – Language Specification & Formal Semantics

### Added
- Formal specification (`docs/formal-specification.md`)
- Semantic validation tests
- Final verification script

### Tests
- 12 unit tests passing
- 13 conformance programs passing

## v0.9 – Runtime Bounds & IR Coverage

### Added
- Runtime bounds checking for dynamic indices
- Complete IR instruction coverage
- Race detection foundation (`src/race.rs`)
- Interpreter backend (`src/interpreter.rs`)
- Better contextual error messages

### Tests
- Runtime bounds tests passing
- IR unit tests passing

## v0.8 – Safety & Conformance Hardening

### Added
- Conformance test suite (valid/invalid programs)
- Indentation validation
- String-safe comment handling
- Move vs copy semantics distinction

### Tests
- 13 conformance programs passing
- Use-after-move detection verified

## v0.7 – ALGOL26 IR Layer

### Added
- ALGOL26 IR (`src/ir.rs`)
- Backend-independent intermediate representation
- Debug IR output (`ALGOL26_DEBUG_IR=1`)

## v0.6 – Semantic Stabilization & Beautiful Diagnostics

### Added
- Error codes (E0001–E0009)
- Helpful suggestions in errors
- Clean Rust-style error format

## v0.5 – Concurrency Syntax

### Added
- `spawn` blocks
- `parallel do` blocks
- `channel` declarations
- `send` / `receive` syntax

## v0.4 – Advanced Safety

### Added
- `val` / `var` immutability
- Escape analysis foundation
- Region memory foundation

## v0.3 – Memory Safety

### Added
- Array indexing with bounds checking
- Compile-time bounds checking for literal indices
- Move semantics foundation
- Ownership tracking foundation

## v0.2 – Type System

### Added
- String variables
- Boolean operations (`and`, `or`)
- Comparison operators (`>=`, `<=`, `!=`)
- Correct operator precedence

## v0.1 – Initial Prototype

### Added
- Lexer with indentation
- Parser with AST
- LLVM code generation
- Basic arithmetic
- Loops and conditionals
- Type inference for numbers
