# Feature: String

> Per-feature contract, following the pattern described in `docs/architecture-direction.md`.
> This file is the authoritative answer to "what is a `String` in ALGOL26, and where does it live?"

## Summary

`String` is ALGOL26's text type. Strings are UTF-8 encoded,
**non-`Copy`**, and immutable in practice — a `String` value can be
reassigned to a `var` binding, but there is no way to mutate the
bytes of an existing string in place.

The type is primitive: it is one of the built-in scalars alongside
`Int`, `Float`, `Bool`, and `Void`. It is stored inline in the
interpreter as `RuntimeValue::String(String)` and lowered in LLVM to
a `char*` pointer to a null-terminated buffer.

The key design choice: **`String.length` and `String.substring`
operate on Unicode codepoints, not bytes.** This was fixed this
session — earlier versions reported byte length, which disagreed
with `substring` on non-ASCII input. See the Byte-vs-codepoint
section below for the full history.

## Syntax

Literal:

```gol
val s := "hello"
val empty := ""
```

Escape sequences (parser-level, standard C-style):

```gol
val newline := "line1\nline2"
val tab     := "col1\tcol2"
val quote   := "say \"hi\""
val backslash := "path\\to\\file"
```

Concatenation uses `+` (lowered to `String.concat` or an inline
`strcat` call):

```gol
val greeting := "Hello, " + "world"
```

Builtins (method-call syntax desugars to function calls):

```gol
val n := String.length("hello")     // 5
val n := "hello".length             // same, method syntax
val sub := String.substring("hello", 1, 3)   // "ell"
val upper := "hi".to_upper()        // "HI"
val lower := "HI".to_lower()        // "hi"
```

## Typing rules

### The type

`Type::String` in `src/common/types.rs`. It is:

- **primitive** — included in `is_primitive()`
- **not `Copy`** — `is_copy()` returns true for `Int`, `Float`,
  `Bool`, `Ptr` only

The non-`Copy` choice is deliberate: strings are heap-allocated and
can be large. Moving them is cheaper than copying, and the borrow
model handles the cases where a function needs to read without
taking ownership.

### Cast rules

From `Type::can_cast_to` in `src/common/types.rs`:

| From | To | Allowed? |
|---|---|---|
| `Int` | `String` | yes |
| `Float` | `String` | yes |
| `Bool` | `String` | yes |
| `String` | `String` | yes (identity) |
| `String` | `Int` | **no** |
| `String` | `Float` | **no** |
| `String` | `*T` | **no** |

The cast-to-string direction is one-way by design: parsing a string
back to a number is not a cast, it is a parse operation that can
fail. The language does not yet have a `String.parse_int` builtin;
adding one would be a separate feature.

Adversarial tests pin the illegal casts:

- `tests/adversarial/14_invalid_cast_string_to_int.gol`
- `tests/adversarial/15_invalid_cast_string_to_pointer.gol`

### Type checking of builtins

Builtin signatures are registered in
`src/ir/verifier/builtins.rs::builtin_signatures()` and consumed by
the IR verifier. The analyzer has its own table that the verifier
asserts is consistent via `builtin_signatures_match_analyzer_table`.

## Ownership

### Move semantics

`String` is non-`Copy`, so passing it to a function **moves** it:

```gol
function consume(s: String) -> Int
    return String.length(s)

procedure main
    val s := "hello"
    val n := consume(s)   // s is moved into consume
    print(s)              // compile error: use after move
```

To pass without moving, use a borrow:

```gol
function borrow(s: &String) -> Int
    return String.length(s)
```

`tests/programs/invalid/string_use_after_move.gol` pins the error.

### String literals are values

A string literal in an expression position creates a fresh string
value each time it is evaluated. This means:

```gol
procedure main
    print("hello")   // fresh string
    print("hello")   // another fresh string
```

is not a leak, and there is no aliasing concern between the two
literals — they are independent values.

## Byte-vs-codepoint semantics

**`String.length` returns Unicode codepoints, not UTF-8 bytes.** This
was fixed this session in two commits:

1. `7032e46` — interpreter `String.length` and generic `len`/`length`
   switched from `s.len()` (bytes) to `s.chars().count()`
   (codepoints).
2. `aa3b5ca` — LLVM backend stopped calling C `strlen` and started
   calling `algol26_strlen_utf8`, an emitted LLVM helper that
   counts codepoints.

Before the fix, `String.length("héllo")` returned 6 (bytes) but
`String.substring("héllo", 0, 2)` returned `"hé"` (codepoints). The
two builtins disagreed on the same input. Now they agree.

The differential test `test_differential_string_length_unicode` pins
the codepoint semantics:

```rust
let source = r#"
procedure main
    val s := "héllo"
    print(String.length(s))
"#;
// Both backends report 5.
```

