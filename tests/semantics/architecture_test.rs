// ALGOL26 - Architecture Tests (FIXED)
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

    // Int + Float = Float
    let result = checker.validate_binary_op(&BinOp::Add, &Type::Int, &Type::Float);
    assert_eq!(result, Type::Float);

    // Float + Int = Float
    let result = checker.validate_binary_op(&BinOp::Add, &Type::Float, &Type::Int);
    assert_eq!(result, Type::Float);

    // String + String = String (FIXED: Should work!)
    let result = checker.validate_binary_op(&BinOp::Add, &Type::String, &Type::String);
    assert_eq!(result, Type::String);

    // String + Int should fail
    let result = checker.validate_binary_op(&BinOp::Add, &Type::String, &Type::Int);
    assert_eq!(result, Type::Unknown);
    assert!(
        !checker.take_diagnostics().is_empty(),
        "Should have diagnostic for String + Int"
    );
}

#[test]
fn test_type_checker_comparison() {
    let mut checker = TypeChecker::new();

    // Numeric comparisons
    let result = checker.validate_binary_op(&BinOp::Greater, &Type::Int, &Type::Float);
    assert_eq!(result, Type::Bool);

    let result = checker.validate_binary_op(&BinOp::Less, &Type::Float, &Type::Float);
    assert_eq!(result, Type::Bool);

    // String comparisons should work
    let result = checker.validate_binary_op(&BinOp::Less, &Type::String, &Type::String);
    assert_eq!(result, Type::Bool);
}

#[test]
fn test_type_checker_logical_ops() {
    let mut checker = TypeChecker::new();

    // Bool && Bool = Bool
    let result = checker.validate_binary_op(&BinOp::And, &Type::Bool, &Type::Bool);
    assert_eq!(result, Type::Bool);

    // Bool || Bool = Bool
    let result = checker.validate_binary_op(&BinOp::Or, &Type::Bool, &Type::Bool);
    assert_eq!(result, Type::Bool);

    // Int && Bool should fail
    let result = checker.validate_binary_op(&BinOp::And, &Type::Int, &Type::Bool);
    assert_eq!(result, Type::Bool); // Returns Bool but should have diagnostic
    assert!(
        !checker.take_diagnostics().is_empty(),
        "Should have diagnostic for Int && Bool"
    );
}

#[test]
fn test_flow_analyzer_termination() {
    use algol26::ir::semantic_ir::{SemanticBlock, Terminator};

    // Block without terminator is not terminated
    let block = SemanticBlock {
        id: 0,
        instructions: Vec::new(),
        terminator: None,
    };
    assert!(!FlowAnalyzer::is_terminated(&block));

    // Block with Return is terminated
    let block = SemanticBlock {
        id: 0,
        instructions: vec![],
        terminator: Some(Terminator::Return {
            value: None,
            type_: Type::Void,
        }),
    };
    assert!(FlowAnalyzer::is_terminated(&block));

    // Block with Jump is terminated
    let block = SemanticBlock {
        id: 0,
        instructions: vec![],
        terminator: Some(Terminator::Jump { block: 1 }),
    };
    assert!(FlowAnalyzer::is_terminated(&block));

    // Block with Branch is terminated
    let block = SemanticBlock {
        id: 0,
        instructions: vec![],
        terminator: Some(Terminator::Branch {
            condition: algol26::ir::semantic_ir::TypedIRValue::Bool(true),
            then_block: 1,
            else_block: 2,
        }),
    };
    assert!(FlowAnalyzer::is_terminated(&block));
}

#[test]
fn test_type_checker_coercion_detection() {
    // Int to Float coercion needed
    let result = TypeChecker::needs_int_to_float_coercion(&Type::Int, &Type::Float);
    assert_eq!(result, Some(true));

    // Float to Int coercion needed
    let result = TypeChecker::needs_int_to_float_coercion(&Type::Float, &Type::Int);
    assert_eq!(result, Some(false));

    // Same types, no coercion
    let result = TypeChecker::needs_int_to_float_coercion(&Type::Float, &Type::Float);
    assert_eq!(result, None);

    // Same types, no coercion
    let result = TypeChecker::needs_int_to_float_coercion(&Type::Int, &Type::Int);
    assert_eq!(result, None);
}

#[test]
fn test_type_inference() {
    let mut checker = TypeChecker::new();

    // Test list type inference
    use algol26::frontend::ast::Expr;
    let elements = vec![Expr::Number(1.0), Expr::Number(2.0), Expr::Number(3.0)];
    let list_type = checker.infer_list_element_type(&elements);
    assert_eq!(list_type, Type::Float);

    // Mixed Int and Float should infer Float
    let elements = vec![Expr::Int(1), Expr::Number(2.0), Expr::Int(3)];
    let list_type = checker.infer_list_element_type(&elements);
    assert_eq!(list_type, Type::Float);
}

#[test]
fn test_equality_type_checking() {
    let mut checker = TypeChecker::new();

    // Int == Int should work
    let result = checker.validate_binary_op(&BinOp::Equal, &Type::Int, &Type::Int);
    assert_eq!(result, Type::Bool);

    // Int == Float should work (with coercion)
    let result = checker.validate_binary_op(&BinOp::Equal, &Type::Int, &Type::Float);
    assert_eq!(result, Type::Bool);

    // String == Int should fail
    let result = checker.validate_binary_op(&BinOp::Equal, &Type::String, &Type::Int);
    assert_eq!(result, Type::Bool); // Returns Bool but should have diagnostic
    assert!(
        !checker.take_diagnostics().is_empty(),
        "Should have diagnostic for String == Int"
    );
}
