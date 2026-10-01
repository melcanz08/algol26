// src/diagnostics/renderer.rs

//! Renders `CompileError` and `Diagnostic` as human-readable text.
//!
//! Format:
//!
//! ```text
//! error[E0002]: message
//!   --> 12:5
//!    |
//! 12 |     let x = foo(1, 2, 3)
//!    |             ^^^^^^^^^^^^^
//!    |
//!    = help: remove one argument
//! ```
//!
//! Everything is a `String`; the caller decides where it goes.
//! `CompileError::display` prints one via `eprintln!`; a future
//! `algol26 inspect` subcommand can print a batch.

use crate::common::diagnostics::{CompileError, Diagnostic, Severity};
use crate::diagnostics::snippet::render_snippet;

/// Render a single error. Trailing newline included.
///
/// Legacy signature. Uses `err.source_line` for the snippet, which
/// means it cannot show multi-line spans and has no access to the
/// surrounding source. New callers that have the full source text
/// in scope should use `render_one_with_source`.
pub fn render_one(err: &CompileError) -> String {
    render_one_with_source(err, None)
}

/// Render a single error. When `source` is `Some`, the renderer
/// extracts the covered line(s) itself; when `None`, it falls back
/// to `err.source_line`, preserving pre-Phase-3 behavior.
pub fn render_one_with_source(err: &CompileError, source: Option<&str>) -> String {
    let mut out = String::new();

    // Header line. Warnings and notes use their own labels so the
    // renderer can be reused for non-fatal diagnostics without a
    // separate code path.
    let severity_label = match err.severity {
        Severity::Error => "error",
        Severity::Warning => "warning",
        Severity::Note => "note",
    };
    out.push_str(&format!(
        "{}[{}]: {}\n",
        severity_label,
        err.error_code.as_str(),
        err.message
    ));

    // Location + source excerpt
    if err.line() > 0 {
        match &err.file {
            Some(f) => out.push_str(&format!("  --> {}:{}:{}\n", f, err.line(), err.column())),
            None => out.push_str(&format!("  --> {}:{}\n", err.line(), err.column())),
        }

        let snippet_lines = compute_snippet(err, source);
        if !snippet_lines.is_empty() {
            let gutter = " ".repeat(err.line().to_string().len());
            out.push_str(&format!("{} |\n", gutter));
            for line in &snippet_lines {
                out.push_str(line);
                out.push('\n');
            }
            out.push_str(&format!("{} |\n", gutter));
        }
    }

    // Help line
    if let Some(s) = &err.suggestion {
        out.push_str(&format!("  = help: {}\n", s));
    }

    // Notes
    for note in &err.notes {
        out.push_str(&format!("  = note: {}\n", note));
    }

    // Secondary spans. Phase 6 will render these inline as
    // `label` under their own caret lines. For now they surface
    // as notes so the data model has a visible path through the
    // renderer and Phase 6 can replace this loop.
    for (span, label) in &err.secondary {
        out.push_str(&format!("  = note: {} ({})\n", label, span));
    }

    out
}

/// Choose between the real snippet renderer (when source is
/// available) and the legacy `source_line` path (when it is not).
fn compute_snippet(err: &CompileError, source: Option<&str>) -> Vec<String> {
    if let Some(src) = source {
        let gutter = err.line().to_string().len();
        let s = render_snippet(src, err.span, err.label.as_deref(), gutter);
        return s.lines;
    }
    legacy_snippet(err)
}

/// Pre-Phase-3 snippet rendering: uses `err.source_line` directly.
/// Preserved so `CompileError::display()` and existing tests keep
/// working without a source argument.
fn legacy_snippet(err: &CompileError) -> Vec<String> {
    if err.source_line.is_empty() {
        return Vec::new();
    }
    let gutter = err.line().to_string().len();
    let blank = " ".repeat(gutter);
    let (caret_col, caret_width) = caret_for(err);
    let label_suffix = err
        .label
        .as_deref()
        .map(|l| format!(" {}", l))
        .unwrap_or_default();

    let mut lines = Vec::with_capacity(2);
    lines.push(format!("{} | {}", err.line(), err.source_line));
    lines.push(format!(
        "{} | {}{}{}",
        blank,
        " ".repeat(caret_col.saturating_sub(1)),
        "^".repeat(caret_width),
        label_suffix,
    ));
    lines
}

/// Column and width for the caret line.
///
/// Column is 1-based; width is at least 1. Multi-line spans fall back
/// to a single caret at the start column — underlining across line
/// breaks in a single-line snippet isn't meaningful.
fn caret_for(err: &CompileError) -> (usize, usize) {
    let span = &err.span;
    if span.start_line == span.end_line && span.start_column > 0 {
        let start = span.start_column;
        let end = span.end_column.max(start);
        return (start, end - start + 1);
    }
    if span.start_column > 0 {
        return (span.start_column, 1);
    }
    (1, 1)
}

/// Render a batch of diagnostics with a summary footer.
///
/// Footer appears only when more than one diagnostic is present.
pub fn render_all(diags: &[Diagnostic]) -> String {
    render_all_with_source(diags, None)
}

