# ALGOL26 Language Gaps

Discovered by the ChainCSS port attempt (`~/dev/web/chaincss/diff-corpus/`).

## G1 — `PassMetadata` index signature

TypeScript's `PassMetadata` has `[passNamespace: string]: unknown`.
ALGOL26 records cannot represent open-ended key sets. Needs a policy
decision before Phase 4.

**Status:** Deferred to Phase 4.

## G2 — `CSSValueNode` (CSS value AST)

`IRDeclarationMeta.ast` uses an 11-variant recursive union with its own
recursive-descent parser (`parseCSSValue`). A sub-compiler in its own
right.

**Status:** Deferred to Phase 2E or 3. Phase 2A represents it as
`Option[JsonValue]`.

## G3 — `Json` numeric tower

ALGOL26's `examples/json/json.gol` has `Num(Int)` only. ChainCSS IR
needs both integer and float values (specificity 0.1, timestamps,
dimensions like 1.5em). Fix: two-variant `Json` with `NumInt(Int)` and
`NumFloat(Float)`.

**Status:** Resolved by design decision. ~10 lines.

## G4 — `List.push` (Phase 2A-pre-1)

`List<T>` has no `push`, no concatenation, no preallocation. Building
runtime-length lists is impossible. JSON parsing cannot construct
`Json.Arr(...)`.

Template confirmed: `Map.insert` at `src/backends/interpreter/eval.rs:1372`
shows the mutating-builtin dispatch pattern. `Type::mut_borrow` exists
(`src/common/types.rs:413`). Adding `List.push` is ~40-80 lines including
tests.

**Status:** Phase 2A-pre-1. Est. 1 session.

## G5 — `Int.parse` / `Float.parse` (RESOLVED)

No `Int.parse` or `Float.parse` builtins. But `String.to_chars` returns
`List[Char]`, and `Char.to_int` returns the codepoint. Digit parsing is
a 10-line helper. Float parsing is ~80-120 lines. No language feature
needed.

**Status:** Resolved by existing primitives.

## G6 — `Map` capability constraints

`Map` is interpreter-only (LLVM/WASM refuse). Iteration order is
unspecified; current interpreter sorts `keys()`. JSON output order may
not be preserved byte-for-byte.

**Status:** Constraint. Phase 2 runs under `--interpreter`. Phase 2D
comparison must be structural, not byte-for-byte.

## G7 — `List` capability matrix

Interpreter: full. LLVM: partial. WASM: unverified. Same constraint as G6.

**Status:** Constraint. Interpreter-only for Phase 2.
