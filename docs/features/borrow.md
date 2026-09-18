# Feature: Borrow (`&T` / `&mut T`)

> Per-feature contract, following the pattern described in `docs/architecture-direction.md`.
> This file is the authoritative answer to "what is a borrow in ALGOL26, and where does it live?"

## Summary

A **borrow** is a temporary reference to a value that is owned by
someone else. It is the mechanism for reading or mutating a value
without taking ownership of it. Borrows are the central feature of
ALGOL26's ownership model: without them, every function call would
either consume its arguments or require them to be `Copy`.

Two kinds exist:

- `&T`    — **shared borrow**. Any number of shared borrows of the
  same place may exist at once. The place cannot be mutated or moved
  while a shared borrow is live.
- `&mut T` — **mutable borrow**. At most one mutable borrow of a place
  may exist at a time. While it is live, the place cannot be read,
  borrowed again, or moved.

Borrows are **linear in time, not in count**. Two shared borrows may
coexist; a shared and a mutable borrow may not. This matches the
rules in `docs/decisions/0005-ownership-model.md`.

## Syntax

Taking a borrow:

```gol
val x := 42
val r := &x         // shared borrow
var y := 42
val m := &mut y     // mutable borrow
```

Dereferencing:

```gol
val v := *r         // dereference a shared borrow
*m := 43            // write through a mutable borrow
```

Borrow in argument position — this is the common case and is
**temporary**:

```gol
function add(x: &Int, y: &Int) -> Int
    return *x + *y

procedure main
    val a := 10
    val b := 32
    print(add(&a, &b))
```

`add(&a, &b)` creates two temporary borrows whose lifetime is the
call itself. When the call returns, they end. The original `a` and `b`
remain usable.

Returning a borrow from a function:

```gol
function first(x: &List<Int>) -> &Int
    return &x[0]
```

A returned borrow outlives the function; its lifetime is bound to the
lifetime of the input it was derived from. This is the *reference
escape* case the analyzer tracks.

## Typing rules

| Expression | Type |
|---|---|
| `&x` where `x: T` | `&T` |
| `&mut x` where `x: mut T` | `&mut T` |
| `*r` where `r: &T` or `r: &mut T` | `T` |
| `r[i]` where `r: &List<T>` | `T` (element read) |

Location in the type system: `src/common/types.rs`,
variants `Type::Borrow(Box<Type>)` and `Type::MutBorrow(Box<Type>)`.
Constructors: `Type::borrow(inner)` and `Type::mut_borrow(inner)`.

Parsing: `Type::from_str` accepts `&T`, `&mut T`, `Borrow<T>`,
`borrow<T>`, `MutBorrow<T>`, `mutborrow<T>`, `mut_borrow<T>`.

### Coercion

Covariant in the inner type for reads:

```
Type::Borrow<Int>    -> Type::Borrow<Float>       (allowed)
Type::MutBorrow<Int> -> Type::MutBorrow<Float>    (allowed by can_coerce_to)
```

**`&mut T` covariance is debatable.** Rust makes `&mut T` invariant
because writing through a coerced `&mut Float` where the caller sees
`&mut Int` would put a `Float` into an `Int` slot. ALGOL26 currently
allows this coercion. This is a possible soundness gap worth
investigating — see Open Questions.

No implicit coercion between `&T` and `&mut T` in either direction:

```gol
val m: &mut Int := ...
val s: &Int := m     // compile error: cannot coerce &mut to &
```

Explicit *reborrowing* is the sanctioned conversion: `&*m` produces a
shared borrow of the pointee. This is discussed under Reborrow below.

## Ownership

### What a borrow does

1. **Does not move the place.** `&x` leaves `x` usable after the
   borrow ends (or, for a temporary borrow, after the enclosing
   expression).

2. **Blocks move while live.** If a shared or mutable borrow of `x`
   is live, `val y := x` (which moves `x`) is a compile error.
   Diagnostic: `E-MOVE-002` — "Cannot move 'x' while borrowed".

3. **Blocks mutation while shared.** A shared borrow prevents `x := v`.

