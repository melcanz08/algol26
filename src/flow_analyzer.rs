// ALGOL26 - Flow Analyzer
// Responsible for control flow graph analysis

use crate::semantic_ir::{SemanticBlock, SemanticInstruction};
use crate::flow_result::FlowResult;

pub struct FlowAnalyzer;

impl FlowAnalyzer {
    pub fn new() -> Self {
        FlowAnalyzer
    }
    
    pub fn is_terminated(block: &SemanticBlock) -> bool {
        matches!(
            block.instructions.last(),
            Some(SemanticInstruction::Return { .. })
            | Some(SemanticInstruction::Jump { .. })
            | Some(SemanticInstruction::Branch { .. })
            | Some(SemanticInstruction::Switch { .. })
            | Some(SemanticInstruction::IteratorNext { .. })
            | Some(SemanticInstruction::Fork { .. })
        )
    }
    
    pub fn merge_flows(then_flow: FlowResult, else_flow: FlowResult) -> FlowResult {
        match (then_flow, else_flow) {
            (FlowResult::Unreachable, FlowResult::Unreachable) => {
                FlowResult::Unreachable
            }
            (_t, _e) => {
                // Both or one reachable - need merge block
                FlowResult::Reachable(0) // Placeholder - caller will assign block ID
            }
        }
    }
    
    pub fn is_reachable(flow: &FlowResult) -> bool {
        matches!(flow, FlowResult::Reachable(_))
    }
}

impl Default for FlowAnalyzer {
    fn default() -> Self {
        Self::new()
    }
}
