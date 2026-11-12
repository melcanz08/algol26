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


### W0002 regression-test status

The analyzer's G18 fallback resolves an empty variant table from
`self.union_types` by name. On every shape we've tested —
including the recursive `Json` union and a 4-deep nested match on
it — this fallback succeeds, so W0002 does not fire. W0002 is
defensive: it warns only if the analyzer reaches an empty-table
state where the fallback *also* fails. Source programs cannot
reliably reach that state, so there is no firing test.

Tested: config plumbing (`--no-warn=W0002`, `--deny-warnings=W0002`)
via `warning_w0002_not_emitted_on_shallow_match` and
`warning_w0002_suppressed_by_no_warn` in
`tests/integration/warning_system.rs`.

The runtime symptom (silent fallthrough at depth >= 2) is deferred
to the full G18-addendum fix.

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

---

## G18d — recursive-union stub-depth mismatch in equality checks

**Symptom:** assigning a `Map<String, Json>` built from JSON parsing
into a variable declared as `Map<String, Json>` from the defaults
fails with a type mismatch. The two `Union { name: "Json" }` types
differ only in the *depth of the placeholder stub* inside their
`Arr` / `Obj` payloads: one has `List(Union { variants: [] })`, the
other `List(Union { variants: [ ..., Arr(List(Union { variants: [] }))) ] })`.
The equality check compares the whole variant list including
payloads; different stub depths mean unequal types.

**Root cause:** the recursive union `Json` has a placeholder for
itself inside its own variant payloads. That placeholder is a
`Union { variants: [] }` at some finite depth (wherever resolution
stopped). Different code paths resolve `Json` at different depths —
one from the registry (deeper), one from a context that had the
shallower stub. Type equality fails because the payloads differ
structurally.

**Workaround applied in the port:** the `intents` field of
`ChainCSSConfig` is typed as `Json` rather than `Map<String, Json>`.
Treating the whole field as opaque JSON sidesteps the comparison —
behaviorally equivalent to TS's `Record<string, IntentDefinition>`.
`config_to_json` inserts the `Json` directly instead of iterating
a typed map.

**Status:** Open. The right fix is either stub-depth normalization
at union registration (all `Json` values get the same stub depth) or
structural equality that ignores stub depth. Deferred — the port
works around it cleanly, and the workaround is behaviorally
equivalent to TS.

---

## G23 — TS/JS execution model is not portable

**Symptom:** ChainCSS's config loader executes `.ts` and `.js` config
files via dynamic `import()` (jiti for `.ts`). Plugins are objects
with `setup` / `transform` / `transformCSS` function fields. Macros
are `MacroHandler` functions. Presets may be functions of type
`(base) => Config`.

**Impact:** these values are *executable JavaScript*. ALGOL26 can't
execute JS without embedding a JS engine (out of scope for the port).

**Workaround applied in the port:**
- `config.gol` reads `chaincss.config.json` only. The four corpus
  entries gained `.json` twins alongside their `.js` originals.
- Fields whose TS type is function-valued (`macros`, `plugins`,
  presets in function form) are stored as opaque `Json` in the
  port's records. `config_to_json` round-trips them faithfully;
  the port just doesn't execute them.
- `.ts`/`.js` config file execution is deferred to Phase 10+ (plugin
  host / dynamic execution), if ever implemented.

**Status:** Accepted divergence. The port's config-loading path
handles JSON configs — the standard for cross-language tools.
Executable configs remain a Node-specific capability.

---

## Phase 4 — complete (2026-11-01)

IR parity (3/3 corpus entries) was already verified in Phase 2D.
Config-loading parity (4/4 corpus entries) is now verified by the
`config.gol` port.

Files added:
- src/chaincss/config.gol — ChainCSSConfig + nested records +
  get_default_config + load_config_from_json + config_to_json
