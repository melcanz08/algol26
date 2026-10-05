# ADR 0035 — Interpreter Write-Through for `&mut self` Receivers

## Status

Proposed. Not yet implemented.

> **Status note (2026-10-05).** This ADR is a direct follow-up to
> ADR 0033. That ADR shipped methods for non-generic user types and
> verified them on the interpreter, except for one case: a method
> with a `&mut self` receiver that writes through `self`. The
> interpreter saves and restores its variable frame around every
> call, so a `self.name := new_name` inside a method mutates a
> local copy and never reaches the caller's binding. Two conformance
> fixtures are parked: `method_mut_receiver.gol` and
> `method_matches_free_function.gol`.
>
> **Revision note (2026-10-05, pre-implementation).** The first
> draft of this ADR proposed a `RuntimeValue::Ref(name)` transient
> alias. Code inspection showed the interpreter is not a flat-map
> evaluator: `self.variables` is saved and restored around every
> call (`let saved_vars = std::mem::take(&mut self.variables)` in
> `eval.rs`). A `Ref(name)` created inside the callee would resolve
> in an empty frame. This revision replaces the `Ref` design with
> **copy-in / copy-out**, which fits the existing frame model and
> requires no new runtime value. See §Alternatives considered for
> the discarded draft.

## Context

ADR 0033 committed ALGOL26 to three receiver modes: `self: T`
(consume), `self: &T` (shared borrow), `self: &mut T` (exclusive
borrow). The first two work end-to-end on the interpreter today.
The third parses, type-checks, and lowers, but the write never
reaches the caller.

The IR the builder emits for `u.rename("Alicia")` is:

```
Call {
    function: "User_rename",
    args: [
        BorrowMutable { expr: Variable("u", User), target_type: MutBorrow(User) },
        String("Alicia"),
    ],
}
```

Inside the method body, `self.name := new_name` becomes a
`FieldAssign` whose target is `Variable("self", MutBorrow(User))`.

The interpreter's call dispatch (`eval.rs`, the `TypedIRValue::Call`
arm) does this:

```rust
let saved_vars = std::mem::take(&mut self.variables);
for (param_name, arg) in func.params.iter().zip(call.args.iter()) {
    let val = self.eval_value(arg)?;
    self.variables.insert(param_name.clone(), val);
}
self.execute_function(func)?;
self.variables = saved_vars;
```

Two properties follow:

1. The callee's frame is a fresh map containing only its own
   parameters and locals. The caller's bindings are stashed in
   `saved_vars` for the duration of the call.
2. Evaluating `BorrowMutable { expr: Variable("u") }` produces
   the receiver's *value* (a `RuntimeValue::Record`). The callee
   sees a copy. `FieldAssign` mutates the copy. On return the
   copy is discarded with the callee's frame.

Read-only mutating-method-free methods work because the pass-
through gives them the record's value, which is all they need for
reads. Mutating methods silently lose the write.

The full parity picture:

| Backend | Records | References | Methods |
|---|---|---|---|
| Interpreter | Yes | Read-only | Read-only |
| LLVM | No | Yes | Blocked on records |
| WASM | No | No | Blocked on both |

Adding LLVM record support is a separate multi-day project with
its own design problems (layout, codegen, verifier rules). WASM
record support is a third. Neither is required to make mutating
methods run today.

## Decision

The interpreter gains **copy-in / copy-out** for `&mut self`
receivers. Concretely:

### The call dispatcher records a write-back plan

Before evaluating arguments, the dispatcher scans the caller's
argument list. For each argument at position `i` whose IR shape is
`BorrowMutable { expr: Variable(name), .. }`, it records:

```
(i, name)
```

The index is into the callee's `params`; `name` is the caller's
binding.

This is a syntactic scan of the IR, not an evaluation. The
`BorrowMutable { expr: Variable(_) }` shape is exactly what
`expand_impl_methods` and the builder emit for `&mut self` today;
no IR change is needed to make the pattern recognizable.

