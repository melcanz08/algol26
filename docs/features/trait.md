# Feature: Trait (`trait` / `impl`)

> Per-feature contract, following the pattern described in `docs/architecture-direction.md`.
> This file is the authoritative answer to "what is a trait in ALGOL26, and where does it live?"

## Summary

A **trait** names a set of method signatures that a type can
implement. A trait has zero or more required methods (must be
implemented by every `impl`) and zero or more default methods (have
a body and may be overridden).

`impl TraitName for TypeName` provides the method implementations.
Unlike some languages, ALGOL26 traits are used **only at analysis
time**: a trait-bound function parameter (`x: T` where `T: Trait`)
is resolved at compile time to a concrete type or to a
monomorphized generic. There is no dynamic dispatch, no vtable, and
no runtime trait object.

This is the key design choice of the feature: traits exist to give
the analyzer information about what methods a type has, not to give
the runtime polymorphic behavior. If dynamic dispatch is ever added,
it would be a new feature contract layered on top of this one.

## Syntax

Trait declaration:

```gol
trait Comparable
    function compare(self: Self, other: Self) -> Int
```

Trait with default method:

```gol
trait Display
    function to_string(self: Self) -> String

    function print_it(self: Self) -> Void
        print(self.to_string())
```

Impl block:

```gol
impl Comparable for Int
    function compare(self: Int, other: Int) -> Int
        return self - other
```

Generic impl (impl for a parameterized type):

```gol
impl<T> Printable for List<T>
    function to_string(self: List<T>) -> String
        return "list"
```

Trait bound on a generic parameter:

```gol
function max_of<T: Comparable>(a: T, b: T) -> T
    if a.compare(b) > 0 then
        return a
    else
        return b
```

Method-call syntax on a trait-typed value desugars to a function
call. `x.compare(y)` becomes `Call(compare, [x, y])` at IR
construction — the receiver becomes the first argument. See
`tests/ir/borrow_deref_addrof_test.rs::test_method_call_desugars_to_function_call`.

## Typing rules

| Expression | Type |
|---|---|
| `trait Name ... end` | declares a trait in the trait registry |
| `impl Trait for Type ... end` | registers an impl; type-checks each method against the trait signature |
| `x.method(args)` where `method` is declared on `Trait` and `Type` implements it | return type of the method |
| `function f<T: Trait>(x: T)` | callable only with types that implement `Trait` |

A trait name is not a type. There is no `val x: Comparable` — traits
are used only as bounds on generics and as namespaces for methods.

Traits are **nominal**: implementing `Comparable` requires an
explicit `impl Comparable for T`. There is no structural typing (a
type does not accidentally satisfy a trait by having the same
methods). This is a design decision worth revisiting — see Open
Questions.

### Trait bounds enforcement

When a function is declared `f<T: Trait>(...)`, the analyzer checks
at every call site that the concrete type substituted for `T`
implements `Trait`. If not, the call is rejected.

Tests: `test_comparable_bound_allows_int`,
`test_comparable_bound_rejects_string`,
`test_display_bound_allows_float` in
`tests/semantics/trait_bounds_enforcement.rs`.

### Default methods

A trait may declare a method with a body. The body is available to
every `impl` that does not override it. The body's `self` type is
the impl's concrete type, not the trait. This is resolved at
monomorphization time.

## IR representation

Traits are not represented in the semantic IR. They are resolved
**before** IR construction: by the time `SemanticIRBuilder::build`
runs, every method call has been desugared to a plain function call
with the correct target.

There is no `TypedIRValue::TraitMethodCall` variant. There is no
`Instruction::Dispatch`. Method calls are ordinary calls.

Trait declarations themselves are not carried into the IR either.
After analysis, the IR sees only functions and types. This is why
adding a new trait does not require any IR changes — only analyzer
changes.

Generic trait bounds are resolved through monomorphization.
`src/ir/monomorphize.rs` specializes generic functions for each
concrete type actually used at a call site. Two calls to
`max_of<Int>` and `max_of<Float>` become two distinct monomorphized
functions, and the trait bounds are not present in either.

## Analyzer

The trait machinery lives in `src/semantics/trait_registry/`:

| File | Responsibility |
|---|---|
| `mod.rs` | `TraitRegistry` struct, `TraitDecl`, `GenericImpl` types |
| `register.rs` | `register_trait`, `register_impl` — record declarations |
| `resolve.rs` | `type_implements_trait`, `resolve_method`, `TypePattern` matching |
| `validate.rs` | `validate_impl` — signature checking, undefined-trait rejection |
| `tests.rs` | Registry-level unit tests |

