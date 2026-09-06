#![allow(dead_code)]
use crate::ir::semantic_ir::{Instruction, Terminator, SemanticBinOp, SemanticFunction, SemanticProgram, TypedIRValue};
use std::collections::HashMap;

#[derive(Debug, Clone)]
pub enum RuntimeValue { Int(i64), Float(f64), String(String), Bool(bool), List(Vec<RuntimeValue>), Void }

impl RuntimeValue {
    fn as_bool(&self) -> bool {
        match self {
            RuntimeValue::Bool(b) => *b,
            RuntimeValue::Int(i) => *i!= 0,
            RuntimeValue::Float(f) => *f!= 0.0,
            RuntimeValue::String(s) =>!s.is_empty(),
            RuntimeValue::List(l) =>!l.is_empty(),
            RuntimeValue::Void => false,
        }
    }
    fn display(&self) -> String {
        match self {
            RuntimeValue::Int(i) => format!("{}", i),
            RuntimeValue::Float(f) => {
                if f.fract() == 0.0 { format!("{:.1}", f) } else { format!("{}", f) }
            },
            RuntimeValue::String(s) => s.clone(),
            RuntimeValue::Bool(b) => format!("{}", b),
            RuntimeValue::List(l) => {
                let items: Vec<String> = l.iter().map(|v| v.display()).collect();
                format!("[{}]", items.join(", "))
            },
            RuntimeValue::Void => String::new(),
        }
    }
}

pub struct Interpreter {
    variables: HashMap<String, RuntimeValue>,
    output: Vec<String>,
    program: SemanticProgram,
    return_value: Option<RuntimeValue>,
}

impl Interpreter {
    pub fn new(program: SemanticProgram) -> Self { Self { variables: HashMap::new(), output: Vec::new(), program, return_value: None } }
    pub fn run(&mut self) -> Result<String, String> {
        let main_func = self.program.functions.iter().find(|f| f.name == "main").cloned().ok_or("No main")?;
        self.execute_function(&main_func)?;
        Ok(self.output.join("\n"))
    }

    fn eval_value(&self, v: &TypedIRValue) -> RuntimeValue {
        match v {
            TypedIRValue::Int(i) => RuntimeValue::Int(*i),
            TypedIRValue::Float(f) => RuntimeValue::Float(*f),
            TypedIRValue::Bool(b) => RuntimeValue::Bool(*b),
            TypedIRValue::String(s) => RuntimeValue::String(s.clone()),
            TypedIRValue::Void => RuntimeValue::Void,
            TypedIRValue::Variable(name, _) => self.variables.get(name).cloned().unwrap_or(RuntimeValue::Void),
            TypedIRValue::List(elems, _) => RuntimeValue::List(elems.iter().map(|e| self.eval_value(e)).collect()),
            TypedIRValue::ArrayAccess { array, index,.. } => {
                let arr = self.eval_value(array);
                let idx = self.eval_value(index);
                let idx_usize = match idx {
                    RuntimeValue::Int(i) => if i < 0 { 0 } else { i as usize },
                    RuntimeValue::Float(f) => f as usize,
                    _ => 0,
                };
                if let RuntimeValue::List(list) = arr { list.get(idx_usize).cloned().unwrap_or(RuntimeValue::Void) } else { RuntimeValue::Void }
            },
            TypedIRValue::Call { function, args,.. } => self.eval_builtin_call(function, args),
            TypedIRValue::MethodCall { receiver, method_name, args,.. } => {
                let recv = self.eval_value(receiver);
                let arg_vals: Vec<RuntimeValue> = args.iter().map(|a| self.eval_value(a)).collect();
                self.eval_method_call(recv, method_name, &arg_vals)
            },
            TypedIRValue::BinaryOp { op, left, right,.. } => {
                let l = self.eval_value(left);
                let r = self.eval_value(right);
                Self::eval_binop(op, l, r)
            },
            TypedIRValue::Cast { value,.. } => self.eval_value(value),
            _ => RuntimeValue::Void,
        }
    }

