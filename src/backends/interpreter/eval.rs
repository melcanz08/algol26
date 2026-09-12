// src/backends/interpreter/eval.rs

use super::Interpreter;
use super::runtime::{runtime_kind, RuntimeValue};
use crate::ir::semantic_ir::{SemanticBinOp, TypedIRValue};
use crate::common::types::Type;

impl Interpreter {
    pub(super) fn eval_value(&mut self, v: &TypedIRValue) -> RuntimeValue {
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
    pub(super) fn eval_binop(op: &SemanticBinOp, l: RuntimeValue, r: RuntimeValue) -> RuntimeValue {
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
    pub(super) fn eval_builtin_call(&mut self, func: &str, args: &[TypedIRValue]) -> RuntimeValue {
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
    pub(super) fn eval_call(&mut self, function: &str, args: &[TypedIRValue]) -> RuntimeValue {
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