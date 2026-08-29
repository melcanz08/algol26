// ALGOL26 - Expression Translator Tests
// Verifies expression translation works independently

use algol26::expr_translator::ExprTranslator;
use algol26::ast::{Expr, BinOp};
use algol26::semantic_type::SemanticType;
use std::collections::HashMap;

#[test]
fn test_translate_simple_exprs() {
    let mut translator = ExprTranslator::new();
    let scopes: Vec<HashMap<String, algol26::expr_translator::VariableInfo>> = Vec::new();
    
    // Number
    let expr = Expr::Number(5.0);
    let result = translator.translate(&expr, &scopes);
    assert_eq!(result.type_of(), SemanticType::Float);
    
    // Int
    let expr = Expr::Int(5);
    let result = translator.translate(&expr, &scopes);
    assert_eq!(result.type_of(), SemanticType::Int);
    
    // String
    let expr = Expr::String("hello".to_string());
    let result = translator.translate(&expr, &scopes);
    assert_eq!(result.type_of(), SemanticType::String);
    
    // Bool
    let expr = Expr::Bool(true);
    let result = translator.translate(&expr, &scopes);
    assert_eq!(result.type_of(), SemanticType::Bool);
}

#[test]
fn test_translate_binary_op_with_coercion() {
    let mut translator = ExprTranslator::new();
    let scopes: Vec<HashMap<String, algol26::expr_translator::VariableInfo>> = Vec::new();
    
    // Int + Float should produce Float with coercion
    let expr = Expr::Binary {
        left: Box::new(Expr::Int(5)),
        op: BinOp::Add,
        right: Box::new(Expr::Number(3.0)),
    };
    
    let result = translator.translate(&expr, &scopes);
    
    match result {
        algol26::semantic_ir::TypedIRValue::BinaryOp { result_type, .. } => {
            assert_eq!(result_type, SemanticType::Float);
        }
        _ => panic!("Expected BinaryOp"),
    }
    
    let diagnostics = translator.take_diagnostics();
    assert!(diagnostics.is_empty(), "Expected no diagnostics, got: {:?}", diagnostics);
}

#[test]
fn test_translate_var_lookup() {
    let mut translator = ExprTranslator::new();
    
    let mut scope = HashMap::new();
    scope.insert(
        "x".to_string(),
        algol26::expr_translator::VariableInfo {
            type_: SemanticType::Float,
            mutable: false,
        },
    );
    let scopes = vec![scope];
    
    let expr = Expr::Var("x".to_string());
    let result = translator.translate(&expr, &scopes);
    
    match result {
        algol26::semantic_ir::TypedIRValue::Variable(name, type_) => {
            assert_eq!(name, "x");
            assert_eq!(type_, SemanticType::Float);
        }
        _ => panic!("Expected Variable"),
    }
}

#[test]
fn test_translate_function_call() {
    let mut translator = ExprTranslator::new();
    let scopes: Vec<HashMap<String, algol26::expr_translator::VariableInfo>> = Vec::new();
    
    let expr = Expr::FunctionCall {
        name: "Math.sqrt".to_string(),
        args: vec![Expr::Number(16.0)],
    };
    
    let result = translator.translate(&expr, &scopes);
    
    match result {
        algol26::semantic_ir::TypedIRValue::Call { function, .. } => {
            assert_eq!(function, "Math.sqrt");
        }
        _ => panic!("Expected Call"),
    }
}
