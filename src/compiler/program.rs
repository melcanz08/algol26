// src/compiler/program.rs

//! The compilation unit that flows through the pipeline.
//!
//! Deliberately narrow. Config and capabilities live on
//! `CompilerContext`; this struct owns only data derived from the
//! source being compiled.

use crate::compiler::TypedProgram;
use crate::frontend::ast::{FunctionDecl, ImplBlock, TraitDecl};
use crate::ir::semantic_ir::SemanticProgram;
use std::rc::Rc;

/// Ast-level inputs consumed by `TypeCheckPass`.
///
/// The analyzer's output — type table, `TypeInfo` — lives on
/// `Program::typed`, not here. This struct is the frontend's output
/// before any semantic annotation.
pub struct AstPayload {
    pub functions: Rc<Vec<FunctionDecl>>,
    pub traits: Vec<TraitDecl>,
    pub impls: Vec<ImplBlock>,
}

pub struct Program {
    pub source: String,
    pub filename: String,

    /// Ast-level inputs, before type checking.
    pub ast: Option<AstPayload>,

    /// Typed AST, produced by `TypeCheckPass`.
    pub typed: Option<TypedProgram>,

    /// Semantic IR, produced by `BuildSemanticIRPass`. Cleared and
    /// replaced by each IR-producing phase, so the field always
    /// holds the current IR for this compilation.
    pub semantic_ir: Option<SemanticProgram>,

    /// True if the IR currently in `semantic_ir` has passed
    /// verification. Set by `run_verify_pass` and the trailing
    /// `VerifyIrPass` in `run_optimize_pass`; cleared by
    /// `run_build_ir_pass` (which produces fresh, unverified IR).
    pub verified: bool,
}

impl Program {
    pub fn new(source: impl Into<String>, filename: impl Into<String>) -> Self {
        Self {
            source: source.into(),
            filename: filename.into(),
            ast: None,
            typed: None,
            semantic_ir: None,
            verified: false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn program_starts_empty() {
        let p = Program::new("src", "test.gol");
        assert!(p.ast.is_none());
        assert!(p.typed.is_none());
        assert!(p.semantic_ir.is_none());
        assert!(!p.verified);
    }
}