4. **Blocks all access while mutable.** A mutable borrow prevents
   reads, writes, further borrows, and moves of the place.
   Diagnostics: `E-BORROW-004`.

### What a place is

A place is a named variable. Compound places (`x[0]`, `p.field`) are
**not** currently tracked as distinct places by the ownership
analyzer. `&x[0]` is treated as a borrow of the whole list `x`, not
of the element. This is conservative but sound: it forbids some
programs that would be legal (two disjoint element borrows) but
never accepts an unsound one.

### Borrow lifetime

`BorrowLifetime` in `src/semantics/state/mod.rs` has four variants:

| Variant | Meaning | Example |
|---|---|---|
| `Temporary(CallId)` | Ends when the call site returns | `add(&a, &b)` |
| `Local(name)` | Bound to the scope of the borrowing variable | `val r := &x` |
| `Region(name)` | Bound to the region the borrower lives in | `region r { ... }` |
| `Static` | Outlives everything | (not currently produced) |

The lifetime of a borrow is determined at IR-construction time by
`src/ir/cfg/builder.rs` and checked by `src/ir/cfg/dataflow.rs`:

- If the borrower name starts with `__tmp_call_`, the borrow is
  `Temporary` and ends when the corresponding `Call` instruction is
  processed.
- Otherwise, the lifetime is the borrower's declaration region (if
  any), falling back to the current region, then `Local(borrower)`.

### Reborrow

Reborrowing is passing a borrow by reference without taking ownership:

```gol
function length(s: &List<Int>) -> Int
    return List.length(s)

function caller(s: &List<Int>) -> Int
    return length(s)   // s is reborrowed, not moved
```

Reborrowing is **implicit** in ALGOL26 for shared borrows. The
analyzer treats the argument to `length(s)` as a shared reborrow of
`s`, with a temporary lifetime. The original `s` remains usable
after the call.

Explicit reborrows use deref-then-borrow syntax:

```gol
val r: &mut Int := ...
val s := &*r       // shared reborrow of the pointee
```

The analyzer tracks this as a borrow of `*r` (the pointee), not of
`r` itself. See `tests/semantics/borrow_checker_extra_test.rs`.

## IR representation

In `src/ir/semantic_ir.rs`, four `TypedIRValue` variants:

| Source | Variant |
|---|---|
| `&x` | `TypedIRValue::Borrow { expr, target_type }` |
| `&mut x` | `TypedIRValue::MutBorrow { expr, target_type }` |
| `*r` | `TypedIRValue::Deref { expr, target_type }` |
| `&x` in pointer position | `TypedIRValue::AddrOf { expr, target_type }` |

`Borrow` and `AddrOf` overlap. `AddrOf` is used when the borrow is
produced from a primitive pointer operation (e.g. `&` on a
variable in a low-level context); `Borrow` is the standard borrow
expression. Both produce a `&T`.

### IR verifier rules

The verifier enforces:

- `Borrow { expr, target_type }` requires `expr` to verify and
  `target_type` to be `&T` for some `T`.
- `MutBorrow { expr, target_type }` requires `expr` to be a mutable
  place and `target_type` to be `&mut T`.
- `Deref { expr, target_type }` requires `expr` to be a `&T` or
  `&mut T` and returns `target_type`.

## CFG representation

The CFG builder in `src/ir/cfg/builder.rs` translates borrow
expressions into `CfgInstruction` values:

| Source pattern | Emitted `CfgInstruction` |
|---|---|
| `val r := &x` | `Declare{r}` then `Borrow{borrower: r, place: x, mutable: false}` |
| `val r := &mut x` | `Declare{r}` then `Borrow{borrower: r, place: x, mutable: true}` |
| `f(&mut x)` | `Borrow{borrower: "__tmp_call_{id}_{x}", place: x, mutable: true}` then `Call` |
| `f(&x)` | `Borrow{borrower: "__tmp_call_{id}_{x}", place: x, mutable: false}` then `Call` |
| `return &x` | `ReturnRef{place: x}` |
| `return x` where `x` is not a borrow | `Use{x}` (not `ReturnRef`) |

