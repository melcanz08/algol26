// src/compiler/program.rs

//! The compilation unit that flows through the pipeline.
//!
//! Right now this only carries what `VerifyIrPass` needs. As more
//! passes migrate, more fields appear here. Deliberately *not* the
//! god-object: config lives in `CompilerContext`, not here.

use crate::common::types::Type;
use crate::frontend::ast::FunctionDecl;
use crate::ir::semantic_ir::SemanticProgram;
use std::collections::HashMap;
use std::rc::Rc;

/// The Ast-level payload that `BuildSemanticIRPass` consumes.
///
/// Kept as a named struct rather than two loose fields so that
/// subsequent Ast-level passes (should any be added) share the
/// same shape and the `Program` doesn't grow N parallel fields.
pub struct AstPayload {
    pub functions: Rc<Vec<FunctionDecl>>,
    pub type_table: HashMap<usize, Type>,
}

pub struct Program {
    pub source: String,
    pub filename: String,

    /// Ast-level inputs, consumed by the lowering pass.
    pub ast: Option<AstPayload>,

    /// Semantic IR. `None` before the lowering pass runs, `Some` after.
    pub semantic_ir: Option<SemanticProgram>,

    pub verified: bool,
}

impl Program {
    pub fn new(source: impl Into<String>, filename: impl Into<String>) -> Self {
        Self {
            source: source.into(),
            filename: filename.into(),
            ast: None,
            semantic_ir: None,
            verified: false,
        }
    }
}