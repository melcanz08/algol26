# Diagnostics — current state (Phase 0 investigation)

## Where diagnostics live

`CompileError` (in `src/common/diagnostics.rs`) is the sole error type returned through the pipeline. It carries `message: String`, `span: Span`, `source_line: String`, `error_code: ErrorCode` (a coarse bucket, `E0001..E0009`), `suggestion: Option<String>`, and `file: Option<String>`. `Diagnostic` is a two-variant enum (`Error(CompileError)` / `Warning(String)`) used for accumulation in `CompilerContext::diagnostics`. A parallel, unstructured vocabulary of `E-XXX-NNN` sub-codes exists inside `message` strings; unifying them is future work per the `ErrorCode` doc comment.

## Span shape

`Span` is four 1-based `usize` fields (`start_line`, `start_column`, `end_line`, `end_column`) with no byte offsets, no length, and no source reference. Column semantics are pinned by `Span::contains` tests. Snippet extraction therefore requires scanning source text to locate line boundaries and count display columns — it cannot slice by offset.

## Source flow

The source line is stored on the error itself (`source_line`), populated at construction time by the caller. `CompileError::simple(...)` and `::new(...)` receive it; `CompileError::at(span, ...)` sets it empty. The renderer (`src/diagnostics/renderer.rs`) reads this field and cannot show multi-line spans, context lines, or secondary spans. The clean inversion is: leave `source_line` in place as vestigial, add `source: Option<&str>` to `render_one`/`render_all`, and supply the source from `main.rs` where the file is already in scope. This gives every existing error snippet rendering without touching the 217 constructor call sites.

## Call-site counts

217 sites construct a `CompileError` (`simple`/`at`/`new` combined). 34 sites call `.display()`. The `eprintln!`/`eprint!` count needs a re-run (the previous grep was mangled by shell backtick substitution). Most of the 217 are unenriched legacy calls that need no migration: once Phase 3 lands, they render with carets and snippets automatically. Phase 5's real scope is the 20–40 analyzer sites worth enriching with labels, secondary spans, and notes.

## What tests pin

`tests/integration/diagnostic_test.rs` checks `ErrorCode::as_str()`, the field values set by `CompileError::new(...).with_suggestion(...)`, and that `Diagnostic::Warning(...).display()` does not panic — nothing about rendered output. `tests/integration/diagnostics_quality_test.rs` is more substantive: `E0003` for undefined vars with the name in the message, `E0002` for type mismatch, `E0007` for double-borrow and use-after-move, `suggestion.is_some()` for four of the error classes, and — critically — `diag_position_points_at_condition` asserts a real `line == 3, column == 8` for a specific source, not `0:0`. Rendered-output format is not pinned by any test, which means the renderer can change freely as long as message/code/suggestion/span content is preserved. One latent bug: `test_all_negative_corpus_produce_structured_errors` iterates `tests/integration/negative/` filtering on extension `.al26`, but the files are `.gol`, so the loop is currently a no-op.