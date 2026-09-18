# Feature: Range (`a..b`)

> Per-feature contract, following the pattern described in `docs/architecture-direction.md`.
> This file is the authoritative answer to "what is a range in ALGOL26, and where does it live?"

## Status: partial — parsed, IR exists, backends refuse

**This is the second feature contract in this directory whose status
is not "Stable."** Like `unsafe`, ranges are lexed and parsed, and
unlike `unsafe`, they also have an IR representation. But the
interpreter refuses them with `EvalError::Unsupported`, and I have
not seen any LLVM or WASM lowering.

The current state is "infrastructure exists, semantics do not."
Ranges parse, they can be constructed as values, and then any attempt
to actually run a program containing one fails at the backend.

This contract documents what exists, what the intended design likely
is (based on the IR shape), and the work needed to finish the feature.

## Summary

A **range** is a pair of bounds `start..end` naming a sequence of
integers. In most languages that have them, ranges are used for:

1. `for i in 0..n` — iterating over a numeric interval
2. `list[0..k]` — slicing a list or string
3. `0..n` as a first-class value passed to a function

In ALGOL26, the current IR representation
(`TypedIRValue::Range(Box<TypedIRValue>, Box<TypedIRValue>)`) is a
pair of two values with no attached type information, which suggests
the intended use is (1) and (3) — ranges as iteration specifications
and as passable values — rather than (2), which would require the
IR to know the slice's element type.

None of the three uses are currently supported end-to-end.

## Syntax

The lexer tokenizes `..` as a range operator (see
`frontend::lexer::tests::test_range_tokens`). The parser accepts
ranges in expression position (see
`frontend::parser::tests::test_parse_range`).

The exact surface syntax has two plausible forms:

```gol
val r := 0..10       // start..end
val r := 0...10      // start...end (inclusive?)
```

Which one the parser accepts is not confirmed by the tests I have
read. The lexer token is called `Range`; the exact source characters
it matches are not visible in the test names. **A five-minute check
of `frontend/lexer/operator.rs` would resolve this.**

Ranges are **expressions**, not statements. They can appear anywhere
an expression is valid. There is no statement form.

### Where ranges are not (currently) usable

Ranges are **not** currently usable in `for` loops:

```gol
for i in 0..10 do       // parse: yes; run: no
    print(i)
```

The `for` construct iterates over lists (`for x in list do`), not
over ranges. Whether the design intends `for i in range` to work is
not confirmed by tests.

## Typing rules

There is no `Type::Range` variant in `src/common/types.rs`. This is
the first signal that the feature is unfinished: without a type,
a range value cannot be assigned, passed to a function, or stored.

The IR variant `TypedIRValue::Range(start, end)` exists, but
`TypedIRValue::type_of()` returns `Type::Unknown` for it (this was
noted in the Round 3 review of `semantic_ir.rs` — the `_ =>` arm
catches `Range` along with `Array` and `FieldAccess`). So a range's
type is not representable in the current type system.

**Consequence:** a range can be constructed but not used. It cannot
be the right-hand side of a `val` binding (the analyzer needs a type
to bind), it cannot be an argument to a function (the verifier needs
a type to check against the parameter), and it cannot be returned
from a function.

The only place a range can appear without needing a type is inside
another construct that consumes it directly — and no such construct
currently exists.

### What this means in practice

```gol
procedure main
    val r := 0..10       // analyzer: probably type error
```

Any program that tries to *use* a range value is likely to fail at
the analyzer, before the interpreter's refusal even matters. The
refusal is only reachable for programs that construct a range in a
position the analyzer tolerates and then pass it to a backend — a
narrow window.

## IR representation

In `src/ir/semantic_ir.rs`:

```rust
// NEW: Range
Range(Box<TypedIRValue>, Box<TypedIRValue>),
```

The variant is a pair of `TypedIRValue` (start and end), with no
type annotation of its own. It carries no information about whether
the bounds are inclusive or exclusive, no information about step,
and no information about what the range will be used for.

The `// NEW:` comment suggests the variant was added recently and
has not yet propagated through the rest of the pipeline.

### IR verifier

The verifier's `verify_value` handles `Range` structurally — verifies
the two bounds and returns the claimed type. Since the claimed type
is `Type::Unknown` (see `type_of()`), the verifier does not enforce
anything beyond "both bounds are themselves well-typed values."

Whether the bounds must be `Int` is not enforced. A range like
`"a".."z"` (string bounds) would pass the verifier structurally
if both operands verify.

### CFG representation

