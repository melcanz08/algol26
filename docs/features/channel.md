# Feature: Channel<T>

> Per-feature contract, following the pattern described in `docs/architecture-direction.md`.
> This file is the authoritative answer to "what is `Channel<T>` in ALGOL26, and where does it live?"

## Summary

`Channel<T>` is ALGOL26's typed message-passing primitive between
concurrent tasks. Values sent on a channel are moved (or copied, if
`T: Copy`) to the receiving task. Channels are the only sanctioned way
for two `spawn` blocks to exchange non-trivial state.

There are two producer-consumer forms in the language:

1. `channel ch: Channel<T>` declares a channel.
2. `send ch, value` sends; `receive ch -> v` receives.

Channels are linear: a channel value is moved when passed to a spawned
task, and sending on a moved channel is a compile error.

## Syntax

Declaration:

```gol
val ch: Channel<Int> := channel
val string_ch: Channel<String> := channel
```

Send:

```gol
send ch, 42
send string_ch, "hello"
```

Receive:

```gol
val v := receive ch
```

Pattern (receive into a binding):

```gol
receive ch -> v
print(v)
```

A minimal two-task example:

```gol
procedure main
    val ch: Channel<Int> := channel
    spawn
        send ch, 42
    val got := receive ch
    print(got)
```

The above is the pattern in `tests/corpus/corpus_26_spawn_channel.gol`.

## Typing rules

| Expression | Type |
|---|---|
| `channel` in a context requiring `Channel<T>` | `Channel<T>` |
| `send ch, v` where `ch: Channel<T>`, `v: T` | `Void` |
| `receive ch` where `ch: Channel<T>` | `T` |

`Channel<T>` is **invariant** in `T`. There is no covariance:
`Channel<Int>` does not coerce to `Channel<Float>`. This is deliberate
— a channel is a communication endpoint, and coercing the element type
would change the wire format at runtime.

Location in the type system: `src/common/types.rs`, variant
`Type::Channel(Box<Type>)`.
Constructor: `Type::channel(element_type)`.

Parsing: `Type::from_str` accepts both `Channel<T>` and `channel[T]`
syntax.

## Ownership

Channels are **non-`Copy`** in the general case. The rules:

1. **Declaration.** `val ch: Channel<T> := channel` binds `ch` in the
   enclosing scope.

2. **Move into spawn.** When a `spawn` block captures `ch`, the channel
   is moved into the spawned task. The parent cannot send or receive
   on `ch` afterward.

3. **Send semantics.** `send ch, v` moves `v` into the channel if `T` is
   non-`Copy`, or copies it if `T: Copy`. The ownership analyzer tracks
   this as a `Move` on `v`.

4. **Receive semantics.** `receive ch -> v` binds a fresh `v` of type
   `T` in the current scope. If `T` is non-`Copy`, `v` owns the
   received value.

5. **Channel lifetime.** A channel lives as long as any task holds a
   reference to it. Sending on a channel with no living receiver is a
   runtime deadlock, not a compile error — the analyzer cannot in
   general prove a receiver will run.

The ownership model uses `Escape` on channel sends to record that a
value has left the current task's ownership domain:

```
Instruction::Send { channel, value } -> CfgInstruction::Escape { from: value, to: "channel" }
```

This is what makes the analyzer refuse `send ch, &x` where `x` is a
local reference — the escape analysis (ADR 0007, region memory) marks
the reference as escaping via the channel and requires it to outlive
the channel's scope.

## IR representation

In `src/ir/semantic_ir.rs`:

| Concept | Variant |
|---|---|
| Channel declaration | `Instruction::ChannelDecl { name, type_ }` |
| Send (long form) | `Instruction::ChannelSend { channel, value }` |
| Send (short form) | `Instruction::Send { channel, value }` |
| Receive (long form) | `Instruction::ChannelReceive { channel, target }` |
| Receive (short form) | `Instruction::Receive { channel, target }` |

**The four send/receive instructions are duplicates.** `Send` and
`ChannelSend` have identical fields; same for `Receive` and
`ChannelReceive`. The IR carries both because two code paths produce
them. This is a Tier 7 (canonical IR) cleanup item — one pair should
be deleted and the producer updated.

### IR verifier rules

The verifier enforces:

- `ChannelDecl { name, type_ }` requires `type_` to be a `Channel<T>`.
- `Send` / `ChannelSend` require `channel` to have type `Channel<T>`
  and `value` to coerce to `T`.
