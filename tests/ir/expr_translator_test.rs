// ALGOL26 - Expression Translator Tests
// Verifies expression translation works independently

use algol26::common::span::Span;
use algol26::common::types::Type;
use algol26::frontend::ast::{BinOp, Expr};
use algol26::semantics::expr_translator::ExprTranslator;
use std::collections::HashMap;

#[test]
fn test_translate_simple_exprs() {
    let mut translator = ExprTranslator::new();
    let scopes: Vec<HashMap<String, algol26::semantics::semantic_builder::VariableInfo>> =
        Vec::new();

    // Number
    let expr = Expr::Number(5.0);
    let result = translator.translate(&expr, &scopes);
    assert_eq!(result.type_of(), Type::Float);

    // Int
    let expr = Expr::Int(5);
    let result = translator.translate(&expr, &scopes);
    assert_eq!(result.type_of(), Type::Int);

    // String
    let expr = Expr::String("hello".to_string());
    let result = translator.translate(&expr, &scopes);
    assert_eq!(result.type_of(), Type::String);

    // Bool
    let expr = Expr::Bool(true);
    let result = translator.translate(&expr, &scopes);
    assert_eq!(result.type_of(), Type::Bool);
}

#[test]
fn test_translate_binary_op_with_coercion() {
    let mut translator = ExprTranslator::new();
    let scopes: Vec<HashMap<String, algol26::semantics::semantic_builder::VariableInfo>> =
        Vec::new();

    // Int + Float should produce Float with coercion
    let expr = Expr::Binary {
        left: Box::new(Expr::Int(5)),
        op: BinOp::Add,
        right: Box::new(Expr::Number(3.0)),
    };

    let result = translator.translate(&expr, &scopes);

    match result {
        algol26::ir::semantic_ir::TypedIRValue::BinaryOp { result_type, .. } => {
            assert_eq!(result_type, Type::Float);
        }
        _ => panic!("Expected BinaryOp"),
    }

    let diagnostics = translator.take_diagnostics();
    assert!(
        diagnostics.is_empty(),
        "Expected no diagnostics, got: {:?}",
        diagnostics
    );
}

#[test]
fn test_translate_var_lookup() {
    let mut translator = ExprTranslator::new();

    let mut scope = HashMap::new();
    scope.insert(
        "x".to_string(),
        algol26::semantics::semantic_builder::VariableInfo {
            capture_mode: Some(algol26::semantics::flow_result::CaptureMode::Read),
            type_: Type::Float,
            mutable: false,
        },
    );
    let scopes = vec![scope];

    let expr = Expr::Var("x".to_string(), Span::default());
    let result = translator.translate(&expr, &scopes);

    match result {
        algol26::ir::semantic_ir::TypedIRValue::Variable(name, type_) => {
            assert_eq!(name, "x");
            assert_eq!(type_, Type::Float);
        }
        _ => panic!("Expected Variable"),
    }
}

#[test]
fn test_translate_function_call() {
    let mut translator = ExprTranslator::new();
    let scopes: Vec<HashMap<String, algol26::semantics::semantic_builder::VariableInfo>> =
        Vec::new();

    let expr = Expr::FunctionCall {
        span: Span::default(),
        name: "Math.sqrt".to_string(),
        args: vec![Expr::Number(16.0)],
    };

    let result = translator.translate(&expr, &scopes);

    match result {
        algol26::ir::semantic_ir::TypedIRValue::Call { function, .. } => {
            assert_eq!(function, "Math.sqrt");
        }
        _ => panic!("Expected Call"),
    }
}
