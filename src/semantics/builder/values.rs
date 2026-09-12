// src/semantics/semantic_builder/values.rs

use super::*;

impl SemanticIRBuilder {
    /// Resolve `receiver.method` to the mangled name that actually exists
    /// in `function_types`. Tries both `Type.method` (built-ins) and
    /// `Type_method` (impl-derived names).
    pub(super) fn resolve_method_call(&self, receiver_type: &Type, method_name: &str) -> Option<String> {
        let base = Self::base_type_name(receiver_type)?;

        // Dot form — matches Math.sqrt, String.length, List.sum, File.read, …
        let dot_form = format!("{}.{}", base, method_name);
        if self.function_types.contains_key(&dot_form) {
            return Some(dot_form);
        }

        // Underscore form — matches names produced by expand_impl_methods.
        let underscore_form = format!("{}_{}", base, method_name);
        if self.function_types.contains_key(&underscore_form) {
            return Some(underscore_form);
        }

        None
    }
    pub(super) fn coerce_value(&self, value: TypedIRValue, target: &Type) -> TypedIRValue {
        let value_type = value.type_of();
        if value_type != Type::Unknown
            && *target != Type::Unknown
            && value_type.can_coerce_to(target)
            && value_type != *target
        {
            TypedIRValue::Cast {
                value: Box::new(value),
                target_type: target.clone(),
            }
        } else {
            value
        }
    }
    #[allow(dead_code)]
    pub(super) fn compile_pattern_match(&self, pattern: &Pattern, value: &TypedIRValue) -> bool {
        match pattern {
            Pattern::Some(_) => matches!(value, TypedIRValue::Some(_)),
            Pattern::SomeNested(inner) => match value {
                TypedIRValue::Some(v) => self.compile_pattern_match(inner, v),
                _ => false,
            },
            Pattern::OkNested(inner) => match value {
                TypedIRValue::Ok { value: v, .. } => self.compile_pattern_match(inner, v),
                _ => false,
            },
            Pattern::ErrorNested(inner) => match value {
                TypedIRValue::Error { value: v, .. } => self.compile_pattern_match(inner, v),
                _ => false,
            },
            Pattern::Guarded { pattern, condition } => {
                self.compile_pattern_match(pattern, value) && self.evaluate_guard(condition)
            }
            _ => true,
        }
    }
    #[allow(dead_code)]
    pub(super) fn evaluate_guard(&self, _condition: &Expr) -> bool {
        true
    }
    #[allow(dead_code)]
    pub(super) fn stmt_has_complex_cf(stmt: &Stmt) -> bool {
        match stmt {
            Stmt::Break | Stmt::Continue | Stmt::Defer { .. } => true,
            Stmt::Expression(expr) => Self::expr_has_complex_cf(expr),
            Stmt::Spawn { body } | Stmt::RegionBlock { body, .. } | Stmt::UnsafeBlock { body } => {
                body.iter().any(Self::stmt_has_complex_cf)
            }
            Stmt::Parallel { blocks } => blocks
                .iter()
                .any(|b| b.iter().any(Self::stmt_has_complex_cf)),
            _ => false,
        }
    }
    #[allow(dead_code)]
    pub(super) fn expr_has_complex_cf(expr: &Expr) -> bool {
        match expr {
            Expr::Block {
                statements,
                trailing_expr,
            } => {
                statements.iter().any(Self::stmt_has_complex_cf)
                    || trailing_expr
                        .as_ref()
                        .is_some_and(|e| Self::expr_has_complex_cf(e))
            }
            Expr::If {
                then_branch,
                else_branch,
                ..
            } => {
                Self::expr_has_complex_cf(then_branch)
                    || else_branch
                        .as_ref()
                        .is_some_and(|e| Self::expr_has_complex_cf(e))
            }
            Expr::Match { cases, .. } => cases.iter().any(|c| Self::expr_has_complex_cf(&c.body)),
            Expr::TryCatch {
                try_branch,
                catch_branch,
                ..
            } => Self::expr_has_complex_cf(try_branch) || Self::expr_has_complex_cf(catch_branch),
            Expr::For {
                body,
                trailing_expr,
                ..
            } => {
                body.iter().any(Self::stmt_has_complex_cf)
                    || trailing_expr
                        .as_ref()
                        .is_some_and(|e| Self::expr_has_complex_cf(e))
            }
            Expr::While {
                body,
                trailing_expr,
                ..
            } => {
                body.iter().any(Self::stmt_has_complex_cf)
                    || trailing_expr
                        .as_ref()
                        .is_some_and(|e| Self::expr_has_complex_cf(e))
            }
            _ => false,
        }
    }
}