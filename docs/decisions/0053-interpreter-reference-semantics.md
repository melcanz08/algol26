# ADR 0053: Interpreter reference semantics

Status: Accepted. Implementation in three sessions.

## Context

The interpreter treats a reference as a value snapshot. The
entire runtime model is at `src/backends/interpreter/eval.rs:175`:

    TypedIRValue::BorrowShared { expr, .. }
    | TypedIRValue::BorrowMutable { expr, .. }
    | TypedIRValue::ReadReference { expr, .. }
    | TypedIRValue::AddrOf { expr, .. } => self.eval_value(expr)?,

Every reference-valued expression evaluates to the value of its
operand. `&p.x` produces `10` (the current value of `p.x`), not a
pointer to `p.x`. `*r` reads `10` regardless of what happened to
`p.x` afterward. `val r := &p.x; p.x := 20; print(*r)` prints
`10` on the interpreter.

The LLVM backend, by contrast, emits a real `getelementptr` for
`&p.x` (fixed in an earlier commit) and prints `20`. The two
backends disagreed.

The analyzer now rejects the motivating program at the borrow
check (ADR 0044 stages 2b-2d). So the divergence is not
observable for the shapes now covered.

**What actually reaches the interpreter** is narrower than an
earlier draft of this ADR claimed. A probe of `*r := *r + 1`
inside a `proc` taking `r: &mut Int` fails at *parse* time:
`Stmt::Assign`'s left-hand side is a `String` name, not an
`Expr`, so `*r := ...` is not a grammar production. `Instruction::
WriteReference` exists in the IR and the LLVM backend lowers it,
but no source program currently produces it. Writes through
references are therefore out of scope for this ADR.

The shapes that do reach the interpreter with references are:
reads through references (`*r` where `r: Borrow<T>`), references
inside records and other composites, `&field`/`&element` borrows
in method receivers. On all of these, the interpreter returns a
snapshot rather than a live view.

**The interpreter is the language's reference semantics.** Every
differential test treats its output as ground truth. ADR 0049
was decided in favor of the interpreter's behavior over LLVM's.
If the interpreter cannot execute references correctly, it stops
being a valid oracle for the reference-shaped subset of the
language.

## Decision

Model references in the interpreter as
`RuntimeValue::Reference(Place)`, where `Place` is the same type
the analyzer and the CFG already use. Resolution is by
projection against the live `variables` map — no
`Rc<RefCell<...>>` refactor of the interpreter's storage yet.

Reads through references resolve the place at read time.
Writes through references route back through the place. This
gives references their expected "live view" semantics: the value
seen by `*r` is whatever `p.x` holds now, not what it held when
`&p.x` was evaluated.

## Scope

One session. Add `RuntimeValue::Reference(Place)`.
`BorrowShared` / `BorrowMutable` / `AddrOf` produce it.
`ReadReference` resolves it through a `resolve_place` helper
that walks the current `variables` map. A `place_of_ir_value`
helper mirrors the analyzer's `place_of_expr` at the IR level.
Reads through references become live.

**Out of scope, deliberately:**

- **Writes through references.** `*r := ...` is not a grammar
  production; the parser rejects it. Making writes work requires
  AST changes (`Stmt::Assign`'s target from `String` to `Expr`),
  parser changes, type-checker changes, and interpreter changes.
  That is a separate ADR — "reference writes" or equivalent.

- **Storage-level aliasing.** A future ADR may replace
  `HashMap<String, RuntimeValue>` with
  `HashMap<String, Rc<RefCell<RuntimeValue>>>`; see Alternatives
  below.

- **Region-lifetime interaction.** References with region
  lifetimes are gated by ADR 0045's `E-REGION-001` at the CFG
  dataflow layer, which runs before either backend. This ADR
  makes the interpreter's *runtime representation* of references
  correct; it does not change which programs are accepted.

## Consequences

**Positive.**

- The interpreter becomes a correct reference implementation for
  the reference subset of the language.
- The differential test suite regains coverage of reference
  shapes. Today a program with `&field` is rejected by the
  analyzer before it reaches either backend; after ADR 0044
  stages 2b-2d, the analyzer no longer rejects the safe shapes,
  and the interpreter must execute them correctly.
- `RuntimeValue::Reference(Place)` is the natural representation
  for the LLVM backend's fat pointers and the WASM backend's
  eventual references, if/when those paths are unified.

**Negative.**

- The interpreter grows a new value variant. Every match on
  `RuntimeValue` — display, equality, pattern matching,
  arithmetic — needs a case for it, or an explicit decision to
  reject it in that context.
- Projection-based resolution reads through `p`'s current value
  each time. Programs with many reads through the same reference
  do more work than a direct pointer would; acceptable for a
  tree-walking interpreter.
- The `Place` type is imported into the runtime crate, adding a
  `semantics -> runtime` dependency that did not exist before.
  Alternative: define a small runtime-local `Place`. Rejected
  because the whole point of `Place` (ADR 0044) was to have one
  representation shared between layers.

## Alternatives considered

### A. Interpreter refuses references

Add a `Feature::References` capability, refuse any program
containing `BorrowShared` / `BorrowMutable` / `ReadReference` /
`AddrOf` when the interpreter is chosen. Small patch.

**Rejected.** The interpreter is the language's reference
semantics. Making it partial means the language no longer has a
reference implementation for references. The differential suite
would then be running against LLVM as ground truth, inverting
the relationship that every other test in the repo assumes.

### C. Leave as-is, document the divergence

The codegen probe `g1_borrow_field` already reports `BOTH-FAIL`
(the analyzer rejects the program before either backend runs).
For the shapes the analyzer does not reject, the interpreter
continues to return snapshots.

**Rejected.** Silent wrong answers are the failure mode this
project's fail-closed discipline exists to eliminate. Making an
exception for the most fundamental aliasing construct is the
wrong precedent.

### D. `Rc<RefCell<Value>>` storage for all variables

Replace `HashMap<String, RuntimeValue>` with
`HashMap<String, Rc<RefCell<RuntimeValue>>>`. References hold the
`Rc` directly.

**Deferred, not rejected.** This is a bigger structural change
that makes aliasing explicit at the storage level. It would
handle all reference shapes uniformly. But it also ripples
through every read and write in the interpreter — a multi-week
refactor. Start with projection-based resolution; upgrade if
aliasing patterns demand it. A future ADR can revisit.

## See also

- ADR 0044 — place-based borrow tracking (the analyzer side)
- `docs/decisions/0005-ownership-model.md` — original borrow rules
- `src/semantics/state/mod.rs` — the `Place` type definition
- `src/backends/interpreter/eval.rs` — the current reference lowering
- `src/backends/llvm_codegen/value.rs` — `compile_reference`, the
  LLVM side that already produces real addresses
