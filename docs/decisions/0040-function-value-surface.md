# ADR 0040 — Function-Value Surface: Defaults, Function Types, Closures

## Status

Proposed. First-pass decision on which first-class-function features
are in scope for the language's near-term roadmap.

> **Status note (2026-10-07).** This ADR was prompted by a direct
> inspection of the parser, AST, and language reference, which
> confirmed three absences and two incidental inconsistencies.
> It freezes the current state so future contributors know these
> are deliberate deferrals rather than oversights, and it fixes
> two small defects found during the inspection.

## Context

Three function-related features are absent from Algol26, confirmed
by reading the parser (`src/frontend/parser/items.rs`), the AST
(`src/frontend/ast.rs`), the type system (`src/common/types.rs`),
and the language reference (`docs/language-reference.md`):

**Default parameter values.** `parse_function` reads a parameter
name and its type annotation, then checks for a comma. There is
no grammar for `name: Type = expr`. The lexer additionally has no
bare `=` token — the diagnostic for a bare `=` at statement
position reads 'use `:=` for assignment or `==` for equality'.
Default values would need both a lexer change and an AST shape
change (`FunctionDecl.params` would gain a third element), which
ripples through every consumer of `params`.

**Function types in source.** `Type::Function { params, return_type }`
exists in `src/common/types.rs`. It is matched by
`type_mentions_reference`, `contains_type_var`, `substitute`, and
`map_type` (where it is grouped with pointer-like types). It has
unit tests. But `TypeSyntax` (`src/frontend/ast.rs`) has only
`Named`, `Generic`, and `Unknown` variants. `parse_type_syntax`
(`src/frontend/parser/types.rs`) produces only those. A user
cannot write a function-typed parameter, return, variable, or
field. `Type::Function` is a dead internal representation —
reachable in match arms, unreachable from source.

**Closures and lambdas.** The AST has no `Lambda` or `Closure`
`ExprKind` variant. The lexer has no pipe token. The parser has
no rule for anonymous-function syntax. Nested function
declarations exist and are exercised by
`test_stress_nested_functions`, but nested functions are not
closures — they are separate `SemanticFunction` entries in the
IR, with no capture mechanism. A test named
`test_stress_10_level_closure_capture` in
`tests/integration/release_hardening.rs` generates ten *separate
top-level* functions `f0..f9`, each calling the previous. No
capture occurs; the name is a misnomer.

Two incidental inconsistencies were also found:

- `parse_function`'s error message reads `Expected 'function' or
  'Proc'` — capital `P`. The keyword is lowercase `proc`. A user
  who mistypes the declaration form is told to try a spelling
  that does not exist.
- `Type::Function` and `Type::Tuple` are orphan variants: present
  in every type-system match, never constructible from source.
  Neither has a comment explaining whether it is reserved for a
  planned feature or a leftover from an earlier design.

## Decision

### Default parameter values: not in v1

Algol26 will not support default values on function or method
parameters in v1. The language's style favors explicit call sites;
a default is implicit behavior at the call graph, and the
ergonomic gain does not justify the surface cost.

The related pattern — a function that wants different call
shapes — is expressed today via overload-like naming (`parse`,
`parse_with_defaults`) or via an `Option<T>` parameter with `None`
meaning 'use the fallback'. Both are explicit at the call site.

If a real use case appears, defaults can be its own ADR with its
own design pass. That ADR would need to decide:

- Evaluation time of the default expression (declaration time vs.
  call time)
- Whether defaults can reference earlier parameters
- Whether a default can be added to a trait method declaration
- Whether the caller can distinguish 'passed the default' from
  'passed nothing'

Each is a distinct design question. Bundling them into this ADR
would balloon it without a motivating program.

### Function types in source: not in v1

Algol26 will not expose a surface syntax for function types in v1.
`Type::Function` is retained as a reserved internal representation
and gains a doc comment stating so. The variant is not deleted
because removing it from `common/types.rs` would ripple through
`substitute`, `contains_type_var`, and the type-system tests for
no current benefit, and because a future feature (an FFI callback
parameter, a trait method taking a function value) may want it.

The reason for the deferral is not syntax. Adding a `TypeSyntax::Function`
variant, a parser branch, and a `to_type` arm is a half-day of
work. The reason is semantics: a first-class function value needs
a runtime representation. Three candidate designs, each with
different consequences:

1. **Raw code pointer.** `ptr` to the function's entry. No capture.
   Cheapest representation; sufficient for FFI callbacks; not
   sufficient for anything that captures variables.
2. **Fat pointer with closure environment.** `{ code_ptr, env_ptr }`.
   Standard for languages with closures. Requires a heap-allocated
   environment or a region-allocated one, plus a calling convention
   that passes the environment as a hidden argument.
3. **Interface / trait object.** `dyn Fn(T) -> U` — same shape as
   ADR 0038's dynamic dispatch. Requires vtables. Not in scope
   until 0038 lands.

Choosing between these requires knowing what the function value is
for. Without a program that needs it, no choice is better than the
others. The ADR that introduces function values must name a
motivating use case and pick the representation that serves it.

### Closures and lambdas: not in v1

Algol26 will not support anonymous function expressions in v1.
This follows from the function-type decision: a lambda is a value
of function type, and without a runtime representation for
function values, a lambda has nowhere to live.

Nested function declarations (a `function` inside another
`function`'s body) are a separate feature and are partially
supported today. Whether they support capture of the enclosing
scope is not decided here. If nested functions grow capture, they
become closures in everything but syntax, and the reasoning above
applies.

The concrete gap to close is smaller than it appears. Most of the
programs a closure would serve are served today by a small helper
function declared at module scope, with the 'captured' values
passed explicitly. That is more verbose but compiles on every
backend.
### Incidental fixes: land alongside this ADR

**1. Parser error message.** `parse_function`'s error for an
unrecognized declaration head reads `Expected 'function' or 'Proc'`.
Change to `Expected 'function' or 'proc'` to match the actual
keyword. One-line fix; no test depends on the exact string.

**2. Orphan type documentation.** `Type::Function` and
`Type::Tuple` in `src/common/types.rs` gain doc comments marking
them as reserved-internal, not exposed in `TypeSyntax`. The
comments should name the ADR that would expose them if it lands
(for `Function`, this ADR's successor; for `Tuple`, whichever ADR
introduces multiple return values). No code change.

**3. Test rename.** `test_stress_10_level_closure_capture` in
`tests/integration/release_hardening.rs` becomes
`test_stress_10_level_call_chain`. The body is unchanged; the
test measures compile latency on a chain of top-level function
calls, which the new name describes accurately.

## Rationale

The three features share a common shape: each is a small parser
or type-system addition whose weight is in the runtime model
underneath. Defaults need an evaluation model. Function types
need a representation model. Closures need both. Adding any of
them without a motivating program is design-by-anticipation.

The language's current feature set composes. Adding one of these
three would introduce a second 'shape' of function alongside the
declaration-only model — creating a class of values (function
values) whose ownership, borrowing, and copy semantics are not
yet specified. That is not a reason to never add them; it is a
reason to add them one at a time, with a named use case, and to
extend the ownership model deliberately rather than incidentally.

## Alternatives considered

### Default parameters with a special syntax (`name: Type ?= expr`)

**Rejected.** A non-standard operator (`?=`) introduces lexer
surface for one feature. The language's keyword set is small and
each token is earned. If defaults land, they should use the
language's existing equality vocabulary.

### Default parameters with the bare `=` operator

**Rejected.** `=` is not a token in Algol26. The lexer's explicit
diagnostic ('use `:=` for assignment or `==` for equality') exists
to keep the language's assignment syntax unambiguous. Adding `=`
back for this one case would weaken that.

### Function type syntax `(T) -> U`

**Deferred, not rejected.** The syntax matches the return-type
arrow the language already uses and is the natural notation if
function types land. The deferral is about representation, not
notation.

### Function type syntax `fn(T) -> U`

**Rejected.** `fn` is not a keyword in Algol26; `function` is.
Adding `fn` would create an abbreviation used only in this one
position. The language has consistently used full words
(`function`) or short declarations (`proc`, `rec`).

### Lambdas with pipe syntax `|x| expr`

**Deferred.** Pipe tokens do not exist in the lexer and are
ambiguous with boolean `or` in some contexts. If lambdas land,
the syntax choice should be reconsidered in light of the function
type syntax chosen.

### Adding defaults and lambdas but not function types

**Rejected.** Defaults without function types is coherent (the
two features are unrelated). Lambdas without function types is
not — a lambda's type is a function type, and there would be no
way to name it. If lambdas are ever needed, function types come
first.

## Consequences

### Positive

- The three absences are documented as decisions, not oversights.
  A future contributor who notices 'no closures' finds this ADR
  and the reasoning.
- The parser's error message stops sending users to a nonexistent
  keyword.
- The orphan-type comments prevent the next reader of
  `common/types.rs` from assuming `Type::Function` is reachable.
- The renamed test reflects what it actually does.

### Negative

- Programs that would be cleaner with a small default or a lambda
  will be slightly more verbose. This is by design.
- The suite of traits, generics, and impls is deliberately not
  extended toward functional-programming idioms. A user coming
  from a language where closures are idiomatic will find the
  closest expression awkward.
- The two orphan types remain in the codebase. A reader who greps
  for `Type::Function` will still find uses that look live; the
  doc comment mitigates but does not eliminate this.

### Neutral

- No code generation or IR changes. Nothing in the runtime or
  the backends is affected.
- The capability matrix is unaffected. No backend refuses these
  features because they are not parseable in the first place.

## What this ADR does not decide

- **The representation of function values, if they ever land.**
  This ADR names the three candidates (raw pointer, fat pointer
  with environment, trait object) but picks none.
- **Whether nested functions capture.** Nested function
  declarations exist; whether they can reference variables from
  enclosing scopes is a separate question that this ADR does not
  resolve.
- **The interaction of first-class functions with the borrow
  checker.** A function value that captures an `&mut` reference
  and outlives the borrow's scope is a real problem. That belongs
  in the ADR that introduces function values.

## References

- `src/frontend/parser/items.rs` — `parse_function`,
  the params loop, the error message that gets fixed.
- `src/frontend/parser/types.rs` — `parse_type_syntax`,
  the function that lacks a `Function` branch.
- `src/frontend/ast.rs` — `TypeSyntax` enum with no `Function`
  variant.
- `src/common/types.rs` — `Type::Function`, `Type::Tuple`,
  the two orphan variants.
- `docs/language-reference.md` — the reference that will gain
  a 'Not yet supported' section.
- `tests/integration/release_hardening.rs` —
  `test_stress_10_level_closure_capture`, the test to rename.
- ADR 0038 (Dynamic Dispatch) — the trait-object representation
  of function values depends on the vtables 0038 introduces.
- ADR 0025 (Trait Bounds Status) — the where-clause satisfaction
  gap, orthogonal but adjacent to the function-surface discussion.

## See also

- `docs/features/function.md` — the function contract; gains a
  scope note pointing at this ADR.
- `docs/language-reference.md` — gains a 'Not yet supported'
  section listing the three deferred features.
- `docs/decisions/README.md` — the ADR index.