- `Receive` / `ChannelReceive` require `channel` to have type
  `Channel<T>`; the target is bound to type `T` in the current block.
- `verifier_rejects_receive_on_non_channel` in `src/ir/verifier/tests.rs`
  confirms the non-channel case is rejected.

## CFG representation

The CFG builder lowers channel sends to a `Use` plus an `Escape`:

```
Instruction::ChannelSend { channel, value }
  -> CfgInstruction::Use { name: value }
  -> CfgInstruction::Escape { from: value, to: "channel" }
  -> CfgInstruction::Use { name: channel }
```

The `Escape` instruction is what the dataflow engine turns into an
`E-ESCAPE-002` diagnostic if the escaped value is a reference whose
lifetime does not outlive the channel.

See `src/ir/cfg/builder.rs` for the translation, and
`src/ir/cfg/dataflow.rs` for the escape check.

## Backends

| Backend | Support | Evidence |
|---|---|---|
| Interpreter | **Partial** — declaration, send, receive all no-op | See below |
| LLVM | **Unsupported** | No lowering; capability check refuses |
| WASM | **Unsupported** | `wasm_rejects_channels` in `src/backends/capabilities/tests.rs` |

### Interpreter behavior

The interpreter's module doc says:

> Not supported:
> - channel send/receive (no-op instructions).

In `src/backends/interpreter/mod.rs`, the channel instruction arms are:

```rust
Instruction::ChannelDecl { .. } => {}
Instruction::Send { .. } => {}
Instruction::Receive { .. } => {}
Instruction::ChannelSend { .. } => {}
Instruction::ChannelReceive { .. } => {}
```

