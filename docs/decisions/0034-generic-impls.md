# ADR 0034 — Generic Impls (`impl<T> Trait for Type<T>`)

## Status

Accepted. Implemented.

> **Implementation note (2026-10-06).** Shipped across commits
> `bcaeb34` (C1 AST), `84533c4` (C2 parser + C4 partial), `273a13d`
> (C4 coherence), and `6824d40` (C4/C5 monomorphization).
>
> C3 (mangling) required no code change: because C1/C2 split
> `target_type` and `target_type_args`, `expand_impl_methods`'s
> `format!("{}_{}", target_type, name)` already produced
> `Pair_show`, not `Pair<T>_show`.
>
> Fixtures: `generic_method_impl.gol` (returns a literal) and
> `generic_impl_uses_type_param.gol` (returns a `T`-typed field)
> both pass on the interpreter.

> **Status note (2026-10-05).** This ADR is a direct follow-up to
> ADR 0033. That ADR shipped methods for non-generic user types
> (records, enums, nominal types) and explicitly deferred generic
> impls to their own design pass. This is that pass.
>
> The fixture `tests/conformance/pending/generic_method_impl.gol`
> records the syntax this ADR intends to support. It currently
> fails at parse time.

## Context

ADR 0033 introduced inherent impls and trait impls on user-defined
types. It deferred generic impls because the change touches the
frontend, the analyzer, and the mangling scheme simultaneously —
each of which had its own concerns in the non-generic case.

The current state, verified against the code:

- **Parser** (`src/frontend/parser/items.rs::parse_impl`) reads
  `impl <identifier> for <identifier>` or `impl <identifier>`.
  It has no rule for `<T>` after `impl`.
- **AST** (`src/frontend/ast.rs::ImplBlock`) has
  `trait_name: Option<String>` and `target_type: String`. Neither
  carries generic type parameters. `target_type` is a display
  string, not a parsed type.
- **`expand_impl_methods`** (`src/compiler.rs`) renames each method
  to `format!("{}_{}", target_type, method.name)`. A generic
  target like `Pair<T>` would produce `Pair<T>_show` — angle
  brackets in a symbol name.
- **Analyzer** (`src/semantics/analyzer/mod.rs::analyze_with_spans`)
  registers impls via the trait registry. The trait registry has a
  `generic_impls: Vec<GenericImpl>` branch already — for impls
  whose `target_type` contains `<` — but nothing parses them into
  the AST yet.
- **Method resolution** (`src/semantics/analyzer/expr.rs` and
  `src/semantics/builder/values.rs::resolve_method_call`) matches
  the receiver's concrete type name against the mangled name.
  There is no pattern-matching against `Pair<T>`.

The consequence is the fixture's parse error:
`Expected trait or type name, found Lt`.

## Decision

ALGOL26 will support `impl<T> Trait for Type<T>` and
`impl<T> Type<T>` — the same two forms ADR 0033 introduced for
non-generic types, extended to carry type parameters.

### Syntax

```gol
rec Pair<T>
    first: T
    second: T

trait Showable
    function show(self: &Self) -> String

impl<T> Showable for Pair<T>
    function show(self: &Pair<T>) -> String
        return "pair"

procedure main
    val p := Pair<Int> { first: 1, second: 2 }
    print(p.show())
```

Inherent generic impls use the same shape without the `for`
clause:

```gol
impl<T> Pair<T>
    function describe(self: &Pair<T>) -> String
        return "pair"
```

### Type parameters are per-impl, not per-method

`impl<T>` declares type parameters that are in scope for every
method in the block. A method cannot introduce its own type
parameters beyond those declared on the impl. This matches the
existing rule for functions: type parameters are declared once,
at the head of the declaration.

### Receiver form is `self: &Pair<T>` (or `&Self`)

Two spellings are equivalent inside a generic impl:

```gol
impl<T> Showable for Pair<T>
    function show(self: &Pair<T>) -> String   // explicit

impl<T> Showable for Pair<T>
    function show(self: &Self) -> String      // shorthand
```

