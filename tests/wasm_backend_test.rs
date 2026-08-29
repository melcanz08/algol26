// ALGOL26 - WASM Backend Tests

use algol26::backend::{Backend, BackendRegistry};
use algol26::wasm_backend::WasmBackend;
use algol26::llvm_backend::LlvmBackend;
use algol26::interpreter_backend::InterpreterBackend;
use algol26::semantic_ir::SemanticProgram;
use algol26::semantic_builder::SemanticIRBuilder;
use algol26::parser::Parser;
use algol26::lexer::Lexer;
use algol26::ast::FunctionDecl;

fn build_ir(source: &str) -> (SemanticProgram, Vec<String>, Vec<FunctionDecl>) {
    let lexer = Lexer::new(source.to_string()).unwrap();
    let mut parser = Parser::new(lexer.tokens);
    let functions = parser.parse_program().unwrap();
    let (ir, diagnostics) = SemanticIRBuilder::build(&functions);
    (ir, diagnostics, functions)
}

#[test]
fn test_wasm_backend_name() {
    let source = "procedure main\n    Terminal.print(\"Hello\")\n";
    let (_ir, _diag, functions) = build_ir(source);
    
    let backend = WasmBackend::new(functions);
    assert_eq!(backend.name(), "wasm");
    assert!(!backend.can_execute());
}

#[test]
fn test_wasm_backend_description() {
    let source = "procedure main\n    Terminal.print(\"Hello\")\n";
    let (_ir, _diag, functions) = build_ir(source);
    
    let backend = WasmBackend::new(functions);
    assert!(!backend.description().is_empty());
}

#[test]
fn test_backend_registry_includes_wasm() {
    let source = "procedure main\n    Terminal.print(\"Hello\")\n";
    let (_ir, _diag, functions) = build_ir(source);
    
    let mut registry = BackendRegistry::new();
    registry.register(Box::new(LlvmBackend::new(functions.clone())));
    registry.register(Box::new(InterpreterBackend::new()));
    registry.register(Box::new(WasmBackend::new(functions)));
    
    let backends = registry.list();
    assert!(backends.contains(&"llvm"));
    assert!(backends.contains(&"interpreter"));
    assert!(backends.contains(&"wasm"));
}

#[test]
fn test_wasm_backend_trait_contract() {
    let source = "procedure main\n    Terminal.print(\"Hello\")\n";
    let (_ir, _diag, functions) = build_ir(source);
    
    let wasm: Box<dyn Backend> = Box::new(WasmBackend::new(functions));
    assert_eq!(wasm.name(), "wasm");
    assert!(!wasm.description().is_empty());
    assert!(!wasm.can_execute());
}
