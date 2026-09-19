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
