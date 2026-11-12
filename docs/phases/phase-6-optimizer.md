# Phase 6 — Optimizer Port

**Status:** Not started. This document is the work order.
**Target:** `src/chaincss/optimizer.gol` (new) + supporting files.
**Source:** `chaincss/src/compiler/pipeline/optimizers/*.ts`,
  `chaincss/src/compiler/pipeline/pipeline.ts`,
  `chaincss/src/compiler/pipeline/pass-scheduler.ts`,
  `chaincss/src/compiler/incremental/graph-builder.ts`,
  `chaincss/src/compiler/pipeline/ir/types.ts`.
**Exit criterion:** post-optimizer IR parity, pass-by-pass and combined,
  on the frozen corpus, on the interpreter backend.

---

## 1. Why this document exists

The migration roadmap (§14) describes Phase 6 as "Port `optimizer.ts` →
`optimizer.gol` (pass pipeline). Each optimization pass is ported and
diffed independently. Pass ordering must be preserved exactly."

Reading the actual TypeScript reveals Phase 6 is larger than that
one-line description suggests. The nine passes are independent, but
they are not *independent of the IR*. Three layers of infrastructure
must land first, and one architectural decision (G43, `Map` order)
must be resolved before the first pass can be byte-verified. This
document names each precondition, the order to resolve them, and the
acceptance criterion for each.

It supersedes the roadmap's one-paragraph Phase 6 description. It does
not supersede the roadmap's exit criterion.

---

## 2. What already exists (Phase 2–5 substrate)

Phase 4 (IR + config) and Phase 5 (normalizers) are further along than
the roadmap's coarse year-map implies. The following files are
verified working (the `cargo test` suite exercises them through the
conformance harness):

| File | What it provides | Line count |
|---|---|---|
| `src/chaincss/json.gol` | `Json` union, `stringify`, `parse` | ~1100 |
| `src/chaincss/json_ext.gol` | typed accessors (`json_as_obj`, etc.) | ~110 |
| `src/chaincss/ir_types.gol` | `StyleIR`, `IRRule`, `IRDeclaration`, `IRAtRule`, `IRPseudoClass`, `IRCondition`, `IRKeyframeFrame`, `ParsedValue`, plus full JSON serialization | ~700 |
| `src/chaincss/ir_factory.gol` | `create_rule`, `create_declaration`, `record_history`, `stable_id` (FNV-1a) | ~250 |
| `src/chaincss/parse_ir.gol` | `parse_ir(styles: Json, source_file: String) -> StyleIR` | ~450 |
| `src/chaincss/pipeline_types.gol` | `Correction`, `NormalizationResult`, `NormalizationPass`, `CorrectionResult`, `IntentContext`, `HealResult` | ~180 |
| `src/chaincss/parser.gol` | `normalize_property`, `kebab_lower`, `starts_with`, `is_numeric_string`, etc. | ~700 |
| `src/chaincss/config.gol` | `ChainCSSConfig`, `get_default_config`, `load_config_from_json` | ~600 |
| `src/chaincss/diagnostics.gol` | `DiagnosticReporter`, `format_report` | ~400 |
| `src/chaincss/unit_normalizer.gol` | `normalize_units(ir) -> NormalizationResult` | ~400 |
| `src/chaincss/intent_data.gol` | `IntentData` loader | ~200 |
| `src/chaincss/intent_detector.gol` | `IntentEngine`, `levenshtein`, `correct`, `validate` | ~550 |
| `src/chaincss/ast.gol` | `StyleDefinition`, `AtRuleNode`, `NestedRuleNode` | ~400 |
| `src/chaincss/shorthand.gol` | `GridOptions`, `FlexOptions`, etc. | ~450 |

**What is *not* in `ir_types.gol`:** `IRGraph`, `IRGraphEdge`,
`PassMetadata`, `IRRuleMeta`, `IRDeclarationMeta` (partially), and any
`OptimizationPass`-family type. See §3 for what must be added.

**What is *not* in `pipeline_types.gol`:** the entire Optimization
stage type family. Its header explicitly says:

> `Everything else (ValidationPass, OptimizationPass, LoweringPass,
> CompilerPass, PipelineResult, Inspector types) is Phase 6+ and is
> deferred. When Phase 6 starts, add the relevant subset here rather
> than porting the whole file.`

