# Feature: Generic (`<T>`, `where T: Bound`)

> Per-feature contract, following the pattern described in `docs/architecture-direction.md`.
> This file is the authoritative answer to "what are generics in ALGOL26, and where do they live?"

## Summary

A **generic** is a function parameterized over one or more types.
Generic parameters are written in angle brackets:
`function f<T>(x: T) -> T`. The parameter `T` stands for a concrete
type supplied at each call site.

ALGOL26 uses **monomorphization**, not dynamic dispatch. The
analyzer records each generic call site's type arguments; a plan
closes over the transitive specialization graph; and the IR
builder emits one concrete `SemanticFunction` per (generic
function, concrete type-args) pair. The backends see only
concrete functions.

This is the same discipline as traits: all generic machinery is
resolved at compile time, before the backends run. Every backend
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

Generic type parameter with a trait bound, using the `where`
clause form:

```gol
function max_of<T>(a: T, b: T) -> T where T: Comparable
    if a.compare(b) > 0 then
        return a
    else
        return b
```

Generic parameter used inside a container type (the container
case was fixed during the CLI exercises — see "Type parameter
inference" below):

```gol
function first<T>(xs: List<T>) -> Option<T>
    if List.length(xs) == 0
        return None
    return Some(xs[0])
```

Generic type in a variable declaration:

```gol
val nums: List<Int> := [1, 2, 3]
val ids: Result<Int, String> := Ok(42)
```

Type parameters are **single uppercase letters** by convention. The
parser accepts any single-uppercase-letter identifier as a type
variable; multi-letter identifiers are resolved by the analyzer
against the record table and then `TypeSyntax::to_type`'s primitives.
See `Type::from_str` in `src/common/types.rs`.

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

### Type parameter inference

Type parameters are inferred from the call's argument types at the
call site. The analyzer's `unify_types` (in
`src/semantics/analyzer/expr.rs`) recursively binds type variables
inside container types:

- `identity(42)` binds `T = Int`.
- `first([1, 2, 3])` binds `T = Int`, because `List<T>` against
  `List<Int>` recurses into the element.
- `first(rows)` where `rows: List<Sale>` binds `T = Sale`.
- `identity(Some(42))` binds `T = Option<Int>`.

The recursion covers `List<T>`, `Option<T>`, `Result<T, E>`,
`Map<K, V>`, `Pointer<T>`, `Array<T, N>`, `Channel<T>`, and
nested combinations. A `Type::TypeVar` directly as a parameter type
still binds — that path is unchanged from before `unify_types`.

Explicit call-site type arguments (`identity<Int>(42)`) are not
supported by the parser. Where inference cannot determine a
parameter, the program is rejected with a diagnostic. In practice,
inference from argument types succeeds for every well-typed call.

### Instantiation plan

The analyzer records one `Instantiation` fact per generic call
site in `SemanticAnalyzer::instantiations`. Each fact carries the
call site's `ExprId`, the callee's name, the declared type-parameter
names in order, and the inferred concrete `type_args`.

`InstantiationPlan::from_instantiations` builds two tables:

- `call_sites: HashMap<ExprId, CallSiteInstantiation>` — every
  generic call site.
- `specializations: HashMap<String, Specialization>` — concrete
  specializations, keyed by mangled name (`function_Type1_Type2`).

Symbolic call sites — those inside another generic's body, whose
type args are still `TypeVar` or `Unknown` — are recorded but do
not produce a specialization immediately.

`InstantiationPlan::close(functions)` resolves the transitive
closure: it walks each concrete specialization's body under that
specialization's bindings, substitutes symbolic type args into
concrete ones, and iterates to fixpoint. The worklist uses a
`HashSet` of already-seen mangled names, so the closure terminates
even for mutually-recursive generic calls. See ADR 0013.

### IR representation

The IR builder emits one `SemanticFunction` per specialization of
each generic function, named by
`mangled_name(function, type_args)` — e.g. `identity_Int`,
`pair_String_Int`. Non-generic functions are emitted unchanged.
The original generic function templates are analyzer input, not
compiler output.

`resolved_callee_name` in the IR builder rewrites each generic call
site's callee to the mangled name of its matching specialization.
If the plan has no matching specialization — which should not
happen after `close` — the builder emits a diagnostic naming the
unresolved call rather than falling back silently.

The IR verifier sees only concrete functions. A `TypeVar` in the
final IR would be a compiler bug. ADR 0014's invariant check
rejects `Type::TypeVar` in executable IR via
`crate::ir::verifier::invariants`.

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
| `Map(k, v)` | substitute key and value |
| `Generic { name, args }` | substitute each arg |
| `Record(name, args)` | substitute each type argument |
| `Function { params, return_type }` | substitute each param and the return |
| all others | return clone unchanged |

The recursion into every composite is what makes nested generics
work: `List<T>` where `T = Int` becomes `List<Int>`; `Map<String,
Pair<T, T>>` where `T = Float` becomes `Map<String, Pair<Float,
Float>>`.

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
the instantiation plan.

## Monomorphization cost

The current implementation generates one specialization per
concrete `(function, type args)` pair. A generic function called
with `Int` and `String` produces two specializations. A generic
function called with `List<Int>` and `List<String>` produces two
more.

**Code size grows with the number of distinct instantiations, not
with the number of call sites.** Calling `identity(42)` a thousand
times produces one specialization, not a thousand.

There is no dead-specialization elimination. If a specialization is
produced but never called (e.g. because of dead code elimination at
the AST level), the specialization is still emitted. This is a
possible optimizer pass; not currently implemented.

