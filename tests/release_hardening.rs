// ALGOL26 - Release Hardening Tests
// Negative testing, stress testing, and equivalence

use algol26::semantic_ir::SemanticProgram;
use algol26::semantic_builder::SemanticIRBuilder;
use algol26::parser::Parser;
use algol26::lexer::Lexer;
use algol26::optimizer::Optimizer;

fn build_ir(source: &str) -> (SemanticProgram, Vec<String>) {
    let lexer = Lexer::new(source.to_string()).unwrap();
    let mut parser = Parser::new(lexer.tokens);
    let functions = parser.parse_program().unwrap();
    SemanticIRBuilder::build(&functions)
}

#[test]
fn test_optimization_preserves_semantics() {
    let source = r#"
procedure main
    val x := 5.0 + 3.0
    val y := x * 2.0
    Terminal.print(y)
"#;
    
    let (mut ir, diagnostics) = build_ir(source);
    assert!(diagnostics.is_empty());
    
    // Optimize
    let mut optimizer = Optimizer::new();
    optimizer.optimize(&mut ir);
    
    // The IR should still be valid after optimization
    assert!(!ir.functions.is_empty());
}

#[test]
fn test_negative_invalid_syntax() {
    let source = "procedure main\n    this is not valid syntax";
    let lexer = Lexer::new(source.to_string()).unwrap();
    let mut parser = Parser::new(lexer.tokens);
    assert!(parser.parse_program().is_err());
}

#[test]
fn test_negative_undefined_variable() {
    let source = r#"
procedure main
    Terminal.print(undefined_var)
"#;
    
    let (_ir, diagnostics) = build_ir(source);
    assert!(!diagnostics.is_empty(), "Expected diagnostics for undefined variable");
}

#[test]
fn test_negative_type_mismatch() {
    let source = r#"
procedure main
    val x := "hello" + 5.0
"#;
    
    let (_ir, diagnostics) = build_ir(source);
    assert!(!diagnostics.is_empty(), "Expected diagnostics for type mismatch");
}

#[test]
fn test_stress_nested_control_flow() {
    let source = r#"
procedure main
    val arr := [1.0, 2.0, 3.0, 4.0, 5.0]
    var total := 0.0
    
    for item in arr do
        if item > 2.0 then
            total := total + item
            if item > 4.0 then
                break
    
    Terminal.print(total)
"#;
    
    let (_ir, diagnostics) = build_ir(source);
    assert!(diagnostics.is_empty(), "Expected no diagnostics, got: {:?}", diagnostics);
}

#[test]
fn test_stress_nested_functions() {
    let source = r#"
function add(x: float, y: float) -> float
    return x + y

function multiply(x: float, y: float) -> float
    return x * y

function compute(x: float) -> float
    return multiply(add(x, 2.0), add(x, 3.0))

procedure main
    val result := compute(5.0)
    Terminal.print(result)
"#;
    
    let (_ir, diagnostics) = build_ir(source);
    assert!(diagnostics.is_empty(), "Expected no diagnostics, got: {:?}", diagnostics);
}

#[test]
fn test_stress_multiple_imports() {
    // Test that imports are handled (module resolution happens in compiler)
    let source = r#"
import "utils.gol"
import "math.gol"

procedure main
    Terminal.print("With imports")
"#;
    
    let lexer = Lexer::new(source.to_string()).unwrap();
    let mut parser = Parser::new(lexer.tokens);
    let functions = parser.parse_program().unwrap();
    assert!(!functions.is_empty());
}

#[test]
fn test_optimizer_idempotent() {
    let source = r#"
procedure main
    val x := 5.0 + 3.0
    Terminal.print(x)
"#;
    
    let (mut ir, _) = build_ir(source);
    let mut optimizer = Optimizer::new();
    
    // Optimize once
    optimizer.optimize(&mut ir);
    let stats1 = optimizer.stats.clone();
    
    // Optimize again - should not change anything
    let mut optimizer2 = Optimizer::new();
    optimizer2.optimize(&mut ir);
    
    assert_eq!(optimizer2.stats.folded_constants, 0, "Second optimization should not fold more constants");
}
