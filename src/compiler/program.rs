// src/compiler/program.rs

//! The compilation unit that flows through the pipeline.
//!
//! Deliberately narrow. Config and capabilities live on
//! `CompilerContext`; this struct owns only data derived from the
//! source being compiled.

use crate::compiler::TypedProgram;
use crate::frontend::ast::{FunctionDecl, ImplBlock, TraitDecl};
use crate::ir::semantic_ir::SemanticProgram;
use std::collections::HashMap;
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
    pub span_map: HashMap<usize, (usize, usize)>,
}

pub struct Program {
    pub source: String,
    pub filename: String,

    /// Ast-level inputs, before type checking.
    pub ast: Option<AstPayload>,

    /// Typed AST, produced by `TypeCheckPass`.
    ///
    /// **Invariant:** `typed.functions` MUST be the same `Rc` as
    /// `ast.functions` — same allocation, same addresses. The
    /// analyzer keys `type_table` by those addresses; a clone would
    /// silently invalidate every lookup. See
    /// `docs/compiler/type-table-addressing.md`.
    pub typed: Option<TypedProgram>,

    /// Semantic IR, produced by `BuildSemanticIRPass`.
    pub semantic_ir: Option<SemanticProgram>,

    /// Set once verification has succeeded at least once.
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

    /// Debug assertion: if both `ast` and `typed` are present, their
    /// `functions` fields must point at the same allocation.
    ///
    /// Call this from tests and debug-only paths. It catches the exact
    /// class of bug documented in `docs/compiler/type-table-addressing.md`.
    #[cfg(debug_assertions)]
    pub fn assert_addressing_invariant(&self) {
        if let (Some(ast), Some(typed)) = (self.ast.as_ref(), self.typed.as_ref()) {
            assert!(
                Rc::ptr_eq(&ast.functions, &typed.functions),
                "type-table addressing invariant violated: \
                 ast.functions and typed.functions point at different allocations"
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::compiler::{TypeInfo, TypedProgram};
    #[test]
    fn program_starts_empty() {
        let p = Program::new("src", "test.gol");
        assert!(p.ast.is_none());
        assert!(p.typed.is_none());
        assert!(p.semantic_ir.is_none());
        assert!(!p.verified);
    }

    #[test]
    fn addressing_invariant_holds_when_rc_is_shared() {
        let funcs: Rc<Vec<FunctionDecl>> = Rc::new(Vec::new());
        let mut p = Program::new("", "");
        p.ast = Some(AstPayload {
            functions: Rc::clone(&funcs),
            traits: Vec::new(),
            impls: Vec::new(),
            span_map: HashMap::new(),
        });
        p.typed = Some(TypedProgram {
            functions: Rc::clone(&funcs),
            type_info: TypeInfo::default(),
            type_table: HashMap::new(),
        });
        p.assert_addressing_invariant();
    }

    #[test]
    #[should_panic(expected = "addressing invariant violated")]
    fn addressing_invariant_catches_divergent_allocations() {
        let mut p = Program::new("", "");
        p.ast = Some(AstPayload {
            functions: Rc::new(Vec::new()),
            traits: Vec::new(),
            impls: Vec::new(),
            span_map: HashMap::new(),
        });
        p.typed = Some(TypedProgram {
            functions: Rc::new(Vec::new()), // deliberately a different Rc
            type_info: TypeInfo::default(),
            type_table: HashMap::new(),
        });
        p.assert_addressing_invariant();
    }
}
