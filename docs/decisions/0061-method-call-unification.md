# ADR 0061: FunctionCall / MethodCall Unification

Status: Proposed.

## Context

The parser produces two different AST shapes for what is
syntactically one construct: a method call with a dotted name.

```gol
x.method()           // receiver is a Var
make().method()      // receiver is a Call
p.field.method()     // receiver is a FieldAccess
arr[0].method()      // receiver is an ArrayAccess
```

The first case is parsed as:

```rust
ExprKind::FunctionCall {
    name: "x.method".to_string(),
    args: vec![],
}
```

The receiver is folded into the function name string. The
analyzer/builder split that name at dispatch time.

Every other case is parsed as:

```rust
ExprKind::MethodCall {
    receiver: Box<Expr>,        // the receiver expression
    method: "method".to_string(),
    args: vec![],
}
```

### Why the split exists

The `FunctionCall { name: "x.method" }` form predates records.
When methods could only be called on bare identifiers, folding
the receiver into a string let the analyzer's `Var` dispatch
handle `x.method()` by string-matching `"x.method"` and looking
up the impl via `{Type}_{method}` mangling. This was simple and
correct for the method-on-variable case.

Records introduced complex receivers (`p.field.method()`,
`arr[0].method()`, `make().method()`), which cannot be folded
into a string. `ExprKind::MethodCall` was added for those. Both
forms have coexisted ever since.

### What the split costs

**Duplicated logic at every layer.** The method-dispatch tiers
(inherent impl -> builtin -> primitive impl -> trait impl) are
implemented twice:

- **Analyzer**: `FunctionCall`'s dotted-name branch and
  `MethodCall`'s arm. Each is 150-200 lines. They must agree on
  precedence, argument types, and return types.
- **Builder**: `translate_expr`'s `FunctionCall` arm has a
  dotted-name block; `MethodCall`'s arm has a second block.
  Same tiers, second implementation.
- **Interpreter**: `FunctionCall` with a dotted name does
  string parsing; `MethodCall` uses the receiver value directly.
  Two dispatch paths.

**Drift between the two forms.** Each new method-dispatch rule
has to be added to both sites or the two forms diverge. Two
comments in `analyzer/expr.rs` mark known drift:

> Keep in sync with `resolve_method_call` in
> `src/semantics/builder/values.rs` -- both must agree on which
> type forms own inherent methods.

A third dispatch site (`SemanticIRBuilder::resolve_method_call`)
is not covered by either comment. The ADR 0060 session found a
bug (`List.append` mutability check) where one site had the
check and the other did not.

**Bugs from drift.** Concretely:

- **Bug B** (commit `5d04656`): `List.append` on a complex
  receiver (a `MethodCall`) skipped the mutability check that
  the `FunctionCall` path (`xs.append(x)`) applied.
- **`x.field.method()` on LLVM** (`STATUS.md` known gap):
  `MethodCall`'s LLVM codegen path handles struct-value
  receivers differently from `FunctionCall`'s, and the former
  has an LLVM verifier failure that the latter does not.

The purpose of this ADR is to eliminate the dual
representation and its bug class.

### What a MethodCall covers

Under this ADR, `MethodCall` becomes the single AST node for
every dotted call whose receiver is a value:

```rust
MethodCall {
    receiver: Box<Expr>,     // Var, FieldAccess, ArrayAccess, Call, etc.
    method: String,
    args: Vec<Expr>,
}
```

This includes `x.method()` -- the receiver is `Expr::Var("x")`.
The parser no longer folds the receiver into the function name.

### What FunctionCall covers

`FunctionCall { name, args }` remains for:

- **Bare function calls**: `print(x)`, `factorial(5)`.
- **Builtin namespaces**: `List.length(xs)`, `Map.get(m, k)`,
  `String.to_upper(s)`, `Math.sqrt(x)`, `File.read(path)`,
  `Int.to_string(n)`.
- **Trait-scoped UFCS calls**: `Comparable::compare(a, b)`.
- **Associated type / constant access**: `Self::SIZE`,
  `Day.from_ordinal(3)`, `UserId.from_base(42)`.

The distinction is that `FunctionCall`'s first segment is a
type name (a compiler-known namespace), not a value. `List` is
not an expression; `x` is. The parser already knows which
identifiers are builtin namespaces at parse time (from a fixed
list), so it can classify at parse time without a lookahead
into the value context.

### What this ADR deliberately does not change

- **Builtin dotted dispatch** (`List.length` etc.) remains a
  `FunctionCall`. A future ADR could unify further, but it's
  out of scope here.