    fn eval_binop(op: &SemanticBinOp, l: RuntimeValue, r: RuntimeValue) -> RuntimeValue {
        match op {
            SemanticBinOp::Add => match (l, r) {
                (RuntimeValue::Int(a), RuntimeValue::Int(b)) => RuntimeValue::Int(a + b),
                (RuntimeValue::Float(a), RuntimeValue::Float(b)) => RuntimeValue::Float(a + b),
                (RuntimeValue::Int(a), RuntimeValue::Float(b)) => RuntimeValue::Float(a as f64 + b),
                (RuntimeValue::Float(a), RuntimeValue::Int(b)) => RuntimeValue::Float(a + b as f64),
                (RuntimeValue::String(a), RuntimeValue::String(b)) => RuntimeValue::String(a + &b),
                _ => RuntimeValue::Void,
            },
            SemanticBinOp::Subtract => match (l, r) {
                (RuntimeValue::Int(a), RuntimeValue::Int(b)) => RuntimeValue::Int(a - b),
                (RuntimeValue::Float(a), RuntimeValue::Float(b)) => RuntimeValue::Float(a - b),
                (RuntimeValue::Int(a), RuntimeValue::Float(b)) => RuntimeValue::Float(a as f64 - b),
                (RuntimeValue::Float(a), RuntimeValue::Int(b)) => RuntimeValue::Float(a - b as f64),
                _ => RuntimeValue::Void,
            },
            SemanticBinOp::Multiply => match (l, r) {
                (RuntimeValue::Int(a), RuntimeValue::Int(b)) => RuntimeValue::Int(a * b),
                (RuntimeValue::Float(a), RuntimeValue::Float(b)) => RuntimeValue::Float(a * b),
                (RuntimeValue::Int(a), RuntimeValue::Float(b)) => RuntimeValue::Float(a as f64 * b),
                (RuntimeValue::Float(a), RuntimeValue::Int(b)) => RuntimeValue::Float(a * b as f64),
                _ => RuntimeValue::Void,
            },
            SemanticBinOp::Divide => match (l, r) {
                (RuntimeValue::Int(a), RuntimeValue::Int(b)) => if b!= 0 { RuntimeValue::Int(a / b) } else { RuntimeValue::Void },
                (RuntimeValue::Float(a), RuntimeValue::Float(b)) => RuntimeValue::Float(a / b),
                (RuntimeValue::Int(a), RuntimeValue::Float(b)) => RuntimeValue::Float(a as f64 / b),
                (RuntimeValue::Float(a), RuntimeValue::Int(b)) => RuntimeValue::Float(a / b as f64),
                _ => RuntimeValue::Void,
            },
            SemanticBinOp::Equal => RuntimeValue::Bool(Self::values_equal(&l, &r)),
            SemanticBinOp::NotEqual => RuntimeValue::Bool(!Self::values_equal(&l, &r)),
            SemanticBinOp::Greater => Self::compare(l, r, |o| o > 0),
            SemanticBinOp::Less => Self::compare(l, r, |o| o < 0),
            SemanticBinOp::GreaterEqual => Self::compare(l, r, |o| o >= 0),
            SemanticBinOp::LessEqual => Self::compare(l, r, |o| o <= 0),
            SemanticBinOp::And => RuntimeValue::Bool(l.as_bool() && r.as_bool()),
            SemanticBinOp::Or => RuntimeValue::Bool(l.as_bool() || r.as_bool()),
        }
    }

    fn values_equal(a: &RuntimeValue, b: &RuntimeValue) -> bool {
        match (a, b) {
            (RuntimeValue::Int(x), RuntimeValue::Int(y)) => x == y,
            (RuntimeValue::Float(x), RuntimeValue::Float(y)) => (x - y).abs() < 1e-9,
            (RuntimeValue::Int(x), RuntimeValue::Float(y)) => (*x as f64 - *y).abs() < 1e-9,
            (RuntimeValue::Float(x), RuntimeValue::Int(y)) => (*x - *y as f64).abs() < 1e-9,
            (RuntimeValue::String(x), RuntimeValue::String(y)) => x == y,
            (RuntimeValue::Bool(x), RuntimeValue::Bool(y)) => x == y,
            _ => false,
        }
    }

    fn compare<F: Fn(i32)->bool>(l: RuntimeValue, r: RuntimeValue, f: F) -> RuntimeValue {
        let ord = match (&l, &r) {
            (RuntimeValue::Int(a), RuntimeValue::Int(b)) => a.cmp(b) as i32,
            (RuntimeValue::Float(a), RuntimeValue::Float(b)) => if a < b { -1 } else if a > b { 1 } else { 0 },
            (RuntimeValue::Int(a), RuntimeValue::Float(b)) => if (*a as f64) < *b { -1 } else if (*a as f64) > *b { 1 } else { 0 },
            (RuntimeValue::Float(a), RuntimeValue::Int(b)) => if *a < *b as f64 { -1 } else if *a > *b as f64 { 1 } else { 0 },
            (RuntimeValue::String(a), RuntimeValue::String(b)) => if a < b { -1 } else if a > b { 1 } else { 0 },
            _ => return RuntimeValue::Bool(false),
        };
        RuntimeValue::Bool(f(ord))
    }