The CFG builder has **no** handling for `TypedIRValue::Range` as a
value. It only has handling for `Instruction::IteratorInit`, which
expects a list or array. A range is not a list or array, so a range
passed to an iterator would fail at the CFG stage.

The observable effect: a program that gets past the analyzer with a
range value will produce a `CfgInstruction::Unsupported` at the CFG
builder, which the dataflow engine turns into
`E-UNSUPPORTED-001`.

## Backends

| Backend | Support | Evidence |
|---|---|---|
| Interpreter | **Unsupported** | `EvalError::Unsupported { construct: "ranges" }` in `src/backends/interpreter/eval.rs` |
| LLVM | **Unsupported** | No `Range` handling in `src/backends/llvm_codegen/` (as far as I have read) |
| WASM | **Unsupported** | No `Range` handling; capability check likely refuses |

### Interpreter

The interpreter explicitly refuses ranges:

```rust
TypedIRValue::Range(..) => {
    return Err(EvalError::Unsupported {
        construct: "ranges",
        hint: "ranges are not yet lowered by the interpreter",
    });
}
```

This is the fail-closed path added in the interpreter-totality work
this session. Before that, `Range` fell through to a wildcard arm
and returned `RuntimeValue::Void` silently.

### LLVM

I have not seen any `Range` handling in the LLVM codegen. If a range
value reaches LLVM lowering, the likely outcome is either a wildcard
fallback (silent zero) or a `CompileError`. Whether the capability
check refuses range-using programs before lowering is unverified.

### WASM

Same as LLVM. Unverified which path handles the refusal.

## Why ranges are unfinished

Ranges are the clearest example in the language of a feature whose
IR representation was added before its type-system representation.
The `// NEW:` comment and the `Type::Unknown` return from `type_of()`
both point to a design in progress.

The reason is straightforward: making ranges first-class requires
deciding what a range *is*, which is a design question the language
has not yet answered. Three plausible answers, each with different
consequences:

1. **Range is a first-class value.** A `Type::Range` variant, a
   `RuntimeValue::Range(i64, i64)`, and full language support. This
   is the most flexible, but requires deciding inclusivity, step,
   and how a range interacts with `List` (can a range index a list?).
2. **Range is sugar for `for`.** The parser rewrites
   `for i in 0..10` into `for i in list_from_range(0, 10)` at AST
   construction, and `Range` never reaches the IR. This is simpler
   but means ranges cannot be passed around.
3. **Range is a builtin type.** A `Range` value exists but only
   supports a limited set of operations (iteration, `length`, bounds
   access), similar to Python's `range` object.

Each path has different work items. None has been chosen yet.

## Diagnostics

Range-related error codes currently emitted:

| Code | Meaning | Emitted from |
|---|---|---|
| `E-UNSUPPORTED-001` | Unsupported IR operation (Range) | `dataflow.rs` via `CfgInstruction::Unsupported` |
| — | Interpreter does not support ranges | `EvalError::Unsupported` (interpreter) |

**No `E-RANGE-NNN` code exists.** This is the ninth feature in this
directory with a diagnostic gap (after traits, generics, defer,
spawn, FFI, alloc/free, unsafe, and String). But since ranges do not
currently work, the gap is moot until the feature is implemented.

## Safety

Ranges have no safety-relevant behavior. A range cannot be used
where an unsafe operation would be needed, and no runtime errors are
possible from a range because no range operation executes.

## Test coverage

Current coverage across the tree:

**Lexer:**

- `frontend::lexer::tests::test_range_tokens` — the `..` token is
  lexed as `Token::Range` (or similar)

**Parser:**

- `frontend::parser::tests::test_parse_range` — a range expression
  parses to some AST node

**That's it.** No analyzer test, no IR test, no backend test, no
corpus program, no conformance fixture, no differential test.

### Gaps

The gap list for ranges is the feature itself. Only the lexer and
parser have tests, because nothing beyond those stages accepts a
range.

Once the feature is designed, the test suite needs:

- The chosen syntax form (`..` vs `...`, inclusive vs exclusive end).
- A test that a range is a value of a specific type.
- A test for what operations are valid on a range.
- Backend tests for whichever backends support it.
- At least one corpus program using the feature end-to-end.

## Maturity

Following the stages in `docs/architecture-direction.md`:

```
Range (a..b)
    semantics:   Not implemented
    parsed:      yes (lexer + parser tests exist)
    typed:       no (no Type::Range)
    validated:   no
    IR:          partial (TypedIRValue::Range exists, no type)
    verified:    structural (bounds verified, type not)
    interpreter: refused (EvalError::Unsupported)
    LLVM:        refused or missing (unverified which)
    WASM:        refused or missing (unverified which)
    optimized:   N/A
```

