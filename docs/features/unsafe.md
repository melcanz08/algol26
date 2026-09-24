# Feature: Unsafe (`unsafe ...`)

> Per-feature contract, following the pattern described in `docs/architecture-direction.md`.
> This file is the authoritative answer to "what is `unsafe` in ALGOL26, and where does it live?"

## Status: enforced (ADR 0015)

The `unsafe` keyword is enforced for two operations: raw pointer
dereference and the `alloc` / `free` builtins. Operations that
require an unsafe block outside one are rejected at analysis time.
`AddrOf` on a non-place expression is rejected unconditionally —
inside and outside unsafe blocks — by a pre-existing check that
ADR 0015 does not change.

See `docs/decisions/0015-unsafe-enforcement.md` for the decision
and `docs/decisions/0009-unsafe.md` for the original design intent.

## Summary

`unsafe` is intended to mark regions of code that opt out of one or
more of ALGOL26's safety guarantees. Per ADR 0009, operations that
should require `unsafe` include:

- Raw pointer dereference and pointer arithmetic
- Manual memory operations outside a region
- Foreign function calls (FFI)
- Direct hardware access

The design intent is that a reader can grep for `unsafe` and see
every place the language's usual rules do not apply. This is the
same discipline as Rust: `unsafe` is not a permission, it is a
promise that the author has manually verified the local invariants.

## Current state

### What exists

| Layer | Present? | Location |
|---|---|---|
| Lexer keyword | yes | `src/frontend/lexer/mod.rs` |
| Parser | yes | `src/frontend/parser/stmt.rs` |
| AST node | yes | `Stmt::UnsafeBlock` |
| Analyzer | yes | `unsafe_depth` counter on `SemanticAnalyzer` |
| IR | not needed | blocks are unwrapped before IR |
| Verifier | not needed | the analyzer is the enforcement point |
| Backends | not needed | the IR sees plain instructions |
| Capability matrix | not needed | compile-time-only feature |

The grep across `src/` returns exactly two ALGOL26-level hits: the
lexer keyword and the parser entry point. Everything else in the
grep output is Rust-level `unsafe` blocks used by the LLVM codegen
to call inkwell APIs (which are themselves `unsafe` because they
wrap raw LLVM C calls). Those Rust `unsafe` blocks are unrelated to
the ALGOL26 `unsafe` keyword.

### What this means in practice

```gol
procedure main
    val p := alloc(4)          // rejected: alloc requires unsafe
    unsafe
        val q := alloc(4)      // accepted
```

compile to identical IR, produce identical output, and have identical
safety properties. The `unsafe` keyword is currently a no-op.

This is not a bug in the sense of producing wrong output. It is a
gap: the language advertises a safety boundary that does not exist.

## Syntax

The parser accepts `unsafe` as a statement keyword followed by an
indented block. No `do` or `end unsafe` — the body is determined by
indentation, like every other ALGOL26 block.

```gol
procedure main
    unsafe
        print("inside")
```

`unsafe` is a **statement**, not an expression. It has no type. The
block body is type-checked normally.

The old syntax shown in ADR 0009 (`unsafe do ... end unsafe`) is
historical. The current parser uses indentation.

### ADR 0009 discrepancy

`docs/decisions/0009-unsafe.md` has two conflicting status markers:

- The header note (dated 2026-09-16) says: "`unsafe` blocks and FFI
  are both implemented."
- The `## Status` section says: "🔲 Planned".

Neither is accurate. FFI is implemented (import direction only; see
`ffi.md`). `unsafe` is parsed but not enforced. The ADR should be
corrected, but ADRs are frozen — the correction belongs in a new
ADR that supersedes 0009, not in an edit to it.

## Intended semantics (per ADR 0009)

The design described in ADR 0009 has four principles:

1. **Explicit opt-in.** No operation that could violate safety is
   permitted outside an `unsafe` block.
2. **Localized and auditable.** An `unsafe` block has a bounded
   scope; reviewers can inspect exactly the region where the rules
   do not apply.
3. **Documented in code.** A comment inside the block should explain
   why the operation is safe.
4. **Compiler can identify unsafe regions.** The compiler tracks
   which functions contain `unsafe` blocks; a caller can see from
   the signature that a function might violate invariants.

The safety boundary per the ADR:

```
SAFE ALGOL26             UNSAFE ALGOL26
    |                        |
ownership                raw pointers
bounds                   pointer arithmetic
regions                  manual memory
channels                 FFI
type safety              hardware access
```

## Enforcement

The two operations gated by `unsafe`:

1. **Raw pointer dereference.** `*p` where `p: Pointer<T>`.
   `Borrow<T>` and `MutBorrow<T>` remain safe to dereference.
   The existing null-deref checks fire first.

2. **`alloc` and `free`.** Both are recognized by name in the
   builtin dispatch and require an `unsafe` block.

`AddrOf` on non-place expressions is rejected unconditionally by
the analyzer, inside and outside unsafe blocks. This check predates
ADR 0015 and is unchanged.

Implementation:

- `SemanticAnalyzer` has an `unsafe_depth: usize` counter.
- The `Stmt::UnsafeBlock` arm increments it before analyzing the
  body, decrements after. Nesting works via the counter.
- The `ExprKind::Deref` arm rejects `Pointer<T>` derefs when
  `unsafe_depth == 0`. The `ExprKind::FunctionCall` arm rejects
  `alloc` / `free` calls when `unsafe_depth == 0`.

Checks are at analysis time. The IR builder and backends are
unchanged — the boundary is a front-end distinction.

Diagnostic code: `E0007` (the ownership / memory-safety bucket).
The concrete diagnostic is carried in the message text.

