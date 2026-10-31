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

## G4 — `List.push` (FALSE ALARM — resolved)

**Correction.** Earlier assessment was wrong. `List.append(x)` exists
and works: `docs/decisions/0028-list-append.md` (status: Current),
dispatch arm at `src/backends/interpreter/eval.rs:1500`, corpus test
at `tests/corpus/corpus_42_list_append_llvm.gol`.

`List.append` is method-form, mutating, requires `var`, returns `Void`,
interpreter-only (LLVM/WASM refuse via `Feature::ListAppend`). It does
not appear in `src/builtins.rs` because it is dispatched directly in
`eval_builtin_call`, same pattern as `Map.insert` — which is why the
initial grep missed it.

**Status:** Resolved. No language work needed. Phase 2A-pre-1 removed.

## G5 — `String.to_int` / `Float.parse` (FALSE ALARM — resolved)

**Correction.** `String.to_int` exists (`src/builtins.rs` and dispatch
at `src/backends/interpreter/eval.rs:~1500`). Returns `Option[Int]`.
No `Float.parse` equivalent, but that is a minor hand-rolling cost, not
a blocker. The earlier assessment missed `String.to_int` by grepping
for `Int.parse` instead of checking the string conversion builtins.

**Status:** Resolved. `String.to_int` covers integer parsing.
Float parsing hand-rolled in the JSON parser.

## G6 — `Map` capability constraints

`Map` is interpreter-only (LLVM/WASM refuse). Iteration order is
unspecified; current interpreter sorts `keys()`. JSON output order may
not be preserved byte-for-byte.

**Status:** Constraint. Phase 2 runs under `--interpreter`. Phase 2D
comparison must be structural, not byte-for-byte.

## G7 — `List` capability matrix

Interpreter: full. LLVM: partial. WASM: unverified. Same constraint as G6.

**Status:** Constraint. Interpreter-only for Phase 2.

---

## Note on the G4 error (2026-10-31)

G4 was recorded as a hard blocker before checking `docs/decisions/`,
`docs/features/`, and `tests/corpus/` for existing implementations.
Three files were already on disk answering the question:

- `docs/decisions/0028-list-append.md`
- `tests/corpus/corpus_42_list_append_llvm.gol`
- `docs/features/list.md` (incomplete — did not list `append`)

Correction: **check existing ADRs, tests, and feature docs before
declaring a language gap.** A gap that already has a landed ADR is
not a gap. Future entries in this file should cite what was checked,
not just what was found missing.

## Methodology (added 2026-10-31, after G4 and G5 false alarms)

Two consecutive gaps (G4, G5) were recorded as blockers when the
feature already existed under a different name:

- **G4** — recorded as `List.push` missing. `List.append` exists (ADR 0028).
- **G5** — recorded as `Int.parse` missing. `String.to_int` exists (builtins.rs).

Both were found by *reading*, not grepping. The check that would have
caught them:

1. `docs/decisions/*.md` — is there a landed ADR?
2. `docs/features/*.md` — is the feature documented?
3. `tests/corpus/*.gol` — is there a test?
4. `src/builtins.rs` **and** `src/backends/interpreter/eval.rs` —
   grep for the *concept*, not the exact name. Methods dispatched
   directly in `eval_builtin_call` do not appear in `builtins.rs`.
5. `docs/features/list.md`, `string.md`, `map.md`, etc. — the
   feature docs may be incomplete; do not trust them as authoritative.

A candidate gap that fails all five checks is a gap. A gap that fails
one or two is a documentation omission, not a language gap.


## G9 — Record-to-union cross-kind references (FIXED)

Records and unions in the same file could not reference each
other via fields. Specifically, `rec R { field: U }` where `U`
is a union declared in the same file failed with:

    error[E0003]: Unknown type `U`

even when `U` was declared earlier in the file.

**Root cause:** `src/semantics/analyzer/mod.rs`
`analyze_with_spans` ran the full record registration sequence
(pass 1 for names, then pass 2 for fields) before any union
registration (pass 1 for names, then pass 2 for variant
payloads). So when `resolve_record_fields` resolved a field
whose type was a union, `self.union_types` was still empty.

**Reverse direction worked** — a union variant payload naming a
record resolved correctly, because union resolution ran after
record resolution finished.

**Discovered:** 2026-10-31, while writing
`src/chaincss/json.gol`:
    pub union Json
        ...
    rec ParseOk
        value: Json
        pos: Int

**Fix:** `src/semantics/analyzer/mod.rs` — interleave pass 1
for both kinds, then pass 2 for both:

    for rec in records { register_record_name(rec) }
    for union in unions { register_union_name(union) }
    for rec in records { resolve_record_fields(rec) }
    for union in unions { resolve_union_variants(union) }

**Verification:** corpus_01, corpus_41, corpus_62, and
examples/json all produce expected output after the change.
No regression.

**Status:** Fixed in the compiler. Future port files can freely
mix `rec` and `union` in one file.

---

## Methodology update (2026-10-31, after G9)

The lesson of G9 (and G4, G5) is that the port's real output
is **fixes to ALGOL26**, not workarounds in the ported code.
When a wall is hit:

1. Identify whether the wall is a genuine ALGOL26 weakness
   (features missing, ordering bugs, reserved-word
   collision) or a client-code mistake.
2. If it's a weakness, fix it in the compiler. The fix belongs
   in `src/`, not in a workaround in `.gol` code.
3. Record the fix in this file with before/after code, root
   cause, and verification.
4. Then rewrite the client code without the workaround — it
   should be more natural now.

The ChainCSS port is the forcing function; the compiler fixes
are the deliverable. A port file that "works" through a
contorted shape is a failed port — it hides the weakness that
the port should have exposed.

This session produced G9: 4 lines in `mod.rs`, discovered by
trying to write `json.gol` naturally. That's the right shape.
