#![allow(dead_code)]

// src/ir/optimizer.rs - HARDENED
// Complete optimizer with all passes properly integrated

use crate::ir::semantic_ir::{
    Instruction, SemanticBinOp, SemanticProgram, Terminator, TypedIRValue,
};
use std::collections::{HashMap, HashSet};

pub struct Optimizer {
    stats: OptimizationStats,
}

#[derive(Debug, Default)]
pub struct OptimizationStats {
    pub removed_blocks: usize,
    pub folded_constants: usize,
    pub eliminated_dead_code: usize,
    pub propagated_constants: usize,
    pub simplified_branches: usize,
}

impl Optimizer {
    fn has_cfg_cycle(func: &crate::ir::semantic_ir::SemanticFunction) -> bool {
        use std::collections::HashSet;

        fn visit(
            func: &crate::ir::semantic_ir::SemanticFunction,
            node: usize,
            visiting: &mut HashSet<usize>,
            visited: &mut HashSet<usize>,
        ) -> bool {
            if visiting.contains(&node) {
                return true;
            }
            if visited.contains(&node) {
                return false;
            }
            visiting.insert(node);
            if let Some(block) = func.blocks.iter().find(|b| b.id == node) {
                if let Some(term) = &block.terminator {
                    for successor in term.successors() {
                        if visit(func, successor, visiting, visited) {
                            return true;
                        }
                    }
                }
            }
            visiting.remove(&node);
            visited.insert(node);
            false
        }

        visit(
            func,
            func.entry_block,
            &mut HashSet::new(),
            &mut HashSet::new(),
        )
    }

    pub fn new() -> Self {
        Self {
            stats: OptimizationStats::default(),
        }
    }

    pub fn optimize(&mut self, program: &mut SemanticProgram) {
        // Run optimization passes in order
        for func in &mut program.functions {
            // Pass 1: Remove unreachable blocks
            self.remove_unreachable_blocks(func);

            // Pass 2: Constant folding
            self.constant_folding(func);

            // Pass 3: Constant propagation (SKIPPED for cyclic CFGs)
            // ALGOL26: Constant propagation is NOT loop-aware yet
            // Skip it for functions with cycles to avoid incorrect propagation
            if !Self::has_cfg_cycle(func) {
                self.constant_propagation(func);
            }

            // Pass 4: Dead code elimination
            self.dead_code_elimination(func);

            // Pass 5: Branch simplification
            self.simplify_branches(func);

            // Pass 6: Remove unreachable blocks again (after other optimizations)
            self.remove_unreachable_blocks(func);
        }
    }

    fn remove_unreachable_blocks(&mut self, func: &mut crate::ir::semantic_ir::SemanticFunction) {
        let mut reachable = HashSet::new();
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

        let before = func.blocks.len();
        func.blocks.retain(|b| reachable.contains(&b.id));
        self.stats.removed_blocks += before - func.blocks.len();
    }

    fn constant_folding(&mut self, func: &mut crate::ir::semantic_ir::SemanticFunction) {
        for block in &mut func.blocks {
            for instr in &mut block.instructions {
                match instr {
                    Instruction::Declare { value, .. } => {
                        if self.fold_value(value) {
                            self.stats.folded_constants += 1;
                        }
                    }
                    Instruction::Assign { value, .. } => {
                        if self.fold_value(value) {
                            self.stats.folded_constants += 1;
                        }
                    }
                    Instruction::Print { value } => {
                        if self.fold_value(value) {
                            self.stats.folded_constants += 1;
                        }
                    }
                    Instruction::ArrayAssign {
                        array,
                        index,
                        value,
                    } => {
                        if self.fold_value(array) {
                            self.stats.folded_constants += 1;
                        }
                        if self.fold_value(index) {
                            self.stats.folded_constants += 1;
                        }
                        if self.fold_value(value) {
                            self.stats.folded_constants += 1;
                        }
                    }
                    Instruction::Call { args, .. } => {
                        for arg in args {
                            if self.fold_value(arg) {
                                self.stats.folded_constants += 1;
                            }
                        }
                    }
                    Instruction::Send { value, .. } => {
                        if self.fold_value(value) {
                            self.stats.folded_constants += 1;
                        }
                    }
                    _ => {}
                }
            }

            if let Some(term) = &mut block.terminator {
                match term {
                    Terminator::Return { value: Some(v), .. } => {
                        if self.fold_value(v) {
                            self.stats.folded_constants += 1;
                        }
                    }
                    Terminator::Branch { condition, .. } => {
                        if self.fold_value(condition) {
                            self.stats.folded_constants += 1;
                        }
                    }
                    Terminator::Switch { value, .. } => {
                        if self.fold_value(value) {
                            self.stats.folded_constants += 1;
                        }
                    }
                    _ => {}
                }
            }
        }
    }

