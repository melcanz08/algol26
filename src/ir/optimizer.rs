// src/ir/optimizer.rs
//
// Optimization passes run in this order, per function:
//   1. remove_unreachable_blocks
//   2. constant_folding
//   3. constant_propagation — skipped when the function's CFG has a
//      cycle, because the pass is not loop-aware and would propagate
//      a value that changes across iterations
//   4. dead_code_elimination — keeps every mutable declaration and
//      seeds liveness from Declare/Assign/IteratorInit initializers
//      so nested uses (e.g. inside ArrayAccess) are counted
//   5. simplify_branches — folds `Branch(true/false, ...)` into a
//      plain Jump
//   6. remove_unreachable_blocks again, to clean up after the above

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

impl Default for Optimizer {
    fn default() -> Self {
        Self::new()
    }
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

                // Skip folding when both operands are `Int` and either is
                // outside the range where f64 represents every integer
                // exactly (2^53). Integer arithmetic above that bound
                // loses precision when routed through f64, producing a
                // wrong compile-time constant. Leaving the node alone
                // is safe — the runtime computes the correct i64 result.
                if let (TypedIRValue::Int(l), TypedIRValue::Int(r)) =
                    (left.as_ref(), right.as_ref())
                {
                    const SAFE_BOUND: i64 = 1i64 << 53;
                    // `checked_abs` returns `None` for `i64::MIN`, which
                    // is out of the safe range by definition.
                    let l_big = l.checked_abs().map_or(true, |a| a > SAFE_BOUND);
                    let r_big = r.checked_abs().map_or(true, |a| a > SAFE_BOUND);
                    if l_big || r_big {
                        return false;
                    }
                }

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
            // Constants defined in one basic block are NOT visible to
            // sibling or join blocks. Before the fix, the pass walked
            // blocks in storage order and let a constant defined in
            // one branch leak into the join, miscompiling programs
            // like:
            //
            //     var x := 5.0
            //     if cond
            //         x := 10.0
            //     y := x    -- wrongly folded to y := 10.0
            //
            // Clearing per block keeps the pass trivially correct
            // without needing a dominator tree. Cross-block
            // propagation is a future improvement that requires
            // proper dominance analysis.
            constants.clear();

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
                    Instruction::Declare { name, value, .. } => {
                        // Variables referenced in an initializer are used.
                        // (Don't count the declared name itself.)
                        let mut deps = HashSet::new();
                        collect_variables_from_value(value, &mut deps);
                        deps.remove(name);
                        for d in deps { used_variables.insert(d); }
                    }
                    Instruction::Assign { target, value } => {
                        let mut deps = HashSet::new();
                        collect_variables_from_value(value, &mut deps);
                        deps.remove(target);
                        for d in deps { used_variables.insert(d); }
                    }
                    Instruction::IteratorInit { iterable, .. } => {
                        collect_variables_from_value(iterable, &mut used_variables);
                    }
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
                        // Mutable variables are always kept — they may
                        // participate in loop-carried state, and this
                        // pass is not loop-aware.
                        //
                        // Immutable declarations are removed when the
                        // variable is unused. This is safe *only* because
                        // the builder emits any initializer side effects
                        // (e.g. a Call) as a separate instruction
                        // immediately before the Declare; DCE never
                        // removes a Call. If the builder ever inlines
                        // side effects into Declare values, this pass
                        // must be revisited. See
                        // `dce_preserves_call_side_effects_even_when_result_unused`.
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::common::types::Type;
    use crate::ir::semantic_ir::{
        Instruction, SemanticBlock, SemanticFunction, SemanticProgram, Terminator, TypedIRValue,
    };

    fn block(id: usize, instrs: Vec<Instruction>, term: Terminator) -> SemanticBlock {
        SemanticBlock {
            id,
            instructions: instrs,
            terminator: Some(term),
        }
    }

