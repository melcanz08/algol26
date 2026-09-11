#![allow(dead_code)]

// src/semantics/control_flow.rs

use crate::ir::semantic_ir::{
    Instruction, SemanticBlock, SemanticFunction, SemanticProgram, Terminator,
};

pub struct ControlFlowAnalyzer;
pub struct ControlFlowTranslator;

impl ControlFlowAnalyzer {
    pub fn new() -> Self {
        Self
    }

    pub fn add_instruction(block: &mut SemanticBlock, instr: Instruction) {
        block.instructions.push(instr);
    }

    pub fn set_terminator(block: &mut SemanticBlock, term: Terminator) {
        block.terminator = Some(term);
    }

    pub fn analyze_function(func: &SemanticFunction) -> Result<(), String> {
        // Check entry block exists
        if !func.blocks.iter().any(|b| b.id == func.entry_block) {
            return Err(format!("Function '{}' missing entry block", func.name));
        }

        // Check all blocks terminated
        for block in &func.blocks {
            if block.terminator.is_none() {
                return Err(format!("Block {} has no terminator", block.id));
            }
        }

        // Check all successors exist
        let block_ids: std::collections::HashSet<usize> =
            func.blocks.iter().map(|b| b.id).collect();

        for block in &func.blocks {
            if let Some(term) = &block.terminator {
                for succ in term.successors() {
                    if !block_ids.contains(&succ) {
                        return Err(format!(
                            "Block {} references non-existent successor {}",
                            block.id, succ
                        ));
                    }
                }
            }
        }

        Ok(())
    }
}

impl ControlFlowTranslator {
    pub fn new() -> Self {
        Self
    }

    pub fn translate(&self, program: &mut SemanticProgram) -> Result<(), String> {
        for func in &mut program.functions {
            if let Err(e) = self.translate_function(func) {
                eprintln!("DEBUG translate: func '{}' failed: {}", func.name, e);
                return Err(e);
            }
        }
        Ok(())
    }

    fn translate_function(&self, func: &mut SemanticFunction) -> Result<(), String> {
        // Step 1: Ensure all blocks have proper terminators
        for block in &mut func.blocks {
            if block.terminator.is_none() {
                block.terminator = Some(Terminator::Return {
                    value: None,
                    type_: func.return_type.clone(),
                });
            }
        }

        // Step 2: Verify function structure
        ControlFlowAnalyzer::analyze_function(func)?;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::common::types::Type;
    use crate::ir::semantic_ir::{SemanticBlock, SemanticFunction, SemanticProgram, Terminator};

    #[test]
    fn test_analyze_valid_function() {
        let mut program = SemanticProgram::new();
        let entry = program.new_block_id();
        let func = SemanticFunction {
            name: "main".to_string(),
            params: vec![],
            return_type: Type::Void,
            blocks: vec![SemanticBlock {
                id: entry,
                instructions: vec![],
                terminator: Some(Terminator::Return {
                    value: None,
                    type_: Type::Void,
                }),
            }],
            entry_block: entry,
            is_extern: false,
        };

        assert!(ControlFlowAnalyzer::analyze_function(&func).is_ok());
    }

    #[test]
    fn test_analyze_missing_entry() {
        let func = SemanticFunction {
            name: "main".to_string(),
            params: vec![],
            return_type: Type::Void,
            blocks: vec![SemanticBlock {
                id: 0,
                instructions: vec![],
                terminator: Some(Terminator::Return {
                    value: None,
                    type_: Type::Void,
                }),
            }],
            entry_block: 999,
            is_extern: false,
        };

        assert!(ControlFlowAnalyzer::analyze_function(&func).is_err());
    }

    #[test]
    fn test_analyze_missing_terminator() {
        let func = SemanticFunction {
            name: "main".to_string(),
            params: vec![],
            return_type: Type::Void,
            blocks: vec![SemanticBlock {
                id: 0,
                instructions: vec![],
                terminator: None,
            }],
            entry_block: 0,
            is_extern: false,
        };

        assert!(ControlFlowAnalyzer::analyze_function(&func).is_err());
    }

    #[test]
    fn test_translate_adds_terminators() {
        let mut program = SemanticProgram::new();
        let entry = program.new_block_id();
        let func = SemanticFunction {
            name: "main".to_string(),
            params: vec![],
            return_type: Type::Void,
            blocks: vec![SemanticBlock {
                id: entry,
                instructions: vec![],
                terminator: None,
            }],
            entry_block: entry,
            is_extern: false,
        };
        program.functions.push(func);

        let translator = ControlFlowTranslator::new();
        assert!(translator.translate(&mut program).is_ok());
        assert!(program.functions[0].blocks[0].terminator.is_some());
    }
}
