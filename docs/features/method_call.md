# Feature: Method Call (`x.method(args)`, `x.method`)

> Per-feature contract, following the pattern described in `docs/architecture-direction.md`.
> This file is the authoritative answer to "what is a method call in ALGOL26, and where does it live?"

## Summary

A **method call** is syntactic sugar for a function call with the
receiver as the first argument. `s.length()` desugars to
`length(s)`; `nums.sum()` desugars to `sum(nums)`;
`x.compare(y)` desugars to `compare(x, y)`.

This is a **pure desugaring** feature. There is no `MethodCall`
instruction in the IR, no `MethodCall` value in `TypedIRValue`, and
no backend knows method calls exist. By the time the IR is
constructed, every method call is an ordinary `Call`.

Two surface forms are equivalent:

```gol
nums.length       // bare — no parentheses
nums.length()     // parenthesized
```

Both lower to the same `Call(List.length, [nums])`. The bare form is
available for zero-argument methods; the parenthesized form works
for any arity.

This is the shortest and simplest feature contract in this directory.
It exists primarily to answer "where does method call syntax get
resolved?" — the answer is the IR builder, and nowhere else.

## Syntax

Bare (zero arguments):

```gol
val n := s.length
val t := nums.sum
```

Parenthesized (any arity):

```gol
val n := s.length()
val t := nums.sum()
val c := x.compare(y)
val u := s.substring(0, 5)
```

Chained:

```gol
val upper_length := "hello".to_upper().length
```

The bare form is not available when the method takes arguments. The
parser distinguishes them by the presence or absence of `(` following
the identifier.

## Typing rules

Method call syntax does not have its own typing rules. The desugared
call is type-checked as a normal function call:

1. Resolve the function name (the method name).
2. Check the argument count: the receiver plus any explicit arguments.
3. Check each argument's type against the function's parameters.
4. The call's type is the function's return type.

This means method call syntax is available exactly when a function
with the method's name exists and its first parameter accepts the
receiver's type. If a user writes `nums.sum()` and there is a
`sum` function taking a `List<Int>` or `List<Float>`, it works; if
not, the analyzer rejects the call with a normal "unknown function"
or "argument type mismatch" error.

### No method namespace

There is no separate method lookup path. `x.foo(y)` and `foo(x, y)`
resolve to the same function. This means:

- User-defined functions can be called with method syntax: if
  `function greet(name: String) -> String` exists, then
  `"world".greet` is a valid expression.
- Trait methods use the same mechanism. `a.compare(b)` becomes
  `compare(a, b)`, and trait resolution finds the right concrete
  implementation through monomorphization.

### Interaction with traits

When the receiver's type is a generic parameter `T: Trait`, the
method name is resolved through the trait registry. The desugaring
produces `Call(method_name, [receiver, ...args])`, and the
monomorphizer later specializes the call for the concrete `T` at
each instantiation.

See `docs/features/trait.md` and `docs/features/generic.md` for the
resolution rules at the analyzer and monomorphizer levels.

## Where desugaring happens

The desugaring is performed by the **IR builder**, not the parser
and not the analyzer. The parser produces an AST node for the method
call; the analyzer type-checks it as a call; the IR builder
flattens it into an ordinary `TypedIRValue::Call`.

### Parser

The parser has a rule for method call syntax in expression position.
It produces an AST node that carries the receiver, the method name,
and the argument list. The exact AST variant name is unverified —
the parser tests (`frontend::parser::tests::test_parse_method_call`,
`test_method_call_tokens`) confirm that method call syntax parses,
but do not name the AST node.

### Analyzer

The analyzer treats the method call as a function call for type
checking. It resolves the method name to a function, checks argument
types, and records the return type. The desugaring is not yet
visible — the analyzer sees the receiver as the first argument in
its own internal representation.

### IR builder

The IR builder emits a `TypedIRValue::Call` with the method name as
`function` and `[receiver, ...args]` as `args`.

The critical test is
`tests/ir/borrow_deref_addrof_test.rs::test_method_call_desugars_to_function_call`,
which asserts that a method call produces an `Instruction::Call`
with `func == "List.length"` for a `.length` call on a list.

## IR representation

**None specific to method calls.** After desugaring, the IR contains
only `TypedIRValue::Call` and `Instruction::Call`. There is no
method-call variant.

This is the same discipline as traits, generics, and defer: the
feature is resolved before the IR representation matters, so every
backend supports it for free.

## Backends

| Backend | Support | Evidence |
|---|---|---|
| Interpreter | Supported | Sees only the desugared `Call` |
| LLVM | Supported | Same |
| WASM | Supported | Same |

No backend has method-call-specific code, because no backend sees
method calls. This is the entire point of the desugaring: it keeps
the surface syntax out of the backend pipeline.

## Diagnostics

Method-call-related error codes currently emitted:

**None specific to method calls.** When a method call is invalid —
unknown method name, wrong argument types, receiver of the wrong
type — the error is reported through the normal function-call error
path. There is no "method 'foo' not found on type 'Bar'" message
distinct from "function 'foo' not found".

**This is a diagnostics-quality gap.** A user who writes
`nums.nonexistent()` gets an error saying "unknown function
`nonexistent`" — technically correct, but it does not mention that
the receiver was a list, or what methods a list supports. A
dedicated method-lookup error would be more helpful.

This is the same category as the diagnostic gap in traits, generics,
defer, spawn, FFI, alloc/free, unsafe, String, and range. Ten
features now lack dedicated diagnostic codes.

## Safety

