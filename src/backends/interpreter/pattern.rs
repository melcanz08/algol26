// src/backends/interpreter/pattern.rs

use super::runtime::{EvalError, RuntimeValue};
use super::Interpreter;
use crate::ir::semantic_ir::SemanticPattern;

impl Interpreter {
    /// Match `value` against `pattern`. On success, return the list of
    /// variable bindings the pattern introduces.
    pub(super) fn try_pattern_match(
        &mut self,
        pattern: &SemanticPattern,
        value: &RuntimeValue,
    ) -> Result<Option<Vec<(String, RuntimeValue)>>, EvalError> {
        match pattern {
            SemanticPattern::Wildcard => Ok(Some(Vec::new())),

            SemanticPattern::Some { binding } => match value {
                RuntimeValue::Option(Some(v)) => Ok(Some(vec![(binding.clone(), (**v).clone())])),
                _ => Ok(None),
            },

            SemanticPattern::None => match value {
                RuntimeValue::Option(None) => Ok(Some(Vec::new())),
                _ => Ok(None),
            },

            SemanticPattern::Ok { binding } => match value {
                RuntimeValue::Result {
                    is_ok: true,
                    value: v,
                } => Ok(Some(vec![(binding.clone(), (**v).clone())])),
                _ => Ok(None),
            },

            SemanticPattern::Error { binding } => match value {
                RuntimeValue::Result {
                    is_ok: false,
                    value: v,
                } => Ok(Some(vec![(binding.clone(), (**v).clone())])),
                _ => Ok(None),
            },

            SemanticPattern::Literal(lit) => {
                let lit_val = self.eval_value(lit)?;
                if lit_val.display() == value.display() {
                    Ok(Some(Vec::new()))
                } else {
                    Ok(None)
                }
            }

            SemanticPattern::Record { name, bindings } => match value {
                RuntimeValue::Record { name: rn, fields } if rn == name => {
                    let mut out = Vec::with_capacity(bindings.len());
                    for b in bindings {
                        match fields.iter().find(|(n, _)| n == b) {
                            Some((_, v)) => out.push((b.clone(), v.clone())),
                            None => return Ok(None),
                        }
                    }
                    Ok(Some(out))
                }
                _ => Ok(None),
            },

            // ADR 0030: enum variant match. An enum value at runtime
            // is its ordinal (an Int). The IR builder resolved the
            // ordinal from the matched type's `Type::Enum` at
            // translation time, so this is a plain integer compare.
            SemanticPattern::Variant { ordinal, .. } => match value {
                RuntimeValue::Int(n) if *n == *ordinal => Ok(Some(Vec::new())),
                _ => Ok(None),
            },

            // ADR 0055 Session 3. Union variant pattern. The
            // runtime value is `RuntimeValue::Union { tag, payload,
            // .. }`. Match on tag; if it fires, bind each payload
            // element to its corresponding name. Binding is by
            // value (the ADR's design decision) — payload elements
            // are cloned, not moved.
            // ADR 0055 Session 3 follow-up. A guarded pattern:
            // match the inner pattern first, then evaluate the
            // condition with the payload bindings temporarily in
            // scope. On guard success, return the bindings so the
            // caller's arm body sees them; on failure, return None
            // so the switch falls through to the next case.
            SemanticPattern::Guarded { inner, condition } => {
                let bindings = match self.try_pattern_match(inner, value)? {
                    Some(b) => b,
                    None => return Ok(None),
                };
                let mut saved: Vec<(String, Option<RuntimeValue>)> = Vec::new();
                for (name, val) in &bindings {
                    saved.push((
                        name.clone(),
                        self.variables.insert(name.clone(), val.clone()),
                    ));
                }
                let cond_result = self.eval_value(condition);
                for (name, old) in saved {
                    match old {
                        Some(v) => {
                            self.variables.insert(name, v);
                        }
                        None => {
                            self.variables.remove(&name);
                        }
                    }
                }
                let cond = cond_result?;
                match cond {
                    RuntimeValue::Bool(true) => Ok(Some(bindings)),
                    RuntimeValue::Bool(false) => Ok(None),
                    other => Err(EvalError::TypeMismatch {
                        op: "Guarded condition",
                        left: super::runtime::runtime_kind(&other),
                        right: "Bool",
                    }),
                }
            }
            SemanticPattern::VariantWithPayload { tag, bindings, .. } => match value {
                RuntimeValue::Union {
                    tag: value_tag,
                    payload,
                    ..
                } if *value_tag == *tag => {
                    if bindings.len() != payload.len() {
                        return Err(EvalError::Runtime(format!(
                            "internal: VariantWithPayload expects {} binding(s), \
                             runtime value has {} payload element(s)",
                            bindings.len(),
                            payload.len()
                        )));
                    }
                    let mut out = Vec::with_capacity(bindings.len());
                    for (name, val) in bindings.iter().zip(payload.iter()) {
                        out.push((name.clone(), val.clone()));
                    }
                    Ok(Some(out))
                }
                _ => Ok(None),
            },
        }
    }
}
