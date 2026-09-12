// src/semantics/trait_registry/tests.rs

use super::*;
use crate::frontend::ast::TypeSyntax;

#[test]
fn test_register_trait() {
    let mut registry = TraitRegistry::new();
    let trait_decl = TraitDecl {
        name: "Comparable".to_string(),
        methods: vec![TraitMethod {
            name: "compare".to_string(),
            params: vec![(
                "other".to_string(),
                Some(TypeSyntax::Named("Self".to_string())),
            )],
            return_type: Some(TypeSyntax::Named("Int".to_string())),
        }],
    };
    registry.register_trait(trait_decl);
    assert!(registry.trait_exists("Comparable"));
}

#[test]
fn test_type_implements_trait() {
    let mut registry = TraitRegistry::new();

    let trait_decl = TraitDecl {
        name: "Comparable".to_string(),
        methods: vec![TraitMethod {
            name: "compare".to_string(),
            params: vec![],
            return_type: Some(TypeSyntax::Named("Int".to_string())),
        }],
    };
    registry.register_trait(trait_decl);

    let impl_block = ImplBlock {
        trait_name: "Comparable".to_string(),
        target_type: "Int".to_string(),
        methods: vec![FunctionDecl {
            name: "compare".to_string(),
            params: vec![],
            return_type: Some(TypeSyntax::Named("Int".to_string())),
            body: vec![],
            is_extern: false,
            ffi_info: None,
            type_params: vec![],
            where_clauses: vec![],
        }],
    };
    registry.register_impl(impl_block);

    assert!(registry.type_implements_trait(&Type::Int, "Comparable"));
    assert!(!registry.type_implements_trait(&Type::Float, "Comparable"));
}

#[test]
fn test_validate_impl_signature_mismatch() {
    let mut registry = TraitRegistry::new();

    let trait_decl = TraitDecl {
        name: "Comparable".to_string(),
        methods: vec![TraitMethod {
            name: "compare".to_string(),
            params: vec![(
                "other".to_string(),
                Some(TypeSyntax::Named("Self".to_string())),
            )],
            return_type: Some(TypeSyntax::Named("Int".to_string())),
        }],
    };
    registry.register_trait(trait_decl);

    let impl_block = ImplBlock {
        trait_name: "Comparable".to_string(),
        target_type: "Int".to_string(),
        methods: vec![FunctionDecl {
            name: "compare".to_string(),
            params: vec![], // Wrong! Missing "other" parameter
            return_type: Some(TypeSyntax::Named("String".to_string())), // Wrong! Should be Int
            body: vec![],
            is_extern: false,
            ffi_info: None,
            type_params: vec![],
            where_clauses: vec![],
        }],
    };

    assert!(registry.validate_impl(&impl_block).is_err());
}

#[test]
fn test_generic_impl() {
    let mut registry = TraitRegistry::new();

    let trait_decl = TraitDecl {
        name: "Display".to_string(),
        methods: vec![TraitMethod {
            name: "display".to_string(),
            params: vec![],
            return_type: Some(TypeSyntax::Named("String".to_string())),
        }],
    };
    registry.register_trait(trait_decl);

    // Generic impl for List<T>
    let impl_block = ImplBlock {
        trait_name: "Display".to_string(),
        target_type: "List<T>".to_string(),
        methods: vec![FunctionDecl {
            name: "display".to_string(),
            params: vec![],
            return_type: Some(TypeSyntax::Named("String".to_string())),
            body: vec![],
            is_extern: false,
            ffi_info: None,
            type_params: vec!["T".to_string()],
            where_clauses: vec![],
        }],
    };
    registry.register_impl(impl_block);

    // List<Int> should implement Display
    assert!(registry.type_implements_trait(&Type::list(Type::Int), "Display"));

    // Int should NOT implement Display
    assert!(!registry.type_implements_trait(&Type::Int, "Display"));
}