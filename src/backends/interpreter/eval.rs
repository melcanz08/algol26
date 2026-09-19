// src/backends/interpreter/eval.rs

use super::runtime::{runtime_kind, EvalError, RuntimeValue};
use super::Interpreter;
use crate::common::types::Type;
use crate::ir::semantic_ir::{SemanticBinOp, TypedIRValue};

/// Extract a numeric `f64` from a runtime value, or fail closed.
///
/// The analyzer only permits `List.sum`/`List.max`/`List.min` on
/// `List<Int>` or `List<Float>`. If a non-numeric element reaches
/// these builtins at runtime, the analyzer missed something —
/// returning a placeholder would silently produce a wrong sum.
fn numeric_element(v: &RuntimeValue, op: &'static str) -> Result<f64, EvalError> {
    match v {
        RuntimeValue::Int(i) => Ok(*i as f64),
        RuntimeValue::Float(f) => Ok(*f),
        other => Err(EvalError::TypeMismatch {
            op,
            left: runtime_kind(other),
            right: "Int | Float",
        }),
    }
}

impl Interpreter {
    pub(super) fn eval_value(&mut self, v: &TypedIRValue) -> Result<RuntimeValue, EvalError> {
        Ok(match v {
            TypedIRValue::Int(i) => RuntimeValue::Int(*i),
            TypedIRValue::Float(f) => RuntimeValue::Float(*f),
            TypedIRValue::Bool(b) => RuntimeValue::Bool(*b),
            TypedIRValue::String(s) => RuntimeValue::String(s.clone()),
            TypedIRValue::Void => RuntimeValue::Void,
            TypedIRValue::Variable(name, _) => {
                self.variables.get(name).cloned().ok_or_else(|| {
                    EvalError::Runtime(format!(
                        "variable `{}` not found at runtime — the verifier \
                         should have caught this",
                        name
                    ))
                })?
            }
            TypedIRValue::List(elems, _) => {
                let mut out = Vec::with_capacity(elems.len());
                for e in elems {
                    out.push(self.eval_value(e)?);
                }
                RuntimeValue::List(out)
            }
            // `Array` was previously unhandled. Treat it as a List —
            // the interpreter's runtime value model has no fixed-size
            // array; fixed sizes are a compile-time property.
            TypedIRValue::Array(elems, _, _) => {
                let mut out = Vec::with_capacity(elems.len());
                for e in elems {
                    out.push(self.eval_value(e)?);
                }
                RuntimeValue::List(out)
            }
            TypedIRValue::ArrayAccess { array, index, .. } => {
                let arr = self.eval_value(array)?;
                let idx = self.eval_value(index)?;
                let idx_usize = match idx {
                    RuntimeValue::Int(i) if i < 0 => {
                        return Err(EvalError::Runtime(format!("array index {} is negative", i)));
                    }
                    RuntimeValue::Int(i) => i as usize,
                    other => {
                        return Err(EvalError::TypeMismatch {
                            op: "ArrayAccess.index",
                            left: runtime_kind(&other),
                            right: "Int",
                        });
                    }
                };
                let list = match arr {
                    RuntimeValue::List(l) => l,
                    other => {
                        return Err(EvalError::TypeMismatch {
                            op: "ArrayAccess.array",
                            left: runtime_kind(&other),
                            right: "List",
                        });
                    }
                };
                list.get(idx_usize).cloned().ok_or_else(|| {
                    EvalError::Runtime(format!(
                        "array index {} out of bounds (length {})",
                        idx_usize,
                        list.len()
                    ))
                })?
            }
            TypedIRValue::BinaryOp {
                op, left, right, ..
            } => {
                let l = self.eval_value(left)?;
                let r = self.eval_value(right)?;
                return Self::eval_binop(op, l, r);
            }
            TypedIRValue::Call { function, args, .. } => {
                return self.eval_call(function, args);
            }
            TypedIRValue::Cast { value, target_type } => {
                let v = self.eval_value(value)?;
                match (v, target_type) {
                    (RuntimeValue::Int(i), Type::Float) => RuntimeValue::Float(i as f64),
                    (RuntimeValue::Float(f), Type::Int) => RuntimeValue::Int(f as i64),
                    (v, _) => v,
                }
            }
            TypedIRValue::Some(inner) => {
                RuntimeValue::Option(Some(Box::new(self.eval_value(inner)?)))
            }
            TypedIRValue::None { .. } => RuntimeValue::Option(None),
            TypedIRValue::Ok { value, .. } => RuntimeValue::Result {
                is_ok: true,
                value: Box::new(self.eval_value(value)?),
            },
            TypedIRValue::Error { value, .. } => RuntimeValue::Result {
                is_ok: false,
                value: Box::new(self.eval_value(value)?),
            },
            TypedIRValue::PtrLiteral(n) => RuntimeValue::Int(*n as i64),
            TypedIRValue::NullPtr => RuntimeValue::Int(0),
            // The interpreter does not model references or regions.
            // Refuse loudly; the capability matrix should have caught
            // this before the interpreter ran.
            TypedIRValue::BorrowShared { .. }
            | TypedIRValue::BorrowMutable { .. }
            | TypedIRValue::ReadReference { .. }
            | TypedIRValue::AddrOf { .. } => {
                return Err(EvalError::Unsupported {
                    construct: "references",
                    hint: "use the LLVM backend (--interpreter does not model borrows)",
                });
            }

            TypedIRValue::Range(..) => {
                return Err(EvalError::Unsupported {
                    construct: "ranges",
                    hint: "ranges are not yet lowered by the interpreter",
                });
            }

            TypedIRValue::FieldAccess { .. } => {
                return Err(EvalError::Unsupported {
                    construct: "field access",
                    hint: "struct fields are not yet modeled by the interpreter",
                });
            }
        })
    }
    pub(super) fn eval_binop(
        op: &SemanticBinOp,
        l: RuntimeValue,
        r: RuntimeValue,
    ) -> Result<RuntimeValue, EvalError> {
        let lk = runtime_kind(&l);
        let rk = runtime_kind(&r);

        let mismatch = |op_name: &'static str| EvalError::TypeMismatch {
            op: op_name,
            left: lk,
            right: rk,
        };

