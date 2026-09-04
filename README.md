# ALGOL26

**ALGOL 58 reimagined for 2026.**

A historically inspired systems programming language combining ALGOL's clarity with indentation, compile-time safety, deterministic resource management, safe concurrency, and modern native compilation.

> **Control without unsafe defaults.**

**Version**: v0.8.0
**Status**: Architecture Hardening COMPLETE — 144 tests, zero warnings
**License**: MIT
**Built with**: Rust 1.70+ + LLVM 18 (inkwell 0.7.1)

---

## Quick Start

```bash
# Build
cargo build
cargo test   # 144 tests, 23 suites, zero failures

# Run a program
./target/debug/algol26 examples/basic/test.gol

# Run comprehensive test
./target/debug/algol26 examples/systems/comprehensive_test/full_test.gol
examples/systems/comprehensive_test/full_test
# === ALL TESTS PASSED ===

# Release build
cargo build --release
```

## Example

```gol
function add(x: Int, y: Int) -> Int
    return x + y

function main() -> Int
    val result := add(5, 3)
    print result  # 8
    return 0
```

## Features

### Language
- Procedures and functions
- Type inference: Int/Float/Bool/String/List/Option/Result
- Arrays with bounds checking
- Immutability (`val`/`var`)
- Move/Copy semantics
- Borrow checking
- **Generics** (`<T>`) with monomorphization
- **Traits/Interfaces** with method dispatch
- **Pattern matching** (parsing complete)
- **FFI** (extern C functions)
- Modules/imports
- Error handling (try/catch/finally)
- Defer (LIFO cleanup)
- Concurrency syntax (spawn, parallel)

### Standard Library

| Module | Functions |
|--------|-----------|
| Math | sqrt, pow, sin, cos, abs, floor, ceil, exp, log, tan |
| String | concat, upper, lower, length, substring |
| File | read, write, append |
| List | length, sum, max, min |

### Backends

| Backend | Output | Status |
|---------|--------|--------|
| LLVM (IRCodeGen) | Native executable | ✅ Stable |
| Interpreter | Direct execution | ✅ Stable (semantic oracle) |
| WASM | .wasm module | ✅ Compilation (execution future) |

### Optimizer
- Constant propagation
- Constant folding
- Common Subexpression Elimination
- Dead Code Elimination

## Architecture

```
src/
├── common/      — types, diagnostics, span
├── frontend/    — lexer, parser, ast, module_loader
├── semantics/   — semantic analysis, type checker, traits, borrow
├── ir/          — semantic IR, verified IR, optimizer, lowering
├── backends/    — LLVM, WASM, Interpreter
├── runtime/     — region memory
└── ffi/         — C types, FFI registry

Pipeline:
Lex → Parse → Desugar → Expand Impl → Monomorphize → Type Check
→ Safety Check → Build IR → Verify → Optimize → Verify → Lower to Backend
```

## Testing (144 tests, 23 suites)

| Suite | Tests | Purpose |
|-------|-------|---------|
| Unit (lib.rs) | 40 | Types, lexer, parser, IR, FFI, span |
| Semantics | 29 | Borrow check, traits, validation |
| IR | 21 | Verification, optimization, defer |
| Backends | 16 | Independence, oracle, WASM |
| Differential | 14 | LLVM vs Interpreter vs WASM |
| Integration | 13 | Conformance, hardening |
| Property | 5 | No-panic on edge cases |
| Fuzz | 4 | 700 iterations, zero crashes |
| Frontend | 2 | FFI parsing |

## Documentation

| Document | Location |
|----------|----------|
| Architecture Contract | docs/compiler/algol26-contract.md |
| IR Pass Contracts | docs/compiler/ir-pass-contracts.md |
| Memory Model | docs/memory/memory-model.md |
| Language Freeze | docs/language/language-freeze.md |
| Versioning | docs/language/versioning.md |
| Formal Specification | docs/formal-specification/ |
| Architecture Inventory | docs/architecture/architecture-inventory.md |
| Decisions (ADRs) | docs/decisions/ |

## Safety Guarantees

| Guarantee | Status |
|-----------|--------|
| Type safety | ✅ Enforced |
| Immutability | ✅ Enforced |
| Bounds checking | ✅ Enforced |
| Use-after-move | ✅ Enforced |
| Borrow checking | ✅ Enforced |
| Trait bounds | ✅ Enforced |
| No-panic (fuzz) | ✅ 700 iterations |
| IR verification | ✅ VerifiedIR wrapper |

## Known Limitations

| Limitation | Target |
|------------|--------|
| Real threads (spawn sequential) | v0.9.0 |
| Pattern matching code gen | v0.8.1 |
| FFI type marshaling | v0.9.0 |
| Generic types (Stack<T>) | v0.9.0 |
| Standard library expansion | v0.9.0 |

## License

MIT — Rommel Edorot Caneos

## Acknowledgments

ALGOL 58 (inspiration) · Python (indentation) · Rust (implementation) · LLVM 18 (backend)