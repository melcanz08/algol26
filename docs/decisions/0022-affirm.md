# ADR 0022: `affirm` — always-on runtime assertion

Status: Accepted

## Context

ALGOL26 has no runtime assertion construct. Programs that want to
check a boolean and stop on failure write their own `if` chain and
call `exit(1)` (LLVM) or return an error (interpreter) — no
single construct, no consistent diagnostic.

## Decision

Add a builtin function:

    affirm(cond: Bool, msg: String) -> Void

Semantics:

- `cond == true`: evaluation continues, no output.
- `cond == false`: the message is printed to stderr with the
  prefix `assertion failed: `, then the program terminates with a
  non-zero exit code.

### Why `affirm` and not `assert`

`assert` carries "debug-only, compiled out in release" baggage in
C++ (`NDEBUG`) and Python (`-O`). ALGOL26's check is always-on.
The name `affirm` avoids that misreading while remaining
semantically adjacent to `assert`.

`require` and `ensure` were rejected: they imply contract-style
obligations (Eiffel), which ALGOL26 does not enforce.

### Why a builtin and not a keyword

A keyword form (`affirm cond, "msg"`) requires a new token, a new
AST node, a new `Stmt` variant, and a new `Instruction` variant.
The builtin form reuses the existing call path: `FunctionCall` →
`Instruction::Call` → both backends dispatch through their
existing builtin tables.

### Why terminate, not return

A failed assertion is unrecoverable. The interpreter returns
`EvalError::Runtime` and the CLI exits non-zero; the LLVM backend
calls `exit(1)` directly. Neither path attempts to unwind, and
neither is catchable from the language.

## Backends

| Backend | Support |
|---|---|
| Interpreter | Yes — `eval_builtin_call` returns `Err(EvalError::Runtime)` on false |
| LLVM | Yes — conditional branch to a fail block emitting `printf` + `exit(1)` |
| WASM | Shares LLVM's codegen; works |

No capability entry: both backends support `affirm`, and it is
not target-specific.

## Consequences

**Positive.** A standard runtime check exists. The failed-assertion
message is consistent across backends. Programs stop trying to
hand-roll assertion-like patterns.

**Negative.** Adds one more name to the builtin namespace. In a
future where ALGOL26 grows a `panic`/`fail` construct for
user-facing errors, `affirm` must be documented distinctly — it is
for programmer invariants, not user errors.

## Tests

- `test_affirm_passing_continues` — interpreter, `affirm(true, ...)`,
  program runs to completion.
- `test_affirm_failing_errors` — interpreter, `affirm(false, ...)`,
  error message contains the user text.
- `test_affirm_lowers_to_llvm` — LLVM backend compiles a program
  containing `affirm` without error.