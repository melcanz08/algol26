# ADR 0064: Channels — Design and Backend Lowering

Status: Proposed (2026-11-12). Extends ADR 0008.

## Context

ADR 0008 (Concurrency Model, 2026-09-16) established message
passing over channels as the primary concurrency mechanism. The
language has full frontend support:

- `Stmt::ChannelDecl { name, span }` — `channel ch`
- `Stmt::Send { channel, value, span }` — `send ch, v`
- `Stmt::Receive { channel, target, span }` — `receive ch -> x`
- `Instruction::ChannelDecl { name, type_ }`
- `Instruction::SendChannel { channel, value }`
- `Instruction::ReceiveChannel { channel, target }`

The interpreter's `mod.rs` handles these three instructions by
returning an error (line ~642-654): channels are explicitly not
modeled by any backend. `Feature::Channels` is therefore absent
from every backend's capability set — the interpreter refuses
them at the semantic layer, LLVM and WASM refuse them at the
capability boundary.

This means channels are currently a parsed-and-analyzed feature
with no runtime anywhere. No program that uses channels can run.

## Decision

Channels become an interpreter-first feature. The interpreter
implements synchronous channels in a single-threaded execution
model. LLVM and WASM continue to refuse channels until real
concurrency lands (ADR 0065).

The channel type is written `Channel<T>`. Its operations are:

| Operation | Signature | Semantics |
|-----------|-----------|-----------|
| Declaration | `channel ch: Channel<T>` | Allocates an empty queue |
| Send | `send ch, v` | Enqueues `v`, transferring ownership |
| Receive | `receive ch -> x` | Blocks until an element is available; binds to `x` |

The scope of this ADR is limited to **declaration, send, receive**.
Additional operations (`close`, `try_receive`, `select`) are
deferred to a follow-up ADR after the core three have shipped.

## Execution model — interpreter

The interpreter is single-threaded. `spawn` and `parallel` run
sequentially (see ADR 0011's findings on the `pending_forks`
worklist). In this model, a channel is a simple FIFO queue:

```
Interpreter state:
  channels: HashMap<String, VecDeque<RuntimeValue>>

ChannelDecl { name, _ } =>
  channels.insert(name, VecDeque::new())

SendChannel { channel, value } =>
  let v = eval(value)?;
  channels[channel].push_back(v);

ReceiveChannel { channel, target } =>
  let v = channels[channel].pop_front()
      .ok_or(RuntimeError("receive on empty channel"))?;
  bind target := v;
```

**Receive blocks if the channel is empty.** In a single-threaded
interpreter, there is no other thread to fill it — so
`receive` on an empty channel is a **runtime error**, not a
block. This is deliberate. The alternative (blocking forever)
hangs the interpreter. The alternative-alternative (returning
`Option::None`) changes the surface syntax (would need `try`
or `?`). A runtime error is honest: the program is
deadlocked, and the interpreter reports it.

This means channels are only useful in the interpreter when
paired with `spawn` that *runs before* the receive. Since
`spawn` runs synchronously in the interpreter, a valid pattern
is:

```gol
channel ch: Channel<Int>
spawn do
    send ch, 42
receive ch -> x
print(Int.to_string(x))
```

Execution: `spawn`'s body runs to completion first (enqueuing
42), then `receive` pops it. Correct.

The pattern that fails:

```gol
channel ch: Channel<Int>
receive ch -> x    // runtime error: empty channel
spawn do
    send ch, 42
```

This is consistent with synchronous `spawn`. A future ADR
(0065) decides whether `spawn` should be truly concurrent in
some backend; if so, `receive` blocking becomes meaningful.

## Ownership and safety

Sending transfers ownership of the value to the channel. After
`send ch, v`, the sender cannot use `v` — this is caught by the
existing move analysis (`use_after_move` diagnostics). Receiving
transfers ownership back to the receiver.

The analyzer's rules:

1. `send ch, v` **moves** `v`. If `v` is not `Copy`, its
   binding is marked moved after the send.
2. `receive ch -> x` **binds** a fresh variable `x` of type
   `T`. The channel's type parameter determines `x`'s type.
3. Channels cannot be shared across `spawn` boundaries unless
   the channel itself is passed. `channel` declarations are
   function-scoped. A nested `spawn` that captures the channel
   by name is subject to the existing race detector
   (`src/semantics/race/`).

Rule 3 requires care. Because channels are the *mechanism* of
safe concurrency, capturing a channel in two `spawn` blocks
should be **allowed** (unlike capturing a `var`). The race
detector must distinguish "captured for send/receive" from
"captured for direct read/write." This is a change to
`race/analyze.rs` and is scoped below.

## Backend status

