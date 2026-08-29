# ALGOL26 Changelog

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