// tests/frontend/parser_diagnostics.rs
//
// Parser-diagnostic-quality tests: not "does the parser accept this
// program?" but "when the parser rejects this program, does the
// message tell the user what they did wrong?"
//
// Distinct from tests/chaincss/compiler_bugs/ (compiler-regression
// tests, one per G## entry in LANGUAGE_GAPS.md). Those pin behavior;
// these pin diagnostic quality. A parser change that turns the
// specific G30 message back into the generic "Expected '>', found
// RBracket" is a regression even though no valid program broke.
//
// See LANGUAGE_GAPS.md G30 for the mixed-bracket-family story.

use algol26::frontend::lexer::Lexer;
use algol26::frontend::parser::Parser;

/// Parse the source, assert it fails, and return the resulting
/// CompileError. Tests inspect `.message` and `.suggestion` directly
/// (matching how tests/integration/diagnostic_test.rs asserts on
/// CompileError fields). Using `format!("{}", err)` here would drop
/// the suggestion, because `CompileError::Display` currently renders
/// only the message (see LANGUAGE_GAPS.md G31 — deferred improvement).
fn parse_expect_err(source: &str) -> algol26::common::diagnostics::CompileError {
    let lexer = Lexer::new(source.to_string()).expect("lex");
    let mut parser = Parser::new(lexer.tokens);
    parser
        .parse_program()
        .expect_err("program should fail to parse")
}

#[test]
fn g30_mixed_bracket_family_typo_diagnostic() {
    // `Map<String, Json]` — opened with `<` (angle), closed with
    // `]` (square). The parser used to emit the generic
    //     "Expected '>', found RBracket"
    // which names the parser's expectation but not the user's
    // mistake. The G30 fix emits a targeted message and help text.
    let source = r#"
rec Json
    pub kind: String

rec Probe
    pub m: Option[Map<String, Json]]
"#;
    let err = parse_expect_err(source);

    // The specific message names the actual problem: a generic
    // opened with '<' but closed with ']'.
    assert_eq!(
        err.message, "Expected '>' to close a generic type, found ']'",
        "G30: expected targeted mixed-bracket diagnostic message"
    );

    // The suggestion explains the two bracket families and points
    // at the lint script.
    let suggestion = err.suggestion.as_deref().unwrap_or("");
    assert!(
        suggestion.contains("angle-bracketed"),
        "G30: suggestion should mention angle-bracketed generics, got: {:?}",
        suggestion
    );
    assert!(
        suggestion.contains("square-bracketed"),
        "G30: suggestion should mention square-bracketed forms, got: {:?}",
        suggestion
    );
    assert!(
        suggestion.contains("lint_generic_brackets"),
        "G30: suggestion should point at the lint script, got: {:?}",
        suggestion
    );

    // The old generic message must not appear. If the parser
    // regresses, the "found RBracket" fragment reappears.
    assert!(
        !err.message.contains("RBracket"),
        "G30 regression: generic token-name message leaked through. Got: {:?}",
        err.message
    );
}

#[test]
fn g30_correct_angle_close_still_accepted() {
    // Sanity: the fix doesn't break correct angle-bracket syntax.
    // Same shape as the failing case, with `>` instead of `]`.
    let source = r#"
rec Json
    pub kind: String

rec Probe
    pub m: Option[Map<String, Json>]
"#;
    let lexer = Lexer::new(source.to_string()).expect("lex");
    let mut parser = Parser::new(lexer.tokens);
    parser
        .parse_program()
        .expect("correct angle-bracket close should parse");
}

#[test]
fn g30_correct_square_close_still_accepted() {
    // Sanity: the fix doesn't break correct square-bracket syntax.
    // `Option[List[Json]]` uses `]` legitimately at every level.
    let source = r#"
rec Json
    pub kind: String

rec Probe
    pub m: Option[List[Json]]
"#;
    let lexer = Lexer::new(source.to_string()).expect("lex");
    let mut parser = Parser::new(lexer.tokens);
    parser
        .parse_program()
        .expect("correct square-bracket close should parse");
}
