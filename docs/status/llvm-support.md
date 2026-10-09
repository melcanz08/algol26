# Status: LLVM backend support

> What the LLVM backend supports today, what it deliberately
> refuses, and where each decision is documented. This is a
> status page, not a decision — the "why" lives in the ADR or
> feature doc cited per item.

ALGOL26 has three backends: interpreter, LLVM, WASM. They are
peers, not tiers — each consumes the same IR and offers different
feature coverage. This page tracks what the LLVM backend covers
today and what it refuses.

Programs whose feature surface exceeds what LLVM can lower are
refused by the capability check with a clear diagnostic
(`E0002: The LLVM backend does not support: <feature>`). Refusal
is always a compile error, never a silent wrong-code lowering.
That posture is deliberate — see ADR 0002 (fail-closed) and the
sessions that closed A1/A2.

A program that uses a feature LLVM cannot lower can still run on
a backend that supports it (typically the interpreter today).
Users pick the backend that fits the program; the capability
matrix (`algol26 inspect --capabilities`) is how they find out
which one.

For a machine-readable view, `algol26 inspect --capabilities`
prints the full feature × backend matrix.

## Deliberate refusals

Each item below is refused by the capability check. None is a bug;
each is either "not yet implemented" or "by design."

### D1 — `Map<K, V>`

Status: **Refused.** Interpreter-only.

Reason: the map runtime requires a hash-table implementation and
a heap-allocated backing store. No LLVM codegen exists for maps.
See `docs/decisions/0027-map.md` and
`docs/features/map.md`.

To close: implement hash-table codegen. Not currently scheduled.

### D2 — `List.append`

Status: **Refused.** Interpreter-only.

Reason: `List<T>` on LLVM is a stack-allocated `[N x elem]`
array with the length tracked in compiler bookkeeping. `.append`
requires a runtime length field, dynamic allocation, and a
growable buffer. See `docs/decisions/0042-llvm-dynamic-lists.md`
(the plan) and `docs/decisions/0028-list-append.md` (the
language feature).

To close: execute ADR 0042's four-step migration. Scheduled but
not started.

### D3 — `Result<T, E>` / `try` / `catch`

Status: **Refused.**

Reason: `Result` has no LLVM lowering. `value.rs` has arms for
`Some` and `None` (as a tagged struct), but `Ok` / `Error` return
`unsupported_operation`. The `try` / `catch` / `finally`
expression is likewise refused.

To close: define an LLVM representation for a tagged union and
lower `try` / `catch` to tagged checks and branches. No ADR yet.

### D4 — `String.split` / `String.join`

Status: **Refused.**

Reason: both return `List<String>` (or take one), and the current
LLVM `List` representation cannot be constructed from a function
return value — a `List` value not bound to a named variable has
nowhere to store the bookkeeping (see
`docs/features/list_llvm.md`). This is the same class as D2: the
static-length representation has no way to represent a
dynamically-produced list.

To close: same as D2 (ADR 0042). Once lists can be dynamic, split
and join become ordinary list-returning functions.

### D5 — `File.*` builtins

Status: **Refused.**

Reason: `File.read`, `File.write`, `File.open`, `File.close` and
the related builtins are recognized by the analyzer and dispatched
by the interpreter. No LLVM codegen exists, and no plan.

To close: lower to libc `fopen` / `fread` / `fwrite` / `fclose`
via the existing FFI infrastructure. Probably small.

### D6 — `args()`

Status: **Refused.**

Reason: `args()` returns the command-line arguments as
`List<String>`. Same list-return-value problem as D4.

To close: depends on ADR 0042. Alternatively, if the LLVM backend
gains a way to construct a `List<String>` at runtime from an
existing array, `args()` follows.

### D7 — Generic records (`rec Box<T>`)

Status: **Refused.** Documented in ADR 0036's scope boundary.

Reason: record type construction in LLVM codegen assumes a
one-to-one mapping from `Type::Record(name, args)` to a named
LLVM struct. Generic records require monomorphization — one LLVM
struct per instantiation — which the codegen does not do.

The interpreter supports generic records; the corpus fixture
`tests/fixtures/generic_record.gol` is annotated
`// supported: interpreter`.

To close: monomorphize generic records at the IR level. Medium-
sized project; no ADR yet.

### D8 — Variadic FFI format-string validation

