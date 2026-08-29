# ALGOL26

**ALGOL 58 reimagined for 2026.**

A historically inspired systems programming language combining ALGOL's clarity with indentation, compile-time safety, deterministic resource management, safe concurrency, and modern native compilation.

> **Control without unsafe defaults.**

**Version**: v0.1.0 (First Public Release)  
**Status**: Compiler construction complete. Language specification frozen.  
**License**: MIT  
**Built with**: Rust + LLVM

---

## Table of Contents

- [Quick Start](#quick-start)
- [Example](#example)
- [Features](#features)
- [Installation](#installation)
- [Documentation](#documentation)
- [Testing](#testing)
- [Safety Guarantees](#safety-guarantees)
- [Architecture](#architecture)
- [Project Structure](#project-structure)
- [Contributing](#contributing)
- [Changelog](#changelog)
- [License](#license)

---

## Quick Start

```bash
# Clone
git clone https://github.com/YOUR_USERNAME/algol26.git
cd algol26

# Build
cargo build --release

# Run a program
./target/release/algol26 run examples/hello.gol

# Compile to WASM
./target/release/algol26 wasm examples/hello.gol

# Type-check only
./target/release/algol26 check examples/hello.gol
```

## Installation

```bash
# Install from source
cargo install --path .

# Verify installation
algol26 --version
algol26 --help
```

### Requirements

- Rust 1.70+ (edition 2021)
- LLVM 17 (via inkwell)
- Clang (for linking native executables)

## Example

```gol
function add(x: float, y: float) -> float
    return x + y

procedure main
    val result := add(5.0, 3.0)
    Terminal.print(result)
    
    val greeting := "Hello from ALGOL26!"
    Terminal.print(greeting)
```

## Features

### Language

- Procedures and functions with parameters
- Type inference with Int/Float/Bool/String/List
- Arrays with bounds checking
- Immutability (val/var)
- Move semantics
- Module system (imports)
- Error handling (try/catch/finally)
- Defer statements (LIFO)
- Concurrency syntax (spawn, parallel)

### Standard Library (22 functions)

| Module | Functions | Status |
|--------|-----------|--------|
| Math | 10 | ✅ Implemented |
| String | 5 | ✅ Implemented |
| File | 3 | ✅ Implemented |
| List | 4 | 🟡 Registered |

### Safety

- Compile-time type checking
- Bounds checking (literal + dynamic)
- Immutability enforcement
- Use-after-move detection
- Race detection (write-write conflicts)

### Backends (3)

| Backend | Output | Status |
|---------|--------|--------|
| LLVM | Native executable | ✅ Stable |
| Interpreter | Direct execution | ✅ Stable |
| WASM | .wasm module | 🟡 Baseline |

### Architecture

- Backend-independent Semantic IR
- Backend trait for extensibility
- Optimizer (constant folding + DCE)
- Module loader with circular import detection
- 25+ focused source modules

## Documentation

| Document | Description |
|----------|-------------|
| [Formal Specification](docs/formal-specification.md) | Frozen language specification |
| [Language Reference](docs/language-reference.md) | Syntax and features |
| [Architecture](docs/architecture.md) | Compiler pipeline and modules |
| [Current Status](docs/current-status.md) | Project status and roadmap |
| [Vision](docs/vision.md) | Project vision |
| [Design Principles](docs/design-principles.md) | Core design philosophy |
| [Historical Lineage](docs/historical-lineage.md) | ALGOL heritage |
| [Safety Roadmap](docs/safety-roadmap.md) | Safety guarantee plans |
| [Design Decisions](docs/decisions/) | Architecture Decision Records |

## Testing (70 tests)

```bash
# Run all tests
cargo test --release

# Run conformance suite (25 programs)
./run_conformance.sh

# Run final verification
./final_verification.sh
```

### Test Suites

| Suite | Tests | Purpose |
|-------|-------|---------|
| Unit tests | 11 | Individual components |
| Backend independence | 4 | IR backend-agnostic |
| Defer lowering | 8 | All control-flow exits |
| Backend trait | 4 | Backend contract |
| Architecture | 4 | Refactored modules |
| Expression translator | 4 | Expression → IR |
| Conformance | 2 | Valid/invalid programs |
| Differential | 12 | Backend equivalence |
| Semantic validation | 5 | Safety guarantees |
| Type system | 2 | Option/Result types |
| WASM backend | 4 | WASM trait contract |
| Optimizer | 2 | Constant folding + DCE |
| Release hardening | 8 | Negative + stress tests |

## Safety Guarantees

| Guarantee | Status | Evidence |
|-----------|--------|----------|
| Type safety | 🟢 Proven | Compile-time |
| Immutability | 🟢 Proven | Compile-time |
| Bounds (literal) | 🟢 Proven | Compile-time |
| Bounds (dynamic) | 🟢 Proven | Runtime check |
| Use-after-move | 🟢 Proven | Compile-time |
| Race (write-write) | 🟢 Detected | Compile-time |
| No undefined behavior | 🔴 Not proven | Formal proof needed |

## Architecture

```
Source (.gol)
    ↓
Lexer → Parser → AST
    ↓
Semantic Analysis
    ├── TypeChecker
    ├── FlowAnalyzer
    ├── ExprTranslator
    └── ControlFlow
    ↓
Semantic IR (backend-independent)
    ↓
Optimizer (constant folding + DCE)
    ↓
┌─────────────┬──────────────┬─────────────┐
│ LLVM Backend│ Interpreter  │ WASM Backend│
│ Native Code │ Direct Exec  │ .wasm Module│
└─────────────┴──────────────┴─────────────┘
```

## Project Structure

```
algol26/
├── src/
│   ├── main.rs              # CLI entry point
│   ├── lib.rs               # Library interface
│   ├── lexer.rs             # Tokenizer
│   ├── parser.rs            # Parser
│   ├── ast.rs               # AST definitions
│   ├── semantic.rs          # Semantic analyzer
│   ├── type_checker.rs      # Type validation
│   ├── flow_analyzer.rs     # CFG analysis
│   ├── expr_translator.rs   # Expression → IR
│   ├── control_flow.rs      # Block management
│   ├── semantic_ir.rs       # Backend-independent IR
│   ├── optimizer.rs         # Constant folding + DCE
│   ├── backend.rs           # Backend trait
│   ├── llvm_backend.rs      # LLVM code generation
│   ├── wasm_backend.rs      # WASM code generation
│   ├── interpreter_backend.rs # Direct execution
│   ├── codegen.rs           # LLVM specifics
│   ├── module_loader.rs     # Import resolution
│   ├── stdlib.rs            # Math functions
│   ├── string_module.rs     # String operations
│   ├── file_module.rs       # File I/O
│   ├── list_module.rs       # List operations
│   ├── race.rs              # Race detection
│   ├── defer_lowering.rs    # Defer lowering
│   └── diagnostics.rs       # Error reporting
├── docs/                    # Documentation (10+ files)
├── tests/                   # 70 tests across 16 suites
├── examples/                # Example programs
└── Cargo.toml
```

## Contributing

Contributions are welcome! Please:

1. Fork the repository
2. Create a feature branch
3. Make your changes
4. Add tests if applicable
5. Run `cargo test` to verify
6. Submit a pull request

### Development Workflow

```bash
# Build
cargo build --release

# Run all tests
cargo test

# Run conformance
./run_conformance.sh
```

## Changelog

See [CHANGELOG.md](CHANGELOG.md) for version history.

## License

MIT — Rommel Edorot Caneos

---

## Acknowledgments

- **ALGOL 58** — For the historical inspiration
- **ISWIM** — For the influence on design
- **Python** — For indentation-based blocks
- **Rust** — For the implementation language
- **LLVM** — For the backend infrastructure