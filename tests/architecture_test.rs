// ALGOL26 - Architecture Tests
// Verifies that refactored modules work independently

use algol26::type_checker::TypeChecker;
use algol26::flow_analyzer::FlowAnalyzer;
use algol26::semantic_type::SemanticType;
use algol26::ast::BinOp;

#[test]
fn test_type_checker_binary_ops() {
    let mut checker = TypeChecker::new();
    
    // Int + Int = Int
    let result = checker.validate_binary_op(
        &BinOp::Add,
        &SemanticType::Int,
        &SemanticType::Int,
    );
    assert_eq!(result, SemanticType::Int);
    
    // Int + Float = Float (promotion)
    let result = checker.validate_binary_op(
        &BinOp::Add,
        &SemanticType::Int,
        &SemanticType::Float,
    );
    assert_eq!(result, SemanticType::Float);
    
    // Float + Int = Float (promotion)
    let result = checker.validate_binary_op(
        &BinOp::Add,
        &SemanticType::Float,
        &SemanticType::Int,
    );
    assert_eq!(result, SemanticType::Float);
    
    // String + String = String (would be handled elsewhere)
    let result = checker.validate_binary_op(
        &BinOp::Add,
        &SemanticType::String,
        &SemanticType::String,
    );
    assert_eq!(result, SemanticType::Unknown); // TypeChecker doesn't handle strings
}

#[test]
fn test_type_checker_comparison() {
    let mut checker = TypeChecker::new();
    
    // Int > Float should be Bool
    let result = checker.validate_binary_op(
        &BinOp::Greater,
        &SemanticType::Int,
        &SemanticType::Float,
    );
    assert_eq!(result, SemanticType::Bool);
}

#[test]
fn test_flow_analyzer_termination() {
    use algol26::semantic_ir::{SemanticBlock, SemanticInstruction};
    
    // Empty block - not terminated
    let block = SemanticBlock { id: 0, instructions: Vec::new() };
    assert!(!FlowAnalyzer::is_terminated(&block));
    
    // Block with Return - terminated
    let block = SemanticBlock {
        id: 0,
        instructions: vec![SemanticInstruction::Return {
            value: None,
            type_: SemanticType::Void,
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
    let result = TypeChecker::needs_int_to_float_coercion(
        &SemanticType::Int,
        &SemanticType::Float,
    );
    assert_eq!(result, Some(true));
    
    let result = TypeChecker::needs_int_to_float_coercion(
        &SemanticType::Float,
        &SemanticType::Int,
    );
    assert_eq!(result, Some(false));
    
    let result = TypeChecker::needs_int_to_float_coercion(
        &SemanticType::Float,
        &SemanticType::Float,
    );
    assert_eq!(result, None);
}
