// ALGOL26 - Backend Trait Tests
// Verifies the Backend trait contract works correctly

use algol26::backends::backend::{Backend, BackendRegistry};
use algol26::ir::semantic_ir::SemanticProgram;
use algol26::semantics::semantic_builder::SemanticIRBuilder;
use algol26::frontend::parser::Parser;
use algol26::frontend::lexer::Lexer;
use algol26::backends::llvm_backend::LlvmBackend;
use algol26::backends::interpreter_backend::InterpreterBackend;

fn build_semantic_ir(source: &str) -> (SemanticProgram, Vec<String>, Vec<algol26::frontend::ast::FunctionDecl>) {
    let lexer = Lexer::new(source.to_string()).unwrap();
    let mut parser = Parser::new(lexer.tokens);
    let (functions, _, _) = parser.parse_program().unwrap();
    let (ir, diagnostics) = SemanticIRBuilder::build(&functions);
    (ir, diagnostics, functions)
}

#[test]
fn test_backend_registry() {
    let mut registry = BackendRegistry::new();
    
    let source = r#"
procedure main
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
procedure main
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
procedure main
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