    #[test]
    fn constant_propagation_does_not_leak_across_branches() {
        // Program shape (matching the IR the builder emits):
        //
        //   entry: Declare(y, 0.0); Declare(x, 5.0);
        //          Branch(cond, then, else)
        //   then:  Assign(x, 10.0); Jump merge
        //   else:  Jump merge
        //   merge: Assign(y, Var(x)); Return(y)
        //
        // With the bug, the pass sees `then` (block 1) before `merge`
        // (block 3) in storage order, records x = 10.0, and rewrites
        // the merge's `Assign(y, Var(x))` to `Assign(y, 10.0)`. The
        // correct result at runtime when `cond` is false is 5.0.
        let mut program = SemanticProgram::new();
        let entry = program.new_block_id();
        let then_id = program.new_block_id();
        let else_id = program.new_block_id();
        let merge_id = program.new_block_id();

        let func = SemanticFunction {
            name: "f".to_string(),
            params: vec![("cond".to_string(), Type::Bool)],
            return_type: Type::Float,
            blocks: vec![
                block(
                    entry,
                    vec![
                        Instruction::Declare {
                            name: "y".to_string(),
                            mutable: true,
                            type_: Type::Float,
                            value: TypedIRValue::Float(0.0),
                        },
                        Instruction::Declare {
                            name: "x".to_string(),
                            mutable: true,
                            type_: Type::Float,
                            value: TypedIRValue::Float(5.0),
                        },
                    ],
                    Terminator::Branch {
                        condition: TypedIRValue::Variable("cond".to_string(), Type::Bool),
                        then_block: then_id,
                        else_block: else_id,
                    },
                ),
                block(
                    then_id,
                    vec![Instruction::Assign {
                        target: "x".to_string(),
                        value: TypedIRValue::Float(10.0),
                    }],
                    Terminator::Jump { block: merge_id },
                ),
                block(
                    else_id,
                    vec![],
                    Terminator::Jump { block: merge_id },
                ),
                block(
                    merge_id,
                    vec![Instruction::Assign {
                        target: "y".to_string(),
                        value: TypedIRValue::Variable("x".to_string(), Type::Float),
                    }],
                    Terminator::Return {
                        value: Some(TypedIRValue::Variable("y".to_string(), Type::Float)),
                        type_: Type::Float,
                    },
                ),
            ],
            entry_block: entry,
            is_extern: false,
        };

        program.functions.push(func);

        let mut opt = Optimizer::new();
        opt.optimize(&mut program);

        let merge = program.functions[0]
            .blocks
            .iter()
            .find(|b| b.id == merge_id)
            .expect("merge block was removed");

        let assign = merge
            .instructions
            .iter()
            .find_map(|i| match i {
                Instruction::Assign { target, value } if target == "y" => Some(value),
                _ => None,
            })
            .expect("Assign to `y` was removed");

        assert!(
            matches!(assign, TypedIRValue::Variable(name, _) if name == "x"),
            "constant propagation leaked across branches: got {:?}",
            assign
        );
    }
        #[test]
    fn folding_skips_large_int_literals() {
        // A BinaryOp over two `Int` operands whose values exceed 2^53
        // must not be folded to a Float. The pass should leave the
        // node alone; the runtime computes the correct i64 result.
        use crate::common::types::Type;
        use crate::ir::semantic_ir::{
            Instruction, SemanticBinOp, SemanticBlock, SemanticFunction, SemanticProgram,
            Terminator, TypedIRValue,
        };

        let mut program = SemanticProgram::new();
        let entry = program.new_block_id();

        // 2^53 + 1 — not exactly representable in f64.
        let big: i64 = 9_007_199_254_740_993;

        let func = SemanticFunction {
            name: "f".to_string(),
            params: vec![],
            return_type: Type::Int,
            blocks: vec![SemanticBlock {
                id: entry,
                instructions: vec![Instruction::Declare {
                    name: "r".to_string(),
                    mutable: false,
                    type_: Type::Int,
                    value: TypedIRValue::BinaryOp {
                        op: SemanticBinOp::Add,
                        left: Box::new(TypedIRValue::Int(big)),
                        right: Box::new(TypedIRValue::Int(1)),
                        result_type: Type::Int,
                    },
                }],
                terminator: Some(Terminator::Return {
                    value: Some(TypedIRValue::Variable("r".to_string(), Type::Int)),
                    type_: Type::Int,
                }),
            }],
            entry_block: entry,
            is_extern: false,
        };
        program.functions.push(func);

        let mut opt = Optimizer::new();
        opt.optimize(&mut program);

        let declare = program.functions[0].blocks[0]
            .instructions
            .iter()
            .find_map(|i| match i {
                Instruction::Declare { name, value, .. } if name == "r" => Some(value),
                _ => None,
            })
            .expect("Declare for `r` not found");

        assert!(
            matches!(declare, TypedIRValue::BinaryOp { .. }),
            "large Int arithmetic must not be folded, got: {:?}",
            declare
        );
    }

