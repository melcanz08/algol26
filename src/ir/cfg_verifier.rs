// src/ir/cfg_verifier.rs
//
// Structural verification of a SemanticProgram's control flow graph.
// Checks performed:
//   - the function has an entry block
//   - block IDs are unique within a function
//   - every block has a terminator
//   - every jump/branch/switch target resolves to an existing block
//   - no unreachable blocks from the entry
//   - function names are unique across the program
//
// Checks NOT performed here (see verifier.rs and future
// data-flow work):
//   - type consistency across block boundaries
//   - ownership/borrow state at joins
//   - domination and use-before-definition
//   - instruction-level semantics

use crate::ir::semantic_ir::{SemanticProgram, Terminator};
use std::collections::HashSet;

pub struct CFGVerifier;

impl CFGVerifier {
    pub fn verify(program: &SemanticProgram) -> Result<(), String> {
        for func in &program.functions {
            Self::verify_function(func)?;
        }

        // Check for duplicate function names
        let mut func_names = HashSet::new();
        for func in &program.functions {
            if !func_names.insert(&func.name) {
                return Err(format!("Duplicate function name '{}'", func.name));
            }
        }

        Ok(())
    }

    fn verify_function(func: &crate::ir::semantic_ir::SemanticFunction) -> Result<(), String> {
        // Check function has blocks
        if func.blocks.is_empty() {
            return Err(format!("Function '{}' has no blocks", func.name));
        }

        // Check for duplicate block IDs
        let mut ids: HashSet<usize> = HashSet::new();
        for block in &func.blocks {
            if !ids.insert(block.id) {
                return Err(format!(
                    "Function '{}' has duplicate block id {}",
                    func.name, block.id
                ));
            }
        }

        // Check entry block exists
        if !ids.contains(&func.entry_block) {
            return Err(format!(
                "Function '{}' entry block {} not found",
                func.name, func.entry_block
            ));
        }

        // Check all blocks have terminators (except extern functions)
        if !func.is_extern {
            for block in &func.blocks {
                if block.terminator.is_none() {
                    return Err(format!(
                        "Function '{}' block {} has no terminator",
                        func.name, block.id
                    ));
                }
            }
        }

        // Check all successor blocks exist
        for block in &func.blocks {
            for succ in block.successors() {
                if !ids.contains(&succ) {
                    return Err(format!(
                        "Function '{}' block {} jumps to unknown block {}",
                        func.name, block.id, succ
                    ));
                }
            }
        }

        // Check for unreachable blocks (except entry)
        let mut reachable = HashSet::new();
        let mut worklist = vec![func.entry_block];

        while let Some(block_id) = worklist.pop() {
            if reachable.insert(block_id) {
                if let Some(block) = func.blocks.iter().find(|b| b.id == block_id) {
                    for succ in block.successors() {
                        if !reachable.contains(&succ) {
                            worklist.push(succ);
                        }
                    }
                }
            }
        }

        for block in &func.blocks {
            if block.id != func.entry_block && !reachable.contains(&block.id) {
                return Err(format!(
                    "Function '{}' has unreachable block {}",
                    func.name, block.id
                ));
            }
        }

        // Check for multiple entry blocks
        let mut entry_count = 0;
        for block in &func.blocks {
            if block.id == func.entry_block {
                entry_count += 1;
            }
        }
        if entry_count != 1 {
            return Err(format!(
                "Function '{}' has {} entry blocks (expected 1)",
                func.name, entry_count
            ));
        }

        // Check switch exhaustiveness
        for block in &func.blocks {
            if let Some(Terminator::Switch {
                cases,
                default_block,
                ..
            }) = &block.terminator
            {
                if default_block.is_none() {
                    // Check if all possible values are covered
                    // For now, just warn about missing default
                    // Full exhaustiveness checking requires type information
                }

                // Check for duplicate case targets
                let mut case_targets = HashSet::new();
                for (_, target) in cases {
                    if !case_targets.insert(*target) {
                        return Err(format!(
                            "Function '{}' block {} has duplicate switch case target {}",
                            func.name, block.id, target
                        ));
                    }
                }
            }
        }

        Ok(())
    }
}

pub fn verify(program: &SemanticProgram) -> Result<(), String> {
    CFGVerifier::verify(program)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::common::types::Type;
    use crate::ir::semantic_ir::{SemanticBlock, SemanticFunction, SemanticProgram, Terminator};

    fn create_test_program() -> SemanticProgram {
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
        program.functions.push(func);
        program
    }

    #[test]
    fn test_valid_program() {
        let program = create_test_program();
        assert!(CFGVerifier::verify(&program).is_ok());
    }

    #[test]
    fn test_duplicate_block_id() {
        let mut program = SemanticProgram::new();
        let block_id = program.new_block_id();
        let func = SemanticFunction {
            name: "main".to_string(),
            params: vec![],
            return_type: Type::Void,
            blocks: vec![
                SemanticBlock {
                    id: block_id,
                    instructions: vec![],
                    terminator: None,
                },
                SemanticBlock {
                    id: block_id,
                    instructions: vec![],
                    terminator: None,
                },
            ],
            entry_block: block_id,
            is_extern: false,
        };
        program.functions.push(func);
        assert!(CFGVerifier::verify(&program).is_err());
    }

    #[test]
    fn test_missing_terminator() {
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
        assert!(CFGVerifier::verify(&program).is_err());
    }

    #[test]
    fn test_unreachable_block() {
        let mut program = SemanticProgram::new();
        let entry = program.new_block_id();
        let unreachable = program.new_block_id();
        let func = SemanticFunction {
            name: "main".to_string(),
            params: vec![],
            return_type: Type::Void,
            blocks: vec![
                SemanticBlock {
                    id: entry,
                    instructions: vec![],
                    terminator: Some(Terminator::Return {
                        value: None,
                        type_: Type::Void,
                    }),
                },
                SemanticBlock {
                    id: unreachable,
                    instructions: vec![],
                    terminator: Some(Terminator::Return {
                        value: None,
                        type_: Type::Void,
                    }),
                },
            ],
            entry_block: entry,
            is_extern: false,
        };
        program.functions.push(func);
        assert!(CFGVerifier::verify(&program).is_err());
    }
}
