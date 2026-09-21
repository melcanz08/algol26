# ADR 0012: The Implicit-Deref Convention and the IR Builder

## Status

**Proposed** (2026-09-21). Not yet implemented.

Responds to a discovery made while implementing ADR 0010
Phase 2b (write-through-`&mut`). The ADR's plan assumed the
analyzer and verifier shared one convention and only the
backends were missing. Three test programs show the
convention is distributed across four layers, each
implementing a different subset.

## Summary

ALGOL26 treats a variable of type `Borrow(T)` or
`MutBorrow(T)` as **transparent** in the surface language. A
`p: MutBorrow(Float)` is written and read as though it were a
`Float`; the reference-ness is visible only in the type
declaration and in borrow-takers (`&p`, `&mut p`, `addr_of p`).

The analyzer implements this convention. It unwraps
`Borrow(T)` / `MutBorrow(T)` to `T` before type-checking an
expression that uses a reference-typed variable in value
position, and it unwraps the target type when checking an
assignment to such a variable.

The IR builder does not implement it. It emits
`Variable(name, MutBorrow(Float))` wherever `name` appears,
and compares the assignment target's declared type against
the RHS type without unwrapping. The result is a pipeline
where different layers disagree about what `x` means.

## What the code actually does today

Three programs, run against commit `1a9ce3d`:

### 1. Write-through rejects at the IR builder

```gol
procedure bump(x: &mut Float)
    x := x + 1.0
```

`algol26 check` emits:

```
warning: Assignment type mismatch for 'x': expected MutBorrow(Float), found Float
error[E0002]: Semantic IR construction failed
```

The analyzer accepted the program. The IR builder's
`Stmt::Assign` arm compared `var_info.type_` (unwrapped:
`MutBorrow(Float)`) against the RHS type (`Float`, because
the analyzer unwraps reference operands inside arithmetic).
The mismatch triggers a diagnostic, and the builder refuses
to emit any IR.

### 2. Reference reads in value position work

```gol
procedure main
    var x := 10.0
    var p: &mut Float := &mut x
    print(p)
```

Compiles. Runs. Prints `10.0`.

The IR builder emits `Variable("p", MutBorrow(Float))` for
the `print` argument. LLVM codegen's `compile_value` for
`Variable` loads `map_type(MutBorrow(Float))` — a pointer —
from `p`'s slot, and `emit_print` dereferences it.
Functionally correct, but only because two layers
(IR builder and LLVM) happen to agree by accident.

### 3. Reference-passing works

```gol
procedure take(x: &mut Float)
    print(x)

procedure main
    var value := 10.0
    var p: &mut Float := &mut value
    take(p)
```

Compiles. Runs. Prints `10.0`.

The IR builder emits `Call { args: [Variable("p", MutBorrow(Float))] }`.
LLVM loads the pointer from `p` and passes it. The callee's
`x` slot receives it. Correct.

### The pattern

| Layer | Read `p` where `p: MutBorrow(T)` | Write `p := v` |
|---|---|---|
| Analyzer | Unwraps to `T` | Unwraps target to `T` |
| IR builder | Emits `Variable("p", MutBorrow(T))` unchanged | Emits `Assign { target: "p" }` and refuses |
| Verifier | Never sees the read case (builder refuses first) | `MutBorrow(inner) => inner` unwrap — dead code |
| LLVM codegen | Loads the pointer, dereferences | Would store into `p`'s slot (never reached) |
| Interpreter | Refuses all references | Refuses all references |

Every layer implements a different subset of one convention,
and no layer implements all of it.

## The design question

Where does the implicit-deref convention get applied?

### Option A — In the IR builder (position-aware)

`translate_expr` gains a position parameter or a sibling
function. When it translates `Expr::Var(name)` and `name`'s
declared type is `Borrow(T)` / `MutBorrow(T)`:

- **Value position** → emit `ReadReference { Variable(name, T), T }`
- **Reference position** (argument whose parameter type is
  `Borrow(T)` / `MutBorrow(T)`) → emit `Variable(name, MutBorrow(T))`
