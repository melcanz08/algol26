// src/common/diagnostics.rs

use crate::common::span::Span;
use std::fmt;

#[derive(Debug, Clone)]
pub struct CompileError {
    pub message: String,
    /// Source location. `Span::default()` (all zeros) means "no
    /// location known" — the renderer treats it as such.
    pub span: Span,
    pub source_line: String,
    pub error_code: ErrorCode,
    pub suggestion: Option<String>,
    /// Source file where the error was detected. `None` for errors
    /// that have no file context (backend capability refusals,
    /// internal invariant failures). The top-level CLI attaches
    /// this via `with_file` from the filename it was given.
    ///
    /// Limitation: for errors inside imported files, this is the
    /// *importing* file, not the file where the error text lives.
    /// Per-node file provenance does not exist yet.
    pub file: Option<String>,

    // Phase 1 additions. All default to empty/None so existing
    // constructor call sites compile unchanged. Consumed by the
    // rustc-style renderer (Phase 3).
    pub severity: Severity,
    pub label: Option<String>,
    pub secondary: Vec<(Span, String)>,
    pub notes: Vec<String>,
}

/// Diagnostic severity. `Error` is fatal (stops the pipeline);
/// `Warning` and `Note` are informational.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Severity {
    #[default]
    Error,
    Warning,
    Note,
}

/// The bucket code the diagnostic renderer prefixes on every error.
///
/// **Two-tier diagnostic system.** ALGOL26 uses two distinct code
/// vocabularies that serve different purposes:
///
/// - **`ErrorCode::E0001..E0009`** (this enum) — the coarse bucket
///   the renderer prints at the start of every message. Stable,
///   enumerated, and machine-readable. Tools that filter or count
///   by category look at these.
/// - **`E-XXX-NNN` sub-codes** — embedded in the *message text* by
///   specific subsystems (`E-MOVE-001`, `E-REGION-001`,
///   `E-ESCAPE-002`, etc.). They carry finer-grained meaning for
///   humans but are not part of the `CompileError` struct.
///
/// A single diagnostic can carry both: `E0002` from the renderer
/// plus `E-UNSUPPORTED-001` inside the message. This is deliberate.
/// Unifying them is a future refactor (see
/// `docs/architecture-direction.md`, Tier 2.5).
///
/// **Current `E0001..E0009` bucket meanings** (approximate; the
/// message carries the detail):
///
/// | Code | Broad category |
/// |---|---|
/// | E0001 | I/O / lexing / file access |
/// | E0002 | IR construction or verification |
/// | E0003 | Undefined identifier |
/// | E0004 | Unsupported operation in a backend |
/// | E0005 | Reserved |
/// | E0006 | Reserved |
/// | E0007 | Concurrency / race / borrow |
/// | E0008 | Reserved |
/// | E0009 | Internal / codegen invariant |
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErrorCode {
    E0001,
    E0002,
    E0003,
    E0004,
    E0005,
    E0006,
    E0007,
    E0008,
    E0009,
    /// Ambiguous method: two or more traits provide a method with
    /// the same name for the same target type, and no inherent impl
    /// shadows them.
    E0010,
    /// Unknown method: the receiver's type has no method with that
    /// name (and no field with that name, for the field-access form).
    E0011,
    /// ADR 0038: a `&dyn Trait` coercion failed — the value's
    /// concrete type does not implement the trait, or the value
    /// is not a borrow at all. Distinct from `E0002` so a caller
    /// can point at the missing impl specifically.
    E0012,
}

#[derive(Debug, Clone)]
pub enum Diagnostic {
    Error(CompileError),
    Warning(String),
}

impl Diagnostic {
    pub fn display(&self) {
        match self {
            Diagnostic::Error(e) => e.display(),
            Diagnostic::Warning(w) => eprintln!("warning: {}", w),
        }
    }
}

impl ErrorCode {
    pub fn as_str(&self) -> &'static str {
        match self {
            ErrorCode::E0001 => "E0001",
            ErrorCode::E0002 => "E0002",
            ErrorCode::E0003 => "E0003",
            ErrorCode::E0004 => "E0004",
            ErrorCode::E0005 => "E0005",
            ErrorCode::E0006 => "E0006",
            ErrorCode::E0007 => "E0007",
            ErrorCode::E0008 => "E0008",
            ErrorCode::E0009 => "E0009",
            ErrorCode::E0010 => "E0010",
            ErrorCode::E0011 => "E0011",
            ErrorCode::E0012 => "E0012",
        }
    }
}

impl CompileError {
    /// Construct with a single-point location. Kept for the ~150
    /// existing call sites that pass `(line, column)` separately.
    pub fn simple(
        message: &str,
        line: usize,
        column: usize,
        source_line: &str,
        error_code: ErrorCode,
    ) -> Self {
        CompileError {
            message: message.to_string(),
            span: Span::point(line, column),
            source_line: source_line.to_string(),
            error_code,
            suggestion: None,
            file: None,
            severity: Severity::Error,
            label: None,
            secondary: Vec::new(),
            notes: Vec::new(),
        }
    }

    /// Construct with a full span. Preferred for new code — preserves
    /// start and end so the renderer can underline multi-column ranges.
    pub fn at(span: Span, message: &str, error_code: ErrorCode) -> Self {
        CompileError {
            message: message.to_string(),
            span,
            source_line: String::new(),
            error_code,
            suggestion: None,
            file: None,
            severity: Severity::Error,
            label: None,
            secondary: Vec::new(),
            notes: Vec::new(),
        }
    }

