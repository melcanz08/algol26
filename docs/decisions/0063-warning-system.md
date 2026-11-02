# ADR 0063 — Warning system

## Status

Proposed (2026-11-02). Implementation tracked in patches 1–10
(below). Will be updated to `Accepted` when all ten land, with any
divergences from this plan recorded as implementation notes.

## Context

The G29 fix (commit `ad08a53`) made mixed-Void `match` expressions
legal: the match yields `Void`, and non-Void arm values are
discarded at IR-build time. That removed real friction from real
code (the ChainCSS port's `report_one` and `main` no longer need
`val _ := report(...)` wrappers).

But the fix is silent. A user who wrote:

    match v
      case Str(s)
        report(r, ...)      # returns Diagnostic -- discarded
        print(s)
      case _
        {}

now gets the correct behavior *without any indication that
`report(r, ...)` produced a value that was discarded*. That's
fine when the discard is intentional (it usually is, in a `Void`
proc's statement-position match). It's not fine when the discard
is a mistake (the user thought the match produced a value and
would use it later).

The right answer is a **warning**: "non-Void arm value discarded
in mixed-Void match." ALGOL26 has no warning system today. Every
diagnostic is fatal. The `CompileError` struct carries a
`severity: Severity` field, but every consumer treats it as
error-only: `Result<(), CompileError>` propagates with `?`,
scheduler treats any `Err` as failure, CLI renders any `Err` with
`error[E####]:` prefix.

This ADR establishes the warning system that G29's warning will be
the first user of. It is deliberately designed as a subsystem, not
as a G29-specific hack: every future warning (unused variables,
deprecated APIs, unreachable code) will use this infrastructure.

## Decision

### 1. `CompileWarning` is a distinct type.

New struct in `src/common/diagnostics.rs`, parallel to
`CompileError` but non-fatal by type:

    pub struct CompileWarning {
        pub span: Span,
        pub message: String,
        pub suggestion: Option<String>,
        pub code: String,           // "W0001", "W0002", ...
        pub notes: Vec<String>,
    }

**Rejected: reuse `CompileError` with `Severity::Warning`.**
Cheaper in the short term, but the type-level separation matters:
- A `Result<T, CompileError>` cannot carry a warning. That's
  structurally enforced if warnings are a different type.
- Every `?` chain in the pipeline would need to check
  `severity` to decide whether to fail — and the "warnings that
  pretend to be errors become errors eventually" failure mode is
  real (a future contributor will forget the check).
- `CompileError` has fields (`error_code`, `secondary`,
  `label`) that don't apply to warnings. Reusing it means those
  fields exist-but-are-unused on the warning path.

Type-level separation is not free (duplication of `Display`,
`Clone`, `Debug` impls) but it earns its keep at the interface.

### 2. Warnings live on `TypedProgram`.

`TypedProgram` (in `src/compiler.rs`) gains:

    pub warnings: Vec<CompileWarning>,

`type_check_program` populates it from
`SemanticAnalyzer::take_warnings()`.

**Rejected: separate channel via tuple return** —
`Result<(TypedProgram, Vec<CompileWarning>)>`. This would break
every caller of `type_check_program` and require cascading
signature changes through the pass infrastructure.

**Rejected: side channel on `CompilerContext`.** Warnings
describe properties of the *program*, not of the *compiler's own
state*. `CompilerContext` is process-scoped; warnings are
program-scoped. Attaching warnings to the program means any
consumer of `TypedProgram` (IR builder, optimizers, emitters) can
both *read* accumulated warnings and *produce* new ones, which
appends to the same list.

### 3. Ordering: by pass completion, then by span.

Warnings produced by a pass are ordered by span (source order)
within that pass. Across passes, warnings are ordered by pass
completion: analyzer warnings first, then later passes.

**Rationale:** deterministic. Warnings from the same pass on the
same file cluster together, which aids reading.

**Deferred:** cross-pass span ordering (interleave all warnings
across all passes by span) requires a stable total order across
passes that each pass currently produces independently. Moving
to that would require every pass to know the global ordering
constraint. ADR follow-up.

### 4. Per-occurrence policy: every occurrence fires.

If a file has 47 mixed-Void matches, the user sees 47 W0001
warnings.

**Rejected: first-per-function, once-per-file.** Those hide
information. Users who don't want 47 warnings have suppression
(config-level, below).

**Rejected: summary counts** (`47 mixed-Void matches in this
file`). The summary loses the span — the user can't jump to the
occurrences. Suppression is the right escape hatch, not
aggregation.

### 5. Span selection: the whole match expression.

For the G29 warning specifically, the span points at the whole
`match` expression, not at the offending arm's trailing
expression, not at the `match` keyword.

**Rationale:** the *match* is the site of the design decision.
Pointing at the arm's trailing expression would blame the arm
(when the value-producing expression is often fine in
isolation); pointing at the keyword would be too vague. The
whole-match span tells the user "this match, as written, discards
a value" without implying any specific sub-expression is wrong.

**Bug found during design:** the current `Match` arm in
`analyze_expr_inner` captures `self.current_span` at the *start*
of the match analysis — but `self.current_span` is updated by
every nested `analyze_expr` call. By the time the `any_void`
branch fires (after all arm bodies have been analyzed), the span
is the *last* arm's last sub-expression, not the match's. **Patch
9 fixes this** by saving the match's span at the start of the
`Match` arm and using the saved value at emission time.

**Pattern for future warnings:** for a warning about a compound
expression (match, if, for, while), point at the expression's
span, not at a sub-expression. This becomes the template.

### 6. Suppression: config-level only in v1.

`algol26 check --no-warn=W0001 file.gol` disables the warning
globally for the invocation.

**Deferred: source-level suppression** (`#[allow(mixed_void_discard)]`
on a match, function, or file). Source-level suppression requires
attribute syntax, which doesn't exist in ALGOL26. Adding it is a
separate feature with its own ADR — the attribute parser, the
attribute-to-warning-code mapping, scoped suppression rules
(function vs. statement vs. expression). Not in v1.

**Why config-level is enough for v1:** the warning this ADR
motivates (W0001, mixed-Void discard) is a *shape* — if a
codebase adopts the pattern intentionally (which the ChainCSS
port does), it disables the warning globally. Per-occurrence
suppression is only needed when a codebase has *mostly* the
pattern but a few intentional exceptions, which is rarer.

### 7. `--deny-warnings` promotes to a compile error.

`algol26 check --deny-warnings file.gol` treats any warning as a
compile failure. The pipeline runs, warnings accumulate, the
driver checks `WarningConfig.is_denied` for each produced warning,
and any denied warnings abort compilation.

**Rendering:** identical to a real error:

    error[W0001]: non-Void arm value discarded in mixed-Void match
      --> file.gol:12:5
       |
    12 |     match v
       |     ^^^^^^^
       |
      = help: if you meant to discard the value, this is fine; ...

The `W` prefix in the code distinguishes "denied warning" from
"real error." A user reading the output can tell which.

**Rejected: `--deny-warnings` transforms the error code to
`E9999`** or some other error sentinel. Keeping the `W` prefix
preserves the link back to the warning catalog.

### 8. Warnings carry through `ScheduleOutcome`.

`ScheduleOutcome` (in `src/compiler/scheduler.rs`) gains:

    pub warnings: Vec<CompileWarning>,

Each pass that produces warnings populates them; the scheduler
appends them to the outcome. `run_pipeline` returns the
`ScheduleOutcome`; the driver reads `outcome.warnings`.

**Why the scheduler, not just the driver:** the scheduler is where
pass boundaries are enforced. If `TypeCheckPass` produces
warnings and `BuildSemanticIRPass` produces warnings, ordering
them (decision 3: by pass completion) requires the scheduler to
be in the loop. Driver-only aggregation would lose the
per-pass grouping.

### 9. CompileWarning and CompileError share Display logic via a helper.

Both structs implement `Display`. They share the message +
suggestion rendering pattern:

    write!(f, "{}", self.message)?;
    if let Some(s) = &self.suggestion {
        write!(f, "\n  = help: {}", s)?;
    }
    Ok(())

**Not** abstracted into a shared trait in v1 — two implementations
of three lines each is cheaper than a trait with two impls. If a
third diagnostic type emerges (Notes? Hints?), revisit.

**Rationale:** premature abstraction is worse than duplication.
The pattern may diverge (e.g., warnings might want "considered
helpful but not critical" prefix) — better to keep them separate
until a concrete reason to unify appears.

### 10. G29's warning code is W0001.

Codes are `W0001`, `W0002`, etc. Sequential. No bucketing yet
(unlike `ErrorCode::E0001..E0009` which are bucket-coded).

**Deferred:** warning code categories (e.g., `W1xxx` = discard
warnings, `W2xxx` = unused warnings). Bucketing is useful when
the warning catalog grows to dozens; with two or three warnings
the sequential codes are clearer. Adopt bucketing when the
catalog crosses ~10 warnings or when a tool wants to filter by
category.

## Implementation plan

Ten patches, dependency-ordered. Each is a separate commit (or
small group of commits) referencing this ADR.

1. **`CompileWarning` type.** `src/common/diagnostics.rs`.
   Includes `Display` impl and `CompileWarning::new(...)` constructor.

2. **`WarningConfig` type.** `src/common/diagnostics.rs`. Fields:
   `disabled: HashSet<String>`, `denied: HashSet<String>`. Methods:
   `is_enabled(code) -> bool`, `is_denied(code) -> bool`,
   `with_disabled(codes)`, `with_denied(codes)`. Default: all
   enabled, nothing denied.

3. **`SemanticAnalyzer.warnings` + `warning_config`.** Fields on
   the analyzer. `take_warnings()` method. `warning_config` threaded
   through `analyze_with_spans`.

4. **`TypedProgram.warnings`.** Field. Populated in
   `type_check_program` from `analyzer.take_warnings()`.

5. **Analyzer emits G29 warning.** One push at the `any_void`
   branch in `analyze_expr_inner`'s `Match` arm. Checks
   `warning_config.is_enabled("W0001")` before pushing.

6. **`ScheduleOutcome.warnings`.** Field. `TypeCheckPass` copies
   `TypedProgram.warnings` into the outcome; `Scheduler::run`
   aggregates across passes.

7. **Driver aggregation + denial check.** `Compiler::run_pipeline`
   returns warnings. `Compiler::compile` and `run_interpreter` read
   them. `WarningConfig.is_denied` promotes denied warnings to
   `CompileError`-shaped failures.

8. **CLI flags + rendering.** `--no-warn=<codes>`,
   `--deny-warnings`, `--json`. Plain rendering matches
   `error[E####]` shape but with `warning[W####]` prefix.

9. **Span selection fix for `Match`.** Save the match's span at
   the start of the `Match` arm; use it at the `any_void` branch
   instead of `self.current_span`.

10. **Tests + docs.** Regression tests for each layer
    (`CompileWarning` Display, `WarningConfig` behavior, analyzer
    produces warning, pipeline propagates, driver denies when
    configured, CLI renders in 4 modes, span points at match).
    Update `LANGUAGE_GAPS.md` G29 to mark warning resolved.

## Deferred (future ADRs)

- **Source-level suppression** (`#[allow(...)]`-style). Requires
  attribute syntax. Its own ADR.
- **Cross-pass span ordering.** Currently warnings are grouped by
  pass; interleaving by span across passes is deferred.
- **Warning code categories** (`W1xxx`, `W2xxx`, etc.). Adopt when
  the catalog grows past ~10 warnings.
- **Warning hierarchies** — parent-child warnings (e.g., "unused
  variable" containing "here's where it's declared"). No use case
  yet.
- **Warning deprecation cycles** — a warning that becomes an
  error after N releases. Requires version tracking in the
  compiler. No use case yet.
- **JSON warning output for IDE/CI consumption.** Part of patch 8
  (the `--json` flag), but the full schema (structured spans,
  source snippets, fix suggestions as machine-readable diffs) is
  deferred.
- **Warning-to-error promotion at project level** (in a config
  file, not just CLI flags). Would need a project config format;
  CLI flags are the v1 answer.

## Consequences

### Positive

- First-class warning subsystem unblocks every future warning.
  "Unused variable," "deprecated API," "unreachable code," "shadowed
  variable," "unused import" — all become a matter of adding a
  warning code and a `self.warnings.push(...)` at the emission site.
- `--deny-warnings` gives CI a hard gate without language changes.
  A team can adopt "warnings are errors" as a policy without
  waiting for attribute syntax.
- Deterministic — no span-ordering ambiguity in v1.
- Type-level separation of warnings from errors prevents
  "warnings that become errors eventually."

### Negative

- `CompileWarning` and `CompileError` duplicate `span`, `message`,
  `suggestion`, `Display`. Cost is small (~30 lines of duplicate
  code) but real. Unification is deferred.
- Warnings are grouped by pass, not interleaved by span. Warnings
  from the analyzer and warnings from a later pass about the same
  region will appear in separate groups. Deferred improvement.
- No source-level suppression until attribute syntax lands. Users
  who want per-occurrence control have to use `--no-warn=` per
  invocation (or set it as a shell alias / build script argument).
- The G29 warning (W0001) becomes the *first* warning and
  implicitly defines the template for later warnings. If its
  shape (span + message + suggestion + code) is wrong, every
  subsequent warning inherits the mistake. Patch 5 needs careful
  review, and future warnings should be reviewed against this
  template before adopting a different shape.

### Neutral

- The warning system is designed *with G29 as its first use
  case*, but not *for* G29. If a future warning needs different
  infrastructure (e.g., a warning that fires from the *optimizer*
  rather than the analyzer), the design should be extended rather
  than worked around. This ADR doesn't preclude extension.
- The decision to defer source-level suppression is a real
  constraint on v1. A user who wants per-occurrence suppression
  has to wait. **That's a defensible tradeoff only if the warning
  catalog stays small** — once there are 20 warnings, config-level
  suppression becomes burdensome and source-level becomes
  necessary. That's the trigger to revisit.

## References

- `ADR 0018` — canonical pipeline (the pass infrastructure this
  builds on)
- `ADR 0030` — enum types (the first feature whose mixed-Void
  match pattern motivates the warning)
- `LANGUAGE_GAPS.md` G29 — the deferred warning this ADR unblocks
- `LANGUAGE_GAPS.md` G31 — the `CompileError::Display` fix (the
  pattern `CompileWarning::Display` follows)
- `LANGUAGE_GAPS.md` G32 — deferred unified renderer (which will
  eventually consume `CompileWarning` too)
- `tools/feature_inventory.py` — the classification of "warnings
  live on `TypedProgram`" means the checker's `feature_doc` /
  `conformance_valid` schema still applies; warnings are not a
  separate inventory dimension.

## Implementation tracking

As patches land, this section is updated with commit hashes.

- Patch 1: (pending)
- Patch 2: (pending)
- Patch 3: (pending)
- Patch 4: (pending)
- Patch 5: (pending)
- Patch 6: (pending)
- Patch 7: (pending)
- Patch 8: (pending)
- Patch 9: (pending)
- Patch 10: (pending)

## Backfill note

This ADR is written *before* the patches, unlike 0019, 0020, 0021,
0062 (which were backfilled). That's deliberate: the six design
decisions in §Decision are real forks with no obvious answer, and
committing to one before implementation means each patch reflects
the agreed design rather than "whatever the first patch happened
to do." If patches reveal a decision was wrong, this ADR is
updated (with an implementation note explaining the change) rather
than silently diverging.
