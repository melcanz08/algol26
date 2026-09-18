# Feature: Alloc / Free (`alloc(n)`, `free(p)`)

> Per-feature contract, following the pattern described in `docs/architecture-direction.md`.
> This file is the authoritative answer to "what are `alloc` and `free` in ALGOL26, and where do they live?"

## Summary

`alloc(n)` returns a pointer to `n` bytes of uninitialized heap
storage. `free(p)` releases storage previously obtained from
`alloc`. They are the explicit memory-management primitives, the
counterpart to the region-based automatic model.

The language guarantees:

1. **No implicit allocation.** Every heap allocation is either
   the result of a visible `alloc(n)` call, or is performed by a
   region on the program's behalf — the language has no hidden
   allocations.
2. **Region attribution.** An `alloc` executed while a region is
   the innermost active region is owned by that region and freed
   automatically on region exit.
3. **Explicit free.** An `alloc` outside any region must be freed
   by an explicit `free(p)`. Failure to free is a memory leak,
   not a safety error.

The allocation model is described in
`docs/decisions/0004-memory-model.md` and
`docs/decisions/0007-region-memory.md`. This contract documents
the implementation.

## Syntax

Allocation:

```gol
val p := alloc(64)         // 64 bytes of uninitialized storage
```

Free:

```gol
free(p)                    // release the storage
```

Inside a region (preferred form):

```gol
region scratch
    val p := alloc(64)
    // ... use p ...
// p is freed automatically here
```

`alloc` is an **expression**, not a statement. `val p := alloc(64)`
is the standard form; `alloc(64)` alone at statement position is
legal but the returned pointer is discarded.

`free` is a **statement**. The result of `free(p)` is `Void` and
cannot be used in an expression.

## Typing rules

| Expression | Type |
|---|---|
| `alloc(n)` where `n: Int` | `*Unknown` |
| `free(p)` where `p: *T` or `p: *Unknown` | `Void` |
| `free(null)` | `Void` (no-op at runtime) |

The return type of `alloc` is `*Unknown` because the language has
no way to know what the caller will store there. A future extension
could allow `alloc<Int>(n)` to return `*Int`, but this is not
currently supported.

### Type of the size argument

`alloc` requires its argument to be `Int`. A `Float` argument is a
type error at the analyzer. The interpreter additionally rejects
negative or zero sizes with `EvalError::TypeMismatch` — see the
runtime behavior section below.

### Valid pointer types

The verifier enforces that `free` operates on a pointer-compatible
type (`Ptr`, `Pointer(_)`, or `NullPtr`). `verifier_rejects_free_on_non_pointer`
in `src/ir/verifier/tests.rs` pins this.

## Ownership

### Who owns a heap allocation

A pointer returned by `alloc` is **owned by the current scope**, but
the language does not enforce freeing. There is no `Drop`-like
mechanism for pointers. A pointer that is not freed is leaked.

However, a pointer allocated inside a region is owned by that
region, and the region frees it on exit. This is the preferred
allocation pattern: use regions to get automatic cleanup without
GC.

### Move semantics

A pointer is `Copy` (`Type::Ptr` is in the `is_copy` set), but a
**typed** pointer (`*T` / `Type::Pointer(_)`) is **not** `Copy`, per
the region-memory model. Copying a `*Unknown` pointer is fine;
copying a `*Int` pointer that is region-owned would duplicate region
ownership and is rejected by the analyzer.

This distinction is subtle and worth internalizing:

```gol
val p := alloc(64)         // p: *Unknown, Copy
val q := p                 // q is a copy; both refer to the same storage
free(p)                    // valid; q is now a dangling copy
free(q)                    // would be a double-free at runtime
```

The interpreter's `heap` model treats a double-free as a no-op
(`heap.remove(&h)` on an already-removed handle does nothing), so
this is not a runtime error in the interpreter. The LLVM backend
lowers to `free(p)` twice, which is undefined behavior in C. The
analyzer is the enforcement mechanism: it should reject the second
`free` if it can see the pointer was already freed.

### Interaction with regions

An `alloc` inside a region records its handle in the region's frame.
When the region exits, the interpreter iterates the frame's
allocations and removes each from the heap. Any explicit `free`
executed before the region exits removes the handle from the heap;
the region's later attempt to free it is a no-op (the handle is
gone).

This means **an explicit `free` inside a region is safe**:

```gol
region r
    val p := alloc(64)
    free(p)                // removes p from the heap
    // ... region exits, tries to free p again: no-op
```

