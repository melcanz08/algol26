// ALGOL26 - Performance Optimizations
// Constant folding and dead code elimination

use std::collections::HashMap;

use crate::common::types::Type;
use crate::ir::semantic_ir::{
    SemanticBinOp, SemanticFunction, SemanticInstruction, SemanticProgram, TypedIRValue,
};

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
        self.constant_propagation(program);
        self.constant_fold(program);
        self.common_subexpression_elimination(program);
        self.dead_code_elimination(program);
    }

    /// Eliminates redundant binary operations by reusing previously computed results.
    fn common_subexpression_elimination(&mut self, program: &mut SemanticProgram) {
        for func in &mut program.functions {
            // Maps computation signature to result variable name
            let mut seen_ops: HashMap<String, String> = HashMap::new();
            let mut eliminated = 0usize;

            for block in &mut func.blocks {
                for instruction in &mut block.instructions {
                    match instruction {
                        SemanticInstruction::Declare { name, value, .. } => {
                            if let TypedIRValue::BinaryOp {
                                op, left, right, ..
                            } = value
                            {
                                let signature = Self::make_op_signature(op, left, right);
                                if let Some(existing) = seen_ops.get(&signature) {
                                    // Replace with variable reference
                                    if existing != name {
                                        *value =
                                            TypedIRValue::Variable(existing.clone(), Type::Float);
                                        eliminated += 1;
                                    }
                                } else {
                                    seen_ops.insert(signature, name.clone());
                                }
                            }
                        }
                        SemanticInstruction::Assign { target, value } => {
                            if let TypedIRValue::BinaryOp {
                                op, left, right, ..
                            } = value
                            {
                                let signature = Self::make_op_signature(op, left, right);
                                if let Some(existing) = seen_ops.get(&signature) {
                                    if existing != target {
                                        *value =
                                            TypedIRValue::Variable(existing.clone(), Type::Float);
                                        eliminated += 1;
                                    }
                                } else {
                                    seen_ops.insert(signature, target.clone());
                                }
                            }
                        }
                        _ => {}
                    }
                }
            }

            self.stats.removed_instructions += eliminated;
        }
    }

    /// Creates a string signature for a binary operation.
    fn make_op_signature(op: &SemanticBinOp, left: &TypedIRValue, right: &TypedIRValue) -> String {
        format!("{:?}:{:?}:{:?}", op, left, right)
    }

    /// Propagates known constant values across instructions.
    /// Replaces Variable(name) with its constant value when safe.
    fn constant_propagation(&mut self, program: &mut SemanticProgram) {
        for func in &mut program.functions {
            let mut constants: HashMap<String, TypedIRValue> = HashMap::new();

            for block in &mut func.blocks {
                for instruction in &mut block.instructions {
                    match instruction {
                        SemanticInstruction::Declare {
                            name,
                            value,
                            mutable,
                            ..
                        } => {
                            // Track immutable variables with constant values
                            if !*mutable {
                                if let TypedIRValue::Int(_)
                                | TypedIRValue::Float(_)
                                | TypedIRValue::String(_)
                                | TypedIRValue::Bool(_) = value
                                {
                                    constants.insert(name.clone(), value.clone());
                                }
                            }
                            // Replace variables in the value with known constants
                            Self::propagate_in_value(value, &constants);
                        }
                        SemanticInstruction::Assign { target, value } => {
                            // Replace variables in the value
                            Self::propagate_in_value(value, &constants);
                            // Update constant tracking
                            if let TypedIRValue::Int(_)
                            | TypedIRValue::Float(_)
                            | TypedIRValue::String(_)
                            | TypedIRValue::Bool(_) = value
                            {
                                constants.insert(target.clone(), value.clone());
                            } else {
                                constants.remove(target);
                            }
                        }
                        SemanticInstruction::Print { value } => {
                            Self::propagate_in_value(value, &constants);
                        }
                        SemanticInstruction::Return { value: Some(v), .. } => {
                            Self::propagate_in_value(v, &constants);
                        }
                        SemanticInstruction::Return { .. } => {}
                        SemanticInstruction::Branch { condition, .. } => {
                            Self::propagate_in_value(condition, &constants);
                        }
                        SemanticInstruction::Call { args, .. } => {
                            for arg in args {
                                Self::propagate_in_value(arg, &constants);
                            }
                        }
                        SemanticInstruction::ArrayAssign {
                            array,
                            index,
                            value,
                            ..
                        } => {
                            Self::propagate_in_value(array, &constants);
                            Self::propagate_in_value(index, &constants);
                            Self::propagate_in_value(value, &constants);
                        }
                        SemanticInstruction::Send { value, .. } => {
                            Self::propagate_in_value(value, &constants);
                        }
                        _ => {}
                    }
                }
            }
        }
    }

    /// Recursively replaces Variable(name) with constant values in the given value.
    fn propagate_in_value(value: &mut TypedIRValue, constants: &HashMap<String, TypedIRValue>) {
        match value {
            TypedIRValue::Variable(name, _) => {
                if let Some(const_val) = constants.get(name) {
                    *value = const_val.clone();
                }
            }
            TypedIRValue::BinaryOp { left, right, .. } => {
                Self::propagate_in_value(left, constants);
                Self::propagate_in_value(right, constants);
            }
            TypedIRValue::Call { args, .. } => {
                for arg in args {
                    Self::propagate_in_value(arg, constants);
                }
            }
            TypedIRValue::ArrayAccess { array, index, .. } => {
                Self::propagate_in_value(array, constants);
                Self::propagate_in_value(index, constants);
            }
            TypedIRValue::List(elements, _) => {
                for elem in elements {
                    Self::propagate_in_value(elem, constants);
                }
            }
            TypedIRValue::Some(v) => {
                Self::propagate_in_value(v, constants);
            }
            TypedIRValue::Ok { value: v, .. } => {
                Self::propagate_in_value(v, constants);
            }
            TypedIRValue::Error { value: v, .. } => {
                Self::propagate_in_value(v, constants);
            }
            _ => {}
        }
    }

    fn constant_fold(&mut self, program: &mut SemanticProgram) {
        for func in &mut program.functions {
            // Track known constant values for variables in the current function scope
            let mut known_constants: HashMap<String, TypedIRValue> = HashMap::new();

            for block in &mut func.blocks {
                for instruction in &mut block.instructions {
                    match instruction {
                        SemanticInstruction::Declare {
                            name,
                            value,
                            mutable,
                            ..
                        } => {
                            // Only track immutable variables as constants
                            if *mutable {
                                let _ = name;
                                let _ = value;
                                continue;
                            }
                            // Check if value is already a constant
                            let is_already_constant =
                                matches!(value, TypedIRValue::Int(_) | TypedIRValue::Float(_));

                            // Try to resolve any variables inside this value using our map
                            let resolved_value =
                                Self::resolve_and_fold_value(value, &known_constants);

                            // Only fold if the value actually changed
                            if let Some(folded) = resolved_value {
                                if *value != folded {
                                    *value = folded.clone();
                                    self.stats.folded_constants += 1;
                                }

                                // Remember this variable is a constant for future lines!
                                if let TypedIRValue::Int(_) | TypedIRValue::Float(_) = value {
                                    known_constants.insert(name.clone(), value.clone());
                                }
                            } else if is_already_constant {
                                // Even if it didn't fully fold, if it's already a raw constant number, track it
                                if let TypedIRValue::Int(_) | TypedIRValue::Float(_) = value {
                                    known_constants.insert(name.clone(), value.clone());
                                }
                            }
                        }
                        SemanticInstruction::Assign { target, value } => {
                            let resolved_value =
                                Self::resolve_and_fold_value(value, &known_constants);
                            if let Some(folded) = resolved_value {
                                // Only fold if the value actually changed
                                if *value != folded {
                                    *value = folded.clone();
                                    self.stats.folded_constants += 1;
                                }

                                if let TypedIRValue::Int(_) | TypedIRValue::Float(_) = value {
                                    known_constants.insert(target.clone(), value.clone());
                                }
                            } else {
                                // If it's reassigned to a non-constant, remove it from tracking
                                known_constants.remove(target);
                            }
                        }
                        SemanticInstruction::Print { value } => {
                            if let Some(folded) =
                                Self::resolve_and_fold_value(value, &known_constants)
                            {
                                if *value != folded {
                                    *value = folded;
                                    self.stats.folded_constants += 1;
                                }
                            }
                        }
                        _ => {}
                    }
                }
            }
        }
    }

    fn resolve_and_fold_value(
        value: &TypedIRValue,
        constants: &HashMap<String, TypedIRValue>,
    ) -> Option<TypedIRValue> {
        match value {
            // If the value is a variable reference, look up its text name in our map
            TypedIRValue::Variable(name, _) => constants.get(name).cloned(),

            // If it's a binary operation, recursively check both the left and right sides first!
            TypedIRValue::BinaryOp {
                op,
                left,
                right,
                result_type,
            } => {
                // Recursively resolve the left side (could be a number, a variable, or another nested operation)
                let final_left =
                    Self::resolve_and_fold_value(left, constants).unwrap_or(*left.clone());
                let final_right =
                    Self::resolve_and_fold_value(right, constants).unwrap_or(*right.clone());

                // Now perform the math check exactly like your original logic
                if let (Some(l), Some(r)) =
                    (final_left.as_constant_f64(), final_right.as_constant_f64())
                {
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
                        Type::Float => Some(TypedIRValue::Float(result)),
                        Type::Int => Some(TypedIRValue::Int(result as i64)),
                        _ => Some(TypedIRValue::Float(result)),
                    }
                } else {
                    None
                }
            }
            // If it's already a flat constant integer or float, return None
            // (no folding needed - it's already folded)
            TypedIRValue::Int(_) | TypedIRValue::Float(_) => None,
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
                for instr in &func.blocks[i].instructions {
                    match instr {
                        SemanticInstruction::Spawn { entry_block } => {
                            if let Some(idx) = func.blocks.iter().position(|b| b.id == *entry_block)
                            {
                                if !reachable[idx] {
                                    reachable[idx] = true;
                                    changed = true;
                                }
                            }
                        }
                        SemanticInstruction::Fork { blocks, join_block } => {
                            for target in blocks {
                                if let Some(idx) = func.blocks.iter().position(|b| b.id == *target)
                                {
                                    if !reachable[idx] {
                                        reachable[idx] = true;
                                        changed = true;
                                    }
                                }
                            }
                            if let Some(idx) = func.blocks.iter().position(|b| b.id == *join_block)
                            {
                                if !reachable[idx] {
                                    reachable[idx] = true;
                                    changed = true;
                                }
                            }
                        }
                        SemanticInstruction::Jump { block: target } => {
                            if let Some(idx) = func.blocks.iter().position(|b| b.id == *target) {
                                if !reachable[idx] {
                                    reachable[idx] = true;
                                    changed = true;
                                }
                            }
                        }
                        SemanticInstruction::IteratorNext {
                            body_block,
                            exit_block,
                            ..
                        } => {
                            if let Some(idx) = func.blocks.iter().position(|b| b.id == *body_block)
                            {
                                if !reachable[idx] {
                                    reachable[idx] = true;
                                    changed = true;
                                }
                            }
                            if let Some(idx) = func.blocks.iter().position(|b| b.id == *exit_block)
                            {
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
                            if let Some(idx) = func.blocks.iter().position(|b| b.id == *then_block)
                            {
                                if !reachable[idx] {
                                    reachable[idx] = true;
                                    changed = true;
                                }
                            }
                            if let Some(idx) = func.blocks.iter().position(|b| b.id == *else_block)
                            {
                                if !reachable[idx] {
                                    reachable[idx] = true;
                                    changed = true;
                                }
                            }
                        }
                        SemanticInstruction::Switch {
                            cases,
                            default_block,
                            ..
                        } => {
                            for (_, tgt) in cases {
                                if let Some(idx) = func.blocks.iter().position(|b| b.id == *tgt) {
                                    if !reachable[idx] {
                                        reachable[idx] = true;
                                        changed = true;
                                    }
                                }
                            }
                            if let Some(def) = default_block {
                                if let Some(idx) = func.blocks.iter().position(|b| b.id == *def) {
                                    if !reachable[idx] {
                                        reachable[idx] = true;
                                        changed = true;
                                    }
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