- chaincss/diff-corpus/ts-loadconfig.ts — TS oracle
- chaincss/diff-corpus/run_config_parity.sh — driver
- chaincss/diff-corpus/inputs/*/chaincss.config.json — 4 configs

Deferred with correct magnitude:
- G1 (PassMetadata index signatures) — Phase 4 conditional
- G23 (TS/JS execution) — Phase 10+
- G18d (stub-depth equality) — workaround in place, fix deferred

Phase 4 exit criterion met.


---

## G27 — Two bugs: `ReadReference` not a place; `eval_call` masks real errors

Two intertwined bugs surfaced together while running the Phase 2A seam
test (`ast_roundtrip_smoke.gol`) against the `003-lenient-passthrough`
corpus entry. Either one alone would block the smoke test; together they
produced a misleading error chain that took several rounds to unravel.

### Bug A — `place_of_ir_value` had no `ReadReference` arm

**Symptom.** `(*m).insert(k, v)` where `m: &mut Map<String, Json>`
reported:

    interpreter does not support `Map.insert on non-place receiver`:
    the receiver must be a variable, field, or other place

**Root cause.** `place_of_ir_value` in
`src/backends/interpreter/eval.rs` matched `Variable`, `FieldAccess`,
`ArrayAccess`, and `_ => None`. `TypedIRValue::ReadReference { expr }`
— the IR node for `*r` where `r: &mut T` — fell into the `_` arm and
returned `None`. Every place-mutating builtin (`Map.insert`,
`List.append`, and any future one) rejected `(*r).method(...)`
receivers.

**Fix.** Add a `ReadReference` arm that delegates to the inner
expression's place:

    TypedIRValue::ReadReference { expr, .. } => self.place_of_ir_value(expr)

No `Deref` projection is added. The reference chase in
`resolve_place` and `assign_place` (G19c) already handles the
dereference implicitly: when the base variable's runtime value is
`Reference(inner_place)`, those functions redirect to `inner_place`
and strip any leading `Deref` projection. Adding one here would
double-deref and hit the `Projection::Deref` unsupported arm.

### Bug B — `eval_call` masked real errors as "returned no value"

**Symptom.** Every error from a nested `execute_function` call reached
the top level as:

    function `path_b` returned no value but its return type is `String`

— regardless of what actually failed inside. Bug A's actual error
message (`Map.insert on non-place receiver`) was invisible; the
outermost frame's "returned no value" message was what users saw. This
masked the real bug for multiple debugging rounds.

**Root cause.** In both `eval_call` and `call_function_with_values`,
the return-value check happened **before** the error check:

    let result = self.execute_function(&callee);
    let ret = match self.return_value.take() {
        Some(v) => v,
        None if callee.return_type == Type::Void => RuntimeValue::Void,
        None => {
            return Err(... "returned no value" ...);   // fires on error too
        }
    };
    // result.is_err() check came here — too late; return above
    // already fired

When `execute_function` errored, `self.return_value` was `None`, so
the "returned no value" message was emitted, and the real error in
`result` was silently discarded.

**Fix.** Reorder: check `result.is_err()` first, propagate the real
error with the frame name for context, then check the return value:

    let result = self.execute_function(&callee);
    if let Err(e) = &result {
        self.pop_frame();
        self.return_value = saved_ret;
        self.region_stack = saved_regions;
        return Err(EvalError::Runtime(
            format!("in {}: {}", callee.name, e)));
    }
    let ret = match self.return_value.take() {
        Some(v) => v,
        None if callee.return_type == Type::Void => RuntimeValue::Void,
        None => { ... "returned no value" ... }
    };

### Verification

- `tests/chaincss/compiler_bugs/g27_mut_map_insert_via_read_reference.gol` — minimal repro for Bug A: `(*m).insert(k, v)` inside a proc that received `m: &mut Map<String, Json>`. Fails before the fix with `Map.insert on non-place receiver`; passes after.
- `tests/chaincss/compiler_bugs/g27_error_propagation.gol` — minimal repro for Bug B: any proc that calls a proc which errors should surface the *inner* error, not the outer "returned no value". Fails before the fix with the outer "returned no value" message; passes after.
- `tests/chaincss/ast_roundtrip_smoke.gol` — Phase 2A seam test. Runs to completion on all 3 corpus entries, printing `OK: ast roundtrip is lossless`.

### Methodology note

Every "returned no value" diagnostic in the last several debugging
rounds was **a real error masked by Bug B**. The smoke test's apparent
failure (no prints, no return) was a `Map.insert on non-place receiver`
error occurring three frames deep — invisible because each enclosing
`eval_call` replaced it with "returned no value" before propagating.

**Lesson:** when the error message doesn't name the actual cause, the
error-propagation layer itself may be the bug. A compiler or
interpreter whose errors are informative at the leaf but opaque at the
root is not just inconvenient — it hides real bugs from the very
people trying to fix them. Check that error *propagation* preserves the
original message before concluding that the error message itself is
wrong.

**Status:** Fixed. Both arms landed in one commit.


---

## G28 — `Date.now()` / `Math.random()` not portable

**Symptom.** `src/compiler/diagnostics/reporter.ts` depends on
`Date.now()` for `Diagnostic.timestamp` and `DiagnosticReport.generatedAt`,
and on `Math.random()` for `Diagnostic.id` (`diag-<now>-<random_b36>`).
ALGOL26 has no time or RNG primitive.

**Same class as G23** (TS/JS execution model not portable). Timestamps
and IDs generated inside the JS runtime have no ALGOL26 equivalent
without embedding a clock/RNG.

**Mitigation used in the port.** `src/chaincss/diagnostics.gol`
substitutes:
- `timestamp: 0` for `Date.now()`
- `generatedAt: 0` for `Date.now()`
- `id: "diag-0-<counter>"` where `counter` is a monotonic field on
  `DiagnosticReporter` (substitutes `Date.now() + Math.random()`)

`format_report` renders the fixed `generatedAt` as the ISO 8601 epoch
zero string: `"1970-01-01T00:00:00.000Z"` — exactly what TS's
`new Date(0).toISOString()` produces.

**Parity strategy.** The comparator
(`diff-corpus/compare_diagnostics.py`) normalizes non-deterministic
fields (id, timestamp, generatedAt) and compares the rendered string
byte-for-byte after the TS harness normalizes its own timestamp line.
The TS harness's `.replace()` on the rendered output forces both sides
to the same fixed clock.

**Note — this makes the port *more deterministic* than the TS
original.** The TS `formatReport()` calls `generateReport()` internally
and produces a fresh `Date.now()` each time; the ALGOL26 port always
emits epoch zero. This is a *feature*, not a divergence — it means
diagnostic output is reproducible across runs, which is generally
desirable for CI goldens and diffing.

**Status:** Accepted divergence. Convention: `timestamp: 0` /
`generatedAt: 0` / counter-based IDs. Matches the convention already
established by `ir_types.gol` (`make_transform_record` uses
`timestamp: 0` and documents the same rationale).

---

## G29 — `match`-arm mixed-yield requires explicit `Void` discarding

**Symptom.** A `match` where one arm ends with a value-returning call
and another arm ends with `{}` or a `Void` expression fails:

    error[E0002]: match arms produce inconsistent results:
    some arms yield a value, others do not

**Example.**

    proc report_one(r: &mut DiagnosticReporter, entry: Json) -> Void
        match entry {
            case Obj(m) then {
                ...
                report(r, ...)      // returns Diagnostic — yields Diagnostic
            }
            case _ then {}          // yields Void
        }

Both arms of the outer `match entry` reach different yields: `Diagnostic`
from `report(...)` in the `Obj` arm, `Void` from `{}` in the `_` arm.

**Not a language bug.** The check is correct — mixed yields within one
`match` are a type error, and that error is exactly what a
strongly-typed language should produce. The issue is **ergonomic**: it's
easy to write the "obvious" code (call the function in statement
position) and hit this because "statement position" and "the last
statement of a match arm" are treated differently by the yield-inference
pass.

**Fix pattern.** When an arm ends with a value-returning call and other
arms yield `Void`, wrap the call:

    val _discarded := report(r, ...)

`val _ := expr` binds the value to an unused variable, discards it, and
yields `Void` — matching the other arms. Alternatives:
- `return` after the call (in a `Void` proc)
- Move the call into a helper proc (`Void`-returning)

**Same fix, same shape.** Both `ast_roundtrip_smoke.gol`'s
`json_to_style_definition` and `diagnostics_format_parity.gol`'s
`report_one` / `main` arms needed this. It has surfaced three times in
Phase 2 alone.

**Port pattern for future files.** Before writing a `match` where any arm
ends with a value-returning call, check whether the other arms yield
`Void`. If yes, wrap the call in `val _ := ...`.

**Common return types that trigger this:**
- `report(...)` → `Diagnostic`
- `File.write(...)` → `Int` (bytes written)
- `r.diagnostics.append(d)` → `Void` (safe, no wrap needed)
- `some_proc(...) -> T` for any concrete `T`

**Status:** Client-side pattern, not a language gap. Documented for
future ports. Not worth a compiler change (the check is correct; a
compiler-side "auto-discard in Void position" rule would be less
predictable than an explicit `val _`).

**Methodology note.** This is the third pattern we've hit that isn't a
language gap but is a *usability* cost: (a) the `Map<...]` typo class
(G-less, documented via lint script), (b) `match`-in-return-position
mixed yield (this entry), (c) `Some(s)` in `WriteReference` requiring
G25. All three have been "the port code was written the obvious way;
the compiler wanted the other way; the difference isn't a bug in either."
Recorded as **ergonomic patterns**, distinct from **language gaps** that
require compiler fixes. The two categories need separate tracking, or
the gap log becomes cluttered with non-bugs.


### G29 — resolution: Design 1 landed (2026-11-02)

**Behavior change.** A `match` with mixed `Void` / non-`Void` arms
is now legal. The match yields `Void`; non-`Void` arm trailing
values are discarded at IR-build time via `Instruction::Nop`. This
removes the friction that forced `val _ := f()` at every such site.

**Strictness preserved.** Two different non-`Void` arm types still
flow through `common_supertype` exactly as before. Note that
`common_supertype(Void, T) == Unknown` (default arm, `types.rs:893`)
— not `Void`. The analyzer's new branches bypass this by explicitly
yielding `Void` for mixed cases; without the branches, the fold
would produce `Unknown`, which silently skips downstream checks
instead of failing. The explicit `all_void` / `any_void` short-circuit
is load-bearing.

**Warning deferred.** The `Diagnostic::Warning` type exists
(`src/common/diagnostics.rs`) and is used by the IR builder
(`build_semantic_ir_program` prints them via
`Diagnostic::Warning(...).display()`), but `SemanticAnalyzer` has
no `warnings` field or `take_warnings()` method — there is no
analyzer warning infrastructure. Emitting "non-Void arm value
discarded" requires that infrastructure. Deferred to a future
warning-system ADR; G29's warning will be its first use. This
means the *behavior* fix ships now, and the *visibility* fix
ships whenever the warning system does.

**Files touched:**
- `src/semantics/analyzer/expr.rs` — `ExprKind::Match` yield-check
  replaced with `all_void` / `any_void` short-circuit branches
- `src/semantics/builder/blocks.rs` — `translate_block_with_result`
  Void-target discard via `Nop` when trailing value type ≠ `Void`
- `src/semantics/analyzer/tests.rs` —
  `test_match_arm_void_mismatch_rejected` replaced by
  `test_match_arm_mixed_void_accepted_statement`, plus two
  regression tests (`all_void`, `all_value`)
- `tests/chaincss/compiler_bugs/g29_mixed_void_match.gol` — new
  regression, exercises the exact port shape

**Verification:**
- `cargo test` — all suites green (including the two new
  regression tests and the updated mixed-Void test)
- `g29_mixed_void_match.gol` — prints `hello` under the interpreter
- `ast_roundtrip_smoke.gol` — still produces
  `OK: ast roundtrip is lossless` on all 3 corpus entries
- `diagnostics_format_parity.gol` — still runs cleanly
  (4/4 diagnostics scenarios passed via `run_diagnostics_parity.sh`)

**Port impact.** The ChainCSS port can now write mixed-Void matches
directly:

    match v
      case Str(s)
        report(r, ...)     // returns Diagnostic — discarded
        print(s)           // continues normally
      case _
        {}

The `val _ := report(...)` wrapper is no longer needed. Existing
code that uses the wrapper continues to work; it's just no longer
required.

**Methodology note.** This is the first change in the G-series
that is a *language-design* change rather than a compiler-bug fix.
G9, G11, G15/16, G18, G19b/c, G25, G27 were all "the compiler was
wrong"; G29 is "the compiler was right but user-hostile." Both
classes matter, but they need separate discipline: bug fixes get
regression tests, design changes get **both** regression tests and
a design rationale (this section). The task was scoped as a split
— behavior now, warning infrastructure later — because shipping
the warning requires its own subsystem, not a small extension of
the analyzer.


#### G27B — audit (2026-11-02)

Searched `src/backends/interpreter/eval.rs` and
`src/backends/interpreter/mod.rs` for siblings of the
`returned no value` mask. The mask shape is: an error path that
constructs a new message instead of propagating the original
error. Every candidate was inspected:

**`eval.rs`**
- L1410 (`File.read`) — constructs `format!("File.read: cannot read
  '{}': {}", path, e)`. **Includes** the original cause. Not a mask.
- L1428 (`File.write`) — same shape. Not a mask.
- L820 (`integer division by zero`), L947/L962/L980/L999 (builtin
  arity errors), L1250/L1263/L1282 (`.unwrap_or("none")` in error
  argument formatting) — all leaves, not masks.
- L78 (`GLOBAL_FRAME` fallback), L502/L886 (`unwrap_or(0)` for
  `checked_shl`) — not error paths.

**`mod.rs`**
- Every `Err(EvalError::Runtime(...))` is a leaf from an
  instruction handler (division, bounds check, infinite loop,
  unsupported op). No error is discarded. Not a mask.

**`call_function_with_values`** — same fix as `eval_call`
(ad08a53). Both now check `result.is_err()` before checking
`self.return_value.take()`.

**Conclusion: one mask (G27B), one fix, zero siblings.**

The audit was worth doing because two things were true:
1. The mask had existed for a long time before being noticed.
   If it existed, siblings might also.
2. The mask's shape — "error-checking order" — is easy to get
   wrong in new code. Ruling out siblings gives confidence for
   future work in this file.

Regression coverage:
- `tests/integration/interpreter_error_propagation.rs` —
  three tests exercising the invariant at depth 1, 2, and 3, with
  both division-by-zero and builtin-leaf errors. Each test asserts
  the `returned no value` mask string is absent.

A negative audit result is a result. Recorded so a future reader
knows the search was done, what was searched, and what was found.


---

## G30 — Parser diagnostics: mixed bracket families in generics

**Not a language gap.** The parser rejects the malformed program
correctly. The gap is *diagnostic quality*: the message names the
parser's expectation but not the user's mistake. Fixed by adding a
targeted branch at the generic-close site; also records a deferred
general improvement to `expect_token`'s token display.

**Symptom.** Writing `Map<String, Json]` (opened with `<`, closed
with `]`) — a common hand-typo, six occurrences during the Phase
2A session — produced:

    error[E0001]: Expected '>', found RBracket

Which tells the user what the parser wanted, not what they did
wrong or how to fix it. `RBracket` is also a token-variant name,
not the character `]`, which is another source of confusion
(see "Deferred" below).

**Root cause.** `Parser::expect_token` (`src/frontend/parser/mod.rs`)
renders the expected/found tokens with `{:?}`, so any token-name
mismatch produces a message that references internal variant names
rather than source characters. The generic-close site in
`parse_type_syntax_atom` (`src/frontend/parser/types.rs`) called
`expect_token(Token::Gt, "'>'")` — so `Expected '>', found
RBracket` was the best it could produce. It did not know that
`RBracket` after `<` was *specifically* a mixed-bracket-family
typo, only that it wasn't the token it wanted.

**Fix.** Targeted branch at the angle-close site:

    if is_bracket {
        self.expect_token(Token::RBracket, "']'")?;
    } else {
        // G30. Targeted diagnostic for the mixed-bracket-family typo.
        if matches!(self.peek(), Token::RBracket) {
            return Err(self
                .error("Expected '>' to close a generic type, found ']'")
                .with_suggestion(
                    "Write `Map<K, V>` with `>` to close angle-\
                     bracketed generics. `]` closes only square-\
                     bracketed forms (`List[...]`, `Option[...]`). \
                     See tools/lint_generic_brackets.py for the same \
                     check at source-text level.",
                ));
        }
        self.expect_token(Token::Gt, "'>'")?;
    }

Produces:

    error[E0001]: Expected '>' to close a generic type, found ']'
      --> src/....gol:2:35
       |
     2 |     pub m: Option[Map<String, Json]]
       |                                   ^
       |
       = help: Write `Map<K, V>` with `>` to close angle-bracketed
               generics. `]` closes only square-bracketed forms
               (`List[...]`, `Option[...]`). See
               tools/lint_generic_brackets.py for the same check at
               source-text level.

**Complementary mechanism.** `tools/lint_generic_brackets.py` (added
during Phase 2A) catches the same class at **source-text level**
before compilation. It scans every `.gol` file and reports unbalanced
bracket depth or `]`-closing-angle-generic on a per-line basis.
The parser catches the same class at **token level** with an
actionable message. Two mechanisms, two layers:

| Layer | Mechanism | When | What it catches |
|---|---|---|---|
| Pre-compile | lint script | CI, pre-commit, ad hoc | Any `.gol` file, any line |
| Compile-time | parser | `algol26 check` / `build` / `run` | Any program being compiled |

The redundancy is intentional. The lint catches the typo *before*
anyone tries to compile; the parser catches it *when* someone does.
Neither is authoritative over the other — they're different
checkpoints on the same typo class.

**Deferred — general token display.** This fix bypasses
`expect_token` for the specific mixed-bracket case. The *general*
issue remains: every `expect_token` error says `found RParen`,
`found Comma`, `found RBracket` — variant names, not source
characters (`)`, `,`, `]`). That's a diagnostic-quality gap across
the whole parser, not just the generic-close site.

Fixing it fully requires:
- A `Token::display_char() -> Option<char>` (or `display_str() ->
  Option<&str>`) method that renders the source character for
  single-character tokens and leaves multi-character tokens
  (identifiers, keywords, string literals) unchanged.
- `expect_token` updated to use `display_char()` for the "found"
  portion.
- Every parser error site updated to use the new display.

That's a multiple-hour refactor that touches the whole parser's
error-reporting surface. Worth doing when the parser is being
touched for other reasons (Phase 5 normalization may cause such
a visit). Recorded here as the deferred part of G30; the targeted
mixed-bracket fix is what ships now.

**Not a compiler bug.** The parser correctly rejects malformed
input. This is a diagnostic-quality improvement, in the same class
as G4's "check ADRs/tests before declaring a gap" — the language
was working, the messaging wasn't.

**Files touched:**
- `src/frontend/parser/types.rs` — targeted mixed-bracket branch
  at the generic-close site
- `tests/frontend/parser_diagnostics.rs` — new test file, three
  tests: mixed-bracket typo produces targeted message; correct
  angle-close still parses; correct square-close still parses
- `tests/frontend/mod.rs` — added `pub mod parser_diagnostics;`

**Verification:**
- `cargo test --test frontend_tests`: 3 new tests pass; existing
  frontend tests unaffected
- `cargo test`: full suite green
- `/tmp/g30_probe.gol` — the failing probe now names the actual
  mistake and points at the lint script
- `/tmp/g30_probe_good.gol` — the correct version still parses
  cleanly after `import "json.gol"` resolves `Json`


---

## G31 — `CompileError::Display` dropped the suggestion

**Symptom.** `format!("{}", err)` rendered only `err.message`; the
`suggestion` field was silently lost. Tests that wanted to assert
the suggestion was present had to access `.suggestion` directly
rather than the Display output.

**Root cause.** `impl Display for CompileError` was:

    write!(f, "{}", self.message)

No other field rendered. The CLI wrapped this with its own
`error[E####]: {msg}` prefix and rendered suggestions separately,
so the *user-facing* output was correct. The *programmatic* API
(Display) was not.

**Fix (narrow scope).** Display now renders message plus a
`= help: <suggestion>` line:

    write!(f, "{}", self.message)?;
    if let Some(s) = &self.suggestion {
        write!(f, "\n  = help: {}", s)?;
    }
    Ok(())

**Why narrow.** The other structured fields (`severity`,
`error_code`, `label`, `secondary`, `notes`, `file`, `line`,
`column`) belong to a planned rustc-style renderer, Phase 3 work.
Rendering them in `Display` here would preempt that renderer's
design and produce an inconsistent format. The comment in the
`CompileError` struct says explicitly: *"Consumed by the
rustc-style renderer (Phase 3)."*

Rendering the suggestion is different: the suggestion is a single
sentence that completes the error's meaning for a human reader.
It's not structural context — it's the "what to do" that every
diagnostic in the CLI already shows. Making `Display` include it
is the minimum needed to make Display output match what a user
would see.

**Additive.** No existing test broke. `msg.contains(...)` on the
message part still works; `err.suggestion.as_deref() == Some(...)`
field access still works; only tests doing exact-equality on
`format!("{}", err)` would be affected (there were none).

**Regression coverage.**
- `src/common/diagnostics.rs::tests::g31_display_includes_suggestion`
  — asserts message + suggestion + `= help:` prefix all appear.
- `src/common/diagnostics.rs::tests::g31_display_without_suggestion_renders_only_message`
  — asserts no `= help:` line when there's no suggestion.

**Discovered by** G30's test — the G30 test originally asserted
`format!("{}", err).contains("angle-bracketed")` (the suggestion
text), which failed because Display dropped the suggestion. G30's
test was rewritten to access fields directly; G31 fixes the
underlying issue that motivated the original assertion style.

**Status:** Fixed. See G32 for the deferred renderer.

---

## G32 — Unified rustc-style diagnostic renderer (deferred)

**Symptom.** Two rendering paths coexist:

1. `CompileError::Display` — message + suggestion (after G31). Used
   by tests and library consumers.
2. The CLI's own renderer — prepends `error[E####]:`, renders
   `--> file:line:col`, includes the source line and caret, and
   prints the suggestion separately.

The two produce different output for the same error. `CompileError`
also carries structured fields (`severity`, `label`, `secondary`,
`notes`) that neither path consumes — they exist for the planned
Phase 3 renderer.

**Not a bug.** The two paths are consistent in *user-visible*
behavior today: `algol26 check` / `run` show the full format;
`format!("{}", err)` shows the plain message + suggestion. A test
author chooses the appropriate path for their purpose.

**Deferred work.** A `rustc-style renderer` would:

- Consume every structured field on `CompileError`.
- Render `severity` as `error:` / `warning:` / `note:`.
- Render `error_code` as `error[E####]:`.
- Render `file:line:col` with the source line and caret.
- Render `label` as the primary span label.
- Render each `secondary` span as a supplementary `--> file:line:col`
  with its own source line.
- Render `notes` as `= note: ...`.
- Render `suggestion` as `= help: ...`.

The output format matches Rust's compiler diagnostics. Once the
renderer exists, `Display` for `CompileError` should delegate to
it, and the CLI should also delegate rather than rolling its own.

**Why deferred.** The source-line-and-caret rendering requires
careful handling of:
- Tab expansion (how many columns does a tab occupy?)
- Multi-byte UTF-8 codepoints (a 4-byte codepoint occupies one
  "column" for the caret, not four).
- Column offsets from `Span.start_column` (which may not be
  1-based in all cases).
- Consistent behavior between `--emit llvm`, `--interpreter`, and
  the standard `check` output.

Each of those is a small ADR. Rushing the renderer to close G31
would leave the format inconsistent with the CLI's current output
and lock in decisions before they're considered.

**Phase 3 scope.** Tracked here so the gap log records the deferral
and the fields on `CompileError` aren't mistaken for dead code.
The Phase 3 session that adds the renderer should:
- Write an ADR for the renderer format.
- Move the CLI's error formatting into the renderer.
- Update `Display` to delegate.
- Add tests for each structured-field rendering.
- Add tests for edge cases (tabs, UTF-8, missing location).

**Status:** Deferred to Phase 3. Not blocking any current work.


### G29 — resolution: warning (W0001) shipped (2026-11-02)

**Warning emitted, configurable, deniable.** The behavior fix
(mixed-Void match yields Void) shipped in `ad08a53`. The warning
that surfaces the discard — which was deferred because it required
building warning infrastructure — is now complete (ADR 0063).

**What works end-to-end:**

- `algol26 check prog.gol` renders
  `warning[W0001]: non-Void arm value discarded in mixed-Void match`
  with a span at the whole match expression, and a suggestion.
  Exit 0; compilation continues.
- `algol26 check --no-warn=W0001 prog.gol` suppresses the warning.
  Exit 0.
- `algol26 check --deny-warnings prog.gol` promotes the warning
  to `error[W0001]: ...` and exits 1. `--deny-warnings` uses a
  `"*"` wildcard in `WarningConfig.denied` (deny all codes
  without knowing the catalog).

**Infrastructure delivered (ADR 0063):**

- `CompileWarning` type (distinct from `CompileError`; ADR 0063
  §Decision 1).
- `WarningConfig` with `is_enabled`, `is_denied`, `"*"` wildcard.
- `SemanticAnalyzer.warnings` + `warning_config` fields +
  `take_warnings()` + `with_warning_config` builder.
- `TypedProgram.warnings` (structural truth for the program).
- `CompilerContext.warnings` (pass-to-scheduler channel).
- `ScheduleOutcome.warnings` (aggregated across passes).
- `CompilerConfig.warning_config` (threaded to the analyzer and
  the denial check).
- `CompileError.code_override: Option<String>` — renders
  `error[W0001]:` / `warning[W0001]:` instead of `error[E0002]:`.
  Added in patch 7.5 as the mechanism for the ADR §Decision 7
  rendering promise.
- CLI flags `--no-warn=CODE[,CODE]` and `--deny-warnings`.
- Driver renders warnings via `render_one_with_source`, with
  correct severity and code.

**Deferred from ADR 0063 (unchanged):**

- Source-level suppression (`#[allow(...)]`). Requires attribute
  syntax; separate feature.
- Cross-pass span ordering (currently warnings grouped by pass
  completion, not interleaved by span).
- Warning code categories (`W1xxx` vs `W2xxx`). Sequential codes
  until the catalog grows.
- JSON output for CI/IDE.

**Divergences from ADR 0063 as originally proposed (all
documented in ADR 0063 §Implementation notes):**

- **Patches 5 and 9 merged.** Span selection for W0001 requires
  saving the match's span at the *start* of the `Match` arm (not
  `self.current_span` at emission time, which by then points at
  the last arm's last sub-expression). Patch 5 alone would emit
  with the wrong span; patch 9 alone would be dead code. Merged
  into one patch, `db9118c`.
- **Patch 7 split.** Threading `warning_config` to the analyzer
  (patch 7, `c2f4c4e`) is distinct from the driver's denial
  check + CLI wiring (patch 8, `cbb45f2`). Splitting kept each
  patch testable in isolation.
- **New patch 7.5.** `CompileError.code_override` was not in the
  original ten-patch plan. Adding it made §Decision 7's
  `error[W0001]:` rendering promise implementable without
  extending the `ErrorCode` enum. `f70a452`.
- **`WarningConfig.is_denied` gained `"*"` wildcard.** `--deny-warnings`
  needs to deny *every* warning without knowing the catalog in
  advance. One-line addition to `is_denied`.

**Regression coverage:** `tests/integration/warning_system.rs`
(7 tests):
- `warning_w0001_fires_by_default`
- `warning_w0001_suppressed_by_no_warn_config`
- `warning_w0001_denied_fails_compilation`
- `warning_config_wildcard_denies_all`
- `warning_config_specific_denial_leaves_others_enabled`
- `no_warning_for_non_mixed_void_match`
- `no_warning_for_all_value_match`

**Status:** Complete. G29 has both behavior (ad08a53) and warning
(this entry). ADR 0063's proposed design is implemented; divergences
are recorded in the ADR's implementation notes.


---

## G33 — tree-sitter grammar: keyword scopes incomplete

The tree-sitter grammar in `tree-sitter-algol26/` does not tokenize
several keywords: `pub`, `and`, `or`, `not`, `self`, `Self`.
`grammar.js` parses them as identifiers, and `queries/highlights.scm`
has no captures for them.

**Impact:** tree-sitter-based editors (Neovim, Helix, Zed, VS Code
via LSP, GitHub's code view) render these in the default text color.
**Sublime Text is unaffected** — it uses
`editors/sublime/ALGOL26.sublime-syntax` (patched 2026-11-02).

**Not a compiler bug.** The compiler's lexer and parser handle all
of these correctly.

**Fix scope:**

1. Add `pub`, `and`, `or`, `not`, `self`, `Self` as keyword
   productions to `grammar.js`.
2. Regenerate `src/parser.c` and `src/node-types.json` via
   `tree-sitter generate`.
3. Add captures to `queries/highlights.scm`.

**Effort:** ~1 hour (grammar edit + regenerate + queries + test
in a tree-sitter-aware editor).

**Status:** Open. Non-blocking. Deferred until the tree-sitter
grammar is touched for other reasons.

---

## G35 — no regex engine (ergonomic, not a gap)

`intent-data.ts` uses six anchored alternation regexes, all of the
form `/^(a|b|c)$/i`:

| Intent     | Pattern                                       |
|------------|-----------------------------------------------|
| flexbox    | `/^flexbox$/i`                                |
| abs        | `/^(absolutely\|abs)$/i`                     |
| rel        | `/^(rel\|relatively)$/i`                     |
| hidden     | `/^(hidden\|invisible)$/i`                   |
| full       | `/^(full\|fullscreen\|full-screen)$/i`      |
| rounded    | `/^(rounded\|round)$/i`                      |

ALGOL26 has no regex engine. Every one of these is "the whole
string, lowercased, equals one of {a, b, c}" — a set-membership
check, not a regex match. The port replaces each with an
equality-against-list on the lowercased input. Behaviorally
identical; no language work needed.

`intent-normalizer.ts` also uses:
- `/[A-Z]/g` (uppercase detection, with an offset-aware replacement
  callback in `toUnprefixedKebab`)
- `/^-?(?:webkit|moz|ms|o)-/` (vendor prefix strip)
- `/^[+-]?(?:\d+\.?\d*|\.\d+)(?:e[+-]?\d+)?$/i` (numeric-string test)

and `layout-macros.ts` uses:
- `/(\d+)[,\s]+\s*(\d+)[,\s]+\s*(\d+)/` (RGB triple)
- `/hsl[a]?\(\s*\d+deg?[,\s]+\s*[\d.]+%?[,\s]+\s*([\d.]+)%/` (HSL)

Each is hand-ported as a linear scanner. See G36 for the
`autoContrast`-specific pair; the others are in `unit_normalizer.gol`
and are noted at the call sites.

**What would turn this into a real gap:** a normalizer that needs
lookahead, backreferences, or unbounded quantifiers. None of the
Phase-5 sources do. If one appears in Phase 6+, that is the trigger
to design a pattern type (or vendor a small regex engine). Until
then, hand-porting is the correct shape — it keeps the port free of
a regex runtime that only six patterns need.

**Status:** Ergonomic note. No compiler change.


## G36 — regex-heavy color parsing in `autoContrast` (mechanical)

`layout-macros.ts`'s `autoContrast` parses `rgb()` / `hsl()` strings
with two regexes:

```
/(\d+)[,\s]+\s*(\d+)[,\s]+\s*(\d+)/
/hsl[a]?\(\s*\d+deg?[,\s]+\s*[\d.]+%?[,\s]+\s*([\d.]+)%/
```

Both are **linear-scan patterns** — no lookahead, no backreferences,
fixed repeat count (three captures in the first, one in the second).
Hand-ported as ~25-line and ~20-line matchers in `layout_macros.gol`.
`NAMED_COLORS` (17 entries) moves to `data/named_colors.json`; the
matcher loads the map at init.

This is the same shape as G35 but called out separately because
the port is larger (~45 lines of scanner) and because it is the
only place in Phase 5 that parses *color syntax*, which Phase 6's
`accessibility-optimizer` will also need. If `accessibility-optimizer`
adds more color-syntax parsing, the right move is to extract a
shared `parse_css_color` in `chaincss/color_parse.gol`, not to
duplicate scanners. Note for Phase 6.

**Verification:** 5 RGB strings, 3 HSL strings, 17 named colors,
plus the empty / `var(...)` / `transparent` early-returns. Golden
file `tests/chaincss/auto_contrast_parity.gol` covers them.

**Status:** Ergonomic note. No compiler change.


## G37 — union-typed `normalizeRule` casts cannot be ported structurally

Both `unit-normalizer.ts` and `intent-normalizer.ts` define a
recursive helper with a union-typed parameter:

```ts
function normalizeRule(rule: IRRule | IRAtRule) {
  if (!rule || ("isDead" in rule && rule.isDead)) return;
  // ...
  if ("pseudoClasses" in rule && rule.pseudoClasses) { ... }
  if ("atRules" in rule && rule.atRules) { ... }
  // ...
}
```

and both cast `IRPseudoClass` and `IRKeyframeFrame` to `IRRule`:

```ts
normalizeRule(pseudoClasses[i] as unknown as IRRule);
normalizeRule(keyframes[j] as unknown as IRRule);
```

The casts work at TS runtime because `IRPseudoClass` and
`IRKeyframeFrame` simply **don't have** the fields
(`isDead`, `pseudoClasses`, `nestedRules`, `atRules`), so the
`in`-guards are false and the branches are skipped. The pseudo-
class / keyframe case therefore normalizes declarations but does
**not** recurse — a behavior the type system does not express.

ALGOL26 types are nominal; `as unknown as IRRule` has no
equivalent. The port must make the split explicit:

- `normalize_rule(rule: &mut IRRule)` — full recursion (checks `is_dead`,
  walks pseudoClasses / nestedRules / atRules).
- `normalize_at_rule(at: &mut IRAtRule)` — no `is_dead` check, walks
  nestedRules and keyframes.
- `normalize_declarations_only(decls: &mut List[IRDeclaration], ...)` —
  used for pseudo-class bodies and keyframe frames.

Three functions instead of one. **Behaviorally equivalent** — the
union-typed TS version only branches on `"isDead" in rule`,
`"pseudoClasses" in rule`, and `"atRules" in rule`, each of which is
a static property of the concrete type at runtime.

**Cost:** every Phase-5+ normalizer that recurses will need the same
three-way split. `intent-normalizer.ts`, `unit-normalizer.ts`,
`token-lowering.ts`, `scroll-timeline.ts`, `specificity-sorter.ts`,
`dead-code-eliminator.ts` all have the same shape. That is six files
× three functions = eighteen functions instead of six. Worth logging
because it is a recurring cost of nominal typing over structural
typing, not a one-off.

**What would make this a real language gap:** if a future normalizer
needs to iterate over a heterogeneous list of IR nodes (e.g. a
`List[IRNode]` where `IRNode = IRRule | IRAtRule | IRPseudoClass`).
None of the Phase-5 sources do; they recurse over homogeneous lists
(`nestedRules: List[IRRule]`, `pseudoClasses: List[IRPseudoClass]`).
If that changes, ALGOL26 needs a tagged-node form (a union of
records) or the port needs a `NodeKind` enum + dispatch. Defer.

**Status:** Accepted ergonomic cost. No compiler change.


## G38 — module-level mutable state has no natural ALGOL26 form

Four Phase-5 sources use module-level mutable singletons:

| Source                   | Module-level state                                       |
|--------------------------|----------------------------------------------------------|
| `intent-detector.ts`     | `correctionCache: Map`, `customKeys: Set`                |
| `intent-data.ts`         | `SEMANTIC_INTENTS`, `VALUE_CORRECTIONS`, `KNOWN_PROPERTIES`, `customKnown`, `customKnownProperties`, `propertyCache` |
| `layout-macros.ts`       | `LAYOUT_MACROS`, `contrastCache`                         |
| `scroll-timeline.ts`     | `animCounter`, `entangleCounter`, `kebabCache`           |

Each is initialized at module load, mutated by `register*` /
`reset*` / `invalidate*` functions, and consumed by the pass entry
point. The TS module system gives them process-global lifetime for
free.

ALGOL26 has no module-level `var` (verify against `src/chaincss/
config.gol` and `diagnostics.gol` — both are pure or state-record
based, neither uses module-global mutable state). The port threads
state records explicitly:

- `IntentEngine { corrections_cache, custom_keys, value_corrections,
  known_properties, builtin_known, semantic_intents }`
- `LayoutMacroRegistry { macros, contrast_cache }`
- `ScrollCompiler { counter }`

Each pass entry point takes `&mut Engine` as its first parameter.
`register*` / `reset*` become `proc` with `&mut Engine` receivers,
matching the `DiagnosticReporter` pattern already established in
`diagnostics.gol`.

**Two consequences the diff harness must know about:**

1. **Cache eviction order.** TS evicts `correctionCache.keys().next()`
   — insertion order. ALGOL26's `Map` iterates lexicographically
   (G6). The eviction victim will differ. Cache eviction is
   observationally inert (performance only), so the comparator
   normalizes it away. Not a parity risk, but note it in
   `run_normalizer_parity.sh`'s normalization step so a future
   reader doesn't chase it.

2. **`register*` call ordering.** The TS pipeline calls
   `registerCustomIntentKeys`, `invalidateIntentCache`,
   `registerCustomKnownProperties`, `resetLayoutMacros`, etc.
   between phases. The ALGOL26 port must call the same functions
   on the same engine instance in the same order. The diff harness
   exercises these paths explicitly (see `normalizer_parity.gol`'s
   `--stage=intent` mode which calls `register_custom_keys` before
   normalization).

**What would make this a real language gap:** if Phase 6's
`pass-scheduler.ts` requires pass instances to be
shared-and-mutated across scheduler ticks without an explicit
owner. The current Phase-5 sources all have an explicit caller
(`intentNormalizer.normalize(ir)`), so threading works. Phase 6
should be checked before assuming the same.

**Status:** Accepted ergonomic cost. Port pattern established
(`IntentEngine`, `DiagnosticReporter`). No compiler change.

## G39 — `match` does not auto-deref `&Union` (ergonomic, not a gap)

**Symptom.** Matching on a reference to a union fails:

    pub proc json_is_number(v: &Json) -> Bool
        match v
            case NumInt(_) then { return true }
            ...

produces:

    error[E0002]: Variant pattern 'NumInt(..)' requires a union type,
    found Borrow<Json>
      --> src/chaincss/json_ext.gol:18:11

**Cause.** `v: &Json` has type `Borrow<Json>`. Variant patterns
(`NumInt(_)`, `Obj(inner)`, …) require the **union value**, not a
reference to it. `match *v` dereferences and matches on the value.

**Consequence — and this is the ergonomic cost.** Every `pub proc`
that takes `v: &Json` (or `&Map`, `&List`, …) and matches on it
must be written as `match *v`. Once matched, the bound variable is
the moved-out value, not a reference — so the body uses `n`, `s`,
`inner` directly (`0.0 + n`, not `0.0 + *n`).

**Why this isn't a language bug.** Auto-deref on `match` would have
to choose between move and borrow semantics per-arm, which is exactly
the ambiguity Rust's match ergonomics are designed to avoid. ALGOL26's
"explicit `*` at the match site" is a defensible design — it just
costs one character per match.

**Cost across Phase 5.** Every normalizer that pattern-matches on
`Json` / `Option[Json]` / `Map` values will need `match *v`. In
`json_ext.gol` alone it was six call sites. In `unit_normalizer.gol`
it will be every declaration value check. In `intent_normalizer.gol`
it will be every `passMeta` unwrap. The pattern is mechanical and
the fix is always the same.

**Pattern for future port files.** When a proc takes `v: &T` and
matches on `v`, write `match *v` and use the arms as if `v` were
`T`. If the arm binds a variable (`case NumInt(n) then`), use `n`
as a value, not `*n`.

**Status:** Accepted ergonomic cost. No compiler change planned.

## G40 — `Option[T] = None` doesn't infer (ergonomic, not a gap)

**Symptom.**

    if m.get(k) = None then { m.insert(k, v) }

produces:

    error[E0002]: Equality requires matching types,
    found Option<Json> and Option<Unknown>

**Cause.** `m.get(k)` has type `Option[Json]`. The bare `None` in the
comparison has type `Option[Unknown]` — the type parameter is not
inferred from the left-hand side. Equality then fails.

**Fix — use `match`, not equality.**

    match m.get(k) {
        case Some(_existing) then {}
        case None then { m.insert(k, v) }
    }

This is the idiom `config.gol`, `ir_types.gol`, and `diagnostics.gol`
already use everywhere else. The `= None` form was a one-off in the
`declaration_meta_to_json` extensions merge.

**Why this isn't a language bug.** Equality against `None` would
need bidirectional type inference to pick `Option[Json]` for the
right-hand side. ALGOL26's inference is left-to-right; the left side
gives the right side nothing. Match-based unwrapping is more verbose
but more predictable — and it composes with the `match *v` deref
pattern (G39).

**Pattern for future port files.** Never write `if x = None` or
`if x = Some(_)`. Always use `match x { case Some(...) then ...;
case None then ... }`. If the arm body is one statement, it's a
one-line match — not noticeably longer than the equality form, and
it compiles.

**Status:** Accepted ergonomic cost. No compiler change planned.

## G34 — `Float.to_string` diverges from JS `Number.prototype.toString`

**Confirmed divergence, not a theoretical one.** `Float.to_string`
(per `src/builtins.rs`'s signature table comment) is *"`%.6f` with
trailing zeros stripped from the fractional part"*. That is not
JS's `Number.prototype.toString`, which uses the shortest
round-trip representation.

Measured on ALGOL26 v0.8.0 (interpreter), 2026-11-08:

| Value   | ALGOL26               | JS              | Diverges? |
|---------|-----------------------|-----------------|-----------|
| 12.0    | `12`                  | `12`            | no        |
| 0.1     | `0.1`                 | `0.1`           | no        |
| 1.5     | `1.5`                 | `1.5`           | no        |
| 1e21    | `1000000000000000000000` | `1e+21`       | **yes**   |
| 1e-7    | `0`                   | `1e-7`          | **yes**   |
| -0.0    | `0`                   | `0`             | no        |

Two distinct failures:

1. **Format shape** (`1e21`): JS uses exponential notation above
   `1e21` and below `1e-7`; ALGOL26 always uses fixed. Cosmetic
   but byte-diffing.
2. **Precision loss** (`1e-7 → 0`): `%.6f` rounds anything with a
   magnitude below `0.0000005` to zero. For CSS this is
   inconsequential (no real CSS value has more than ~4 decimal
   places), but it's a **precision loss**, not just formatting.

**Impact on Phase 5 (normalizer).** None. The unit-normalizer
either appends `"px"` to an `Int` (via `Int.to_string`, exact) or
appends it to a **string** that already matched a numeric regex
(also exact — string concat). The float → string path is not
exercised by the Phase-5 normalizers. Phase 5 will pass without a
fix.

**Impact on Phase 7 (emitter).** Blocking for byte-for-byte parity.
If any declaration's final value is a non-integer float that came
through the numeric path, the emitter must render it with JS's
shortest round-trip.

**Two options for Phase 7:**

- **(a) Add `f64_to_js_string` builtin.** Port ECMA-262 §7.1.12.1
  (Number::toString) exactly. ~150 lines. Well-specified; the
  algorithm is in the spec and has reference implementations
  (V8's `DoubleToString`, SpiderMonkey's `NumberToCString`,
  Rust crate `ryu` for shortest-round-trip + manual exponent
  adjustment). Unblocks Phase 7 without a comparator workaround.

- **(b) Normalize floats in the comparator.** Compare
  structurally, parse both sides' floats, compare as `f64`. Loses
  byte-for-byte property but unblocks Phase 7.

**Recommendation:** (a), before Phase 7. The `%.6f` behavior is
also a **correctness** issue for any consumer of `Float.to_string`
beyond the emitter, and `%.6f`-with-strip is a worse default than
JS-exact in every non-CSS use too. Consider the fix as both a
language improvement and a port unblocker.

## Resolution (2026-11-12)

**Status:** Interpreter fixed. LLVM/WASM divergence remains, deferred to
Phase 7 prep.

### Fixed (interpreter)

`src/backends/interpreter/eval.rs::format_float_for_display` now
implements ECMA-262 §7.1.12.1 (Number::toString).

**Scope note: `print(float)` is not affected by G34 and is not a
bug.** `print(float)` uses `common::types::print::format_float`,
which formats with `{:.1}` (one decimal place) — deliberately. The
interpreter and LLVM/WASM backends already agree on this rule
(`llvm_format(&Type::Float) == "%.1f\n"`), and
`common::types::print::tests::float_has_one_decimal_place` pins
it. `Float.to_string` and `print` are two different operations
with two different contracts; G34 changes only the former.

Likewise, `frontend/formatter/emit.rs::format_float` formats
source literals for `algol26 fmt` (preserves `5.0` as `5.0`,
writes `1.5` as `1.5`) and is out of G34's scope.

The only remaining G34 site is `Float.to_string` on LLVM/WASM
(below). The
implementation reuses `format!("{:?}", f)` for shortest-round-trip
digits and re-emits them per the spec's fixed-vs-exponential rule
(`< 1e-6` or `>= 1e21` uses exponential; `e+` / `e-` with no
zero-padding; `NaN`, `Infinity`, `-Infinity` capitalized per JS;
`±0.0` renders `"0"`).

Verified against a Node.js 22 oracle over 10,000 uniformly-random
f64 samples plus a hand-picked regime battery:

- `tests/float_to_string_oracle.rs` — the test
- `tests/float_to_string/oracle.json` — 10,000 `(hex, expected)` pairs
- `tools/gen_float_oracle.js` — the regeneration script (run with
  `node`, requires a recent V8; committed output makes the test
  hermetic)

Accepted divergences: 2/10,000.

### Known divergence (~0.01-0.02% of random samples)

Rust's `{:?}` and V8's ECMA-262 shortest-round-trip implementations
disagree on an exact-tie pattern: when two same-length decimal
representations both round-trip to the same f64 (an exact decimal
midpoint), Rust resolves the last-digit tie toward the value closer
to the binary mantissa (odd last digit), while V8 uses
round-half-to-even (even last digit).

Empirically: 1-2 cases per 10,000 uniformly-random f64 samples,
always the same direction (Rust's last digit = V8's last digit + 1,
Rust's last digit is odd). Sample from several regenerations:

- `2155494342014890.3` vs `2155494342014890.2`
- `776647748525147.3` vs `776647748525147.2`
- `233305898117001.13` vs `233305898117001.12`
- `1921566535247357.3` vs `1921566535247357.2`
- `183763293141550.13` vs `183763293141550.12`

The affected magnitude range is roughly 10^14 and above (below that,
shortest-round-trip digits diverge too rarely for the tie pattern to
appear in 10k random samples). No CSS value reaches 17-significant-
digit precision at any magnitude, so the divergence never affects
the port's output.

The oracle test (`tests/float_to_string_oracle.rs`) tolerates this
pattern generically rather than via a fixed hex skip-list:

- same digit-string length
- exactly one byte differs
- both differing bytes are ASCII digits
- `Rust's digit == V8's digit + 1` and Rust's digit is odd

Any failure outside that pattern fails the test — so the tolerance
is narrow enough to catch real formatter bugs while accepting the
documented tie-break. The oracle JSON can be regenerated
(`node tools/gen_float_oracle.js`) without needing to edit the
test's tolerances.

Fixing the divergence outright requires the `ryu-js` crate (a `ryu`
fork matching V8's tie-break) or decimal-midpoint detection in
`parse_shortest`. Deferred to a future session; not Phase-6- or
Phase-7-blocking.

### Not fixed (LLVM/WASM)

`src/backends/llvm_codegen/builtins.rs:2238` still emits
`sprintf("%.6f")` + `algol26_float_to_string_strip`. This diverges
from the interpreter on any value outside the `%.6f`-faithful range
(magnitude < 1e-6 or >= 1e21) and any value whose shortest-round-
trip differs from 6-decimal truncation. `1/3` is an example: the
LLVM/WASM backends still produce `"0.333333"` where the interpreter
now produces `"0.3333333333333333"`.

Closing this requires establishing a build path for `runtime/*.c`
(nothing compiles them today — no `build.rs` reference, no CI step,
no makefile rule; the LLVM codegen declares the symbols via
`add_function` but the C bodies are unreferenced), then adding
`runtime/float.c` with the same algorithm. Alternative: a
`build.rs`-linked Rust `extern "C"` helper called by both backends.

Deferred to Phase 7 prep. The differential harness will surface
the divergence when both backends run on the same fixture; the
`float_to_string.gol` conformance test currently declares
`// BACKEND: interpreter llvm wasm` and asserts identical output
across all three, so it **will** fail on the LLVM/WASM legs until
this is fixed. The fixture has been annotated to reflect that (see
below). Closing the LLVM/WASM side is a hard prerequisite for
enabling that fixture on both backends again.

### Workaround for Phase 6/7 ports

None needed for interpreter parity: the interpreter matches JS for
all CSS-relevant values. When exercising the LLVM backend on a
fixture that calls `Float.to_string`, expect divergence on
`1/3`-style values.

## G41 — `&mut list[i]` rejected for non-literal index (FIXED)

**Symptom.** In a loop over a list, taking a mutable borrow
of an element by variable index failed:

    var i := 0
    while i < xs.length() {
        mutate_first(&mut xs[i])          // E0007
        i := i + 1
    }

produced:

    error[E0007]: Cannot take a mutable borrow of this expression:
    mutable borrows require an addressable place (a variable,
    a field, or a list element with a literal index)

`&mut xs[0]` (literal) worked; `&mut xs[i]` (variable) did not.
This blocked **every** in-place list-mutation loop, and therefore
every Phase-5+ normalizer that walks a `List[IRDeclaration]` or
similar by index.

**Root cause.** `place_of_expr` in
`src/semantics/analyzer/ownership.rs` (around line 453) required
a literal `Int` index in the `ArrayAccess` arm and returned `None`
for any other index expression. Its comment even said
*"we bail and let the caller use the base name instead"* — but
**the callers did not do that.** Both `expr.rs` borrow arms
(lines 173 shared, 227 mutable) interpret `None` as "not an
addressable place" and emit E0007. The documented fallback was
never wired up.

The `stmt.rs` write-through-borrow path (`Stmt::FieldAssign` /
`Stmt::ArrayAccessAssign`, around line 550) already used the
correct conservative fallback: non-literal index → whole-list
place. That path **worked** for writes because it hand-rolled the
fallback rather than relying on `place_of_expr`.

**Fix.** `src/semantics/analyzer/ownership.rs`:

    ExprKind::ArrayAccess { array, index, .. } => {
        let mut base = self.place_of_expr(array)?;
        match &index.as_ref().kind {
            ExprKind::Int(n, _) => {
                if *n < 0 {
                    return None;   // negative literal: still rejected
                }
                base.projections.push(Projection::Index(*n as usize));
            }
            _ => {
                // Non-literal index: whole-list conservative place.
                // No projection pushed; two `&mut xs[i]` / `&mut xs[j]`
                // with variable i, j overlap by `Place::overlaps`.
            }
        }
        Some(base)
    }

Negative literal indexes (`xs[-1]`) still bail to `None`, preserving
the existing compile-time rejection of out-of-bounds literal indexes.

**Semantics.** Non-literal indexes now borrow the **whole list**.
Two `&mut xs[i]` / `&mut xs[j]` (where i, j are variables) are
treated as overlapping and the second is rejected — conservative
but sound. `&mut xs[0]` and `&mut xs[1]` (literal indexes) remain
**disjoint** — precise, matching the existing behavior. This is
the exact model `stmt.rs` already uses for writes.

**Verification.**

- `probe_mut_index.gol` — `&mut xs[0]` and `&mut xs[i]` both pass.
- `probe_overlap.gol` — `&mut xs[i]` + `&mut xs[j]` (non-literal)
  correctly rejected with "Cannot mutably borrow 'xs' while it is
  mutably borrowed"; `&mut ys[0]` + `&mut ys[1]` (literal) passes.
- `cargo test` — full suite green (soundness suite, verifier
  invariants, all borrow-checker tests).

**Port impact.** The natural form

    val corr := normalize_one_declaration(&mut decls[i], unitless)

is now accepted by the analyzer for `decls: &mut List[IRDeclaration]`
**modulo one remaining issue** — see G42 for that follow-up.

**Status:** Fixed. Six-line change in `place_of_expr` plus updated
comment. Regression tests added: `/tmp/probe_mut_index.gol` and
`/tmp/probe_overlap.gol` should be promoted to `tests/` before
Phase 5 completes.

## G42 — interpreter rejects `&mut list[i]` for non-literal index (FIXED)

**Symptom.** After G41 relaxed the analyzer to accept `&mut list[i]`
with a variable index, the same code failed at *runtime*:

    Runtime error: interpreter does not support `mutable reference
    to non-addressable expression`: bind the value to a `var` first,
    then take a `&mut` of the binding

**Root cause.** G41 fixed `place_of_expr` in
`src/semantics/analyzer/ownership.rs` — but the *interpreter* has
an independent mirror of the same logic,
`place_of_ir_value` in `src/backends/interpreter/eval.rs`
(around line 542), and it still required a literal `Int` index:

    TypedIRValue::ArrayAccess { array, index, .. } => {
        let idx = match index.as_ref() {
            TypedIRValue::Int(n) => *n,
            _ => return None,        // ← same restriction as pre-G41
        };
        ...
    }

The `BorrowMutable` arm calls `place_of_ir_value(expr)`, gets
`None`, and returns `EvalError::Unsupported`.

**Fix.** Added `place_of_ir_value_precise` in `eval.rs` — a
variant that *evaluates* the index expression when it is not a
literal, obtaining a concrete `usize`. Used it in the
`BorrowShared` / `AddrOf` and `BorrowMutable` arms.

The semantics differ from the analyzer's fix, deliberately:

- **Analyzer** (G41): drop the projection, whole-list conservative
  place. Used for **overlap checking** at compile time. Two
  `&mut xs[i]` / `&mut xs[j]` with variable `i`, `j` overlap; the
  second is rejected.

- **Interpreter** (G42): evaluate `index` to a concrete value at
  borrow time, store `[Index(<value>)]` in the `Place`. Used for
  **read/write** at runtime. If the program passed analysis (only
  one `&mut xs[i]` live at a time), we know `i`'s value exactly;
  the interpreter can be precise.

This matches Rust: `&mut xs[i]` takes the address of *one element*
at borrow time. If `i` later changes, the reference still points
at the original element. Capturing `i`'s value at borrow time
matches that.

**Untouched:** the existing `place_of_ir_value` remains for
`Map.insert` and `List.append` receivers, which are `Variable` or
`FieldAccess` (no indexed receivers in the current code). A future
builtin needing an indexed receiver gets the same fix pattern.

**Verification.**

- `probe_deref_list.gol` — `&mut (*xs)[i]` where `xs: &mut List`
  inside a `while` loop, called once per element — passes and
  produces `[2, 3]` after incrementing.
- `probe_still_rejected.gol` — two `&mut xs[i]` / `&mut xs[j]`
  with variable `i`, `j` — still rejected by the analyzer
  ("Cannot mutably borrow 'xs' while it is mutably borrowed").
  The G41 conservative overlap rule holds; G42 did not weaken it.
- `cargo test` — full suite green (soundness suite, verifier
  invariants, borrow-checker tests).

**Port impact.** `unit_normalizer.gol`'s natural form

    val corr := normalize_one_declaration(&mut (*decls)[i], unitless)

now compiles **and** runs. Before G41+G42, the same code required
the copy-out/copy-back workaround. Every Phase-5+ normalizer that
walks a `List` by index and mutates in place is now unblocked.

**Discovery.** Found while running the Phase-5 unit-normalizer
differential harness (`run_unit_parity.sh`). The analyzer accepted
the code; the interpreter rejected it at the first `&mut (*decls)[i]`.
G41's fix was necessary but incomplete — the interpreter mirrors
the analyzer's place logic and needs the same treatment.

**Methodology note.** This is the second case in the G-series
where a fix in one layer (analyzer) exposes the identical bug in a
parallel layer (interpreter). The pattern: **when fixing a check
in the analyzer, grep for the same logic in the interpreter.**
`place_of_expr` (analyzer) and `place_of_ir_value` (interpreter)
are siblings; both needed the same relaxation. The verification
pattern — probe, build, run, compare to expected — was the same
for both.

**Status:** Fixed. `eval.rs` gains `place_of_ir_value_precise`;
two borrow arms updated. Probes and full test suite green.

## G43 — `Map` loses insertion order; JSON objects cannot preserve source order

**Symptom.** When comparing the ALGOL26 unit-normalizer's output
against the TS reference on `card.chain.ts`, the declaration order
diverges:

    TS:  display, flex-direction, gap, padding, background,
         border-radius, box-shadow     (source order)

    GOL: background, border-radius, box-shadow, display,
         flex-direction, gap, padding  (alphabetical)

Same seven declarations, same values, different order. The
alphabetical order is `Map.keys()` — the interpreter sorts keys
lexicographically (G6).

**Impact — and why this is not cosmetic.** For CSS, declaration
order is semantically significant: a later `color: red` overrides
an earlier `color: blue`. The ChainCSS emitter iterates declarations
in source order to produce byte-identical CSS. Alphabetical order
would emit them in the wrong sequence, changing the CSS output.

- **Phase 5 (normalizer):** order-insensitive. The normalizer
  transforms values in place; order is preserved except through the
  `Map` representation. Phase-5 parity can be checked with
  order-normalized comparison.
- **Phase 7 (emitter):** order-sensitive and blocking. Byte-for-byte
  parity with TS requires source order.

**Root cause.** `Map<K, V>` in ALGOL26 is a HashMap. `Map.keys()`
returns sorted keys (interpreter's choice). `Json.Obj(Map<String, Json>)`
therefore loses the key order of any parsed JSON object. `parse_ir.gol`
iterates `Map.keys()`, so the source order of `card.chain.ts`'s
declarations is gone by the time the IR is built.

TS's `Object` and `JSON.parse` both preserve key insertion order;
`Object.keys()` returns keys in the order they were first set. The
ChainCSS pipeline relies on this at the emitter.

**Two fix options (both are compiler-side):**

**(a) Add an ordered-map primitive.** `Map<K, V>` gets an optional
insertion-order list, or a separate `OrderedMap<K, V>` type is
added. `parse()` produces `OrderedMap` from JSON objects;
`parse_ir.gol` iterates in insertion order. Cost: touches every
`Map` consumer if added to `Map`, or a parallel type if separate.

**(b) Extend `Json` with an ordered object variant.**
`Json.Obj` gains a sibling `Json.OrderedObj(List[(String, Json)])`;
`parse()` emits the ordered form; consumers that don't care
iterate the list, consumers that do care get source order. Cost:
every `match json` arm needs a new variant, but the impact is
contained to `json.gol` and its direct consumers.

**(a)** is cleaner long-term (ordered maps are generally useful).
**(b)** is more surgical (JSON is where order matters, so scope
the fix to JSON).

**Recommendation:** (a) if the codebase already has a use for
ordered maps beyond JSON; otherwise (b), because it scopes the
change to the actual problem.

**Interim state (Phase 5).** The differential comparator
(`compare_normalized.py`) sorts both `declarations` (by property)
and `corrections` (by property + original) before comparison.
**This is a temporary measure pending G43.** Once the compiler fix
lands, both sorts are removed and the structural comparison becomes
order-sensitive, matching TS's semantics exactly.

Phase 5's unit-normalizer passes 4/4 with these sorts. The `ir`
object matches structurally even without the `declarations` sort
(the values were never wrong); only the array order diverges.
The `corrections` array needs the sort because the corrections
naturally emerge in source order from TS and alphabetical order
from the ALGOL26 side — the same root cause (Map key iteration).

**Discovery.** Found while running Phase 5's unit-normalizer
differential harness (`run_unit_parity.sh`). The harness's first
real comparison surfaced the divergence. Phase 2D's `parse_ir.gol`
verification was structural and declaration-order-insensitive by
G6's own note ("Phase 2D comparison must be structural, not
byte-for-byte"), so the divergence wasn't caught there. It becomes
observable only once we compare against the TS oracle field-for-field.

**Status:** Open. Compiler fix required for Phase 7. Phase-5 parity
proceeds with order-normalized comparison.

**Methodology note.** This is the first gap in the G-series where
the fix is a *feature addition* rather than a *bug fix*. The
distinction matters:

- **Bug fix:** the compiler does something wrong. Fix it. Ship
  with a regression test. Examples: G9, G11, G19, G41, G42.
- **Feature addition:** the language lacks a capability required
  by a real workload. Design it (ADR), implement it, test it.
  Examples: G1 (index signatures — partially deferred), G23
  (JS execution — deferred), G43.

Feature additions are not blockers for the current phase (Phase 5
can proceed with the comparator workaround) but they are
prerequisites for the phase that actually needs them (Phase 7
needs G43). The roadmap should note this: **G43 must land before
Phase 7 starts.**

**Status (2026-11-12):** ADR 0066 (`docs/decisions/0066-json-ordered-object.md`)
resolves this gap. Decision: add `Json.OrderedObj(List[(String, Json)])`
as a sibling of `Json.Obj`; the JSON parser emits `OrderedObj`;
`parse_ir.gol` iterates its entries in source order. ADR 0027 is
untouched — `Map` remains a hash map with unspecified order.

Construction-site measurement: `grep -rn "Json.Obj(" src/chaincss/*.gol
| wc -l` reports **76** as of 2026-11-12. Not small enough for
replace-to-be-trivial, not large enough for coexist-to-be-permanent.
Both variants coexist in v1. `Obj` deprecation is revisited in Phase 7
prep when the emitter's `stringify` usage is settled.


## Ergonomic note — square-opened generics closed with `>` (`[...>`)

**Symptom.** The compiler reports:

    error[E0001]: Expected ']', found Gt
    --> src/file.gol:13:60
    |
 13 |     pub value_corrections: Map<String, List[ValueCorrection>>
    |                                                            ^

The line looks correct at first glance. The `>>` at the end is
right — `Map<String, List<ValueCorrection>>` should close both
generics with `>`.

**Root cause: `List[` was written instead of `List<`.**

    Map<String, List[ValueCorrection>>
                ^^^^ square opener

The `[` after `List` opens a **square-bracket generic** (like
`Option[...]`, `List[...]` — ALGOL26 supports both families).
The parser then expects `]` to close it. Finding `>` (the `Gt`
token) is an error — "expected `]`, found `Gt`".

**Diagnosis method.** This is the fastest check:

    sed -n '<reported_line>p' src/file.gol | hexdump -C

Look for byte `5b` (`[`) where you expect `3c` (`<`). The
hexdump is unambiguous — no visual ambiguity from characters
that look similar in a terminal.

**Linter gap.** `tools/lint_generic_brackets.py` catches the
*reverse* class (`Map<...]` — angle opened, square closed), but
not `List[...>` (square opened, angle closed). The linter's
current rule set only checks one direction. The compiler catches
both, so `tools/check_all.sh` (lint + compile) still catches it.

**Client fix.** Change `List[` to `List<` where the generic is
meant to be angle-bracketed. Everywhere `List[...]` appears with
an angle `>` closer, the opener is wrong.

**Why this isn't a language gap.** ALGOL26 supports both
`List<T>` and `List[...]` — two families for the same generic.
Mismatched families are a client-code typo. The diagnostic is
clear ("expected `]`, found `Gt`") and points at the right line.
The hexdump resolves the visual ambiguity.

**Pattern to watch for.** Any `List[...>` (square open + angle
close) or `Map<...]` (angle open + square close) is a typo. The
family must match on both ends. The linter catches one direction;
the compiler catches both.

## Addendum to G40 — typed empty list literals

Same class as `Option[T] = None` (main G40 entry): the compiler's
type inference works forward, not backward.

- `return []` in a `-> List[T]` function fails at runtime with
  "return type mismatch: expected List(Record(...)), found
  List(Unknown)".
- `return List<T> []` fails at **compile time** with "Undefined
  variable 'List'" — `List<ValueCorrection> []` is parsed as
  `List < ValueCorrection > []` (a comparison expression), not a
  typed constructor.

**The working form is the `var`-annotated binding:**

    var empty: List<ValueCorrection> := []
    return empty

The `var` annotation gives `empty` the concrete type. `return
empty` returns that type. `Map<K, V> {}` works because `{ }` after
`Map<...>` is unambiguously a constructor call, unlike `[]` after
`List<...>`.

**Pattern for future port files.** Any `return []` in a function
whose return type is a concrete `List[T]` needs the `var` binding
form. `return []` alone produces `List<Unknown>`. `return List<T>
[]` doesn't parse. The `var`-binding form is the only one that
works.

## G46 — `Map<_,_>` in a record field panics LLVM codegen (FIXED)

**Status:** Fixed 2026-11-12.

### Original diagnosis (wrong)

The initial entry described a "type-table slot collision" between a
`Map<_,_>`'s internal representation and a sibling `List<Int>` local,
citing `cur[0] := 5` producing a `Bool`-typed `cur`. **There are no
slots**: `type_table_id` is a `HashMap<ExprId, Type>`
(`compiler.rs:82`, `analyzer/mod.rs:116`) — a `HashMap` cannot have
slot collisions. The diagnosis conflated several unrelated symptoms
observed from the port's error messages, and the port's
`append`-workaround was working around the *effects* without
identifying the cause.

### Actual bug

Six parallel `TypeSyntax -> Type` resolvers exist across the compiler,
each independently maintained:

  1. `src/frontend/ast.rs::TypeSyntax::to_type`
  2. `src/semantics/analyzer/items.rs::resolve_type_syntax`
  3. `src/semantics/analyzer/items.rs::resolve_syntax_with_records`
  4. `src/semantics/builder/mod.rs::resolve_type_syntax`
  5. `src/backends/llvm_codegen/types.rs::resolve_field_type`
  6. `src/backends/llvm_codegen/types.rs::resolve_field_type_with_subst`

Every builtin type constructor (`List`, `Option`, `Result`, `Map`,
`Set`, `Channel`, `Pointer`, `Borrow`, `MutBorrow`) must be present
in all six. When `Map` was added, it went into the four
frontend/analyzer/builder resolvers but was **missed in the two
LLVM-side resolvers**. Additionally, those two resolvers' `_`
fallback was `Type::record(name, args)` — which silently converts
any *unrecognized* generic into a `Type::Record` rather than
failing closed.

The chain of failure for `rec Holder { items: Map<String, Int> }`:

1. `resolve_field_type_with_subst` (the generic path used by
   `resolve_record_fields`) had no `"map"` arm, so
   `Map<String, Int>` fell through to
   `_ => Type::record(name, resolved_args)`.
2. The field's resolved type became
   `Type::Record("Map", [String, Int])` instead of
   `Type::Map(String, Int)`.
3. LLVM `map_type(Type::Record("Map", ...))` did
   `self.record_decls.get("Map")` at `types.rs:416`, found nothing,
   and hit the `unreachable!("unknown record 'Map'")` panic.

The interpreter was unaffected because it type-erases `Map` values
to `RuntimeValue::Map(HashMap)` — it never reads the declared field
type. Only LLVM does. So `run --interpreter` succeeded while `run`
panicked, which is why the port's tests (interpreter-only) never
caught this.

### Why the port's `append`-workaround didn't help

`intent_detector.gol`'s `levenshtein` was rewritten to avoid
`Stmt::ArrayAssign` (`cur[idx] := v`), with a comment claiming the
ArrayAssign analyzer didn't record the array expression's type. That
comment misidentified the cause: the *actual* failure shape was
`Map<_,_>` in a record field, which panics with or without an
`ArrayAssign` in the same function. The workaround happens to
produce correct output (the `append`-based DP is a valid algorithm),
but the stated reason was wrong, and the workaround does not fix
G46 — it just avoids one code path that happened to be present when
the bug was first observed. Once this fix landed, both `append` and
`cur[idx] :=` forms work.

### Fix

`src/backends/llvm_codegen/types.rs`:

- Both `resolve_field_type` and `resolve_field_type_with_subst` now
  recognize the full builtin set (`list`, `option`, `result`,
  `pointer` / `ptr`, `borrow`, `mutborrow` / `mut_borrow`,
  `channel`, `map`, `set`).
- Both now consult `record_decls` only for names that actually
  resolve to a user record (`_ if self.record_decls.contains_key(name)
  => Type::record(name, resolved_args)`), and fall through to
  `Type::Unknown` for unrecognized generics — matching the
  fail-closed discipline used elsewhere in the compiler. Previously,
  a typo like `Mapp<String, Int>` silently became
  `Type::Record("Mapp", ...)` and panicked LLVM with a confusing
  "unknown record" message; now it becomes `Type::Unknown` and the
  type-table completeness pass (or a downstream verifier) reports
  it with a targeted diagnostic.

### Tests

- `tests/chaincss/compiler_bugs/g46_map_field_record.gol` — minimal
  reproducer (`rec Holder { items: Map<String, Int> }` plus a
  `Map { "a": 1 }` literal and `.length()`, `.get()`, `.contains()`).
- `tests/conformance/valid/records/map_field.gol` — end-to-end pin,
  exercised on both interpreter and LLVM backends (first time a
  Map-in-record field runs on LLVM in the test suite).

### Cleanup deferred

The six-resolver duplication is a maintenance hazard: adding a new
builtin type constructor requires touching six independent functions
with subtly different signatures, and the drift is silent until a
specific shape reaches codegen. Consolidation into a single
parameterized resolver (substitution map + records map) is scheduled
for Phase 7 alongside the `Float.to_string` LLVM/WASM work. **Until
then, any `TypeSyntax` handling change must audit all six sites.**

### Related gaps exposed

- **`print(Option<T>)` on LLVM** — the reproducer's
  `print(h.items.get("a"))` fails at LLVM codegen with
  `cannot print value of type Option(Int)`, while the interpreter
  prints `Some(1)`. Tracked as **G54** (see below). Not part of G46.
- **`ArrayAssign` with `Map` field in scope** — the port's workaround
  claimed this was broken; after G46's fix, both `append` and
  `cur[idx] :=` forms work with `Map` record fields present. No
  further bug exists in that combination as tested. The
  `intent_detector.gol` workaround comment should be updated to note
  that the fix landed and the workaround is now optional.

---

## G54 — LLVM/WASM cannot print `Option<T>` and other composites (Open)

**Status:** Open. Surfaced 2026-11-12 during G46 verification.

The interpreter renders any `RuntimeValue` via its `Display` impl,
including `Some(1)`, `None`, `Ok(1)`, `Error("x")`, records, lists
of records, etc. LLVM and WASM only support `printf` for primitive
types (`Int`, `Float`, `String`, `Bool`) plus lists of those.
`print(some_option)` produces:

```
error[E0002]: LLVM codegen: cannot print value of type Option(Int)
(no LLVM format mapping; see common::types::print)
```

**Impact:** any conformance fixture with
`// BACKEND: interpreter llvm` or `interpreter llvm wasm` that prints
an `Option`, `Result`, record, or nested composite fails on the
LLVM/WASM legs. The current fixtures avoid this by printing
primitives; the port's own tests (`intent_detector.gol`,
`intent_data.gol`) print only primitives for the same reason, but
not by policy.

**Workaround for Phase 6/7 ports:** never print non-primitive values
on LLVM/WASM paths. Add a `.length()` or `.is_some()`-style
reduction if a value needs to be visible.

**Fix path (Phase 7):** extend the LLVM/WASM print lowering to
handle composites by calling a runtime formatter. Requires a
`runtime/print.c` (or similar) implementing per-type formatting and
a rewrite of the `compile_builtin_value` print arm to dispatch by
`arg.type_of()`. Alternatively, port the interpreter's `Display`
rendering to a shared Rust helper invoked via `extern "C"` — same
shape as the deferred `Float.to_string` LLVM fix (G34).

**Not blocking Phase 6.** The interpreter (where all Phase 6 port
tests currently run) handles all types. LLVM/WASM print parity is a
Phase 7 concern.

## G51 — Recursive-union analysis overflows the stack in deep matches

**Status:** Open. Observed 2026-11-12 while writing a G18-addendum
regression fixture.

A source program with a recursive union (`Json` with
`Arr(List[Json])` and `Obj(Map<String, Json>)`) compiles fine
standalone, but type-checking a deeply-nested `match` over the
same value overflows the test's 8 MB stack. With
`RUST_MIN_STACK=32 MB` it compiles, but W0002 does not fire (the
analyzer's G18 fallback resolves the empty table cleanly — see
the W0002 note above).

This is not the G18-addendum bug (which is a silent runtime
fallthrough). It is a stack-depth limit in the analyzer's
recursion through the recursive union's type structure during
pattern binding. The relationship to the actual port's behavior
is unclear: `intent_data.gol` compiles today with the split-helper
workaround, so its match depth stays below the threshold.

Investigation needed:
- Confirm the overflow is in `bind_pattern_variables` /
  `check_pattern_type`'s recursion (backtrace).
- Decide whether to raise the analyzer stack (a
  `stacker`/`RUST_MIN_STACK`-equivalent) or reduce recursion
  (iterative variant-table lookup).
- Determine the depth threshold empirically.

Not Phase-6-blocking: the port doesn't currently hit it. Worth
logging because it's a worse user experience than the silent
fallthrough G18-addendum addresses.

## G50 — LLVM/WASM emit invalid IR for records passed by value

**Symptom:** `proc add(a: Point, b: Point) -> Point` compiles under
`--interpreter`, but `algol26 build` (LLVM) and `algol26 wasm` fail
with "Generated LLVM/WASM IR is invalid: Call parameter type does
not match function signature" — the call passes `ptr %Point_tmp`
where the function signature expects `%Point` (by value).

**Impact:** records-by-value is claimed `ok` in the capability
matrix (both LLVM and WASM), but fails. Every port file that uses
records passed by value will not compile to native binaries.

**Root cause:** the LLVM and WASM codegens disagree on the calling
convention for records passed by value. One side emits `ptr`, the
other expects the record value.

**Fix scope:** ~20-50 lines in `llvm_codegen/instruction.rs` and
`wasm_backend.rs`. Unify on one calling convention.

**Status:** Open. `capability_matrix_is_accurate` test fails on
`records.by-value × llvm` and `× wasm`.

## G52 — LLVM/WASM panic on UnionConstruct with unrecorded target

**Symptom:** `match s { case Circle(r) then ... }` where
`s := Shape.Circle(3.0)` compiles under `--interpreter`, but
`algol26 build` (LLVM) and `algol26 wasm` fail with:

    error[E0009]: LLVM codegen: UnionConstruct target `s` has no
    recorded type. A builder producer emitted UnionConstruct without
    registering the target's Type::Union in var_types.

**Impact:** unions work in the interpreter but not in native
backends. The matrix claims `union × llvm: ok`, `× wasm: ok`.
Same class as G47 (panic instead of clean refusal).

**Root cause:** the builder's `UnionConstruct` path doesn't register
the target's `Type::Union` in `var_types`. Either the builder or
the verifier should catch this.

**Fix scope:** ~5-20 lines in `semantics/builder/expr.rs` (register
the type) or `ir/verifier/instruction.rs` (add the check).
Same class as G47 — a verifier hole.

**Status:** Open. `capability_matrix_is_accurate` test fails on
`union × llvm` and `× wasm`.

## G53 — try/catch: analyzer doesn't infer T and E into a unified Result

**Symptom.** `try { Ok(5) } catch e { 99 }` produces IR with
`Result<Int, Unknown>` (Ok path knows T=Int but not E). `try {
Error(0) } catch e { 99 }` produces `Result<Unknown, Int>` (Error
path knows E=Int but not T). And the whole try/catch expression's
result type is `Unknown` when the try body is an `Error(...)` —
because the Ok arm's payload type is never observed.

**Consequence.** LLVM codegen panics on the `Unknown` type
reaching `map_type`. WASM refuses `try_catch` because `Result`
is refused under WASM (independent).

**Root cause.** The analyzer's `type_of_expr` for the try/catch
expression doesn't unify `T` from the Ok arm with `T` inferred
from the catch body, nor `E` from the Error arm with `E` inferred
from context. Each arm's expression contributes only its own
payload type; the other is `Unknown`.

**Current state.** Codegen workarounds in place:
- `try_value_type` derived from the trailing expression's type
  (so Ok path gets `Result<Int, Unknown>`, an improvement over
  `Result<Unknown, Unknown>`).
- `i64` placeholder for `Unknown` payloads in `map_type(Result)`
  (the unused slot is never read — safe).

These don't fix the `result_var: Unknown` case (the whole
try/catch expression's type when the try body never produces
an Ok payload). That case still panics under LLVM.

**Capability matrix.** `try_catch × interpreter: ok`.
`try_catch × llvm: refused` (G53 — analyzer gap).
`try_catch × wasm: refused` (`Feature::Result` refused under
WASM).

**Fix scope.** Extend the analyzer's try/catch inference to:
1. Unify `T` from the Ok arm's payload and the catch body's type.
2. Unify `E` from the Error arm's payload and the catch variable's
   declared type (or default `Void` if unconstrained).
3. Handle the "try body never produces Ok" case — infer `T = Void`
   when the try body is a pure `Error(...)` constructor.

The third case is unsound in general (a `try` body with an
early `return Ok(...)` is not a pure `Error(...)`), so the fix
may require a small dataflow analysis or a conservative default
of `Void` for `T` when the Ok path is provably unreachable.

Alternatively: require an explicit type annotation on the
try/catch result:
    val err: Int := try { Error(0) } catch e { 99 }
The annotation pins `T = Int`, and no analyzer change is needed.

**Status.** Open. `try_catch × llvm` is `refused` in the
capability matrix until this is fixed. `try_catch × interpreter`
works.
