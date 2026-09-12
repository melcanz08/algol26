// src/semantics/semantic_builder/blocks.rs

use super::*;

impl SemanticIRBuilder {
    pub(super) fn safe_push_instruction(
        &mut self,
        func: &mut SemanticFunction,
        block_id: usize,
        instruction: Instruction,
    ) {
        if let Some(block) = func.blocks.iter_mut().find(|b| b.id == block_id) {
            block.instructions.push(instruction);
        } else {
            self.diagnostics
                .push(format!("block {} not found", block_id));
        }
    }
    pub(super) fn safe_set_terminator(
        &mut self,
        func: &mut SemanticFunction,
        block_id: usize,
        term: Terminator,
    ) {
        if let Some(block) = func.blocks.iter_mut().find(|b| b.id == block_id) {
            block.terminator = Some(term);
        } else {
            self.diagnostics
                .push(format!("block {} not found", block_id));
        }
    }
    pub(super) fn block_is_terminated(&self, func: &SemanticFunction, id: usize) -> bool {
        func.blocks
            .iter()
            .find(|b| b.id == id)
            .map_or(false, |b| Self::is_terminated(b))
    }
    pub(super) fn translate_block_with_result(
        &mut self,
        program: &mut SemanticProgram,
        func: &mut SemanticFunction,
        start_block: usize,
        statements: &[Stmt],
        trailing_expr: Option<&Expr>,
        target: &str,
        target_type: Type,
    ) -> Option<usize> {
        // Translate the statements
        let flow = self.translate_block(program, func, start_block, statements);
        let final_block = match flow {
            FlowResult::Reachable(id) => id,
            FlowResult::Unreachable => return None,
        };

        // If there is a trailing expression, translate it and assign to target
        if let Some(expr) = trailing_expr {
            let value = self.translate_expr(program, func, final_block, expr);
            // Optionally coerce the value to the target type
            let coerced = self.coerce_value(value, &target_type);
            let _ = self.safe_push_instruction(
                func,
                final_block,
                SemanticInstruction::Assign {
                    target: target.to_string(),
                    value: coerced,
                },
            );
        } else {
            // No trailing expression: assign a default value (Void or a default of target_type)
            // For now, we assign Void; this may be insufficient for types that need a value.
            let _ = self.safe_push_instruction(
                func,
                final_block,
                SemanticInstruction::Assign {
                    target: target.to_string(),
                    value: TypedIRValue::Void,
                },
            );
        }

        Some(final_block)
    }
    pub(super) fn allocate_result_var(&mut self, func: &mut SemanticFunction, current_block: usize, type_hint: Type) -> String {
        let name = format!("__result_{}", self.iter_counter);
        self.iter_counter += 1;
        // Declare the variable in the current scope (and in the IR)
        self.declare_var(&name, type_hint.clone(), true);
        let _ = self.safe_push_instruction(
            func,
            current_block,
            SemanticInstruction::Declare {
                name: name.clone(),
                mutable: true,
                type_: type_hint,
                value: TypedIRValue::Void,
            },
        );
        name
    }
}