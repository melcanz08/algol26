// src/diagnostics/snippet.rs

//! Source snippet extraction for the rustc-style renderer.
//!
//! Given source text and a `Span`, produce the two lines that appear
//! under an error header:
//!
//! ```text
//! 12 |     x := 5.5
//!    |     ^^^^^^^ expected Int, found Float
//! ```
//!
//! `Span` carries 1-based line/column pairs and no byte offsets, so
//! column arithmetic here works in *display columns* (Unicode scalar
//! values, tabs expanded to a fixed width), not bytes.

use crate::common::span::Span;

/// Tab width used when expanding tabs to spaces. Matches the
/// convention used by rustc and most editors.
const TAB_WIDTH: usize = 4;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RenderedSnippet {
    /// Two lines: the source excerpt, then the caret line. Empty if
    /// the span carries no usable location.
    pub lines: Vec<String>,
}

/// Render the source line(s) covered by `span`, plus a caret line
/// underneath. Returns an empty snippet when the span is unset
/// (`start_line == 0`) or points past the end of `source` — callers
/// should treat an empty result as "no snippet to show."
///
/// Multi-line spans render a single caret at the start column.
/// Proper continuation-line rendering is deferred; the current
/// behavior matches `renderer::caret_for`, which also collapses
/// multi-line spans to a single caret.
pub fn render_snippet(
    source: &str,
    span: Span,
    label: Option<&str>,
    gutter_width: usize,
) -> RenderedSnippet {
    if span.start_line == 0 {
        return RenderedSnippet { lines: Vec::new() };
    }

    let source_lines: Vec<&str> = source.lines().collect();
    let line_idx = span.start_line.saturating_sub(1);
    let Some(raw_line) = source_lines.get(line_idx) else {
        return RenderedSnippet { lines: Vec::new() };
    };

    let expanded = expand_tabs(raw_line, TAB_WIDTH);
    let chars: Vec<char> = expanded.chars().collect();
    let line_len = chars.len();

    let start_col = span.start_column.saturating_sub(1).min(line_len);

    let width = if span.start_line == span.end_line && span.end_column > span.start_column {
        // Inclusive underline: `end_column` is the last covered column.
        span.end_column
            .saturating_sub(start_col)
            .min(line_len - start_col)
    } else {
        1
    };
    let width = width.max(1);

    let line_num = format!("{:>width$}", span.start_line, width = gutter_width);
    let blank_gutter = " ".repeat(gutter_width);

    let mut lines = Vec::with_capacity(2);
    lines.push(format!("{} | {}", line_num, expanded));

    let caret_body = format!(
        "{}{}{}",
        " ".repeat(start_col),
        "^".repeat(width),
        label.map(|l| format!(" {}", l)).unwrap_or_default(),
    );
    lines.push(format!("{} | {}", blank_gutter, caret_body));

    RenderedSnippet { lines }
}

/// Expand tabs to `tab_width`-aligned spaces. Alignment is computed
/// from the number of display columns emitted so far, so a tab
/// following "ab" at width 4 becomes two spaces, not four.
fn expand_tabs(s: &str, tab_width: usize) -> String {
    let mut out = String::with_capacity(s.len());
    let mut col = 0usize;
    for c in s.chars() {
        if c == '\t' {
            let spaces = tab_width - (col % tab_width);
            out.extend(std::iter::repeat(' ').take(spaces));
            col += spaces;
        } else {
            out.push(c);
            col += 1;
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn point_span_renders_single_caret() {
        let source = "let x = 5\n";
        let span = Span::point(1, 5);
        let s = render_snippet(source, span, None, 1);
        assert_eq!(s.lines.len(), 2);
        assert_eq!(s.lines[0], "1 | let x = 5");
        assert_eq!(s.lines[1], "  |     ^");
    }

    #[test]
    fn range_span_renders_underline() {
        let source = "let x = foo(1, 2, 3)\n";
        // Columns 9..=19 cover `foo(1, 2, 3)`.
        let span = Span::new(1, 9, 1, 20);
        let s = render_snippet(source, span, None, 1);
        assert_eq!(s.lines[0], "1 | let x = foo(1, 2, 3)");
        // 8 leading spaces + 12 carets.
        assert!(s.lines[1].contains("^^^^^^^^^^^^"), "got: {:?}", s.lines[1]);
    }

    #[test]
    fn label_appended_after_carets() {
        let source = "    x := 5.5\n";
        let span = Span::new(1, 10, 1, 13);
        let s = render_snippet(source, span, Some("expected Int, found Float"), 1);
        assert!(
            s.lines[1].contains("^^^ expected Int, found Float"),
            "got: {:?}",
            s.lines[1]
        );
    }

    #[test]
    fn tab_expands_to_four_spaces() {
        let source = "\tlet x = 5\n";
        // Tab expanded to 4 spaces, so `let` starts at display column 5.
        let span = Span::point(1, 5);
        let s = render_snippet(source, span, None, 1);
        assert_eq!(s.lines[0], "1 |     let x = 5");
        assert_eq!(s.lines[1], "  |     ^");
    }

    #[test]
    fn tab_mid_line_aligns_to_tab_stop() {
        let source = "ab\tcd\n";
        // "ab" occupies cols 1-2, tab expands to 2 spaces (to col 4),
        // so "cd" starts at column 5.
        let span = Span::point(1, 5);
        let s = render_snippet(source, span, None, 1);
        assert_eq!(s.lines[0], "1 | ab  cd");
        assert_eq!(s.lines[1], "  |     ^");
    }

    #[test]
    fn unicode_columns_count_chars_not_bytes() {
        let source = "let s = \"héllo\"\n";
        // "héllo" — the `é` is one display column even though it's
        // two UTF-8 bytes.
        let span = Span::point(1, 10);
        let s = render_snippet(source, span, None, 1);
        // 9 leading spaces, then caret.
        assert_eq!(s.lines[1], "  |          ^");
    }

    #[test]
    fn out_of_range_span_does_not_panic() {
        let source = "hi\n";
        let span = Span::point(5, 99);
        let s = render_snippet(source, span, None, 1);
        assert!(s.lines.is_empty());
    }

    #[test]
    fn multiline_span_single_caret_at_start() {
        let source = "foo(\n    1,\n    2\n)\n";
        let span = Span::new(1, 4, 3, 5);
        let s = render_snippet(source, span, None, 1);
        // First line only, single caret at column 4.
        assert_eq!(s.lines[0], "1 | foo(");
        assert_eq!(s.lines[1], "  |    ^");
    }

    #[test]
    fn line_zero_returns_empty() {
        let source = "hello\n";
        let s = render_snippet(source, Span::default(), None, 1);
        assert!(s.lines.is_empty());
    }

    #[test]
    fn gutter_width_pads_line_number() {
        let source = "one\ntwo\nthree\n";
        let span = Span::point(3, 1);
        let s = render_snippet(source, span, None, 3);
        assert_eq!(s.lines[0], "  3 | three");
        assert_eq!(s.lines[1], "    | ^");
    }

    #[test]
    fn empty_source_returns_empty() {
        let s = render_snippet("", Span::point(1, 1), None, 1);
        assert!(s.lines.is_empty());
    }

    #[test]
    fn caret_never_zero_width() {
        // Degenerate span: start_column > end_column on the same line.
        let source = "hello\n";
        let span = Span::new(1, 4, 1, 2);
        let s = render_snippet(source, span, None, 1);
        // Width clamps to 1, not 0.
        assert_eq!(s.lines[1], "  |    ^");
    }
}