    fn eval_method_call(&self, recv: RuntimeValue, method: &str, args: &[RuntimeValue]) -> RuntimeValue {
        match recv {
            RuntimeValue::String(s) => match method {
                "upper" | "to_upper" | "toUpperCase" => RuntimeValue::String(s.to_uppercase()),
                "lower" | "to_lower" | "toLowerCase" => RuntimeValue::String(s.to_lowercase()),
                "len" | "length" => RuntimeValue::Float(s.len() as f64),
                "trim" => RuntimeValue::String(s.trim().to_string()),
                _ => RuntimeValue::Void,
            },
            RuntimeValue::List(list) => match method {
                "len" | "length" => RuntimeValue::Float(list.len() as f64),
                "get" => {
                    if let Some(RuntimeValue::Int(i)) = args.first() {
                        list.get(*i as usize).cloned().unwrap_or(RuntimeValue::Void)
                    } else { RuntimeValue::Void }
                },
                _ => RuntimeValue::Void,
            },
            _ => RuntimeValue::Void,
        }
    }

    fn eval_builtin_call(&self, func: &str, args: &[TypedIRValue]) -> RuntimeValue {
        let arg_vals: Vec<RuntimeValue> = args.iter().map(|a| self.eval_value(a)).collect();
        match func {
            "List.length" | "len" | "length" => {
                if let Some(first) = arg_vals.first() {
                    match first {
                        RuntimeValue::String(s) => RuntimeValue::Float(s.len() as f64),
                        RuntimeValue::List(l) => RuntimeValue::Float(l.len() as f64),
                        _ => RuntimeValue::Float(0.0),
                    }
                } else { RuntimeValue::Float(0.0) }
            },
            "String.length" | "String.len" => {
                if let Some(first) = arg_vals.first() {
                    match first {
                        RuntimeValue::String(s) => RuntimeValue::Int(s.len() as i64),
                        RuntimeValue::List(l) => RuntimeValue::Int(l.len() as i64),
                        _ => RuntimeValue::Int(0),
                    }
                } else { RuntimeValue::Int(0) }
            },
            "List.sum" | "sum" => {
                if let Some(RuntimeValue::List(list)) = arg_vals.first() {
                    let mut sum_f = 0.0;
                    for v in list {
                        match v {
                            RuntimeValue::Int(i) => sum_f += *i as f64,
                            RuntimeValue::Float(f) => sum_f += f,
                            _ => {},
                        }
                    }
                    RuntimeValue::Float(sum_f)
                } else { RuntimeValue::Float(0.0) }
            },
            "String.to_upper" | "String.upper" | "to_upper" | "upper" => {
                if let Some(RuntimeValue::String(s)) = arg_vals.first() { RuntimeValue::String(s.to_uppercase()) } else { RuntimeValue::Void }
            },
            "String.to_lower" | "String.lower" | "to_lower" | "lower" => {
                if let Some(RuntimeValue::String(s)) = arg_vals.first() { RuntimeValue::String(s.to_lowercase()) } else { RuntimeValue::Void }
            },
            _ => {
                let norm = func.split('.').last().unwrap_or(func);
                match norm {
                    "length" | "len" => {
                        if let Some(first) = arg_vals.first() {
                            match first {
                                RuntimeValue::String(s) => RuntimeValue::Float(s.len() as f64),
                                RuntimeValue::List(l) => RuntimeValue::Float(l.len() as f64),
                                _ => RuntimeValue::Float(0.0),
                            }
                        } else { RuntimeValue::Float(0.0) }
                    },
                    "sum" => {
                        if let Some(RuntimeValue::List(list)) = arg_vals.first() {
                            let mut sum_f = 0.0;
                            for v in list { match v { RuntimeValue::Int(i) => sum_f += *i as f64, RuntimeValue::Float(f) => sum_f += f, _ => {}, } }
                            RuntimeValue::Float(sum_f)
                        } else { RuntimeValue::Float(0.0) }
                    },
                    "to_upper" | "upper" => {
                        if let Some(RuntimeValue::String(s)) = arg_vals.first() { RuntimeValue::String(s.to_uppercase()) } else { RuntimeValue::Void }
                    },
                    "to_lower" | "lower" => {
                        if let Some(RuntimeValue::String(s)) = arg_vals.first() { RuntimeValue::String(s.to_lowercase()) } else { RuntimeValue::Void }
                    },
                    _ => RuntimeValue::Void,
                }
            }
        }
    }

