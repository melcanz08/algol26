# ADR 0048: Restore the procedure / function distinction

Status: Accepted

## Context

ALGOL26 currently has two function-declaration keywords, `proc` and
`function`, that overlap almost completely. Verified by probe:

| Declaration | Result |
|---|---|
| `proc foo` | legal - declares a function returning nothing |
| `proc foo() -> Int` | REJECTED - `proc` refuses `->` |
| `function foo` | legal - identical to `proc foo` |
| `function foo() -> Int` | legal |

So `proc foo` and `function foo` (no arrow) are interchangeable, and
`function` is a strict superset of `proc`. The language has two
keywords where one would do, and the difference between them is
purely which forms each refuses - not what role each plays.

This is a departure from ALGOL 58, where the two words meant
genuinely different things:

- **`procedure`** was the general subroutine form: a block of
  statements, assignable to the procedure name for one or more
  return values.
- **`function`** was the single-expression form: a named
  computation, closer to a mathematical mapping. `f(x) := x / 2`.

Two different concepts, two different keywords. The distinction
held through ALGOL 58, began eroding in ALGOL 60 (which allowed
statement bodies in `function`), and was gone by Pascal (which
kept the two words but made `function`/`procedure` differ only in
whether a return type was present). C collapsed further: every
function is a function, `void` handles the no-return case. The
two-keyword system survived, but the semantics behind the two
keywords stopped being distinct around 1960.

The rename question - `function` to something shorter - is the
opportunity to restore the ALGOL 58 split. Doing it while already
breaking the keyword saves a second breaking change later. Not
doing it leaves the language with a redundant keyword pair and no
coherent answer to "why are there two?"

## Decision

Restore the ALGOL 58 distinction, with three changes:

1. **`proc` becomes the general declaration keyword.** Block body,
   optional `-> T`, `return` for early exit. This is what
   ALGOL 58 called `procedure` and what modern languages call
   `function` (`fn` in Rust, `func` in Go, `def` in Python).

2. **`fn` becomes the single-expression math form.** Body is one
   indented expression. The expression is the return value. No
   `return` keyword, no statement body, no `-> Void`.

3. **`function` is removed.** It has no remaining role.

Both keywords are declarations; only their bodies differ. A `proc`
is a sequence of statements that may produce a value via
`return`; an `fn` is a value expression parameterized over its
arguments.

## Syntax

### `proc` - general declaration

```gol
proc main
    print("hello")

proc sum(xs: List<Int>) -> Int
    var total := 0
    for x in xs
        total := total + x
    return total

proc update(self: &mut Counter, by: Int)
    self.value := self.value + by
```

- Body is an indented block of statements.
- `-> T` is optional; absent means the proc returns nothing.
- `return E` produces the value; `return` alone exits a void proc.
- Multiple statements, control flow, mutation - all allowed.

### `fn` - single-expression form

```gol
fn square(x: Int) -> Int
    x * x

fn double(x: Float) -> Float
    x * 2.0

fn max_of(a: Int, b: Int) -> Int
    if a > b then a else b

fn identity<T>(x: T) -> T
    x
```

- Body is **one indented expression**.
- The expression's value is the return value.
- No `return` keyword. No statement body. No trailing semicolon or
  block.
- `-> T` is **required**: the codomain is part of the declaration,
  matching the mathematical reading (`f: R -> R`).
- The expression may contain control flow that is itself an
  expression - `if ... then ... else ...`, `match`, arithmetic,
  function calls.

### Call syntax is unchanged

`proc` and `fn` are called identically. No difference at call
sites:

```gol
proc main
    print(square(7))       // fn call
    print(sum([1, 2, 3]))  // proc call
```

## Semantics

### `fn`

- **Pure by construction of the body, not by guarantee.** An `fn`
  body is an expression. There is no statement position inside it,
  so there is nowhere for a side effect to live. But a function
  call is itself an expression, so an `fn` body can call another
  function that has side effects. The split is about *body shape*,
  not purity enforcement. Purity, if the language ever wants it,
  is a separate feature.
- **Type-checked.** The single expression must produce a value
  coercible to `-> T`. A mismatch is a compile error, same as a
  `return E` mismatch in a `proc`.
- **Generic.** `fn` supports type parameters, trait bounds, and
  `where` clauses the same way `proc` does.

### `proc`