There is no double-free error in this case. This is a deliberate
design choice: the region's cleanup is best-effort and does not
require the program to track which handles are still live.

## IR representation

In `src/ir/semantic_ir.rs`:

| Concept | Variant |
|---|---|
| `alloc(n)` | `Instruction::Allocate { target: String, size: TypedIRValue, type_: Type }` |
| `free(p)` | `Instruction::Free { ptr: TypedIRValue }` |

`Allocate` binds a fresh variable `target` to the returned pointer.
`size` is the size expression; the interpreter evaluates it to a
positive integer. `type_` is the declared type of the pointer
(`*Unknown` in the current implementation).

`Free` takes a `TypedIRValue` for the pointer, evaluates it, and
expects an integer handle. Non-integer operands are a runtime error
in the interpreter (`EvalError::TypeMismatch`) and an IR-level error
in the verifier (`verifier_rejects_free_on_non_pointer`).

### IR verifier rules

The verifier enforces:

- `Allocate { size, .. }` requires `size` to verify and to be `Int`.
- `Allocate` binds `target` to `Type::Ptr` or `Type::Pointer(_)`.
- `Free { ptr }` requires `ptr` to verify and to have a
  pointer-compatible type. `NullPtr` is accepted (free of null is a
  no-op); the verifier does not enforce that the pointer was
  obtained from `alloc`.

The verifier does **not** check:

- That `ptr` was actually produced by `alloc` (as opposed to being
  an uninitialized variable of pointer type).
- That `ptr` has not been freed already (this is the analyzer's
  job, and is not currently checked).
- That the total allocated size is within any limit.

## CFG representation

In `src/ir/cfg/builder.rs`, `Allocate` lowers to a `Declare`:

```rust
I::Allocate { target, .. } => instrs.push(CfgInstruction::Declare { name: target.clone() }),
```

This treats the returned pointer as a fresh variable in the dataflow
analysis. The pointer is `Available` after the alloc.

`Free` lowers to a `Use` followed by a `Move`:

```rust
I::Free { ptr } => {
    if let Some(var_name) = extract_var_name(ptr) {
        instrs.push(CfgInstruction::Use { name: var_name.clone() });
        instrs.push(CfgInstruction::Move { name: var_name });
    } else {
        instrs.push(CfgInstruction::Unsupported { op: format!("Free non-var {:?}", ptr) });
    }
}
```

The `Move` after `Use` means: the dataflow analysis treats `free(p)`
as if `p` were moved into the free call. After the `free`, `p` is
`Moved` and using it again is a `E-MOVE-001` error. This is how the
analyzer catches free-then-deref:

```gol
val p := alloc(64)
free(p)
val v := *p              // E-MOVE-001: use of moved 'p'
```

The adversarial test `24_free_then_deref.gol` exercises exactly this
case.

## Runtime behavior

### Interpreter

The interpreter uses a simulated heap in
`src/backends/interpreter/mod.rs`:

```rust
pub(super) heap: HashMap<usize, Vec<u8>>,
pub(super) next_ptr: usize,
```

`next_ptr` starts at 1 so that `0` means null. Each `Allocate`
increments `next_ptr` and inserts a zero-filled `Vec<u8>` into the
heap. The handle (an integer) is stored in the target variable as a
`RuntimeValue::Int`. Pointers are integers under the hood, but no
real memory is addressed — the interpreter is not a memory model,
it is a simulation of the allocation *interface*.

`Free` looks up the handle, removes it from the heap, and is a no-op
if the handle is not present. This makes double-free safe at the
interpreter level (the second free finds nothing to remove), which
is a departure from C semantics but consistent with the language's
region-cleanup model.

### Allocation size validation

The interpreter rejects non-positive sizes at runtime:

```rust
Instruction::Allocate { target, size, .. } => {
    let requested = match self.eval_value(size)? {
        RuntimeValue::Int(i) if i > 0 => i as usize,
        RuntimeValue::Float(f) if f > 0.0 => f as usize,
        other => {
            return Err(EvalError::TypeMismatch {
                op: "Allocate.size",
                left: runtime_kind(&other),
                right: "positive Int",
            });
        }
    };
    // ...
}
```

This is the fail-closed path added in the interpreter-totality work
this session. A negative or zero size is a runtime error, not a
silent allocation of an empty buffer.

### LLVM

The LLVM backend lowers `alloc(n)` to `malloc(n)` and `free(p)` to
`free(p)` (see `register_stdlib` in
`src/backends/llvm_codegen/builtins.rs`). No bounds checking, no
double-free detection, no region attribution — the raw C semantics.