- **Trait-scoped UFCS** (`Trait::method(x, y)`) stays as
  `FunctionCall` with a trait-name segment.
- **Trait method call syntax** (`x.method()` where `x` is
  `&dyn Trait`): unchanged. The receiver is a value, the
  method call goes through the unified `MethodCall` path,
  and the existing virtual-call lowering still applies.

## Decision

Unify `FunctionCall { name: "x.method" }` and
`MethodCall { receiver, method }` into a single
`MethodCall { receiver, method, args }` AST node.

The parser produces `MethodCall` for every dotted call whose
first segment is not a builtin namespace (per the fixed list:
`List`, `Map`, `String`, `Math`, `File`, `Int`, `Float`, `Bool`,
`Char`, `Option`, `Result`, `Set`, `Channel`, `Ptr`, `alloc`,
`free`, and trait names from the trait registry). The
`x.method()` case becomes `MethodCall { receiver: Var("x"),
method, args }`, identical in shape to `make().method()` and
`p.field.method()`.

The analyzer's `FunctionCall` dotted-name arm is deleted; its
logic moves into the unified `MethodCall` arm. The builder's
`FunctionCall` dotted-name block is deleted; its logic moves
into the builder's `MethodCall` arm. The interpreter's
dotted-name-from-string parsing is deleted; the receiver is
now an expression, evaluated like any other.

Both `FunctionCall` and `MethodCall` remain in the AST. The
distinction is now semantic (a call on a type-level namespace
vs. a call on a value), not syntactic (an accident of which
shape the parser emitted).

## Consequences

**Positive.**

- **One dispatch site per layer.** The analyzer's method-
  dispatch tiers live in one arm. Same for builder and
  interpreter. Each tier is written once.
- **The `x.field.method()` LLVM bug disappears by
  construction.** `Var` receivers and complex receivers go
  through the same codegen path.
- **The "keep in sync" comments can be deleted.** The two
  existing ones on `analyzer/expr.rs` mark drift sites that
  this ADR eliminates.
- **New method-dispatch rules are added once.** A future
  feature adds one arm to the unified dispatch.
- **The parser becomes simpler.** `parse_postfix` and
  `parse_identifier_expr` no longer have an `is_var_receiver`
  special case.

**Negative.**

- **Churn across every layer.** Every site that constructs
  `ExprKind::FunctionCall { name: "x.method" }` must be
  updated. This is mechanical but touches 20-50 sites.
- **Semantic change in method resolution order.** The
  `FunctionCall` arm and the `MethodCall` arm currently walk
  the tiers in slightly different orders. See the appendix.
  After unification, one order wins for both.
- **The analyzer's error messages change.** A program with
  a method call on a non-existent value now produces the
  `MethodCall` arm's error. A few tests that assert specific
  error text may need updating.
- **No behavior change for users.** Every currently-accepted
  program continues to be accepted. The observable change
  is that the `x.field.method()` bug is fixed and one or two
  error messages improve.

## Implementation plan

Two sessions. Each is an atomic commit: at the end of each,
`cargo build` is green, all tests pass, and every corpus
fixture produces the same output as before.

### Session 1 -- Parser + analyzer

**Parser changes** (`src/frontend/parser/expr.rs`):

- `parse_postfix`'s `Token::Dot` branch: remove the
  `is_var_receiver` special case. Always produce
  `ExprKind::MethodCall { receiver: Box::new(expr), method,
  args, span }`.
- `parse_identifier_expr`'s `Token::Dot` branch: same.
- `src/frontend/parser/items.rs` (if it handles dotted
  names): audit and update.

**Analyzer changes** (`src/semantics/analyzer/expr.rs`):

- Delete the dotted-name handling from the `FunctionCall` arm.
  The `FunctionCall` arm no longer splits on `.`; if the name
  contains `.`, it's a builtin or a trait -- dispatch
  accordingly, but not to user-defined methods.
- Extend the `MethodCall` arm to handle `receiver:
  Var(name)`: look up the variable's type and mutability,
  then dispatch through the existing method tiers.
- Confirm the unified dispatch order.
- Update every analyzer test that constructs
  `FunctionCall { name: "x.method" }`.

**Verification:**

- `cargo build` green.
- `cargo test --lib frontend::parser` green.
- `cargo test --lib semantics::analyzer` green.
- `cargo test --test corpus_diff` green. This is the
  strongest signal -- dozens of fixtures use `x.method()`.

### Session 2 -- Builder + interpreter

**Builder changes** (`src/semantics/builder/expr.rs`):