| Backend | Status | Notes |
|---------|--------|-------|
| Interpreter | **Implements** | FIFO queue, blocking receive is a runtime error on empty |
| LLVM | **Refuses** | Deferred to ADR 0065 (real threading) |
| WASM | **Refuses** | WebWorkers + SharedArrayBuffer required; permanent refusal likely |

## Implementation roadmap

Three sessions, each self-contained. Each ends green and is
committed independently.

### Session 1 — Interpreter channel runtime

Scope:

- `src/backends/interpreter/mod.rs`: implement the three
  instruction handlers. `channels: HashMap<String, VecDeque>`
  field on the interpreter.
- `ChannelDecl` allocates the queue.
- `SendChannel` evaluates and pushes.
- `ReceiveChannel` pops, binds, or errors on empty.
- Capability: `Feature::Channels` joins `interpreter()`.
- Tests: three positive fixtures (declare/send/receive,
  send-then-receive in spawn, two channels). One negative
  (receive on empty is a runtime error).
- Update `tests/cap_impl/expected_matrix.json`,
  `tests/coverage_matrix.rs`, `tests/coverage_maturity.rs`.

Effort: ~2 hours. Unblocks nothing on LLVM/WASM.

### Session 2 — Race detector channel awareness

Scope:

- `src/semantics/race/analyze.rs`: a channel capture for
  `send`/`receive` does not count as a conflicting access.
  Two `spawn` blocks both sending on `ch` should be accepted.
  Two `spawn` blocks reading `var x` should still be rejected.
- Tests: two new positive cases (`two_spawn_send_ok`,
  `two_spawn_receive_ok`) plus a negative case that still
  rejects shared mutable capture.

Effort: ~1 hour. Depends on Session 1.

### Session 3 — LLVM lowering investigation

Scope: **investigation only**. The session's deliverable is a
written report appended to ADR 0065, not code.

Questions to answer:

1. Does the runtime support `std::thread`? (The current link
   step uses `cc`; pthreads would be a new `-lpthread` flag
   in `src/toolchain/linker.rs`.)
2. What is the C runtime API for a channel? Suggested shape:
   `algol26_channel_new(elem_size)`,
   `algol26_channel_send(ch, void *elem)`,
   `algol26_channel_recv(ch, void *out)`.
   Backed by a pthread mutex + condition variable.
3. What does soundness look like with two real threads? The
   race detector is compile-time only; does it catch
   everything? If not, a documented 
   `unsafe { }` boundary around channel operations might be
   the honest answer.

Effort: ~2 hours. Output is a report, not code.

## Alternatives considered

### Alternative 1 — Implement channels directly on LLVM/WASM

Skip the interpreter. Add the pthread-backed C runtime first,
then flip the LLVM capability. Rejected because:

- The interpreter is the semantic reference. Any feature that
  doesn't run there can't be tested against the expected
  semantics.
- Real concurrency requires a threading model. That model
  is ADR 0065's subject; channels depend on it.

### Alternative 2 — Non-blocking `try_receive` only

Make `receive` return `Option<T>` and never block. Rejected
because:

- The surface syntax `receive ch -> x` (per ADR 0008) does
  not show an Option. Changing it is a language change.
- Blocking receive is the standard channel semantic. Some
  future program will need it.

### Alternative 3 — Unbounded vs bounded channels

Bounded channels (`channel ch: Channel<T> of capacity 16`)
add a `send` block when full. Rejected for now:

- No current program needs boundedness.
- The unbounded FIFO is correct for the interpreter's
  single-threaded model (sends never block).
- Bounded channels can be a follow-up if a use case appears.

## Open questions

1. **`close` semantics.** When a channel is closed,
   `receive` should return an error or `Option::None`.
   Decided in a follow-up ADR after sessions 1–3 land.
2. **`select` over multiple channels.** The idiomatic
   Go-style construct. Requires a new statement form
   (`select { case recv ch1 -> x; case recv ch2 -> y }`).
   Out of scope for ADR 0064.
3. **Channel type parameter.** `Channel<T>` — but the
   frontend parses `channel ch` with no type annotation.
   The analyzer infers `T` from the first `send`. This
   matches the language's general inference style but means
   an unused channel has type `Channel<Unknown>`.
   Confirm the analyzer's behavior during Session 1.

## See also

- `docs/decisions/0008-concurrency-model.md` — the parent ADR
- `docs/decisions/0065-spawn-fork.md` — the task model
- `docs/decisions/0011-phase4-task-model.md` — `Fork` shape
- `src/frontend/ast.rs` — `Stmt::ChannelDecl` / `Send` / `Receive`
- `src/ir/semantic_ir/instructions.rs` — `ChannelDecl` etc.
- `src/backends/interpreter/mod.rs:642` — current refusal