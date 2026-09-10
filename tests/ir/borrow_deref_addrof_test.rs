// tests/borrow_deref_addrof_test.rs - FIXED
use algol26::common::diagnostics::Result;
use algol26::compiler::Compiler;

fn compile_and_check(source: &str) -> Result<()> {
    let mut compiler = Compiler::new();
    compiler.compile(source, "test.gol", "test_output", true, false)
}

#[test]
fn test_borrow_expression() {
    let source = r#"
procedure main
    val x := 5.0
    val y := &x
    print(y)
"#;

    assert!(
        compile_and_check(source).is_ok(),
        "Borrow expression should compile"
    );
}

#[test]
fn test_deref_expression() {
    let source = r#"
procedure main
    val x := 5.0
    val y := &x
    val z := *y
    print(z)
"#;

    assert!(
        compile_and_check(source).is_ok(),
        "Deref expression should compile"
    );
}

#[test]
fn test_addrof_expression() {
    let source = r#"
procedure main
    val x := 5.0
    val y := &x
    print(y)
"#;

    assert!(
        compile_and_check(source).is_ok(),
        "AddrOf expression should compile"
    );
}

#[test]
fn test_double_borrow_fails() {
    use algol26::frontend::lexer::Lexer;
    use algol26::frontend::parser::Parser;
    use algol26::semantics::semantic::SemanticAnalyzer;

    // Plain string (not r#"..."#) to avoid top-level Indent tokens.
    let source = "procedure main\n    var x := 5.0\n    var y := &mut x\n    var z := &mut x\n";

    let lexer = Lexer::new(source.to_string()).expect("lexer failed");
    let mut parser = Parser::new(lexer.tokens);
    let program = parser.parse_program().expect("parser failed");

    let mut analyzer = SemanticAnalyzer::new();
    let result = analyzer.analyze_with_spans(
        &program.functions,
        &program.traits,
        &program.impls,
        &std::collections::HashMap::new(),
    );

    assert!(
        result.is_err(),
        "Double mutable borrow should fail, but analysis succeeded"
    );

    let err = result.unwrap_err();
    let msg = format!("{:?}", err);
    assert!(
        msg.contains("mutably borrow") || msg.contains("borrow"),
        "Expected a borrow error, got: {}",
        msg
    );
}

#[test]
fn test_borrow_immutable_fails() {
    let source = r#"
procedure main
    val x := 5.0
    var y := &mut x
"#;

    assert!(
        compile_and_check(source).is_err(),
        "Cannot mutably borrow immutable variable"
    );
}

#[test]
fn test_deref_non_pointer_fails() {
    let source = r#"
procedure main
    val x := 5.0
    val y := *x
"#;

    assert!(
        compile_and_check(source).is_err(),
        "Cannot dereference non-pointer"
    );
}

#[test]
fn test_method_call_desugars_to_function_call() {
    use algol26::frontend::lexer::Lexer;
    use algol26::frontend::parser::Parser;
    use algol26::ir::semantic_ir::TypedIRValue;
    use algol26::semantics::semantic::SemanticAnalyzer;
    use algol26::semantics::semantic_builder::SemanticIRBuilder;

    let source = "\
procedure main
    var list := [1.0, 2.0, 3.0]
    var n := list.length()
";
    let lexer = Lexer::new(source.to_string()).unwrap();
    let mut parser = Parser::new(lexer.tokens);
    let program = parser.parse_program().unwrap();

    let mut analyzer = SemanticAnalyzer::new();
    analyzer
        .analyze_with_spans(
            &program.functions,
            &program.traits,
            &program.impls,
            &std::collections::HashMap::new(),
        )
        .unwrap();
    let type_table = analyzer.take_type_table();

    let (ir, _) = SemanticIRBuilder::build(&program.functions, type_table);

    // Walk the IR and look for a Call to "List.length" with 1 argument.
    let mut found = false;
    for func in &ir.functions {
        for block in &func.blocks {
            for instr in &block.instructions {
                if let algol26::ir::semantic_ir::Instruction::Declare {
                    value: TypedIRValue::Call { function, args, .. },
                    ..
                } = instr
                {
                    if function == "List.length" && args.len() == 1 {
                        found = true;
                    }
                }
            }
        }
    }
    assert!(found, "Expected method call to desugar into Call(List.length)");
}

#[test]
fn test_if_expr_in_vardecl_keeps_following_statements() {
    use algol26::frontend::lexer::Lexer;
    use algol26::frontend::parser::Parser;
    use algol26::semantics::semantic::SemanticAnalyzer;
    use algol26::semantics::semantic_builder::SemanticIRBuilder;
    use algol26::ir::semantic_ir::Instruction;

    let source = "\
procedure main
    val x := if true
        42
    else
        0
    print(x)
";
    let lexer = Lexer::new(source.to_string()).unwrap();
    let mut parser = Parser::new(lexer.tokens);
    let program = parser.parse_program().unwrap();

    let mut analyzer = SemanticAnalyzer::new();
    analyzer
        .analyze_with_spans(
            &program.functions,
            &program.traits,
            &program.impls,
            &std::collections::HashMap::new(),
        )
        .unwrap();
    let type_table = analyzer.take_type_table();

    let (ir, _) = SemanticIRBuilder::build(&program.functions, type_table);

    // The print must be present somewhere in the IR, not dropped.
    let has_print = ir.functions.iter().any(|f| {
        f.blocks.iter().any(|b| {
            b.instructions
                .iter()
                .any(|i| matches!(i, Instruction::Print { .. }))
        })
    });
    assert!(has_print, "print statement after if-expr VarDecl was dropped");
}