The dataflow engine (`src/ir/cfg/dataflow.rs`) handles each of these
in `OwnershipTransfer::transfer`:

- `Borrow { borrower, place, mutable }`:
  1. Checks `place` is not moved (`E-MOVE-001`) or uninitialized
     (`E-INIT-001`).
  2. Determines the borrow lifetime.
  3. Calls `incoming.borrow(borrower, place, kind, lifetime)`.

- `Call { name, args }`: ends temporary borrows whose place appears
  in `args`.

- `ReturnRef { place }`: marks the place as escaping via return
  (`E-ESCAPE-001`).

## Diagnostics

Borrow-related error codes currently emitted:

| Code | Meaning | Emitted from |
|---|---|---|
| `E-BORROW-004` | Cannot assign/use while mutably borrowed | `dataflow.rs` |
| `E-MOVE-001` | Borrow of moved value | `dataflow.rs` |
| `E-MOVE-002` | Move of borrowed value | `dataflow.rs` |
| `E-INIT-001` | Borrow of uninitialized value | `dataflow.rs` |
| `E-ESCAPE-001` | Reference escapes via return | `dataflow.rs` |
| `E-ESCAPE-002` | Reference escapes via channel send | `dataflow.rs` |
| `E-REGION-001` | Reference outlives region | `dataflow.rs` |

**Codes that do NOT exist** (despite being mentioned in prior
reports): `E-BORROW-001`, `E-BORROW-002`, `E-BORROW-003`,
`E-ESCAPE-003`. The current vocabulary is exactly the seven above.

The gap from `E-BORROW-004` (the only BORROW code) is that
shared-borrow-specific errors — e.g. "cannot borrow `x` mutably while
`x` is shared-borrowed" — currently fall through to
`E-BORROW-004` or to the wrong code. If those diagnostics need
distinct codes, adding them is a Tier 2 follow-up.

## Analyzer

`src/semantics/analyzer/ownership.rs` has:

- `register_mutable_borrow(reference, source) -> Result<()>`

**`register_borrow` (shared) does not exist as a public method.**
This was verified by grep across the codebase; only the mutable
variant is a named method. Shared borrows are registered through
`SemanticState::borrow(...)` directly or through another path I have
not traced. This is worth confirming before treating the analyzer as
complete — see Open Questions.

## Backends

| Backend | Support | Evidence |
|---|---|---|
| Interpreter | **Refuses** | `TypedIRValue::Borrow/MutBorrow/Deref/AddrOf` return `EvalError::Unsupported` (added in PR-A) |
| LLVM | **Partial / unverified** | LLVM has no reference type; borrows lower to pointer operations. Full audit not done. |
| WASM | **Unverified** | No borrow test in `src/backends/capabilities/tests.rs`. |

### Interpreter

The interpreter was updated in this session to explicitly refuse
borrows rather than silently return `Void`:

```rust
TypedIRValue::Borrow { .. }
| TypedIRValue::MutBorrow { .. }
| TypedIRValue::Deref { .. }
| TypedIRValue::AddrOf { .. } => {
    return Err(EvalError::Unsupported {
        construct: "references",
        hint: "use the LLVM backend (--interpreter does not model borrows)",
    });
}
```

This means any `--interpreter` run of a program using borrows fails
cleanly with an error message. The trade-off: some corpus tests that
use borrows will refuse to run under the interpreter. They should be
marked `// BACKEND: llvm` or moved to a fixture that skips the
interpreter.

### LLVM

The LLVM backend lowers borrows to pointer values. `&x` becomes an
`alloca` address; `*r` becomes a load; `*m := v` becomes a store.
Because LLVM has no reference type, the borrow/move discipline is
**not** re-checked at codegen time — the analyzer and verifier are
the only enforcement.

This is fine as long as the analyzer is sound. It is not a
fail-closed backend for borrows; it assumes the analyzer did its
job. The Tier 2 fail-closed audit should examine whether any
borrow-derived pointer operation can reach LLVM codegen without
being validated.

## Optimizer rules

None implemented. Candidates:

- **Copy propagation through temporary borrows.** A
  `Borrow{__tmp_call_N_x}` followed by `Call` and never used between
  the two is dead and could be removed.

Every optimizer rule must preserve the borrow live range: removing
a borrow that the analyzer used to justify a diagnostic must not
make the program verifiable but wrong.

## Safety

- No aliasing rule is enforced at runtime. The analyzer is the sole
  enforcement mechanism.
- `&mut T` covariance (see Coercion above) is a possible soundness
  gap if the analyzer permits writing a `Float` through a `&mut`
  that was coerced from `&mut Int`.
- Escape analysis (`E-ESCAPE-001`, `E-ESCAPE-002`) prevents a
  reference to a local from outliving that local. It is the
  mechanism behind the region-memory model.

## Test coverage

Current coverage across the tree:

**Semantics-level (`tests/semantics/borrow_checker_*.rs`):**

- `test_borrow_basic_works`, `test_borrow_does_not_move`
- `test_borrow_chain`, `test_borrow_scope_end_allows_reuse`
- `test_multiple_immutable_borrows_ok`, `test_multiple_borrows_different_variables`
- `test_double_mutable_borrow_fails`, `test_mutable_borrow_then_immutable_fails`
- `test_read_during_mutable_borrow_fails`, `test_mut_borrow_of_immutable_fails`
- `test_borrow_moved_variable_fails`, `test_borrow_moved_in_loop_fails`
- `test_borrow_in_conditional`, `test_borrow_in_loop`,
  `test_borrow_in_function_scope`
- `test_borrow_across_function_calls`, `test_borrow_across_function_boundary`
- `test_borrow_across_defer`, `test_borrow_in_defer_after_move`
- `test_borrow_across_loop_break`, `test_borrow_across_loop_continue`
- `test_borrow_after_conditional_move`, `test_borrow_in_nested_scope`
- `test_double_borrow_in_parallel`, `test_mutable_borrow_in_loop`

**IR-level (`tests/ir/borrow_deref_addrof_test.rs`):**

- `test_borrow_expression`, `test_deref_expression`, `test_addrof_expression`
- `test_double_borrow_fails`, `test_borrow_immutable_fails`, `test_deref_non_pointer_fails`

**CFG-level (`src/ir/cfg/dataflow.rs` tests):**

- `dataflow_enforces_move_semantics`
- `dataflow_enforces_region_outlives`

**Soundness fixtures (`tests/soundness/borrowing/`):**

- `borrow_across_branch.gol`
- `borrow_across_loop.gol`
- `persistent_mut_borrow.gol`
- `temporary_mut_borrow.gol`

### Gaps

- **No test for `&mut T` covariance.** If the coercion is unsound,
  no test currently exercises the case.
- **No test for compound-place borrows** (`&x[0]`, `&p.field`).
  These are conservatively promoted to whole-place borrows; whether
  the analyzer handles them correctly is not verified.
- **No differential test.** No borrow-using program runs through
  both the interpreter and LLVM, because the interpreter refuses
  borrows. Until the interpreter supports them (or a
  borrow-checking-only differential mode exists), this cannot be
  closed.
- **No test for borrows in a `defer` block** beyond the two named
  above. Defer + borrow is a known tricky combination.
- **No test for reborrow through a function parameter** — the case
  where `caller(s: &List<Int>)` passes `s` to `length(s)`.
  `test_borrow_across_function_calls` covers a related case, but
  not the specific reborrow-through-parameter pattern.

## Maturity

Following the stages in `docs/architecture-direction.md`:

```
Borrow (&T / &mut T)
    semantics:   Stable (with one open question on &mut covariance)
    parsed:      yes
    typed:       yes
    validated:   yes (analyzer + CFG dataflow)
    IR:          yes
    verified:    yes
    interpreter: refused (fail-closed)
    LLVM:        partial (lowered; not fail-closed for borrow errors)
    WASM:        unverified
    optimized:   no rules
```