Ranges are second-lowest-maturity feature in the language, slightly
ahead of `unsafe` (which has no IR representation at all).

## What this means for the language

Ranges are a **partially-built feature** that shipped into the tree
before its design was settled. The `// NEW:` comment is the
historical marker. Whoever added the IR variant had a plan for what
ranges would be, but the plan was not carried through to
completion, and the plan was not written down.

There are three honest paths forward:

1. **Design and implement ranges** per one of the three models in
   "Why ranges are unfinished" above. This is a Tier 1 feature
   design project, not a Tier 2 cleanup item.
2. **Remove the feature.** Delete `TypedIRValue::Range`, the parser
   handling, and the lexer token. If the language does not need
   ranges, having them in a half-state is worse than not having
   them at all.
3. **Leave as-is with an explicit note** in the language reference
   that ranges are reserved syntax and should not be used. This is
   the least honest of the three, but it is what is currently
   happening by default.

Option 1 is the design intent. Options 2 and 3 are the fallbacks if
the design work is not pursued.

## Checklist for related features

If you are implementing ranges (or a similar syntactic construct
with no type-system representation yet), the work items are:

1. **Decide the model.** First-class value? `for` sugar? Builtin
   type? Each has different consequences.
2. **Add `Type::Range` if model 1 or 3.** Update `Type::from_str`,
   `Display`, `can_coerce_to`, `can_cast_to`, `common_supertype`,
   `inner_type`, `contains_type_var`, `substitute`.
3. **Update `TypedIRValue::type_of` to handle `Range`.** Currently
   returns `Type::Unknown`.
4. **Add the analyzer rules.** What type does `a..b` have? What
   type must `a` and `b` be? What operations are valid on a range?
5. **Add backend lowerings.** Interpreter: a `RuntimeValue::Range`
   variant. LLVM: a struct `{ i64, i64 }` or two `i64` values.
   WASM: same.
6. **Update the IR verifier.** Enforce that bounds are `Int`.
7. **Integrate with `for`.** If model 2, `for i in 0..n` needs a
   parser or analyzer rewrite. If model 1 or 3, the iterator
   instruction needs to accept a range as an iterable.
8. **Capability matrix.** Declare which backends support ranges.
9. **Tests.** Lexer, parser, analyzer, IR, backends, differential.
10. **Update this contract.** The status should change from
    "partial" to "Stable" once the design is settled.

## Open questions

- **What is the surface syntax?** The lexer has a `Range` token, but
  I have not confirmed whether it is `..`, `...`, `to`, or something
  else. This is the first question to resolve.

- **Is the end inclusive or exclusive?** `0..10` — does it include
  10? This determines iteration counts and off-by-one behavior. The
  answer should be documented in `docs/language-reference.md` once
  decided.

- **Can a range be passed to a function?** Depends on the model.
  Model 1 and 3 say yes; model 2 says no.

- **Can a range be stored in a variable?** Same dependency.

- **Can a range index a list or string?**
  `list[0..5]` would be a slicing operation. Model 1 supports this
  naturally; models 2 and 3 require a separate slicing feature.

- **What does `for i in 0..n` do?** Iterates `i` from 0 to `n-1`
  (exclusive) or 0 to `n` (inclusive)?

- **What is the type of `a..b` if `a` and `b` are `Float`?**
  Currently no type, so the question is deferrable. A `Range<Float>`
  would require deciding step size, which introduces more design
  questions.

- **Should ranges have a `step`?** `0..10..2` is a common extension.
  Not currently supported.

- **Should there be an inclusive-range literal?** Rust has both
  `a..b` (exclusive) and `a..=b` (inclusive). ALGOL26 would need
  to decide whether to follow this pattern.

## See also

- `docs/architecture-direction.md` — the feature contract pattern itself
- `docs/features/unsafe.md` — the other low-maturity feature
- `docs/features/list.md` — iteration over lists currently uses
  `IteratorInit`, not ranges
- `src/ir/semantic_ir.rs` — `TypedIRValue::Range`
- `src/backends/interpreter/eval.rs` — the `EvalError::Unsupported` refusal
- `src/frontend/lexer/operator.rs` — the `Range` token (unverified path)
- `src/frontend/parser/expr.rs` — range parsing (unverified path)
- `frontend::lexer::tests::test_range_tokens`
- `frontend::parser::tests::test_parse_range`
