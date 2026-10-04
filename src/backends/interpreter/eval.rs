// src/backends/interpreter/eval.rs

use super::runtime::{runtime_kind, EvalError, MapKey, RuntimeValue};
use super::Interpreter;
use crate::common::types::Type;
use crate::ir::semantic_ir::{SemanticBinOp, TypedIRValue};
use std::collections::HashMap;

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
            TypedIRValue::Record { name, fields, .. } => {
                let mut out = Vec::with_capacity(fields.len());
                for (fname, fval) in fields {
                    out.push((fname.clone(), self.eval_value(fval)?));
                }
                RuntimeValue::Record {
                    name: name.clone(),
                    fields: out,
                }
            }
            TypedIRValue::Map { entries, .. } => {
                let mut out = HashMap::with_capacity(entries.len());
                for (k, v) in entries {
                    let k_val = self.eval_value(k)?;
                    let v_val = self.eval_value(v)?;
                    let key = MapKey::from_runtime(&k_val)?;
                    out.insert(key, v_val);
                }
                RuntimeValue::Map(out)
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
                    // ADR 0029: nominal wrap/unwrap. The runtime
                    // representation of a nominal type is identical
                    // to its base, so the value flows through
                    // unchanged. Explicit arm so the intent is
                    // visible without tracing the catch-all.
                    (v, Type::Distinct { .. }) => v,
                    // ADR 0030: enum wrap/unwrap. Same shape — the
                    // runtime value is just the ordinal as an Int.
                    (v, Type::Enum { .. }) => v,
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

            TypedIRValue::FieldAccess { object, field, .. } => {
                let obj = self.eval_value(object)?;
                match obj {
                    RuntimeValue::Record { fields, .. } => fields
                        .into_iter()
                        .find(|(n, _)| n == field)
                        .map(|(_, v)| v)
                        .ok_or_else(|| {
                            EvalError::Runtime(format!(
                                "field `{}` not found at runtime — analyzer should have caught this",
                                field
                            ))
                        })?,
                    other => {
                        return Err(EvalError::TypeMismatch {
                            op: "FieldAccess",
                            left: runtime_kind(&other),
                            right: "Record",
                        });
                    }
                }
            }
            // ADR 0032 A5d: sets erase to their u64 bit pattern.
            // The IR value's `bits` field is exactly the runtime
            // representation. Reusing `RuntimeValue::Int` matches how
            // enums and subranges erase (they flow through `Int` too —
            // see the `Cast` arms below).
            TypedIRValue::Set { bits, .. } => RuntimeValue::Int(*bits as i64),
            TypedIRValue::SetSingleton {
                element,
                element_type,
            } => {
                let elem_val = self.eval_value(element)?;
                let ordinal: i64 = match elem_val {
                    RuntimeValue::Int(i) => i,
                    RuntimeValue::Bool(b) => {
                        if b {
                            1
                        } else {
                            0
                        }
                    }
                    other => {
                        return Err(EvalError::TypeMismatch {
                            op: "SetSingleton.element",
                            left: runtime_kind(&other),
                            right: "Int or Bool",
                        });
                    }
                };
                let low: i64 = match element_type {
                    Type::Subrange { low, .. } => *low,
                    _ => 0,
                };
                let bit = (ordinal - low) as u32;
                let mask = 1u64.checked_shl(bit).unwrap_or(0);
                RuntimeValue::Int(mask as i64)
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
            // ADR 0032 A5d: set operations on the u64 bit pattern,
            // carried in RuntimeValue::Int. `i64` and `u64` have the
            // same bit layout; `as i64`/`as u64` round-trips losslessly.
            SemanticBinOp::SetUnion => match (&l, &r) {
                (RuntimeValue::Int(a), RuntimeValue::Int(b)) => {
                    Ok(RuntimeValue::Int(((*a as u64) | (*b as u64)) as i64))
                }
                _ => Err(mismatch("SetUnion")),
            },
            SemanticBinOp::SetIntersection => match (&l, &r) {
                (RuntimeValue::Int(a), RuntimeValue::Int(b)) => {
                    Ok(RuntimeValue::Int(((*a as u64) & (*b as u64)) as i64))
                }
                _ => Err(mismatch("SetIntersection")),
            },
            SemanticBinOp::SetDifference => match (&l, &r) {
                (RuntimeValue::Int(a), RuntimeValue::Int(b)) => {
                    Ok(RuntimeValue::Int(((*a as u64) & !(*b as u64)) as i64))
                }
                _ => Err(mismatch("SetDifference")),
            },
            SemanticBinOp::SetMember => match (&l, &r) {
                (RuntimeValue::Int(d), RuntimeValue::Int(s)) => {
                    // (1u64 << d) & s != 0. `d` has already been
                    // shifted down to its bit position by the IR
                    // builder (see A5d-llvm's low-subtraction).
                    let bit = 1u64.checked_shl(*d as u32).unwrap_or(0);
                    Ok(RuntimeValue::Bool((bit & (*s as u64)) != 0))
                }
                _ => Err(mismatch("SetMember")),
            },
            SemanticBinOp::SetSubset => match (&l, &r) {
                (RuntimeValue::Int(a), RuntimeValue::Int(b)) => {
                    // a <= b iff a & ~b == 0.
                    Ok(RuntimeValue::Bool(((*a as u64) & !(*b as u64)) == 0))
                }
                _ => Err(mismatch("SetSubset")),
            },
            SemanticBinOp::SetStrictSubset => match (&l, &r) {
                (RuntimeValue::Int(a), RuntimeValue::Int(b)) => {
                    let au = *a as u64;
                    let bu = *b as u64;
                    Ok(RuntimeValue::Bool((au & !bu) == 0 && au != bu))
                }
                _ => Err(mismatch("SetStrictSubset")),
            },
            SemanticBinOp::SetSuperset => match (&l, &r) {
                (RuntimeValue::Int(a), RuntimeValue::Int(b)) => {
                    // b <= a.
                    Ok(RuntimeValue::Bool(((*b as u64) & !(*a as u64)) == 0))
                }
                _ => Err(mismatch("SetSuperset")),
            },
            SemanticBinOp::SetStrictSuperset => match (&l, &r) {
                (RuntimeValue::Int(a), RuntimeValue::Int(b)) => {
                    let au = *a as u64;
                    let bu = *b as u64;
                    Ok(RuntimeValue::Bool((bu & !au) == 0 && au != bu))
                }
                _ => Err(mismatch("SetStrictSuperset")),
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
            "String.trim" => match arg_vals.first() {
                Some(RuntimeValue::String(s)) => Ok(RuntimeValue::String(s.trim().to_string())),
                Some(other) => Err(EvalError::TypeMismatch {
                    op: "String.trim",
                    left: runtime_kind(other),
                    right: "String",
                }),
                None => Err(EvalError::Runtime("String.trim: missing argument".into())),
            },
            "String.split" => {
                let s = match arg_vals.first() {
                    Some(RuntimeValue::String(s)) => s.clone(),
                    Some(other) => {
                        return Err(EvalError::TypeMismatch {
                            op: "String.split",
                            left: runtime_kind(other),
                            right: "String",
                        })
                    }
                    None => {
                        return Err(EvalError::Runtime(
                            "String.split: missing string argument".into(),
                        ))
                    }
                };
                let sep = match arg_vals.get(1) {
                    Some(RuntimeValue::String(s)) => s.clone(),
                    Some(other) => {
                        return Err(EvalError::TypeMismatch {
                            op: "String.split.separator",
                            left: runtime_kind(other),
                            right: "String",
                        })
                    }
                    None => {
                        return Err(EvalError::Runtime(
                            "String.split: missing separator argument".into(),
                        ))
                    }
                };
                let parts: Vec<RuntimeValue> = s
                    .split(sep.as_str())
                    .map(|p| RuntimeValue::String(p.to_string()))
                    .collect();
                Ok(RuntimeValue::List(parts))
            }
            "String.join" => {
                let list = match arg_vals.first() {
                    Some(RuntimeValue::List(l)) => l.clone(),
                    Some(other) => {
                        return Err(EvalError::TypeMismatch {
                            op: "String.join",
                            left: runtime_kind(other),
                            right: "List",
                        })
                    }
                    None => {
                        return Err(EvalError::Runtime(
                            "String.join: missing list argument".into(),
                        ))
                    }
                };
                let sep = match arg_vals.get(1) {
                    Some(RuntimeValue::String(s)) => s.clone(),
                    Some(other) => {
                        return Err(EvalError::TypeMismatch {
                            op: "String.join.separator",
                            left: runtime_kind(other),
                            right: "String",
                        })
                    }
                    None => {
                        return Err(EvalError::Runtime(
                            "String.join: missing separator argument".into(),
                        ))
                    }
                };
                let parts: Vec<String> = list
                    .iter()
                    .map(|v| match v {
                        RuntimeValue::String(s) => Ok(s.clone()),
                        other => Err(EvalError::TypeMismatch {
                            op: "String.join element",
                            left: runtime_kind(other),
                            right: "String",
                        }),
                    })
                    .collect::<Result<Vec<_>, _>>()?;
                Ok(RuntimeValue::String(parts.join(&sep)))
            }
            "File.read" => match arg_vals.first() {
                Some(RuntimeValue::String(path)) => match std::fs::read_to_string(path) {
                    Ok(content) => Ok(RuntimeValue::String(content)),
                    Err(e) => Err(EvalError::Runtime(format!(
                        "File.read: cannot read '{}': {}",
                        path, e
                    ))),
                },
                Some(other) => Err(EvalError::TypeMismatch {
                    op: "File.read",
                    left: runtime_kind(other),
                    right: "String",
                }),
                None => Err(EvalError::Runtime(
                    "File.read: missing path argument".into(),
                )),
            },
            "File.write" => match (arg_vals.first(), arg_vals.get(1)) {
                (Some(RuntimeValue::String(path)), Some(RuntimeValue::String(content))) => {
                    match std::fs::write(path, content) {
                        Ok(()) => Ok(RuntimeValue::Int(content.len() as i64)),
                        Err(e) => Err(EvalError::Runtime(format!(
                            "File.write: cannot write '{}': {}",
                            path, e
                        ))),
                    }
                }
                (None, _) => Err(EvalError::Runtime(
                    "File.write: missing path argument".into(),
                )),
                (_, None) => Err(EvalError::Runtime(
                    "File.write: missing content argument".into(),
                )),
                (Some(p), Some(c)) => Err(EvalError::TypeMismatch {
                    op: "File.write",
                    left: runtime_kind(p),
                    right: runtime_kind(c),
                }),
            },
            "File.append" => match (arg_vals.first(), arg_vals.get(1)) {
                (Some(RuntimeValue::String(path)), Some(RuntimeValue::String(content))) => {
                    use std::io::Write;
                    match std::fs::OpenOptions::new()
                        .create(true)
                        .append(true)
                        .open(path)
                    {
                        Ok(mut f) => match f.write_all(content.as_bytes()) {
                            Ok(()) => Ok(RuntimeValue::Int(content.len() as i64)),
                            Err(e) => Err(EvalError::Runtime(format!(
                                "File.append: cannot append to '{}': {}",
                                path, e
                            ))),
                        },
                        Err(e) => Err(EvalError::Runtime(format!(
                            "File.append: cannot open '{}': {}",
                            path, e
                        ))),
                    }
                }
                (None, _) => Err(EvalError::Runtime(
                    "File.append: missing path argument".into(),
                )),
                (_, None) => Err(EvalError::Runtime(
                    "File.append: missing content argument".into(),
                )),
                (Some(p), Some(c)) => Err(EvalError::TypeMismatch {
                    op: "File.append",
                    left: runtime_kind(p),
                    right: runtime_kind(c),
                }),
            },
            "affirm" => {
                let cond = match arg_vals.first() {
                    Some(RuntimeValue::Bool(b)) => *b,
                    Some(other) => {
                        return Err(EvalError::TypeMismatch {
                            op: "affirm.cond",
                            left: runtime_kind(other),
                            right: "Bool",
                        })
                    }
                    None => {
                        return Err(EvalError::Runtime(
                            "affirm: missing condition argument".into(),
                        ))
                    }
                };
                if cond {
                    return Ok(RuntimeValue::Void);
                }
                let msg = match arg_vals.get(1) {
                    Some(RuntimeValue::String(s)) => s.clone(),
                    Some(other) => {
                        return Err(EvalError::TypeMismatch {
                            op: "affirm.msg",
                            left: runtime_kind(other),
                            right: "String",
                        })
                    }
                    None => {
                        return Err(EvalError::Runtime(
                            "affirm: missing message argument".into(),
                        ))
                    }
                };
                Err(EvalError::Runtime(format!("assertion failed: {}", msg)))
            }
            "args" => {
                if !arg_vals.is_empty() {
                    return Err(EvalError::Runtime(format!(
                        "args: takes no arguments, got {}",
                        arg_vals.len()
                    )));
                }
                let list: Vec<RuntimeValue> = self
                    .command_line_args
                    .iter()
                    .map(|s| RuntimeValue::String(s.clone()))
                    .collect();
                Ok(RuntimeValue::List(list))
            }
            "Int.to_string" => {
                let n = match arg_vals.first() {
                    Some(RuntimeValue::Int(n)) => *n,
                    Some(other) => {
                        return Err(EvalError::TypeMismatch {
                            op: "Int.to_string",
                            left: runtime_kind(other),
                            right: "Int",
                        })
                    }
                    None => {
                        return Err(EvalError::Runtime("Int.to_string: missing argument".into()))
                    }
                };
                Ok(RuntimeValue::String(n.to_string()))
            }
            "String.to_int" => {
                let s = match arg_vals.first() {
                    Some(RuntimeValue::String(s)) => s.clone(),
                    Some(other) => {
                        return Err(EvalError::TypeMismatch {
                            op: "String.to_int",
                            left: runtime_kind(other),
                            right: "String",
                        })
                    }
                    None => {
                        return Err(EvalError::Runtime("String.to_int: missing argument".into()))
                    }
                };
                match s.parse::<i64>() {
                    Ok(n) => Ok(RuntimeValue::Option(Some(Box::new(RuntimeValue::Int(n))))),
                    Err(_) => Ok(RuntimeValue::Option(None)),
                }
            }
            "Map.insert" => {
                // ─── Structural receiver access ───
                // Evaluating args[0] would clone the map, so mutation would
                // be lost. Instead, look at it structurally: the IR builder
                // always emits `Variable(name)` as the receiver of a Map
                // method call, so we can grab the name and mutate
                // `self.variables[name]` in place. Same shape as
                // `Instruction::FieldAssign`.
                let receiver_name = match args.first() {
                    Some(TypedIRValue::Variable(name, _)) => name.clone(),
                    _ => {
                        return Err(EvalError::Unsupported {
                            construct: "Map.insert on non-variable receiver",
                            hint:
                                "the IR builder always emits a Variable as the Map.insert receiver",
                        });
                    }
                };

                if args.len() != 3 {
                    return Err(EvalError::Runtime(format!(
                        "Map.insert expects 3 arguments (receiver, key, value), got {}",
                        args.len()
                    )));
                }

                // Evaluate key and value before borrowing self.variables mutably.
                let k_val = self.eval_value(&args[1])?;
                let v_val = self.eval_value(&args[2])?;
                let key = MapKey::from_runtime(&k_val)?;

                let receiver = self.variables.get_mut(&receiver_name).ok_or_else(|| {
                    EvalError::Runtime(format!(
                        "Map.insert target `{}` not found at runtime",
                        receiver_name
                    ))
                })?;

                match receiver {
                    RuntimeValue::Map(entries) => {
                        entries.insert(key, v_val);
                        Ok(RuntimeValue::Void)
                    }
                    other => Err(EvalError::TypeMismatch {
                        op: "Map.insert",
                        left: runtime_kind(other),
                        right: "Map",
                    }),
                }
            }

            "Map.get" => {
                let receiver = self.eval_value(&args[0])?;
                let k_val = self.eval_value(&args[1])?;
                let key = MapKey::from_runtime(&k_val)?;
                match receiver {
                    RuntimeValue::Map(entries) => match entries.get(&key) {
                        Some(v) => Ok(RuntimeValue::Option(Some(Box::new(v.clone())))),
                        None => Ok(RuntimeValue::Option(None)),
                    },
                    other => Err(EvalError::TypeMismatch {
                        op: "Map.get",
                        left: runtime_kind(&other),
                        right: "Map",
                    }),
                }
            }

            "Map.contains" => {
                let receiver = self.eval_value(&args[0])?;
                let k_val = self.eval_value(&args[1])?;
                let key = MapKey::from_runtime(&k_val)?;
                match receiver {
                    RuntimeValue::Map(entries) => {
                        Ok(RuntimeValue::Bool(entries.contains_key(&key)))
                    }
                    other => Err(EvalError::TypeMismatch {
                        op: "Map.contains",
                        left: runtime_kind(&other),
                        right: "Map",
                    }),
                }
            }

            "Map.keys" => {
                let receiver = self.eval_value(&args[0])?;
                match receiver {
                    RuntimeValue::Map(entries) => {
                        // Collect owned keys so the match below binds by value
                        // (no `*` vs `&` ambiguity) and sorts deterministically.
                        let mut keys: Vec<MapKey> = entries.keys().cloned().collect();
                        keys.sort();
                        let out: Vec<RuntimeValue> = keys
                            .into_iter()
                            .map(|k| match k {
                                MapKey::Int(i) => RuntimeValue::Int(i),
                                MapKey::String(s) => RuntimeValue::String(s),
                                MapKey::Bool(b) => RuntimeValue::Bool(b),
                            })
                            .collect();
                        Ok(RuntimeValue::List(out))
                    }
                    other => Err(EvalError::TypeMismatch {
                        op: "Map.keys",
                        left: runtime_kind(&other),
                        right: "Map",
                    }),
                }
            }

            "Map.values" => {
                let receiver = self.eval_value(&args[0])?;
                match receiver {
                    RuntimeValue::Map(entries) => {
                        // Sort by key so `.keys()` and `.values()` are aligned
                        // — `zip(m.keys(), m.values())` gives (k, v) pairs.
                        let mut sorted: Vec<(&MapKey, &RuntimeValue)> = entries.iter().collect();
                        sorted.sort_by(|a, b| a.0.cmp(b.0));
                        let out: Vec<RuntimeValue> =
                            sorted.into_iter().map(|(_, v)| v.clone()).collect();
                        Ok(RuntimeValue::List(out))
                    }
                    other => Err(EvalError::TypeMismatch {
                        op: "Map.values",
                        left: runtime_kind(&other),
                        right: "Map",
                    }),
                }
            }

            "Map.length" => {
                let receiver = self.eval_value(&args[0])?;
                match receiver {
                    RuntimeValue::Map(entries) => Ok(RuntimeValue::Int(entries.len() as i64)),
                    other => Err(EvalError::TypeMismatch {
                        op: "Map.length",
                        left: runtime_kind(&other),
                        right: "Map",
                    }),
                }
            }
            "List.append" => {
                // Same structural-receiver pattern as Map.insert: evaluating
                // args[0] would clone the list, so mutation would be lost.
                let receiver_name = match args.first() {
                    Some(TypedIRValue::Variable(name, _)) => name.clone(),
                    _ => {
                        return Err(EvalError::Unsupported {
                            construct: "List.append on non-variable receiver",
                            hint:
                                "the IR builder always emits a Variable as the List.append receiver",
                        });
                    }
                };

                if args.len() != 2 {
                    return Err(EvalError::Runtime(format!(
                        "List.append expects 2 arguments (receiver, value), got {}",
                        args.len()
                    )));
                }

                let val = self.eval_value(&args[1])?;
                let receiver = self.variables.get_mut(&receiver_name).ok_or_else(|| {
                    EvalError::Runtime(format!(
                        "List.append target `{}` not found at runtime",
                        receiver_name
                    ))
                })?;

                match receiver {
                    RuntimeValue::List(items) => {
                        items.push(val);
                        Ok(RuntimeValue::Void)
                    }
                    other => Err(EvalError::TypeMismatch {
                        op: "List.append",
                        left: runtime_kind(other),
                        right: "List",
                    }),
                }
            }
            _ => Err(EvalError::Unsupported {
                construct: "builtin",
                hint: "interpreter has no dispatch arm for this registered \
                       builtin — the IR verifier accepted it, but the \
                       interpreter cannot execute it. This is an interpreter \
                       completeness gap.",
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

            // Wrap the callee's error with the function name so
            // nested calls produce a readable chain
            // (`in f: in g: ...`). The error type is `EvalError`
            // throughout; nothing is swallowed.
            result.map_err(|e| EvalError::Runtime(format!("in {}: {}", callee.name, e)))?;

            Ok(ret)
        } else {
            self.eval_builtin_call(function, args)
        }
    }
}
