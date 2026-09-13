# Type Table Addressing

**Status:** Active invariant. Violating it produces wrong codegen
silently. Read before touching any phase that consumes
`ParsedProgram::functions` or `TypedProgram::functions`.

## The problem

`TypedProgram::type_table` is a `HashMap<usize, Type>` keyed by the
memory address of expression nodes inside `ParsedProgram::functions`:

```rust
pub type_table: std::collections::HashMap<usize, crate::common::types::Type>,
```

The table is populated by `SemanticAnalyzer::analyze_with_traits`
walking `parsed.functions` and recording `&expr as *const _ as usize`
for each expression it types. It is consumed by `SemanticIRBuilder`
walking `functions` and looking up `&expr as *const _ as usize` for
each expression it lowers.

**Lookups only hit if the analyzer and the builder visit the same
allocation.** If anything clones the `Vec<FunctionDecl>` between
them, every address changes, every lookup misses, and the builder
falls back to `Type::Unknown`.

## The failure mode

`Type::Unknown` is not an error. The IR verifier tolerates it — most
expressions that "should" have a resolved type simply pass through
with `Unknown`, because the verifier's job is well-formedness, not
type completeness. Codegen then emits whatever `Unknown` lowers to.

Concrete example: `tests/conformance/valid/short_circuit.gol`
declares `side_effect(msg: String) -> Bool`. The `__result_0` call
inside the short-circuit lowering used to be emitted with
`return_type: Unknown` when the AST was cloned between `type_check`
and `build_semantic_ir`, producing wrong LLVM IR. The differential
tests did not catch this because the interpreter also tolerates
`Unknown` and the LLVM path was never exercised on that file.

## The invariant

`ParsedProgram::functions` and `TypedProgram::functions` are
`Rc<Vec<FunctionDecl>>`. This is not an optimization. It is the
mechanism by which the allocation survives across phases.

**Phases before `type_check` may rebuild the `Rc`.** The AST is
still in flux and no type table exists yet, so no addresses are
yet load-bearing:

| Phase                  | Rebuilds `Rc`? |
|------------------------|----------------|
| `parse`                | yes (creates)  |
| `process_imports`      | yes            |
| `desugar`              | yes            |
| `expand_impl_methods`  | yes            |
| `monomorphize`         | yes            |
| `type_check`           | **no**         |

**Every phase at or after `type_check` must preserve it.** The
analyzer walks a fixed allocation; anything downstream of it must
walk the same one. That means:

- Pass the `Rc` by move or by `Rc::clone`. Never `(*parsed.functions).clone()`.
- Never rebuild a `ParsedProgram`/`TypedProgram` between `type_check` and `build_semantic_ir`.
- Any `Pass<Program>` that carries `Rc<Vec<FunctionDecl>>` in its
  `AstPayload` must move the `Rc` out unchanged.

## How the code enforces it

`Compiler::type_check` does:

```rust
Ok(TypedProgram {
    functions: Rc::clone(&parsed.functions),
    // ...
})
```

A refcount bump, not a deep clone. `typed.functions` and
`parsed.functions` are the same `Vec` at the same address.

`BuildSemanticIRPass::run` does:

```rust
crate::compiler::build_semantic_ir_program(&ast.functions, ast.type_table.clone())
```

`&ast.functions` deref-coerces through `Rc<Vec<T>>` to `&[T]`
pointing at the original allocation. Do not change this to
`ast.functions.clone()` — that would produce a fresh `Vec` and the
lookups would miss.

## How we know if it breaks

`tests/compiler_pipeline_equiv.rs::build_ir_pass_produces_identical_ir_to_direct_call`
runs the frontend twice per conformance file, once through
`build_semantic_ir_program` and once through `BuildSemanticIRPass`,
and asserts the resulting `SemanticProgram` is byte-identical via
`Debug`. If the invariant breaks, the two paths diverge on any file
where the type table matters.

That test is the reason this invariant was discovered. It caught
`short_circuit.gol` on the first run of `BuildSemanticIRPass`.

## Why not a stable ID

`Rc` makes addresses **stable**. It does not make them **correct**
as keys — they are still an implementation detail of the current
allocation.

A stronger fix is a monotonically-assigned `ExprId` on every
expression node, created during parsing and carried through the
AST. The type table keys on `ExprId`, and the whole problem
disappears: no addresses, no invariant, no allocation discipline.

That is a larger change — it touches the parser, every AST node,
the analyzer, and the builder — and is not required for
correctness today. Tracked in `docs/archive/safety-roadmap.md`
as a long-term refactor.

## References

- `docs/compiler/ir-pass-contracts.md` — contract shape for the
  passes that preserve this invariant
- `docs/decisions/0003-type-system.md` — type representation
- `tests/compiler_pipeline_equiv.rs` — the equivalence oracle