    fn fold_value(&mut self, value: &mut TypedIRValue) -> bool {
        match value {
            TypedIRValue::BinaryOp {
                op,
                left,
                right,
                result_type,
            } => {
                // Recursively fold operands first
                self.fold_value(left);
                self.fold_value(right);

                // Try to fold if both are constants
                if let (Some(l), Some(r)) = (left.as_constant_f64(), right.as_constant_f64()) {
                    let result = match op {
                        SemanticBinOp::Add => Some(l + r),
                        SemanticBinOp::Subtract => Some(l - r),
                        SemanticBinOp::Multiply => Some(l * r),
                        SemanticBinOp::Divide => {
                            if r != 0.0 {
                                Some(l / r)
                            } else {
                                // Division by zero - don't fold
                                None
                            }
                        }
                        SemanticBinOp::Greater => Some(if l > r { 1.0 } else { 0.0 }),
                        SemanticBinOp::Less => Some(if l < r { 1.0 } else { 0.0 }),
                        SemanticBinOp::GreaterEqual => Some(if l >= r { 1.0 } else { 0.0 }),
                        SemanticBinOp::LessEqual => Some(if l <= r { 1.0 } else { 0.0 }),
                        SemanticBinOp::Equal => Some(if l == r { 1.0 } else { 0.0 }),
                        SemanticBinOp::NotEqual => Some(if l != r { 1.0 } else { 0.0 }),
                        _ => None,
                    };

                    if let Some(res) = result {
                        // Determine result type
                        let new_type = if matches!(
                            op,
                            SemanticBinOp::Greater
                                | SemanticBinOp::Less
                                | SemanticBinOp::GreaterEqual
                                | SemanticBinOp::LessEqual
                                | SemanticBinOp::Equal
                                | SemanticBinOp::NotEqual
                        ) {
                            crate::common::types::Type::Bool
                        } else if *result_type == crate::common::types::Type::Int {
                            crate::common::types::Type::Int
                        } else {
                            crate::common::types::Type::Float
                        };

                        // Create folded value
                        *value = if new_type == crate::common::types::Type::Bool {
                            TypedIRValue::Bool(res != 0.0)
                        } else if new_type == crate::common::types::Type::Int {
                            TypedIRValue::Int(res as i64)
                        } else {
                            TypedIRValue::Float(res)
                        };

                        return true;
                    }
                }
                false
            }
            TypedIRValue::Cast {
                value: inner,
                target_type,
            } => {
                if self.fold_value(inner) {
                    // If inner is now constant, fold the cast
                    if let Some(inner_const) = inner.as_constant_f64() {
                        *value = match target_type {
                            crate::common::types::Type::Int => {
                                TypedIRValue::Int(inner_const as i64)
                            }
                            crate::common::types::Type::Float => TypedIRValue::Float(inner_const),
                            crate::common::types::Type::String => {
                                TypedIRValue::String(inner_const.to_string())
                            }
                            crate::common::types::Type::Bool => {
                                TypedIRValue::Bool(inner_const != 0.0)
                            }
                            _ => return false,
                        };
                        return true;
                    }
                }
                false
            }
            _ => false,
        }
    }

