// src/frontend/lexer/tests.rs

use super::*;

// ─── PR-1a helpers ──────────────────────────────────────────────────────
// `Lexer.tokens` is now `Vec<SpannedToken>` (token + span). These helpers
// let the tests keep working on just the token kinds, which is all they
// ever cared about. Span assertions get added in PR-1b.

fn token_kinds(lexer: &Lexer) -> Vec<Token> {
    lexer.tokens.iter().map(|st| st.token.clone()).collect()
}

fn has_token(lexer: &Lexer, t: &Token) -> bool {
    lexer.tokens.iter().any(|st| &st.token == t)
}

// ─── Tests ──────────────────────────────────────────────────────────────

#[test]
fn test_simple_tokens() {
    let source = "var x := 5";
    let lexer = Lexer::new(source.to_string()).expect("ICE");
    assert_eq!(
        token_kinds(&lexer),
        vec![
            Token::Var,
            Token::Identifier("x".to_string()),
            Token::Assign,
            Token::IntLit(5),
            Token::Eof,
        ]
    );
}

#[test]
fn test_indentation() {
    let source = "procedure main\n    var x := 5\n    if x > 3\n        print x";
    let lexer = Lexer::new(source.to_string()).expect("ICE");
    assert!(has_token(&lexer, &Token::Indent));
    assert!(has_token(&lexer, &Token::Dedent));
}

#[test]
fn test_comments_in_strings() {
    let source = "var s := \"hello // world\"";
    let lexer = Lexer::new(source.to_string()).expect("ICE");
    assert!(has_token(&lexer, &Token::StringLit("hello // world".to_string())));
}

#[test]
fn test_string_escapes() {
    let source = "var s := \"hello\\nworld\"";
    let lexer = Lexer::new(source.to_string()).expect("ICE");
    assert!(has_token(&lexer, &Token::StringLit("hello\nworld".to_string())));
}

#[test]
fn test_number_literals() {
    let source = "var a := 123\nvar b := 45.67\nvar c := 1e10\nvar d := 1_000_000";
    let lexer = Lexer::new(source.to_string()).expect("ICE");
    assert!(has_token(&lexer, &Token::IntLit(123)));
    assert!(has_token(&lexer, &Token::FloatLit(45.67)));
    assert!(has_token(&lexer, &Token::FloatLit(1e10)));
    assert!(has_token(&lexer, &Token::IntLit(1000000)));
}

#[test]
fn test_dotted_identifiers() {
    let source = "var x := Math.sqrt(16)";
    let lexer = Lexer::new(source.to_string()).expect("ICE");
    // Now should be Identifier("Math"), Dot, Identifier("sqrt"), LParen...
    assert!(has_token(&lexer, &Token::Identifier("Math".to_string())));
    assert!(has_token(&lexer, &Token::Dot));
    assert!(has_token(&lexer, &Token::Identifier("sqrt".to_string())));
}

#[test]
fn test_method_call_tokens() {
    let source = "list.append(3)";
    let lexer = Lexer::new(source.to_string()).expect("ICE");
    assert!(has_token(&lexer, &Token::Identifier("list".to_string())));
    assert!(has_token(&lexer, &Token::Dot));
    assert!(has_token(&lexer, &Token::Identifier("append".to_string())));
}

#[test]
fn test_range_tokens() {
    let source = "1..5";
    let lexer = Lexer::new(source.to_string()).expect("ICE");
    assert!(has_token(&lexer, &Token::IntLit(1)));
    assert!(has_token(&lexer, &Token::DotDot));
    assert!(has_token(&lexer, &Token::IntLit(5)));
}

#[test]
fn test_case_keyword() {
    let source = "match x\n    case 1\n        print 1";
    let lexer = Lexer::new(source.to_string()).expect("ICE");
    assert!(has_token(&lexer, &Token::Case));
}

#[test]
fn test_escaped_quote_does_not_break_comment_stripping() {
    // ALGOL26 source: var s := "a\"b" // comment
    // The escaped quote must not terminate the string, so the trailing
    // `// comment` must still be recognized as a comment and stripped.
    let source = "var s := \"a\\\"b\" // comment";
    let lexer = Lexer::new(source.to_string()).expect("ICE");
    assert!(has_token(&lexer, &Token::StringLit("a\"b".to_string())));
    // The word "comment" must NOT have been tokenized.
    assert!(!has_token(&lexer, &Token::Identifier("comment".to_string())));
}

#[test]
fn test_escaped_backslash_before_quote() {
    // ALGOL26 source: var s := "\\" // done
    // `"\\"` is a string containing one backslash. The next quote closes
    // the string, and `//` starts a comment.
    let source = "var s := \"\\\\\" // done";
    let lexer = Lexer::new(source.to_string()).expect("ICE");
    assert!(has_token(&lexer, &Token::StringLit("\\".to_string())));
    assert!(!has_token(&lexer, &Token::Identifier("done".to_string())));
}

#[test]
fn test_non_ascii_identifier_position_tracking() {
    // `xé` has 2 chars but 3 bytes in UTF-8. The lexer must advance its
    // internal position counter by *chars*, not bytes, or every token
    // that follows a non-ASCII identifier gets a column that's too large.
    let source = "var xé := 5";
    let lexer = Lexer::new(source.to_string()).expect("ICE");

    let int_token = lexer
        .tokens
        .iter()
        .find(|st| matches!(st.token, Token::IntLit(5)))
        .expect("IntLit(5) not found in lexer output");

    // 1-based columns in `var xé := 5`:
    //   v=1  a=2  r=3  ' '=4  x=5  é=6  ' '=7  :=8-9  ' '=10  5=11
    assert_eq!(
        int_token.span.start_column, 11,
        "IntLit(5) should be at column 11, got {}",
        int_token.span.start_column
    );
}

#[test]
fn test_mut_is_a_keyword() {
    // `mut` must lex to Token::Mut, not Identifier("mut"). This is
    // required for `&mut x` and `&mut T` to parse via token match
    // instead of the fragile identifier-string compare.
    let source = "&mut x";
    let lexer = Lexer::new(source.to_string()).expect("ICE");
    assert!(has_token(&lexer, &Token::Ampersand));
    assert!(has_token(&lexer, &Token::Mut));
    assert!(!has_token(&lexer, &Token::Identifier("mut".to_string())));
}

#[test]
fn test_mut_in_signature_lexes_as_keyword() {
    // Regression for a coupled-tokenizer bug: `mut` in a function
    // *signature* must lex to Token::Mut (not Identifier("mut")),
    // otherwise `&mut T` in a type annotation fails to parse as
    // MutBorrow.
    let source = "function f(x: &mut Float) -> Float\n    return x";
    let lexer = Lexer::new(source.to_string()).expect("ICE");
    assert!(
        has_token(&lexer, &Token::Mut),
        "expected Token::Mut in signature tokens, got: {:?}",
        token_kinds(&lexer)
    );
    assert!(
        !has_token(&lexer, &Token::Identifier("mut".to_string())),
        "`mut` must not appear as Identifier in signature tokens"
    );
}