The registry has three maps (from the `TraitRegistry` struct):

- `traits: HashMap<String, TraitDecl>` — trait name to declaration
- `default_methods: HashMap<String, HashMap<String, FunctionDecl>>` —
  trait name to method name to default body
- `generic_impls: Vec<GenericImpl>` — parameterized impls like
  `impl<T> Printable for List<T>`

### Registration flow

1. The analyzer walks the AST and calls `register_trait` for each
   `trait` declaration.
2. Then calls `register_impl` for each `impl` block. The impl's
   trait name must be in the registry; otherwise it is rejected
   with a message naming the undefined trait.
3. `validate_impl` checks that every method required by the trait
   is either provided by the impl or has a default in the trait.

The `validate.rs` diff from rustfmt confirms the code path: an impl
of an undefined trait is rejected before any method checking happens.

### Resolution flow

When the analyzer encounters `x.method(args)` where `x: T`:

1. Look up `method` in the traits that `T` implements.
2. If found, desugar to `Call(method, [x, ...args])`.
3. If not found, report a missing-method error.

`resolve.rs` handles `TypePattern::Generic(name, args)` — a generic
type pattern where `name` is a single uppercase letter matches any
type. This is what makes `impl<T> Printable for List<T>` apply to
`List<Int>`, `List<String>`, and so on.

## Backends

| Backend | Support | Evidence |
|---|---|---|
| Interpreter | Supported (indirectly) | No trait-specific instructions; ordinary calls run |
| LLVM | Supported (indirectly) | Same — trait resolution happens before IR |
| WASM | Supported (indirectly) | Same |

Traits are a **compile-time-only** feature. They do not appear in
the IR, so every backend supports them for free. A program using
traits compiles to the same IR as the equivalent program with the
method calls manually desugared.

This is the reason there are no capability-matrix entries for
traits. The capability system checks IR features, not language
features, and traits never reach the IR.

## Diagnostics

Trait-related error codes currently emitted:

**None in the `E-XXX-NNN` format.** Trait errors are produced as
plain `String` messages through the analyzer's `Result<(), String>`
return type, not as coded diagnostics.

Examples of trait error messages I have seen in test assertions:

- "Impl references undefined trait 'NAME'" — from `validate.rs`
- Method-signature-mismatch messages — from `validate_impl`
- "Type does not implement trait 'NAME'" — from bound checking

**This is a gap.** Every other feature in the language has an
`E-XXX-NNN` diagnostic vocabulary; traits do not. Bringing trait
diagnostics into the coded system is a Tier 2 follow-up.

## Safety

- No runtime polymorphism means no vtable and no dynamic dispatch
  errors.
- Bound violations are caught at analysis time, before IR
  construction. A call `f(x)` where `x` does not implement the
  required trait is rejected with a compile error, not a runtime
  check.
- Default methods are monomorphized per impl. There is no shared
  code and no possibility of a default method accidentally seeing
  a different `Self` at runtime.

## Test coverage

Current coverage across the tree:

**Registry unit tests (`src/semantics/trait_registry/tests.rs`):**

- `test_register_trait` — a trait declaration enters the registry
- `test_generic_impl` — a parameterized impl is registered
- `test_type_implements_trait` — resolution returns true for a
  concrete impl
- `test_validate_impl_signature_mismatch` — a method with the wrong
  signature is rejected

**Semantics-level (`tests/semantics/`):**

- `test_trait_bounds_enforcement.rs`: three tests for generic bounds
- `test_trait_method_test.rs`: `test_trait_registration_and_lookup`,
  `test_trait_registry_resolution`

**Corpus:**

- `corpus_27_trait_basic.gol` — a trait with no implementation
- `corpus_28_trait_impl.gol` — one impl
- `corpus_29_trait_two_impls.gol` — two impls of the same trait
- `corpus_37_trait_no_arg.gol` — trait method with no arguments

**Examples:**

- `examples/traits/trait_test.gol`
- `examples/traits/trait_bounds_test.gol`

**Parser:** trait and impl parsing is covered by
`frontend/lexer/tests::test_mut_in_signature_lexes_as_keyword`
(shared keywords) and by the corpus program presence — I have not
seen dedicated parser tests for `trait`/`impl` syntax.

