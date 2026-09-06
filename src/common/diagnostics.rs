// algol26/src/common/diagnostics.rs
use crate::common::span::Span;
use std::fmt;

#[derive(Debug, Clone)]
pub struct CompileError {
    pub message: Box<String>,
    pub span: Option<Span>,
    pub line: usize,
    pub column: usize,
    pub source_line: Box<String>,
    pub error_code: ErrorCode,
    pub suggestion: Option<Box<String>>,
}

#[allow(dead_code)]
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
    pub fn suggest_fix(&self) -> Option<&str> {
        self.suggestion.as_deref().map(|s| s.as_str())
    }
    pub fn with_context(mut self, context: &str) -> Self {
        let new_sugg = format!(
            "{}. {}",
            context,
            self.suggestion
                .as_deref()
                .map(|s| s.as_str())
                .unwrap_or_default()
        );
        self.suggestion = Some(Box::new(new_sugg));
        self
    }
    pub fn new(
        message: &str,
        line: usize,
        column: usize,
        source_line: &str,
        error_code: ErrorCode,
    ) -> Self {
        CompileError {
            message: Box::new(message.to_string()),
            span: None,
            line,
            column,
            source_line: Box::new(source_line.to_string()),
            error_code,
            suggestion: None,
        }
    }
    pub fn with_span(mut self, span: Span) -> Self {
        self.span = Some(span);
        self
    }
    pub fn with_suggestion(mut self, suggestion: &str) -> Self {
        self.suggestion = Some(Box::new(suggestion.to_string()));
        self
    }
    pub fn display(&self) {
        if self.line > 0 {
            eprintln!(
                "error[{}]: {} (line {}, column {})",
                self.error_code.as_str(),
                self.message,
                self.line,
                self.column
            );
        } else {
            eprintln!("error[{}]: {}", self.error_code.as_str(), self.message);
        }
        if self.line > 0 {
            eprintln!("  --> Line {}:{}", self.line, self.column);
            eprintln!("  |");
            eprintln!("{} | {}", self.line, self.source_line);
            eprintln!("  | {}^", " ".repeat(self.column));
        }
        if let Some(suggestion) = &self.suggestion {
            eprintln!("  |");
            eprintln!("  = help: {}", suggestion);
        }
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
        CompileError::new(&msg, 0, 0, "", ErrorCode::E0001)
    }
}
impl From<&str> for CompileError {
    fn from(msg: &str) -> Self {
        CompileError::new(msg, 0, 0, "", ErrorCode::E0001)
    }
}
