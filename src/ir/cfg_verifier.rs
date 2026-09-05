#![allow(dead_code)]

// src/ir/cfg_verifier.rs - BlockResult - tracks termination state of CFG blocks
// Prevents emitting dead code after terminators

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BlockResult {
    pub block_id: usize,
    pub terminated: bool,
}

impl BlockResult {
    pub fn new(block_id: usize) -> Self {
        BlockResult {
            block_id,
            terminated: false,
        }
    }

    pub fn terminated(block_id: usize) -> Self {
        BlockResult {
            block_id,
            terminated: true,
        }
    }

    pub fn is_terminated(&self) -> bool {
        self.terminated
    }
}

// SemanticCFGVerifier - complete CFG invariant checker
pub struct SemanticCFGVerifier;

impl SemanticCFGVerifier {
    pub fn verify(func: &crate::ir::semantic_ir::SemanticFunction) -> Result<(), String> {
        use crate::ir::semantic_ir::SemanticInstruction;
        use std::collections::HashSet;

        // 1. Check duplicate block IDs
        let mut block_ids = HashSet::new();
        for block in &func.blocks {
            if !block_ids.insert(block.id) {
                return Err(format!("Duplicate block ID found: {}", block.id));
            }
        }

        // 2. Check entry block exists
        if !block_ids.contains(&func.entry_block) {
            return Err(format!("Entry block {} does not exist", func.entry_block));
        }

        // 3. Check each block
        for block in &func.blocks {
            let mut terminator_count = 0;
            let mut found_terminator = false;

            for inst in &block.instructions {
                // Check no instruction after terminator
                if found_terminator {
                    return Err(format!(
                        "Instruction emitted after terminator in block {}",
                        block.id
                    ));
                }

                match inst {
                    SemanticInstruction::Jump { block: target } => {
                        found_terminator = true;
                        terminator_count += 1;
                        if !block_ids.contains(target) {
                            return Err(format!(
                                "Invalid jump target {} in block {}",
                                target, block.id
                            ));
                        }
                    }
                    SemanticInstruction::Branch {
                        then_block,
                        else_block,
                        ..
                    } => {
                        found_terminator = true;
                        terminator_count += 1;
                        if !block_ids.contains(then_block) || !block_ids.contains(else_block) {
                            return Err(format!("Invalid branch target in block {}", block.id));
                        }
                    }
                    SemanticInstruction::Return { .. } => {
                        found_terminator = true;
                        terminator_count += 1;
                    }
                    _ => {}
                }
            }

            // 4. Check block has exactly one terminator (except empty merge blocks)
            if terminator_count == 0 && !block.instructions.is_empty() {
                return Err(format!(
                    "Block {} lacks a valid terminator instruction",
                    block.id
                ));
            }

            if terminator_count > 1 {
                return Err(format!(
                    "Block {} has multiple terminators ({})",
                    block.id, terminator_count
                ));
            }
        }

        Ok(())
    }

    pub fn verify_program(program: &crate::ir::semantic_ir::SemanticProgram) -> Result<(), String> {
        for func in &program.functions {
            Self::verify(func)?;
        }
        Ok(())
    }

    pub fn assert_valid(program: &crate::ir::semantic_ir::SemanticProgram) {
        if let Err(e) = Self::verify_program(program) {
            panic!("CFG verification failed: {}", e);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::common::types::Type;
    use crate::ir::semantic_ir::{
        SemanticBlock, SemanticFunction, SemanticInstruction, SemanticProgram, TypedIRValue,
    };

    #[test]
    fn test_empty_program() {
        let program = SemanticProgram::new();
        assert!(SemanticCFGVerifier::verify_program(&program).is_ok());
    }

    #[test]
    fn test_duplicate_block_id() {
        let func = SemanticFunction {
            name: "test".to_string(),
            params: Vec::new(),
            return_type: Type::Void,
            blocks: vec![
                SemanticBlock {
                    id: 0,
                    instructions: vec![SemanticInstruction::Return {
                        value: None,
                        type_: Type::Void,
                    }],
                },
                SemanticBlock {
                    id: 0,
                    instructions: Vec::new(),
                },
            ],
            entry_block: 0,
            is_extern: false,
        };

        assert!(SemanticCFGVerifier::verify(&func).is_err());
    }

    #[test]
    fn test_instruction_after_terminator() {
        let func = SemanticFunction {
            name: "test".to_string(),
            params: Vec::new(),
            return_type: Type::Void,
            blocks: vec![SemanticBlock {
                id: 0,
                instructions: vec![
                    SemanticInstruction::Return {
                        value: None,
                        type_: Type::Void,
                    },
                    SemanticInstruction::Print {
                        value: TypedIRValue::Float(1.0),
                    },
                ],
            }],
            entry_block: 0,
            is_extern: false,
        };

        assert!(SemanticCFGVerifier::verify(&func).is_err());
    }

    #[test]
    fn test_invalid_jump_target() {
        let func = SemanticFunction {
            name: "test".to_string(),
            params: Vec::new(),
            return_type: Type::Void,
            blocks: vec![SemanticBlock {
                id: 0,
                instructions: vec![SemanticInstruction::Jump { block: 999 }],
            }],
            entry_block: 0,
            is_extern: false,
        };

        assert!(SemanticCFGVerifier::verify(&func).is_err());
    }

    #[test]
    fn test_valid_cfg() {
        let func = SemanticFunction {
            name: "test".to_string(),
            params: Vec::new(),
            return_type: Type::Void,
            blocks: vec![SemanticBlock {
                id: 0,
                instructions: vec![SemanticInstruction::Return {
                    value: None,
                    type_: Type::Void,
                }],
            }],
            entry_block: 0,
            is_extern: false,
        };

        assert!(SemanticCFGVerifier::verify(&func).is_ok());
    }
}