## Diagnostics

Generic-related error messages are produced as `CompileError`
values with `E0002` (type errors) or `E0004` (unknown names) codes:

- "Type mismatch for generic parameter 'T': expected X, found Y" —
  when two arguments disagree on a type parameter's binding
  (`unify_types` in the analyzer).
- "Argument 'name' type mismatch: expected X, found Y" — when an
  argument doesn't coerce to the resolved parameter type.
- "Generic call to `f` still has unresolved type arguments after
  substitution" — when the plan cannot resolve a call's type args
  even after `close`.
- "Generic call to `f` has no matching specialization in the plan"
  — when the plan has no entry for a call that should have one.

All are `E0002` (or `E0004`) rather than the `E-XXX-NNN` format
used elsewhere. Bringing generic diagnostics into the coded system
is a Tier 2 follow-up; the same gap as traits.

## Safety

- The instantiation plan's worklist terminates: the visited set is
  keyed by mangled name, which is injective, so each specialization
  is queued at most once.
- No runtime type errors: every type is concrete before codegen.
- No dynamic dispatch: the specialization name is known statically.
- A `TypeVar` reaching the IR is rejected by the executable-IR
  invariant check (ADR 0014).

## Test coverage

**Instantiation plan (`src/ir/instantiation_plan.rs::tests`):**

- `empty_instantiations_produce_empty_plan`
- `one_concrete_instantiation_creates_one_specialization`
- `two_instantiations_of_same_function_produce_two_specializations`
- `symbolic_instantiation_records_call_site_but_no_specialization`
- `closure_materializes_single_hop_specialization` — `f<T>` calls
  `g<T>`, closure materializes `g`
- `closure_materializes_two_hop_specialization` — `f` → `g` → `h`
- `closure_with_composite_type_argument` — `List<Int>` as a
  type argument
- `closure_is_idempotent` — running `close` twice produces the
  same plan
- `closure_handles_multiple_concrete_calls` — `outer(1)` and
  `outer("hello")` materialize both specializations

**Type-level (`src/common/types.rs::tests`):**

- `test_contains_type_var` — detects TypeVar in composite types
- `test_substitute` — substitution through `List<T>` works
- `test_type_parsing` — `T` parses as `TypeVar("T")`

**Analyzer:**

- `records_instantiation_for_generic_call_with_int_argument`
- `records_instantiation_for_generic_call_with_reference_argument`
- `non_generic_calls_record_no_instantiation`

**Interpreter (end-to-end):**

- `tests/features.gol` in the CLI validation repo exercises
  `first<T>(xs: List<T>) -> Option<T>` with `T = Int`.

**Semantics-level:**

- Trait bounds on generics: `tests/semantics/trait_bounds_enforcement.rs`.

### Gaps

- **No test for a generic function with a `Result<T, E>` return
  type.** `substitute` recurses into `Result`, but the analyzer
  path from a generic return type through the plan to the builder
  is not directly tested.
- **No differential test specifically for generics.** Since
  generics resolve before IR, a differential test exercises the
  same concrete function on both backends — useful as a smoke
  test, but not specifically a generics test.
- **No test for a generic with a trait bound that calls the bound's
  method.** `max_of<T: Comparable>` calling `a.compare(b)` — the
  resolution of the bound method to the impl's function is not
  covered end to end.

## Maturity

Following the stages in `docs/architecture-direction.md`:

```
Generic (<T>)
    semantics:   Stable
    parsed:      yes
    typed:       yes
    validated:   yes
    IR:          via InstantiationPlan + builder
    verified:    N/A (verifier sees concrete IR)
    interpreter: supported (indirectly)
    LLVM:        supported (indirectly)
    WASM:        supported (indirectly)
    optimized:   no dead-specialization elimination
```

Like traits, generics have no backend asymmetry. The entire feature
lives in the frontend, the analyzer, and the instantiation plan.

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
4. `src/semantics/analyzer/` — type inference (`unify_types`) and
   instantiation recording.
5. `src/semantics/trait_registry/` — if the feature uses trait
   bounds, registration and validation there.
6. `src/ir/instantiation_plan.rs` — specialization plan and
   `close`.
7. `src/semantics/builder/build.rs` — specialization emission.
8. `src/semantics/builder/mod.rs::resolved_callee_name` — call-site
   rewriting.
9. `tests/semantics/` — analyzer-level tests for inference and
   bound checking.
10. `tests/corpus/` — end-to-end programs.
11. `docs/features/<feature>.md` — this file.

## Open questions

- **Should generic diagnostics use `E-XXX-NNN` codes?** Same gap as
  traits. Recommended for Tier 2.
- **Should there be dead-specialization elimination?** A generic
  function called only with `Int` that is then eliminated by DCE
  still produces an `f_Int` specialization. Removing it would
  require a reachability pass over the specialization graph.
  Currently not implemented.
- **Explicit call-site type arguments** (`identity<Int>(42)`) are
  not parsed. Whether to add them, and how they interact with
  argument-position inference, is an open design question.

## See also

- `docs/architecture-direction.md` — the feature contract pattern itself
- `docs/features/trait.md` — trait bounds on generics
- `docs/decisions/0003-type-system.md` — the type system
- `docs/decisions/0013-executable-ir-generic-invariant.md` — the
  instantiation plan and `close`
- `src/common/types.rs` — `TypeVar`, `Generic`, `substitute`
- `src/ir/instantiation_plan.rs` — the plan and closure
- `tests/semantics/trait_bounds_enforcement.rs`