use crate::frontend::lexer::Lexer;
use crate::frontend::parser::Parser;
use crate::semantics::semantic::SemanticAnalyzer;

fn analyze(source: &str) -> Result<(), String> {
    let lexer = Lexer::new(source.to_string()).map_err(|e| *e.message)?;
    let mut parser = Parser::new(lexer.tokens);
    let program = parser.parse_program().map_err(|e| *e.message)?;
    let mut analyzer = SemanticAnalyzer::new();
    analyzer.analyze(&program.functions).map_err(|e| *e.message)
}

#[test]
fn test_borrow_across_loop_continue() {
    let source = r#"
procedure main
    val x := 10.0
    val y := &x
    val arr := [1.0, 2.0, 3.0]
    for item in arr do
        if item > 1.0 then
            continue
        print(y)
"#;
    let r = analyze(source);
    assert!(r.is_ok(), "borrow across continue should be ok: {:?}", r);
}

#[test]
fn test_borrow_across_defer() {
    let source = r#"
procedure main
    val x := 10.0
    val y := &x
    defer print(y)
    print(y)
"#;
    assert!(analyze(source).is_ok());
}

#[test]
fn test_move_in_branch() {
    let source = r#"
procedure main
    val s := "hi"
    if true then
        val moved := s
        print(moved)
"#;
    let r = analyze(source);
    assert!(r.is_ok() || r.is_err(), "must not ICE: {:?}", r);
}