/// Batch variant that passes `source` through to each error's
/// `render_one_with_source`. Warnings (which are `String` today,
/// not `CompileError`) do not use the source.
pub fn render_all_with_source(diags: &[Diagnostic], source: Option<&str>) -> String {
    let mut out = String::new();
    let mut errors = 0usize;
    let mut warnings = 0usize;

    for d in diags {
        match d {
            Diagnostic::Error(e) => {
                errors += 1;
                out.push_str(&render_one_with_source(e, source));
            }
            Diagnostic::Warning(w) => {
                warnings += 1;
                out.push_str(&format!("warning: {}\n", w));
            }
        }
    }

    if errors + warnings > 1 {
        out.push_str(&format!(
            "\n{} error(s), {} warning(s) generated\n",
            errors, warnings
        ));
    }

    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::common::diagnostics::ErrorCode;
    use crate::common::span::Span;

    #[test]
    fn renders_header_and_location() {
        let err = CompileError::new("oops", 12, 5, "    let x = foo()", ErrorCode::E0002);
        let s = render_one(&err);
        assert!(s.contains("error[E0002]: oops"), "missing header:\n{}", s);
        assert!(s.contains("--> 12:5"), "missing location:\n{}", s);
        assert!(
            s.contains("12 |     let x = foo()"),
            "missing snippet:\n{}",
            s
        );
    }

    #[test]
    fn caret_width_from_single_line_span() {
        let mut err = CompileError::new(
            "call arity mismatch",
            12,
            5,
            "    let x = foo(1,2,3)",
            ErrorCode::E0002,
        );
        err = err.with_span(Span::new(12, 13, 12, 21));
        let s = render_one(&err);
        assert!(s.contains("^^^^^^^^^"), "wrong caret width:\n{}", s);
    }

    #[test]
    fn renders_help_line_when_suggestion_present() {
        let err = CompileError::new("bad", 1, 1, "", ErrorCode::E0001)
            .with_suggestion("try something else");
        let s = render_one(&err);
        assert!(
            s.contains("= help: try something else"),
            "missing help:\n{}",
            s
        );
    }

    #[test]
    fn batch_summary_appears_for_multiple_diagnostics() {
        let e1 = Diagnostic::Error(CompileError::new("a", 1, 1, "", ErrorCode::E0001));
        let e2 = Diagnostic::Warning("b".to_string());
        let s = render_all(&[e1, e2]);
        assert!(
            s.contains("1 error(s), 1 warning(s) generated"),
            "no summary:\n{}",
            s
        );
    }

    #[test]
    fn batch_summary_absent_for_single_diagnostic() {
        let e = Diagnostic::Error(CompileError::new("a", 1, 1, "", ErrorCode::E0001));
        let s = render_all(&[e]);
        assert!(!s.contains("generated"), "unexpected summary:\n{}", s);
    }

    #[test]
    fn renders_filename_when_present() {
        let err = CompileError::new("oops", 12, 5, "    let x = foo()", ErrorCode::E0002)
            .with_file("main.gol");
        let s = render_one(&err);
        assert!(
            s.contains("--> main.gol:12:5"),
            "expected filename in location:\n{}",
            s
        );
    }

    // ─── Phase 3: source-aware rendering ─────────────────────────

    #[test]
    fn source_argument_produces_snippet() {
        let source = "procedure main\n    var x := 5.5\n    print x\n";
        let err = CompileError::at(Span::new(2, 5, 2, 7), "type mismatch", ErrorCode::E0002)
            .with_file("main.gol");
        let s = render_one_with_source(&err, Some(source));
        assert!(s.contains("error[E0002]: type mismatch"), "{}", s);
        assert!(s.contains("--> main.gol:2:5"), "{}", s);
        assert!(s.contains("2 |     var x := 5.5"), "{}", s);
        assert!(s.contains("^^^"), "{}", s);
    }

    #[test]
    fn label_rendered_after_carets() {
        let source = "    var x: Int := 5.5\n";
        let err = CompileError::at(Span::new(1, 19, 1, 21), "type mismatch", ErrorCode::E0002)
            .with_label("expected Int, found Float");
        let s = render_one_with_source(&err, Some(source));
        assert!(s.contains("^^^ expected Int, found Float"), "{}", s);
    }

    #[test]
    fn notes_rendered_as_note_lines() {
        let err =
            CompileError::at(Span::point(1, 1), "boom", ErrorCode::E0002).with_note("see ADR 0005");
        let s = render_one(&err);
        assert!(s.contains("= note: see ADR 0005"), "{}", s);
    }

    #[test]
    fn warning_severity_uses_warning_label() {
        let err = CompileError::warning("unused var", Span::point(1, 1), ErrorCode::E0002);
        let s = render_one(&err);
        assert!(s.starts_with("warning[E0002]"), "{}", s);
    }

    #[test]
    fn source_none_falls_back_to_source_line() {
        let err = CompileError::new("oops", 12, 5, "    let x = foo()", ErrorCode::E0002);
        let s = render_one_with_source(&err, None);
        assert!(s.contains("12 |     let x = foo()"), "{}", s);
    }

    #[test]
    fn secondary_spans_rendered_as_notes_for_now() {
        let err = CompileError::at(Span::point(5, 5), "use after move", ErrorCode::E0007)
            .with_secondary(Span::point(3, 5), "moved here");
        let s = render_one(&err);
        assert!(s.contains("= note: moved here"), "{}", s);
    }
}