    fn constant_propagation(&mut self, func: &mut crate::ir::semantic_ir::SemanticFunction) {
        let mut constants: HashMap<String, TypedIRValue> = HashMap::new();

        for block in &mut func.blocks {
            for instr in &mut block.instructions {
                match instr {
                    Instruction::Declare { name, value, .. } => {
                        if let TypedIRValue::Int(_) = value {
                            constants.insert(name.clone(), value.clone());
                        } else if let TypedIRValue::Float(_) = value {
                            constants.insert(name.clone(), value.clone());
                        } else if let TypedIRValue::Bool(_) = value {
                            constants.insert(name.clone(), value.clone());
                        } else if let TypedIRValue::String(_) = value {
                            constants.insert(name.clone(), value.clone());
                        }
                    }
                    Instruction::Assign { target, value } => {
                        // Replace variable with constant if known
                        if let TypedIRValue::Variable(name, _) = value {
                            if let Some(constant) = constants.get(name) {
                                *value = constant.clone();
                                self.stats.propagated_constants += 1;
                            }
                        }

                        // Update constant map
                        if let TypedIRValue::Int(_) = value {
                            constants.insert(target.clone(), value.clone());
                        } else if let TypedIRValue::Float(_) = value {
                            constants.insert(target.clone(), value.clone());
                        } else {
                            constants.remove(target);
                        }
                    }
                    _ => {}
                }
            }
        }
    }

    fn dead_code_elimination(&mut self, func: &mut crate::ir::semantic_ir::SemanticFunction) {
        // Pass 1: Build declaration dependency graph
        let mut dependencies: HashMap<String, HashSet<String>> = HashMap::new();

        for block in &func.blocks {
            for instr in &block.instructions {
                match instr {
                    Instruction::Declare { name, value, .. } => {
                        let mut deps = HashSet::new();
                        collect_variables_from_value(value, &mut deps);
                        deps.remove(name);
                        dependencies.insert(name.clone(), deps);
                    }
                    Instruction::Assign { target, value } => {
                        let mut deps = HashSet::new();
                        collect_variables_from_value(value, &mut deps);
                        deps.remove(target);
                        dependencies.insert(target.clone(), deps);
                    }
                    _ => {}
                }
            }
        }

        // Pass 2: Seed liveness from externally observable uses
        let mut used_variables: HashSet<String> = HashSet::new();

        for block in &func.blocks {
            for instr in &block.instructions {
                match instr {
                    Instruction::Print { value } => {
                        collect_variables_from_value(value, &mut used_variables);
                    }
                    Instruction::Call { args, .. } => {
                        for arg in args {
                            collect_variables_from_value(arg, &mut used_variables);
                        }
                    }
                    Instruction::ArrayAssign {
                        array,
                        index,
                        value,
                    } => {
                        collect_variables_from_value(array, &mut used_variables);
                        collect_variables_from_value(index, &mut used_variables);
                        collect_variables_from_value(value, &mut used_variables);
                    }
                    Instruction::Send { value, .. } => {
                        collect_variables_from_value(value, &mut used_variables);
                    }
                    Instruction::Receive { target, .. } => {
                        used_variables.insert(target.clone());
                    }
                    _ => {}
                }
            }

            if let Some(term) = &block.terminator {
                match term {
                    Terminator::Return {
                        value: Some(value), ..
                    } => {
                        collect_variables_from_value(value, &mut used_variables);
                    }
                    Terminator::Branch { condition, .. } => {
                        collect_variables_from_value(condition, &mut used_variables);
                    }
                    Terminator::Switch { value, .. } => {
                        collect_variables_from_value(value, &mut used_variables);
                    }
                    _ => {}
                }
            }
        }

        // Pass 3: Compute transitive closure
        let mut worklist: Vec<String> = used_variables.iter().cloned().collect();
        while let Some(name) = worklist.pop() {
            if let Some(deps) = dependencies.get(&name) {
                for dep in deps {
                    if used_variables.insert(dep.clone()) {
                        worklist.push(dep.clone());
                    }
                }
            }
        }

        // Pass 4: Conservative removal
        // ALGOL26: NEVER remove mutable declarations or loop body instructions
        // Only remove immutable declarations that are truly unused
        let before = func
            .blocks
            .iter()
            .map(|b| b.instructions.len())
            .sum::<usize>();
        for block in &mut func.blocks {
            block.instructions.retain(|instr| {
                match instr {
                    Instruction::Declare { name, mutable, .. } => {
                        // ALGOL26: Mutable variables are ALWAYS kept
                        // (they may participate in loop-carried state)
                        if *mutable {
                            true
                        } else {
                            used_variables.contains(name)
                        }
                    }
                    _ => true, // Never remove non-Declare instructions
                }
            });
        }
        let after = func
            .blocks
            .iter()
            .map(|b| b.instructions.len())
            .sum::<usize>();
        self.stats.eliminated_dead_code += before - after;
    }

