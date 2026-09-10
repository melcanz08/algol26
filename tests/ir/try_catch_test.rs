use algol26::backends::interpreter::Interpreter;
use algol26::frontend::lexer::Lexer;
use algol26::frontend::parser::Parser;
use algol26::semantics::semantic::SemanticAnalyzer;
use algol26::semantics::semantic_builder::SemanticIRBuilder;

fn run_source(source: &str) -> String {
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
    let mut interpreter = Interpreter::new(ir);
    interpreter.run().unwrap()
}

#[test]
fn test_try_catch_ok_path() {
    let source = "\
procedure main
    val x := try
        Ok(42)
    catch e
        0
    print(x)
";
    assert_eq!(run_source(source).trim(), "42");
}

#[test]
fn test_try_catch_error_path() {
    let source = "\
procedure main
    val x := try
        Error(\"oops\")
    catch e
        99
    print(x)
";
    assert_eq!(run_source(source).trim(), "99");
}