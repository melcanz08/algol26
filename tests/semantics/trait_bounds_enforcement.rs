// Trait Bounds Enforcement Tests
// Verify that where T: Comparable is enforced during monomorphization

use algol26::ir::monomorphize::Monomorphizer;
use algol26::frontend::ast::{FunctionDecl, WhereClause};

#[test]
fn test_comparable_bound_allows_int() {
    let func = FunctionDecl {
        name: "max".to_string(),
        params: vec![("a".to_string(), "T".to_string()), ("b".to_string(), "T".to_string())],
        return_type: Some("T".to_string()),
        body: vec![],
        is_extern: false,
        ffi_info: None,
        type_params: vec!["T".to_string()],
        where_clauses: vec![WhereClause {
            type_param: "T".to_string(),
            trait_name: "Comparable".to_string(),
        }],
    };
    
    let monomorphizer = Monomorphizer::new();
    let result = monomorphizer.check_trait_bounds_for_instantiation(
        &func,
        &[algol26::common::types::Type::Int],
    );
    assert!(result.is_ok(), "Int should implement Comparable");
}

#[test]
fn test_comparable_bound_rejects_string() {
    let func = FunctionDecl {
        name: "max".to_string(),
        params: vec![("a".to_string(), "T".to_string()), ("b".to_string(), "T".to_string())],
        return_type: Some("T".to_string()),
        body: vec![],
        is_extern: false,
        ffi_info: None,
        type_params: vec!["T".to_string()],
        where_clauses: vec![WhereClause {
            type_param: "T".to_string(),
            trait_name: "Comparable".to_string(),
        }],
    };
    
    let monomorphizer = Monomorphizer::new();
    let result = monomorphizer.check_trait_bounds_for_instantiation(
        &func,
        &[algol26::common::types::Type::String],
    );
    assert!(result.is_err(), "String should NOT implement Comparable");
}

#[test]
fn test_display_bound_allows_float() {
    let func = FunctionDecl {
        name: "show".to_string(),
        params: vec![("value".to_string(), "T".to_string())],
        return_type: Some("String".to_string()),
        body: vec![],
        is_extern: false,
        ffi_info: None,
        type_params: vec!["T".to_string()],
        where_clauses: vec![WhereClause {
            type_param: "T".to_string(),
            trait_name: "Display".to_string(),
        }],
    };
    
    let monomorphizer = Monomorphizer::new();
    let result = monomorphizer.check_trait_bounds_for_instantiation(
        &func,
        &[algol26::common::types::Type::Float],
    );
    assert!(result.is_ok(), "Float should implement Display");
}
