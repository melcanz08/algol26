// src/common/span.rs - ALGOL26 Span — Unified source location representation
// Replaces ad-hoc (line, column) tuples throughout the compiler

/// A range in source code
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct Span {
    /// Starting line (1-based)
    pub start_line: usize,
    /// Starting column (1-based)
    pub start_column: usize,
    /// Ending line (1-based)
    pub end_line: usize,
    /// Ending column (1-based)
    pub end_column: usize,
}

impl Span {
    /// Create a new span
    pub fn new(start_line: usize, start_column: usize, end_line: usize, end_column: usize) -> Self {
        Span {
            start_line,
            start_column,
            end_line,
            end_column,
        }
    }

    /// Create a point span (single location)
    pub fn point(line: usize, column: usize) -> Self {
        Span {
            start_line: line,
            start_column: column,
            end_line: line,
            end_column: column,
        }
    }

    /// Create a span from a line and column with default end
    pub fn from_line_column(line: usize, column: usize) -> Self {
        Span::point(line, column)
    }

    /// Check if this span contains another span.
    ///
    /// Columns are compared only when the corresponding start/end lines are
    /// equal — a column on line 3 has no meaningful comparison to a column
    /// on line 1.
    pub fn contains(&self, other: &Span) -> bool {
        let start_ok = self.start_line < other.start_line
            || (self.start_line == other.start_line && self.start_column <= other.start_column);

        let end_ok = self.end_line > other.end_line
            || (self.end_line == other.end_line && self.end_column >= other.end_column);

        start_ok && end_ok
    }

    /// Get the line (for backward compatibility)
    pub fn line(&self) -> usize {
        self.start_line
    }

    /// Get the column (for backward compatibility)
    pub fn column(&self) -> usize {
        self.start_column
    }
}

impl std::fmt::Display for Span {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if self.start_line == self.end_line {
            write!(f, "{}:{}", self.start_line, self.start_column)
        } else {
            write!(
                f,
                "{}:{}-{}:{}",
                self.start_line, self.start_column, self.end_line, self.end_column
            )
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_span_creation() {
        let span = Span::new(1, 5, 1, 10);
        assert_eq!(span.start_line, 1);
        assert_eq!(span.start_column, 5);
        assert_eq!(span.end_line, 1);
        assert_eq!(span.end_column, 10);
    }

    #[test]
    fn test_span_point() {
        let span = Span::point(3, 7);
        assert_eq!(span.line(), 3);
        assert_eq!(span.column(), 7);
    }

    #[test]
    fn test_span_display() {
        let span = Span::point(2, 4);
        assert_eq!(span.to_string(), "2:4");

        let range = Span::new(1, 1, 3, 5);
        assert_eq!(range.to_string(), "1:1-3:5");
    }

    #[test]
    fn test_span_contains() {
        let outer = Span::new(1, 1, 10, 10);
        let inner = Span::new(3, 3, 5, 5);
        assert!(outer.contains(&inner));
        assert!(!inner.contains(&outer));
    }
    #[test]
    fn test_span_contains_multiline_inner() {
        // Regression for the case where the inner span lives on a line
        // strictly between the outer's start and end lines. The old
        // implementation compared columns across different lines and
        // wrongly returned false.
        let outer = Span::new(1, 5, 3, 10);
        let inner = Span::new(2, 20, 2, 25);
        assert!(
            outer.contains(&inner),
            "outer spans lines 1-3, inner is entirely on line 2"
        );
    }

    #[test]
    fn test_span_contains_same_line_uses_columns() {
        // Same line on both endpoints: columns matter.
        let outer = Span::new(1, 5, 1, 20);
        let inner = Span::new(1, 10, 1, 15);
        assert!(outer.contains(&inner));

        // Inner extends past outer on the same line — must be false.
        let too_wide = Span::new(1, 10, 1, 25);
        assert!(!outer.contains(&too_wide));
    }

    #[test]
    fn test_span_contains_straddling_lines() {
        // Outer spans lines 2-4. Inner starts on line 1 — must be false.
        let outer = Span::new(2, 5, 4, 10);
        let earlier = Span::new(1, 5, 2, 5);
        assert!(!outer.contains(&earlier));

        // Inner ends on line 5 — must be false.
        let later = Span::new(3, 5, 5, 5);
        assert!(!outer.contains(&later));

        // Inner starts and ends on the outer's boundary lines but is
        // fully inside the columns at those lines — true.
        let at_boundary = Span::new(2, 5, 4, 10);
        assert!(outer.contains(&at_boundary));
    }
}
