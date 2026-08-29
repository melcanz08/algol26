# ALGOL26 Historical Lineage

This document tracks which features come from ALGOL 58, which are ALGOL26 innovations, and which are modern inspirations.

## Lineage Map

```
ALGOL 58 (1958)
   │
   ├── procedure declarations
   ├── block structure
   ├── structured control flow
   ├── mathematical expressions
   ├── arrays
   └── type declarations
   │
   ▼
ALGOL26 (2026)
   │
   ├── significant indentation [Python/ISWIM]
   ├── type inference [Haskell/ML]
   ├── ownership [Rust]
   ├── move semantics [Rust]
   ├── immutability [Functional]
   ├── bounds checking [Modern safety]
   ├── concurrency [Go/Erlang]
   └── LLVM backend [Modern compiler]
```

## Feature Classification

### [ALGOL58] - Direct ALGOL Heritage

| Feature | ALGOL 58 | ALGOL26 | Notes |
|---------|----------|---------|-------|
| `procedure` keyword | Yes | Yes | Preserved |
| Assignment `:=` | Yes | Yes | Preserved |
| Mathematical expressions | Yes | Yes | Preserved |
| Block structure | Yes | Yes (indentation) | Adapted |
| Structured control flow | Yes | Yes | Preserved |
| Arrays | Yes | Yes (bounds-checked) | Enhanced |

### [ALGOL26] - Deliberate Innovations

| Feature | Origin | Rationale |
|---------|--------|-----------|
| `val` keyword | Functional languages | Immutability by default |
| `var` keyword | Modern languages | Explicit mutability |
| Type inference | Haskell/ML | Reduce boilerplate |
| Move semantics | Rust | Memory safety |
| Bounds checking | Modern safety | No buffer overflows |
| `spawn`/`parallel` | Go/Erlang | Safe concurrency |
| `channel`/`send`/`receive` | Go/Erlang | Message passing |

### [Modern] - Implementation Technology

| Component | Technology | Role |
|-----------|-----------|------|
| Lexer | Rust | Tokenization |
| Parser | Rust | AST construction |
| Semantic Analysis | Rust | Type/ownership checking |
| ALGOL26 IR | Custom | Backend-independent IR |
| LLVM Backend | LLVM | Native code generation |
| Interpreter | Rust | Direct execution |

## Design Philosophy

### What ALGOL26 Preserves from ALGOL 58:
- Clarity of algorithms
- Mathematical notation
- Structured programming
- Procedural abstraction
- Scientific computing orientation

### What ALGOL26 Changes:
- Indentation instead of begin/end
- Static typing with inference
- Memory safety by default
- Concurrency support
- Modern tooling

### What ALGOL26 Adds (2026):
- Ownership model
- Bounds checking
- Immutability
- Race detection
- Multi-backend support
- Beautiful diagnostics

## The Core Question

ALGOL26 asks:

> What would ALGOL have become if its original design
> philosophy had continued evolving for another 68 years?

Not:

> What if we modernized ALGOL syntax?

This distinction is fundamental to the project's identity.