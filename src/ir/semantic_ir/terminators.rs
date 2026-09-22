// src/ir/semantic_ir/terminators.rs
//
// `Terminator` — the last instruction of a `SemanticBlock`, plus
// the `successors()` method used by the CFG verifier and the
// dataflow engine.

use super::patterns::SemanticPattern;
use super::values::TypedIRValue;
use crate::common::types::Type;

#[derive(Debug, Clone)]
pub enum Terminator {
    Return {
        value: Option<TypedIRValue>,
        type_: Type,
    },
    Jump {
        block: usize,
    },
    Branch {
        condition: TypedIRValue,
        then_block: usize,
        else_block: usize,
    },
    Switch {
        value: TypedIRValue,
        cases: Vec<(SemanticPattern, usize)>,
        default_block: Option<usize>,
    },
    IteratorNext {
        iterator: String,
        target: String,
        body_block: usize,
        exit_block: usize,
    },
    Spawn {
        entry_block: usize,
    },
    Fork {
        blocks: Vec<usize>,
        join_block: usize,
    },
}

impl Terminator {
    pub fn successors(&self) -> Vec<usize> {
        match self {
            Self::Jump { block } => vec![*block],
            Self::Branch {
                then_block,
                else_block,
                ..
            } => vec![*then_block, *else_block],
            Self::IteratorNext {
                body_block,
                exit_block,
                ..
            } => vec![*body_block, *exit_block],
            Self::Switch {
                cases,
                default_block,
                ..
            } => {
                let mut v: Vec<_> = cases.iter().map(|c| c.1).collect();
                if let Some(d) = default_block {
                    v.push(*d);
                }
                v
            }
            Self::Spawn { entry_block } => vec![*entry_block],
            Self::Fork { blocks, .. } => {
                // The join block is *not* a direct successor of the
                // fork. Control reaches the join only after every
                // branch has executed and jumped to it. The
                // interpreter's `pending_forks` mechanism enforces
                // this by intercepting each branch's jump to the
                // join and running the next pending branch instead;
                // only the final branch's jump is allowed through.
                //
                // Listing the join as a successor of the fork would
                // let the CFG claim that pre-fork state can reach
                // the join without executing a branch, which is a
                // path no execution follows.
                blocks.clone()
            }
            _ => vec![],
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fork_successors_do_not_include_join() {
        let term = Terminator::Fork {
            blocks: vec![1, 2],
            join_block: 3,
        };
        let succs = term.successors();
        assert_eq!(succs, vec![1, 2]);
        assert!(
            !succs.contains(&3),
            "join block must not be a direct successor of the fork"
        );
    }

    #[test]
    fn fork_with_single_branch_has_single_successor() {
        let term = Terminator::Fork {
            blocks: vec![5],
            join_block: 9,
        };
        assert_eq!(term.successors(), vec![5]);
    }
}