`Self` expands to the impl's target type (`Pair<T>`). The
substitution mechanism is the one ADR 0033 added to
`validate_method_signature`, extended to carry the impl's type
parameter bindings.

### Mangling strips type arguments

The mangled method name uses the *base name* of the impl's target
type, not the full generic form:

```
impl<T> Showable for Pair<T> { function show }
    → mangled name: Pair_show
```

Not `Pair<T>_show`, not `Pair_T_show`. The type argument is
recovered at call sites through the existing monomorphization
machinery (see §Monomorphization below).

This produces a potential collision with a non-generic `impl
Pair` — but such a coexistence is itself a coherence error. A type
cannot have both `impl Pair<T>` and `impl Pair` (see §Coherence
below); the parser or the analyzer rejects the pair.

### Coherence: one impl per (trait, type-head)

For a given trait `T` and a type head `H` (the unparameterized
name), at most one impl may exist. `impl<T> Showable for Pair<T>`
and `impl Showable for Pair<Int>` cannot coexist — they would
produce the same mangled names and the same pattern match.

The rule is enforced at impl registration: if a second impl for
the same `(trait_name, type_head)` pair is seen, it is an E0002
error. Trait-less inherent impls are subject to the same rule
against other inherent impls of the same type head.

This is a v1 rule. Specialization (a more specific impl winning
over a more general one) is out of scope and would be its own
ADR.

### Monomorphization

A generic impl is *not* monomorphized the way a generic function
is. The impl declares a single method body; that body is emitted
once per concrete type argument that actually appears at a call
site. `Pair<Int>` and `Pair<String>` produce two specializations
of `Pair_show` if both are used; neither is emitted if the impl is
never called.

The existing `InstantiationPlan` machinery already does this for
generic functions. The call-site recording added in
`ExprKind::FunctionCall` (the `instantiations: Vec<Instantiation>`
field) extends naturally: when a method call resolves to a generic
impl, the analyzer records an instantiation with the impl's type
parameters bound to the concrete receiver's type arguments.

### Resolution at call time

When `p.show()` is analyzed with `p: Pair<Int>`:

1. The inherent tier looks up `Pair_show` in `self.functions`.
2. If found and the impl was generic, the analyzer records an
   instantiation `Pair_show<T := Int>`.
3. The IR builder's `resolve_method_call` returns `Pair_show`
   (the base name); the builder's `resolved_callee_name` rewrites
   it to the specialized symbol — `Pair_show__Int` or whatever
   `InstantiationPlan::mangled_name` produces — using the same
   path generic function calls already take.

No new lookup tier is introduced. The trait registry's existing
`GenericImpl` pattern-matching does the work; the analyzer's
instantiation recording and the builder's callee rewriting are
already in place from ADR 0013 and ADR 0033.

## What changes, layer by layer

### B1 — AST

`ImplBlock` gains two fields:

```rust
pub struct ImplBlock {
    pub trait_name: Option<String>,
    pub type_params: Vec<String>,        // NEW
    pub target_type: String,             // still the base name
    pub target_type_args: Vec<TypeSyntax>, // NEW
    pub methods: Vec<FunctionDecl>,
}
```

`target_type` remains the base name string (`"Pair"`). The
`target_type_args` field carries the syntactic type arguments
(`[Named("T")]` for `Pair<T>`; `[Named("Int")]` for a
hypothetical `Pair<Int>`). Both are needed: the base name for
mangling, the type args for matching against concrete receivers.

### B2 — Parser

`parse_impl` gains three rules:

1. After `impl`, if the next token is `<`, parse a type-parameter
   list into `type_params`.
2. The trait or type name is still a single identifier, but if it
   is followed by `<`, parse type arguments into
   `target_type_args`.
3. If the first name after `impl` (and optional type args) is
   followed by `for`, it's a trait impl. Otherwise, it's an
   inherent impl with `trait_name = None`.

The existing `parse_type_syntax` handles `<T>` in type position;
the impl parser reuses it via a shared helper.

### B3 — Analyzer

Two additions:

**Registration-time coherence check.** A new pass over `impls`
detects duplicate `(trait_name, type_head)` pairs and errors with
E0002. Runs in `analyze_with_spans` before user functions are
analyzed.

