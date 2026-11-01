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


## G11 — Stale union placeholders in record field types (FIXED)

Second-order consequence of G9's fix. Once records could reference
unions in the same file, the references captured the union's
**pass-1 placeholder** (empty `variants`) rather than the resolved
union. The record table then held types like
`Json { variants: [] }` while function signatures held
`Json { variants: [<full>] }`.

The IR verifier's `Terminator::Return` consistency check compared
these with structural `!=`, so the two "different" `Json` types
were rejected as a return-type mismatch.

Discovered while running `tests/chaincss/json_parse_smoke.gol`:

    error[E0002]: Function 'parse' declared return type
      Result { ok: Json { variants: [<full>] }, error: String }
    but returns
      Result { ok: Json { variants: [] }, error: String }

**Root cause:** `resolve_record_fields` runs during record pass 2.
Under G9's interleaved registration, union pass 1 (which registers
placeholders) has run, but union pass 2 (which fills in variants)
has not. Any record field naming a union therefore captures the
empty-variants placeholder permanently.

**Fix:** Two parts, both in the compiler.

1. `src/semantics/analyzer/mod.rs`: after union pass 2 completes,
   walk `self.records` and call `rewrite_union_placeholders` on
   every field type. Replaces any `Type::Union { variants: [] }`
   with the fully-resolved union from `self.union_types`.

2. `src/ir/verifier/terminator.rs`: the return-type consistency
   check now uses `can_coerce_to` in addition to structural `!=`.
   Two unions with the same `UnionTypeId` are semantically the
   same type (as `can_coerce_to` already knows), so this check
   needs the same semantic equality.

**Verification:** All 479 lib tests pass, all integration tests
pass, and the four corpus sanity checks (corpus_01, corpus_41,
corpus_62, examples/json) still produce expected output.
`json_parse_smoke.gol` now runs successfully.

**Status:** Fixed. Both parts landed in one commit.

---

## Methodology note on G11 (2026-10-31)

G9 was correct but incomplete. Fixing the *symptom* (records
couldn't see unions) required fixing the *root cause* (records
hold placeholder unions). The port's real deliverable is not
"make json.gol compile" — it's "fix ALGOL26's real bugs, even
the second-order ones that G9's fix exposed."

The pattern: **fix, then look for the second-order effects of
the fix.** G9 removed a wall; G11 was the crack in the wall that
G9 built. G9's fix enabled the record-to-union reference; G11's
fix made that reference carry the correct data.

A single fix often just moves the bug. The full fix requires
verifying the downstream consumers of the fix. In this case,
the pipeline was:

  G9 fix -> record field gets placeholder
        -> IR builder propagates placeholder
        -> verifier rejects as "different type"
        -> G11 fix: rewrite placeholders + verifier respects union id

Each step was necessary. The port's deliverable is the full
chain, not just the first link.


## G12 — IR builder drops statements after certain `if`-block patterns

