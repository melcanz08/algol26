// ALGOL26 - Flow Analyzer
// Responsible for control flow graph analysis

use crate::ir::semantic_ir::{SemanticBlock, SemanticInstruction};
use crate::semantics::flow_result::FlowResult;

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
            (FlowResult::Reachable(id), FlowResult::Unreachable) => {
                // Only then branch is reachable — use its block ID
                FlowResult::Reachable(id)
            }
            (FlowResult::Unreachable, FlowResult::Reachable(id)) => {
                // Only else branch is reachable — use its block ID
                FlowResult::Reachable(id)
            }
            (FlowResult::Reachable(_), FlowResult::Reachable(_)) => {
                // Both reachable — caller must create a merge block.
                // Return Unreachable here; the caller will handle merging.
                // This is NOT a fake ID — it's a signal that merging is needed.
                FlowResult::Unreachable
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
