# Feature: Generic (`<T>`, `<T: Bound>`)

> Per-feature contract, following the pattern described in `docs/architecture-direction.md`.
> This file is the authoritative answer to "what are generics in ALGOL26, and where do they live?"

## Summary

A **generic** is a function, type, or impl parameterized over a type
or types. Generic parameters are written in angle brackets:
`function f<T>(x: T) -> T`. The parameter `T` stands for a concrete
type supplied at each call site.

ALGOL26 uses **monomorphization**, not dynamic dispatch. When the
analyzer sees `f<Int>(x)`, it records that a specialization of `f`
for `Int` is needed. `src/ir/monomorphize.rs` runs before IR
construction and produces one concrete function per
`(generic function, concrete type args)` pair. The runtime sees
only concrete functions.

This is the same discipline as traits: all generic machinery is
resolved at compile time, before the IR is built. Every backend
supports generics for free.

## Syntax

Generic function:

```gol
function identity<T>(x: T) -> T
    return x
```

Generic function with two parameters:

```gol
function swap<A, B>(a: A, b: B) -> B
    return b
```

Generic type parameter with a trait bound:

```gol
function max_of<T: Comparable>(a: T, b: T) -> T
    if a.compare(b) > 0 then
        return a
    else
        return b
```

Generic impl block:

```gol
impl<T> Printable for List<T>
    function to_string(self: List<T>) -> String
        return "list"
```

Generic type in a variable declaration:

```gol
val nums: List<Int> := [1, 2, 3]
val ids: Result<Int, String> := Ok(42)
```

Type parameters are **single uppercase letters** by convention, and
the parser accepts any single-uppercase-letter identifier as a type
variable. A multi-letter identifier is treated as a concrete type
name. See `Type::from_str` in `src/common/types.rs`:

```rust
if s_trimmed.len() == 1 {
    if let Some(c) = s_trimmed.chars().next() {
        if c.is_uppercase() {
            return Type::TypeVar(s_trimmed.to_string());
        }
    }
}
```

## Typing rules

### Type variables

`Type::TypeVar(String)` represents an unsubstituted type parameter.
It is **not** a type in the normal sense — it is a placeholder that
must be substituted before use.

| Operation | Behavior with TypeVar |
|---|---|
| `can_coerce_to(TypeVar(T), X)` | `true` for any `X` |
| `can_coerce_to(X, TypeVar(T))` | `true` for any `X` |
| `common_supertype(TypeVar(T), X)` | `X` (concrete wins) |
| `common_supertype(X, TypeVar(T))` | `X` |
| `substitute({T -> Int})` | replaces `TypeVar("T")` with `Int` |
| `contains_type_var()` | `true` |

The `can_coerce_to` behavior is **permissive by design**: during
type inference, `T` can unify with any concrete type, and forcing a
coercion check would reject valid programs. The soundness guarantee
is that every `TypeVar` is resolved to a concrete type before IR
construction — a program that reaches codegen with an unresolved
`TypeVar` in its type table is a compiler bug, not a user error.

### Instantiation

When `f<Int>(x)` is analyzed:

1. Look up `f`'s signature: `function f<T>(x: T) -> T`.
2. Substitute `T := Int` throughout the signature.
3. Result: `f` specialized at `Int` has signature
   `function f_Int(x: Int) -> Int`.
4. Call is type-checked against the specialized signature.
5. A specialization request `(f, [Int])` is recorded for the
   monomorphizer.

### Type parameter inference

Where a call does not spell out the type parameter, it is inferred
from the argument types. `identity(42)` infers `T := Int` from the
argument. Where inference fails (e.g. `identity([])` — `T` is the
element type of an empty list), the caller must annotate:
`identity<Int>([])`.

## IR representation

### Before monomorphization

The generic function is present in the analyzer's type table with
its parameterized signature. The IR builder does **not** see
`TypeVar` types in value positions — by the time
`SemanticIRBuilder::build` runs, the type table has been populated
with concrete specializations.

### After monomorphization

`src/ir/monomorphize.rs` walks the type table and for each
specialization request `(function_name, type_args)` produces a new
`SemanticFunction` with:

- A name of the form `<function_name>_<type1>_<type2>_...`. The
  exact format is pinned by
  `test_two_param_generic_name_follows_declaration_order`.
- Parameters with `TypeVar` replaced by the concrete types.
- Body with `TypedIRValue::Variable` types substituted.
- Nested substitutions recursed into list elements, array indices,
  and field accesses (see `substitution_recurses_into_array_index`).

