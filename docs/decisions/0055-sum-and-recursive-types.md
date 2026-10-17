# ADR 0055: Sum Types and Recursive Types

Status: Proposed. Sessions 1 and 2 implemented (recursive records;
union declarations and construction). Session 3 (match over unions)
planned but not yet implemented — see "Session 3 plan" below.

## Implementation plan

Three sessions.

**Session 1 — recursive records.** ✅ Splits record registration
into two passes: names first (so self-reference and forward-
reference resolve), then field types. Direct self-reference
(a field whose type is exactly the record, with no indirection)
is rejected as infinitely sized. `tests/adr_examples/0055_recursion/`
holds three fixtures: `pass_rec_node.gol` (self-reference through
`Option`), `pass_forward_reference.gol` (forward reference between
records), and `fail_direct_self_reference.gol` (rejected with
`E0002`).

**Session 2 — union declarations and construction.** ✅ Implemented
across seven commits (Steps 1–7). Adds the `union` keyword (Form A
from the syntax-form decision below), the `UnionDecl` /
`UnionVariant` AST nodes, the `Type::Union` variant with its
`UnionTypeId`, two-pass analyzer registration (names first, then
variant payloads), `Json.Num(42)` / `Json.Null` construction
recognized by the analyzer and lowered by the builder, and
`RuntimeValue::Union { tag, union_name, variant_name, payload }`
in the interpreter. LLVM and WASM refuse union programs at the
capability boundary via `Feature::Unions`, producing a clean
`E0002` with the `--interpreter` hint.

Two known gaps, deferred:
- `resolve_syntax_with_records` does not consult the union table,
  so a union name in a function signature fails at registration
  time. No test triggers this yet.
- `process_imports` carries the top-level file's unions through
  without merging imported unions. No cross-file union test
  exercises this yet.

Smoke test:
    union Json
        Null
        Num(Int)
    proc main
        val a := Json.Null
        val b := Json.Num(42)
        print("both constructed")
prints `both constructed` through the interpreter, and is refused
by LLVM with `E0002: The LLVM backend does not support: union
types (declarations, construction)`.

**Session 3 — match over unions.** ⏸️ Planned but not yet
implemented. See "Session 3 plan" below for the design
decisions and step breakdown.

**Deferred: LLVM / WASM codegen for union payloads.** The ADR's
own "What is out of scope" section defers this to a follow-up.
Interpreter-only until the codegen project lands.

### Session 3 plan

Session 3 is the destructuring half of sum types: `match j` with
`case Num(n) then ...` extracts `n` from `Json.Num(42)`. It is
additive on top of Sessions 1 and 2 — the `Type::Union` variant,
the `RuntimeValue::Union { tag, .. }` runtime value, and the
capability matrix's `Feature::Unions` all already exist.

Four design decisions, frozen here:

**1. Binding semantics — by value.**

`case Num(n) then ...` binds `n` to a copy of the payload if the
payload type is `Copy` (Int, Float, Bool), and to a **moved** value
if it is not (String, record, list). The matched union becomes
partially unusable after that arm — the payload was consumed.
This matches how `Option<NonCopyType>` matching works today:
`case Some(v)` gives `v : NonCopyType` by move.

Rust shipped with pattern-by-value and added reference patterns
later. ALGOL26 v1 does the same: `case &Num(n)` (reference
patterns) is deferred.

**2. Payload arity — mandatory.**

`case Pt(x, y)` requires the `Pt` variant to declare exactly
two payload types. Arity mismatch is `E0002`, same diagnostic as
construction-time arity mismatch (`Json.Num()` for a one-payload
variant).

**3. Exhaustiveness — required, `case _` is the fallback.**

Every variant of the union must appear as a `case Name` arm, or a
wildcard `case _` must be present. Same rule as enum
exhaustiveness (`check_match_exhaustiveness`'s `Type::Enum`
arm), reusing the same `covered_variants` set. The diagnostic
names the first missing variant, matching the enum behavior.

**4. Guards — deferred.**

`case Num(n) if n > 0 then ...` is not part of Session 3. The
existing `Pattern::Guarded` mechanism for `Option`/`Result` can
be extended later; Session 3 is `case Variant(names) then ...`
with no `if`.

### Session 3 implementation steps

Seven steps. Each lands as its own commit.

1. **AST + parser — `Pattern::VariantWithBinding`.**
   Add `Pattern::VariantWithBinding { variant: String,
   payload: Vec<String> }`. Parse `case Name(n1, n2, ...)` inside
   a `match` body. The parser context (inside `match`) distinguishes
   `Num(n)` (a variant binding) from `Num(n)` (a function call).

2. **Analyzer — payload binding.**
   Extend `check_pattern_type` and `bind_pattern_variables` for
   `VariantWithBinding`. Look up the variant in
   `Type::Union::variants`, check arity, bind each name to its
   declared payload type.

