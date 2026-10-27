# ADR 0060: Union Codegen on LLVM and WASM

Status: Proposed. This ADR implements the backend-codegen half
of ADR 0055 ("Sum Types and Recursive Types"), which that ADR
explicitly deferred:

> Status: Implemented. ... LLVM/WASM codegen for union payloads
> is deferred to a follow-up ADR — union programs are refused by
> `Feature::Unions` at the capability boundary.

This is that follow-up.

## Context

ADR 0055 added `union` declarations to the language. A union
value is one of several tagged variants, each carrying zero or
more typed payloads:

```gol
union Json
    Null
    Bool(Bool)
    Num(Int)
    Str(String)
    Arr(List<Json>)
    Obj(Map<String, Json>)
```

Construction (`Json.Num(42)`, `Json.Null`), pattern matching
(`case Num(n) then ...`), exhaustiveness checking, and cross-file
merging all landed on the interpreter. On LLVM and WASM, the
capability matrix refuses every union program at the boundary
via `Feature::Unions` with the `--interpreter` hint. That
refusal is clean — no panics, no miscompiles — but it means a
union-using program cannot be compiled to a native binary.

This blocks a real use case. A tagged union is the natural
representation of a compiler's IR (an `Instr` is `Add`, or
`Load`, or `Branch`, each carrying different payloads); a JSON
value; an AST node; a linked list. Programs that need these
shapes must currently run through the interpreter. That is
acceptable for teaching examples but not for anything that
needs native performance or a static binary.

ADR 0055's own "What is missing" section names the lowering
problem explicitly:

> The LLVM and WASM backends need a representation for tagged
> unions with variable-sized payloads (boxed or fat-pointer).
> Both are non-trivial and probably interpreter-only at first,
> like records and `Option`.

The "like records and `Option`" precedent is the load-bearing
one. Records and `Option` were also interpreter-only at first,
and both later landed on LLVM via named struct types with
field-based GEP+load+store. This ADR does the same for unions.

## Decision

Represent a union value on LLVM and WASM as a named LLVM struct
with two fields:

```llvm
%Union_<Name> = type { i32 tag, [N x i64] payload }
```

Where:

- `tag` is the variant's ordinal (a `u32`, matching
  `SemanticPattern::VariantWithPayload { tag, .. }` and the
  `UnionConstruct` instruction's `tag` field).
- `N` is `ceil(max_payload_size / 8)`, where `max_payload_size`
  is the largest payload size (in bytes) across all variants.
  For a payloadless-only union, `N = 0` (a zero-length array —
  LLVM handles this as a placeholder).
- The payload buffer is `[N x i64]`. The choice of `i64` (rather
  than `i8`) is deliberate: an `[N x i8]` field has alignment 1,
  so the payload buffer would not be 8-aligned, and storing an
  `i64`-sized payload (e.g. `Int`, `Float`, `String` pointer,
  list descriptor) into it would be misaligned UB. `[N x i64]`
  is naturally 8-aligned, matching the maximum alignment of any
  ALGOL26 type. Every ALGOL26 type has alignment <= 8. So
  `[N x i64]` suffices for every payload.
- The payload buffer is untyped storage (a repeated `i64`, not a
  typed struct). At construction and at pattern-match time, the
  codegen GEPs into the buffer to get an `i64*`, then bitcasts
  to the specific variant's payload type and performs a
  load/store at offset 0.

A variant with no payloads (`Null`) writes only the tag; the
payload buffer is left `undef`. A variant with one payload
writes that payload as a single value. A variant with multiple
payloads writes them as an anonymous struct `{T1, T2, ...}`
occupying the payload buffer, matching the declaration order.

The struct type is cached in a new `union_struct_types` table on
`IRCodeGen`, mirroring the existing `record_struct_types` table
(ADR 0036). Naming convention: `%Union_<Name>` where `<Name>` is
the declared union name. Same-name unions from different modules
are distinguished by their `UnionTypeId`, same as `Type::Record`.

## Implementation plan

