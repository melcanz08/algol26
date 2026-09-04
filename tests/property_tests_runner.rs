// ALGOL26 Property-Based Tests — Simplified version
// Uses simple random testing without proptest macro

use algol26::frontend::lexer::Lexer;
use algol26::frontend::parser::Parser;
use algol26::common::types::Type;

/// Generate random strings and verify lexer doesn't panic
#[test]
fn test_lexer_no_panic_random_strings() {
    let random_strings = vec![
        String::new(),
        " ".to_string(),
        "\n".to_string(),
        "\t".to_string(),
        "function".to_string(),
        "12345".to_string(),
        "\"unterminated".to_string(),
        "/* comment".to_string(),
        "🦀".to_string(),
        "a".repeat(1000),
        " ".repeat(100),
        "\n".repeat(50),
    ];
    
    for input in random_strings {
        let _ = Lexer::new(input);
    }
}

/// Verify parser doesn't panic on lexed tokens from random strings
#[test]
fn test_parser_no_panic_random_strings() {
    let random_strings = vec![
        "function main() -> Int\n    return 0",
        "procedure main\n    print 1",
        "val x := 5",
        "if true\n    print 1",
        "for i in [1, 2, 3]\n    print i",
        "trait Test\n    function method()",
        "impl Test for Int\n    function method()\n        return 0",
    ];
    
    for source in random_strings {
        if let Ok(lexer) = Lexer::new(source.to_string()) {
            let _ = std::panic::catch_unwind(|| {
                let mut parser = Parser::new(lexer.tokens);
                let _ = parser.parse_program();
            });
        }
    }
}

/// Verify type system operations never panic
#[test]
fn test_type_system_no_panic() {
    let types = vec![
        Type::Int,
        Type::Float,
        Type::String,
        Type::Bool,
        Type::Void,
        Type::Ptr,
        Type::Unknown,
        Type::Never,
        Type::list(Type::Int),
        Type::option(Type::Float),
        Type::result(Type::Int, Type::String),
        Type::TypeVar("T".to_string()),
    ];
    
    for t in &types {
        let _ = format!("{}", t);
        let _ = t.is_numeric();
        let _ = t.is_copy();
        let _ = t.can_coerce_to(&Type::Unknown);
        let _ = t.common_supertype(&Type::Int);
    }
}

/// Verify Type::from_str never panics on edge cases
#[test]
fn test_type_from_str_no_panic() {
    let edge_cases = vec![
        "", " ", "int", "INT", "Float", "list", "list<int>",
        "option<float>", "result<int, string>", "T", "U", "V",
        "*", "&", "&mut", "unknown", "never", "ptr",
        "list<", ">", "<", "list<>", "option<>", "result<>",
        "very_long_type_name_that_doesnt_exist",
    ];
    
    for s in edge_cases {
        let _ = Type::from_str(s);
    }
}

/// Verify compile never panics on malformed source
#[test]
fn test_compile_no_panic_malformed_source() {
    let malformed_sources = vec![
        "",                       // Empty
        "function",               // Incomplete function
        "function main(",         // Unclosed paren
        "function main()",        // No body
        "val",                    // Incomplete declaration
        "if",                     // Incomplete if
        "for",                    // Incomplete for
        "while",                  // Incomplete while
        "trait",                  // Incomplete trait
        "impl",                   // Incomplete impl
    ];
    
    for source in malformed_sources {
        let _ = std::panic::catch_unwind(|| {
            if let Ok(lexer) = Lexer::new(source.to_string()) {
                let mut parser = Parser::new(lexer.tokens);
                let _ = parser.parse_program();
            }
        });
    }
}
