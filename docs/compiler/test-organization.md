# ALGOL26 Test Organization

**Effective**: v0.8.0

## Test Suite → Architecture Layer Mapping

| Test File | Architecture Layer | What It Tests |
|-----------|-------------------|---------------|
| `architecture_test.rs` | Architecture | Flow analyzer, type checker invariants |
| `backend_independence.rs` | Backend | Semantic IR is backend-independent |
| `backend_trait_test.rs` | Backend | Backend trait contract |
| `borrow_checker_test.rs` | Semantic | Ownership/borrowing rules |
| `conformance_test.rs` | Integration | Valid/invalid programs compile correctly |
| `defer_lowering_test.rs` | Lowering | Defer statements lower correctly |
| `diagnostic_test.rs` | Diagnostics | Error codes, CompileError format |
| `differential_test.rs` | Integration | LLVM vs Interpreter produce same output |
| `differential_true.rs` | Integration | Extended differential tests |
| `expr_translator_test.rs` | IR | Expression translation to IR |
| `ffi_test.rs` | Frontend | FFI parsing |
| `ir_verification_test.rs` | IR | IR verify() catches invalid IR |
| `optimizer_test.rs` | Optimizer | Constant folding, DCE |
| `release_hardening.rs` | Integration | Stress tests, negative tests |
| `semantic_validation.rs` | Semantic | Type safety, bounds, immutability |
| `trait_method_test.rs` | Semantic | Trait registry, method resolution |
| `type_system_test.rs` | Semantic | Type constructors, coercion |
| `type_unification_test.rs` | Semantic | Type unification rules |
| `wasm_backend_test.rs` | Backend | WASM backend trait contract |

## Test Pyramid

```
                    ┌──────────────────┐
                    │  End-to-End (2)  │  conformance_test, release_hardening
                    └────────┬─────────┘
                             │
                 ┌───────────┴───────────┐
                 │ Differential (2)      │  differential_test, differential_true
                 └───────────┬───────────┘
                             │
             ┌───────────────┴───────────────┐
             │ IR Tests (2)                  │  ir_verification, expr_translator
             └───────────────┬───────────────┘
                             │
         ┌───────────────────┴───────────────────┐
         │ Semantic Tests (6)                    │  borrow_checker, semantic_validation,
         │                                       │  trait_method, type_system,
         │                                       │  type_unification, architecture_test
         └───────────────────┬───────────────────┘
                             │
         ┌───────────────────┴───────────────────┐
         │ Unit Tests (in-file #[cfg(test)])     │  lexer, parser, types, trait_registry
         └───────────────────────────────────────┘
```

## Test Counts by Layer

| Layer | Test Files | Total Tests |
|-------|-----------|-------------|
| End-to-End | 2 | ~10 |
| Differential | 2 | ~12 |
| IR | 2 | ~8 |
| Semantic | 6 | ~30 |
| Unit (in-file) | 4 | ~31 |
| Backend | 3 | ~12 |
| Diagnostics | 1 | ~3 |
| Lowering | 1 | ~8 |
| **TOTAL** | **22** | **~114** |

## Adding a New Test

1. Identify which layer the test belongs to
2. Add to the appropriate test file
3. If no file exists for that layer, create one
4. Follow naming convention: `test_<feature>_<scenario>`
5. Always include a comment explaining what invariant is being tested