    /// Legacy alias for `simple` — kept so existing call sites don't
    /// need to change in this PR.
    pub fn new(
        message: &str,
        line: usize,
        column: usize,
        source_line: &str,
        error_code: ErrorCode,
    ) -> Self {
        CompileError::simple(message, line, column, source_line, error_code)
    }

    /// Construct a non-fatal warning at the given span.
    pub fn warning(message: &str, span: Span, error_code: ErrorCode) -> Self {
        let mut e = Self::at(span, message, error_code);
        e.severity = Severity::Warning;
        e
    }

    /// Construct a compile error for a backend that cannot lower
    /// a specific operation.
    ///
    /// The message is self-describing — it names both the operation
    /// and the backend — so a test can assert on the text without
    /// pattern-matching a structured error type. This is the
    /// standard constructor for fail-closed backend paths: any
    /// operation the backend does not implement becomes a call to
    /// this function, never a silent fallback.
    ///
    /// Uses `E0004`, the code the LLVM backend already uses for
    /// "unhandled builtin" and "requires a specific argument shape."
    /// The code is a coarse bucket; the message carries the detail.
    pub fn unsupported_operation(operation: &str, backend: &str) -> Self {
        Self::simple(
            &format!(
                "backend `{}` does not support operation `{}`",
                backend, operation
            ),
            0,
            0,
            "",
            ErrorCode::E0004,
        )
    }

    pub fn with_span(mut self, span: Span) -> Self {
        self.span = span;
        self
    }

    /// Attach the source filename. Idempotent — a later call does
    /// not overwrite a filename that is already set, so an error
    /// that named its own file is not clobbered by the CLI.
    pub fn with_file(mut self, file: impl Into<String>) -> Self {
        if self.file.is_none() {
            self.file = Some(file.into());
        }
        self
    }

    pub fn with_suggestion(mut self, suggestion: &str) -> Self {
        self.suggestion = Some(suggestion.to_string());
        self
    }

    pub fn with_context(mut self, context: &str) -> Self {
        // Add context as suggestion if no suggestion exists
        if self.suggestion.is_none() {
            self.suggestion = Some(context.to_string());
        }
        self
    }

    /// Text printed under the primary caret (e.g. "expected Int,
    /// found Float"). Distinct from `suggestion`, which becomes an
    /// `= help:` line.
    pub fn with_label(mut self, label: impl Into<String>) -> Self {
        self.label = Some(label.into());
        self
    }

    /// Attach a secondary span with its own label (e.g. "borrowed
    /// here", "moved here"). Multiple secondary spans accumulate in
    /// the order they were added.
    pub fn with_secondary(mut self, span: Span, label: impl Into<String>) -> Self {
        self.secondary.push((span, label.into()));
        self
    }

    /// Attach an unattributed note, rendered as `= note: ...`.
    pub fn with_note(mut self, note: impl Into<String>) -> Self {
        self.notes.push(note.into());
        self
    }

    pub fn suggest_fix(&self) -> Option<&str> {
        self.suggestion.as_deref()
    }

    pub fn display(&self) {
        eprint!("{}", crate::diagnostics::renderer::render_one(self));
    }

    /// Start line of the error location. `0` means "unknown".
    pub fn line(&self) -> usize {
        self.span.start_line
    }

    /// Start column of the error location. `0` means "unknown".
    pub fn column(&self) -> usize {
        self.span.start_column
    }
}

impl fmt::Display for CompileError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.message)
    }
}

impl std::error::Error for CompileError {}

pub type Result<T> = std::result::Result<T, CompileError>;

impl From<String> for CompileError {
    fn from(msg: String) -> Self {
        CompileError::simple(&msg, 0, 0, "", ErrorCode::E0001)
    }
}

impl From<&str> for CompileError {
    fn from(msg: &str) -> Self {
        CompileError::simple(msg, 0, 0, "", ErrorCode::E0001)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn enriched_compile_error_round_trips_new_fields() {
        let e = CompileError::at(Span::point(3, 5), "boom", ErrorCode::E0002)
            .with_label("expected Int")
            .with_secondary(Span::new(2, 1, 2, 10), "borrowed here")
            .with_note("see ADR 0005");

        assert_eq!(e.severity, Severity::Error);
        assert_eq!(e.label.as_deref(), Some("expected Int"));
        assert_eq!(e.secondary.len(), 1);
        assert_eq!(e.secondary[0].1, "borrowed here");
        assert_eq!(e.notes, vec!["see ADR 0005"]);
    }

    #[test]
    fn simple_constructor_defaults_new_fields() {
        let e = CompileError::simple("oops", 1, 1, "", ErrorCode::E0001);
        assert_eq!(e.severity, Severity::Error);
        assert!(e.label.is_none());
        assert!(e.secondary.is_empty());
        assert!(e.notes.is_empty());
    }

    #[test]
    fn warning_constructor_sets_severity() {
        let e = CompileError::warning("heads up", Span::point(2, 3), ErrorCode::E0002);
        assert_eq!(e.severity, Severity::Warning);
        assert_eq!(e.message, "heads up");
    }

    #[test]
    fn secondary_spans_accumulate_in_order() {
        let e = CompileError::at(Span::point(5, 5), "use after move", ErrorCode::E0007)
            .with_secondary(Span::point(3, 5), "moved here")
            .with_secondary(Span::point(4, 5), "and here");
        assert_eq!(e.secondary.len(), 2);
        assert_eq!(e.secondary[0].1, "moved here");
        assert_eq!(e.secondary[1].1, "and here");
    }
}
