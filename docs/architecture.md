# ALGOL26 Compiler Architecture

**Version**: v2.5.0 (Frozen)

## Pipeline Overview

```
ALGOL26 SOURCE (.gol)
       │
       ▼
    Lexer (src/lexer.rs)
    - Tokenizes source
    - Handles indentation
       │
       ▼
    Parser (src/parser.rs)
    - Builds AST
    - Handles control flow, modules, errors
       │
       ▼
    AST (src/ast.rs)
    - Expression trees
    - Statement trees
       │
       ▼
    Semantic Analysis
    ├── TypeChecker (src/type_checker.rs)
    ├── FlowAnalyzer (src/flow_analyzer.rs)
    ├── ExprTranslator (src/expr_translator.rs)
    └── ControlFlow (src/control_flow.rs)
       │
       ▼
    Semantic IR (src/semantic_ir.rs)
    - Backend-independent representation
       │
       ▼
    Optimizer (src/optimizer.rs)
    - Constant folding
    - Dead code elimination
       │
       ▼
    ┌─────────────────────────────────────────┐
    │              Backends                   │
    │        (src/backend.rs - trait)         │
    ├─────────────────────────────────────────┤
    │  LlvmBackend (src/llvm_backend.rs)      │
    │  InterpreterBackend                     │
    │  WasmBackend (src/wasm_backend.rs)      │
    └─────────────────────────────────────────┘
       │              │              │
       ▼              ▼              ▼
  Native Exec  Direct Exec    .wasm Module
```

## Module Responsibilities

| Module | Responsibility | Status |
|--------|---------------|--------|
| `lexer.rs` | Tokenize source, indentation | ✅ Complete |
| `parser.rs` | Build AST from tokens | ✅ Complete |
| `ast.rs` | AST data structures | ✅ Complete |
| `semantic.rs` | Type checking, scoping | ✅ Complete |
| `type_checker.rs` | Type validation, coercion | ✅ Complete |
| `flow_analyzer.rs` | CFG termination | ✅ Complete |
| `expr_translator.rs` | Expression → IR | ✅ Complete |
| `control_flow.rs` | Control flow blocks | ✅ Complete |
| `semantic_builder.rs` | AST → IR orchestration | ✅ Complete |
| `semantic_ir.rs` | Backend-independent IR | ✅ Stable |
| `optimizer.rs` | Constant folding + DCE | 🟡 Initial |
| `backend.rs` | Backend trait contract | ✅ Stable |
| `llvm_backend.rs` | LLVM code generation | ✅ Working |
| `wasm_backend.rs` | WASM generation | 🟡 Baseline |
| `interpreter_backend.rs` | Direct execution | ✅ Working |
| `codegen.rs` | LLVM-specific code | ✅ Working |
| `stdlib.rs` | Math functions | ✅ Complete |
| `string_module.rs` | String operations | ✅ Complete |
| `file_module.rs` | File I/O | ✅ Complete |
| `list_module.rs` | List operations | 🟡 Registered |
| `module_loader.rs` | Import resolution | ✅ Implemented |
| `race.rs` | Race detection | 🟡 Foundation |
| `defer_lowering.rs` | Defer lowering | ✅ Implemented |

## Backend Contract

```rust
pub trait Backend {
    fn compile(&self, ir: &SemanticProgram, output_name: &str) -> Result<BackendOutput>;
    fn name(&self) -> &str;
    fn description(&self) -> &str;
    fn can_execute(&self) -> bool;
}
```

## Backend Comparison

| Backend | Output | Language Parity | Status |
|---------|--------|-----------------|--------|
| LLVM | Native executable | Full | ✅ Stable |
| Interpreter | Direct execution | Full | ✅ Stable |
| WASM | .wasm module | Baseline | 🟡 Working |

## Test Architecture (70 tests)

| Suite | Tests |
|-------|-------|
| Unit tests | 11 |
| Backend independence | 4 |
| Defer lowering | 8 |
| Backend trait | 4 |
| Architecture | 4 |
| Expression translator | 4 |
| Conformance | 2 |
| Differential (true) | 8 |
| Differential (original) | 4 |
| Semantic validation | 5 |
| Type system | 2 |
| WASM backend | 4 |
| Optimizer | 2 |
| Release hardening | 8 |
| **Total** | **70** |