The specialization name is **stable across runs**
(`test_specialized_name_is_stable_across_runs`). Two compilations
of the same source produce byte-identical IR. This matters for
incremental builds and for differential testing, where the LLVM and
interpreter paths must see the same IR.

### IR verifier

The IR verifier sees only concrete functions. A `TypeVar` in the
final IR would be a compiler bug. There is no explicit verifier
rule for "no TypeVar in IR" — it is a precondition the
monomorphizer is expected to satisfy.

Adding such a check would be a small Tier 2 item: a scan over the
final `SemanticProgram` that returns an error if any type is
`TypeVar(_)` or `Generic { .. }` with unsubstituted args.

## Type substitution

`Type::substitute(&self, substitutions: &HashMap<String, Type>) -> Type`
is the core operation. It recurses into every composite type:

| Type | Behavior |
|---|---|
| `TypeVar(name)` | lookup in map; if present, replace; else keep |
| `List(inner)` | substitute inner |
| `Array(inner, size)` | substitute inner, keep size |
| `Tuple(elements)` | substitute each |
| `Option(inner)` | substitute inner |
| `Result { ok, error }` | substitute both |
| `Pointer(inner)`, `Borrow(inner)`, `MutBorrow(inner)` | substitute inner |
| `Channel(inner)` | substitute inner |
| `Generic { name, args }` | substitute each arg |
| `Function { params, return_type }` | substitute each param and the return |
| all others | return clone unchanged |

The recursion into every composite is what makes nested generics
work: `List<T>` where `T = Int` becomes `List<Int>`.

## Backends

| Backend | Support | Evidence |
|---|---|---|
| Interpreter | Supported (indirectly) | Sees only monomorphized concrete functions |
| LLVM | Supported (indirectly) | Same |
| WASM | Supported (indirectly) | Same |

Like traits, generics are a compile-time-only feature. By the time
any backend executes the program, every generic has been resolved
to a concrete specialization. There are no `T` values at runtime,
no generic container layouts that depend on an unresolved type, and
no per-backend work to do.

This means: **adding a new generic type is nearly free on the
backend side**. The cost is entirely in parsing, type checking, and
monomorphization.

## Monomorphization cost

The current implementation generates one specialization per
concrete `(function, type args)` pair. A generic function called
with `Int` and `String` produces two specializations. A generic
function called with `List<Int>` and `List<String>` produces two
more.

**Code size grows with the number of distinct instantiations, not
with the number of call sites.** Calling `identity<Int>(x)` a
thousand times produces one specialization, not a thousand.

There is no dead-specialization elimination. If a specialization is
produced but never called (e.g. because of dead code elimination at
the AST level), the specialization is still emitted. This is a
possible optimizer pass; not currently implemented.

## Diagnostics

Generic-related error codes currently emitted:

**None in the `E-XXX-NNN` format.** Like traits, generic errors are
produced as free-form `String` messages through the analyzer's
`Result<(), String>` path.

Examples of generic error messages:

- "Cannot infer type parameter `T`" — when inference fails
- "Type argument `X` does not satisfy bound `Y`" — from trait-bound
  checking
- "Wrong number of type arguments for `f`: expected N, got M"

This is the same gap as traits. Bringing generic diagnostics into
the coded system is a Tier 2 follow-up.

## Safety

- No monomorphization loop: the recursion is bounded by the depth
  of the type graph. A generic function `f<T>` that calls itself
  with `f<List<T>>` would loop forever, but the analyzer rejects
  such calls because it cannot bound the expansion. See Open
  Questions.
- No runtime type errors: every type is concrete before codegen.
- No dynamic dispatch: the specialization name is known statically.

## Test coverage

Current coverage across the tree:

**Monomorphizer (`src/ir/monomorphize.rs::tests`):**

- `substitution_recurses_into_array_index` — nested substitution
  inside array access works
- `two_param_generic_name_follows_declaration_order` — the
  specialization name format is deterministic
- `specialized_name_is_stable_across_runs` — two compilations
  produce identical names

**Corpus:**

- `corpus_generics/generics_test.gol` (under `examples/generics/`)

**Type-level (`src/common/types.rs::tests`):**

- `test_contains_type_var` — detects TypeVar in composite types
- `test_substitute` — substitution through `List<T>` works
- `test_type_parsing` — `T` parses as `TypeVar("T")`

**Semantics-level:**

- Trait bounds on generics: `test_comparable_bound_allows_int`,
  `test_comparable_bound_rejects_string`,
  `test_display_bound_allows_float` in
  `tests/semantics/trait_bounds_enforcement.rs`

