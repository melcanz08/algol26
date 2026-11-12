# ADR 0066: `Json.OrderedObj` — insertion-order preservation at the JSON boundary

Status: Proposed

## Context

ADR 0027 introduced `Map<K, V>` and deliberately rejected insertion-order
guarantees: "Ordered maps. No `Map.ordered` or insertion-order guarantee.
Hash order is unspecified. Programs that need order sort the keys." That
was correct for the language-level container. Every existing caller in
the ALGOL26 repo either tolerates or relies on `Map.keys()` returning
sorted keys: config emptiness checks (`user_output.keys().length() > 0`),
`stringify_object`'s deterministic output, `wordfreq` and word-count
fixtures' expected output. Reversing ADR 0027 would break all of them.

The **ChainCSS port** is a use case ADR 0027 did not anticipate. JSON
objects preserve key order as a spec-mandated property of the source
(the JSON spec doesn't require preservation, but every real parser does
it, and TypeScript's `Object.entries` preserves it). ChainCSS's IR
derives from JSON source order at three points:

- The rule list order in `StyleIR.rules` is component-declaration order.
- `specificity-sorter` sorts rules by specificity with a *stable*
  tie-break on original index. The tie-break only produces correct
  output if the input order matches TS's.
- The emitter (Phase 7) preserves declaration order within a rule.

The port's current pipeline loses source order at the JSON parse
boundary: `json.gol`'s `parse_object_pairs` inserts into a
`Map<String, Json>`, and `Map.keys()` sorts. By the time
`parse_ir.gol` iterates `style_map.keys()`, source order is gone. The
Phase 5 parity comparator *sorts both sides* to hide this — a
workaround that hides real bugs. Phase 6's `specificity-sorter` cannot
use the same workaround because its output is a *new* order that
depends on the *input* order.

The weakness doc (G43) and the Phase 6 kickoff doc (P1) both frame the
fix. After a grep audit of all `.keys()` call sites, they converge on
the same answer: **add insertion order to `Json`, not to `Map`.**

## Decision

Add a new `Json` variant:

```gol
pub union Json
    Null
    Bool(Bool)
    NumInt(Int)
    NumFloat(Float)
    Str(String)
    Arr(List[Json])
    Obj(Map<String, Json>)                        // existing
    OrderedObj(List[(String, Json)])              // new
```

`Json.OrderedObj` holds an ordered list of `(key, value)` pairs. The
JSON parser emits `OrderedObj`. Programmatic construction can use
either `Obj` (unordered, keyed by `Map`) or `OrderedObj` (ordered,
keyed by index). Consumers match both variants.

Both variants coexist initially. Whether `Obj` is eventually removed
(replacing it with `OrderedObj` everywhere) is a follow-up ADR — it
depends on the construction-site sweep size, which is measured in
implementation step 1. If the sweep is under ~50 sites, replacement
is the cleaner long-term answer; if larger, coexistence is the
sustainable interim.

### Parser

`parse_object_pairs` and its helpers (`parse_object_pair`,
`parse_object_finish`, `parse_object_insert`, `parse_object_separator`)
change their accumulator from `Map<String, Json>` to `List[(String,
Json)]`. `parse_object_insert` appends instead of inserting.

`parse_object` returns `Json.OrderedObj(entries)` instead of
`Json.Obj(Map {})` for both the empty and non-empty cases.

### Stringify

`stringify_object` gains a sibling `stringify_ordered_object`. To
preserve the current byte-for-byte output (needed for the Phase 5
parity tests' golden files), `stringify_ordered_object` **sorts**
its entries by key before emitting. This means `parse(stringify(x))
== x` holds for `OrderedObj` up to key order — the same guarantee
`Obj` has today.

`stringify` dispatches:

```gol
case Obj(m) then stringify_object(m)
case OrderedObj(entries) then stringify_ordered_object(entries)
```

**Reviewer note.** Sorting on serialize is a byte-compat choice. If
the emitter (Phase 7) is the only consumer of `stringify`-as-output,
and the optimizer has already determined rule order, then sorted
serialization is safe. But if `stringify` is used as debug output
(`inspect --ir`, the parity harnesses), source-order serialization
may be more useful for diffing. The decision is deferrable to
implementation: the `stringify_ordered_object` body is one `sort_by`
call, trivially swapped if a reviewer prefers insertion order.

### Consumers

Every `match json { case Obj(m) ... }` arm gains a sibling:

```gol
case OrderedObj(entries) then { /* consume entries as (String, Json) list */ }
```

The 21 existing `case Obj` sites in `src/chaincss/*.gol` are the
sweep. In most cases the sibling arm's body mirrors the `Obj` arm
(iterate the entries list, look up by linear scan rather than
`Map.get`). `json_ext.gol`'s accessors (`json_as_obj`,
`json_obj_get`, etc.) are the abstraction boundary — they get
extended once, and their callers don't need to know which variant
they were handed.

### Consumers that need order specifically

- `parse_ir.gol`'s `parse_components` (line ~358) iterates
  `components.keys()` today. Switch to: if the value is
  `OrderedObj`, iterate `entries` in order; if it's `Obj`, fall
  back to sorted `keys()`. This is the *only* consumer that needs
  source order in Phase 6.
- `parse_ir.gol`'s `parse_one_component` -> `parse_rule_for_selector`
  iterates `style_map.keys()` today. Same switch — and this is where
  declaration order within a rule is established.

Consumers that don't need order (`config.gol`'s emptiness checks,
`diagnostics.gol`, `json.gol`'s own stringifier) match `OrderedObj`
and proceed as they would have for `Obj`.

### Analyzer, IR, interpreter, backends

No changes. `Json` is the ChainCSS port's own type; it does not affect
the ALGOL26 language's IR, interpreter, or backends. This ADR is
entirely within `src/chaincss/`.

### Comparator

The Phase 5 comparator's "sort both sides" workaround
(`diff-corpus/compare_normalized.py`) can remain during the
transition. It is retired when Phase 6's pass parity tests demonstrate
source order is preserved end-to-end — at which point the comparator
gets an "order-sensitive" mode, and the sort workaround becomes a
bug-hiding hack.

## What is deliberately not in v1

- **Replacing `Obj` entirely.** Both variants coexist in v1. `Obj`
  remains for programmatic construction where order doesn't matter.
  Whether it is eventually removed (a follow-up ADR) is decided by
  the construction-site sweep size, measured in implementation
  step 1.
- **Preserving order through `Map` iteration.** ADR 0027 stands.
  `Map.keys()` stays sorted.
- **A `Json.Obj` -> `Json.OrderedObj` converter helper.** If a
  consumer receives an `Obj` and needs to iterate in a specific
  order, it sorts (as today). `OrderedObj` is the only ordered form;
  conversion is explicit (`List` from `Map.keys()`, sorted, or
  insertion-ordered if the caller constructed it that way).
- **Insertion-order `stringify` output.** `stringify_ordered_object`
  sorts. This is a deliberate backward-compat choice — the Phase 5
  golden files stay valid. The choice is reviewable (see the
  reviewer note above).

## Consequences

**Positive.**

- Source order is preserved end-to-end in the ChainCSS port.
  `specificity-sorter` receives rules in the same order TS's
  `specificity-sorter` does. Combined-pass byte parity becomes
  achievable.
- ADR 0027 is not touched. `Map` continues to be a hash map with
  unspecified iteration order.
- The change is entirely within `src/chaincss/`. No compiler or
  runtime changes. No LLVM/WASM codegen. No ADR 0027 amendment.
- `parse(stringify(x)) == x` holds for both variants (up to key
  order within an object, which is the same tolerance `Obj` has).
- The sweep is bounded: **21 `case Obj` sites** in `src/chaincss/`,
  plus the construction-side sites (`Json.Obj(...)` calls in
  `config.gol`, `json.gol`, `diagnostics.gol`, etc.).

**Negative.**

- Every `match json { ... }` needs the two-arm pattern. G18's
  match-depth hazard becomes slightly worse if a nested
  `match` inside a `match` has to handle both variants at each
  level. The `json_ext.gol` accessor boundary is the mitigation;
  nested matches go through it rather than duplicating the
  variant-check pattern inline.
- `parse_object_pairs`'s accumulator changes from
  `Map<String, Json>` to `List[(String, Json)]`. Duplicate-key
  handling changes: `Map.insert` was last-write-wins; a `List` of
  entries needs the caller to decide (parse is not going to
  dedupe). This is fine — JSON allows duplicate keys, and the
  spec's behavior is unspecified — but it's a behavior change at
  the parse boundary. The comparator should tolerate it; if any
  existing fixture relies on last-write-wins, it needs updating.
- Two `Json` object variants means the language surface has two
  ways to represent "a JSON object," and future contributors must
  choose. Documented in `json.gol`'s header comment.

**Neutral.**

- The `Json` union grows from 8 to 9 variants. The exhaustiveness
  cascade in `match` is a known cost of union extension.
- `stringify_ordered_object` sorts; `stringify_object` continues to
  iterate `Map.keys()` (already sorted). Both emit sorted output.
  Uniform behavior from the caller's perspective.

## Implementation order

1. **Measure the sweep.** Count construction sites: `grep -rn
   "Json.Obj(" src/chaincss/*.gol | wc -l`. If < 50, prefer the
   replace-in-follow-up path; if larger, coexist is the working
   assumption. The ADR's decision on `Obj` deprecation is decided
   here, before code changes.
2. **Add `OrderedObj` to `Json`** (`json.gol`). Update `stringify`
   to dispatch. Add `stringify_ordered_object` (sorts entries by
   key, preserves current output). Compiles in isolation; no
   callers yet exercise it. ~50 lines.
3. **Extend `json_ext.gol`'s accessors** to handle both variants.
   `json_as_obj`, `json_as_ordered_entries`, `json_obj_get`,
   `json_obj_get_map`, `json_obj_get_arr`. ~60 lines. The
   abstraction boundary: callers use these, not direct `match`.
4. **Change `parse_object`** to emit `OrderedObj`. Accumulator
   thread: `Map<String, Json>` -> `List[(String, Json)]` across
   `parse_object_pairs`, `parse_object_pair`, `parse_object_finish`,
   `parse_object_insert`, `parse_object_separator`. ~100 lines of
   surgical change. Existing parse-parity tests should still pass,
   with output now `OrderedObj` instead of `Obj`.
5. **Sweep the 21 `case Obj` sites** to add `case OrderedObj`
   siblings. Mechanical, one file at a time. `ast.gol`,
   `config.gol`, `ir_types.gol`, `pipeline_types.gol`,
   `intent_data.gol`, `intent_detector.gol`, `unit_normalizer.gol`,
   `parse_ir.gol`, `diagnostics.gol`. ~150 lines across 9 files.
6. **Switch `parse_ir.gol`'s iteration to source order.**
   `parse_components` and `parse_rule_for_selector` iterate
   `OrderedObj.entries` directly. ~30 lines.
7. **Update the comparator** to drop the sort workaround — or add
   an order-sensitive mode used only by Phase 6's pass-parity tests.
   `diff-corpus/compare_normalized.py`. ~40 lines.
8. **Tests, feature doc updates, LANGUAGE_GAPS.md.** ~200 lines.

Steps 2-4 are the mechanical core. Step 5 is the sweep. Steps 6-8
validate the change.

**Estimated:** 1 session to write and merge the ADR + 2-3 sessions
of implementation. The three implementation sessions are:

- **Session 1:** steps 1-3 (measure, add variant, extend accessors).
  Compiles cleanly. Existing tests unaffected because nothing
  constructs `OrderedObj` yet.
- **Session 2:** steps 4-5 (parse change + sweep). This is where
  existing tests break and need updating; the sweep is where
  regressions surface.
- **Session 3:** steps 6-8 (source-order iteration, comparator,
  tests, docs). Verification.

## Tests

**Parser round-trip:**
- `ordered_obj_parses`
- `ordered_obj_stringify_sorts_entries` — `parse(stringify(x))`
  produces the same object up to key order
- `ordered_obj_empty`
- `ordered_obj_nested`

**Source-order preservation:**
- `parse_components_preserves_source_order` — a fixture with
  multiple components in non-alphabetical order
- `parse_rule_for_selector_preserves_declaration_order` — a
  rule with declarations in non-alphabetical order

**Accessors:**
- `json_as_obj_handles_ordered`
- `json_as_obj_handles_obj`
- `json_obj_get_works_on_both`

**Sweep regression:**
- Every existing `.gol` fixture in `tests/chaincss/` passes.
- `ast_roundtrip_smoke.gol` — already exercises JSON round-trip.

**Phase 6 integration:**
- `parse_ir_parity_source_order.gol` — a new differential driver
  that runs `parse_ir` and compares against TS's `parseIR` *without*
  sorting. This is the acceptance criterion from Phase 6's
  kickoff doc section 3 P1.

## Open questions

- **Does `Obj` eventually get deprecated?** Decided by the
  construction-site count in implementation step 1.
- **Duplicate keys in `parse`.** `Map.insert` was last-write-wins.
  `List` append is first-wins-by-index (i.e., both entries are
  preserved). Which is correct? JSON's spec says "the behavior of
  software in the presence of duplicate keys is unpredictable."
  Practical answer: preserve both entries (they'll both stringify,
  which differs from today's output). Compare: TypeScript's
  `JSON.parse` preserves the *last* value. If parity matters,
  `parse_object_insert` could scan the accumulator and replace the
  existing entry rather than appending. Deferred to implementation
  — the corpus likely has no duplicate-key fixtures.
- **Ordered-iteration in `for` loops.** If `OrderedObj` becomes the
  dominant form, `for k in json.entries` might become a language
  feature. Not in v1; `for (k, v) in entries` isn't supported by
  ALGOL26 (no tuple destructuring).

## See also

- ADR 0027: `Map<K, V>` — the container ADR this coexists with
- `docs/phases/phase-6-optimizer.md` section 3 P1 — the precondition
  this ADR resolves
- `LANGUAGE_GAPS.md` G43 — the gap this ADR closes
- `src/chaincss/json.gol` — where the change lands
- `src/chaincss/parse_ir.gol` — the consumer that needs source order
