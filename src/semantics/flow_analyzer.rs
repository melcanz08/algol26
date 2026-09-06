// src/semantics/flow_analyzer.rs 

#![allow(dead_code)]
use crate::ir::semantic_ir::{SemanticProgram, SemanticBlock};

pub struct FlowAnalyzer;
impl FlowAnalyzer {
    pub fn analyze(_program: &SemanticProgram) -> Result<(), String> { Ok(()) }
    pub fn is_terminated(block: &SemanticBlock) -> bool { block.terminator.is_some() }
}
