// src/semantics/builder/values.rs

use super::*;

impl SemanticIRBuilder {
    /// Resolve `receiver.method` to the mangled name that actually exists
    /// in `function_types`. Tries both `Type.method` (built-ins) and
    /// `Type_method` (impl-derived names).
    pub(super) fn resolve_method_call(
        &self,
        receiver_type: &Type,
        method_name: &str,
    ) -> Option<String> {
        // Records have no builtin base name, so try the impl-mangled
        // form directly: `expand_impl_methods` renames `show` on
        // `Sale` to `Sale_show`, which is registered in
        // `function_types` just like a builtin. Fall through to the
        // generic path only if that misses.
        if let Type::Record(name, _) = receiver_type {
            let mangled = format!("{}_{}", name, method_name);
            if self.function_types.contains_key(&mangled) {
                return Some(mangled);
            }
            return None;
        }

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
            && value_type != *target
            && value_type.can_coerce_to(target)
            && value_type.can_cast_to(target)
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
            Stmt::Break(_) | Stmt::Continue(_) | Stmt::Defer { .. } => true,
            Stmt::Expression(expr) => Self::expr_has_complex_cf(expr),
            Stmt::Spawn { body, .. }
            | Stmt::RegionBlock { body, .. }
            | Stmt::UnsafeBlock { body, .. } => body.iter().any(Self::stmt_has_complex_cf),
            Stmt::Parallel { blocks, .. } => blocks
                .iter()
                .any(|b| b.iter().any(Self::stmt_has_complex_cf)),
            _ => false,
        }
    }

    #[allow(dead_code)]
    pub(super) fn expr_has_complex_cf(expr: &Expr) -> bool {
        match &expr.kind {
            ExprKind::Block {
                statements,
                trailing_expr,
                ..
            } => {
                statements.iter().any(Self::stmt_has_complex_cf)
                    || trailing_expr
                        .as_ref()
                        .is_some_and(|e| Self::expr_has_complex_cf(e))
            }
            ExprKind::If {
                then_branch,
                else_branch,
                ..
            } => {
                Self::expr_has_complex_cf(then_branch)
                    || else_branch
                        .as_ref()
                        .is_some_and(|e| Self::expr_has_complex_cf(e))
            }
            ExprKind::Match { cases, .. } => {
                cases.iter().any(|c| Self::expr_has_complex_cf(&c.body))
            }
            ExprKind::TryCatch {
                try_branch,
                catch_branch,
                ..
            } => Self::expr_has_complex_cf(try_branch) || Self::expr_has_complex_cf(catch_branch),
            ExprKind::For {
                body,
                trailing_expr,
                ..
            } => {
                body.iter().any(Self::stmt_has_complex_cf)
                    || trailing_expr
                        .as_ref()
                        .is_some_and(|e| Self::expr_has_complex_cf(e))
            }
            ExprKind::While {
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
    /// Extract the runtime ordinal from a constant set element value.
    ///
    /// Set elements reach the IR builder wrapped in one or more `Cast`
    /// nodes: `Day.Saturday` becomes `Cast { value: Int(5), target_type: Day }`,
    /// and `Byte(7)` (where `Byte = Int in 0..63`) becomes
    /// `Cast { value: Int(7), target_type: Byte }`. This unwraps those
    /// chains to the underlying integer.
    ///
    /// Returns `None` for non-constant values (`Variable`, `Call`, etc.);
    /// those are rejected by the caller for now (runtime set construction
    /// is a later sub-phase).
    pub(super) fn extract_set_element_ordinal(v: &TypedIRValue) -> Option<i64> {
        match v {
            TypedIRValue::Int(n) => Some(*n),
            TypedIRValue::Bool(b) => Some(if *b { 1 } else { 0 }),
            TypedIRValue::Cast { value, .. } => Self::extract_set_element_ordinal(value),
            _ => None,
        }
    }
}