## History: the original design sketch

The sections below preserve the pre-0015 planning that described a
larger Tier 2 project. The project was scoped down by ADR 0015 to
the two operations above.

## Diagnostics

Unsafe-related error codes currently emitted:

**None.** `unsafe` produces no diagnostics because it produces no
semantics.

The intended diagnostic, once enforcement exists, would be
`E-UNSAFE-001` — "operation X requires an `unsafe` block".

This is the **seventh** feature with no coded diagnostics (after
traits, generics, defer, spawn, FFI, and alloc/free). But it is a
different kind of gap: those features produce free-form strings for
errors that *do* occur, whereas `unsafe` produces no errors because
it enforces nothing.

## Test coverage

Current coverage across the tree:

**Corpus:**

- `corpus_32_unsafe_simple.gol` — `unsafe` block with one print
- `corpus_33_unsafe_in_fn.gol` — `unsafe` inside a function

**Parser:** no dedicated parser test for `unsafe`. The corpus
programs are the only fixture.

**Analyzer:** none.

**IR:** none.

**Backends:** none.

### Gaps

The gap list for this feature is the feature itself. Nothing beyond
lexing and parsing is tested, because nothing beyond lexing and
parsing exists.

Once enforcement is added, the test suite needs:

- A positive case: an unsafe operation inside an `unsafe` block is
  accepted.
- A negative case: the same operation outside an `unsafe` block is
  rejected with `E-UNSAFE-001`.
- A nesting case: `unsafe` inside `unsafe` (should be idempotent).
- An `unsafe` block with no unsafe operations (should be accepted;
  the block is empty of effect but not an error).

## Maturity

Following the stages in `docs/architecture-direction.md`:

```
unsafe
    semantics:   Not implemented (parsed only)
    parsed:      yes
    typed:       no (the block is type-checked, but the keyword's
                      effect on typing is not modeled)
    validated:   no
    IR:          N/A (no IR representation)
    verified:    no
    interpreter: N/A (block unwraps to a plain block)
    LLVM:        N/A
    WASM:        N/A
    optimized:   N/A
```

This is the lowest-maturity feature in the language. Every other
feature is at least Parsed; most are Stable. `unsafe` is Parsed and
nothing more.

## Why this matters

An advertised safety boundary that does not exist is worse than no
boundary at all. A reader of `corpus_32_unsafe_simple.gol` sees the
keyword and reasonably concludes that the code inside it does
something the compiler would otherwise refuse. It does not.

There are three honest resolutions:

1. **Implement enforcement** (the Tier 2 project described above).
2. **Remove the keyword.** If `unsafe` is not going to be enforced,
   the lexer and parser should reject it rather than silently
   ignore it.
3. **Rename it.** If `unsafe` is intended as documentation-only (a
   marker that has no compiler effect, like `#[allow(...)]` in
   Rust), then the name is misleading and something like
   `note "..."` would be honest.

Option 1 is the design intent. The other two are fallbacks if the
enforcement project is not pursued.

## Open questions

- **Is `alloc` outside a region unsafe?** In the current language,
  `alloc(n)` outside a region returns a raw pointer that must be
  freed manually. This is the kind of thing `unsafe` usually gates.
  But the `alloc_free.md` contract documents `alloc` as a normal
  expression. Resolving the mismatch is a prerequisite for
  implementing `unsafe` enforcement.

- **Is FFI unsafe?** `ffi.md` documents that `extern "C"`
  declarations and calls currently require no `unsafe` block. ADR
  0009 lists FFI as an unsafe operation. If `unsafe` enforcement
  lands, every existing FFI-using program (the corpus, the
  examples) breaks until wrapped in `unsafe`. That is a real
  migration cost, worth considering before the enforcement project
  starts.

- **Does `unsafe` appear in a function signature?** Rust distinguishes
  `unsafe fn` (a function whose callers must be in `unsafe`) from an
  `unsafe` block (a function body that contains unsafe operations).
  ADR 0009 does not mention `unsafe fn`. If the design is intended
  to include it, that is a separate declaration form; if not, then
  every function that uses `unsafe` internally looks the same to a
  caller as one that does not.

- **What is the interaction with `region`?** A region-tracked
  pointer is safe by construction. An `unsafe` block around a region
  operation is presumably redundant, but whether the language
  rejects, warns, or accepts the redundancy is undefined.

- **What happens to `unsafe` blocks with no unsafe operations?**
  The current behavior accepts them silently (since nothing is
  enforced). The intended behavior — accept, warn, or reject — is
  not defined by ADR 0009.

- **Should the parser reject `unsafe` until semantics exist?**
  This is the strongest version of "remove the keyword." A user
  writing `unsafe` today gets no warning that it does nothing.
  A `#[deprecated]`-style warning is a middle option, but Rust does
  not have a syntax for warning keywords, and adding one is a
  larger change.

## See also

- `docs/architecture-direction.md` — the feature contract pattern itself
- `docs/decisions/0009-unsafe.md` — the design decision (with a
  status marker that contradicts its own header note; see the
  "ADR 0009 discrepancy" section above)
- `docs/features/ffi.md` — FFI is listed as an unsafe operation in
  ADR 0009 but is not currently gated
- `docs/features/alloc_free.md` — `alloc`/`free` semantics and
  whether they should require `unsafe`
- `src/frontend/lexer/mod.rs:198` — the keyword
- `src/frontend/parser/stmt.rs:381` — `parse_unsafe()`
- `tests/corpus/corpus_32_unsafe_simple.gol`, `corpus_33_unsafe_in_fn.gol`
- `docs/no-panic-policy.md` — related discipline about what the
  compiler refuses
