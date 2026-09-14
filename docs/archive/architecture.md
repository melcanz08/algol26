# ALGOL26 Compiler Architecture

**Version**: v0.8.0

This document describes the compiler as it exists today. The
pipeline below is the one implemented in `src/compiler.rs`; the
module paths are the real ones. Where a backend or pass does not
support a feature, that is stated explicitly.

For the language itself, see `docs/language/language-reference.md`.
For historical design decisions, see `docs/decisions/`.

---

## Pipeline Overview

The compile phase in `src/compiler.rs` runs thirteen phases in
order. The diagram below maps each phase to the module that
implements it.

```
Source (.gol)
  |
  v
Phase  1  Lex        src/frontend/lexer.rs
                     tokenize source; emit Indent/Dedent markers
  |
  v
Phase  2  Parse      src/frontend/parser.rs
                     build AST from tokens
  |
  v
AST                  src/frontend/ast.rs
                     expression and statement trees, each
                     node carrying a Span
  |
  v
Phase  3  Imports    src/frontend/module_loader.rs
                     inline imported files; detect cycles
  |
  v
Phase  4  Desugar    src/ir/loop_desugar.rs
                     unroll small known-length loops;
                     resolve list literals; fold constant ifs
  |
  v
Phase  5  Expand     compiler.rs (expand_impl_methods)
                     trait impl methods become functions with
                     a leading `self` parameter
  |
  v
Phase  6  Mono       src/ir/monomorphize.rs
                     specialize generic functions per call site
  |
  v
Phase  7  Analyze    src/semantics/semantic.rs
                     type inference, ownership, borrows,
                     trait resolution; produces a type table
                     keyed by AST node address
  |
  v
Phase  8  Safety     src/semantics/race.rs
                     syntactic access-conflict detection
                     over spawn/parallel blocks
  |
  v
Phase  9  IR Build   src/semantics/semantic_builder.rs
                     construct a CFG from the typed AST;
                     consume the analyzer's type table
  |
  v
Phase 10  Verify     src/ir/cfg_verifier.rs
                     src/ir/semantic_verifier.rs
                     structural + instruction-level checks
  |
  v
Phase 11  Optimize   src/ir/optimizer.rs
                     constant folding, DCE, branch
                     simplification
  |
  v
Phase 12  Verify     (same verifiers as phase 10)
                     re-run after optimization
  |
  v
Phase 13  Lower      src/backends/
                     LLVM, interpreter, or WASM
```

---

## Layer Description

### Frontend

The lexer and parser produce an AST that preserves source spans.
Every token carries a (line, column) pair; the parser threads those
into `Span` fields on AST nodes. The semantic analyzer uses these
spans for diagnostics.

Indentation is handled lexically: the lexer emits `Indent` and
`Dedent` tokens as the indent depth changes, and the parser treats
those as block boundaries. Mixed tabs and spaces are a lexical
error.

### Semantic Analysis

`SemanticAnalyzer` (`src/semantics/semantic.rs`) is the single type
and ownership authority. It is responsible for:

- Type inference and unification
- Ownership tracking (moves, `Copy` vs. non-`Copy`)
- Borrow checking (lexical, no NLL)
- Trait resolution and impl validation
- Null-binding tracking (statics for statically-null deref)
- A short-circuit-aware visit that mirrors the IR builder's
  structure so the type table matches what the IR will produce

The analyzer writes a **type table** into a `HashMap<usize, Type>`,
keyed by the address of each `Expr` node in the AST. Downstream
passes query this table instead of re-inferring types.

### Type Table Invariant

Because the table is keyed by node address, the IR builder must
not clone AST nodes. Every pass that reads the AST takes `&Expr`
or `&[Stmt]`; cloning produces new heap allocations with new
addresses and the table lookups miss. This invariant is enforced
by using `Cow<[Stmt]>` where a value-producing expression may or
may not need to synthesize a branch, but never by cloning an
`Expr` in place.

### Intermediate Representation

The IR is a CFG: `SemanticProgram` -> `SemanticFunction` ->
`SemanticBlock` -> `[Instruction]` + `Option<Terminator>`.
Blocks are identified by `usize` ids assigned by the builder.

Instructions include `Declare`, `Assign`, `ArrayAssign`, `Print`,
`Call`, `MethodCall`, `IteratorInit`, `ChannelDecl`, `Send`,
`Receive`, `Allocate`, `Free`.

Terminators are `Return`, `Jump`, `Branch`, `Switch`,
`IteratorNext`, `Spawn`, `Fork`, `Defer`. The `Defer` variant is
vestigial: defers are chained directly at return time by the IR
builder, so `Terminator::Defer` never appears in built IR.

### Verification

Two verifiers run in sequence:

- **`cfg_verifier.rs`** -- structural: every block has a
  terminator, every target resolves, no unreachable blocks, block
  ids are unique per function, function names are unique.
- **`semantic_verifier.rs`** -- instruction-level: `Declare`
  values coerce to their declared type, `Assign` targets are
  mutable and compatible, `Call` resolves to a known signature
  and matches its argument count and types, `Branch` conditions
  are `Bool`, `Switch` values are non-`Void`, and each value
  node's self-described type matches what its operands imply.

`VerifiedIR` (`src/ir/verified_ir.rs`) is a wrapper type whose
constructor runs the verifier. A `VerifiedIR` value can only
exist if verification succeeded. All backend entry points take
`&VerifiedIR`, so no unverified IR reaches codegen.

### Backends

Three backends implement the `Backend` trait in
`src/backends/backend.rs`:

| Backend       | Source files                                    | Output              |
|---------------|-------------------------------------------------|---------------------|
| LLVM          | `llvm_backend.rs`, `ir_codegen.rs`              | Native executable   |
| Interpreter   | `interpreter_backend.rs`, `interpreter.rs`      | Direct execution    |
| WASM          | `wasm_backend.rs`                               | `.wasm` module      |

