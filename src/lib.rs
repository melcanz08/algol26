// ALGOL26 - Library entry point
// Exposes compiler modules for testing and external use

pub mod lexer;
pub mod parser;
pub mod ast;
pub mod codegen;
pub mod diagnostics;
pub mod compiler;
pub mod semantic;
pub mod escape;
pub mod region;
pub mod region_memory;
pub mod stdlib;
pub mod string_module;
pub mod file_module;
pub mod ir;
pub mod ir_translator;
pub mod ir_codegen;
pub mod semantic_type;
pub mod semantic_ir;
pub mod semantic_builder;
pub mod defer_lowering;
pub mod cfg_verifier;
pub mod flow_result;
pub mod type_checker;
pub mod flow_analyzer;
pub mod expr_translator;
pub mod control_flow;
pub mod race;
pub mod interpreter;
pub mod backend;
pub mod module_loader;
pub mod llvm_backend;
pub mod interpreter_backend;
pub mod wasm_backend;
pub mod optimizer;
