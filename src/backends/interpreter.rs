// src/backends/interpreter.rs
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
use crate::common::types::Type;
use crate::ir::semantic_ir::{
    Instruction, SemanticBinOp, SemanticFunction, SemanticProgram, Terminator, TypedIRValue,
};
use std::collections::HashMap;


#[derive(Debug, Clone)]
pub enum RuntimeValue {
    Int(i64),
    Float(f64),
    String(String),
    Bool(bool),
    List(Vec<RuntimeValue>),
    Option(Option<Box<RuntimeValue>>),
    Result { is_ok: bool, value: Box<RuntimeValue> },
    Void,
}

impl RuntimeValue {
    pub fn as_bool(&self) -> bool {
        match self {
            RuntimeValue::Bool(b) => *b,
            RuntimeValue::Int(i) => *i != 0,
            RuntimeValue::Float(f) => *f != 0.0,
            RuntimeValue::String(s) => !s.is_empty(),
            RuntimeValue::List(l) => !l.is_empty(),
            RuntimeValue::Option(Some(_)) => true,
            RuntimeValue::Option(None) => false,
            RuntimeValue::Result { is_ok, .. } => *is_ok,
            RuntimeValue::Void => false,
        }
    }

    pub fn display(&self) -> String {
        match self {
            RuntimeValue::Int(i) => crate::common::types::print::format_int(*i),
            RuntimeValue::Float(f) => crate::common::types::print::format_float(*f),
            RuntimeValue::String(s) => s.clone(),
            RuntimeValue::Bool(b) => crate::common::types::print::format_bool(*b),
            RuntimeValue::List(l) => {
                let items: Vec<String> = l.iter().map(|v| v.display()).collect();
                format!("[{}]", items.join(", "))
            }
            RuntimeValue::Option(Some(v)) => format!("Some({})", v.display()),
            RuntimeValue::Option(None) => "None".to_string(),
            RuntimeValue::Result { is_ok: true, value } => format!("Ok({})", value.display()),
            RuntimeValue::Result { is_ok: false, value } => format!("Error({})", value.display()),
            RuntimeValue::Void => String::new(),
        }
    }
}

/// Human-readable name for a `RuntimeValue` variant, used only for
/// diagnostics when an internal invariant is broken.
fn runtime_kind(v: &RuntimeValue) -> &'static str {
    match v {
        RuntimeValue::Int(_) => "Int",
        RuntimeValue::Float(_) => "Float",
        RuntimeValue::String(_) => "String",
        RuntimeValue::Bool(_) => "Bool",
        RuntimeValue::List(_) => "List",
        RuntimeValue::Option(_) => "Option",
        RuntimeValue::Result { .. } => "Result",
        RuntimeValue::Void => "Void",
    }
}

pub struct Interpreter {
    variables: HashMap<String, RuntimeValue>,
    output: Vec<String>,
    program: SemanticProgram,
    return_value: Option<RuntimeValue>,
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

