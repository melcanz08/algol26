#![allow(dead_code)]

// src/semantics/flow_analyzer.rs - SIMPLE VERSION
use crate::ir::semantic_ir::{SemanticBlock, SemanticProgram};

pub struct FlowAnalyzer;

impl FlowAnalyzer {
    pub fn analyze(_program: &SemanticProgram) -> Result<(), String> {
        Ok(())
    }

    pub fn is_terminated(block: &SemanticBlock) -> bool {
        block.terminator.is_some()
    }
}
