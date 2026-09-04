#![allow(dead_code)]

// FlowResult - tracks reachability during CFG construction

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum FlowResult {
    Reachable(usize),
    Unreachable,
}

impl FlowResult {
    pub fn block_id(&self) -> Option<usize> {
        match self {
            FlowResult::Reachable(id) => Some(*id),
            FlowResult::Unreachable => None,
        }
    }
    
    pub fn is_reachable(&self) -> bool {
        matches!(self, FlowResult::Reachable(_))
    }
}

// Coercion helper
/*impl crate::common::types::Type {
    pub fn can_coerce_to(&self, target: &crate::common::types::Type) -> bool {
        match (self, target) {
            (crate::common::types::Type::Int, crate::common::types::Type::Float) => true,
            (crate::common::types::Type::Unknown, _) => true,
            (_, crate::common::types::Type::Unknown) => true,
            (a, b) => a == b,
        }
    }
}*/

// Loop context for break/continue
#[derive(Debug, Clone, Copy)]
pub struct LoopContext {
    pub break_block: usize,
    pub continue_block: usize,
}

// Defer context for cleanup blocks
#[derive(Debug, Clone, Default)]
pub struct DeferContext {
    pub cleanup_blocks: Vec<usize>,
}

// Variable capture modes for concurrency
#[derive(Debug, Clone, PartialEq)]
pub enum CaptureMode {
    Read,
    Write,
    Move,
}

// Terminator kinds
#[derive(Debug, Clone, PartialEq)]
pub enum TerminatorKind {
    Return,
    Jump,
    Branch,
    Switch,
    IteratorNext,
    Fork,
}