    fn execute_function(&mut self, func: &SemanticFunction) -> Result<(), String> {
        let mut current = func.entry_block;
        let mut visited = 0;
        loop {
            if visited > 10000 { return Err("infinite loop".into()); }
            visited += 1;
            let block = func.blocks.iter().find(|b| b.id == current).ok_or("block not found")?.clone();
            for instr in &block.instructions {
                match instr {
                    Instruction::Declare { name, value,.. } => { self.variables.insert(name.clone(), self.eval_value(value)); },
                    Instruction::Assign { target, value } => { self.variables.insert(target.clone(), self.eval_value(value)); },
                    Instruction::ArrayAssign { array, index, value } => {
                        if let TypedIRValue::Variable(arr_name, _) = array.as_ref() {
                            let idx = self.eval_value(index);
                            let val = self.eval_value(value);
                            let idx_usize = match idx { RuntimeValue::Int(i) => i as usize, RuntimeValue::Float(f) => f as usize, _ => 0 };
                            if let Some(RuntimeValue::List(list)) = self.variables.get(arr_name).cloned() {
                                let mut new_list = list;
                                if idx_usize < new_list.len() { new_list[idx_usize] = val; } else if idx_usize == new_list.len() { new_list.push(val); }
                                self.variables.insert(arr_name.clone(), RuntimeValue::List(new_list));
                            }
                        }
                    },
                    Instruction::Print { value } => { self.output.push(self.eval_value(value).display()); },
                    Instruction::Call { func: fname, args, result } => {
                        if fname == "println" || fname == "print" {
                            if let Some(a) = args.first() { self.output.push(self.eval_value(a).display()); }
                        } else {
                            let rv = self.eval_builtin_call(fname, args);
                            if let Some(res) = result { self.variables.insert(res.clone(), rv); }
                        }
                    },
                    Instruction::MethodCall { object, method, args, result } => {
                        let recv = self.variables.get(object).cloned().unwrap_or(RuntimeValue::Void);
                        let arg_vals: Vec<RuntimeValue> = args.iter().map(|a| self.eval_value(a)).collect();
                        let rv = self.eval_method_call(recv, method, &arg_vals);
                        if let Some(res) = result { self.variables.insert(res.clone(), rv); }
                    },
                    _ => {}
                }
            }
            match &block.terminator {
                Some(Terminator::Return { value,.. }) => { if let Some(v) = value { self.return_value = Some(self.eval_value(v)); } break; },
                Some(Terminator::Jump { block: b }) => { current = *b; continue; },
                Some(Terminator::Branch { condition, then_block, else_block }) => {
                    let cond = self.eval_value(condition).as_bool();
                    current = if cond { *then_block } else { *else_block };
                    continue;
                },
                Some(Terminator::Switch {.. }) => break,
                Some(Terminator::IteratorNext { iterator, target, body_block, exit_block }) => {
                    let idx_key = format!("{}_idx", iterator);
                    let idx = match self.variables.get(&idx_key) { Some(RuntimeValue::Int(i)) => *i as usize, _ => 0 };
                    let iter_val = self.variables.get(iterator).cloned().unwrap_or(RuntimeValue::Void);
                    if let RuntimeValue::List(list) = iter_val {
                        if idx < list.len() {
                            self.variables.insert(target.clone(), list[idx].clone());
                            self.variables.insert(idx_key, RuntimeValue::Int((idx + 1) as i64));
                            current = *body_block;
                        } else { current = *exit_block; }
                    } else { current = *exit_block; }
                    continue;
                },
                Some(Terminator::Spawn { entry_block }) => { current = *entry_block; continue; },
                Some(Terminator::Fork { join_block,.. }) => { current = *join_block; continue; },
                Some(Terminator::Defer { cleanup_block }) => { current = *cleanup_block; continue; },
                None => break,
            }
        }
        Ok(())
    }
}
