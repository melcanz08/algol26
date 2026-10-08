# Feature: Trait (`trait` / `impl`)

> Per-feature contract, following the pattern described in `docs/architecture-direction.md`.
> This file is the authoritative answer to "what is a trait in ALGOL26, and where does it live?"

## Summary

A **trait** names a set of method signatures that a type can
implement. `impl TraitName for TypeName` provides the method
implementations. Traits are used **only at analysis time**: a
trait-bound function parameter is resolved at compile time to a
concrete type or to a monomorphized generic. There is no dynamic
dispatch, no vtable, and no runtime trait object.

Traits exist to give the analyzer information about what methods a
type has. They do not exist to give the runtime polymorphic
behavior. If dynamic dispatch is ever added, it would be a new
feature contract layered on top of this one.

## Syntax

Trait declaration. The method list names required methods; the
receiver is not declared — it is bound to `self` at the `impl` site:

```gol
trait Comparable
    function compare(other: Self) -> Int
```

Impl block for a concrete type:

```gol
impl Comparable for Int
    function compare(other: Int) -> Int
        return self - other
```

Impl block for a record:

```gol
rec Point
    x: Int
    y: Int

impl Show for Point
    function show() -> String
        return "(" + Int.to_string(self.x) + ", " + Int.to_string(self.y) + ")"
```

Inside an `impl` method body, `self` is bound to the receiver.
The receiver is inserted by `expand_impl_methods` in the frontend
normalization step; the method signature does not declare it.

Trait bound on a generic parameter, using the `where` clause form:

```gol
function max_of<T>(a: T, b: T) -> T where T: Comparable
    if a.compare(b) > 0 then
        return a
    else
        return b
```

### Method-call syntax

Calling a trait method on a value uses the `x.method()` form:

```gol
val p := Point { x: 1, y: 2 }
print(p.show())
```

The parser generates `FunctionCall { name: "p.show", args: [] }`,
and the IR builder's `resolve_method_call` resolves the callee to
the impl-mangled name (`Point_show`) before emitting a `Call`.
The receiver is prepended as the first argument at IR construction
time.

## Typing rules

| Expression | Type |
|---|---|
| `trait Name ... end` | declares a trait in the trait registry |
| `impl Trait for Type ... end` | registers an impl; type-checks each method against the trait signature |
| `x.method(args)` where `method` is declared on `Trait` and `Type` implements it | return type of the method |
| `function f<T>(x: T) -> R where T: Trait` | callable only with types that implement `Trait` |

A trait name is not a type. There is no `val x: Comparable`.
Traits are used only as bounds on generics and as namespaces for
methods.

Traits are **nominal**: implementing `Comparable` requires an
explicit `impl Comparable for T`. There is no structural typing
(a type does not accidentally satisfy a trait by having the same
methods). This is a design decision worth revisiting — see Open
Questions.

### Trait bounds enforcement

`where T: Trait` is accepted if `Trait` is a declared trait.
Whether the concrete type substituted for `T` at a call site
implements the trait is not currently checked — see ADR 0025 and
the "Open questions" section below.

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

## Analyzer

The trait machinery lives in `src/semantics/trait_registry/`:

| File | Responsibility |
|---|---|
| `mod.rs` | `TraitRegistry` struct, `TraitDecl`, `GenericImpl` types |
| `register.rs` | `register_trait`, `register_impl` — record declarations |
| `resolve.rs` | `type_implements_trait`, `resolve_method`, `TypePattern` matching |
| `validate.rs` | `validate_impl` — signature checking, undefined-trait rejection |
| `tests.rs` | Registry-level unit tests |

The registry tracks declared traits, their methods, and the impls
registered for each (trait, target-type) pair.

### Registration flow

1. The analyzer walks the AST and calls `register_trait` for each
   `trait` declaration.
2. Then calls `register_impl` for each `impl` block. The impl's
   trait name must be in the registry; otherwise it is rejected
   with a message naming the undefined trait.
3. `validate_impl` checks that every method required by the trait
   is provided by the impl, with a matching signature.

### Resolution flow

When the analyzer encounters `x.method(args)` where `x: T`:

1. Look up `method` in the traits that `T` implements.
2. If found, use the method's return type.
3. If not found, report a missing-method error.

`resolve.rs` handles `TypePattern::Generic(name, args)` — a generic
type pattern where `name` is a single uppercase letter matches any
type. This is what makes a parameterized impl apply to concrete
instantiations.

The IR builder has a parallel resolution path: `resolve_method_call`
in `src/semantics/builder/values.rs` looks up the method name in
`function_types`. For records, it tries the impl-mangled form
(`<Record>_<method>`) before falling through to the builtin path.
This was added during the CLI exercises — see commit `f5bc75d`.

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

The `✅` on the capability matrix's `Traits + impls` row for LLVM
means a trait-using program compiles and runs when its `impl`
bodies use features LLVM supports. A `show()` implementation that
returns a `String` is fine; one that constructs a record would be
refused because records are refused.

## Diagnostics

Trait errors are produced as `CompileError` values with codes
`E0002` (type errors) and `E0004` (unknown names). Examples:

- "Unknown trait 'NAME'" — when `impl` or `where` names a trait
  that was not declared.
- "Type does not implement trait 'NAME'" — from bound checking.
- Method-signature-mismatch messages — from `validate_impl`.

