// ALGOL26 - Control Flow Translator
// Responsible for translating control flow statements

use crate::ir::semantic_ir::{SemanticProgram, SemanticFunction, SemanticBlock, SemanticInstruction};
use crate::semantics::flow_result::FlowResult;
use crate::semantics::flow_analyzer::FlowAnalyzer;

pub struct ControlFlowTranslator;

impl ControlFlowTranslator {
    pub fn new() -> Self {
        ControlFlowTranslator
    }
    
    pub fn ensure_block(func: &mut SemanticFunction, block_id: usize) {
        if !func.blocks.iter().any(|b| b.id == block_id) {
            func.blocks.push(SemanticBlock {
                id: block_id,
                instructions: Vec::new(),
            });
        }
    }
    
    pub fn add_instruction(func: &mut SemanticFunction, block_id: usize, instruction: SemanticInstruction) {
        Self::ensure_block(func, block_id);
        if let Some(block) = func.blocks.iter_mut().find(|b| b.id == block_id) {
            block.instructions.push(instruction);
        }
    }
    
    pub fn is_terminated(func: &SemanticFunction, block_id: usize) -> bool {
        func.blocks
            .iter()
            .find(|b| b.id == block_id)
            .map(|b| FlowAnalyzer::is_terminated(b))
            .unwrap_or(false)
    }
    
    pub fn add_jump_if_needed(func: &mut SemanticFunction, from_block: usize, to_block: usize) {
        if !Self::is_terminated(func, from_block) {
            Self::add_instruction(func, from_block, SemanticInstruction::Jump { block: to_block });
        }
    }
    
    pub fn create_merge_block(
        program: &mut SemanticProgram,
        func: &mut SemanticFunction,
        flows: &[FlowResult],
    ) -> FlowResult {
        let any_reachable = flows.iter().any(|f| matches!(f, FlowResult::Reachable(_)));
        
        if !any_reachable {
            return FlowResult::Unreachable;
        }
        
        let merge_id = program.new_block_id();
        
        for flow in flows {
            if let FlowResult::Reachable(id) = flow {
                if !Self::is_terminated(func, *id) {
                    Self::add_instruction(func, *id, SemanticInstruction::Jump { block: merge_id });
                }
            }
        }
        
        Self::ensure_block(func, merge_id);
        
        FlowResult::Reachable(merge_id)
    }
}

impl Default for ControlFlowTranslator {
    fn default() -> Self {
        Self::new()
    }
}
