# ALGOL26 Architecture Inventory (Stage A)

**Date**: 2026-09-03
**Total Source**: 12,108 lines across 35 files

## File Size Analysis

### God Modules (>1000 lines) — PRIORITY CONCERN

| File | Lines | Concern | Action |
|------|-------|---------|--------|
| `semantic_builder.rs` | 2112 | Largest file. Mixes AST→IR translation, type inference, IR construction | Split into: `ast_to_ir.rs`, `ir_builder.rs` |
| `parser.rs` | 1548 | Too large. Handles all parsing | Split into: `parser.rs` (orchestration), `parse_expr.rs`, `parse_stmt.rs`, `parse_pattern.rs` |
| `semantic.rs` | 1320 | Growing god module | Extract: `type_inference.rs`, `borrow_check.rs`, `return_check.rs` |
| `ir_codegen.rs` | 1188 | Mixes IR→LLVM, stdlib registration, string/list operations | Split into: `llvm_codegen.rs`, `stdlib_llvm.rs` |

### Medium Files (200-1000 lines) — MONITOR

| File | Lines | Concern | Action |
|------|-------|---------|--------|
| `lexer.rs` | 853 | Growing. Contains tokenization + keyword table + signature parsing | Extract: `tokens.rs`, `keywords.rs` |
| `optimizer.rs` | 416 | OK but needs pass contracts | Document invariants |
| `interpreter.rs` | 401 | OK — good size for backend | Keep |
| `compiler.rs` | 386 | Becoming conductor but growing | Keep as conductor, move logic out |
| `types.rs` | 385 | **OVERLAP with semantic_type.rs** | Resolve overlap |
| `monomorphize.rs` | 368 | OK but needs explicit pass contract | Document |
| `loop_desugar.rs` | 307 | OK | Document |
| `ast.rs` | 273 | OK | Keep |
| `ffi.rs` | 244 | OK | Keep |
| `semantic_ir.rs` | 223 | **Should be MOST IMPORTANT file** | Document as canonical IR |
| `expr_translator.rs` | 218 | Overlap with semantic_builder? | Investigate |
| `cfg_verifier.rs` | 214 | OK | Keep |
| `race.rs` | 212 | OK | Keep |

### Small Files (<200 lines) — HEALTHY

| File | Lines | Status |
|------|-------|--------|
| `trait_registry.rs` | 150 | ✅ Good |
| `main.rs` | 153 | ✅ Good (CLI entry) |
| `type_checker.rs` | 156 | ✅ Good |
| `diagnostics.rs` | 107 | ⚠️ Needs expansion for unified diagnostics |
| `defer_lowering.rs` | 83 | ✅ Good |
| `wasm_backend.rs` | 81 | ✅ Good |
| `module_loader.rs` | 78 | ✅ Good |
| `escape.rs` | 77 | ✅ Good |
| `region.rs` | 77 | ✅ Good |
| `region_memory.rs` | 76 | ✅ Good |
| `control_flow.rs` | 76 | ✅ Good |
| `backend.rs` | 67 | ✅ Good — small trait |
| `flow_result.rs` | 66 | ✅ Good |
| `interpreter_backend.rs` | 54 | ✅ Good |
| `llvm_backend.rs` | 48 | ✅ Good |
| `flow_analyzer.rs` | 47 | ⚠️ Very small — is it complete? |
| `lib.rs` | 36 | ✅ Good |
| `semantic_type.rs` | 8 | ❌ **Just a re-export of types.rs** |

## Critical Issues Found

### 1. `semantic_type.rs` (8 lines) is REDUNDANT

```rust
// semantic_type.rs
pub use crate::types::Type as SemanticType;
```

**Problem**: Creates `SemanticType` alias confusingly used alongside `Type`.
**Action**: Delete `semantic_type.rs`. Update all `SemanticType` → `Type`.

### 2. `types.rs` vs `semantic_type.rs` OVERLAP