    fn execute_function(&mut self, func: &SemanticFunction) -> Result<(), String> {
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
            _ => {}
        }
        Ok(())
    }

    /// Match `value` against `pattern`. On success, return the list of
    /// variable bindings the pattern introduces.
    fn try_pattern_match(
        &mut self,
        pattern: &crate::ir::semantic_ir::SemanticPattern,
        value: &RuntimeValue,
    ) -> Option<Vec<(String, RuntimeValue)>> {
        use crate::ir::semantic_ir::SemanticPattern;

        match pattern {
            SemanticPattern::Wildcard => Some(Vec::new()),

            SemanticPattern::Some { binding } => match value {
                RuntimeValue::Option(Some(v)) => {
                    Some(vec![(binding.clone(), (**v).clone())])
                }
                _ => None,
            },

            SemanticPattern::None => match value {
                RuntimeValue::Option(None) => Some(Vec::new()),
                _ => None,
            },

            SemanticPattern::Ok { binding } => match value {
                RuntimeValue::Result { is_ok: true, value: v } => {
                    Some(vec![(binding.clone(), (**v).clone())])
                }
                _ => None,
            },

            SemanticPattern::Error { binding } => match value {
                RuntimeValue::Result { is_ok: false, value: v } => {
                    Some(vec![(binding.clone(), (**v).clone())])
                }
                _ => None,
            },

            SemanticPattern::Literal(lit) => {
                let lit_val = self.eval_value(lit);
                if lit_val.display() == value.display() {
                    Some(Vec::new())
                } else {
                    None
                }
            }
        }
    }

    fn eval_value(&mut self, v: &TypedIRValue) -> RuntimeValue {
        match v {
            TypedIRValue::Int(i) => RuntimeValue::Int(*i),
            TypedIRValue::Float(f) => RuntimeValue::Float(*f),
            TypedIRValue::Bool(b) => RuntimeValue::Bool(*b),
            TypedIRValue::String(s) => RuntimeValue::String(s.clone()),
            TypedIRValue::Void => RuntimeValue::Void,
            TypedIRValue::Variable(name, _) => self
                .variables
                .get(name)
                .cloned()
                .unwrap_or(RuntimeValue::Void),
            TypedIRValue::List(elems, _) => {
                RuntimeValue::List(elems.iter().map(|e| self.eval_value(e)).collect())
            }
            TypedIRValue::ArrayAccess { array, index, .. } => {
                let arr = self.eval_value(array);
                let idx = self.eval_value(index);
                let idx_usize = match idx {
                    RuntimeValue::Int(i) => {
                        if i < 0 {
                            0
                        } else {
                            i as usize
                        }
                    }
                    RuntimeValue::Float(f) => f as usize,
                    _ => 0,
                };
                if let RuntimeValue::List(list) = arr {
                    list.get(idx_usize).cloned().unwrap_or(RuntimeValue::Void)
                } else {
                    RuntimeValue::Void
                }
            }
            TypedIRValue::BinaryOp {
                op, left, right, ..
            } => {
                let l = self.eval_value(left);
                let r = self.eval_value(right);
                Self::eval_binop(op, l, r)
            }
            TypedIRValue::Call { function, args, .. } => self.eval_call(function, args),
            TypedIRValue::Cast { value, target_type } => {
                let v = self.eval_value(value);
                match (v, target_type) {
                    (RuntimeValue::Int(i), Type::Float) => RuntimeValue::Float(i as f64),
                    (RuntimeValue::Float(f), Type::Int) => RuntimeValue::Int(f as i64),
                    (v, _) => v,
                }
            }
            TypedIRValue::Some(inner) => {
                RuntimeValue::Option(Some(Box::new(self.eval_value(inner))))
            }
            TypedIRValue::None { .. } => RuntimeValue::Option(None),
            TypedIRValue::Ok { value, .. } => RuntimeValue::Result {
                is_ok: true,
                value: Box::new(self.eval_value(value)),
            },
            TypedIRValue::Error { value, .. } => RuntimeValue::Result {
                is_ok: false,
                value: Box::new(self.eval_value(value)),
            },
            _ => RuntimeValue::Void,
        }
    }

    fn eval_binop(op: &SemanticBinOp, l: RuntimeValue, r: RuntimeValue) -> RuntimeValue {
        // Capture the operand kinds before `(l, r)` is moved into the match.
        // Used only on the unreachable path — cheap enough to always compute.
        let lk = runtime_kind(&l);
        let rk = runtime_kind(&r);

        match op {
            SemanticBinOp::Add => match (l, r) {
                (RuntimeValue::Int(a), RuntimeValue::Int(b)) => RuntimeValue::Int(a + b),
                (RuntimeValue::Float(a), RuntimeValue::Float(b)) => RuntimeValue::Float(a + b),
                (RuntimeValue::Int(a), RuntimeValue::Float(b)) => RuntimeValue::Float(a as f64 + b),
                (RuntimeValue::Float(a), RuntimeValue::Int(b)) => RuntimeValue::Float(a + b as f64),
                (RuntimeValue::String(a), RuntimeValue::String(b)) => RuntimeValue::String(a + &b),
                _ => unreachable!(
                    "interpreter: Add received non-numeric operands ({lk}, {rk}) — \
                     builder should have coerced"
                ),
            },
            SemanticBinOp::Subtract => match (l, r) {
                (RuntimeValue::Int(a), RuntimeValue::Int(b)) => RuntimeValue::Int(a - b),
                (RuntimeValue::Float(a), RuntimeValue::Float(b)) => RuntimeValue::Float(a - b),
                _ => unreachable!(
                    "interpreter: Subtract received mixed operands ({lk}, {rk}) — \
                     builder should have coerced"
                ),
            },
            SemanticBinOp::Multiply => match (l, r) {
                (RuntimeValue::Int(a), RuntimeValue::Int(b)) => RuntimeValue::Int(a * b),
                (RuntimeValue::Float(a), RuntimeValue::Float(b)) => RuntimeValue::Float(a * b),
                _ => unreachable!(
                    "interpreter: Multiply received mixed operands ({lk}, {rk}) — \
                     builder should have coerced"
                ),
            },
            SemanticBinOp::Divide => match (l, r) {
                (RuntimeValue::Int(a), RuntimeValue::Int(b)) => {
                    if b != 0 {
                        RuntimeValue::Int(a / b)
                    } else {
                        println!("Error: integer division by zero");
                        std::process::exit(1);
                    }
                }
                (RuntimeValue::Float(a), RuntimeValue::Float(b)) => RuntimeValue::Float(a / b),
                _ => unreachable!(
                    "interpreter: Divide received mixed operands ({lk}, {rk}) — \
                     builder should have coerced"
                ),
            },
            SemanticBinOp::Equal => RuntimeValue::Bool(l.display() == r.display()),
            SemanticBinOp::NotEqual => RuntimeValue::Bool(l.display() != r.display()),
            SemanticBinOp::Greater => match (l, r) {
                (RuntimeValue::Int(a), RuntimeValue::Int(b)) => RuntimeValue::Bool(a > b),
                (RuntimeValue::Float(a), RuntimeValue::Float(b)) => RuntimeValue::Bool(a > b),
                _ => unreachable!(
                    "interpreter: Greater received mixed operands ({lk}, {rk}) — \
                     builder should have coerced"
                ),
            },
            SemanticBinOp::Less => match (l, r) {
                (RuntimeValue::Int(a), RuntimeValue::Int(b)) => RuntimeValue::Bool(a < b),
                (RuntimeValue::Float(a), RuntimeValue::Float(b)) => RuntimeValue::Bool(a < b),
                _ => unreachable!(
                    "interpreter: Less received mixed operands ({lk}, {rk}) — \
                     builder should have coerced"
                ),
            },
            SemanticBinOp::GreaterEqual => match (l, r) {
                (RuntimeValue::Int(a), RuntimeValue::Int(b)) => RuntimeValue::Bool(a >= b),
                (RuntimeValue::Float(a), RuntimeValue::Float(b)) => RuntimeValue::Bool(a >= b),
                _ => unreachable!(
                    "interpreter: GreaterEqual received mixed operands ({lk}, {rk}) — \
                     builder should have coerced"
                ),
            },
            SemanticBinOp::LessEqual => match (l, r) {
                (RuntimeValue::Int(a), RuntimeValue::Int(b)) => RuntimeValue::Bool(a <= b),
                (RuntimeValue::Float(a), RuntimeValue::Float(b)) => RuntimeValue::Bool(a <= b),
                _ => unreachable!(
                    "interpreter: LessEqual received mixed operands ({lk}, {rk}) — \
                     builder should have coerced"
                ),
            },
        }
    }

    fn eval_builtin_call(&mut self, func: &str, args: &[TypedIRValue]) -> RuntimeValue {
        let arg_vals: Vec<RuntimeValue> = args.iter().map(|a| self.eval_value(a)).collect();

        match func {
            "List.length" | "len" | "length" => {
                if let Some(RuntimeValue::List(l)) = arg_vals.first() {
                    RuntimeValue::Int(l.len() as i64)
                } else if let Some(RuntimeValue::String(s)) = arg_vals.first() {
                    RuntimeValue::Int(s.len() as i64)
                } else {
                    RuntimeValue::Int(0)
                }
            }
            "List.sum" | "sum" => {
                if let Some(RuntimeValue::List(list)) = arg_vals.first() {
                    let sum: f64 = list
                        .iter()
                        .map(|v| match v {
                            RuntimeValue::Int(i) => *i as f64,
                            RuntimeValue::Float(f) => *f,
                            _ => 0.0,
                        })
                        .sum();
                    RuntimeValue::Float(sum)
                } else {
                    RuntimeValue::Float(0.0)
                }
            }
            "Math.sqrt" => {
                if let Some(RuntimeValue::Float(f)) = arg_vals.first() {
                    RuntimeValue::Float(f.sqrt())
                } else {
                    RuntimeValue::Void
                }
            }
            "String.concat" | "String_concat" => {
                if arg_vals.len() == 2 {
                    match (&arg_vals[0], &arg_vals[1]) {
                        (RuntimeValue::String(a), RuntimeValue::String(b)) => {
                            RuntimeValue::String(format!("{}{}", a, b))
                        }
                        _ => RuntimeValue::Void,
                    }
                } else {
                    RuntimeValue::Void
                }
            }
            "String.to_upper" | "String.upper" | "to_upper" | "upper" => {
                if let Some(RuntimeValue::String(s)) = arg_vals.first() {
                    RuntimeValue::String(s.to_uppercase())
                } else {
                    RuntimeValue::Void
                }
            }
            "String.to_lower" | "String.lower" | "to_lower" | "lower" => {
                if let Some(RuntimeValue::String(s)) = arg_vals.first() {
                    RuntimeValue::String(s.to_lowercase())
                } else {
                    RuntimeValue::Void
                }
            }
            "String.length" | "String.len" | "strlen" => {
                if let Some(RuntimeValue::String(s)) = arg_vals.first() {
                    RuntimeValue::Int(s.len() as i64)
                } else {
                    RuntimeValue::Int(0)
                }
            }
            _ => RuntimeValue::Void,
        }
    }

        /// Evaluate a call to either a user function or a built-in. User
    /// functions are dispatched with a fresh frame; the caller's frame
    /// is saved and restored. Returns `Void` if the callee errors
    /// internally (block not found, infinite loop) — the error is
    /// printed to stderr.
    fn eval_call(&mut self, function: &str, args: &[TypedIRValue]) -> RuntimeValue {
        if let Some(callee) = self
            .program
            .functions
            .iter()
            .find(|f| f.name == function)
            .cloned()
        {
            let arg_vals: Vec<RuntimeValue> =
                args.iter().map(|a| self.eval_value(a)).collect();

            let saved_vars = std::mem::take(&mut self.variables);
            let saved_ret = self.return_value.take();

            for ((param_name, _), val) in callee.params.iter().zip(arg_vals) {
                self.variables.insert(param_name.clone(), val);
            }

            let result = self.execute_function(&callee);
            let ret = self.return_value.take().unwrap_or(RuntimeValue::Void);

            self.variables = saved_vars;
            self.return_value = saved_ret;

            if let Err(e) = result {
                eprintln!("[interpreter] error in {}: {}", callee.name, e);
                return RuntimeValue::Void;
            }

            ret
        } else {
            self.eval_builtin_call(function, args)
        }
    }
}
