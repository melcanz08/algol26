#![allow(dead_code)]
use crate::ir::semantic_ir::{SemanticBlock, Instruction, Terminator};

pub struct ControlFlowAnalyzer;
pub struct ControlFlowTranslator;

impl ControlFlowAnalyzer {
    pub fn new() -> Self { Self }
    pub fn add_instruction(block: &mut SemanticBlock, instr: Instruction) {
        block.instructions.push(instr);
    }
    pub fn set_terminator(block: &mut SemanticBlock, term: Terminator) {
        block.terminator = Some(term);
    }
}

impl ControlFlowTranslator {
    pub fn new() -> Self { Self }
    pub fn translate(&self, _program: &mut crate::ir::semantic_ir::SemanticProgram) -> Result<(), String> { Ok(()) }
}
