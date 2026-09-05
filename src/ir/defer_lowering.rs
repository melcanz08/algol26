use crate::ir::semantic_ir::{SemanticFunction, SemanticInstruction, SemanticProgram};

pub struct DeferLoweringPass {
    defer_stacks: Vec<Vec<usize>>, // Stack of scopes, each containing cleanup block IDs
}

impl DeferLoweringPass {
    #[allow(dead_code)]
    pub fn new() -> Self {
        Self {
            defer_stacks: vec![Vec::new()],
        }
    }

    pub fn run(&mut self, program: &mut SemanticProgram) {
        for func in &mut program.functions {
            self.lower_function(func);
        }
    }

    #[allow(dead_code)]
    fn push_scope(&mut self) {
        self.defer_stacks.push(Vec::new());
    }

    #[allow(dead_code)]
    fn pop_scope(&mut self) -> Vec<usize> {
        self.defer_stacks.pop().unwrap_or_default()
    }

    fn add_defer(&mut self, cleanup_block: usize) {
        if let Some(scope) = self.defer_stacks.last_mut() {
            scope.push(cleanup_block);
        }
    }

    fn collect_active_defers(&self) -> Vec<usize> {
        let mut active = Vec::new();
        // Traverse scopes outward (LIFO order overall, inner scopes first)
        for scope in self.defer_stacks.iter().rev() {
            for &cleanup_id in scope.iter().rev() {
                active.push(cleanup_id);
            }
        }
        active
    }

    fn lower_function(&mut self, func: &mut SemanticFunction) {
        self.defer_stacks = vec![Vec::new()];
        let mut new_blocks = Vec::new();

        for mut block in std::mem::take(&mut func.blocks) {
            let mut new_instructions = Vec::new();

            for instruction in block.instructions {
                match instruction {
                    SemanticInstruction::Defer { cleanup_block } => {
                        self.add_defer(cleanup_block);
                    }
                    SemanticInstruction::Return { value, type_ } => {
                        // Intercept return and inject LIFO cleanup chain
                        let defers = self.collect_active_defers();
                        if defers.is_empty() {
                            new_instructions.push(SemanticInstruction::Return { value, type_ });
                        } else {
                            // Chain jumps through each cleanup block
                            let current_target = defers[0];
                            // Note: In a full CFG builder, you link the sequence:
                            // block -> cleanup_1 -> cleanup_2 -> actual return
                            new_instructions.push(SemanticInstruction::Jump {
                                block: current_target,
                            });
                            // Append instruction sequence to wire cleanups back to return
                        }
                    }
                    other => {
                        new_instructions.push(other);
                    }
                }
            }
            block.instructions = new_instructions;
            new_blocks.push(block);
        }
        func.blocks = new_blocks;
    }
}