### Gaps

- **No negative test for method-signature mismatch via the full
  analyzer path.** The registry-level test exists but a
  compile-a-`.gol`-file test would be stronger.
- **No test for a trait with a default method used by an impl that
  does not override it.** The default-method code path exists in the
  struct (`default_methods` map) but I have not seen a test that
  exercises the override-vs-inherit distinction.
- **No test for conflicting impls.** Two `impl Foo for Bar` blocks
  — is the second one rejected? Silently overriding? Not tested.
- **No test for a generic impl matched against multiple concrete
  types.** `impl<T> Printable for List<T>` should make both
  `List<Int>` and `List<String>` printable; no test confirms this.
- **No per-backend fixture.** Since traits are compile-time only,
  a differential test would not exercise anything special, but a
  conformance program under `tests/conformance/valid/trait_*.gol`
  would be useful documentation.

## Maturity

Following the stages in `docs/architecture-direction.md`:

```
trait / impl
    semantics:   Stable (one open question on nominal vs structural)
    parsed:      yes
    typed:       yes
    validated:   yes (registry + validate_impl)
    IR:          N/A (fully resolved before IR construction)
    verified:    N/A
    interpreter: supported (indirectly)
    LLVM:        supported (indirectly)
    WASM:        supported (indirectly)
    optimized:   N/A
```

Traits are unique among the feature contracts in this directory:
they are the only feature with **no backend asymmetry**, because
they never reach the backends. Whatever the analyzer accepts, every
backend executes.

## Checklist for related features

If you are adding a feature *like* traits (a compile-time-only
abstraction that resolves before IR construction), you need to
touch:

1. `src/frontend/ast.rs` — new AST nodes for the declaration syntax.
2. `src/frontend/parser/` — parse the new syntax into those nodes.
3. `src/semantics/trait_registry/` or a new registry module — the
   compile-time data structure.
4. `src/semantics/analyzer/` — hooks to register, validate, and
   resolve through the new registry.
5. `src/ir/semantic_ir.rs` — likely nothing, if the feature is fully
   resolved before IR construction. This is the win.
6. `tests/semantics/` — analyzer-level tests.
7. `tests/corpus/` — end-to-end programs demonstrating the feature.
8. `docs/features/<feature>.md` — this file.

Note that the checklist is much shorter than for features that
reach the IR. The absence of items 2–11 from the `list.md`
checklist reflects the design choice that keeps compile-time-only
features cheap to add.

## Open questions

- **Nominal vs structural typing.** Currently a type must declare
  `impl Trait for Type` to satisfy `T: Trait`. A structural system
  would let any type that happens to have a matching `method`
  satisfy the bound. Structural typing is more ergonomic but
  interacts badly with method-name collisions and with the
  monomorphization rules. Current choice is deliberate.

- **Trait inheritance.** A trait can currently only require methods,
  not other traits. `trait Ordered: Comparable` (requiring `Comparable`)
  is not expressible. This is a common feature in other languages
  and is likely the next extension.

- **Associated types.** No `type Item;` in traits. Iterator-like
  traits need this. Not currently planned.

- **Trait objects.** No `Box<dyn Trait>` or equivalent. Requires
  runtime dispatch, which the current design avoids. If added, it
  is a new feature layered on this one, with its own contract.

- **Where should trait diagnostics get coded?** Every other feature
  uses `E-XXX-NNN` codes; traits use free-form strings. This is a
  Tier 2 item.

- **Should `validate_impl` catch more cases?** The current
  validation checks trait existence and method signatures. It does
  not check for: duplicate impls of the same trait for the same
  type, impls of a trait for a type the trait was not declared for,
  or default-method overrides that change the signature. Each of
  these is a possible extension.

## See also

- `docs/architecture-direction.md` — the feature contract pattern itself
- `docs/features/generic.md` — (to be written) trait bounds are the
  bridge between traits and generics
- `docs/decisions/0003-type-system.md` — the type system this feature
  extends
- `src/semantics/trait_registry/` — the registry implementation
- `src/semantics/analyzer/` — where the registry is called
- `src/ir/monomorphize.rs` — specialization of generic functions
- `tests/semantics/trait_bounds_enforcement.rs`
- `tests/semantics/trait_method_test.rs`
- `tests/corpus/corpus_27_trait_basic.gol`
- `examples/traits/trait_test.gol`
