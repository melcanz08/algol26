// ALGOL26 Optimization Safety Tests
// INVARIANT: execute(original_ir) == execute(optimized_ir)

use algol26::backends::interpreter::Interpreter;
use algol26::frontend::lexer::Lexer;
use algol26::frontend::parser::Parser;
use algol26::ir::optimizer::Optimizer;
use algol26::semantics::semantic::SemanticAnalyzer;
use algol26::semantics::semantic_builder::SemanticIRBuilder;

fn compile_to_ir(source: &str) -> algol26::ir::semantic_ir::SemanticProgram {
    let lexer = Lexer::new(source.to_string()).unwrap();
    let mut parser = Parser::new(lexer.tokens);
    let program = parser.parse_program().unwrap();
    let functions = program.functions;
    let traits = program.traits;
    let impls = program.impls;
    let mut analyzer = SemanticAnalyzer::new();
    analyzer
        .analyze_with_traits(
            &functions,
            &traits,
            &impls,
            &std::collections::HashMap::new(),
        )
        .unwrap();
    let (ir, _) = SemanticIRBuilder::build(&functions, std::collections::HashMap::new());
    ir
}

fn execute(ir: &algol26::ir::semantic_ir::SemanticProgram) -> String {
    let mut interpreter = Interpreter::new(ir.clone());
    interpreter.run().unwrap()
}

fn assert_optimization_preserves(source: &str) {
    let original_ir = compile_to_ir(source);
    let original_output = execute(&original_ir);
    let mut optimized_ir = original_ir.clone();
    let mut optimizer = Optimizer::new();
    optimizer.optimize(&mut optimized_ir);
    let optimized_output = execute(&optimized_ir);
    assert_eq!(
        original_output, optimized_output,
        "Optimization changed output!\nOriginal: {}\nOptimized: {}",
        original_output, optimized_output
    );
}

#[test]
fn test_optimization_preserves_arithmetic() {
    let source = r#"
function main() -> Int
    print 3.0
    print 15.0
    return 0
"#;
    assert_optimization_preserves(source);
}

#[test]
fn test_optimization_preserves_lists() {
    let source = r#"
function main() -> Int
    val arr := [1, 2, 3, 4, 5]
    print List.sum(arr)
    return 0
"#;
    assert_optimization_preserves(source);
}

#[test]
fn test_optimization_preserves_strings() {
    let source = r#"
function main() -> Int
    print "hello"
    return 0
"#;
    assert_optimization_preserves(source);
}

#[test]
fn test_dce_preserves_list_declare_used_by_indexing() {
    use algol26::frontend::lexer::Lexer;
    use algol26::frontend::parser::Parser;
    use algol26::ir::optimizer::Optimizer;
    use algol26::semantics::semantic::SemanticAnalyzer;
    use algol26::semantics::semantic_builder::SemanticIRBuilder;

    let source = "\
procedure main
    val arr := [10.0, 20.0, 30.0]
    var first := arr[0]
    print(first)
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

    let (mut ir, _) = SemanticIRBuilder::build(&program.functions, type_table);

    let mut opt = Optimizer::new();
    opt.optimize(&mut ir);

    // After DCE, the declare for `arr` must still exist.
    let arr_still_declared = ir.functions.iter().any(|f| {
        f.blocks.iter().any(|b| {
            b.instructions.iter().any(|i| {
                if let algol26::ir::semantic_ir::Instruction::Declare { name, .. } = i {
                    name == "arr"
                } else {
                    false
                }
            })
        })
    });
    assert!(arr_still_declared, "DCE removed 'arr' despite it being indexed");
}