This is the correct fail-closed behavior for the LLVM backend: the
language's safety guarantees are enforced by the analyzer, and the
LLVM codegen trusts that enforcement. If the analyzer passes a
program, the LLVM codegen will produce the corresponding C
`malloc`/`free` calls.

### WASM

Raw memory is supported in WASM (WASM has a linear memory model),
but I have not read the WASM lowering code for `alloc`/`free`.
The capability matrix presumably accepts raw memory on WASM; this
should be verified before treating WASM as a supported backend
for this feature.

## Diagnostics

Alloc/free-related error codes currently emitted:

| Code | Meaning | Emitted from |
|---|---|---|
| `E-MOVE-001` | Use of moved value | `dataflow.rs` — fires for use-after-free |
| `E-MOVE-002` | Move of borrowed value | `dataflow.rs` |
| — | Free on non-pointer | `verifier_rejects_free_on_non_pointer` (message text) |
| — | Allocation size not positive | `EvalError::TypeMismatch` (interpreter) |

**There is no `E-ALLOC-NNN` or `E-FREE-NNN` code.** This is the
same diagnostic gap as traits, generics, defer, spawn, and FFI.
Use-after-free is reported as a move error, which is technically
correct but loses the connection to `free`.

## Safety

- **Use-after-free**: caught by the analyzer via the `Move` after
  `Free` in the CFG. Reported as `E-MOVE-001`.
- **Double-free**: caught by the analyzer if it can see that the
  pointer was already moved. The interpreter treats a double-free
  as a no-op; the LLVM backend relies on the analyzer.
- **Null dereference**: caught by the analyzer if it can track that
  a pointer is `null`. `test_literal_null_deref_rejected` and
  `test_known_null_binding_deref_rejected` in `src/semantics/analyzer/`
  pin this.
- **Negative size**: rejected by the interpreter at runtime;
  whether the analyzer rejects it is unverified.
- **Huge size**: not checked. `alloc(1000000000)` will attempt a
  one-GB allocation. The LLVM backend will `malloc` it (may fail
  and return null; the language has no error return for `alloc`);
  the interpreter will allocate a `Vec<u8>` of that size (may OOM).
  Not currently a language-level concern.

## Test coverage

Current coverage across the tree:

**Interpreter (`src/backends/interpreter_backend.rs::tests`):**

- `test_interpreter_allocate_and_free` — basic alloc/free cycle
- `test_interpreter_region_frees_allocation_on_exit` — region-owned allocation freed on exit

**Runtime (`src/runtime/region_memory.rs::tests`):**

- `test_create_and_allocate`
- `test_double_create_fails`
- `test_allocate_in_freed_region_fails`
- `test_double_free_prevented`
- `test_child_region_management`

**IR verifier (`src/ir/verifier/tests.rs`):**

- `verifier_rejects_free_on_non_pointer`

**Analyzer (implicit):**

- Free-then-deref, null-deref cases are covered by the general move
  and null analyses.

**Adversarial (`tests/adversarial/`):**

- `23_null_pointer_deref.gol` — dereferencing null
- `24_free_then_deref.gol` — use-after-free
- `25_double_free.gol` — freeing twice
- `30_null_value_is_valid.gol` — passing null around is legal

**Capability tests (`src/backends/capabilities/tests.rs`):**

- `interpreter_accepts_raw_memory`
- `llvm_accepts_raw_memory`

**Conformance:**

- `tests/conformance/valid/arrays.gol` (uses alloc, presumably)

### Gaps

- **No test for `alloc(-1)` or `alloc(0)`** at the analyzer level.
  The interpreter rejects them at runtime; whether the analyzer
  rejects them at compile time is not pinned.
- **No test for `alloc` outside any region and never freed.**
  A leak test would be useful; the language currently has no leak
  detection.
- **No test for LLVM lowering of `alloc`/`free`.** The LLVM codegen
  registers `malloc` and `free`, but I have not seen a differential
  test that exercises them.
- **No test for WASM `alloc`/`free`.** The WASM backend may or may
  not lower them.
- **No test for `free(null)`.** The interpreter treats it as a
  no-op; whether the analyzer accepts `free(null)` at compile time
  is not covered.
- **No test for `free` of a pointer that was moved.** The analyzer
  catches use-after-free, but the specific case of a pointer moved
  to a function and freed inside that function is not tested.

## Maturity

Following the stages in `docs/architecture-direction.md`:

```
alloc / free
    semantics:   Stable
    parsed:      yes
    typed:       yes
    validated:   partial (size positivity not checked by analyzer)
    IR:          yes (Instruction::Allocate, Instruction::Free)
    verified:    yes (Free on non-pointer rejected)
    interpreter: supported (simulated heap)
    LLVM:        supported (malloc/free lowering)
    WASM:        unverified
    optimized:   N/A (side-effecting)
```

Alloc/free is one of the oldest features in the language and one of
the most stable. Its semantics have not changed recently, and its
test coverage is broad.

The main open item is the same diagnostic gap that affects traits,
generics, defer, spawn, and FFI: use-after-free is reported as
`E-MOVE-001` rather than a dedicated `E-FREE-001`. This is not a
correctness issue, only a diagnostics-quality issue.

## Checklist for related features

If you are adding a feature *like* `alloc`/`free` (a primitive with
an explicit call syntax and different behavior in regions vs.
outside them), you need to touch:

1. `src/frontend/parser/` — parse the call as an expression or
   statement depending on the primitive.
2. `src/ir/semantic_ir.rs` — a new `Instruction` variant.
3. `src/ir/verifier/instruction.rs` — verification rules for the
   new instruction.
4. `src/ir/cfg/builder.rs` — translation to `CfgInstruction`.
5. `src/ir/cfg/dataflow.rs` — any dataflow rules the primitive
   needs (typically the effect on the place it operates on).
6. `src/backends/interpreter/mod.rs` — the runtime behavior.
7. `src/backends/interpreter/runtime.rs` — a `RuntimeValue` variant
   if the primitive produces a non-integer handle.
8. `src/backends/llvm_codegen/builtins.rs` — the LLVM lowering,
   typically a call to a C library function.
9. `src/backends/capabilities/scan.rs` — declare backend support.
10. `src/backends/capabilities/tests.rs` — accept/reject per backend.
11. `tests/adversarial/` — negative cases.
12. `tests/conformance/valid/<feature>.gol`.
13. `docs/features/<feature>.md` — this file.

## Open questions

- **Should `alloc` be `unsafe`?** In some languages, explicit
  allocation is gated behind `unsafe`. In ALGOL26, it is a normal
  expression. This is defensible (the language already distinguishes
  region-owned from free-standing allocations), but it means a user
  can leak memory without any warning. A warning-only diagnostic
  for `alloc` outside a region might help.

- **Should there be an `alloc_zeroed(n)` or `alloc<T>(n)` form?**
  Not currently. Would be a small feature on top of the existing
  primitive.

- **How do we handle allocation failure?** `malloc` can return
  `null` under memory pressure. The LLVM lowering does not check
  this; a subsequent dereference is undefined behavior. The
  interpreter's heap model never fails to allocate. A future version
  might return `Result<*T, AllocError>` from `alloc`, but this is
  a language-level change and not currently planned.

- **Should `free` be a statement or an expression?** Currently a
  statement. Making it an expression (returning a `Result` or unit)
  would be a design change.

- **Should double-free be a compile error?** The analyzer catches
  some cases (via `Move` after `Free`), but not all. A more precise
  analysis would track pointer state (`Allocated` / `Freed`) and
  reject any use of a `Freed` pointer. Not currently implemented.

- **Should `alloc` inside a region be allowed to escape the region?**
  Currently, no — a pointer allocated in region `r` cannot be stored
  in a location that outlives `r`. This is enforced by the region
  escape analysis. A future extension could allow `leak(p)` to
  detach an allocation from its region and promote it to the caller's
  region, but this is not currently supported.

- **How does the runtime module `region_memory.rs` relate to the
  interpreter's heap model?** They are parallel implementations of
  region-aware allocation. The interpreter uses its own heap; the
  runtime module is unused. This is the same duplication noted in
  the region contract.

## See also

- `docs/architecture-direction.md` — the feature contract pattern itself
- `docs/features/region.md` — the region-based allocation model
- `docs/features/ffi.md` — how raw pointers cross the FFI boundary
- `docs/features/unsafe.md` — (to be written) the unsafe block
- `docs/decisions/0004-memory-model.md`
- `docs/decisions/0007-region-memory.md`
- `src/ir/semantic_ir.rs` — `Instruction::Allocate`, `Instruction::Free`
- `src/backends/interpreter/mod.rs` — the simulated heap
- `src/backends/llvm_codegen/builtins.rs` — `malloc`/`free` registration
- `src/runtime/region_memory.rs` — the parallel runtime module
- `tests/adversarial/24_free_then_deref.gol`
- `tests/adversarial/25_double_free.gol`