    #[test]
    fn folding_still_works_for_small_ints() {
        // Sanity check: folding is not disabled wholesale — small
        // ints still fold.
        use crate::common::types::Type;
        use crate::ir::semantic_ir::{
            Instruction, SemanticBinOp, SemanticBlock, SemanticFunction, SemanticProgram,
            Terminator, TypedIRValue,
        };

        let mut program = SemanticProgram::new();
        let entry = program.new_block_id();

        let func = SemanticFunction {
            name: "f".to_string(),
            params: vec![],
            return_type: Type::Int,
            blocks: vec![SemanticBlock {
                id: entry,
                instructions: vec![Instruction::Declare {
                    name: "r".to_string(),
                    mutable: false,
                    type_: Type::Int,
                    value: TypedIRValue::BinaryOp {
                        op: SemanticBinOp::Add,
                        left: Box::new(TypedIRValue::Int(2)),
                        right: Box::new(TypedIRValue::Int(3)),
                        result_type: Type::Int,
                    },
                }],
                terminator: Some(Terminator::Return {
                    value: Some(TypedIRValue::Variable("r".to_string(), Type::Int)),
                    type_: Type::Int,
                }),
            }],
            entry_block: entry,
            is_extern: false,
        };
        program.functions.push(func);

        let mut opt = Optimizer::new();
        opt.optimize(&mut program);

        let declare = program.functions[0].blocks[0]
            .instructions
            .iter()
            .find_map(|i| match i {
                Instruction::Declare { name, value, .. } if name == "r" => Some(value),
                _ => None,
            })
            .expect("Declare for `r` not found");

        assert!(
            matches!(declare, TypedIRValue::Int(5)),
            "small Int arithmetic should still fold to a literal, got: {:?}",
            declare
        );
    }

        #[test]
    fn dce_preserves_call_side_effects_even_when_result_unused() {
        // The IR builder emits a `Call` instruction with
        // `result: Some(name)` immediately before a `Declare` for
        // statements like `val unused := Math.abs(-1.0)`. DCE removes
        // the unused Declare but must keep the Call — otherwise the
        // call's side effect would disappear. This test pins that
        // coupling.
        use crate::common::types::Type;
        use crate::ir::semantic_ir::{
            Instruction, SemanticBlock, SemanticFunction, SemanticProgram, Terminator,
            TypedIRValue,
        };

        let mut program = SemanticProgram::new();
        let entry = program.new_block_id();

        let func = SemanticFunction {
            name: "f".to_string(),
            params: vec![],
            return_type: Type::Void,
            blocks: vec![SemanticBlock {
                id: entry,
                instructions: vec![
                    Instruction::Call {
                        func: "Math.abs".to_string(),
                        args: vec![TypedIRValue::Float(-1.0)],
                        result: Some("unused".to_string()),
                    },
                    Instruction::Declare {
                        name: "unused".to_string(),
                        mutable: false,
                        type_: Type::Float,
                        value: TypedIRValue::Void,
                    },
                ],
                terminator: Some(Terminator::Return {
                    value: None,
                    type_: Type::Void,
                }),
            }],
            entry_block: entry,
            is_extern: false,
        };
        program.functions.push(func);

        let mut opt = Optimizer::new();
        opt.optimize(&mut program);

        let block = &program.functions[0].blocks[0];

        let call_still_there = block.instructions.iter().any(|i| {
            matches!(i, Instruction::Call { result: Some(name), .. } if name == "unused")
        });
        let declare_removed = !block
            .instructions
            .iter()
            .any(|i| matches!(i, Instruction::Declare { name, .. } if name == "unused"));

        assert!(
            call_still_there,
            "DCE removed the Call instruction; side effects were lost"
        );
        assert!(
            declare_removed,
            "DCE should have removed the unused immutable Declare"
        );
    }
}