Trait diagnostics use the general type-system codes rather than a
trait-specific `E-XXX-NNN` vocabulary. This is the same gap as
generics; bringing them into the coded system is a Tier 2
follow-up.

## Safety

- No runtime polymorphism means no vtable and no dynamic dispatch
  errors.
- Trait and impl declarations are validated before IR construction.
- `impl` methods are monomorphized per target type. There is no
  shared code and no possibility of a method body accidentally
  seeing a different `Self` at runtime.

## Test coverage

**Registry unit tests (`src/semantics/trait_registry/tests.rs`):**

- `test_register_trait` — a trait declaration enters the registry
- `test_generic_impl` — a parameterized impl is registered
- `test_type_implements_trait` — resolution returns true for a
  concrete impl
- `test_validate_impl_signature_mismatch` — a method with the wrong
  signature is rejected

**Semantics-level (`tests/semantics/`):**

- `trait_bounds_enforcement.rs` — generic-bound tests
- `trait_method_test.rs`: `test_trait_registration_and_lookup`,
  `test_trait_registry_resolution`

**Interpreter (end-to-end):**

- `record_method_call_dispatches_to_impl` in
  `src/backends/interpreter/tests.rs` exercises `impl Show for
  Point` + `p.show()` end to end.
- `tests/features.gol` in the CLI validation repo exercises the
  same shape plus `affirm`.

**Corpus:**

- `corpus_27_trait_basic.gol` — a trait with no implementation
- `corpus_28_trait_impl.gol` — one impl
- `corpus_29_trait_two_impls.gol` — two impls of the same trait
- `corpus_37_trait_no_arg.gol` — trait method with no arguments

**Examples:**

- `examples/traits/trait_test.gol`
- `examples/traits/trait_bounds_test.gol`

### Gaps

- **No negative test for method-signature mismatch via the full
  analyzer path.** The registry-level test exists but a
  compile-a-`.gol`-file test would be stronger.
- **No test for conflicting impls.** Two `impl Foo for Bar` blocks
  — is the second one rejected? Silently overriding? Not tested.
- **No test for a generic impl matched against multiple concrete
  types.** A parameterized impl should make both `List<Int>` and
  `List<String>` printable; no test confirms this.
- **No per-backend fixture.** Since traits are compile-time only,
  a differential test would not exercise anything special, but a
  conformance program under `tests/conformance/valid/trait_*.gol`
  would be useful documentation.

## Maturity

Following the stages in `docs/architecture-direction.md`:

```
trait / impl
    semantics:   Stable (nominal; bounds name-resolved only)
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

Traits are the only feature in this directory with **no backend
asymmetry**, because they never reach the backends. Whatever the
analyzer accepts, every backend executes (subject to the capability
matrix of the features the impl bodies use).

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
5. `src/semantics/builder/values.rs::resolve_method_call` — if the
   feature's methods are called via `x.method()`, add the mangled
   name resolution path there.
6. `src/ir/semantic_ir/` — likely nothing, if the feature is fully
   resolved before IR construction. This is the win.
7. `tests/semantics/` — analyzer-level tests.
8. `tests/corpus/` — end-to-end programs.
9. `docs/features/<feature>.md` — this file.

The checklist is much shorter than for features that reach the IR.

## Open questions

- **Nominal vs structural typing.** Currently a type must declare
  `impl Trait for Type` to satisfy `T: Trait`. A structural system
  would let any type that happens to have a matching `method`
  satisfy the bound. Structural typing is more ergonomic but
  interacts badly with method-name collisions and with the
  monomorphization rules. Current choice is deliberate.
- **Bounds are name-resolved only (ADR 0025).** `where T: Trait`
  is accepted if `Trait` is declared; whether the concrete type
  substituted for `T` at a call site implements the trait is not
  checked. Closing this gap is the highest-value trait-system
  follow-up.
- **Trait inheritance.** A trait can currently only require methods,
  not other traits. `trait Ordered: Comparable` is not expressible.
- **Associated types.** No `type Item;` in traits. Iterator-like
  traits need this. Not currently planned.
- **Default methods.** A trait method cannot yet have a body. The
  `TraitMethod` AST node carries `name`, `params`, and
  `return_type`, but no `body`. Adding default methods is a
  follow-up.
- **Trait objects.** No `Box<dyn Trait>` or equivalent. Requires
  runtime dispatch, which the current design avoids.
- **Should `validate_impl` catch more cases?** The current
  validation checks trait existence and method signatures. It does
  not check for duplicate impls of the same trait for the same
  type, or impls of a trait for a type the trait was not declared
  for. Each of these is a possible extension.

## See also

- `docs/features/dyn_trait.md` — borrowed dynamic dispatch (`&dyn Trait` / `&mut dyn Trait`)
- `docs/decisions/0038-dynamic-dispatch.md` — the ADR for dynamic dispatch
- `docs/architecture-direction.md` — the feature contract pattern itself
- `docs/features/generic.md` — trait bounds are the bridge between
  traits and generics
- `docs/decisions/0003-type-system.md` — the type system this feature
  extends
- `docs/decisions/0025-trait-bounds-status.md` — the name-resolution
  only status of bounds checking
- `src/semantics/trait_registry/` — the registry implementation
- `src/semantics/builder/values.rs` — `resolve_method_call`
- `tests/semantics/trait_bounds_enforcement.rs`
- `tests/semantics/trait_method_test.rs`
- `tests/corpus/corpus_27_trait_basic.gol`
- `examples/traits/trait_test.gol`