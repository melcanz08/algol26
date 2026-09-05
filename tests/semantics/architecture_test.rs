// ALGOL26 - Architecture Tests
// Verifies that refactored modules work independently

use algol26::common::types::Type;
use algol26::frontend::ast::BinOp;
use algol26::semantics::flow_analyzer::FlowAnalyzer;
use algol26::semantics::type_checker::TypeChecker;

#[test]
fn test_type_checker_binary_ops() {
    let mut checker = TypeChecker::new();

    // Int + Int = Int
    let result = checker.validate_binary_op(&BinOp::Add, &Type::Int, &Type::Int);
    assert_eq!(result, Type::Int);

    // Int + Float = Float (promotion)
    let result = checker.validate_binary_op(&BinOp::Add, &Type::Int, &Type::Float);
    assert_eq!(result, Type::Float);

    // Float + Int = Float (promotion)
    let result = checker.validate_binary_op(&BinOp::Add, &Type::Float, &Type::Int);
    assert_eq!(result, Type::Float);

    // String + String = String (would be handled elsewhere)
    let result = checker.validate_binary_op(&BinOp::Add, &Type::String, &Type::String);
    assert_eq!(result, Type::Unknown); // TypeChecker doesn't handle strings
}

#[test]
fn test_type_checker_comparison() {
    let mut checker = TypeChecker::new();

    // Int > Float should be Bool
    let result = checker.validate_binary_op(&BinOp::Greater, &Type::Int, &Type::Float);
    assert_eq!(result, Type::Bool);
}

#[test]
fn test_flow_analyzer_termination() {
    use algol26::ir::semantic_ir::{SemanticBlock, SemanticInstruction};

    // Empty block - not terminated
    let block = SemanticBlock {
        id: 0,
        instructions: Vec::new(),
    };
    assert!(!FlowAnalyzer::is_terminated(&block));

    // Block with Return - terminated
    let block = SemanticBlock {
        id: 0,
        instructions: vec![SemanticInstruction::Return {
            value: None,
            type_: Type::Void,
        }],
    };
    assert!(FlowAnalyzer::is_terminated(&block));

    // Block with Jump - terminated
    let block = SemanticBlock {
        id: 0,
        instructions: vec![SemanticInstruction::Jump { block: 1 }],
    };
    assert!(FlowAnalyzer::is_terminated(&block));
}

#[test]
fn test_type_checker_coercion_detection() {
    let result = TypeChecker::needs_int_to_float_coercion(&Type::Int, &Type::Float);
    assert_eq!(result, Some(true));

    let result = TypeChecker::needs_int_to_float_coercion(&Type::Float, &Type::Int);
    assert_eq!(result, Some(false));

    let result = TypeChecker::needs_int_to_float_coercion(&Type::Float, &Type::Float);
    assert_eq!(result, None);
}