Method call syntax has no safety-relevant behavior. It desugars to a
function call, which the ownership analyzer treats exactly like any
other call. No new safety properties are introduced.

The one subtlety: when a method call moves its receiver, the move is
reported through the normal move-tracking path. There is no
"method-call receiver was moved" error distinct from the general
move error.

## Test coverage

Current coverage across the tree:

**Lexer (`frontend::lexer::tests`):**

- `test_method_call_tokens` — the tokens `.` and identifier are
  lexed correctly in method call position

**Parser (`frontend::parser::tests`):**

- `test_parse_method_call` — method call syntax parses

**IR (`tests/ir/borrow_deref_addrof_test.rs`):**

- `test_method_call_desugars_to_function_call` — verifies the
  desugaring produces `Call(List.length, [receiver])`

**Differential (`tests/differential/differential_true.rs`):**

- `test_differential_method_syntax_no_parens` — `s.length` works
- `test_differential_method_syntax_parens_matches_bare` — `s.length`
  and `s.length()` produce identical output

**Conformance:**

- `tests/conformance/valid/method_syntax_bare.gol`
- `tests/conformance/valid/method_syntax_parens.gol`

### Gaps

- **No test for chained method calls** (`.to_upper().length`).
  Whether the desugaring composes correctly is not verified.
- **No test for a user-defined function called with method syntax.**
  `x.my_function(y)` where `my_function` is user-declared — should
  desugar to `my_function(x, y)`. Not tested.
- **No test for a method call on a trait-bounded generic.**
  `a.compare(b)` inside `function f<T: Comparable>(a: T, b: T)` —
  the resolution through the trait registry plus monomorphization
  is not covered end-to-end.
- **No test for a method call that moves its receiver.** The move
  semantics of `consume(s)` when written `s.consume()` — no test
  confirms the receiver is moved.
- **No test for an unknown method.** The error path exists via the
  general call error, but no test pins the message.

## Maturity

Following the stages in `docs/architecture-direction.md`:

```
Method call (x.method, x.method())
    semantics:   Stable
    parsed:      yes
    typed:       yes (as a call)
    validated:   yes
    IR:          N/A (desugared before IR construction)
    verified:    yes (as a normal call)
    interpreter: supported (as a normal call)
    LLVM:        supported (as a normal call)
    WASM:        supported (as a normal call)
    optimized:   N/A
```

Method call is a thin syntax layer with no runtime or backend
implications. Its entire implementation is a rewrite from one AST
shape to another during IR construction.

## Checklist for related features

If you are adding a feature *like* method-call desugaring (surface
syntax rewritten to existing semantics during IR construction), you
need to touch:

1. `src/frontend/lexer/` — any new tokens (typically none, if the
   syntax reuses existing operators like `.`).
2. `src/frontend/parser/` — new parsing rule for the surface form.
3. `src/frontend/ast.rs` — a new AST node (the desugaring source).
4. `src/semantics/analyzer/` — type checking in terms of the
   underlying construct.
5. `src/ir/semantic_ir.rs` — likely nothing, if the desugaring
   produces existing instruction/value types.
6. The IR builder pass — where the desugaring happens.
7. `tests/ir/` — a test asserting the desugaring produces the
   expected IR.
8. `tests/differential/` — a test asserting both surface forms
   (if there are two) produce identical output.
9. `docs/features/<feature>.md` — this file.

## Open questions

- **Should bare method syntax work for functions that take
  arguments?** Currently no — `x.foo` (no parens) is only valid
  when `foo` takes zero arguments. `x.foo(y)` needs the parens.
  This is a reasonable convention but worth documenting.

- **Should there be a distinction between field access and method
  call?** Currently no fields exist in the language (see
  `TypedIRValue::FieldAccess`, which is also partial), so `x.foo`
  always means a method call. If structs are added later, the
  disambiguation rule matters.

- **Should method calls be reorderable?** `foo(x, y)` and `x.foo(y)`
  produce the same call. But `x.foo(y)` reads more naturally for
  the receiver-first pattern. Whether the language should prefer
  one is a style question, not a semantics question.

- **Should there be a dedicated "method not found" error?** Yes —
  see the Diagnostics section. A user who writes
  `"abc".concat(1, 2)` gets a generic call error, not a message
  naming `concat` as an invalid method for `String`. Tier 2 item.

- **Should method call syntax be available for operators?** E.g.
  `x.+(y)`. Currently no — operators are separate. Whether to
  unify them is a design question.

- **How does method call syntax interact with the `.` in a dotted
  module path?** `module.function()` — is this a method call on
  `module`, or a qualified lookup of `function` in `module`? I have
  not seen the parser rule, but the two interpretations conflict.
  Worth resolving if the language adds modules.

## See also

- `docs/architecture-direction.md` — the feature contract pattern itself
- `docs/features/trait.md` — trait methods use the same desugaring
- `docs/features/generic.md` — monomorphization resolves the concrete
  method target
- `docs/features/list.md` — `List.length`, `List.sum`, `List.max`,
  `List.min` are frequently called as methods
- `docs/features/string.md` — `String.length`, `String.substring`,
  `String.to_upper`, `String.to_lower` are frequently called as
  methods
- `src/frontend/parser/expr.rs` — method call parsing (unverified path)
- `src/frontend/ast.rs` — the AST node for method calls
- `tests/ir/borrow_deref_addrof_test.rs::test_method_call_desugars_to_function_call`
- `tests/differential/differential_true.rs::test_differential_method_syntax_parens_matches_bare`