- **Place position** (assignment target) → emit
  `Variable(name, MutBorrow(T))` for use as the `reference`
  field of `WriteReference`

Once done, the IR is explicit. Verifier checks canonical
operations. Backends see only `ReadReference` / `WriteReference`
/ raw `Variable`, never a `MutBorrow(T)` operand in a value
context.

**Pros:** matches ADR 0010's goal. IR is self-describing. One
place implements the convention. Verifier can check reads
and writes uniformly.

**Cons:** `translate_expr` has ~40 call sites. Adding a
parameter touches all of them. The convention must be
correctly applied at each call site. Large blast radius.

### Option B — In the backends (implicit at lowering)

Keep emitting `Variable(name, MutBorrow(T))` everywhere.
Make LLVM codegen and interpreter decide, per instruction,
whether to deref. This is what LLVM already does today.
Extend it to `Assign`, where a `MutBorrow(T)` target means
write-through.

**Pros:** zero IR-builder changes. Matches current LLVM
behavior.

**Cons:** every backend re-derives the convention. Adding a
backend means implementing it again. The verifier can't
reason about reference reads or writes because it never sees
a `ReadReference` / `WriteReference` — it sees raw
references in operand positions and must know to unwrap
them. This is exactly the coupling ADR 0010 was written to
eliminate.

### Option C — In the analyzer (type-table annotation)

Extend the type table with an explicit
`ExprContext` per node, or add a parallel table mapping each
`Expr::Var` node to "reference itself" vs. "value it points
to." The IR builder reads the annotation and never has to
think about position.

**Pros:** the analyzer already knows the context. No new
parameter on `translate_expr`.

**Cons:** the type table is keyed by `*const Expr as usize`
— adding context requires a second table with the same
keying, and keeping them in sync is another invariant to
maintain. Also, the analyzer records `MutBorrow(Float)` for
`Var("p")` regardless of context, so the annotation would
need to be new information, not a re-derivation of what's
already there.

## Recommendation

**Option A.**

Reasons:

1. **It is the ADR 0010 direction.** The whole point of
   canonical IR was that every consumer reads the same
   canonical operations. Option B reverses that decision
   for the reference family. Option A extends it.

2. **It makes the IR verifiable.** With `ReadReference` and
   `WriteReference` as explicit operations, the verifier can
   check that a read or write through a reference is
   type-correct. Under Option B, the verifier sees a
   `MutBorrow(T)` operand in a value position and must know
   the convention; the check is weaker.

3. **It localizes the convention.** Under Option B, every
   backend re-implements the deref-on-read rule. Under
   Option A, the rule lives in one place (the IR builder).

4. **Blast radius is manageable.** `translate_expr`'s call
   sites fall into three groups — value, reference, and
   place. Most are value. A parameter with a default (or a
   helper function that defaults to `Value`) keeps the
   change focused.

The main objection to A is the parameter threading. That is
real but bounded, and it is a one-time cost. B's cost is
paid every time a backend or a new analysis touches
reference-typed values.

## Migration plan (Option A)

**Principles:** same as ADR 0010. Each step compiles and
passes the test suite. Steps are additive or single-purpose.
The convention gets applied one position at a time.

### Step 1 — Reference-position pass-through

Add a helper `translate_expr_ref` (or a parameter) that
translates an expression without applying the implicit
deref. The `Borrow`, `MutBorrow`, and `AddrOf` arms use it
for their inner expression. `Call` uses it for arguments
whose parameter type is `Borrow(_)` / `MutBorrow(_)`.

No behavioral change: the current builder already passes
the raw `Variable` in these positions.

**Deliverable:** a helper exists, is used consistently by
borrow-takers, and by-name call args use it based on
parameter type.

### Step 2 — Value-position deref

Add the inverse: `Expr::Var(name)` in value position, where
`name`'s declared type is `Borrow(T)` / `MutBorrow(T)`,
translates to `ReadReference { Variable(name, T), T }`.

The verifier's `compute_binop_type` no longer needs to
handle `MutBorrow` operands — the IR builder never emits
them in value position.