### The callee runs unchanged

Parameters are inserted into a fresh frame as they are today. The
callee's `self` binding is a `RuntimeValue::Record` — a copy of
the caller's record, but the callee cannot observe the difference
because the frame is isolated. `self.name := new_name` mutates the
local record via the existing `FieldAssign` path.

### After the callee returns, write-backs are captured

Before `self.variables = saved_vars` restores the caller's frame,
the dispatcher iterates the write-back plan. For each `(i, name)`,
it looks up the callee's parameter name at position `i` in
`func.params`, reads the final value from `self.variables`, and
stores it in a local vector:

```
let mut write_backs: Vec<(String, RuntimeValue)> = Vec::new();
for (i, caller_name) in &plan {
    let param_name = &func.params[*i].0;
    if let Some(val) = self.variables.get(param_name).cloned() {
        write_backs.push((caller_name.clone(), val));
    }
}
```

### The caller's frame is restored, then write-backs applied

```
self.variables = saved_vars;
for (caller_name, val) in write_backs {
    self.variables.insert(caller_name, val);
}
```

Order matters. Capturing before restore means the callee's final
parameter values are read from the callee's frame. Applying after
restore means the caller's other bindings aren't visible to the
write-back and vice versa.

### `BorrowShared` is unchanged

Shared borrows never write through, so the existing pass-through
is correct. `BorrowShared { expr }` continues to evaluate to
`expr`'s value.

### `&mut expr` where `expr` is not a bare variable falls back

The write-back plan only fires on
`BorrowMutable { expr: Variable(name) }`. Any other shape —
`BorrowMutable { expr: FieldAccess { ... } }`, an array element, a
temporary — is not recorded, and the call behaves exactly as it
does today. Real support would need a place-expression model,
which is a larger problem than ADR 0033 was solving.

### No new `RuntimeValue` variant

The interpreter's value model is untouched. There is no `Ref`, no
`Rc<RefCell<...>>`, no interior mutability, and no change to how
variables are looked up. The only additions are a scan of the
call's argument list, a small local vector, and a write-back loop.

## Why copy-in / copy-out is correct here

The analyzer enforces exclusivity: at a `&mut self` call site, the
receiver is the only live reference to that variable, and the
borrow cannot escape the call. Under exclusivity, copy-in / copy-
out is **observationally identical** to real aliasing:

- The callee cannot observe the caller's binding while it runs,
  because the frame is isolated — the caller's other bindings are
  stashed in `saved_vars`, unreachable from the callee.
- The caller cannot observe the callee's binding during the call,
  because the call is synchronous.
- After the call, the write-back applies the callee's final value
  to the caller's binding atomically, which is exactly what a real
  pointer dereference would produce.

Real aliasing (a reference that survives the call) would only be
needed if references could escape. ADR 0035's scope excludes that,
and ADR 0005's borrow checker excludes it at compile time. Copy-
in / copy-out is not a compromise; it is the correct semantics for
the language as designed.

The cost is a copy of the receiver both ways. For records of
realistic size, the copy is negligible. For pathological cases it
is O(size), but so is any pass-by-value and the language does not
currently have huge records.

## Scope boundary

Explicitly **not** part of this ADR:

- **LLVM record support.** Separate project, separate ADR.
- **WASM record or reference support.** Same.
- **First-class references in user code.** `val r := &mut x`
  where `r` is used after the enclosing expression — the analyzer
  already rejects these, and the interpreter gains no new
  mechanism for them.
- **`&mut expr` where `expr` is not a bare variable.** Pass-
  through fallback preserves current behavior.
- **Interior mutability, reference counting, GC.** The
  interpreter remains value-semantic.
- **A frame-stack rewrite.** Moving the interpreter to a stack of
  `HashMap`s would enable real aliasing cheaply, but it touches
  every `self.variables.get(...)` call site (~30 today). Not worth
  it for this feature. See §Alternatives considered.

