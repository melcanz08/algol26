
#![allow(dead_code)]
use crate::ir::semantic_ir::{Terminator, SemanticProgram};

pub struct DeferLoweringPass;

impl DeferLoweringPass {
    pub fn new() -> Self { Self }
    pub fn lower(&self, program: &mut SemanticProgram) -> Result<(), String> {
        for func in &mut program.functions {
            for block in &mut func.blocks {
                if let Some(Terminator::Defer { cleanup_block }) = &block.terminator {
                    let cb = *cleanup_block;
                    block.terminator = Some(Terminator::Jump { block: cb });
                }
            }
        }
        Ok(())
    }
}
pub fn lower(program: &mut SemanticProgram) -> Result<(), String> { DeferLoweringPass::new().lower(program) }