### The `algol26_strlen_utf8` helper

Emitted in `src/backends/llvm_codegen/builtins.rs::register_stdlib`.
It walks the byte stream until the null terminator, counting every
byte whose top two bits are not `10` (i.e. not a UTF-8 continuation
byte). That counts ASCII bytes and lead bytes of multi-byte
sequences, skipping continuation bytes — the codepoint count.

It is **not** a replacement for `strlen`. Both are registered; `strlen`
is used elsewhere (e.g. by `strcat` lowering), `algol26_strlen_utf8`
is used only by `String.length`.

## IR representation

In `src/ir/semantic_ir.rs`:

| Concept | Variant |
|---|---|
| String literal | `TypedIRValue::String(String)` |
| String length call | `TypedIRValue::Call { function: "String.length", ... }` |
| String concat | `TypedIRValue::BinaryOp { op: SemanticBinOp::Add, ... }` where both sides are `String` |
| String cast | `TypedIRValue::Cast { value, target_type: Type::String }` |

There is no dedicated string instruction. Everything is expressed
through the generic value and call instructions. This is a
deliberate choice: strings are a primitive type, not an aggregate
with special operations.

### IR verifier rules

The verifier enforces:

- `TypedIRValue::String(_)` is always well-typed and has type
  `Type::String`.
- `Call` to `String.*` builtins checks argument count and types
  against `builtin_signatures()`.
- `BinaryOp::Add` on two `String` operands returns `String`. Mixed
  operand types are a verifier error.
- `Cast` to `String` from `Int`, `Float`, or `Bool` is allowed;
  other source types are rejected by the cast rule in
  `Type::can_cast_to`.

## Builtins

In `src/ir/verifier/builtins.rs` and
`src/backends/interpreter/eval.rs`:

| Builtin | Signature | Behavior |
|---|---|---|
| `String.length` / `String.len` / `strlen` | `String -> Int` | Codepoint count |
| `len` / `length` | `String -> Int` or `List<T> -> Int` | Dispatches on argument type |
| `String.substring` | `String, Int, Int -> String` | `substring(s, start, length)`, codepoint units |
| `String.concat` / `String_concat` | `String, String -> String` | Concatenation; `+` also works |
| `String.to_upper` / `String.upper` / `to_upper` / `upper` | `String -> String` | Uppercase |
| `String.to_lower` / `String.lower` / `to_lower` / `lower` | `String -> String` | Lowercase |

### Alias explosion

Every string builtin has at least two names. `String.length` has
three (`String.length`, `String.len`, `strlen`). `String.to_upper`
has four. This is not a design feature — it accumulated as the
parser and analyzer were built out, and each name has at least one
caller in the corpus or examples. Consolidating them is a Tier 7
(canonical IR) cleanup item, not urgent.

## Print formatting

The canonical string format is `%s\n` for LLVM and `s.clone()` for
the interpreter. Both are defined in
`src/common/types.rs::print` and pinned by
`llvm_specs_match_interpreter_formats`.

The `String` type has no `format_*` function — unlike `Int`, `Float`,
and `Bool`, which have `format_int`, `format_float`, `format_bool`.
A string's `display()` is the string itself.

## Backends

| Backend | Support | Evidence |
|---|---|---|
| Interpreter | Supported | `RuntimeValue::String(String)`; builtins in `eval.rs` |
| LLVM | Supported | `%s` format; `strlen`, `strcat`, `strcmp`, `algol26_strlen_utf8` registered |
| WASM | Supported | `string_length_builtin.gol`, `strings.gol` in conformance; WASM differential tests pass |

### Interpreter

`RuntimeValue::String(String)` is a Rust `String` clone. String
operations are direct Rust method calls:

- `String.length` uses `s.chars().count()` (codepoints)
- `String.substring` uses `s.chars().collect::<Vec<_>>()` and slices
- `String.to_upper` / `to_lower` use `to_uppercase` / `to_lowercase`
- `+` on two strings uses `a + &b`

The interpreter's `runtime_eq` compares strings with `==`, which is
byte-level in Rust. Two strings that render the same but have
different UTF-8 encodings (which is impossible in valid Rust
`String` — Rust normalizes on construction) would not compare equal.

### LLVM

Strings are lowered to `i8*` pointers to null-terminated buffers.
Operations lower to C library functions or the emitted UTF-8 helper:

| Operation | Lowering |
|---|---|
| `print(s)` | `printf("%s\n", s)` |
| `s1 + s2` | `strcat(s1, s2)` — note: mutates `s1` in place, which is why strings are non-`Copy` |
| `String.length(s)` | `algol26_strlen_utf8(s)` |
| `String.substring(s, a, b)` | **unverified** — no lowering seen in the code I read |
| `String.to_upper(s)` | **unverified** — likely no lowering; may refuse via capability check |
| `String.to_lower(s)` | **unverified** — same |