That extension is precondition P2 below.

---

## 3. Preconditions

Phase 6 cannot produce byte-for-byte parity until five preconditions
are resolved. They are *not* optional; each one blocks a subset of the
passes.

### P1 — G43: `Map` order preservation

**Blocks:** `specificity-sorter`, `media-query-packer`,
`duplicate-declaration-detector`, `source-optimizer` — i.e., the
majority of the pass pipeline.

**Why.** `specificity-sorter.ts:...` sorts rules by specificity with
a stable tie-break on original index. The tie-break only produces
correct output if the *input* order matches TS's. But `parse_ir.gol`
walks `style_map.keys()` where `style_map` is a
`Map<String, Json>`. `Map.keys()` sorts lexicographically, not by
source order. So `IRRule.specificity` is correct per-rule, but the
list order fed into the sort does not match TS's.

The `unit_normalizer.gol` header already documents this:

> `G43 (open, Phase 7 blocker): Map.keys() sorts declarations
> alphabetically; TS preserves source order. The parity comparator
> sorts both sides before comparing. Byte-for-byte parity for the
> emitter (Phase 7) requires G43 to land.`

Phase 5 got away with sorting both sides in the comparator. Phase 6
cannot: `specificity-sorter` produces a *new* order, and the
comparator cannot simply re-sort both sides (it would hide real
bugs).

**Decision required.** Three options, from the weakness doc (G43):

| Option | Cost | Verdict |
|---|---|---|
| (a) Add `OrderedMap<K, V>` as a new stdlib type | 2–3 weeks | Overkill for Phase 6, but the 'right' long-term answer |
| (b) Extend `Json` with `Json.OrderedObj(List[(String, Json)])` | 1–2 weeks + 50–100 call-site updates | Surgical, but every `match json { case Obj(m) ... }` in Phase 2–5 needs a new arm |
| (c) Make `Map` preserve insertion order and add `Map.keys_in_order()` | ~3–5 days (compiler change: `runtime/map.c` + `builtins.rs` + interpreter + LLVM/WASM) | Surgical *and* does not touch `Json`. Existing sorted `Map.keys()` calls stay unchanged. New code calls `keys_in_order()`. |

