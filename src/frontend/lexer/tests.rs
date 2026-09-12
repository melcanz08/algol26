// src/frontend/lexer/tests.rs

use super::*;

#[test]
fn test_simple_tokens() {
    let source = "var x := 5";
    let lexer = Lexer::new(source.to_string()).expect("ICE");
    assert_eq!(
        lexer.tokens,
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
    assert!(lexer.tokens.contains(&Token::Indent));
    assert!(lexer.tokens.contains(&Token::Dedent));
}

#[test]
fn test_comments_in_strings() {
    let source = "var s := \"hello // world\"";
    let lexer = Lexer::new(source.to_string()).expect("ICE");
    assert!(lexer.tokens.contains(&Token::StringLit("hello // world".to_string())));
}

#[test]
fn test_string_escapes() {
    let source = "var s := \"hello\\nworld\"";
    let lexer = Lexer::new(source.to_string()).expect("ICE");
    assert!(lexer.tokens.contains(&Token::StringLit("hello\nworld".to_string())));
}

#[test]
fn test_number_literals() {
    let source = "var a := 123\nvar b := 45.67\nvar c := 1e10\nvar d := 1_000_000";
    let lexer = Lexer::new(source.to_string()).expect("ICE");
    assert!(lexer.tokens.contains(&Token::IntLit(123)));
    assert!(lexer.tokens.contains(&Token::FloatLit(45.67)));
    assert!(lexer.tokens.contains(&Token::FloatLit(1e10)));
    assert!(lexer.tokens.contains(&Token::IntLit(1000000)));
}

#[test]
fn test_dotted_identifiers() {
    let source = "var x := Math.sqrt(16)";
    let lexer = Lexer::new(source.to_string()).expect("ICE");
    // Now should be Identifier("Math"), Dot, Identifier("sqrt"), LParen...
    assert!(lexer.tokens.contains(&Token::Identifier("Math".to_string())));
    assert!(lexer.tokens.contains(&Token::Dot));
    assert!(lexer.tokens.contains(&Token::Identifier("sqrt".to_string())));
}

#[test]
fn test_method_call_tokens() {
    let source = "list.append(3)";
    let lexer = Lexer::new(source.to_string()).expect("ICE");
    assert!(lexer.tokens.contains(&Token::Identifier("list".to_string())));
    assert!(lexer.tokens.contains(&Token::Dot));
    assert!(lexer.tokens.contains(&Token::Identifier("append".to_string())));
}

#[test]
fn test_range_tokens() {
    let source = "1..5";
    let lexer = Lexer::new(source.to_string()).expect("ICE");
    assert!(lexer.tokens.contains(&Token::IntLit(1)));
    assert!(lexer.tokens.contains(&Token::DotDot));
    assert!(lexer.tokens.contains(&Token::IntLit(5)));
}

#[test]
fn test_case_keyword() {
    let source = "match x\n    case 1\n        print 1";
    let lexer = Lexer::new(source.to_string()).expect("ICE");
    assert!(lexer.tokens.contains(&Token::Case));
}
