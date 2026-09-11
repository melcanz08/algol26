#![allow(dead_code)]

// src/semantics/flow_result.rs

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

#[derive(Debug, Clone, Copy)]
pub struct LoopContext {
    pub break_block: usize,
    pub continue_block: usize,
}

#[derive(Debug, Clone, Default)]
pub struct DeferContext {
    pub cleanup_blocks: Vec<usize>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum CaptureMode {
    Read,
    Write,
    Move,
}

#[derive(Debug, Clone, PartialEq)]
pub enum TerminatorKind {
    Return,
    Jump,
    Branch,
    Switch,
    IteratorNext,
    Fork,
}
