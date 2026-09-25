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
        }
    }
}