This is the first feature whose interpreter support is intentionally
refused rather than partial. That is a deliberate choice: modeling
borrows in a tree-walking interpreter requires simulating an aliasing
model that the interpreter's flat `HashMap` environment does not
express. The language's borrow model is enforced at compile time;
the interpreter does not need to re-enforce it, only to not silently
mis-handle it. Refusal is the right answer.

## Checklist for related features

If you are adding a feature *like* borrows (a reference type with
aliasing rules), you need to touch:

1. `src/common/types.rs` — new `Type` variant + constructor + parsing
   + `can_coerce_to` + `can_cast_to` + `common_supertype` +
   `inner_type` + `Display`.
2. `src/ir/semantic_ir.rs` — new `TypedIRValue` variant(s).
3. `src/ir/verifier/` — rules for the new variants.
4. `src/semantics/state/mod.rs` — new `BorrowKind` or `BorrowLifetime`
   variant if the aliasing rule differs.
5. `src/semantics/analyzer/ownership.rs` — register/check functions.
6. `src/ir/cfg/builder.rs` — translation to `CfgInstruction`
   (typically a new `CfgInstruction` variant).
7. `src/ir/cfg/dataflow.rs` — dataflow rules and new diagnostic codes.
8. `src/backends/interpreter/eval.rs` — evaluation, or explicit
   refusal via `EvalError::Unsupported`.
9. `src/backends/llvm_codegen/` — lowering.
10. `src/backends/capabilities/scan.rs` — declare backend support.
11. `src/backends/capabilities/tests.rs` — accept/reject per backend.
12. `tests/semantics/borrow_checker_*.rs` — positive and negative
    cases at the analyzer level.
13. `tests/soundness/<family>/` — fixtures in the soundness suite.
14. `docs/features/<feature>.md` — this file.

## Open questions

- **Is `&mut T` covariance sound?** `Type::can_coerce_to` currently
  allows `MutBorrow<Int>` → `MutBorrow<Float>`. If a caller sees a
  `&mut Float` and writes `1.5`, but the underlying storage is an
  `Int`, the write is a type error the analyzer did not catch. Rust
  makes `&mut T` invariant for exactly this reason. This is the
  single most important open question in this contract.

- **Where is shared-borrow registration?** `register_mutable_borrow`
  exists as a named method; `register_borrow` does not. Shared
  borrows are registered somehow (the tests pass), but the code path
  is not obvious from grep. Worth a five-minute investigation before
  treating the analyzer as complete.

- **Should compound places be tracked individually?**
  `&x[0]` and `&x[1]` are currently treated as two borrows of the
  whole `x`, forbidding some valid programs (two disjoint element
  borrows). This is conservative but a real ergonomics cost. Making
  it precise is a Tier 7 (canonical IR) problem — the IR would need
  a `Place` abstraction beyond bare variable names.

- **Should there be explicit reborrow syntax?** Currently reborrow
  is implicit at call sites and explicit via `&*r`. A named operator
  might make the intent clearer, but it would be a new feature
  contract.

- **Should borrow errors have distinct codes per violation kind?**
  Right now `E-BORROW-004` covers assign-while-mutably-borrowed,
  use-while-mutably-borrowed, and (as a fallback) some shared-borrow
  violations. Splitting into `E-BORROW-005`..`E-BORROW-00N` would
  make diagnostics more precise. Not urgent.

## See also

- `docs/architecture-direction.md` — the feature contract pattern itself
- `docs/decisions/0005-ownership-model.md` — the design decision
- `docs/decisions/0006-immutability.md` — `val` vs `var`
- `docs/decisions/0007-region-memory.md` — lifetime and region model
- `docs/features/option.md`, `result.md`, `list.md`, `channel.md` — sibling contracts
- `src/common/types.rs` — `Type::Borrow`, `Type::MutBorrow`
- `src/semantics/state/mod.rs` — `BorrowKind`, `BorrowLifetime`, `BorrowState`
- `src/semantics/analyzer/ownership.rs` — analyzer rules
- `src/ir/cfg/builder.rs`, `src/ir/cfg/dataflow.rs` — CFG translation and checks
- `tests/semantics/borrow_checker_test.rs`, `borrow_checker_extra_test.rs`