**Generic method signature substitution.** When validating a trait
impl's signature against the trait declaration, the substitution
closure from ADR 0033 is extended: after `Self := target_type`,
each of the impl's type parameters is bound to the corresponding
argument in `target_type_args`. For `impl<T> Showable for Pair<T>`
and a trait method `show(self: &Self)`, the substitution is
`Self := Pair<T>`.

No new dispatch path is added in `analyze_expr_inner`. The
existing inherent tier looks up `Pair_show` and finds it (the
mangled name uses the base name, so `Pair_show` is the key
regardless of type arguments).

**Instantiation recording.** The inherent tier's success path
gains one addition: if the resolved function's declaration had
non-empty `type_params`, record an instantiation with those
parameters bound to the receiver's concrete type arguments. This
mirrors the existing recording in the non-dotted call path.

### B4 — IR builder

Minimal. `resolve_method_call` already returns the base mangled
name (`Pair_show`). The builder's subsequent call to
`resolved_callee_name` rewrites it to the specialization's symbol
— the same rewrite generic function calls already go through.

The only adjustment is making sure the receiver type's arguments
flow into the instantiation lookup. The instantiation was already
recorded by the analyzer with the correct type arguments, keyed by
the call site's `ExprId`. The builder's existing
`InstantiationPlan::call_sites` lookup finds it.

### B5 — Tests, docs, matrix

Conformance fixtures to add:

- `generic_impl_trait.gol` — `impl<T> Showable for Pair<T>` with a
  concrete instantiation `Pair<Int>`.
- `generic_impl_inherent.gol` — `impl<T> Pair<T>` (no trait).
- `generic_impl_multiple_args.gol` — `impl<K, V> Foo for Pair2<K, V>`.
- `generic_impl_self_shorthand.gol` — receiver written as `&Self`.
- `generic_impl_coherence_rejected.gol` — two impls for the same
  type head, expecting E0002.
- `generic_impl_nested_types.gol` — receiver is `List<Pair<T>>`.

Move `tests/conformance/pending/generic_method_impl.gol` back to
`valid/methods/` once the parser accepts the syntax.

Update `docs/features/methods.md` §Known limitations to remove the
first item; add a §Generic impls section describing the receiver
form, mangling rule, and coherence.

Add a row to `tests/coverage_matrix.rs` for the generic-impl
feature.

## What remains out of scope

Explicitly deferred, each a candidate for its own ADR:

- **Specialization.** A more specific impl overriding a more
  general one (`impl<T> Foo for Pair<T>` and `impl Foo for
  Pair<Int>` coexisting with `Pair<Int>` picking the specific
  version). This is a substantial design problem involving
  dispatch priority, ambiguity on partial overlap, and codegen
  duplication.
- **Associated types.** `trait Container { type Item; }`.
- **Associated constants.** `trait Sized { const SIZE: Int; }`.
- **Where-clause bounds on impls.** `impl<T: Ord> Sortable for
  List<T>`. Currently `where` clauses only appear on functions.
- **Impls on builtin generic types.** `impl<T> Foo for List<T>`.
  ADR 0033 excluded builtin types from inherent impls; the same
  reasoning applies here. This is not a syntax question — it's a
  coherence question, since two modules could both `impl<T> Foo
  for List<T>`.
- **Negative impls.** `impl !Send for T`. No equivalent concept
  exists in the language today.

## Consequences

### Positive

- The fixture parked at `tests/conformance/pending/` becomes
  runnable. ADR 0033's declared scope is complete.
- The parser, AST, analyzer, and mangling all learn to tolerate
  the generic head uniformly — a prerequisite for any future
  generic-declaration syntax (generic enums, generic nominal
  types).
- No new resolution tier, no new backend work. The IR shape is
  the same as for non-generic methods.

### Negative

- `ImplBlock` grows two fields. Every construction site (the
  parser, tests, any future tooling) must be updated.
- `expand_impl_methods`'s mangling becomes "take the base name,
  discard type args" — a rule that needs to be documented in the
  ADR's §Mangling note and cross-referenced from `methods.md`.