Five sessions. Each lands as its own commit; each is independently
verifiable.

### Session 1 — `map_type(Type::Union)` + payload struct caching

Add:
- `IRCodeGen::union_struct_types: RefCell<HashMap<UnionTypeId, StructType>>`
- The `Type::Union { .. }` arm of `map_type`, replacing the
  current `unreachable!`. It:
  1. Looks up the cached struct type by `UnionTypeId`.
  2. If missing, builds the payload struct type from the
     variant payload sizes and alignments.
  3. Builds the outer struct `{i32, payload}`, caches it, returns.

Add a unit test that constructs `Type::Union` values and asserts
the LLVM struct shape: field count, tag field type, payload size.

No codegen arms are added in this session. A union program still
fails at the capability check. The session's contribution is
that `map_type` no longer panics for unions — though the panic
isn't reached because the capability check refuses first.

### Session 2 — `Instruction::UnionConstruct` codegen

Add the codegen arm for `Instruction::UnionConstruct { target,
union_name, variant_name, tag, payload }` in
`src/backends/llvm_codegen/instruction.rs`. It:

1. Resolves the union's struct type from `var_types[target]`'s
   `Type::Union` (the builder registers the target's type
   before emitting the instruction).
2. Allocates a fresh `%Union_<Name>` on the stack (via
   `create_entry_alloca`).
3. Stores `tag` into field 0.
4. If `payload` is non-empty, composes the payload struct (either
   a single value or an anonymous struct for multi-payload
   variants) via `build_insert_value` on `undef`, then stores
   into the payload buffer with an appropriate GEP+store.
5. Registers `target` in `variables` and `var_types`, same as
   `Instruction::Declare`'s record arm.

The multi-payload construction pattern is the same one
`Instruction::Declare`'s record-literal path already uses to
build a `%Point` from `{x, y}` — `insertvalue` chained on
`undef`. No new mechanism, just the same pattern applied to the
payload struct type.

The capability refusal is not lifted in this session — the
codegen path is untested end-to-end until Session 3 adds match
lowering. Tests in this session construct IR directly and
verify the codegen output has the expected shape.

### Session 3 — `Switch` union fast path

Add `IRCodeGen::compile_union_switch`, mirroring
`compile_option_switch` and `compile_result_switch`. Fast-path
condition (in `compile_terminator`'s `Switch` arm): scrutinee
type is `Type::Union { .. }` AND every case is either
`SemanticPattern::VariantWithPayload { .. }` or
`SemanticPattern::Wildcard`.

The lowering:

1. Extract the tag from field 0 of the scrutinee (an `i32` load).
2. For each `VariantWithPayload { tag, bindings }` case:
   - Create a proxy block `<variant>_bind_<tag>`.
   - In the proxy, for each binding `b` at index `i`:
     - If the variant has one payload, load from the payload
       buffer at offset 0 with the payload type.
     - If multiple, GEP to `payload_struct.field[i]` and load.
     - Store into the binding's alloca.
   - Branch to the case body block.
3. `build_switch` on the tag with `i32` case values.

`Wildcard` cases are pulled out as the switch's default (matching
`compile_option_switch` and the current `Switch` handler).

This session flips the capability:
- Add `Feature::Unions` to `llvm().supported` and
  `wasm().supported`.
- Flip `MATRIX`'s `union` row's `llvm` and `wasm` columns from
  `Refused` to `Full`.
- Update `EXPECTED_MATURITY` for `union` from `InterpreterOnly`
  to `AllBackends`.
- Delete `union_declaration_rejected_by_llvm` and
  `union_declaration_rejected_by_wasm` from
  `src/backends/capabilities/tests.rs`.
- Add `llvm_accepts_union_construction` and
  `llvm_accepts_union_match` capability tests.

Add three corpus fixtures:
- `corpus_62_union_simple_llvm.gol`
- `corpus_63_union_multi_variant_llvm.gol`
- `corpus_64_union_recursive_llvm.gol`

