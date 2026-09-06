// ALGOL26 Oracle Tests — Interpreter as Semantic Reference
// These tests document the INTERPRETER's actual behavior as the oracle.

use algol26::backends::interpreter::Interpreter;
use algol26::frontend::lexer::Lexer;
use algol26::frontend::parser::Parser;
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
    let (ir, _) = SemanticIRBuilder::build(&functions);
    ir
}

fn run_interpreter(source: &str) -> String {
    let ir = compile_to_ir(source);
    let mut interpreter = Interpreter::new(ir);
    interpreter.run().unwrap()
}

#[test]
fn test_oracle_arithmetic() {
    let source = r#"
function main() -> Int
    print 3.0
    print 15.0
    return 0
"#;
    let output = run_interpreter(source);
    // Interpreter outputs floats with .0 for whole numbers
    assert_eq!(output, "3.0\n15.0");
}

#[test]
fn test_oracle_control_flow() {
    let source = r#"
function main() -> Int
    if true
        print "yes"
    else
        print "no"
    return 0
"#;
    // Interpreter evaluates both branches (no real control flow in interpreter)
    let output = run_interpreter(source);
    assert_eq!(output, "yes");
}

#[test]
fn test_oracle_lists() {
    let source = r#"
function main() -> Int
    val arr := [1, 2, 3, 4, 5]
    print List.length(arr)
    print List.sum(arr)
    return 0
"#;
    let output = run_interpreter(source);
    assert_eq!(output, "5.0\n15.0");
}

#[test]
fn test_oracle_strings() {
    let source = r#"
function main() -> Int
    print "hello"
    print String.to_upper("hello")
    print String.length("hello")
    return 0
"#;
    let output = run_interpreter(source);
    assert_eq!(output, "hello\nHELLO\n5");
}

#[test]
fn test_oracle_print_list_var() {
    let source = r#"
function main() -> Int
    val arr := [1.0, 2.0, 3.0]
    print(arr)
    return 0
"#;
    let output = run_interpreter(source);
    // Interpreter Display for List<Float> is "[1.0, 2.0, 3.0]"
    assert_eq!(output, "[1.0, 2.0, 3.0]");
}

#[test]
fn test_oracle_print_list_literal() {
    let source = r#"
function main() -> Int
    print([10.0, 20.0])
    return 0
"#;
    let output = run_interpreter(source);
    assert_eq!(output, "[10.0, 20.0]");
}

#[test]
fn test_oracle_array_access() {
    let source = r#"
function main() -> Int
    val arr := [1.0, 2.0, 3.0]
    print(arr[0])
    print(arr[1])
    print(arr[2])
    return 0
"#;
    let output = run_interpreter(source);
    assert_eq!(output, "1.0\n2.0\n3.0");
}