**The `strcat` mutation is a subtle correctness constraint.** A
naive `s1 + s2` that lowered to `strcat` would need `s1` to have
spare capacity, which C `strcat` does not provide. The current
lowering may allocate a new buffer and copy; I have not verified
which. If it does not, concatenation in LLVM could corrupt memory.
This is a Tier 2 (fail-closed audit) item.

### WASM

WASM has linear memory and can host the same string operations as
LLVM. The differential test
`test_wasm_compiles_same_programs_as_llvm` includes string programs,
so WASM string support exists at some level. The exact lowering is
unverified.

## Diagnostics

String-related error codes currently emitted:

| Code | Meaning | Emitted from |
|---|---|---|
| `E-MOVE-001` | Use of moved string | `dataflow.rs` (general move) |
| `E-MOVE-002` | Move of borrowed string | `dataflow.rs` |

**Same diagnostic gap as traits, generics, defer, spawn, FFI,
alloc/free, and unsafe.** Use-after-move on a string is reported as
the general move error, not a string-specific code. This is correct
behavior but loses the connection to strings.

Cast errors (e.g. `String as Int`) are reported via the analyzer's
general cast-mismatch path, also without a dedicated code.

## Safety

- **No null strings.** A `String` value is always valid; there is
  no `null` string.
- **No mutable aliasing.** Strings are non-`Copy` and immutable in
  practice; a `&String` shared borrow can coexist with another
  `&String`, but not with a `&mut String`.
- **No uninitialized strings.** A `String` value always has a valid
  UTF-8 buffer.
- **No bounds violations in `substring`.** `substring(s, start, len)`
  clamps `start` to `[0, s.len()]` and computes `end = min(start +
  len, s.len())`, so it never panics. The `saturating_add` on the
  length protects against overflow when `len` is near `usize::MAX`.
  This was hardened this session.

## Test coverage

Current coverage across the tree:

**Conformance (`tests/conformance/valid/`):**

- `strings.gol` — basic string operations
- `string_operations.gol` — broader set
- `string_move.gol` — move semantics
- `string_length.gol` — `String.length`
- `string_length_builtin.gol` — `String.length` as a builtin call

**Differential (`tests/differential/differential_true.rs`):**

- `test_differential_string_output` — print two strings
- `test_differential_string_length_builtin` — `String.length("hello")`
- `test_differential_string_length_unicode` — codepoint semantics
- `test_differential_method_syntax_no_parens` — `s.length` and `s.length()` agree

**Corpus:**

- `corpus_08_string_len.gol` — string length
- `corpus_15_list_of_strings.gol` — list of strings
- `corpus_24_channel_string.gol` — string sent through channel

**Adversarial:**

- `14_invalid_cast_string_to_int.gol` — `String as Int` rejected
- `15_invalid_cast_string_to_pointer.gol` — `String as *T` rejected

**Programs (invalid):**

- `string_use_after_move.gol` — use-after-move rejected
- `type_error_string.gol` — type mismatch on string

**Lexer:**

- `test_string_escapes`, `test_comments_in_strings`,
  `test_escaped_backslash_before_quote`,
  `test_escaped_quote_does_not_break_comment_stripping`

**Common types:**

- `test_type_parsing` — `"string"` and `"str"` parse as `Type::String`
- `test_can_cast_to` — `Int -> String` allowed; `String -> Int` rejected

### Gaps

- **No test for `String.substring` on non-ASCII input.** The
  substring builtin uses `chars()`, but no test pins its behavior
  with multi-byte characters.
- **No test for `String.to_upper` / `to_lower` on non-ASCII input.**
  Rust's `to_uppercase` handles Unicode correctly, but this is not
  pinned by a test.
- **No test for `String.concat` explicitly** (only via `+`, which
  is tested indirectly).
- **No test for the aliases.** `String.upper`, `to_upper`, `upper`
  — all four should behave identically; only the canonical name is
  tested.
- **No test for empty string.** `String.length("")`, `"" + "a"`,
  `String.substring("", 0, 0)` — none of these are pinned.
- **No test for `String.substring` with out-of-range start/length.**
  The clamping behavior is documented in the contract but not
  asserted by tests.
- **No test for `String.substring` overflow.** The
  `saturating_add` fix was made this session; no test exercises
  the near-`usize::MAX` length case.
- **No test for LLVM lowering of `String.substring`, `to_upper`,
  or `to_lower`.** These may or may not be lowered; if not, the
  capability check should refuse. Not verified.
- **No test for `s1 + s2` allocation semantics.** Whether the LLVM
  lowering allocates a new buffer or mutates `s1` in place is
  unverified. If it mutates, aliasing bugs are possible.