**Symptom:** `parse_number` in `json.gol` failed with "function `parse`
returned no value" for `"1e5"` and `"1e10"`. The `has_frac=false,
has_exp=true` runtime path never reached any `return`.

**Root cause:** unclear. The AST (via `inspect --ast`) is complete.
The IR (via `inspect --ir`) for the working path is complete. The
failing path's IR was not fully inspected. A debug print inserted
with an inline `if X then "true" else "false"` in expression position
caused the IR verifier to report "Function 'parse_number' block 281
has no terminator" — confirming that the IR builder's emitted CFG
can be missing a terminator on certain paths.

**Workaround in json.gol:** force `has_frac = true` when `has_exp =
true`, routing the exp-only case through the frac+exp path (which
works). The arithmetic result is identical because the empty frac
part contributes zero.

**Status:** Open — compiler-side fix needed. The trigger is narrow
enough that we shipped the workaround. Needs a minimal repro for
a proper fix.

## G13 — Inline-if in a larger expression drops subsequent statements

**Symptom:** adding `print("... " + if has_frac then "true" else "false")`
inside `parse_number` caused the IR verifier to reject the function
with "block 281 has no terminator". The trailing statements after
the print (the entire float computation) were missing from the IR.

**Root cause:** suspected — the inline-`if`-as-expression form is
being mis-parsed or mis-emitted, and subsequent statements in the
same block are dropped. Same class as G12 (dropped statements after
conditional patterns) but with a different trigger.

**Workaround:** never use inline `if X then A else B` as part of a
larger expression in a statement that has subsequent statements.
Use `if X { A } else { B }` block form, or assign the value to a
`val` on a previous line.

**Status:** Open — compiler-side fix needed.


## G15 — `Option[Record]` in function argument position breaks type resolution

**Symptom:** passing an `Option[Record]`-typed parameter to another
function whose parameter is also `Option[Record]` triggers:

    error[E0003]: Unknown type `Option`. It is not a primitive,
    a declared record, or a type parameter in scope.

The error points at the *parameter declaration* `Option[...]`, not
at the call site. The type is legal in isolation; the failure appears
only when the parameter is *used as an argument* in the function
body.

**Minimal repro:** `tests/chaincss/compiler_bugs/G15_option_record_arg.gol`:

    import "ir_types.gol"
    proc consume(source: Option[SourceLocation]) -> Int
        return 1
    proc test(source: Option[SourceLocation]) -> Int
        return consume(source)

Fails with the error above. Removing the `consume(source)` call
(keeping both signatures) makes it pass. Making `source` a plain
`SourceLocation` (not `Option`) also makes it pass.

**Scope:**
- Records as args: OK
- `Option[Primitive]` as args: OK
- `Option[Record]` as args: BROKEN

**Workaround in ALGOL26 source:** avoid passing `Option[Record]`
arguments. Pass a `&Record` + `Bool has_value` flag pair, or
`&Option[Record]` (reference, which is `Copy`) if the analyzer
supports it — untested, see below.

**Discovered:** 2026-10-31 while writing `parse_nested_object` for
the ChainCSS port. The port needs `Option[SourceLocation]` threaded
through three mutually-recursive functions; the arg-passing pattern
is pervasive.

**Status:** Open — compiler-side fix needed. Suspected location:
analyzer's move-tracking / parameter-binding path does not
re-resolve `Option[Record]` correctly. Fix would be in
`src/semantics/analyzer/` around the `Call` expression arm or in
`resolve_type_syntax` caching.

**Tests to add when fixed:**
- `test_option_rec_arg.gol` should compile.
- `test_move_option.gol` and `test_use_twice.gol` should compile.
- Add to `tests/chaincss/compiler_bugs/` as positive tests.


## G16 — `&Record` parameter + record-literal-return function breaks parser

**Symptom:** a function that returns `Record { ... }` (a record
literal in a `var` initializer or `return`), followed by another
function whose parameter is `&Record` and whose body starts with an
`if`, triggers:

    error[E0001]: Unexpected token at top level: Indent

at the second function's body's first line. The parser loses the
enclosing function context.

**Scope (from probes):**
- Single `&Record` param + `if` body: OK
- Two `&Record` params, no record-literal return, no match on fields: OK
- Two `&Record` params, first function has record-literal return,
  second has `if` body: BROKEN
- `var result := Record { ... }` + `match source.field { ... }` +
  `return result`, followed by `&Record` + `if` function: BROKEN

**Not yet isolated to a single feature.** Candidates: record-literal
in `var` initializer, `match source.field` on a `&Record` reference,
or their combination.

**Discovered:** 2026-10-31 while porting `parse_nested_object`.
Together with G15 (`Option[Record]` in arg position), G16 blocks
`parse_nested.gol` (Phase 2C).

**Status:** Open. `parse_nested.gol` deferred to a compiler-fix
session, moved to `tests/chaincss/deferred/parse_nested.gol`.

**Workaround in ALGOL26 source:** unknown without further
investigation. Suspected workaround: avoid record literals in
`var`-initializer + field-match combinations in functions that
precede `&Record`-param functions.

---

## G15 and G16 — RESOLVED 2026-11-01

Both were the same lexer bug, not separate issues. Root cause:
`parse_signature` in `src/frontend/lexer/decl.rs` did not emit
`[` or `]`. Non-`pub` `proc`/`fn` declarations with generic
types in the signature — parameter or return — had their brackets
silently dropped. `pub proc` and record field types were unaffected
because they lex through `tokenize_expression` instead.

Fix: added `'['` and `']'` arms to `parse_signature`, mirroring
`handle_operator`. Six lines.

Minimal repro that fails before the fix and passes after:

    proc second(x: Int) -> Option[String]
        return None

See commit message for full analysis.

---

## G18 — Nested `match` on a union loses the variant table

**Symptom:** an inner `match` inside an outer `match` arm, on a union
value, fails with:

    error[E0004]: Union 'Json' has no variant 'Str'
    = help: Available variants of 'Json':

(empty variant list)

**Minimal repro:** `probeUnion6` —

    proc test(x: Option[Json]) -> Bool
        match x {
            case Some(v) then {
                match v {
                    case Arr(items) then {
                        for item in items {
                            match item {          // ← fails here
                                case Str(_) then { return true }
                                case _ then {}
                            }
                        }
                        return false
                    }
                    case _ then { return false }
                }
            }
            case None then { return false }
        }

`match v` inside `match x` is fine. `match item` inside `match v` fails.
The variant table for `Json` is empty at depth ≥ 2.

**Confirmed passing** (depth 1):
- `match x { case Str(_) ... }` on `x: Json` param
- `for item in items { match item { case Str(_) ... } }` at function top-level

**Confirmed failing** (depth ≥ 2):
- `match inner { ... }` inside a `match outer { case Arr(items) ... }` arm

**Workaround:** extract the inner match into a top-level helper function
(`collect_string_items`, `append_scalar_declarations` in `parser.gol`).
The inner match is then at depth 1 in the helper.

**Status:** ✅ **RESOLVED** (2026-11-01, commit fa13ae8).

**Root cause:** The analyzer's `rewrite_union_placeholders` pass
(added for G11, which fixed the *record field* case) was not applied
to union variant payloads. When a union's own variant payload
referenced the union via indirection — `Arr(List<Json>)`,
`Obj(Map<String, Json>)` — the inner `Json` reference was resolved
against the pass-1 placeholder (`Union { variants: [] }`) during
union pass 2 and never rewritten. Then `for item in items` bound
`item` to the empty stub, and the inner
`match item { case Str(_) ... }` failed with an empty variant list.

**Fix:** `src/semantics/analyzer/mod.rs:582-660` — extend the
placeholder-rewrite pass to also walk union variant payloads, with
bounded fixpoint iteration (up to 10 passes) to resolve one level
of indirection per pass. Cycles (Json contains List<Json>) mean
unbounded expansion is impossible, so iteration is capped and the
common destructure depth (one level) is fully resolved.

**Regression test:** `nested_match_on_recursive_union_resolves_variants`
in `src/semantics/analyzer/tests.rs`.

**Port impact:** The helper cascade in `parse_ir.gol`'s
`handle_scalar_entry` / `handle_pseudo_entry` /
`handle_nested_entry` / `handle_rule_entry` is now removable —
a follow-up commit will collapse them back into the natural
nested-match form.
---

## G19 — `&mut` doesn't propagate through two forwarding layers — RESOLVED

**Symptom:** `main → dispatch(&mut r) → handle(&mut r) → *r := v`
silently loses the mutation. One hop works; two hops don't
propagate.

**Minimal repros:** `g19_a.gol` through `g19_e.gol` cover
primitive/record × 1-hop/2-hop × LLVM/interpreter. Only `d`
(2-hop record via interpreter) originally failed; all others
passed. That isolated the bug to the **interpreter**, not the
analyzer or the LLVM backend.

**Root cause had two parts:**

1. `eval_call` resolved **all** reference arguments to their
   values before binding. That's correct for by-value params
   (the callee needs the value), but wrong for `&mut T`. One-hop
   forwarding happened to work because ADR 0035's write-back
   machinery copied the callee's final parameter value back to
   the caller by name; that mechanism does not chain through a
   second hop.

2. The interpreter's `variables` map was a single `HashMap` per
   frame. References carry `Place { base: name }`, and `name`
   lived in the caller's frame — so a preserved reference was
   unresolvable in the callee's frame. That is exactly the
   trade-off ADR 0053's comment documented:
   *"a reference passed as a parameter does not observe later
   mutations to the base... conservative choice until a future
   ADR introduces a scope-stack `variables` model."*

**Fix:** three interlocking changes in
`src/backends/interpreter/` (commit `eb73193`):

1. **Frame stack.** `variables: HashMap<...>` →
   `variables: Vec<HashMap<...>>`. New helpers:
   - `lookup_var` — walks the stack from innermost to outermost
   - `insert_var` — binds in the innermost frame
   - `write_var` — updates the innermost frame that contains
     the name
   `eval_call` and `call_function_with_values` push a frame on
   entry, pop it on return (including early error returns). This
   is the scope-stack model ADR 0053 anticipated but deferred.

2. **Reference preservation + chasing.** `eval_call` preserves
   `Reference` arguments when the parameter type is
   `Borrow(T)` / `MutBorrow(T)`, and still resolves them for
   by-value parameters. `resolve_place` and `assign_place`
   deref `Reference` values encountered at the base, redirecting
   reads and writes to the referenced place.

3. **`Instruction::FieldAssign` deref.**
   When the base value is a `Reference`, the handler mutates the
   referenced record's field and writes the **whole record** back
   to the referenced place — not to the frame-local binding.

The ADR 0035 write-back machinery is **now dead code**:
mutations propagate directly through `write_var` walking the
stack. Write-back is filtered out for reference params to avoid
double-writing.

**Regression tests:** `g19_a.gol` through `g19_e.gol` are kept
as integration tests. `g19_d.gol` (2-hop record, interpreter) is
the primary regression — the exact case that used to fail.
G18's deep nested-match regression test still green (checked in
`cargo test`).

**Port impact:** the `-> IRRule` threading workaround in
`parse_ir.gol` — `dispatch_rule_entry` / `handle_rule_entry` /
`handle_scalar_entry` / `handle_pseudo_entry` /
`handle_nested_entry` threading the rule by return value rather
than `&mut IRRule` — can now be collapsed back to natural
`&mut IRRule` chaining, matching the TypeScript original's
`parseNestedObject(selector, value, rule, source)` shape.
Follow-up commit will remove the workaround; the current
verified state is preserved.

**Residual — G19b/c.** G19's runtime fix (scope-stack variables,
reference preservation, `resolve_place` reference-chasing) exposes
two follow-up gaps that block fully-natural port code:

**G19b — last-use lifetimes for `&mut`.** The analyzer's
borrow checker treats `&mut x` as held until the enclosing scope
ends, not until last use. That rejects:

    var r := ...
    f(&mut r, ...)
    use(r)            ← E0007: r still mutably borrowed

TS has no equivalent (GC language). Fixing requires last-use tracking for mutable borrows, mirroring
the shared-borrow machinery from ADR 0043 — a borrow is live from
creation to last recorded use, with conservative promotion to a
LAST_USE_SENTINEL inside loops and defer bodies. ~250 lines, 1.5
days. Covers the port's needs. Fixed in commits `7ac9519` +
subsequent frame-aware-place commit. Full branch-precision NLL
(Rust 1.31 style) is a follow-up, not required for the port.

**G19c — projection composition bug in the reference chase.** The scope-stack model
carries `Reference(Place { base: String, ... })`, but `Place.base`
names a variable without recording WHICH frame it lives in. When
`&mut rule` is bound to a parameter in a callee's frame, the
reference's `Place{base: "rule"}` resolves against the CALLEE's
frame — finding the parameter slot, which holds the reference
itself. `resolve_place` then chases the self-reference infinitely.
This blocks chaining `&mut` through two forwarding layers, e.g.:

    proc outer(r: &mut IRRule) -> Void
        inner(&mut r)          ← reference is preserved, then re-resolved
                              ← in inner's frame as self-reference
                              ← infinite recursion

The interpreter *does* have a frame stack; `lookup_var` walks by
name correctly. The actual bug was that `resolve_place` /
`assign_place` chased a `Reference` at the base of a `Place` but
carried the leading `Deref` projection into the composition, so a
second-hop write hit `Unsupported`. Fix: strip one leading `Deref`
before recursing into the chased place, plus frame-origin tracking
to resolve same-name bindings in the correct frame. ~15 lines of
fix + 28 of state support. The 'frame-qualified Place' theory was
right in shape but wrong in magnitude — not a multi-day design
change, just a projection bug plus a small supporting change.

Workaround: keep `-> T` return-type threading for the `&mut`
chain (as `parse_nested_object` does). The port remains at the
helper-cascade state; the fully-natural nested-match + `&mut`
chaining refactor is deferred until G19b/c are resolved.

**G22 — interpreter stack overflow on deeply nested programs.**
The interpreter clones the entire `SemanticProgram` before running
(`Interpreter::with_args(ir.program().clone(), ...)`), and
`Clone::clone` on `TypedIRValue` recurses per nesting level.
Naturally-nested port code (deeply-nested matches via the G18 fix)
produces an IR deep enough to overflow Rust's default 8 MB
main-thread stack during the clone and during `execute_function`'s
iterative block traversal. Confirmed: the refactored `parse_ir.gol`
(the 214-line natural version) overflows even at 256 MB.

Root cause is not depth — the tree is finite (Clone succeeds at
8 MB in some cases). It's that `eval_value` recurses per
`TypedIRValue::FieldAccess` and per `Reference` in
`resolve_place`, and `resolve_place`'s reference-chasing enters
infinite recursion on self-referential references (G19c). Once
G19c is fixed, G22 should follow.

**Status:** ✅ Closed by G19c's fix. The infinite recursion in
`resolve_place` no longer occurs — `test_g19b_full.gol` (the
minimal repro) passes under the interpreter with the default 8 MB
main-thread stack. No thread-stack workaround needed.

**Recommendation:** the port remains at the helper-cascade state
(parity 3/3, all fixes committed). The port-natural refactor is
deferred to a dedicated NLL + frame-qualified Place session.

