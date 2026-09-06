
#![allow(dead_code)]
use crate::ir::semantic_ir::{Instruction, Terminator, SemanticProgram, TypedIRValue, SemanticBinOp};

pub struct Optimizer;
impl Optimizer {
    pub fn new() -> Self { Self }
    pub fn optimize(&mut self, program: &mut SemanticProgram) {
        for func in &mut program.functions {
            let mut reachable = std::collections::HashSet::new();
            let mut worklist = vec![func.entry_block];
            reachable.insert(func.entry_block);
            while let Some(id) = worklist.pop() {
                if let Some(block) = func.blocks.iter().find(|b| b.id == id) {
                    if let Some(term) = &block.terminator {
                        for succ in term.successors() {
                            if reachable.insert(succ) {
                                worklist.push(succ);
                            }
                        }
                    }
                }
            }
            func.blocks.retain(|b| reachable.contains(&b.id));
        }
    }
    fn constant_folding(&self, program: &mut SemanticProgram) {
        for func in &mut program.functions {
            for block in &mut func.blocks {
                for instr in &mut block.instructions {
                    match instr {
                        Instruction::Declare { value, .. } => { Self::fold_value(value); },
                        Instruction::Assign { value, .. } => { Self::fold_value(value); },
                        Instruction::Print { value } => { Self::fold_value(value); },
                        Instruction::ArrayAssign { array, index, value } => { Self::fold_value(array); Self::fold_value(index); Self::fold_value(value); },
                        Instruction::Call { args, .. } => { for a in args { Self::fold_value(a); } },
                        Instruction::Send { value, .. } => { Self::fold_value(value); },
                        _ => {}
                    }
                }
                if let Some(term) = &mut block.terminator {
                    match term {
                        Terminator::Return { value: Some(v), .. } => { Self::fold_value(v); },
                        Terminator::Branch { condition, .. } => { Self::fold_value(condition); },
                        _ => {}
                    }
                }
            }
        }
    }
    fn fold_value(v: &mut TypedIRValue) {
        // stub constant folding
        if let TypedIRValue::BinaryOp { op, left, right, result_type: _ } = v {
            if let (Some(l), Some(r)) = (left.as_constant_f64(), right.as_constant_f64()) {
                let res = match op {
                    SemanticBinOp::Add => l + r,
                    SemanticBinOp::Subtract => l - r,
                    SemanticBinOp::Multiply => l * r,
                    SemanticBinOp::Divide => if r!=0.0 { l / r } else { l },
                    _ => return,
                };
                *v = TypedIRValue::Float(res);
            }
        }
    }
    fn dead_code_elimination(&self, _program: &mut SemanticProgram) {}
}
pub fn optimize(program: &mut SemanticProgram) { Optimizer::new().optimize(program); }
