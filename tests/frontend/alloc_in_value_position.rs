// tests/frontend/alloc_in_value_position.rs
//
// Regression for a parser bug fixed in PR-4: `alloc(n)` previously
// double-consumed the `alloc` keyword in expression position, so
// `val p := alloc(8)` failed to parse. The fix made it work.

use algol26::frontend::lexer::Lexer;
use algol26::frontend::parser::Parser;

#[test]
fn alloc_parses_as_expression_value() {
    let source = "\
procedure main
    val p := alloc(8)
    free(p)
";
    let lexer = Lexer::new(source.to_string()).expect("lex");
    let mut parser = Parser::new(lexer.tokens);
    let program = parser.parse_program().expect(
        "`val p := alloc(8)` should parse — this was the PR-4 fix",
    );
    assert_eq!(program.functions.len(), 1);
}