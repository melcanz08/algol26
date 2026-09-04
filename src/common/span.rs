// ALGOL26 Span — Unified source location representation
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
    
    /// Check if this span contains another span
    pub fn contains(&self, other: &Span) -> bool {
        self.start_line <= other.start_line
            && self.end_line >= other.end_line
            && self.start_column <= other.start_column
            && self.end_column >= other.end_column
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
            write!(f, "{}:{}-{}:{}", 
                self.start_line, self.start_column,
                self.end_line, self.end_column)
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
}