**Recommendation: (c).** It is the smallest change that preserves
source order end-to-end, and it does not require a `Json` variant
sweep. `Map.keys()` continues to return sorted keys (backward-compat
for Phase 5's comparator), but `Map.keys_in_order()` returns insertion
order. The JSON parser (`json.gol`) uses insertion-order internally
and `parse_ir.gol` switches to `keys_in_order()` where it matters.

**Acceptance:** `parse_ir.gol` produces rules in source order, verified
by a new `parse_ir_parity_source_order.gol` fixture that compares
against the TS oracle *without* sorting.

**Estimated:** 1 session to design + 1 session to implement + 1 to
sweep call sites. See ADR slot for the decision.

### P2 — Extend `pipeline_types.gol` with the Optimization stage family

**Blocks:** every pass port.

**Why.** `optimizer.gol` and every pass file need `OptimizationPass`,
`OptimizationContext`, `OptimizationResult`, `OptimizationSavings`.
They are not in `pipeline_types.gol` yet.

**What to add.** Mechanical translation of `pipeline-types.ts` lines
4–8 (the Optimization section):

```gol
pub rec OptimizationContext
    pub tokens: Option[Json]
    pub atomicUsageMap: Option[Json]      // was Map<String, Int>; G1 wrapper
    pub minify: Option[Bool]
    pub atomic: Option[Bool]
    pub threshold: Option[Int]
    pub extensions: Json                  // catch-all for future context keys

pub rec OptimizationSavings
    pub rulesEliminated: Int
    pub declarationsEliminated: Int
    pub bytesSaved: Int

pub rec OptimizationResult
    pub ir: StyleIR
    pub savings: OptimizationSavings
    pub changes: Int

pub rec OptimizationPass
    pub name: String
    pub cost: String                      // "cheap" | "moderate" | "expensive"
    pub requiredFor: List[String]         // GenerationTarget[]
    // the pass body is a bare function, not a field; see §5
```

Note that `OptimizationPass` in ALGOL26 will not carry a function
field (G23). The dispatch is done by name in `optimizer.gol`. See §5.

**Acceptance:** `pipeline_types.gol` compiles with the new records,
`corrections_to_json`-style serializers added for
`OptimizationResult` (parity harness needs them).

**Estimated:** half a session.

### P3 — Type `IRRule.meta` and `IRRule.passMeta`

**Blocks:** `buildIRGraph` (P4), therefore every pass that needs the
graph, therefore `dead-code-eliminator` and any future graph-dependent
pass.

**Why.** `graph-builder.ts` writes `rule.meta.dependencies` and
`rule.meta.dependents` (arrays of node IDs) as it adds edges, and
reads `rule.passMeta.analysis.semantic.tokens` and
`rule.passMeta.optimization.atomic.isAtomic` for token and atomic
detection. Today `ir_types.gol` has:

```gol
pub rec IRRule
    ...
    pub meta: Option[Json]
    pub passMeta: Option[Json]
```

Untyped `Json`. Every read of `meta.dependencies` from ALGOL26 becomes
a nested match through `Json.Obj` → `Map<String, Json>` → `Json.Arr`
→ `List<Json>` → `Json.Str` — and G18's match-depth hazard applies at
three levels. G1's index-signature problem, one layer up from where
`IRDeclarationMeta` solved it.

**What to add.** A typed `IRRuleMeta` and `PassMetadata`, following the
same `IRDeclarationMeta` precedent (named fields + `extensions: Json`):

```gol
pub rec IRRuleMeta
    pub component: Option[String]
    pub framework: Option[String]
    pub atomic: Option[Bool]
    pub dependencies: List[String]
    pub dependents: List[String]
    pub isSyntheticToken: Option[Bool]
    pub tokenName: Option[String]
    pub atRuleType: Option[String]
    pub atRuleName: Option[String]
    pub atRuleQuery: Option[String]
    pub extensions: Json

pub rec PassMetadata
    pub optimization: Option[Json]       // AnalysisPass / OptimizationPass namespace
    pub analysis: Option[Json]
    pub lowering: Option[Json]
    pub extensions: Json
```

Then change `IRRule.meta: Option[IRRuleMeta>` and
`IRRule.passMeta: Option[PassMetadata>`, and update:

- `ir_types.gol`'s `rule_to_json` (serialize the typed fields)
- `ir_factory.gol`'s `create_rule` (construct empty typed meta)
- `parse_ir.gol` (construct `IRRuleMeta` with empty deps; `passMeta` is
  None at parse time)
- `unit_normalizer.gol` (does not currently touch meta)
- `intent_detector.gol` (does not currently touch meta)

**Acceptance:** all Phase 5 parity tests still green; new
`rule_meta_roundtrip.gol` fixture verifies serialization round-trips.

**Estimated:** 1 session. The mechanical work is small; the risk is
missing a call site.

### P4 — Port `buildIRGraph`

**Blocks:** `dead-code-eliminator` (needs `ir.graph` to determine
reachability), and any pass that reasons about overrides or token
derivation. It does *not* block `specificity-sorter`, `ast-optimizer`,
`css-compressor`, `duplicate-declaration-detector`, `media-query-packer`,
`source-optimizer`, `atomic-extractor`, or `accessibility-optimizer`
in isolation — but it blocks `dead-code-eliminator`, which is the
natural first pass.

**Why.** `graph-builder.ts` is ~430 lines of TypeScript. It:

1. Walks `ir.rules` and recursively collects nodes into
   `nodes: Map<IRNodeId, IRRule>`.
2. Creates synthetic `atrule-<id>` nodes for each `IRAtRule`, and
   synthetic `<atrule>-frame-<keyText>` nodes for each keyframe frame.
3. Extracts `$token` references from declaration values (regex
   `/$([a-zA-Z0-9_.-]+)/g`) and creates synthetic `token-<name>` nodes
   on demand.
4. Detects selector overlap (`extractSelectorTokens`,
   `selectorsOverlap`) and adds `overrides` edges.
5. Detects animation-name references and adds `animates` edges to
   keyframe nodes.
6. Reads `rule.passMeta.analysis.semantic.tokens` to add `defines`
   edges (this is analyzer output, absent in Phase 6 — see P4a).
7. Reads `ir.meta.tokenRelationships` to add `derives` edges (same
   caveat — token relationships come from `config.gol`, not Phase 6
   analysis).
8. Updates `rule.meta.dependencies` and `rule.meta.dependents` as a
   *side effect* of edge addition.

**P4a — Defer token-derivation edges.** Steps 6 and 7 read analyzer
output (`passMeta.analysis.semantic.tokens`) and config output
(`ir.meta.tokenRelationships`) that Phase 6 does not produce. Two
choices:

- **(i)** Stub them: emit no `defines` and no `derives` edges. The
  graph is a strict subset of TS's. `dead-code-eliminator` treats the
  graph's reachability as authoritative — a missing edge could cause
  it to erroneously eliminate a live rule. So stubbing is *not* safe
  unless the corpus has no token relationships.
- **(ii)** Port enough of the analyzer to populate
  `passMeta.analysis.semantic.tokens`. That's Phase 6 exceeding its
  scope and touching Phase 8 (analysis) work.

**Decision required: does the Phase 6 corpus (currently `001-card`,
`002-button`, `003-lenient-passthrough`) use tokens or token
relationships?** Grep for `$` in the input `.chain.ts` files. If no
token references, stubbing is safe and P4a collapses into 'emit no' 
token edges.

**Acceptance:** port `buildIRGraph`, `findMaxDepth`, `getGraphStats`,
`exportGraphAsJSON` (the last for parity fixtures). Add a
`graph_parity.gol` harness that compares graph node count, edge count,
edge types, and root/leaf counts against the TS oracle on the
corpus.

**Estimated:** 2–3 sessions. `collectRules` is ~100 lines, the token/
animation detection is ~150 lines, the graph traversal/stat/export is
~150 lines.

### P5 — Cleanup: `intent_detector.gol` G46 workaround

**Blocks:** nothing functionally. It is hygiene.

**Why.** `intent_detector.gol:36-39` still carries:

```gol
// G46 workaround: custom_keys as List<String>, not Map<String, Bool>.
// A record field with Map<_, Bool> corrupts sibling functions'
// type tables (see LANGUAGE_GAPS.md G46). Revert to Map<String, Bool>
// when the compiler bug is fixed.
pub custom_keys: List<String>
```

G46 was fixed 2026-11-12. The comment is now stale, and `custom_keys`
can be `Map<String, Bool>` as originally intended.

**What to do.** Change `custom_keys: List<String>` back to
`Map<String, Bool>`, update the comment to say the fix landed, run
the 16 port tests, iterate if any fail.

**Acceptance:** port tests still green; the G46 comment is gone from
`intent_detector.gol`.

**Estimated:** 30 minutes.

---

## 4. Porting order

The passes are ordered in `pipeline.ts` by `schedulePasses`,
which topologically sorts by `requires`/`produces`/`invalidates`. That
order is deterministic. Port them in *dependency* order, not pipeline
order — the goal is one working parity test at a time, not one working
pipeline.

| Step | Pass | Depends on | Estimated | Session |
|---|---|---|---|---|
| S0 | Preconditions P1–P5 | — | — | 5–7 sessions |
| S1 | `specificity-sorter` | P1, P2 | 1 session | — |
| S2 | `ast-optimizer` | P2, CSS-AST (§6) | 1–2 sessions | — |
| S3 | `duplicate-declaration-detector` | P2, CSS-AST (`astEqual`) | 1 session | — |
| S4 | `css-compressor` | P2, CSS-AST | 1 session | — |
| S5 | `media-query-packer` | P1, P2 | 1 session | — |
| S6 | `dead-code-eliminator` | P2, P3, P4 | 1–2 sessions | — |
| S7 | `source-optimizer` | P1, P2 | 1 session | — |
| S8 | `atomic-extractor` | P2, CSS-AST, atomic context | 1–2 sessions | — |
| S9 | `accessibility-optimizer` | P2, G36 (color parsing), G54 | 1 session | — |
| S10 | `pipeline.gol` orchestrator | all of the above | 1 session | — |
| S11 | Preset parity tests (`default` / `production` / `ci` / `lint` / `atomic`) | S10 | 1 session | — |

**Total: 13–18 sessions**, not counting slippage. That is a multi-week
effort even at one session per working day. The roadmap's
"Year 2 starts with Phase 6" is the honest shape.

**Order rationale.** `specificity-sorter` first, because it is the
only pass that needs P1 but *not* CSS-AST. Once P1 lands, this
validates the preconditions chain end-to-end with the smallest
possible pass. `ast-optimizer` second, because it introduces the
CSS-AST dependency, and by porting it early the CSS-AST port becomes
a known quantity before five more passes need it. `dead-code-eliminator`
deliberately later, because it needs P3+P4 — the two hardest
preconditions — and its parity test is the most meaningful one
(entire rules eliminated vs. not).

---

## 5. `OptimizationPass` in ALGOL26

TypeScript's `OptimizationPass` is an interface with a `name`, a
`cost`, a `requiredFor: GenerationTarget[]`, and an
`optimize(ir, context) => OptimizationResult` method. The method is a
function-valued field.

ALGOL26 has no function-valued record fields (G23). The dispatch must
be **by name**, done in `optimizer.gol`:

```gol
// In optimizer.gol

proc apply_pass(name: String, ir: StyleIR, ctx: OptimizationContext) -> OptimizationResult
    if name = "specificity-sorter" then { return specificity_sorter(ir, ctx) }
    if name = "dead-code-eliminator" then { return dead_code_eliminator(ir, ctx) }
    if name = "ast-optimizer" then { return ast_optimizer(ir, ctx) }
    // ...
    // Defensive: an unknown pass name is a scheduler bug.
    return OptimizationResult {
        ir: ir,
        savings: OptimizationSavings { rulesEliminated: 0, declarationsEliminated: 0, bytesSaved: 0 },
        changes: 0
    }
```

Then each pass file (`specificity_sorter.gol`, etc.) exposes a bare
`pub proc <pass_name>(ir: StyleIR, ctx: OptimizationContext) ->
OptimizationResult`. The `OptimizationPass` record becomes a *descriptor*
(name, cost, requiredFor) with no body, used only by the preset table.

This is the same pattern `unit_normalizer.gol` uses
(`normalize_units(ir) -> NormalizationResult` is a bare `proc`, not a
record). It's the established convention in the port.

---

## 6. The CSS-AST dependency

Four passes need `css-ast.ts`:

| Pass | What it needs |
|---|---|
| `ast-optimizer` | `parseCSSValue`, `optimizeAST`, `printAST` |
| `duplicate-declaration-detector` | `astEqual` (for structural equality) |
| `css-compressor` | `parseCSSValue`, `printAST` (for value normalization) |
| `atomic-extractor` | `parseCSSValue` (for value fingerprinting) |

`css-ast.ts` has not been pasted. It is one of the `ir/` files
(`src/compiler/pipeline/ir/css-ast.ts`), which the roadmap's Phase 4
did not require — `parse_ir.gol` operates on `Json` values directly,
and `unit_normalizer.gol` branches on `IRDeclaration.value: Json`
without going through a structured AST.

**This is a hidden sub-project.** The CSS-AST is a parsed-form
representation of declaration values (`calc(...)`, `1px solid #fff`,
etc.) supporting algebraic optimization and structural equality. It is
probably 600–1000 lines of TypeScript.

**Recommendation: port `css-ast.ts` as a pre-S2 sub-phase (call it
Phase 6.5), or explicitly scope Phase 6.0–6.4 to the four passes that
do not need it (`specificity-sorter`, `media-query-packer`,
`source-optimizer`, plus a `dead-code-eliminator` workaround that
estimates bytes from strings, which is what the current TS
`estimateRuleBytes` does anyway).**

The four CSS-AST-dependent passes become Phase 6.6+, after CSS-AST
is ported. This is not a scope reduction — it is a scoping of the
critical path. `dead-code-eliminator`, `specificity-sorter`,
`media-query-packer`, `source-optimizer` are enough to demonstrate
the pipeline shape works end-to-end. The other four can follow.

---

## 7. Parity protocol

Modeled on `normalizer_parity.gol`. For each pass:

1. **A pass-specific fixture** under `tests/chaincss/optimizer/`,
   e.g. `tests/chaincss/optimizer/specificity_sorter/` containing:
   - `input.chain.ts` (or reuse an existing corpus input)
   - `expected_after_pass.json` — the post-pass IR as produced by
     running the TS pass on the same input

2. **A parity driver** `src/chaincss/optimizer_passes/<pass>_parity.gol`
   modeled on `normalizer_parity.gol`:
   - Read `/tmp/chaincss-parity/current_input.json`
   - `parse_ir(...)` to get the input IR
   - Run only the specific pass (not the whole pipeline)
   - Serialize to `{ "ir": ... }` and print

3. **A shell driver** at `diff-corpus/run_optimizer_<pass>_parity.sh`
   modeled on `run_parity.sh` / `run_unit_parity.sh`:
   - Run `ts-export-source.ts` to produce the input JSON
   - Run the TS pass on the input
   - Run the ALGOL26 parity driver
   - Structural-compare

4. **A `test_conformance_valid` entry**? No — these are
   *pass-specific* tests, not `.gol` conformance fixtures. They live
   under `tests/chaincss/optimizer/` and run through the differential
   harness, not `cargo test`. The `cargo test`-able portion is the
   `.gol` fixture; the shell driver is what CI runs.

Because Phase 5's `run_parity.sh` exists and works, the pattern is
already established. Only the pass-specific dispatch changes.

**Acceptance criterion per pass:** the structural comparison passes on
the whole frozen corpus (`diff-corpus/inputs/*`), not just on the
specific fixture. Pass-specific fixtures are for debugging.

---

## 8. The G43 ADR

P1 needs an ADR (the decision is architectural). Suggested number:
check `docs/decisions/` for the next free number.

The ADR should cover:

- The problem: `Map.keys()` sorts, source order lost at JSON parse.
- The three options (a/b/c above) and their costs.
- Decision: option (c) `Map` order preservation + `keys_in_order()`.
- The `Map` type's contract change: `keys()` returns sorted;
  `keys_in_order()` returns insertion order. Both valid; callers
  choose.
- The compiler change: `runtime/map.c` (LLVM/WASM) + `builtins.rs`
  (interpreter) + the frontend resolution + the spec doc.
- The consequences for Phase 5: `parse_ir.gol` switches to
  `keys_in_order()`; parity comparator updates to *not* sort both
  sides (the honest test).

**Estimated:** 1 session to write, review, and merge.

---

## 9. What this document does not decide

- Whether the four CSS-AST-dependent passes are in Phase 6 or 6.5.
  Depends on a `css-ast.ts` read that has not happened yet.
- The exact form of `PassMetadata.optimization.atomic.isAtomic`.
  Depends on `atomic-extractor.ts` (not pasted).
- The shape of `OptimizationContext.extensions`. The default should
  be `Json.Obj(empty_map)` — the same as `IntentContext.extensions`
  in `pipeline_types.gol`.
- The exact parity comparator normalization (field ordering,
  timestamp normalization, diagnostic ID normalization). The Phase 5
  comparator (`diff-corpus/compare_normalized.py`) is the template.

---

## 10. Acceptance criteria (exit)

Phase 6 is done when:

1. All 9 passes ported and pass their individual parity tests on the
   full corpus.
2. `pipeline.gol` produces the same optimizer output as
   `pipeline.ts` for each of the 5 presets (`default`, `production`,
   `ci`, `lint`, `atomic`) on the full corpus.
3. Combined-pass parity holds: `run_optimizer(ir, preset)` == TS
   `pipeline.execute(ir)` post-optimization, byte-for-byte, on the
   corpus. (Byte-for-byte is achievable because after the optimizer,
   rule order is determined by `specificity-sorter` — not by the
   parser's key order — so G43 no longer affects output.)
4. Full `cargo test` suite green (no regressions in earlier phases).
5. `LANGUAGE_GAPS.md` entries for all Phase 6 discoveries (new gaps,
   resolved gaps, deferred gaps) are current.

Combined-pass byte-for-byte is the strongest possible exit; if a
specific pass produces different whitespace, diagnose whether the
difference is real or a pass-ordering artifact. Sorting at the
parser boundary (G43) and sorting at the optimizer boundary
(`specificity-sorter`) are *different* transformations even when they
produce the same rule set — a rule's position in the output
matters.

---

## 11. Related documents

- `docs/phases/` — the source-of-truth for phase-scoped plans. This
  file follows the pattern.
- `LANGUAGE_GAPS.md` — G34, G43, G46, G54 as they affect Phase 6.
- `docs/features/*.md` — feature contracts. Phase 6 does not change
  any feature contract; the passes are internal.
- `diff-corpus/` — the frozen corpus and shell drivers.
- `tests/chaincss/` — Phase 2–5 fixtures; Phase 6 adds
  `tests/chaincss/optimizer/`.