## Maturity

Following the stages in `docs/architecture-direction.md`:

```
String
    semantics:   Stable (with one Tier 2 unknown: LLVM concat allocation)
    parsed:      yes
    typed:       yes
    validated:   yes
    IR:          yes (as typed values, not special instructions)
    verified:    yes
    interpreter: supported
    LLVM:        partial (length yes, print yes; substring/upper/lower unverified)
    WASM:        supported (exact coverage unverified)
    optimized:   no string-specific rules
```

String is one of the most-used types in the language. Its basic
operations (construct, print, length, concatenate) are stable and
well-tested. Its more advanced operations (`substring`, case
conversion) have a smaller surface and would benefit from more
test coverage.

## Checklist for related features

If you are adding a feature *like* `String` (a primitive type with
builtin operations, non-`Copy` semantics, and per-backend lowering),
you need to touch:

1. `src/common/types.rs` — `Type` variant (if not already present)
   + parsing + cast rules + `is_copy` decision + `Display`.
2. `src/common/types.rs::print` — LLVM format spec + interpreter
   format function if applicable.
3. `src/ir/verifier/builtins.rs` — signatures for the new builtins.
4. `src/semantics/analyzer/` — the analyzer's builtin signature
   table (must match the verifier's; pinned by a test).
5. `src/backends/interpreter/runtime.rs` — `RuntimeValue` variant
   (if not already present).
6. `src/backends/interpreter/eval.rs` — builtin implementations.
7. `src/backends/interpreter/runtime.rs` — extend `runtime_eq`.
8. `src/backends/llvm_codegen/builtins.rs` — C library registration
   + lowering.
9. `src/backends/wasm_backend.rs` — WASM lowering or refusal.
10. `src/backends/capabilities/scan.rs` — declare backend support.
11. `src/backends/capabilities/tests.rs` — accept/reject per backend.
12. `tests/conformance/valid/<feature>.gol`.
13. `tests/differential/differential_true.rs` — differential test.
14. `docs/features/<feature>.md` — this file.

## Open questions

- **Should `String.substring` be O(1)?** Currently it is O(n) in
  the codepoint count because it collects `chars()` into a `Vec`
  before slicing. For a string type in a systems language, this is
  potentially expensive. A future optimization could use a
  byte-offset index and translate offsets lazily.

- **Should there be a `String.parse_int` / `parse_float`?** Not
  currently. Casting from `String` to `Int` is illegal, and there
  is no builtin to do the parse. Adding one would require an error
  return type (`Result<Int, ParseError>` or similar).

- **Should strings be interned?** Two identical string literals
  currently produce two separate values. Interning would save
  memory but complicates move semantics. Not currently planned.

- **Should `s1 + s2` be `O(n)` or `O(n + m)`?** The LLVM lowering
  uses `strcat`, which is `O(n + m)` where `n = strlen(s1)`, `m =
  strlen(s2)`. Whether the buffer for `s1` is reallocated or reused
  is unverified.

- **Should `String.length` be a field access or a method call?**
  Currently it is a method call desugared to `String.length(s)`,
  but the syntax `s.length` looks like field access. The
  desugaring is invisible to the user.

- **What is the escape sequence set?** The lexer tests cover `\n`,
  `\t`, `\"`, `\\`, but the full set (Unicode escapes? hex
  escapes? raw strings?) is not documented in a reference. A
  `docs/language-reference.md` section on string literals would
  help.

- **Should `String` be `Copy` for small strings?** Some languages
  use small-string optimization to make short strings `Copy`. Not
  currently implemented; the `is_copy` rule excludes `String`
  unconditionally.

- **Should string comparison be defined?** `strcmp` is registered in
  LLVM, but I have not seen a `<` or `>` operator applied to strings
  in the analyzer. Whether string ordering is a language feature
  is not pinned by tests.

- **Should the LLVM concat lowering be audited?** Yes — see the
  LLVM section above. This is a Tier 2 item.

## See also

- `docs/architecture-direction.md` — the feature contract pattern itself
- `docs/features/list.md` — the sibling non-`Copy` container
- `docs/features/ffi.md` — `String` maps to `char*` at the FFI boundary
- `docs/decisions/0003-type-system.md` — the numeric tower and primitive types
- `src/common/types.rs` — `Type::String`, print formatting, cast rules
- `src/backends/interpreter/runtime.rs` — `RuntimeValue::String`
- `src/backends/interpreter/eval.rs` — builtin implementations
- `src/backends/llvm_codegen/builtins.rs` — C library registration and `algol26_strlen_utf8`
- `tests/conformance/valid/strings.gol`
- `tests/differential/differential_true.rs` — string differential tests
- `docs/features/unsafe.md` — string operations do not require `unsafe`
