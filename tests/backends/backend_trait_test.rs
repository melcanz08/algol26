// ALGOL26 - Backend Trait Tests
// Verifies the Backend trait contract works correctly

use algol26::backends::backend::{Backend, BackendRegistry};
use algol26::backends::interpreter_backend::InterpreterBackend;
use algol26::backends::llvm_backend::LlvmBackend;
use algol26::frontend::lexer::Lexer;
use algol26::frontend::parser::Parser;
use algol26::ir::semantic_ir::SemanticProgram;
use algol26::semantics::builder::{BuildInputs, SemanticIRBuilder};

fn build_semantic_ir(
    source: &str,
) -> (
    SemanticProgram,
    Vec<String>,
    Vec<algol26::frontend::ast::FunctionDecl>,
) {
    let lexer = Lexer::new(source.to_string()).unwrap();
    let mut parser = Parser::new(lexer.tokens);
    let program = parser.parse_program().unwrap();
    let functions = program.functions;
    let (ir, diagnostics) = SemanticIRBuilder::build(BuildInputs {
        functions: &functions,
        type_table_id: std::collections::HashMap::new(),
        plan: algol26::ir::instantiation_plan::InstantiationPlan::default(),
        records: &[],
        nominal_types: std::collections::HashMap::new(),
        enum_types: std::collections::HashMap::new(),
        subrange_types: std::collections::HashMap::new(),
        union_types: std::collections::HashMap::new(),
        const_values: std::collections::HashMap::new(),
        virtual_calls: std::collections::HashMap::new(), // virtual_calls (ADR 0038)
        trait_ids: std::collections::HashMap::new(),     // trait_ids (ADR 0038)
        assoc_bindings: std::collections::HashMap::new(), // assoc_bindings (ADR 0041)
        function_returns: std::collections::HashMap::new(), // function_returns (ADR 0041)
    });
    (ir, diagnostics, functions)
}

#[test]
fn test_backend_registry() {
    let mut registry = BackendRegistry::new();

    let source = r#"
proc main
    print("Hello")
"#;
    let (_ir, _diagnostics, _functions) = build_semantic_ir(source);

    registry.register(Box::new(LlvmBackend::new()));
    registry.register(Box::new(InterpreterBackend::new()));

    let backends = registry.list();
    assert!(backends.contains(&"llvm"));
    assert!(backends.contains(&"interpreter"));
}

#[test]
fn test_llvm_backend_name() {
    let source = r#"
proc main
    print("Hello")
"#;
    let (_ir, _diagnostics, _functions) = build_semantic_ir(source);

    let backend = LlvmBackend::new();
    assert_eq!(backend.name(), "llvm");
    assert!(backend.can_execute());
}

#[test]
fn test_interpreter_backend_name() {
    let backend = InterpreterBackend::new();
    assert_eq!(backend.name(), "interpreter");
    assert!(backend.can_execute());
}

#[test]
fn test_backend_trait_contract() {
    let source = r#"
proc main
    print("Hello")
"#;
    let (_ir, diagnostics, _functions) = build_semantic_ir(source);
    assert!(diagnostics.is_empty());

    // Both backends should implement the trait
    let llvm: Box<dyn Backend> = Box::new(LlvmBackend::new());
    let interp: Box<dyn Backend> = Box::new(InterpreterBackend::new());

    assert_eq!(llvm.name(), "llvm");
    assert_eq!(interp.name(), "interpreter");

    // Both should have descriptions
    assert!(!llvm.description().is_empty());
    assert!(!interp.description().is_empty());
}
