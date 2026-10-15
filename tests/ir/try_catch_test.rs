// tests/ir/try_catch_test.rs
use algol26::backends::interpreter::Interpreter;
use algol26::compiler::assign_expr_ids;
use algol26::frontend::lexer::Lexer;
use algol26::frontend::parser::Parser;
use algol26::semantics::analyzer::SemanticAnalyzer;
use algol26::semantics::builder::{BuildInputs, SemanticIRBuilder};

fn run_source(source: &str) -> String {
    let lexer = Lexer::new(source.to_string()).unwrap();
    let mut parser = Parser::new(lexer.tokens);
    let program = parser.parse_program().unwrap();

    let mut functions = program.functions;
    assign_expr_ids(&mut functions);

    let mut analyzer = SemanticAnalyzer::new();
    analyzer
        .analyze_with_spans(
            &functions,
            &program.traits,
            &program.impls,
            &program.records,
            &[],
            &[],
            &[],
        )
        .unwrap();
    let type_table = analyzer.take_type_table_id();

    let (ir, _) = SemanticIRBuilder::build(BuildInputs {
        functions: &functions,
        type_table_id: type_table,
        plan: algol26::ir::instantiation_plan::InstantiationPlan::default(),
        records: &[],
        nominal_types: std::collections::HashMap::new(),
        enum_types: std::collections::HashMap::new(),
        subrange_types: std::collections::HashMap::new(),
        const_values: std::collections::HashMap::new(),
        virtual_calls: std::collections::HashMap::new(), // virtual_calls (ADR 0038)
        trait_ids: std::collections::HashMap::new(),     // trait_ids (ADR 0038)
        assoc_bindings: std::collections::HashMap::new(), // assoc_bindings (ADR 0041)
        function_returns: std::collections::HashMap::new(), // function_returns (ADR 0041)
    });
    let mut interpreter = Interpreter::new(ir);
    interpreter.run().unwrap()
}

#[test]
fn test_try_catch_ok_path() {
    let source = "\
proc main
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
proc main
    val x := try
        Error(\"oops\")
    catch e
        99
    print(x)
";
    assert_eq!(run_source(source).trim(), "99");
}
