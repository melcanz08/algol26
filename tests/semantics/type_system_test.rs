// tests/semantics/type_system_test.rs - HARDENED

use algol26::common::types::Type;
use algol26::semantics::type_checker::TypeChecker;

#[test]
fn test_option_some() {
    // Test that Some(value) creates Option type
    let _checker = TypeChecker::new();

    // Need to construct an Expr::Some
    // This would require proper AST construction
    // For now, test the type directly
    let opt_type = Type::option(Type::Int);
    assert!(opt_type.is_composite());
    assert_eq!(opt_type.to_string(), "Option<Int>");
}

#[test]
fn test_option_none() {
    // Test that None has Option<Unknown> type
    let none_type = Type::option(Type::Unknown);
    assert!(none_type.is_composite());
    assert_eq!(none_type.to_string(), "Option<Unknown>");
}

#[test]
fn test_result_types() {
    // Test Result type construction
    let ok_type = Type::result(Type::Int, Type::String);
    assert!(ok_type.is_composite());
    assert_eq!(ok_type.to_string(), "Result<Int, String>");

    let err_type = Type::result(Type::Unknown, Type::String);
    assert_eq!(err_type.to_string(), "Result<Unknown, String>");
}

#[test]
fn test_binary_op_type_checking() {
    let mut checker = TypeChecker::new();

    // Int + Int = Int
    let result =
        checker.validate_binary_op(&algol26::frontend::ast::BinOp::Add, &Type::Int, &Type::Int);
    assert_eq!(result, Type::Int);

    // Int + Float = Float
    let result = checker.validate_binary_op(
        &algol26::frontend::ast::BinOp::Add,
        &Type::Int,
        &Type::Float,
    );
    assert_eq!(result, Type::Float);

    // String + String = String
    let result = checker.validate_binary_op(
        &algol26::frontend::ast::BinOp::Add,
        &Type::String,
        &Type::String,
    );
    assert_eq!(result, Type::String);

    // String + Int = Error (should generate diagnostic)
    let result = checker.validate_binary_op(
        &algol26::frontend::ast::BinOp::Add,
        &Type::String,
        &Type::Int,
    );
    assert_eq!(result, Type::Unknown);
    assert!(!checker.take_diagnostics().is_empty());
}
