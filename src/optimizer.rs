// ALGOL26 - Performance Optimizations
// Constant folding and dead code elimination

use crate::semantic_ir::{
    SemanticProgram, SemanticFunction, SemanticInstruction,
    TypedIRValue, SemanticBinOp,
};
use crate::semantic_type::SemanticType;

#[derive(Debug, Default, Clone)]
pub struct OptimizerStats {
    pub folded_constants: usize,
    pub removed_instructions: usize,
    pub removed_blocks: usize,
}

pub struct Optimizer {
    pub stats: OptimizerStats,
}

impl Optimizer {
    pub fn new() -> Self {
        Optimizer {
            stats: OptimizerStats::default(),
        }
    }

    pub fn optimize(&mut self, program: &mut SemanticProgram) {
        self.constant_fold(program);
        self.dead_code_elimination(program);
    }

    fn constant_fold(&mut self, program: &mut SemanticProgram) {
        for func in &mut program.functions {
            for block in &mut func.blocks {
                for instruction in &mut block.instructions {
                    match instruction {
                        SemanticInstruction::Declare { value, .. } => {
                            if let Some(folded) = Self::fold_typed_value(value) {
                                *value = folded;
                                self.stats.folded_constants += 1;
                            }
                        }
                        SemanticInstruction::Assign { value, .. } => {
                            if let Some(folded) = Self::fold_typed_value(value) {
                                *value = folded;
                                self.stats.folded_constants += 1;
                            }
                        }
                        SemanticInstruction::Print { value } => {
                            if let Some(folded) = Self::fold_typed_value(value) {
                                *value = folded;
                                self.stats.folded_constants += 1;
                            }
                        }
                        _ => {}
                    }
                }
            }
        }
    }

    fn fold_typed_value(value: &TypedIRValue) -> Option<TypedIRValue> {
        match value {
            TypedIRValue::BinaryOp { op, left, right, result_type } => {
                if let (Some(l), Some(r)) = (left.as_constant_f64(), right.as_constant_f64()) {
                    let result = match op {
                        SemanticBinOp::Add => l + r,
                        SemanticBinOp::Subtract => l - r,
                        SemanticBinOp::Multiply => l * r,
                        SemanticBinOp::Divide => {
                            if r == 0.0 {
                                return None;
                            }
                            l / r
                        }
                        _ => return None,
                    };
                    
                    match result_type {
                        SemanticType::Float => Some(TypedIRValue::Float(result)),
                        SemanticType::Int => Some(TypedIRValue::Int(result as i64)),
                        _ => Some(TypedIRValue::Float(result)),
                    }
                } else {
                    None
                }
            }
            _ => None,
        }
    }

    fn dead_code_elimination(&mut self, program: &mut SemanticProgram) {
        for func in &mut program.functions {
            let blocks_before = func.blocks.len();
            Self::remove_unreachable_blocks(func);
            let blocks_after = func.blocks.len();
            self.stats.removed_blocks += blocks_before - blocks_after;
        }
    }

    fn remove_unreachable_blocks(func: &mut SemanticFunction) {
        if func.blocks.is_empty() {
            return;
        }

        let mut reachable = vec![false; func.blocks.len()];
        if let Some(entry_idx) = func.blocks.iter().position(|b| b.id == func.entry_block) {
            reachable[entry_idx] = true;
        }

        let mut changed = true;
        while changed {
            changed = false;
            for i in 0..func.blocks.len() {
                if !reachable[i] {
                    continue;
                }
                if let Some(last) = func.blocks[i].instructions.last() {
                    match last {
                        SemanticInstruction::Jump { block: target } => {
                            if let Some(idx) = func.blocks.iter().position(|b| b.id == *target) {
                                if !reachable[idx] {
                                    reachable[idx] = true;
                                    changed = true;
                                }
                            }
                        }
                        SemanticInstruction::Branch {
                            then_block,
                            else_block,
                            ..
                        } => {
                            if let Some(idx) = func.blocks.iter().position(|b| b.id == *then_block) {
                                if !reachable[idx] {
                                    reachable[idx] = true;
                                    changed = true;
                                }
                            }
                            if let Some(idx) = func.blocks.iter().position(|b| b.id == *else_block) {
                                if !reachable[idx] {
                                    reachable[idx] = true;
                                    changed = true;
                                }
                            }
                        }
                        _ => {}
                    }
                }
            }
        }

        let mut kept = Vec::new();
        for (i, block) in func.blocks.drain(..).enumerate() {
            if reachable[i] {
                kept.push(block);
            }
        }
        func.blocks = kept;
    }
}

impl Default for Optimizer {
    fn default() -> Self {
        Self::new()
    }
}
