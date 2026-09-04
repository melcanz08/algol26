# ALGOL26 Backend Refactoring Roadmap

## Phase 1: Core IR-based LLVM Backend

**Goal:** LLVM backend consumes SemanticProgram instead of AST.

| Step | Task | File(s) | Status |
|------|------|---------|--------|
| 1.1 | Rewrite ir_codegen.rs to consume SemanticProgram | src/ir_codegen.rs | ✅ Done |
| 1.2 | Update llvm_backend.rs to use new IRCodeGen | src/llvm_backend.rs | ✅ Done |
| 1.3 | Update compiler.rs lower_to_llvm to use IRCodeGen | src/compiler.rs | ✅ Done |
| 1.4 | Test: all conformance + differential tests pass | tests/ | ✅ Done |
| **Exit criteria** | Full test suite green with IR-based LLVM backend | | ✅ MET |

**Additional work completed in Phase 1:**

- For loop unrolling (literal lists + variables + function calls)
- break/continue in unrolled loops (top-level + nested in if)
- Bounds checking in SemanticBuilder
- String.to_upper/to_lower/substring (char loops with malloc)
- File.write/read/append (fopen + NULL check + fclose)
- List.length/sum/max/min (compile-time evaluation)
- Spawn/Parallel (sequential fallback with correct control flow)
- Optimizer reachability for all instruction types
- GEP byte-level indexing for string operations
- -lm and -lpthread linker flags

---

## Phase 2: Interpreter Backend Refactoring

**Goal:** Interpreter consumes SemanticProgram instead of old IRProgram.

| Step | Task | File(s) | Status |
|------|------|---------|--------|
| 2.1 | Rewrite interpreter.rs to consume SemanticProgram | src/interpreter.rs | ❌ Not started |
| 2.2 | Update interpreter_backend.rs to use new interpreter | src/interpreter_backend.rs | ❌ |
| 2.3 | Test: differential tests pass (LLVM vs Interpreter) | tests/differential* | ❌ |
| **Exit criteria** | Both backends produce identical results from same IR | | ❌ |

---

## Phase 3: WASM Backend Refactoring

**Goal:** WASM backend consumes SemanticProgram instead of AST.

| Step | Task | File(s) | Status |
|------|------|---------|--------|
| 3.1 | Rewrite wasm_backend.rs to consume SemanticProgram | src/wasm_backend.rs | ❌ Not started |
| 3.2 | Test: WASM trait contract tests pass | tests/wasm_backend_test.rs | ❌ |
| **Exit criteria** | WASM backend works from same IR | | ❌ |

---

## Phase 4: Cleanup

**Goal:** Remove legacy code.

| Step | Task | Status |
|------|------|--------|
| 4.1 | Delete src/ir.rs (old IR) | ❌ |
| 4.2 | Delete src/codegen.rs (AST-based LLVM) | ❌ |
| 4.3 | Verify no references to old IR remain | ❌ |
| **Exit criteria** | cargo build + cargo test clean with zero legacy files | ❌ |

---

## Phase 5: Future Optimizations

| Task | Benefit | Status |
|------|---------|--------|
| Constant propagation pass | Extend optimizer to propagate constants | ❌ |
| Common subexpression elimination | Reduce redundant computation | ❌ |
| Better error reporting (line/column) | Debuggability | ❌ |
| Real threads for spawn/parallel | True parallelism | ❌ |
| Real channel synchronization | Thread-safe communication | ❌ |