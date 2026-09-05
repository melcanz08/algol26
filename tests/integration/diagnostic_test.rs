use algol26::common::diagnostics::{CompileError, Diagnostic, ErrorCode};

#[test]
fn test_error_code_format() {
    assert_eq!(ErrorCode::E0001.as_str(), "E0001");
    assert_eq!(ErrorCode::E0002.as_str(), "E0002");
    assert_eq!(ErrorCode::E0007.as_str(), "E0007");
}

#[test]
fn test_compile_error_creation() {
    let err = CompileError::new(
        "Type mismatch",
        5,
        10,
        "val x: Int := 3.14",
        ErrorCode::E0002,
    )
   .with_suggestion("Convert the value to Int");

    assert_eq!(err.message.as_str(), "Type mismatch");
    assert_eq!(err.line, 5);
    assert_eq!(err.column, 10);
    assert_eq!(err.error_code, ErrorCode::E0002);
    assert_eq!(err.suggestion.as_deref().map(|s| s.as_str()), Some("Convert the value to Int"));
}

#[test]
fn test_diagnostic_display() {
    let diag = Diagnostic::Warning("This is a warning".to_string());
    // Just test it doesn't panic
    diag.display();
}