**Deliverable:** `print(p)`, `p + 1.0`, `return p` all
produce `ReadReference` in the IR. LLVM codegen simplifies
its `Variable` load path (it no longer needs to deref).

### Step 3 — Write-position `WriteReference`

Change `Stmt::Assign` to emit `WriteReference` when the
target's declared type is `MutBorrow(T)`. Drop the current
`MutBorrow(inner) => inner` unwrap from the verifier's
`Assign` arm — after this step, `Assign` never sees a
reference target.

**Deliverable:** `bump(x: &mut Float) { x := x + 1.0 }`
compiles and prints the right value. This is the original
goal of ADR 0010 Phase 2b.

### Step 4 — Update ADR 0010

The current ADR 0010 Phase 2 text says "the analyzer and
verifier implement the rule; the backends do not." That
is wrong in three ways. Replace with a pointer to this
ADR.

## Test plan

Three end-to-end programs, added to `tests/corpus/`:

1. `write_through_mut_float.gol` — the `bump` example.
2. `read_through_shared_ref.gol` — a `&Float` parameter
   read in value position.
3. `reborrow_chain.gol` — `&mut p` where `p: MutBorrow(T)`,
   followed by a write through both.

Plus unit tests in `tests/ir/borrow_deref_addrof_test.rs`
that assert the IR shapes:
- `Expr::Var("p")` in value position where `p: MutBorrow(Float)`
  → `ReadReference { Variable("p", MutBorrow(Float)), Float }`
- `Stmt::Assign` targeting `p` → `WriteReference`

## Open questions

1. **Reborrow.** What does `&mut p` mean when `p: MutBorrow(T)`?
   Under the transparent convention, `p` already refers to a
   `T`, so `&mut p` is a `MutBorrow(T)` re-reference. The
   analyzer currently produces `MutBorrow(T)` for this
   expression (it unwraps the inner). The IR builder must
   match: `BorrowMutable { ReadReference { Variable("p", T), T }, MutBorrow(T) }`.
   Confirm the analyzer's intent before Step 1.

2. **Nested reference types.** `p: Borrow(MutBorrow(Float))`
   — is this legal? If yes, does the convention apply
   recursively? Currently the analyzer's unwrap in
   `Expr::Binary` is one level. Confirm before Step 2.

3. **`AddrOf`.** `addr_of p` where `p: MutBorrow(Float)` —
   is the result `Ptr` to the `Float` (through `p`) or `Ptr`
   to `p` itself? The analyzer's `Expr::AddrOf` arm unwraps
   the inner type, so it produces `Pointer(Float)` (through).
   The builder must match. Confirm.

4. **Call arguments where the parameter is a generic.** If
   `f<T>(x: T)` is called with `p: MutBorrow(Float)`, does
   `T` bind to `MutBorrow(Float)` or to `Float`? The
   analyzer resolves generic parameters against argument
   types it has already unwrapped or not depending on the
   expression kind. Worth a session of reading before Step 1.

## What this ADR does not decide

- Does not change ALGOL26's surface syntax. The transparent
  convention is already what the language does; this ADR
  fixes the IR to match.
- Does not add new language features. `&mut p` and `p := v`
  keep the same meaning they have today (in the parts of the
  compiler that get them right).
- Does not touch the borrow checker's known gaps
  (`&mut x` in call arguments, `escape.rs`, `flow_analyzer.rs`).
  Those are separate problems documented in
  `IMPLEMENTATION_STATUS.md`.

## See also

- `decisions/0010-canonical-ir.md` — the parent project.
  Phase 2b is the trigger for this ADR.
- `decisions/0005-ownership-model.md` — the ownership rules
  the reference family expresses.
- `src/semantics/analyzer/expr.rs` — the analyzer's unwrap
  logic, the convention's current implementation.
- `src/semantics/builder/expr.rs` — the IR builder, where
  the convention is currently missing.
- `src/backends/llvm_codegen/value.rs` — the backend that
  currently implements the convention informally for reads.
- `tests/corpus/` — where the end-to-end tests will live.
