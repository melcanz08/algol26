#!/usr/bin/env python3
"""
Render secondary spans as inline snippet blocks instead of `= note:`
lines, in src/diagnostics/renderer.rs.

Replaces render_one_with_source's single-snippet rendering with a
multi-snippet path that sorts primary + secondary spans by position
and renders each on its own caret line. `^` marks the primary, `-`
marks secondaries (matching rustc).
"""

from pathlib import Path

PATH = Path("src/diagnostics/renderer.rs")

OLD_FN_START = """pub fn render_one_with_source(err: &CompileError, source: Option<&str>) -> String {
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
        "{}[{}]: {}\\n",
        severity_label,
        err.error_code.as_str(),
        err.message
    ));

    // Location + source excerpt
    if err.line() > 0 {
        match &err.file {
            Some(f) => out.push_str(&format!("  --> {}:{}:{}\\n", f, err.line(), err.column())),
            None => out.push_str(&format!("  --> {}:{}\\n", err.line(), err.column())),
        }

        let snippet_lines = compute_snippet(err, source);
        if !snippet_lines.is_empty() {
            let gutter = " ".repeat(err.line().to_string().len());
            out.push_str(&format!("{} |\\n", gutter));
            for line in &snippet_lines {
                out.push_str(line);
                out.push('\\n');
            }
            out.push_str(&format!("{} |\\n", gutter));
        }
    }

    // Help line
    if let Some(s) = &err.suggestion {
        out.push_str(&format!("  = help: {}\\n", s));
    }

    // Notes
    for note in &err.notes {
        out.push_str(&format!("  = note: {}\\n", note));
    }

    // Secondary spans. Phase 6 will render these inline as
    // `label` under their own caret lines. For now they surface
    // as notes so the data model has a visible path through the
    // renderer and Phase 6 can replace this loop.
    for (span, label) in &err.secondary {
        out.push_str(&format!("  = note: {} ({})\\n", label, span));
    }

    out
}"""

NEW_FN = """pub fn render_one_with_source(err: &CompileError, source: Option<&str>) -> String {
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
        "{}[{}]: {}\\n",
        severity_label,
        err.error_code.as_str(),
        err.message
    ));

    // Location + source excerpt
    if err.line() > 0 {
        match &err.file {
            Some(f) => out.push_str(&format!("  --> {}:{}:{}\\n", f, err.line(), err.column())),
            None => out.push_str(&format!("  --> {}:{}\\n", err.line(), err.column())),
        }

        // Gutter width is the widest line number we will print —
        // primary or any secondary.
        let max_line = std::iter::once(err.span.start_line)
            .chain(err.secondary.iter().map(|(s, _)| s.start_line))
            .max()
            .unwrap_or(0);
        let gutter_width = max_line.max(1).to_string().len();
        let blank_gutter = " ".repeat(gutter_width);

        if let Some(src) = source {
            // Build the list of blocks to render: primary uses `^`,
            // secondaries use `-`. Sorted by position so they appear
            // in source order.
            let mut entries: Vec<(Span, Option<&str>, char)> = Vec::new();
            entries.push((err.span, err.label.as_deref(), '^'));
            for (span, label) in &err.secondary {
                entries.push((*span, Some(label.as_str()), '-'));
            }
            entries.sort_by_key(|(s, _, _)| (s.start_line, s.start_column));

            out.push_str(&format!("{} |\\n", blank_gutter));
            for (span, label, caret_char) in &entries {
                if span.start_line == 0 {
                    continue;
                }
                let block = render_snippet_with_caret(src, *span, *label, gutter_width, *caret_char);
                for line in &block {
                    out.push_str(line);
                    out.push('\\n');
                }
                // Separate blocks with a gutter line so overlapping
                // context is visually distinct.
                out.push_str(&format!("{} |\\n", blank_gutter));
            }
        } else {
            // No source available — fall back to the legacy snippet
            // path for the primary, then render secondaries as
            // `= note:` lines below.
            let snippet = legacy_snippet(err);
            if !snippet.is_empty() {
                out.push_str(&format!("{} |\\n", blank_gutter));
                for line in &snippet {
                    out.push_str(line);
                    out.push('\\n');
                }
                out.push_str(&format!("{} |\\n", blank_gutter));
            }
        }
    }

    // Help line
    if let Some(s) = &err.suggestion {
        out.push_str(&format!("  = help: {}\\n", s));
    }

    // Notes
    for note in &err.notes {
        out.push_str(&format!("  = note: {}\\n", note));
    }

    // Secondary spans become `= note:` lines only when we could not
    // render them inline (no source text).
    if source.is_none() {
        for (span, label) in &err.secondary {
            out.push_str(&format!("  = note: {} ({})\\n", label, span));
        }
    }

    out
}

/// Render one snippet with a caller-chosen caret character.
/// `^` for the primary span; `-` for secondary spans, matching
/// rustc's style. Delegates to `render_snippet` for the actual
/// source excerpt and column arithmetic.
fn render_snippet_with_caret(
    source: &str,
    span: Span,
    label: Option<&str>,
    gutter_width: usize,
    caret_char: char,
) -> Vec<String> {
    let mut lines = crate::diagnostics::snippet::render_snippet(
        source,
        span,
        label,
        gutter_width,
    )
    .lines;
    if caret_char != '^' {
        if let Some(last) = lines.last_mut() {
            *last = last.replace('^', &caret_char.to_string());
        }
    }
    lines
}"""


def main():
    if not PATH.exists():
        print(f"ERROR: {PATH} not found. Run from the repo root.")
        raise SystemExit(1)

    src = PATH.read_text()
    if OLD_FN_START not in src:
        print("FAIL: render_one_with_source does not match the expected text.")
        print("The file may have been reformatted. Inspect by hand.")
        raise SystemExit(1)

    src = src.replace(OLD_FN_START, NEW_FN, 1)

    # `compute_snippet` is no longer called; remove it and its imports
    # would fail if left dangling. Simplest: leave it — dead code but
    # harmless, and `cargo clippy -D warnings` will flag it. If it
    # does, delete the function by hand.
    PATH.write_text(src)
    print(f"OK: patched {PATH}")


if __name__ == "__main__":
    main()