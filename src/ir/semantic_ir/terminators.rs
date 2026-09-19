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
            Self::Fork { blocks, join_block } => {
                let mut v = blocks.clone();
                v.push(*join_block);
                v
            }
            _ => vec![],
        }
    }
}