**Current**: `semantic_type.rs` just re-exports `types::Type`.
**Problem**: Two names for the same thing creates confusion.
**Action**: Consolidate to single `types.rs`. Remove `semantic_type.rs`.

### 3. Root Directory Has Historical Scripts

```
add_runtime_bounds.py
apply_critical_fixes.py
check_project.sh
final_verification.sh
run_conformance.sh
verify_v11.sh
```

**Action**: Move to `tools/` or delete if historical.

### 4. Generated Artifacts in `examples/`

```
examples/*/main (binary)
examples/*/main.ll (LLVM IR)
examples/*/weather (binary)
examples/*/weather.ll
temperature_report.txt
weather_report.txt
```

**Action**: Add to `.gitignore` or move to `build/`.

## Dependency Graph (Current)

```
ast.rs
  ↓
lexer.rs → parser.rs → ast.rs
  ↓
compiler.rs
  ├── module_loader.rs
  ├── loop_desugar.rs
  ├── monomorphize.rs
  ├── semantic.rs
  │   ├── type_checker.rs
  │   ├── types.rs (semantic_type.rs re-export)
  │   ├── flow_analyzer.rs
  │   ├── race.rs
  │   ├── escape.rs
  │   ├── control_flow.rs
  │   └── trait_registry.rs
  ├── semantic_builder.rs
  │   └── semantic_ir.rs
  ├── expr_translator.rs
  ├── defer_lowering.rs
  ├── optimizer.rs
  ├── cfg_verifier.rs
  └── backend.rs
      ├── llvm_backend.rs → ir_codegen.rs
      ├── interpreter_backend.rs → interpreter.rs
      └── wasm_backend.rs → ir_codegen.rs

region.rs / region_memory.rs (partially orphaned?)
ffi.rs (parsed but execution incomplete)
```

## Recommended Stage A Actions

### Immediate (This Week)

1. **Delete `semantic_type.rs`** — Replace all `SemanticType` → `Type`
2. **Move scripts to `tools/`** — Clean root directory
3. **Add `.gitignore` for generated artifacts** — Keep examples/ clean
4. **Document Semantic IR as canonical** — Add header comment
5. **Remove debug eprintln from all source** — Already done ✅

### Short-term (Next 2 Weeks)

6. **Split `semantic_builder.rs` (2112 lines)** — Into `ast_to_ir.rs` and `ir_helpers.rs`
7. **Split `parser.rs` (1548 lines)** — Into sub-parsers
8. **Extract from `semantic.rs` (1320 lines)** — Borrow check and type inference
9. **Document optimizer pass contracts** — Input/output invariants
10. **Add IR verifier after each pass** — Extend `cfg_verifier.rs`

### Medium-term (Next Month)

11. **Unify diagnostics** — All through `CompileError` with error codes
12. **Add source spans to IR nodes** — For better errors
13. **Establish backend independence tests** — Verify no backend leaks into frontend
14. **Profile compiler performance** — Identify hotspots
15. **Add fuzzing for parser and lexer** — No panic on invalid input

## Architecture Maturity Score

| Component | Score (0-10) | Notes |
|-----------|-------------|-------|
| Lexer | 7/10 | Large but functional |
| Parser | 6/10 | Too large, needs splitting |
| AST | 8/10 | Clean |
| Type System | 7/10 | Overlap with semantic_type |
| Semantic Analysis | 6/10 | Growing god module |
| Semantic IR | 7/10 | Needs explicit contract |
| Monomorphization | 7/10 | Works, needs documentation |
| Trait Registry | 8/10 | Clean separation |
| Optimizer | 6/10 | Works but no explicit contracts |
| LLVM Backend | 7/10 | Good but large |
| Interpreter | 8/10 | Clean, good for testing |
| WASM Backend | 6/10 | Thin wrapper, needs expansion |
| Diagnostics | 5/10 | Inconsistent, needs unification |
| Testing | 8/10 | 105 tests, good coverage |
| Documentation | 6/10 | Multiple docs, possible overlap |

## Next Action

After this inventory, proceed to **Stage B: Dependency Cleanup**.