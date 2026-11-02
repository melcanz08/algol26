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