        match op {
            SemanticBinOp::Add => match (l, r) {
                (RuntimeValue::Int(a), RuntimeValue::Int(b)) => {
                    Ok(RuntimeValue::Int(a.wrapping_add(b)))
                }
                (RuntimeValue::Float(a), RuntimeValue::Float(b)) => Ok(RuntimeValue::Float(a + b)),
                (RuntimeValue::Int(a), RuntimeValue::Float(b)) => {
                    Ok(RuntimeValue::Float(a as f64 + b))
                }
                (RuntimeValue::Float(a), RuntimeValue::Int(b)) => {
                    Ok(RuntimeValue::Float(a + b as f64))
                }
                (RuntimeValue::String(a), RuntimeValue::String(b)) => {
                    Ok(RuntimeValue::String(a + &b))
                }
                _ => Err(mismatch("Add")),
            },
            SemanticBinOp::Subtract => match (l, r) {
                (RuntimeValue::Int(a), RuntimeValue::Int(b)) => {
                    Ok(RuntimeValue::Int(a.wrapping_sub(b)))
                }
                (RuntimeValue::Float(a), RuntimeValue::Float(b)) => Ok(RuntimeValue::Float(a - b)),
                _ => Err(mismatch("Subtract")),
            },
            SemanticBinOp::Multiply => match (l, r) {
                (RuntimeValue::Int(a), RuntimeValue::Int(b)) => {
                    Ok(RuntimeValue::Int(a.wrapping_mul(b)))
                }
                (RuntimeValue::Float(a), RuntimeValue::Float(b)) => Ok(RuntimeValue::Float(a * b)),
                _ => Err(mismatch("Multiply")),
            },
            SemanticBinOp::Divide => match (l, r) {
                (RuntimeValue::Int(a), RuntimeValue::Int(b)) => {
                    if b == 0 {
                        return Err(EvalError::Runtime("integer division by zero".into()));
                    }
                    a.checked_div(b).map(RuntimeValue::Int).ok_or_else(|| {
                        EvalError::Runtime(format!("integer division overflow: {} / {}", a, b))
                    })
                }
                (RuntimeValue::Float(a), RuntimeValue::Float(b)) => Ok(RuntimeValue::Float(a / b)),
                _ => Err(mismatch("Divide")),
            },

            // Structural equality. Uses `runtime_eq`, which handles
            // mixed Int/Float comparison so `1 == 1.0` is `true`
            // here, matching LLVM.
            SemanticBinOp::Equal => Ok(RuntimeValue::Bool(l.runtime_eq(&r))),
            SemanticBinOp::NotEqual => Ok(RuntimeValue::Bool(!l.runtime_eq(&r))),

            SemanticBinOp::Greater => match (l, r) {
                (RuntimeValue::Int(a), RuntimeValue::Int(b)) => Ok(RuntimeValue::Bool(a > b)),
                (RuntimeValue::Float(a), RuntimeValue::Float(b)) => Ok(RuntimeValue::Bool(a > b)),
                _ => Err(mismatch("Greater")),
            },
            SemanticBinOp::Less => match (l, r) {
                (RuntimeValue::Int(a), RuntimeValue::Int(b)) => Ok(RuntimeValue::Bool(a < b)),
                (RuntimeValue::Float(a), RuntimeValue::Float(b)) => Ok(RuntimeValue::Bool(a < b)),
                _ => Err(mismatch("Less")),
            },
            SemanticBinOp::GreaterEqual => match (l, r) {
                (RuntimeValue::Int(a), RuntimeValue::Int(b)) => Ok(RuntimeValue::Bool(a >= b)),
                (RuntimeValue::Float(a), RuntimeValue::Float(b)) => Ok(RuntimeValue::Bool(a >= b)),
                _ => Err(mismatch("GreaterEqual")),
            },
            SemanticBinOp::LessEqual => match (l, r) {
                (RuntimeValue::Int(a), RuntimeValue::Int(b)) => Ok(RuntimeValue::Bool(a <= b)),
                (RuntimeValue::Float(a), RuntimeValue::Float(b)) => Ok(RuntimeValue::Bool(a <= b)),
                _ => Err(mismatch("LessEqual")),
            },
        }
    }
    pub(super) fn eval_builtin_call(
        &mut self,
        func: &str,
        args: &[TypedIRValue],
    ) -> Result<RuntimeValue, EvalError> {
        let mut arg_vals = Vec::with_capacity(args.len());
        for a in args {
            arg_vals.push(self.eval_value(a)?);
        }

        match func {
            // `length` reports Unicode codepoints for strings, not
            // UTF-8 bytes. `String.substring` already counts
            // codepoints (via `chars()`), so the two agree on the
            // same unit. The LLVM backend still uses C `strlen`
            // (bytes) — see the TODO in `llvm_codegen/builtins.rs`.
            "List.length" | "len" | "length" => match arg_vals.first() {
                Some(RuntimeValue::List(l)) => Ok(RuntimeValue::Int(l.len() as i64)),
                Some(RuntimeValue::String(s)) => Ok(RuntimeValue::Int(s.chars().count() as i64)),
                Some(other) => Err(EvalError::TypeMismatch {
                    op: "length",
                    left: runtime_kind(other),
                    right: "List | String",
                }),
                None => Err(EvalError::Runtime("length: missing argument".into())),
            },

            "List.sum" | "sum" => match arg_vals.first() {
                Some(RuntimeValue::List(list)) => {
                    let mut sum = 0.0;
                    for v in list {
                        sum += numeric_element(v, "List.sum")?;
                    }
                    Ok(RuntimeValue::Float(sum))
                }
                Some(other) => Err(EvalError::TypeMismatch {
                    op: "List.sum",
                    left: runtime_kind(other),
                    right: "List",
                }),
                None => Err(EvalError::Runtime("List.sum: missing argument".into())),
            },

            "List.max" => match arg_vals.first() {
                Some(RuntimeValue::List(list)) => {
                    let mut max = f64::NEG_INFINITY;
                    for v in list {
                        let f = numeric_element(v, "List.max")?;
                        if f > max {
                            max = f;
                        }
                    }
                    Ok(RuntimeValue::Float(max))
                }
                Some(other) => Err(EvalError::TypeMismatch {
                    op: "List.max",
                    left: runtime_kind(other),
                    right: "List",
                }),
                None => Err(EvalError::Runtime("List.max: missing argument".into())),
            },

            "List.min" => match arg_vals.first() {
                Some(RuntimeValue::List(list)) => {
                    let min = list
                        .iter()
                        .filter_map(|v| match v {
                            RuntimeValue::Int(i) => Some(*i as f64),
                            RuntimeValue::Float(f) => Some(*f),
                            _ => None,
                        })
                        .fold(f64::INFINITY, f64::min);
                    Ok(RuntimeValue::Float(min))
                }
                Some(other) => Err(EvalError::TypeMismatch {
                    op: "List.min",
                    left: runtime_kind(other),
                    right: "List",
                }),
                None => Err(EvalError::Runtime("List.min: missing argument".into())),
            },

            "String.substring" => {
                let s = match arg_vals.first() {
                    Some(RuntimeValue::String(s)) => s.clone(),
                    Some(other) => {
                        return Err(EvalError::TypeMismatch {
                            op: "String.substring",
                            left: runtime_kind(other),
                            right: "String",
                        })
                    }
                    None => {
                        return Err(EvalError::Runtime(
                            "String.substring: missing string argument".into(),
                        ))
                    }
                };
                let start = match arg_vals.get(1) {
                    Some(RuntimeValue::Int(i)) => (*i).max(0) as usize,
                    Some(other) => {
                        return Err(EvalError::TypeMismatch {
                            op: "String.substring.start",
                            left: runtime_kind(other),
                            right: "Int",
                        })
                    }
                    None => {
                        return Err(EvalError::Runtime(
                            "String.substring: missing start argument".into(),
                        ))
                    }
                };
                let length = match arg_vals.get(2) {
                    Some(RuntimeValue::Int(i)) => (*i).max(0) as usize,
                    Some(other) => {
                        return Err(EvalError::TypeMismatch {
                            op: "String.substring.length",
                            left: runtime_kind(other),
                            right: "Int",
                        })
                    }
                    None => {
                        return Err(EvalError::Runtime(
                            "String.substring: missing length argument".into(),
                        ))
                    }
                };
                let chars: Vec<char> = s.chars().collect();
                let start = start.min(chars.len());
                let end = start.saturating_add(length).min(chars.len());
                Ok(RuntimeValue::String(chars[start..end].iter().collect()))
            }

            "Math.sqrt" | "Math.sin" | "Math.cos" | "Math.tan" | "Math.exp" | "Math.log"
            | "Math.floor" | "Math.ceil" | "Math.abs" => {
                let x = match arg_vals.first() {
                    Some(RuntimeValue::Float(f)) => *f,
                    Some(RuntimeValue::Int(i)) => *i as f64,
                    Some(other) => {
                        return Err(EvalError::TypeMismatch {
                            op: "Math.*",
                            left: runtime_kind(other),
                            right: "Int | Float",
                        })
                    }
                    None => return Err(EvalError::Runtime("Math.*: missing argument".into())),
                };
                let r = match func {
                    "Math.sqrt" => x.sqrt(),
                    "Math.sin" => x.sin(),
                    "Math.cos" => x.cos(),
                    "Math.tan" => x.tan(),
                    "Math.exp" => x.exp(),
                    "Math.log" => x.ln(),
                    "Math.floor" => x.floor(),
                    "Math.ceil" => x.ceil(),
                    "Math.abs" => x.abs(),
                    _ => {
                        // The outer match arm binds `func` to the
                        // Math.* names handled above. Reaching this
                        // point means the arm list and the inner
                        // dispatch got out of sync.
                        return Err(EvalError::Unsupported {
                            construct: "Math builtin dispatch",
                            hint: "internal: arm list out of sync with Math.* alternatives",
                        });
                    }
                };
                Ok(RuntimeValue::Float(r))
            }

            "Math.pow" => {
                let (a, b) = match (arg_vals.first(), arg_vals.get(1)) {
                    (Some(RuntimeValue::Float(a)), Some(RuntimeValue::Float(b))) => (*a, *b),
                    (Some(RuntimeValue::Int(a)), Some(RuntimeValue::Float(b))) => (*a as f64, *b),
                    (Some(RuntimeValue::Float(a)), Some(RuntimeValue::Int(b))) => (*a, *b as f64),
                    (Some(RuntimeValue::Int(a)), Some(RuntimeValue::Int(b))) => {
                        (*a as f64, *b as f64)
                    }
                    _ => {
                        return Err(EvalError::TypeMismatch {
                            op: "Math.pow",
                            left: arg_vals.first().map(runtime_kind).unwrap_or("none"),
                            right: arg_vals.get(1).map(runtime_kind).unwrap_or("none"),
                        })
                    }
                };
                Ok(RuntimeValue::Float(a.powf(b)))
            }

            "String.concat" | "String_concat" => match (arg_vals.first(), arg_vals.get(1)) {
                (Some(RuntimeValue::String(a)), Some(RuntimeValue::String(b))) => {
                    Ok(RuntimeValue::String(format!("{}{}", a, b)))
                }
                _ => Err(EvalError::TypeMismatch {
                    op: "String.concat",
                    left: arg_vals.first().map(runtime_kind).unwrap_or("none"),
                    right: arg_vals.get(1).map(runtime_kind).unwrap_or("none"),
                }),
            },

            "String.to_upper" | "String.upper" | "to_upper" | "upper" => match arg_vals.first() {
                Some(RuntimeValue::String(s)) => Ok(RuntimeValue::String(s.to_uppercase())),
                Some(other) => Err(EvalError::TypeMismatch {
                    op: "String.to_upper",
                    left: runtime_kind(other),
                    right: "String",
                }),
                None => Err(EvalError::Runtime(
                    "String.to_upper: missing argument".into(),
                )),
            },

            "String.to_lower" | "String.lower" | "to_lower" | "lower" => match arg_vals.first() {
                Some(RuntimeValue::String(s)) => Ok(RuntimeValue::String(s.to_lowercase())),
                Some(other) => Err(EvalError::TypeMismatch {
                    op: "String.to_lower",
                    left: runtime_kind(other),
                    right: "String",
                }),
                None => Err(EvalError::Runtime(
                    "String.to_lower: missing argument".into(),
                )),
            },

            "String.length" | "String.len" | "strlen" => match arg_vals.first() {
                Some(RuntimeValue::String(s)) => Ok(RuntimeValue::Int(s.chars().count() as i64)),
                Some(other) => Err(EvalError::TypeMismatch {
                    op: "String.length",
                    left: runtime_kind(other),
                    right: "String",
                }),
                None => Err(EvalError::Runtime("String.length: missing argument".into())),
            },

            _ => Err(EvalError::Unsupported {
                construct: "builtin",
                hint: "unknown builtin — the IR verifier should have rejected this call",
            }),
        }
    }
    /// Evaluate a call to either a user function or a built-in. User
    /// functions are dispatched with a fresh frame; the caller's frame
    /// is saved and restored. Returns `Void` if the callee errors
    /// internally (block not found, infinite loop) — the error is
    /// printed to stderr.
    pub(super) fn eval_call(
        &mut self,
        function: &str,
        args: &[TypedIRValue],
    ) -> Result<RuntimeValue, EvalError> {
        if let Some(callee) = self
            .program
            .functions
            .iter()
            .find(|f| f.name == function)
            .cloned()
        {
            let mut arg_vals = Vec::with_capacity(args.len());
            for a in args {
                arg_vals.push(self.eval_value(a)?);
            }

            let saved_vars = std::mem::take(&mut self.variables);
            let saved_ret = self.return_value.take();
            let saved_regions = std::mem::take(&mut self.region_stack);

            for ((param_name, _), val) in callee.params.iter().zip(arg_vals) {
                self.variables.insert(param_name.clone(), val);
            }

            let result = self.execute_function(&callee);
            let ret = match self.return_value.take() {
                Some(v) => v,
                None if callee.return_type == Type::Void => RuntimeValue::Void,
                None => {
                    return Err(EvalError::Runtime(format!(
                        "function `{}` returned no value but its return type is `{}`",
                        callee.name, callee.return_type
                    )));
                }
            };

            self.variables = saved_vars;
            self.return_value = saved_ret;
            self.region_stack = saved_regions;

            // `execute_function` still returns `Result<(), String>`
            // in PR-A. Convert.
            result.map_err(|e| EvalError::Runtime(format!("in {}: {}", callee.name, e)))?;

            Ok(ret)
        } else {
            self.eval_builtin_call(function, args)
        }
    }
}