- **Statement body.** Zero or more statements, terminated by an
  optional `return`. If a return type is declared and control
  reaches the end of the body without a `return`, the analyzer
  rejects the program (the existing "may not return a value on
  all paths" check).
- **Optional return type.** `proc foo` and `proc foo()` declare a
  void proc. `proc foo() -> T` declares one that returns `T`.
- **Mutability, `&mut`, regions.** All existing mechanisms work
  inside `proc` bodies.

### Methods

Both forms work inside `impl` blocks, but with a caveat:

```gol
impl Counter
    proc increment(self: &mut Counter)
        self.value := self.value + 1

    fn doubled(self: &Counter) -> Int
        self.value * 2
```

An `fn` method with a `&mut self` is syntactically legal but
unusual - a mutating method is nearly always a `proc`. The
analyzer does not forbid it; convention is that `proc` handles
mutation and `fn` handles pure computation.

## What this means for `main`

`main` is an entry point, not a value. It almost always has
statements (setup, output, control flow). The conventional
declaration is:

```gol
proc main
    print("hello")
```

`fn main() -> Int` is technically legal (an entry point that is a
single expression producing an `Int`), but is not the idiomatic
form. The runtime still finds `main` by name; the keyword used to
declare it is the programmer's choice, following the body-shape
rule.

## Migration plan

Four phases, each independently reviewable.

1. **Parser and lexer.** Add `fn` as a token. Keep `function` and
   `proc` for now. Implement the `fn`-body-must-be-single-
   expression rule. Update `parser/items.rs` to accept `fn` in
   the same positions `function` currently is.

2. **Analyzer.** Add the check: an `fn` body must be a single
   expression yielding the declared type. Reject `return` inside
   `fn`. Reject statement sequences inside `fn`.

3. **Fixture migration.** Walk every `.gol` file:

   - Declarations currently written `function foo(args) -> T` with
     a single-expression body (`return E`) become `fn foo(args)
     -> T` followed by the indented `E`.
   - Declarations currently written `function foo(args) -> T`
     with a multi-statement body become `proc foo(args) -> T`.
   - Declarations currently written `function foo` with no return
     type become `proc foo`.
   - Declarations currently written `proc foo` are unchanged.

   This is a scripted transformation followed by manual review of
   anything the script cannot classify confidently.

4. **Remove `function`.** Delete `Token::Function` from the lexer,
   remove the `function` keyword, update the parser's declaration
   checks. This is the red-then-green commit - the parser change
   makes every remaining `function` a syntax error; the fixture
   migration already replaced them, so the tree goes green.

After phase 4, the keyword set is `proc`, `fn`, `rec`, `impl`,
`val`, `var`, `trait`, `enum`, `type`, `region`, `unsafe`,
`spawn`, `parallel`, `channel`, `send`, `receive`, plus control
keywords (`if`, `for`, `while`, `match`, `return`, `defer`).

## Consequences

**Positive.**

- The two keywords have distinct meanings again. `proc` = does
  things; `fn` = computes a value. The question "which one do
  I use" has an answer based on body shape, not a lexical
  accident.
- `fn` is two characters, matching `proc` in brevity. The
  keyword set is shorter and more uniform.
- The distinction is user-visible and self-documenting. A reader
  scanning a file sees `fn` and knows it is a small computation;
  sees `proc` and knows it is a sequence of steps.
- Restores a piece of ALGOL 58 that the intervening decades lost.
  The vision doc's claim to ALGOL heritage gets a concrete,
  non-cosmetic manifestation.

**Negative.**

- A breaking change across every `.gol` file in the tree.
- Two keywords to explain where a newcomer might expect one.
  "Why are there two?" now has a real answer, but the answer
  takes a sentence.
- `fn` bodies cannot contain `print`, mutation, or any other
  statement - which means "I want to print something AND return
  a value" requires `proc`, even for short functions. This is by
  design (that is what `proc` is for) but is a real ergonomic
  constraint.

**Neutral.**

- No backend changes. Both `proc` and `fn` lower to the same
  `SemanticFunction` in the IR. The distinction is front-end
  only.
- No verifier changes. The IR does not know which keyword
  declared a function.

## Alternatives considered

### A. Rename `function` to `func`, leave `proc` alone

The minimal rename. Keeps both keywords, keeps the overlap.
Downside: the language still has two keywords for one concept,
and the "why two?" question has no good answer.

**Rejected** because it does not fix the underlying redundancy.

### B. Collapse both into a single keyword

One declaration keyword, optional `-> T`, block body. This is
what Rust does (`fn`), what Go does (`func`), what Python does
(`def`). Simple, and no "which one?" question.

**Rejected** because it discards the ALGOL 58 distinction the
language was named for. It is a defensible choice for a different
language, but for a language whose vision doc explicitly invokes
ALGOL heritage, restoring the split is more honest.

### C. Keep `function`/`proc` as-is, add `fn` as a third form

Three keywords, one for each of general, void, and math.
Confusing. No historical basis.

**Rejected.**

### D. Make the return type optional on `fn`

`fn square(x: Int)` followed by `x * x`, return type inferred
from the body.

**Deferred.** Inference would require the analyzer to derive the
body type before establishing the signature, which complicates
the type-checking order. Required `-> T` is simpler and matches
the mathematical reading (a function is a mapping into a
codomain, which is part of its identity). If inference becomes
desirable later, it can be added without breaking the current
form.

## Open questions

- **Should `fn` bodies allow `defer`?** A defer is a statement,
  and `fn` bodies contain none. A `fn` body could be a deferred
  expression - but that is exotic, and the answer is probably no.
- **Should `fn` reject `&mut` parameters?** A `&mut Int` in a
  function whose body is a pure expression is unusual but not
  impossible (`fn inc(r: &mut Int) -> Int` + `*r + 1`). The
  language could forbid it as a purity signal, or allow it as a
  syntactic convenience. This ADR allows it; a future ADR could
  tighten.
- **Recursion in `fn`.** `fn factorial(n: Int) -> Int` +
  `if n <= 1 then 1 else n * factorial(n - 1)`. Legal. Recursion
  is an expression because `if` is an expression and the call is
  an expression.
- **Iteration in `fn`.** `for` is a statement, not an expression.
  An `fn` cannot contain a `for` loop. Programs that want a
  loop-and-return use `proc`. This is by design.

## See also

- `docs/vision.md` - the ALGOL heritage claim this ADR gives a
  concrete expression to
- `docs/decisions/0001-significant-indentation.md` - the
  indentation-based body that `fn` inherits
- ALGOL 58 Preliminary Report (1958) - the original
  `procedure`/`function` distinction
- `src/frontend/lexer/mod.rs` - where the keyword table lives
- `src/frontend/parser/items.rs` - function declaration parsing
- `src/semantics/analyzer/expr.rs:1654` - the visibility check
  that currently names the keyword as "function" in a
  diagnostic
