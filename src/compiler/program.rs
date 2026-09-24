// src/compiler/program.rs

//! The compilation unit that flows through the pipeline.
//!
//! Deliberately narrow. Config and capabilities live on
//! `CompilerContext`; this struct owns only data derived from the
//! source being compiled.

use crate::compiler::TypedProgram;
use crate::frontend::ast::{FunctionDecl, ImplBlock, TraitDecl};
use crate::ir::semantic_ir::SemanticProgram;
use crate::ir::verified_ir::VerifiedIR;
use std::rc::Rc;

/// Ast-level inputs consumed by `TypeCheckPass`.
pub struct AstPayload {
    pub functions: Rc<Vec<FunctionDecl>>,
    pub traits: Vec<TraitDecl>,
    pub impls: Vec<ImplBlock>,
}

/// ADR 0017. The pipeline's IR representation. Exactly one of
/// three states at any point; the enum makes "the IR is verified"
/// a machine-checked property of the value rather than a boolean
/// beside it.
#[derive(Debug, Default)]
pub enum IrState {
    /// No IR has been produced yet. `BuildSemanticIRPass` is the
    /// first pass that moves out of this state.
    #[default]
    Absent,

    /// Semantic IR has been built but not yet verified.
    /// `BuildSemanticIRPass` produces this; `VerifyIrPass` consumes it.
    Built(SemanticProgram),

    /// Verified IR. Only constructible via `VerifyIrPass` or
    /// `VerifiedIR::new`. Backend entry points require this state.
    Verified(VerifiedIR),
}

pub struct Program {
    pub source: String,
    pub filename: String,

    /// Ast-level inputs, before type checking.
    pub ast: Option<AstPayload>,

    /// Typed AST, produced by `TypeCheckPass`.
    pub typed: Option<TypedProgram>,

    /// The current IR, in exactly one of the three `IrState`
    /// variants. Replaced by each IR-producing phase.
    pub ir: IrState,
}

impl Program {
    pub fn new(source: impl Into<String>, filename: impl Into<String>) -> Self {
        Self {
            source: source.into(),
            filename: filename.into(),
            ast: None,
            typed: None,
            ir: IrState::Absent,
        }
    }

    /// Read the current IR as unverified, if that is its state.
    /// Returns `None` when the IR is `Absent` or `Verified`.
    pub fn semantic_ir(&self) -> Option<&SemanticProgram> {
        match &self.ir {
            IrState::Built(p) => Some(p),
            _ => None,
        }
    }

    /// Read the current IR as verified, if that is its state.
    pub fn verified_ir(&self) -> Option<&VerifiedIR> {
        match &self.ir {
            IrState::Verified(v) => Some(v),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn program_starts_absent() {
        let p = Program::new("src", "test.gol");
        assert!(p.ast.is_none());
        assert!(p.typed.is_none());
        assert!(matches!(p.ir, IrState::Absent));
        assert!(p.semantic_ir().is_none());
        assert!(p.verified_ir().is_none());
    }
}