**These are silent no-ops, not fail-closed errors.** A program that
sends on a channel and then receives will receive `Void` (or whatever
the target's default is), not the sent value. This is a Tier 2
(fail-closed) gap: the interpreter should return
`EvalError::Unsupported` for these instructions until a real queue
implementation exists.

The corpus tests `corpus_23_channel_int.gol`,
`corpus_24_channel_string.gol`, `corpus_25_two_channels.gol`,
`corpus_26_spawn_channel.gol` currently pass — because they only
exercise the *analyzer* (which accepts the channel constructs) and
not the *runtime semantics* (which would fail if the interpreter had
a real queue). Once the interpreter's no-ops become errors, those
corpus tests will need to be re-examined.

This is the most important open issue in this contract. The channel
feature is Stable in the analyzer and partial in the runtime, and
the gap is silent.

### LLVM and WASM refusal

Both backends correctly refuse channel-using programs via the
capability check. There is no partial lowering. A program that uses
channels will not compile to LLVM or WASM.

This is the correct fail-closed behavior. It means `Channel<T>` is
a language feature with a single fully-supported backend (once the
interpreter implements a real queue) and two explicit refusals.

## Safety

- **Data races.** Two tasks cannot share mutable state directly;
  they must exchange values through a channel. The race detector
  (`src/semantics/race/`) enforces this: a `spawn` that reads a
  `var` in the parent scope produces a conservative warning.
- **Reference escape.** A reference sent on a channel is checked for
  escape (`E-ESCAPE-002`). A reference whose lifetime does not outlive
  the channel is rejected.
- **Channel move.** Once a channel is moved into a `spawn`, the
  parent cannot use it. This prevents a class of use-after-move bugs
  in a concurrent context.
- **No panics.** The language has no `.unwrap()` on channels; a
  receive that would block forever is a runtime deadlock, not a
  panic. Deadlock detection is not implemented (see Open Questions).

## Optimizer rules

None implemented. Channels are side-effecting by definition; no
optimizer rule should remove a `Send` or `Receive` even if its result
is unused.

## Test coverage

Current coverage across the tree:

- `tests/corpus/corpus_23_channel_int.gol` — Int channel
- `tests/corpus/corpus_24_channel_string.gol` — String channel
- `tests/corpus/corpus_25_two_channels.gol` — multiple channels
- `tests/corpus/corpus_26_spawn_channel.gol` — channel across spawn
- `src/ir/verifier/tests.rs`: `verifier_accepts_channel_send_receive`,
  `verifier_rejects_receive_on_non_channel`
- `src/backends/capabilities/tests.rs`: `wasm_rejects_channels`
- `src/semantics/race/`: `test_read_write_race_detected`,
  `test_write_write_race_detected`, `test_var_read_during_spawn_is_conservatively_flagged`

### Gaps

- **No test that exercises channel semantics at runtime.** The
  interpreter treats sends and receives as no-ops, so any such test
  would currently fail unless it expected no-op behavior.
- No test for channel move into spawn.
- No test for `send` on a channel with a `&`-reference value
  (the `E-ESCAPE-002` path).
- No test for sending a non-`Copy` value (`String`) and receiving it.
- No test for channel declaration without use.
- No per-backend fixture under `tests/conformance/`.

## Maturity

Following the stages in `docs/architecture-direction.md`:

```
Channel<T>
    semantics:   Stable (analyzer)
    parsed:      yes
    typed:       yes
    validated:   yes (escape analysis on sends)
    IR:          yes
    verified:    yes
    interpreter: partial (no-op; not fail-closed)
    LLVM:        unsupported (correct refusal)
    WASM:        unsupported (correct refusal)
    optimized:   no rules
```

The maturity table has an unusual shape for this feature: the
*analysis* is Stable, one *backend* is partial (interpreter no-op),
and two backends are explicitly unsupported. This is not a problem
in itself — the problem is the partial interpreter, because silent
no-op is worse than explicit refusal.

## Checklist for related features

If you are adding a feature *like* `Channel<T>` (a typed
message-passing primitive with cross-task lifetime tracking), you
need to touch:

1. `src/common/types.rs` — new `Type` variant + constructor + parsing
   + `inner_type` + `Display`.
2. `src/ir/semantic_ir.rs` — new `Instruction` variants.
3. `src/ir/verifier/` — rules for the new instructions.
4. `src/ir/cfg/builder.rs` — translation to `CfgInstruction`,
   including any `Escape` or `Use` semantics.
5. `src/ir/cfg/dataflow.rs` — any new diagnostic codes.
6. `src/semantics/analyzer/` — type checking and ownership rules.
7. `src/semantics/race/` — race detector rules for the new
   cross-task interaction.
8. `src/backends/interpreter/mod.rs` — instruction handling (or a
   fail-closed refusal).
9. `src/backends/interpreter/runtime.rs` — a `RuntimeValue` variant
   if the feature produces values.
10. `src/backends/capabilities/scan.rs` — declare backend support.
11. `src/backends/capabilities/tests.rs` — accept/reject per backend.
12. `tests/corpus/` — end-to-end program exercising the feature.
13. `tests/conformance/valid/<feature>.gol`.
14. `docs/features/<feature>.md` — this file.

## Open questions

- **Should the interpreter implement real channel semantics?** Yes,
  but this requires modeling blocking, which the tree-walking
  interpreter does not currently do. A simple queue plus a
  "receive blocks until non-empty" rule works for a single-threaded
  interpreter but is not faithful to the language's concurrency
  model. This is the biggest open item on the feature.

- **Should the interpreter fail closed on channels now?** Yes, as a
  Tier 2 stopgap. Returning `EvalError::Unsupported` for channel
  instructions is a five-line change. It will break the four corpus
  tests above; those tests should be moved to a `known_unsupported/`
  directory or marked with a `// BACKEND: interpreter-unsupported`
  comment until a real queue exists.

- **Should there be a `select` / `recv_timeout` construct?** Not
  currently. Would be a new feature contract describing how it
  interacts with `defer` and the ownership model.

- **Should channels be `Send`/`Sync`-typed?** ALGOL26 has no trait
  system for concurrency marker traits. The race detector is a
  conservative approximation. A more precise system would need
  a `Send` bound on the element type `T`, but this is not currently
  expressible.

- **Should a channel be closed?** No `close(ch)` operation exists.
  Closing is a common source of bugs in other languages; whether
  ALGOL26 wants it is a design question.

## See also

- `docs/architecture-direction.md` — the feature contract pattern itself
- `docs/features/option.md` — the sibling feature contract example
- `docs/decisions/0008-concurrency-model.md` — `spawn` and `parallel`
- `docs/decisions/0007-region-memory.md` — escape analysis context
- `src/common/types.rs` — `Type::Channel`
- `src/ir/semantic_ir.rs` — channel instructions
- `src/ir/cfg/builder.rs` — `Escape` lowering
- `tests/corpus/corpus_26_spawn_channel.gol`
- `src/semantics/race/` — race detector