    fn simplify_branches(&mut self, func: &mut crate::ir::semantic_ir::SemanticFunction) {
        for block in &mut func.blocks {
            if let Some(Terminator::Branch {
                condition,
                then_block,
                else_block,
            }) = &block.terminator
            {
                match condition {
                    TypedIRValue::Bool(true) => {
                        // Always take then branch
                        block.terminator = Some(Terminator::Jump { block: *then_block });
                        self.stats.simplified_branches += 1;
                    }
                    TypedIRValue::Bool(false) => {
                        // Always take else branch
                        block.terminator = Some(Terminator::Jump { block: *else_block });
                        self.stats.simplified_branches += 1;
                    }
                    _ => {}
                }
            }
        }
    }

    pub fn stats(&self) -> &OptimizationStats {
        &self.stats
    }
}

fn collect_variables_from_value(value: &TypedIRValue, vars: &mut HashSet<String>) {
    match value {
        TypedIRValue::Variable(name, _) => {
            vars.insert(name.clone());
        }
        TypedIRValue::BinaryOp { left, right, .. } => {
            collect_variables_from_value(left, vars);
            collect_variables_from_value(right, vars);
        }
        TypedIRValue::List(elements, _) => {
            for elem in elements {
                collect_variables_from_value(elem, vars);
            }
        }
        TypedIRValue::ArrayAccess { array, index, .. } => {
            collect_variables_from_value(array, vars);
            collect_variables_from_value(index, vars);
        }
        TypedIRValue::Call { args, .. } => {
            for arg in args {
                collect_variables_from_value(arg, vars);
            }
        }
        TypedIRValue::Cast { value, .. } => {
            collect_variables_from_value(value, vars);
        }
        TypedIRValue::Some(inner) => {
            collect_variables_from_value(inner, vars);
        }
        TypedIRValue::Ok { value, .. } => {
            collect_variables_from_value(value, vars);
        }
        TypedIRValue::Error { value, .. } => {
            collect_variables_from_value(value, vars);
        }
        TypedIRValue::Borrow { expr, .. } => {
            collect_variables_from_value(expr, vars);
        }
        TypedIRValue::MutBorrow { expr, .. } => {
            collect_variables_from_value(expr, vars);
        }
        TypedIRValue::Deref { expr, .. } => {
            collect_variables_from_value(expr, vars);
        }
        TypedIRValue::AddrOf { expr, .. } => {
            collect_variables_from_value(expr, vars);
        }
        _ => {}
    }
}

pub fn optimize(program: &mut SemanticProgram) {
    Optimizer::new().optimize(program);
}