### Gaps

- **No test for monomorphization of a generic that takes a generic
  type argument.** `identity<List<Int>>(x)` — is this expanded
  correctly? Not tested.
- **No test for a generic function calling another generic
  function.** `f<T>` calls `g<T>` — is the specialization chain
  correct? Not tested.
- **No test for a generic function with a `Result<T, E>` return
  type.** Substitution through `Result` is handled by `substitute`
  but the analyzer path from a generic return type to a concrete
  monomorphized function is not tested.
- **No test for a generic with a trait bound that calls the bound's
  method.** `max_of<T: Comparable>` calls `a.compare(b)` — the
  desugaring to a concrete method call at monomorphization time is
  not covered.
- **No differential test.** Since generics are resolved before IR,
  a differential test would exercise the same concrete function on
  both backends — useful as a smoke test, but not specifically a
  generics test.
- **No test that a `TypeVar` in the final IR is rejected.** The
  precondition is unchecked. A small soundness test would scan
  `SemanticProgram` for `TypeVar` and panic.

## Maturity

Following the stages in `docs/architecture-direction.md`:

```
Generic (<T>)
    semantics:   Stable
    parsed:      yes
    typed:       yes
    validated:   yes
    IR:          via monomorphization (no TypeVar reaches IR)
    verified:    N/A (verifier sees concrete IR)
    interpreter: supported (indirectly)
    LLVM:        supported (indirectly)
    WASM:        supported (indirectly)
    optimized:   no dead-specialization elimination
```

Like traits, generics have no backend asymmetry. The entire feature
lives in the frontend and the monomorphizer.

## Checklist for related features

If you are adding a feature *like* generics (compile-time type
parameterization), you need to touch:

1. `src/common/types.rs` — `TypeVar` handling in `substitute`,
   `contains_type_var`, `common_supertype` (already present for the
   existing `TypeVar`; new features may need additional type
   parameters).
2. `src/frontend/parser/` — parsing of new type-parameter syntax.
3. `src/frontend/ast.rs` — AST nodes for the parameterized
   declaration.
4. `src/semantics/analyzer/` — type inference and instantiation
   logic.
5. `src/semantics/trait_registry/` — if the feature uses trait
   bounds, registration and validation there.
6. `src/ir/monomorphize.rs` — specialization generation.
7. `src/ir/semantic_ir.rs` — likely nothing if the feature is
   resolved before IR construction.
8. `tests/semantics/` — analyzer-level tests for inference and
   bound checking.
9. `tests/corpus/` — end-to-end programs.
10. `docs/features/<feature>.md` — this file.

## Open questions

- **Is monomorphization terminating?** The current design assumes
  finite type-argument nesting. A generic that instantiates itself
  with a larger type (`f<T>` calls `f<List<T>>`) would loop. The
  analyzer is expected to reject such calls, but I have not seen
  the code that does this rejection. If it does not exist, a
  user program could hang the compiler. This should be confirmed
  and, if necessary, a depth limit added.

- **Should generic diagnostics use `E-XXX-NNN` codes?** Same gap as
  traits. Recommended for Tier 2.

- **Should there be dead-specialization elimination?** A generic
  function called only with `Int` that is then eliminated by DCE
  still produces an `f_Int` specialization. Removing it would
  require a reachability pass over the specialization graph.
  Currently not implemented.

- **Should `TypeVar` be allowed to reach the IR?** Currently no —
  the monomorphizer's job is to eliminate it. An explicit
  precondition check would make the invariant visible. A small
  Tier 2 item.

- **What happens if the analyzer cannot infer a type parameter?**
  The current behavior is an error requiring an explicit type
  argument. Whether the error is emitted at the right place (the
  call, not the function definition) is not verified by tests.

- **Are generic type parameters covariant?**
  `Type::Generic { name, args }` covariance follows
  `can_coerce_to` on the args — `Generic("List", [Int])` coerces
  to `Generic("List", [Float])`. Since `Generic` and `List`
  overlap in the type system (both can represent `List<Int>`), it
  is unclear which the analyzer uses. Worth confirming.

## See also

- `docs/architecture-direction.md` — the feature contract pattern itself
- `docs/features/trait.md` — trait bounds on generics
- `docs/decisions/0003-type-system.md` — the type system
- `src/common/types.rs` — `TypeVar`, `Generic`, `substitute`
- `src/ir/monomorphize.rs` — the specialization pass
- `tests/semantics/trait_bounds_enforcement.rs`
- `examples/generics/generics_test.gol`
