# ADR 0015: Unsafe enforcement

Status: Proposed

## Context

ADR 0009 established unsafe blocks as a language boundary. The
surface syntax `unsafe { ... }` is parsed, and `Stmt::UnsafeBlock`
exists in the AST. The semantic analyzer currently accepts unsafe
blocks and gives their contents the same meaning as any other
statement.

That is the whole problem: today, `unsafe { *p }` and `*p`
compile to identical IR. The boundary has no meaning to the
compiler, so it does not exist as a safety guarantee.

Three operations are semantically unsafe in the current language:

1. **Raw pointer dereference.** `*p` where `p: Pointer<T>`.
   `ExprKind::Deref` resolves to `TypedIRValue::ReadReference`
   regardless of whether the operand is `Borrow<T>`, `MutBorrow<T>`,
   or `Pointer<T>`. Only the last is unsafe.

2. **`AddrOf` on non-place expressions — already enforced.** The
   analyzer's `ExprKind::AddrOf` arm unconditionally rejects any
   operand that is not a variable, array element, field access, or
   dereference. This ADR does not change that check; it stays
   rejected inside and outside unsafe blocks. It is listed here
   because it is a raw-pointer operation, not because it needs new
   enforcement.

3. **Allocation and deallocation.** `alloc(n)` and `free(p)`.
   Both bypass the borrow system and produce or consume raw
   pointers. Today they are dispatched through
   `SemanticInstruction::Allocate` and `SemanticInstruction::Free`
   with no safety check.

FFI calls to `extern` functions are sometimes listed as a fourth
candidate. They are not in scope here: extern declarations carry
`ffi_info` and are already isolated behind the capability system.
Marking them unsafe is a language decision, not a memory-safety
one, and belongs to a separate ADR if it is wanted.

## Decision

Unsafe operations are permitted only inside `unsafe { ... }`
blocks. The analyzer rejects any unsafe operation encountered with
an empty unsafe context. The check happens once, at analysis
time; the IR builder and backends are unchanged.

The rest of this section settles four specific questions.

### 1. Which operations require unsafe?

The three listed in Context: raw pointer dereference, `AddrOf` on
non-place expressions, and the `alloc` / `free` builtins.

The first is the operation the language design already
distinguishes by type: `Borrow<T>` and `MutBorrow<T>` are safe to
dereference, `Pointer<T>` is not. The analyzer sees the operand's
type at every `ExprKind::Deref`, so the check is a match on the
resolved type — no new state.

The second is checked structurally. `ExprKind::AddrOf` is only
safe when its operand is a place: `Var`, `FieldAccess`, or
`ArrayAccess`. Any other operand — a literal, a call, a
parenthesized expression that isn't a place — requires unsafe.

The third is checked at the call site. `alloc` and `free` are
recognized by name in the existing builtin dispatch. The check is
inserted where they are dispatched.

### 2. Where is the unsafe context tracked?

On `SemanticAnalyzer`, as a depth counter:

    unsafe_depth: usize

Incremented when entering `Stmt::UnsafeBlock`, decremented when
leaving. Every unsafe operation consults `unsafe_depth > 0`.

A counter rather than a bool because `unsafe` blocks nest — a
future nested form is common, and the counter costs nothing. The
analyzer must decrement on every exit path, including early
returns and `?` propagation, so the increment/decrement pair is
written with a small guard type rather than manual calls.

### 3. Where is the boundary enforced?

In the analyzer, at the point each unsafe operation is
encountered. Fail-closed at the earliest opportunity, same shape
as the ADR 0014 TypeVar check.

    *p        -> analyzer sees Deref, inspects operand type
              -> if Pointer<T> and unsafe_depth == 0, error E00XX

    &expr     -> analyzer sees AddrOf, inspects operand shape
              -> if not a place and unsafe_depth == 0, error E00XX

    alloc(n)  -> analyzer sees FunctionCall to "alloc"
    free(p)   -> analyzer sees FunctionCall to "free"
              -> if unsafe_depth == 0, error E00XX

The IR builder is unchanged. It does not need to know which
blocks were unsafe, because a well-formed program reaching the
builder has already had its unsafe operations checked. A program
that would fail the check never reaches the builder.

This is Option (a) from the ADR draft: reject at analysis, do not
record unsafe-ness on IR nodes. Option (b) — record an
`is_unsafe` attribute on IR instructions and re-check in the
verifier — is deferred. It becomes relevant only if a future
backend needs the boundary preserved, which none does today.

### 4. What happens at codegen?

