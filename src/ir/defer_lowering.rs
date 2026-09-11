#![allow(dead_code)]

// src/ir/defer_lowering.rs

use crate::ir::semantic_ir::{SemanticProgram, Terminator};

pub struct DeferLoweringPass;

impl DeferLoweringPass {
    pub fn new() -> Self {
        Self
    }

    pub fn lower(&self, program: &mut SemanticProgram) -> Result<(), String> {
        for func in &mut program.functions {
            self.lower_function(func)?;
        }
        Ok(())
    }

    fn lower_function(
        &self,
        func: &mut crate::ir::semantic_ir::SemanticFunction,
    ) -> Result<(), String> {
        // Collect all defer terminators and their cleanup blocks
        let mut defer_worklist: Vec<(usize, usize)> = Vec::new(); // (block_id, cleanup_block)

        for block in &func.blocks {
            if let Some(Terminator::Defer { cleanup_block }) = block.terminator {
                defer_worklist.push((block.id, cleanup_block));
            }
        }

        // Process each defer
        for (block_id, cleanup_block_id) in defer_worklist {
            self.lower_defer(func, block_id, cleanup_block_id)?;
        }

        Ok(())
    }

    fn lower_defer(
        &self,
        func: &mut crate::ir::semantic_ir::SemanticFunction,
        block_id: usize,
        cleanup_block_id: usize,
    ) -> Result<(), String> {
        // Get the original terminator of the block with defer
        let original_terminator = {
            let block = func
                .blocks
                .iter()
                .find(|b| b.id == block_id)
                .ok_or_else(|| format!("Block {} not found", block_id))?;

            block
                .terminator
                .clone()
                .ok_or_else(|| format!("Block {} has no terminator", block_id))?
        };

        // Extract the original target from the defer terminator
        let (original_target, is_return) = match original_terminator {
            Terminator::Defer { cleanup_block: _ } => {
                // Defer was the only terminator - need to find original target
                // For now, assume return after defer
                (None, true)
            }
            _ => {
                // This shouldn't happen - we only process defer terminators
                return Err(format!("Block {} does not have defer terminator", block_id));
            }
        };

        // Set block terminator to jump to cleanup
        if let Some(block) = func.blocks.iter_mut().find(|b| b.id == block_id) {
            block.terminator = Some(Terminator::Jump {
                block: cleanup_block_id,
            });
        }

        // Set cleanup block terminator to original target
        if let Some(cleanup_block) = func.blocks.iter_mut().find(|b| b.id == cleanup_block_id) {
            if is_return {
                cleanup_block.terminator = Some(Terminator::Return {
                    value: None,
                    type_: func.return_type.clone(),
                });
            } else if let Some(target) = original_target {
                cleanup_block.terminator = Some(Terminator::Jump { block: target });
            } else {
                cleanup_block.terminator = Some(Terminator::Return {
                    value: None,
                    type_: func.return_type.clone(),
                });
            }
        }

        Ok(())
    }
}

pub fn lower(program: &mut SemanticProgram) -> Result<(), String> {
    DeferLoweringPass::new().lower(program)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::common::types::Type;
    use crate::ir::semantic_ir::{SemanticBlock, SemanticFunction, SemanticProgram, Terminator};

    #[test]
    fn test_lower_defer() {
        let mut program = SemanticProgram::new();
        let entry = program.new_block_id();
        let cleanup = program.new_block_id();

        let func = SemanticFunction {
            name: "main".to_string(),
            params: vec![],
            return_type: Type::Void,
            blocks: vec![
                SemanticBlock {
                    id: entry,
                    instructions: vec![],
                    terminator: Some(Terminator::Defer {
                        cleanup_block: cleanup,
                    }),
                },
                SemanticBlock {
                    id: cleanup,
                    instructions: vec![],
                    terminator: None,
                },
            ],
            entry_block: entry,
            is_extern: false,
        };
        program.functions.push(func);

        let pass = DeferLoweringPass::new();
        assert!(pass.lower(&mut program).is_ok());

        // Entry should jump to cleanup
        assert!(matches!(
            program.functions[0].blocks[0].terminator,
            Some(Terminator::Jump { block: _cleanup })
        ));

        // Cleanup should have a terminator now
        assert!(program.functions[0].blocks[1].terminator.is_some());
    }
}
