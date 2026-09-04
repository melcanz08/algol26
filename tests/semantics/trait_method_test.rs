use algol26::frontend::lexer::Lexer;
use algol26::frontend::parser::Parser;

use algol26::semantics::trait_registry::TraitRegistry;
use algol26::common::types::Type;

#[test]
fn test_trait_registration_and_lookup() {
    let source = r#"
trait Comparable
    function compare(other: Self) -> Int

impl Comparable for Int
    function compare(other: Int) -> Int
        if self < other
            return -1
        else if self > other
            return 1
        else
            return 0
"#;
    let lexer = Lexer::new(source.to_string()).unwrap();
    let mut parser = Parser::new(lexer.tokens);
    let (_functions, traits, impls) = parser.parse_program().unwrap();
    
    assert_eq!(traits.len(), 1);
    assert_eq!(traits[0].name, "Comparable");
    assert_eq!(impls.len(), 1);
    assert_eq!(impls[0].trait_name, "Comparable");
    assert_eq!(impls[0].target_type, "Int");
    assert_eq!(impls[0].methods.len(), 1);
    assert_eq!(impls[0].methods[0].name, "compare");
}

#[test]
fn test_trait_registry_resolution() {
    let mut registry = TraitRegistry::new();
    
    // Register a trait
    let trait_decl = algol26::frontend::ast::TraitDecl {
        name: "Comparable".to_string(),
        methods: vec![algol26::frontend::ast::TraitMethod {
            name: "compare".to_string(),
            params: vec![("other".to_string(), "Self".to_string())],
            return_type: Some("Int".to_string()),
        }],
    };
    registry.register_trait(trait_decl);
    
    // Register impl for Int
    let impl_block = algol26::frontend::ast::ImplBlock {
        trait_name: "Comparable".to_string(),
        target_type: "Int".to_string(),
        methods: vec![algol26::frontend::ast::FunctionDecl {
            name: "compare".to_string(),
            params: vec![("other".to_string(), "Int".to_string())],
            return_type: Some("Int".to_string()),
            body: vec![],
            is_extern: false,
            ffi_info: None,
            type_params: vec![],
            where_clauses: vec![],
        }],
    };
    registry.register_impl(impl_block);
    
    // Test resolution
    assert!(registry.type_implements_trait(&Type::Int, "Comparable"));
    assert!(!registry.type_implements_trait(&Type::Float, "Comparable"));
    
    let method = registry.resolve_method(&Type::Int, "compare");
    assert!(method.is_some());
    assert_eq!(method.unwrap().name, "compare");
}
