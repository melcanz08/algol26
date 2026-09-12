// src/backends/interpreter/pattern.rs

use super::Interpreter;
use super::runtime::RuntimeValue;

impl Interpreter {
    /// Match `value` against `pattern`. On success, return the list of
    /// variable bindings the pattern introduces.
    pub(super) fn try_pattern_match(
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
}