This is the session where the feature becomes visible to users.
The refusal is lifted; the fixtures pin end-to-end behavior.

### Session 4 — Payload type coverage

The three corpus fixtures in Session 3 cover `Int`, `String`,
`List<Json>` (for `Json`), and `Map<String, Json>` payloads.
This session verifies coverage of the remaining payload types
that should work by construction but that no fixture pins:

- `Float`, `Bool`, `Char` — scalars.
- `Option<T>`, `Result<T,E>` — already-existing sum types as
  union payloads.
- Nested unions.
- Recursive unions with `Arr(List<Json>)` and
  `Obj(Map<String, Json>)`.

Add fixtures as needed. Update `MATRIX`'s `union` notes to name
the coverage.

If any of these payload types fail, that's a bug in Session 1
or 2, and the fix lands here.

### Session 5 — Documentation + close-out

- Update `docs/STATUS.md`: remove the union row's
  "Interpreter-only" caveat; add a "Known Gaps" bullet noting
  the region-cleanup deferral.
- Update `docs/features/union.md` (or equivalent) to describe
  the LLVM lowering.
- Update `docs/decisions/0055-sum-and-recursive-types.md`'s
  Status line to reference ADR 0060 as complete.

## What is out of scope (v1)

The following are known limitations of v1, each documented as a
follow-up:

### 1. Region cleanup of heap-managed payloads

A union value is stack-allocated (like records), so the union
itself does not interact with region tracking. But a payload
like `Str(String)` or `Arr(List<Json>)` may hold a heap pointer
that is owned by the union value. If the union is stored in a
region, the payload's buffer leaks at region exit — same class
as `List` values stored in record fields today.

The fix requires per-union-value dataflow tracking: a "does
this union hold a live heap payload at this point" analysis,
threaded through region exit. That's a follow-up to ADR 0050
(region cleanup of heap lists) — this ADR does not extend that
analysis to union payloads.

Users who care about memory in v1 should not store union values
with `String`/`List` payloads in regions, or should run through
the interpreter.

### 2. Nested patterns

`case Pair(Some(x), _)` is not supported, matching the language's
current limitation. This is not a union-specific issue.

### 3. Reference patterns

`case &Num(n)` is not supported, matching ADR 0055 Session 3's
deferred items.

### 4. Guards on variant patterns

`case Num(n) if n > 0 then ...` is not supported on LLVM/WASM.
The interpreter supports it. The pattern guard's condition
cannot be lowered through the `Switch` terminator — it requires
a `Branch` after the tag check. Deferred.

### 5. `Box<union>` or user-visible boxing

The payload is inline in the union value, not boxed. A variant
whose payload is large makes every union value that large. This
is the trade-off of Layout D. A future ADR adding `Box<T>`
would let a variant explicitly opt into heap allocation.

## Consequences

Positive:

- Union programs compile to native on LLVM and WASM, joining the
  interpreter. A compiler backend written in ALGOL26 becomes
  writable.
- The runtime representation matches records and lists, so
  region tracking, DCE, optimizer, and IR verifier work on
  unions with the same reasoning they use for those types.
- Recursive unions (`Json` with `Arr(List<Json>)`) work as a
  consequence — recursive records already work.

Negative:

- New struct types per union, cached in a new side table.
- New codegen arms in three places: `map_type`, `instruction.rs`
  (construction), `terminator.rs` (pattern matching).
- No region cleanup of heap payloads (documented gap, not silent).
- Patterns with guards on union variants remain interpreter-only.
- No `Copy` optimization for large unions — every union value is
  the max of all payload sizes, even when the tag is a small
  variant. For `Json` with `Str(String)` (8 bytes) and
  `Arr(List<Json>)` (24 bytes), every `Json` value is 32 bytes.
  Acceptable for the common case, but would be problematic for a
  union with wildly different payload sizes.

## Alternatives considered

### A. Boxed payload (opaque pointer to heap-allocated payload)

`{ i32 tag, ptr payload }` where `payload` is `malloc(sizeof(
payload_i))` at construction.

