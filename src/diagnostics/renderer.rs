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

use crate::common::diagnostics::{CompileError, Diagnostic};

/// Render a single error. Trailing newline included.
pub fn render_one(err: &CompileError) -> String {
    let mut out = String::new();

    // Header line
    out.push_str(&format!(
        "error[{}]: {}\n",
        err.error_code.as_str(),
        err.message
    ));

    // Location + source excerpt
    if err.line() > 0 {
        out.push_str(&format!("  --> {}:{}\n", err.line(), err.column()));

        if !err.source_line.is_empty() {
            let line_num = err.line().to_string();
            let gutter = " ".repeat(line_num.len());

            out.push_str(&format!("{} |\n", gutter));
            out.push_str(&format!("{} | {}\n", line_num, err.source_line));

            // Caret: width comes from the span when it's on one line,
            // otherwise a single caret at the start column.
            let (caret_col, caret_width) = caret_for(err);
            out.push_str(&format!(
                "{} | {}{}\n",
                gutter,
                " ".repeat(caret_col.saturating_sub(1)),
                "^".repeat(caret_width)
            ));
            out.push_str(&format!("{} |\n", gutter));
        }
    }

    // Help
    if let Some(s) = &err.suggestion {
        out.push_str(&format!("  = help: {}\n", s));
    }

    out
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
    // No usable span: default to column 1, width 1.
    (1, 1)
}

/// Render a batch of diagnostics with a summary footer.
///
/// Footer appears only when more than one diagnostic is present.
pub fn render_all(diags: &[Diagnostic]) -> String {
    let mut out = String::new();
    let mut errors = 0usize;
    let mut warnings = 0usize;

    for d in diags {
        match d {
            Diagnostic::Error(e) => {
                errors += 1;
                out.push_str(&render_one(e));
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
        assert!(s.contains("12 |     let x = foo()"), "missing snippet:\n{}", s);
    }

    #[test]
    fn caret_width_from_single_line_span() {
        let mut err = CompileError::new("call arity mismatch", 12, 5, "    let x = foo(1,2,3)", ErrorCode::E0002);
        err = err.with_span(Span::new(12, 13, 12, 21));
        let s = render_one(&err);
        // 12 chars of gutter + 1 space, then 12 spaces before `^`
        // (start_column 13 -> 12 leading spaces), then 9 carets.
        assert!(s.contains("^^^^^^^^^"), "wrong caret width:\n{}", s);
    }

    #[test]
    fn renders_help_line_when_suggestion_present() {
        let err = CompileError::new("bad", 1, 1, "", ErrorCode::E0001)
            .with_suggestion("try something else");
        let s = render_one(&err);
        assert!(s.contains("= help: try something else"), "missing help:\n{}", s);
    }

    #[test]
    fn batch_summary_appears_for_multiple_diagnostics() {
        let e1 = Diagnostic::Error(CompileError::new("a", 1, 1, "", ErrorCode::E0001));
        let e2 = Diagnostic::Warning("b".to_string());
        let s = render_all(&[e1, e2]);
        assert!(s.contains("1 error(s), 1 warning(s) generated"), "no summary:\n{}", s);
    }

    #[test]
    fn batch_summary_absent_for_single_diagnostic() {
        let e = Diagnostic::Error(CompileError::new("a", 1, 1, "", ErrorCode::E0001));
        let s = render_all(&[e]);
        assert!(!s.contains("generated"), "unexpected summary:\n{}", s);
    }
}