## What changes, layer by layer

### I1 — Locate the call dispatch arm

In `src/backends/interpreter/eval.rs`, find the `TypedIRValue::Call`
arm of `eval_value`. The save/restore block is around line 1063:

```rust
let saved_vars = std::mem::take(&mut self.variables);
// ... param insertion, execute_function, ...
self.variables = saved_vars;
```

The exact surrounding code needs reading; the ADR describes the
shape, not the byte-for-byte edit.

### I2 — Compute the write-back plan

Before `let saved_vars = ...`, scan `call.args` (or whatever the
argument list is called in the arm) and produce a vector:

```rust
let write_back_plan: Vec<(usize, String)> = call
    .args
    .iter()
    .enumerate()
    .filter_map(|(i, arg)| match arg {
        TypedIRValue::BorrowMutable { expr, .. } => match &**expr {
            TypedIRValue::Variable(name, _) => Some((i, name.clone())),
            _ => None,
        },
        _ => None,
    })
    .collect();
```

This is a pure syntactic scan. Nothing is evaluated.

### I3 — Capture write-backs after the callee returns

Immediately before `self.variables = saved_vars`, capture the
values to write back:

```rust
let mut write_backs: Vec<(String, RuntimeValue)> =
    Vec::with_capacity(write_back_plan.len());
for (i, caller_name) in &write_back_plan {
    if let Some((param_name, _)) = func.params.get(*i) {
        if let Some(val) = self.variables.get(param_name).cloned() {
            write_backs.push((caller_name.clone(), val));
        }
    }
}
```

### I4 — Apply write-backs after restore

After `self.variables = saved_vars`, apply:

```rust
for (caller_name, val) in write_backs {
    self.variables.insert(caller_name, val);
}
```

### I5 — Fixtures and docs

- Restore `tests/conformance/pending/method_mut_receiver.gol` to
  `tests/conformance/valid/methods/`.
- Create `tests/conformance/valid/methods/method_matches_free_
  function.gol` — the desugaring-equivalence fixture ADR 0033
  described. Two programs, identical output.
- Update `docs/features/methods.md`: move `&mut self` from
  §Known limitations to the supported column; the interpreter row
  of the backend table becomes `Yes` for all three receiver modes.
- Update ADR 0033's status note to point at this ADR for the
  write-through work.

## Consequences

### Positive

- The two backend-blocked method fixtures run end-to-end.
- The desugaring invariant — `u.rename("Alice")` produces
  identical behavior to `rename(&mut u, "Alice")` — becomes
  testable for the first time.
- Mutating methods become usable in real programs on the
  interpreter, closing the gap between "methods as a feature"
  and "methods as a tool."
- No new runtime value, no new capability, no IR or analyzer
  change. The change is contained to one arm of the interpreter's
  call dispatch.

### Negative

- The receiver is copied twice per `&mut self` call. For small
  records this is negligible; for large ones it's a real cost. No
  current programs hit this.
- The write-back is coupled to the syntactic shape
  `BorrowMutable { expr: Variable(_) }`. If a future IR change
  alters that shape, this code silently stops working. Mitigation:
  a debug assertion or a comment pointing at the builder's
  receiver-wrapping site, which is where the shape is created.
- The interpreter still isn't a general reference model. Its
  semantics are "value semantics plus a narrow `&mut self` write-
  back." Documentation needs to say so, but this is much smaller
  than "value semantics except for `Ref`."

### Neutral

- The interpreter and LLVM continue to disagree on records. This
  ADR does not close that gap; it just closes the smaller
  interpreter-side gap that the method feature made visible.

## Implementation order

Three commits, each compiling and revertable:

```
I1. Add the write-back plan scan (I2 above) as a no-op — compute
    it, print it under a debug flag, don't apply. Confirms the IR
    shape is what the ADR expects on real programs.

I2. Capture write-backs after the callee returns and apply them
    after restore (I3, I4). Restore the fixture. This is the
    commit that makes the feature work.

I3. Fixtures, docs, ADR cross-references (I5).
```