3. **Analyzer — exhaustiveness over unions.**
   Extend `check_match_exhaustiveness`'s `Type::Union` arm to
   require every variant be covered, mirroring `Type::Enum`.

4. **Builder — `SemanticPattern::VariantWithPayload`.**
   Add the IR pattern. In `translate_expr`'s `Match` arm, when
   the AST pattern is `VariantWithBinding`, emit
   `SemanticPattern::VariantWithPayload { tag, variant_name,
   bindings }` and dispatch on the union's tag.

5. **IR verifier — variant pattern validation.**
   Verify that `VariantWithPayload` cases appear only on
   `Type::Union` switches, that each tag is in range, and that
   each binding count matches the variant's payload arity.

6. **Interpreter — match on tag, bind payload.**
   In `Switch` evaluation, a `VariantWithPayload { tag, bindings }`
   case matches a `RuntimeValue::Union` whose `tag` equals `tag`.
   Bind each payload element to its corresponding name.

7. **Capability — refuse union match on LLVM/WASM.**
   Any `Switch` containing a `VariantWithPayload` case marks
   `Feature::Unions` in `scan_terminator`, consistent with
   `UnionConstruct`.

### Session 3 smoke test

```gol
union Json
    Null
    Num(Int)
    Str(String)

fn describe(j: Json) -> String
    return match j
        case Null then "null"
        case Num(n) then Int.to_string(n)
        case Str(s) then s

proc main
    print(describe(Json.Null))
    print(describe(Json.Num(42)))
    print(describe(Json.Str("hello")))
```

Expected output through the interpreter:

```
null
42
hello
```

### Session 3 deferred items

- **Guards** on variant patterns (`case Num(n) if n > 0`).
- **Nested patterns** (`case Pair(Some(x), _)`).
- **Reference patterns** (`case &Num(n)`).
- **LLVM/WASM codegen** for union match. Refused by
  `Feature::Unions` like construction.

### Session 3 estimated effort

Roughly 8–10 script rounds, 2–3 days of focused work. Much of the
mechanism — the `Switch` terminator, `SemanticPattern`, enum
exhaustiveness, capability scan — already exists; Session 3 adds
a new pattern kind that plugs into it. Size is closer to Session 2
Step 5 (analyzer construction, ~3 rounds) than Session 2 Step 4b
(cross-cutting threading, ~8 rounds).

## Context

ALGOL26 can express records and enums. It cannot express a value
that is one of several tagged variants carrying different payloads,
and it cannot express a type whose fields refer back to itself.

Both limits show up in any program that processes structured data.
A JSON value is one of: null, a bool, a number, a string, an array
of JSON values, or a map from strings to JSON values. An AST node
is one of: a literal, a binary operation over two child nodes, a
conditional over three child nodes, a loop over a body. A file
system entry is one of: a file with a size, or a directory with
children. A linked list is either empty or a head plus a tail.

None of these can be written today. `docs/language-reference.md`
records the gap: "The language has no variants or tagged unions;
recursive structured data (a JSON tree, an AST) is not expressible.
No ADR yet." This ADR is the missing record.

### What exists

- **`rec`** — a record with named fields of declared types.
  Fields have a fixed layout and a fixed type. No alternative
  shapes.
- **`enum`** — an ordered set of named variants. Each variant
  has an ordinal, and the whole enum is one `Int` at runtime
  (ADR 0030). No variant carries a payload.

These two do not compose into a tagged union. A user cannot say
"a `JVal` is a `JNull`, or a `JBool` carrying a `Bool`, or a
`JNum` carrying an `Int`." The tag is expressible as an enum;
the payload is expressible as a record; but nothing ties them
together such that the payload is only meaningful for its tag.

### What is missing

Two distinct but related features:

1. **Sum types.** A type defined as a disjoint union of variants,
   each with its own payload shape. `Json = Null | Bool(Bool) |
   Num(Int) | Str(String) | Arr(List<Json>) | Obj(Map<String, Json>)`.

2. **Recursive types.** A record or sum type that refers to itself
   in a field or a variant payload. `Json` above refers to itself
   through `List<Json>` and `Map<String, Json>`; a linked list
   refers to itself through `Option<List>`.

Recursive types need sum types to be useful. A record with a
self-referential field is either (a) always absent, or (b)
infinite in size. The useful recursion is through a union:
either the node has children, or it does not.

## Decision

Add **sum types** to ALGOL26. A sum type is a named type with a
fixed list of variants, each carrying zero or more payloads.
Pattern-matching is exhaustive, extending the current match
semantics.

Add **recursive types** as a consequence. A record or sum type
may refer to itself, so long as the self-reference passes
through an indirection (`List`, `Map`, `Option`, `Result`,
`Borrow`, or a variant payload). Direct self-reference — a
record with a field of its own type — stays rejected as an
infinitely sized value.

## Shape of the syntax

Two viable forms. The choice is open and belongs in a follow-up
ADR or an amendment.

**Form A — `union` keyword, separate from `enum`:**