Status: **Not validated.** By design.

Reason: `extern "C" function printf(fmt: String, ...)` is
declared variadic and calls are emitted as C variadic calls. The
compiler does not check that the argument types match the
format-string specifiers. This mirrors C: passing `%d` with a
`Float` argument is undefined behavior at the C level, and the
compiler has no format-string knowledge.

To close (optionally): implement a `printf`-style format-string
checker as an analyzer pass. This is a feature, not a bug fix.
Rust's `printf`-checking is done at compile time via a macro;
ALGOL26 would need a similar mechanism keyed on the `extern as
"printf"` binding.

Until then: documented as C-level UB. Users writing variadic
calls are responsible for format/argument agreement, same as in C.

### D9 — Record ABI across boundaries

Status: **Resolved.** No divergence.

The review flagged record-by-value as a distinct capability
because record parameters, returns, temporaries, and fields
reportedly did not agree on a value-vs-pointer representation.
That concern is now stale: ADR 0036 plus the fix in commit
`fee4b57` (FieldAccess spill for by-value records) closed the
last divergence.

Probed every boundary a record crosses, on both backends:

| Boundary | Interpreter | LLVM |
|---|---|---|
| Local record binding + field access | ACCEPT | ACCEPT |
| Record as by-value parameter | ACCEPT | ACCEPT |
| Record as by-reference parameter | ACCEPT | ACCEPT |
| Record return (constructed literal) | ACCEPT | ACCEPT |
| Record return (via variable) | ACCEPT | ACCEPT |
| Nested record field | ACCEPT | ACCEPT |
| Record in a list, indexed | ACCEPT | ACCEPT |
| Record in a list, iterated | ACCEPT | ACCEPT |
| Record field assignment | ACCEPT | ACCEPT |
| Method with `&self` receiver | ACCEPT | ACCEPT |
| Method with by-value `self` receiver | ACCEPT | ACCEPT |

Every combination that succeeds on the interpreter succeeds on
LLVM with identical output. No boundary is refused on LLVM where
the interpreter accepts.

The one historical exception, `method_by_value_receiver.gol`,
carried a `// supported: interpreter` annotation saying LLVM
refused by-value receiver methods. It now passes on both backends;
the fixture comment is out of date.

## Non-issues

The following are sometimes reported as gaps but are by design:

### D10 — `Borrow<T>` / `Pointer<T>` / `MutBorrow<T>` all collapse to `ptr`

Status: **By design.**

Reason: LLVM uses **opaque pointers** (`ptr`). The distinction
between a shared reference, a mutable reference, and a raw
pointer lives entirely in the ALGOL26 type system. The backend
sees one pointer type.

Consequence: the backend **cannot enforce any of the reference-
safety invariants** — no aliasing checks, no mutability checks,
no lifetime checks at the LLVM level. Those must all be enforced
by the analyzer (source-level) and the verifier (IR-level) before
codegen runs.

This is the intended architecture: safety is a static property,
verified early; codegen is a direct lowering that assumes the
static checks succeeded. It is the same model Rust uses
(LLVM sees `*const T` and `*mut T` as opaque pointers under
`-Zno-ptr-metadata`).

The verifier's job is to make sure no unsafe IR reaches codegen.
That is why the verifier's guarantees (ADR 0014, worklist pass
from commit `0547963`) matter so much: they are the only thing
standing between an analyzer bug and a miscompiled program.

## What's supported

For a positive list — types, statements, expressions, builtins
the LLVM backend lowers today — see:
- `algol26 inspect --capabilities` for the matrix
- `src/backends/capabilities/` for the source of truth
- `docs/features/*.md` for per-feature "Backends" tables

## See also

- `docs/decisions/0002-file-extension.md` — the fail-closed
  posture
- `docs/decisions/0014-verifier-invariants.md` — what the
  verifier guarantees before codegen
- `docs/decisions/0027-map.md` — D1
- `docs/decisions/0028-list-append.md` — D2
- `docs/decisions/0036-llvm-records.md` — D7
- `docs/decisions/0042-llvm-dynamic-lists.md` — D2 and D4's
  plan
- `docs/features/list_llvm.md` — the list representation
- `src/backends/capabilities/scan.rs` — where the refusals are
  emitted
- `src/backends/capabilities/tests.rs` — per-feature accept /
  reject tests
