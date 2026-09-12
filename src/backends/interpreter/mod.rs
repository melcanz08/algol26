// src/backends/interpreter/mod.rs
//
// Tree-walking interpreter over SemanticProgram.
//
// Supported:
//   - the full instruction set except FFI and channel operations
//   - Option, Result, and pattern matching with payload bindings
//   - user-defined function calls (via eval_call) with frame
//     save/restore, including nested calls in expressions
//
// Not supported:
//   - parallel execution. Spawn and Fork run sequentially; the
//     interpreter does not create OS threads.
//   - foreign function calls.
//   - channel send/receive (no-op instructions).
use crate::ir::semantic_ir::{
    Instruction, SemanticFunction, SemanticProgram, Terminator, TypedIRValue,
};
use std::collections::HashMap;

mod runtime;
mod pattern;
mod eval;
pub use runtime::RuntimeValue;

pub struct Interpreter {
    pub(super) variables: HashMap<String, RuntimeValue>,
    pub(super) output: Vec<String>,
    pub(super) program: SemanticProgram,
    pub(super) return_value: Option<RuntimeValue>,
}

impl Interpreter {
    pub fn new(program: SemanticProgram) -> Self {
        Self {
            variables: HashMap::new(),
            output: Vec::new(),
            program,
            return_value: None,
        }
    }

    pub fn run(&mut self) -> Result<String, String> {
        let main_func = self
            .program
            .functions
            .iter()
            .find(|f| f.name == "main")
            .cloned()
            .ok_or("No main function found")?;

        self.execute_function(&main_func)?;
        Ok(self.output.join("\n"))
    }

    pub(super) fn execute_function(&mut self, func: &SemanticFunction) -> Result<(), String> {
        let mut current = func.entry_block;
        let mut iterations = 0;

        loop {
            if iterations > 10000 {
                return Err("Infinite loop detected".to_string());
            }
            iterations += 1;

            let block = func
                .blocks
                .iter()
                .find(|b| b.id == current)
                .ok_or_else(|| format!("Block {} not found", current))?
                .clone();

            for instr in &block.instructions {
                self.execute_instruction(instr)?;
            }

            match &block.terminator {
                Some(Terminator::Return { value, .. }) => {
                    if let Some(v) = value {
                        self.return_value = Some(self.eval_value(v));
                    }
                    return Ok(());
                }
                Some(Terminator::Jump { block: target }) => {
                    current = *target;
                }
                Some(Terminator::Branch {
                    condition,
                    then_block,
                    else_block,
                }) => {
                    let cond = self.eval_value(condition).as_bool();
                    current = if cond { *then_block } else { *else_block };
                }
                Some(Terminator::IteratorNext {
                    iterator,
                    target,
                    body_block,
                    exit_block,
                }) => { 
                    let idx_key = format!("{}_idx", iterator);
                    let current_idx = match self.variables.get(&idx_key) {
                        Some(RuntimeValue::Int(i)) => *i as usize,
                        _ => 0,
                    };

                    let iterable = self
                        .variables
                        .get(iterator)
                        .cloned()
                        .unwrap_or(RuntimeValue::Void);

                    if let RuntimeValue::List(list) = iterable {
                        if current_idx < list.len() {
                            let value = list[current_idx].clone();
                            self.variables.insert(target.clone(), value);
                            self.variables
                                .insert(idx_key, RuntimeValue::Int((current_idx + 1) as i64));
                            current = *body_block;
                        } else {
                            current = *exit_block;
                        }
                    } else {
                        current = *exit_block;
                    }
                }
                Some(Terminator::Spawn { entry_block }) => {
                    current = *entry_block;
                }
                Some(Terminator::Fork { blocks, join_block }) => {
                    if let Some(first) = blocks.first() {
                        current = *first;
                    } else {
                        current = *join_block;
                    }
                }
                Some(Terminator::Switch {
                    value,
                    cases,
                    default_block,
                }) => {
                    let val = self.eval_value(value);
                    let mut matched = false;

                    for (pattern, target) in cases {
                        if let Some(bindings) = self.try_pattern_match(pattern, &val) {
                            // Bind the pattern's payload(s) before jumping.
                            for (name, bound_val) in bindings {
                                self.variables.insert(name, bound_val);
                            }
                            current = *target;
                            matched = true;
                            break;
                        }
                    }

                    if !matched {
                        if let Some(default) = default_block {
                            current = *default;
                        } else {
                            return Err("No matching case".to_string());
                        }
                    }
                }
                None => return Ok(()),
            }
        }
    }
    fn execute_instruction(&mut self, instr: &Instruction) -> Result<(), String> {
        match instr {
            Instruction::Declare { name, value, .. } => {
                let val = self.eval_value(value);
                self.variables.insert(name.clone(), val);
            }
            Instruction::Assign { target, value } => {
                let val = self.eval_value(value);
                self.variables.insert(target.clone(), val);
            }
            Instruction::Print { value } => {
                let val = self.eval_value(value);
                self.output.push(val.display());
            }
            Instruction::Call { func, args, result } => {
                let val = self.eval_call(func, args);
                if let Some(res_name) = result {
                    self.variables.insert(res_name.clone(), val);
                }
            }
            Instruction::ArrayAssign {
                array,
                index,
                value,
            } => {
                let arr_name = match array.as_ref() {
                    TypedIRValue::Variable(name, _) => name.clone(),
                    _ => return Ok(()),
                };

                let idx = self.eval_value(index);
                let val = self.eval_value(value);

                let idx_usize = match idx {
                    RuntimeValue::Int(i) => i as usize,
                    RuntimeValue::Float(f) => f as usize,
                    _ => 0,
                };

                if let Some(RuntimeValue::List(list)) = self.variables.get(&arr_name).cloned() {
                    let mut new_list = list;
                    if idx_usize < new_list.len() {
                        new_list[idx_usize] = val;
                        self.variables
                            .insert(arr_name, RuntimeValue::List(new_list));
                    }
                }
            }
            Instruction::IteratorInit { iterator, iterable } => {
                let val = self.eval_value(iterable);
                self.variables.insert(iterator.clone(), val);
                self.variables.insert(
                    format!("{}_idx", iterator),
                    RuntimeValue::Int(0),
                );
            }
            _=> {}
        }
        Ok(())
    }
}