- Delete the dotted-name handling from the `FunctionCall` arm.
- Extend `MethodCall`'s arm to dispatch on `receiver:
  Var(name)`.
- Verify `coerce_arg_to_param` handles the struct-value case
  for `MethodCall`'s `FieldAccess`-receiver path. This is
  where the `x.field.method()` bug lives.
- Delete the "keep in sync" comments.

**Interpreter changes** (`src/backends/interpreter/eval.rs`):

- Delete the dotted-name parsing in the `FunctionCall` arm.
- `MethodCall` arm: evaluate the receiver, dispatch on its
  type and method name.

**Verification:**

- `cargo build` green.
- `cargo test` fully green.
- `bash tools/verify_all_backends.sh`: 0 mismatches.
- The `x.field.method()` fixture (added to
  `tests/conformance/valid/methods/`) passes on both
  backends.
- `STATUS.md` known-gaps entry for `x.field.method()` removed.

## Testing strategy

The critical test is the corpus. `cargo test --test corpus_diff`
runs every `.gol` fixture in `tests/corpus/` on both backends
and compares output. Many corpus fixtures use `x.method()`
syntax: `corpus_07_loop_math`, `corpus_15_list_of_strings`,
`corpus_28_trait_impl`, `corpus_46_primitive_impl`,
`corpus_58_literal_method_llvm`, and others. If any of them
break during the refactor, it's a real bug.

The conformance suite under `tests/conformance/valid/methods/`
exercises the method-syntax variants. Add a new fixture
`method_on_field.gol` for `x.field.method()` -- the missing
regression test for the bug this ADR fixes.

The analyzer and builder unit tests have dozens of
`FunctionCall { name: "x.method" }` constructions. Every one
must be updated to `MethodCall { receiver: Var("x"), ... }`.

## What is out of scope

- **Builtin dotted dispatch unification.** `List.length(xs)`
  and `xs.length()` will dispatch through different AST
  nodes but reach the same builtin implementation. A future
  ADR could unify further.
- **Trait UFCS unification.** `Comparable::compare(a, b)`
  remains a `FunctionCall`.
- **Function values / closures.** Not currently in the
  language.
- **Dynamic dispatch changes.** `x.method()` where `x` is
  `&dyn Trait` still goes through `VirtualCall`.

## Alternatives considered

**A. Keep both forms, add automated cross-checking.**
Rejected: the two forms take different code paths; writing
"same result" tests requires writing every test twice.

**B. Deprecate the FunctionCall form.** Not possible -- the
parser chooses the AST shape based on the receiver, which is
user-visible; `x.method()` and `p.field.method()` are the
same syntax to the user.

**C. Unify on the string-folding form for everything.**
Rejected: an arbitrary expression can't be folded into a
string (`arr[i()].method()` can't be represented).

**D. Keep both forms but extract a shared helper.**
Rejected: the drift is in the arms deciding *whether* to
call the helper and applying the tier precedence, not in the
helper itself.

## See also

- **ADR 0003** -- Type System. Record / method dispatch
  semantics that motivate this refactor.
- **ADR 0033** -- Methods and Self. The two AST forms both
  target the same method resolution.
- **ADR 0034** -- Generic Impls. One of the tiers.
- **ADR 0041** -- Associated Types. Method-call return types
  depending on `Self::Item`.
- **Commit `5d04656`** -- the specific drift that motivated
  this ADR (Bug B).
- **`STATUS.md` known gaps** -- the `x.field.method()` LLVM
  bug this ADR fixes.

## Appendix: current dispatch-order comparison

**FunctionCall with dotted name** (`analyzer/expr.rs`):

1. Enum variant constructor (`Tier.Gold`)
2. Nominal constructor (`UserId.from_base(x)`)
3. Enum ordinal constructor (`Day.from_ordinal(x)`)
4. Union variant constructor (`Json.Num(42)`)
5. Tuple struct constructor (`Point(x, y)`)
6. Inherent method (`User_rename`)
7. Builtin method (`String.to_upper`)
8. Container method (`List.append`, `Map.insert`)
9. Primitive impl method (`Int_inc`)
10. Trait method (via trait registry)
11. Error: "Undefined function"

**MethodCall** (`analyzer/expr.rs`):

1. `.to_base()` / `.to_ordinal()` extraction intrinsics
2. Container dispatch (`List.append` / `Map.*`)
3. Inherent method
4. Builtin method
5. Primitive impl method
6. Trait method
7. Error: "no method"

The two orders differ in (a) the placement of constructors
(only reachable from `FunctionCall`) and (b) the placement
of containers (before inherent impls on `FunctionCall`,
after on `MethodCall`). The unified order, per this ADR, is
the `MethodCall` order because containers aren't user types
and can't have inherent impls, so the difference is
invisible in practice.