Splitting I1 out is optional but useful: it verifies the scan
without risking a silent write-back bug on the first try.

## Alternatives considered

### Draft: `RuntimeValue::Ref(name)` transient alias

**Rejected.** The first draft of this ADR proposed adding a
`RuntimeValue::Ref(String)` variant, evaluating `BorrowMutable`
of a variable to a `Ref`, and having reads resolve `Ref` by name.
This assumed `self.variables` was a flat map across all calls.
Code inspection showed it is not: the dispatcher stashes the
caller's map in `saved_vars` and installs a fresh empty map for
the callee. A `Ref("u")` created inside the callee would resolve
in an empty frame. The `Ref` design cannot work without also
converting the interpreter to a frame stack (see next).

### Convert the interpreter to a frame stack

**Rejected for this ADR.** A `Vec<HashMap<String, RuntimeValue>>`
with innermost-to-outermost lookup would enable real aliasing with
minimal semantic change. But it touches every `self.variables`
access — ~30 sites across `eval.rs` and `mod.rs`, plus any future
code — and it changes the interpreter's variable-resolution
discipline globally for a feature that needs it in one narrow
case. The copy-in / copy-out change touches one arm of the call
dispatcher. If a future feature needs real references in user
code, the frame-stack rewrite becomes justified; today it isn't.

### Add LLVM record support instead

**Rejected for this ADR.** LLVM is the more architecturally
correct place for references to work — it has real memory
addresses, so `&mut self` maps to a pointer dereference with no
runtime trick. But it's a multi-day project: record layout,
literal codegen, field access/assign, verifier rules, capability
updates. The interpreter path is 2–4 hours. Both eventually land;
this ADR is the smaller one, not the cleaner one.

### Interior mutability (`Rc<RefCell<...>>` for every record)

**Rejected.** Would require threading shared-mutable state through
every record in the interpreter. Two mutable aliases could then
exist at runtime even though the analyzer forbids them — the
runtime would silently permit what the language promises to
reject. Copy-in / copy-out preserves the analyzer's exclusivity
guarantee: at any moment, only one binding exists.

### Do nothing; document the limitation permanently

**Rejected.** Mutating methods are a normal expectation for a
language that has methods. Leaving them permanently unexecutable
on the semantic oracle is a real gap, not a stylistic choice.
The interpreter is what makes the analyzer's promises verifiable.

## Review record

**Pre-implementation revision (2026-10-05).** The initial draft
proposed `RuntimeValue::Ref(name)`. A code inspection of the
interpreter's call dispatch revealed that `self.variables` is
saved and restored around every call, which invalidates the `Ref`
design's central assumption. The revision replaced the mechanism
with copy-in / copy-out, which requires no runtime-value change
and fits the existing frame model. The discarded draft is
preserved in §Alternatives considered.

## References

- ADR 0005 (Ownership Model) — the three receiver modes and the
  exclusivity guarantee this ADR relies on.
- ADR 0018 (Canonical Pipeline) — the interpreter is the semantic
  oracle; keeping it in sync with the analyzer's promises is why
  this ADR exists.
- ADR 0033 (Methods, Receivers, and the OOP Direction) — the
  feature this ADR completes on the interpreter.
- `src/backends/interpreter/eval.rs` — the call dispatch arm the
  write-back hooks into.
- `src/backends/interpreter/mod.rs` — `execute_function`, where
  the callee body runs.
- `tests/conformance/pending/method_mut_receiver.gol` — the
  fixture this ADR makes runnable.

## See also

- `docs/features/methods.md` — the feature contract; §Known
  limitations currently lists `&mut self` write-through as
  backend-blocked.
- `docs/decisions/0034-generic-impls.md` — the other follow-up
  ADR from ADR 0033. Independent of this one; either can land
  first.
- `docs/decisions/README.md` — the ADR index and convention.