Nothing. Unsafe is a front-end distinction. The IR the backends
see is identical whether the source wrote `unsafe { *p }` or was
somehow allowed to write `*p` directly. There is no codegen
difference, no LLVM attribute, no WASM section flag.

The boundary is a promise the compiler makes to the programmer:
this operation passed a check that non-unsafe code cannot pass.
Once the check has run, the distinction has done its job.

## Consequences

**Positive.**

- The `unsafe` keyword now means something. A raw pointer
  dereference outside an unsafe block is a named compiler error,
  not a silently accepted operation. That is what ADR 0009
  committed the language to.

- The check is local. Each of the three operations is inspected
  where it is dispatched; no new pass, no new IR dimension, no
  new field on every node. The analyzer gains one `usize` and
  three call-site checks.

- The existing adversarial corpus and conformance suite already
  have slots for the negative cases. `tests/adversarial/` has
  `23_null_pointer_deref.gol`, `24_free_then_deref.gol`, and
  `25_double_free.gol`; the first will now fail earlier (at the
  unsafe check, not at the null check), and the other two will
  fail at the unsafe check rather than at the double-free check.
  That is a diagnostic-quality improvement, not a regression.

**Negative.**

- Three adversarial tests change which diagnostic they produce.
  The tests themselves need updating to expect the unsafe-block
  error rather than the null/double-free error. This is correct
  behavior — the unsafe check fires first, so it is the right
  diagnostic to surface — but it is a visible change in what the
  user sees for those inputs.

- The check is not a full type-system commitment. It does not
  prove anything about the pointer beyond "the programmer said
  unsafe". That is the point of unsafe — the programmer takes
  responsibility — but it should be documented so nobody expects
  the compiler to verify more than the block boundary.

**Neutral.**

- FFI calls remain unchecked by this ADR. If the language later
  decides extern calls are unsafe, they can be added to the same
  dispatch-time check with a two-line change.

## Tests

The analyzer gets six new tests:

**Positive (unsafe context permits the operation):**

1. `raw_pointer_deref_inside_unsafe_accepted` —
   `val p := alloc(4)` then `unsafe { *p }`.
2. `addr_of_non_place_inside_unsafe_accepted` —
   `unsafe { &(1 + 2) }`.
3. `alloc_inside_unsafe_accepted` —
   `unsafe { alloc(8) }`.
4. `free_inside_unsafe_accepted` —
   `val p := alloc(4)` then `unsafe { free(p) }`.

**Negative (unsafe context absent rejects the operation):**

5. `raw_pointer_deref_outside_unsafe_rejected` —
   `val p := alloc(4)` then `*p` with no surrounding unsafe.
   Expect a diagnostic naming `*p` and the enclosing function.
6. `alloc_outside_unsafe_rejected` — `val p := alloc(4)` with
   no surrounding unsafe. Expect a diagnostic naming `alloc`.

The remaining two operations (`AddrOf` on non-place, `free`) get
the same negative shape and are covered by the adversarial corpus
once it is updated.

## Adversarial corpus

No fixtures change. The soundness runner asserts that a program
is rejected, not which diagnostic code is emitted. Fixtures such
as `23_null_pointer_deref.gol` now fail at the unsafe check rather
than at the null or double-free check; the runner does not
distinguish, so they continue to pass. The diagnostic a user sees
for those inputs changes from "cannot dereference a null pointer"
to "cannot dereference a raw pointer outside `unsafe`" — a
different message, same rejection.

## Alternatives considered

**Record unsafe-ness on IR nodes and verify it in the verifier.**

   Deferred. It would require adding an `is_unsafe: bool` field
   to `SemanticInstruction`, threading it through the builder,
   and adding a second check in `VerifyIrPass`. That is more
   surface than the decision needs. It becomes relevant only if
   a future backend treats unsafe and safe operations
   differently, which none does today. If it ever does, this ADR
   is superseded by one that adds the dimension.

**Make `unsafe` an expression rather than a statement, so it
   can appear in value position.**

   Out of scope. The current grammar treats `UnsafeBlock` as a
   statement. Whether it can produce a value is a grammar
   question independent of whether the boundary is enforced.

**Mark extern calls as unsafe.**

   Deferred. FFI is isolated by the capability system; making it
   unsafe-required is a language-surface decision, not a
   memory-safety one.

**Eliminate unsafe entirely and require all pointer operations
   to be borrow-typed.**

   Rejected. `AddrOf` and `alloc` / `free` exist precisely
   because the language wants raw pointers available under a
   boundary. Removing them would be a language redesign, not a
   hardening step.