- The coherence rule (`one impl per (trait, type-head)`) is
  stricter than some languages permit. It's a v1 simplification,
  not a permanent constraint.

### Neutral

- The trait registry's `generic_impls` branch, which has been
  dormant since it was added, becomes load-bearing. Its
  `TypePattern` matching code is now exercised end-to-end for the
  first time. Expect to find latent bugs.

## Implementation order

Five commits, each compiling and revertable:

```
C1. AST: ImplBlock gains type_params and target_type_args.
    Every ImplBlock literal updated. No behavior change.

C2. Parser: parse_impl accepts impl<T> and Type<T>.
    Constructs the new fields. Add parse tests; no analysis yet.

C3. expand_impl_methods: use the base name for mangling.
    Currently the whole target_type string is used, which for
    Pair<T> would produce Pair<T>_show. Change to strip type
    arguments. For non-generic impls this is a no-op.

C4. Analyzer: coherence check + instantiation recording for
    generic impls. Plus the Self substitution extension.

C5. IR builder: verify receiver-type-args flow into the
    existing InstantiationPlan lookup. Likely zero code change;
    add tests to confirm.

C6. Fixtures, docs/features/methods.md update, coverage_matrix
    row, ADR 0033 status note pointing at 0034 as implemented.
```

Each commit is small enough to review on its own. C3 is the
riskiest — it touches the mangling scheme for all impl methods,
generic or not — so it should include a regression test asserting
that existing non-generic impls produce identical IR before and
after.

## Alternatives considered

### Mangling includes type arguments (`Pair_T_show`)

**Rejected.** It produces a distinct symbol per instantiation,
which is exactly what monomorphization already does — but doing
it at the mangling stage instead of the specialization stage
duplicates the mechanism and breaks the identity between a
generic impl's method and the same method called through a
concrete instantiation. The mangled name is a stable symbol; the
specialization symbol is a per-instantiation rewrite.

### Parse `target_type` as a full `TypeSyntax`

**Rejected for v1.** Storing the target as a parsed type instead
of a string plus type-arg list would be cleaner in principle, but
it changes the trait registry's keying from `(String, String)` to
`(String, Type)` and ripples through every `impls.get(...)` call.
The v1 approach keeps the string key and adds a parallel list of
type args. A future ADR can unify them once the trait registry's
keying is refactored for other reasons.

### Support specialization as part of this ADR

**Rejected.** Specialization is a distinct design problem — how to
order impls, how to detect overlap, how to represent the winner in
the IR — that would double the size of this ADR without
justification. It can be added later as a strict extension: the
coherence rule this ADR enforces (`one impl per type-head`) is
the state that specialization would relax.

### Where-clause bounds on impls

**Deferred.** `impl<T: Ord> Sortable for List<T>` requires the
bound to be checked at impl-resolution time and carried through
monomorphization. This is a real feature but adds analyzer surface
area (trait bound satisfaction, which ADR 0025 already lists as
incomplete) that isn't needed for the common case.

## Review record

No outside review yet. This ADR is the first pass.

## References

- ADR 0005 (Ownership Model) — the three receiver modes.
- ADR 0010 (Canonical IR) — why the IR stays method-agnostic.
- ADR 0013 (Executable IR Generic Invariant) — the instantiation
  recording and specialization plan this ADR reuses.
- ADR 0018 (Canonical Pipeline) — where the changes slot in.
- ADR 0025 (Trait Bounds Status) — the bound-satisfaction gap this
  ADR does not close.
- ADR 0033 (Methods, Receivers, and the OOP Direction) — the
  non-generic feature this ADR extends.
- `src/ir/instantiation_plan.rs` — the specialization machinery.
- `tests/conformance/pending/generic_method_impl.gol` — the fixture
  this ADR makes runnable.

## See also

- `docs/features/methods.md` — the feature contract; §Known
  limitations currently lists generic impls as deferred.
- `docs/features/generic.md` — the generic function machinery this
  ADR parallels.
- `docs/features/trait.md` — the trait mechanism generic impls
  implement.
- `docs/decisions/README.md` — the ADR index and convention.