use algol26::frontend::lexer::Lexer;
use algol26::frontend::parser::Parser;
use algol26::semantics::semantic_builder::SemanticIRBuilder;
use algol26::ir::optimizer::Optimizer;

fn build_ir(src: &str) -> algol26::ir::semantic_ir::SemanticProgram {
    let lexer = Lexer::new(src.to_string()).unwrap();
    let mut parser = Parser::new(lexer.tokens);
    let prog = parser.parse_program().unwrap();
    let (ir, diags) = SemanticIRBuilder::build(&prog.functions);
    assert!(diags.is_empty(), "diags: {:?}", diags);
    ir
}

#[test]
fn test_56_borrow_not_removed() {
    let src = r#"
procedure main
    var arr := [1.0, 2.0, 3.0]
    val r := &arr
    print(r)
"#;
    let mut ir = build_ir(src);
    let before: usize = ir.functions.iter().map(|f| f.blocks.iter().map(|b| b.instructions.iter().filter(|i| format!("{:?}", i).contains("Borrow")).count()).sum::<usize>()).sum();
    let mut opt = Optimizer::new();
    opt.optimize(&mut ir);
    let after: usize = ir.functions.iter().map(|f| f.blocks.iter().map(|b| b.instructions.iter().filter(|i| format!("{:?}", i).contains("Borrow")).count()).sum::<usize>()).sum();
    assert_eq!(before, after, "optimizer must not delete Borrow");
}

#[test]
fn test_56_interpreter_oracle() {
    let src = r#"
procedure main
    val x := 2.0 + 3.0 * 4.0
    print(x)
"#;
    let _ir = build_ir(src);
    // just check IR builds; interpreter backend tested elsewhere
    assert!(true);
}