Rejected. Every union value requires an allocation at
construction, and every match requires a load through the
pointer. The allocation cost dominates for small-payload
variants and the region-cleanup story is worse. A `Json` tree
of depth 1000 with 10k nodes allocates 10k times.

### B. Fat pointer (tag + ptr to a stack-allocated box)

`{ i32 tag, ptr payload }` where `payload` points at a stack
allocation.

Rejected. Escape analysis: a `Json` value returned from a
function would carry a stack pointer that dangles. The value
must be self-contained.

### C. Inline typed struct per variant, tagged with an i32

`{ i32 tag, %Json.Num.Payload num, %Json.Str.Payload str, ... }`
— every variant's payload as a distinct field, all inline.

Rejected. Works for `Option` and `Result` (exactly two
variants), but for N > 2 every union value pays the sum of all
payload sizes. Storage grows quadratically with the number of
variants in a chain of nested unions. Not viable.

### D. Inline packed buffer (this ADR's choice)

`{ i32 tag, [N x i8] payload }` where `N = max(sizeof(
payload_i))`.

Chosen. Standard technique for tagged unions in static
languages (Rust, C++, OCaml, ML). Storage is `4 + align(N)`
bytes per union value, independent of the number of variants.
Construction is stack allocation + two stores (tag + payload).
Matching is one tag load + one GEP + one payload load. No
allocations, no escape analysis needed for the union value
itself.

### E. `void*` payload with type-erased casts

`{ i32 tag, ptr payload }` where the payload is always
heap-allocated and type-erased.

Rejected. Type erasure loses the verifier's ability to check
that a variant's payload binding has the right type at
IR-build time. The whole point of the ALGOL26 type system is
that payload types are known statically.

## Testing

Three corpus fixtures in Session 3:

- `corpus_62_union_simple_llvm.gol` — a two-variant union
  (`Int`/`Bool`), construct and match. Tests the simplest
  lowering (tag + single payload).
- `corpus_63_union_multi_variant_llvm.gol` — a four-variant
  union with mixed payload types (`Null`, `Int`, `String`,
  `Pair(Int, Int)`). Tests multi-payload struct packing and
  more than two tags.
- `corpus_64_union_recursive_llvm.gol` — a recursive `Json`
  union with `Arr(List<Json>)` and `Obj(Map<String, Json>)`
  payloads. Tests recursive unions and container payloads.

Session 4 adds fixtures for `Float`, `Bool`, `Char`, `Option`,
`Result`, and nested unions.

Capability tests in Session 3:
- `llvm_accepts_union_construction` — asserts `check_backend`
  accepts a program containing `Instruction::UnionConstruct`
  on LLVM.
- `llvm_accepts_union_match` — asserts `check_backend` accepts
  a program containing a `Switch` with `VariantWithPayload`
  cases on LLVM.
- The current `union_declaration_rejected_by_llvm` and
  `union_declaration_rejected_by_wasm` tests are deleted;
  they were pinning the refusal this ADR removes.

MATRIX (Session 3):
- `union` row's `llvm` and `wasm` columns flip from `Refused`
  to `Full`.

EXPECTED_MATURITY (Session 3):
- `union` changes from `InterpreterOnly` to `AllBackends`
  (assuming `conformance_dir: Some(..)`; otherwise
  `Universal`).

## See also

- **ADR 0055** — Sum Types and Recursive Types. This ADR
  implements its deferred backend-codegen half.
- **ADR 0036** — LLVM Records. Closest precedent: named
  struct types, GEP-to-field construction, struct-value
  representation.
- **ADR 0042** — LLVM Dynamic Lists. The payload-buffer
  pattern is the same shape this ADR uses for union payloads.
- **ADR 0050** — Region Cleanup of Heap Lists. The region-
  tracking deferral in this ADR is a follow-up to that ADR's
  scope.
- **ADR 0052** — LLVM Codegen Architecture. The codegen arms
  added here follow the patterns documented there.