All three consume `SemanticProgram`; the LLVM and interpreter
backends have been exercised across the full corpus, the WASM
backend is a baseline.

### Runtime

Region memory lives in `src/runtime/region.rs` and
`src/runtime/region_memory.rs`. Regions form a parent/child tree;
deallocating a region deallocates its children; double-free is
prevented at the allocator API. Compile-time proofs that a raw
pointer does not outlive its region are future work.

---

## Backend Contract

```rust
pub trait Backend {
    fn compile(&self, ir: &VerifiedIR, output_name: &str) -> Result<BackendOutput>;
    fn name(&self) -> &str;
    fn description(&self) -> &str;
    fn can_execute(&self) -> bool;
}
```

The trait takes `&VerifiedIR`, not `&SemanticProgram`. That is the
gate: a backend cannot be handed IR that has not passed both
verifiers.

---

## Module Responsibilities

| Module                              | Responsibility                          |
|-------------------------------------|-----------------------------------------|
| `frontend/lexer.rs`                 | Tokenize source; emit Indent/Dedent     |
| `frontend/parser.rs`                | Build AST from tokens                   |
| `frontend/ast.rs`                   | AST node types                          |
| `frontend/module_loader.rs`         | Import resolution and cycle detection   |
| `semantics/semantic.rs`             | Types, ownership, borrows, traits       |
| `semantics/semantic_builder.rs`     | AST -> CFG; consumes the type table     |
| `semantics/trait_registry.rs`       | Trait and impl registration, lookup     |
| `semantics/race.rs`                 | Syntactic access-conflict detection     |
| `semantics/escape.rs`               | Scope-level reference escape tracking   |
| `semantics/control_flow.rs`         | CFG helpers used by the analyzer        |
| `semantics/flow_analyzer.rs`        | `is_terminated` helper                  |
| `ir/semantic_ir.rs`                 | IR data types                           |
| `ir/loop_desugar.rs`                | Loop unrolling and list literal folding |
| `ir/monomorphize.rs`                | Generic specialization                  |
| `ir/optimizer.rs`                   | Folding, DCE, branch simplification     |
| `ir/cfg_verifier.rs`                | Structural CFG checks                   |
| `ir/semantic_verifier.rs`           | Instruction-level semantic checks       |
| `ir/verified_ir.rs`                 | `VerifiedIR` gate type                  |
| `ir/defer_lowering.rs`              | Vestigial (defers chained at IR build)  |
| `backends/backend.rs`               | Backend trait contract                  |
| `backends/llvm_backend.rs`          | LLVM backend driver                     |
| `backends/ir_codegen.rs`            | LLVM IR emission from `SemanticProgram` |
| `backends/interpreter_backend.rs`   | Interpreter backend driver              |
| `backends/interpreter.rs`           | Tree-walking interpreter                |
| `backends/wasm_backend.rs`          | WASM backend (baseline)                 |
| `runtime/region.rs`                 | Region stack and hierarchy              |
| `runtime/region_memory.rs`          | Allocator with double-free protection   |
| `common/types.rs`                   | The `Type` enum                         |
| `common/diagnostics.rs`             | `CompileError`, `ErrorCode`             |
| `common/span.rs`                    | `Span` (line, column, range)            |

---

## Test Layout

| Suite                          | Location                          |
|--------------------------------|-----------------------------------|
| Unit tests                     | Inline `#[cfg(test)]` modules     |
| IR tests                       | `tests/ir_tests.rs` + `tests/ir/` |
| Semantics tests                | `tests/semantics_tests.rs`        |
| Integration tests              | `tests/integration_tests.rs`      |
| Differential tests             | `tests/differential_tests.rs`     |
| Backend tests                  | `tests/backends_tests.rs`         |
| Property tests                 | `tests/property_tests_runner.rs`  |
| Fuzz tests                     | `tests/fuzz_tests_runner.rs`      |
| Adversarial programs           | `tests/adversarial/*.gol`         |
| Conformance corpus             | `tests/programs/`                 |

The adversarial suite is run by `tests/adversarial/run_adversarial.sh`.
Each `.gol` file carries an `// EXPECT:` directive (`REJECT`,
`ACCEPT`, or `RUNTIME-TRAP`). The runner compiles the file, and
for `RUNTIME-TRAP` also executes the binary and checks for a
non-zero exit.

---

## Known Gaps

The following are true today and are not bugs to be hidden, but
limitations the architecture accounts for:

- **LLVM does not lower `Result` or `try/catch`.** Programs using
  `try/catch` are refused by LLVM with a clear diagnostic and
  must run with `--interpreter`. The interpreter handles them
  end-to-end.
- **No alias analysis in the race detector.** A read through a
  reference is not connected to a write of the reference's
  source. Two threads accessing the same memory through
  different names can evade detection.
- **NLL is not implemented.** Borrows live until their enclosing
  scope ends. Programs that rely on non-lexical lifetimes are
  rejected conservatively.
- **Defer only chains on `return`.** `break`, `continue`, and
  block fall-through do not trigger defers. Nested blocks share
  a single defer stack per function.
- **Region pointer lifetime is not proven at compile time.** The
  runtime allocator prevents double-free and use-after-region
  at its own API boundary, but a raw pointer obtained from a
  region and used after the region ends is not statically
  rejected.
- **No common subexpression elimination.** The optimizer
  performs constant folding, dead code elimination, and branch
  simplification only.
- **`Terminator::Defer` is vestigial.** It exists in the IR enum
  but is never emitted; `defer_lowering.rs` scans for it and
  finds nothing. Both can be removed in a future cleanup.