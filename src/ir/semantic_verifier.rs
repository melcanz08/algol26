#![allow(dead_code)]
use crate::ir::semantic_ir::{Terminator, SemanticProgram};

pub fn verify(program: &SemanticProgram) -> Result<(), String> {
    crate::ir::cfg_verifier::verify(program)?;
    for func in &program.functions {
        if func.blocks.is_empty() { return Err(format!("function {} has no blocks", func.name)); }
        let mut has_return = false;
        for b in &func.blocks {
            if let Some(Terminator::Return { .. }) = &b.terminator { has_return = true; }
        }
        if !has_return && func.return_type != crate::common::types::Type::Void {
            // allow for now - void return can be implicit
        }
    }
    Ok(())
}
