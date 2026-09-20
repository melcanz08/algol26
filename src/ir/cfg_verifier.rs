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
//   - fork shape: each branch is entered only from the fork block,
//     exits only via a jump to the join, and the join is not also a
//     branch (see ADR 0011)
//
// Checks NOT performed here (see verifier.rs and future
// data-flow work):
//   - type consistency across block boundaries
//   - ownership/borrow state at joins
//   - domination and use-before-definition
//   - instruction-level semantics

use crate::ir::semantic_ir::{SemanticProgram, Terminator};
use std::collections::{HashMap, HashSet};

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

        // Build a predecessor map. Needed for the fork branch-uniqueness
        // check below. The fork's own `successors()` contributes one edge
        // to each of its branches and one to its join.
        let mut predecessors: HashMap<usize, Vec<usize>> = HashMap::new();
        for block in &func.blocks {
            for succ in block.successors() {
                predecessors.entry(succ).or_default().push(block.id);
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

        // Check for duplicate switch case targets. Switch exhaustiveness
        // is not checked here — that requires type information, and the
        // analyzer's `check_match_exhaustiveness` is where it belongs.
        for block in &func.blocks {
            if let Some(Terminator::Switch { cases, .. }) = &block.terminator {
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

        // Fork shape checks. These close the discontinuity documented in
        // ADR 0011: the interpreter's `pending_forks` worklist requires
        // that each branch is entered only from the fork block and exits
        // only via a jump to the join.
        for block in &func.blocks {
            let (blocks_in_fork, join_block): (&[usize], usize) = match &block.terminator {
                Some(Terminator::Fork { blocks, join_block }) => (blocks.as_slice(), *join_block),
                _ => continue,
            };

            let fork_block_set: HashSet<usize> = blocks_in_fork.iter().copied().collect();

            // Rule 1: each branch entered only from the fork block.
            for branch_id in blocks_in_fork {
                match predecessors.get(branch_id) {
                    Some(preds) if preds.len() == 1 && preds[0] == block.id => {}
                    Some(preds) => {
                        return Err(format!(
                            "Function '{}' fork at block {}: branch {} has {} \
                             predecessor(s); expected exactly one (from the fork)",
                            func.name,
                            block.id,
                            branch_id,
                            preds.len()
                        ));
                    }
                    None => {
                        return Err(format!(
                            "Function '{}' fork at block {}: branch {} has no predecessor",
                            func.name, block.id, branch_id
                        ));
                    }
                }
            }

            // Rule 3: join is not also a branch.
            if fork_block_set.contains(&join_block) {
                return Err(format!(
                    "Function '{}' fork at block {}: join block {} also appears \
                     as a branch",
                    func.name, block.id, join_block
                ));
            }

            // Rule 2: no path exits a branch except via Jump to the join.
            for entry in blocks_in_fork.iter().copied() {
                // Compute the branch's reachable set, excluding the join.
                let mut reachable: HashSet<usize> = HashSet::new();
                let mut worklist: Vec<usize> = vec![entry];
                while let Some(bid) = worklist.pop() {
                    if bid == join_block {
                        continue;
                    }
                    if !reachable.insert(bid) {
                        continue;
                    }
                    if let Some(b) = func.blocks.iter().find(|b| b.id == bid) {
                        for succ in b.successors() {
                            worklist.push(succ);
                        }
                    }
                }

                for &bid in &reachable {
                    // Rule 1 generalization: no branch may reach another's entry.
                    if bid != entry && fork_block_set.contains(&bid) {
                        return Err(format!(
                            "Function '{}' fork at block {}: branch {} reaches \
                             branch {}",
                            func.name, block.id, entry, bid
                        ));
                    }

                    let b =
                        func.blocks.iter().find(|b| b.id == bid).expect(
                            "bid came from reachable, which was built by walking func.blocks",
                        );
                    match &b.terminator {
                        Some(Terminator::Return { .. }) => {
                            return Err(format!(
                                "Function '{}' fork at block {}: branch {} returns \
                                 from the function, which the parallel model does \
                                 not allow",
                                func.name, block.id, entry
                            ));
                        }
                        Some(Terminator::Spawn { .. }) | Some(Terminator::Fork { .. }) => {
                            return Err(format!(
                                "Function '{}' fork at block {}: branch {} contains \
                                 nested concurrency, which the parallel model does \
                                 not allow",
                                func.name, block.id, entry
                            ));
                        }
                        _ => {}
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
    use crate::ir::semantic_ir::{
        SemanticBlock, SemanticFunction, SemanticProgram, Terminator, TypedIRValue,
    };

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

    #[test]
    fn well_formed_fork_is_accepted() {
        let mut program = SemanticProgram::new();
        let entry = program.new_block_id();
        let a = program.new_block_id();
        let b = program.new_block_id();
        let join = program.new_block_id();
        let func = SemanticFunction {
            name: "main".to_string(),
            params: vec![],
            return_type: Type::Void,
            blocks: vec![
                SemanticBlock {
                    id: entry,
                    instructions: vec![],
                    terminator: Some(Terminator::Fork {
                        blocks: vec![a, b],
                        join_block: join,
                    }),
                },
                SemanticBlock {
                    id: a,
                    instructions: vec![],
                    terminator: Some(Terminator::Jump { block: join }),
                },
                SemanticBlock {
                    id: b,
                    instructions: vec![],
                    terminator: Some(Terminator::Jump { block: join }),
                },
                SemanticBlock {
                    id: join,
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
        assert!(CFGVerifier::verify(&program).is_ok());
    }

    #[test]
    fn fork_branch_with_extra_predecessor_is_rejected() {
        let mut program = SemanticProgram::new();
        let entry = program.new_block_id();
        let fork_block = program.new_block_id();
        let extra = program.new_block_id();
        let a = program.new_block_id();
        let join = program.new_block_id();
        let func = SemanticFunction {
            name: "main".to_string(),
            params: vec![],
            return_type: Type::Void,
            blocks: vec![
                SemanticBlock {
                    id: entry,
                    instructions: vec![],
                    terminator: Some(Terminator::Branch {
                        condition: TypedIRValue::Bool(true),
                        then_block: fork_block,
                        else_block: extra,
                    }),
                },
                SemanticBlock {
                    id: fork_block,
                    instructions: vec![],
                    terminator: Some(Terminator::Fork {
                        blocks: vec![a],
                        join_block: join,
                    }),
                },
                SemanticBlock {
                    id: extra,
                    instructions: vec![],
                    terminator: Some(Terminator::Jump { block: a }),
                },
                SemanticBlock {
                    id: a,
                    instructions: vec![],
                    terminator: Some(Terminator::Jump { block: join }),
                },
                SemanticBlock {
                    id: join,
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
        // `a` is a fork branch but has two predecessors: `fork_block`
        // and `extra`. Rule 1 rejects it.
        assert!(CFGVerifier::verify(&program).is_err());
    }

    #[test]
    fn fork_branch_with_early_return_is_rejected() {
        let mut program = SemanticProgram::new();
        let entry = program.new_block_id();
        let a = program.new_block_id();
        let b = program.new_block_id();
        let join = program.new_block_id();
        let func = SemanticFunction {
            name: "main".to_string(),
            params: vec![],
            return_type: Type::Void,
            blocks: vec![
                SemanticBlock {
                    id: entry,
                    instructions: vec![],
                    terminator: Some(Terminator::Fork {
                        blocks: vec![a, b],
                        join_block: join,
                    }),
                },
                SemanticBlock {
                    id: a,
                    instructions: vec![],
                    terminator: Some(Terminator::Return {
                        value: None,
                        type_: Type::Void,
                    }),
                },
                SemanticBlock {
                    id: b,
                    instructions: vec![],
                    terminator: Some(Terminator::Jump { block: join }),
                },
                SemanticBlock {
                    id: join,
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
        // Branch `a` terminates with `Return` instead of `Jump{join}`.
        // Rule 2 rejects it.
        assert!(CFGVerifier::verify(&program).is_err());
    }

    #[test]
    fn fork_branch_with_nested_fork_is_rejected() {
        let mut program = SemanticProgram::new();
        let entry = program.new_block_id();
        let a = program.new_block_id();
        let b = program.new_block_id();
        let join = program.new_block_id();
        let a_inner = program.new_block_id();
        let a_inner_join = program.new_block_id();
        let func = SemanticFunction {
            name: "main".to_string(),
            params: vec![],
            return_type: Type::Void,
            blocks: vec![
                SemanticBlock {
                    id: entry,
                    instructions: vec![],
                    terminator: Some(Terminator::Fork {
                        blocks: vec![a, b],
                        join_block: join,
                    }),
                },
                SemanticBlock {
                    id: a,
                    instructions: vec![],
                    terminator: Some(Terminator::Fork {
                        blocks: vec![a_inner],
                        join_block: a_inner_join,
                    }),
                },
                SemanticBlock {
                    id: a_inner,
                    instructions: vec![],
                    terminator: Some(Terminator::Jump {
                        block: a_inner_join,
                    }),
                },
                SemanticBlock {
                    id: a_inner_join,
                    instructions: vec![],
                    terminator: Some(Terminator::Jump { block: join }),
                },
                SemanticBlock {
                    id: b,
                    instructions: vec![],
                    terminator: Some(Terminator::Jump { block: join }),
                },
                SemanticBlock {
                    id: join,
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
        // Branch `a` contains a nested `Fork`. The outer fork's Rule 2
        // sees `a` in its reachable set and rejects the nested terminator.
        assert!(CFGVerifier::verify(&program).is_err());
    }

    #[test]
    fn fork_join_in_branches_is_rejected() {
        let mut program = SemanticProgram::new();
        let entry = program.new_block_id();
        let a = program.new_block_id();
        let b = program.new_block_id();
        let join = program.new_block_id();
        let func = SemanticFunction {
            name: "main".to_string(),
            params: vec![],
            return_type: Type::Void,
            blocks: vec![
                SemanticBlock {
                    id: entry,
                    instructions: vec![],
                    terminator: Some(Terminator::Fork {
                        blocks: vec![a, b, join],
                        join_block: join,
                    }),
                },
                SemanticBlock {
                    id: a,
                    instructions: vec![],
                    terminator: Some(Terminator::Jump { block: join }),
                },
                SemanticBlock {
                    id: b,
                    instructions: vec![],
                    terminator: Some(Terminator::Jump { block: join }),
                },
                SemanticBlock {
                    id: join,
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
        // `join` appears in its own fork's `blocks` list. Rule 1 fires
        // first here (join has three predecessors), but either Rule 1
        // or Rule 3 would reject the shape. The test asserts rejection.
        assert!(CFGVerifier::verify(&program).is_err());
    }
}