```
union Json {
    Null
    Bool(Bool)
    Num(Int)
    Str(String)
    Arr(List<Json>)
    Obj(Map<String, Json>)
}
```

Construction: `Json.Num(42)` or `Num(42)` if the type is
inferable from context.

Destruction:

```
match v {
    case Num(n) then {
        Int.to_string(n)
    }
    case Str(s) then {
        s
    }
    ...
}
```

**Form B — `enum` gains payloads, unifying the two:**

```
enum Json {
    Null
    Bool(Bool)
    Num(Int)
    Str(String)
    Arr(List<Json>)
    Obj(Map<String, Json>)
}
```

Every existing `enum` becomes a special case where no variant
has a payload. Existing `enum` syntax and semantics are
preserved. The two forms differ only in whether the language
gains a new keyword.

Form B is smaller: one construct, not two. Form A is clearer:
a payloadless `enum` and a payloadful `union` are visibly
different kinds of thing. The ADR leaves this open; both are
workable.

## Consequences

**Positive.**

- The language can express JSON values, AST nodes, linked lists,
  trees, option-with-payload (currently `Option<T>` is the only
  such type), and any other tree-shaped data.
- Pattern matching becomes genuinely useful, not just a way to
  switch on ordinals.
- The `Result<T, E>` and `Option<T>` builtins become special
  cases of a general sum-type facility rather than hardcoded.

**Negative.**

- New AST node (`UnionDecl`, `UnionVariant`).
- New pattern form (`Pattern::VariantWithBinding`).
- The record-registration pass must happen before field-type
  resolution, so self-reference resolves. Today the pass
  registers a record only after its fields are checked.
- The IR builder must compute a size for each variant and a
  tag for the union. This is a new representation problem.
- The interpreter needs `RuntimeValue::Union { tag, payload }`.
- The LLVM and WASM backends need a representation for tagged
  unions with variable-sized payloads (boxed or fat-pointer).
  Both are non-trivial and probably interpreter-only at first,
  like records and `Option`.
- Match exhaustiveness must extend to user-defined unions.

## Implementation sketch

1. **Parsing.** `union Name { Variant Variant(T) ... }` (or
   `enum` with payloads). New AST node.

2. **Analysis — declaration pass.** Register every record and
   union name before resolving any field or variant payload
   types. This is what enables self-reference. Today's single
   pass registers a record only after checking its fields, so
   `rec Node { next: Option<Node> }` fails at the `Option<Node>`
   reference. Splitting the pass is the smallest change that
   makes recursion work.

3. **Analysis — construction and match.** `Json.Num(42)` builds
   a value of the union type. `case Num(n) then { ... }` binds
   `n` to the payload when the tag matches. Exhaustiveness
   checks that every variant is covered, or a `_` case is
   present.

4. **IR.** A union value is `{ tag: u32, payload: Box<...> }`
   or equivalent. The payload is heap-allocated; the union
   itself is a fat pointer. Interpreter gets a new value
   variant. Both backends refuse unions at first (capability
   matrix), matching the pattern for records and `Option`.

5. **Match lowering.** Extend the existing match lowering to
   handle payload extraction. For the interpreter this is
   straightforward; for LLVM it is a switch on the tag plus a
   load of the payload from a fixed offset.

## What is out of scope

- **Subtyping or open unions.** A union is closed at declaration.
- **Structural unions.** Two unions with the same variants are
  distinct types, like records.
- **LLVM and WASM codegen for payloads.** Interpreter-only at
  first; the capability matrix refuses the shapes the backends
  cannot lower. Precedent: `Option`, `Result`, records.
- **`Box<T>` or a general-purpose heap pointer.** The payload
  allocation is implicit; a user-visible `Box` is a separate
  feature.

## Alternatives considered

### A. Flat-arena encoding with integer indices

A user represents a JSON tree as `List<JVal>` where each `JVal`
holds indices into the list for its children. No language
change required.

**Rejected.** Every real program that touches trees would have
to hand-roll this encoding. The result is unreadable, error-
prone (dangling indices), and incompatible with the language's
existing ergonomics (match, exhaustive checks). The reference
already documents the gap; adding a workaround does not close
it.

### B. `Any` / dynamic typing for trees

One universal value type, checked at runtime.

**Rejected.** ALGOL26 is statically typed. Introducing `Any`
to work around one missing feature undermines the type system
for every other program.

### C. Defer indefinitely

Leave the gap documented and unimplemented.

**Rejected.** The language cannot express a substantial class
of real programs without this. A JSON parser, an AST
interpreter, a small compiler — any of these is a natural
"hello world" for a systems language, and none of them are
writable today.

## See also

- ADR 0030 — Enum Types (payloadless variants)
- ADR 0024 — Records
- `docs/language-reference.md` — "Sum types" note
- `docs/decisions/0053-interpreter-reference-semantics.md` —
  precedent for interpreter-first, backend-deferred features
