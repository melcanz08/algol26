// src/semantics/analyzer/stmt.rs

use super::*;

impl SemanticAnalyzer {
    pub(super) fn analyze_stmt(&mut self, stmt: &Stmt) -> Result<()> {
        self.in_mut_borrow = false;
        match stmt {
            Stmt::VarDecl { name, value, type_annotation, mutable, .. } => {
                // Detect mut-borrow before analyzing so we can set the "allow
                // read during this declaration" flag.
                let mut_borrow_source: Option<String> = if let Expr::MutBorrow { expr } = value {
                    if let Expr::Var(source_name, _) = expr.as_ref() {

                        Some(source_name.clone())
                    } else {
                        None
                    }
                } else {
                    None
                };

                if mut_borrow_source.is_some() {
                    self.in_mut_borrow = true;
                }

                if let Expr::List(elements) = value {
                    self.declare_list_length(name, elements.len());
                    self.declare_list_values(name, elements.clone());
                } else if let Expr::Var(source, _) = value {
                    if let Some(len) = self.lookup_list_length(source) {
                        self.declare_list_length(name, len);
                    }
                    if let Some(vals) = self.lookup_list_values(source) {
                        self.declare_list_values(name, vals);
                    }
                }

                // Analyze the value FIRST. If it's a mut-borrow,
                // `check_borrow_rules` inside `Expr::MutBorrow` will fire if
                // the source is already mutably borrowed.
                let value_type = if let Some(annotated) = type_annotation {
                    let expected = Type::from_str(annotated);
                    self.analyze_expr_with_context(value, Some(&expected))?
                } else {
                    self.analyze_expr(value)?
                };

                // NOW register the borrow. Doing this after analysis means
                // the first declaration of `&mut x` sets the flag; the second
                // declaration sees the flag and errors out.
                if let Some(source) = &mut_borrow_source {
                    self.register_mutable_borrow(name, source)?;
                }

                if let Some(annotated) = type_annotation {
                    let expected = Type::from_str(annotated);
                    let is_borrow = matches!(value, Expr::Borrow { .. } | Expr::MutBorrow { .. });
                    if !is_borrow
                        && expected != Type::Unknown
                        && !value_type.can_coerce_to(&expected)
                    {
                        return Err(CompileError::simple(
                            &format!(
                                "Type mismatch: variable '{}' declared as {} but assigned {}",
                                name, expected, value_type
                            ),
                            0, 0, "", ErrorCode::E0002,
                        ).with_suggestion(&format!(
                            "Change the type annotation to {} or change the value to {}",
                            value_type, expected
                        )));
                    }
                }
            
                self.declare_variable(name, value_type.clone(), *mutable)?;
                // A `val` bound to `null` is statically known to hold
                // null forever. Record it so a later deref can be
                // rejected at compile time.
                if !*mutable && matches!(value, Expr::NullPtr) {
                    if let Some(scope) = self.null_bindings.last_mut() {
                        scope.insert(name.clone());
                    }
                }
                self.in_mut_borrow = false;

                if let Expr::Var(source, _) = value {
                    if let Some(scope) = self.deferred_captures.last() {
                        if scope.contains(source) {
                            return Err(CompileError::simple(
                                &format!("Cannot move '{}' after it was captured by defer", source),
                                0, 0, "", ErrorCode::E0007,
                            ).with_suggestion(
                                "Deferred statements capture variables at declaration time",
                            ));
                        }
                    }
                    if source != name && !self.is_moved(source) && !value_type.is_copy() {
                        self.mark_moved(source);
                    }
                }
            }
            Stmt::Assign { name, value } => {
                let (var_type, _mutable) = self.lookup_variable(name).ok_or_else(|| {
                    CompileError::simple(
                        &format!("Undefined variable '{}'", name),
                        0, 0, "", ErrorCode::E0003,
                    ).with_suggestion(&format!(
                        "Declare '{}' with 'var {} := ...' or 'val {} := ...'",
                        name, name, name
                    ))
                })?;

                let target_type = match &var_type {
                    Type::MutBorrow(inner) => (**inner).clone(),
                    _ => var_type.clone(),
                };

                let value_type = self.analyze_expr_with_context(value, Some(&target_type))?;
                if target_type != value_type
                    && target_type != Type::Unknown
                    && !value_type.can_coerce_to(&target_type)
                {
                    return Err(CompileError::simple(
                        &format!(
                            "Type mismatch: cannot assign {} to variable of type {}",
                            value_type, target_type
                        ),
                        0, 0, "", ErrorCode::E0002,
                    ).with_suggestion(&format!(
                        "Change the value to {} or declare variable as {}",
                        target_type, value_type
                    )));
                }
                if self.mutable_borrows.iter().any(|m| m.contains_key(name)) {
                    self.release_mutable_borrow(name);
                }
            }
            Stmt::Expression(expr) => {
                match expr {
                    Expr::If { then_branch, else_branch, condition } => {
                        let cond_type = self.analyze_expr(condition)?;
                        if cond_type != Type::Bool && cond_type != Type::Unknown {
                            return Err(CompileError::simple(
                                "If condition must be Bool", 0, 0, "", ErrorCode::E0002,
                            ));
                        }
                        let moved_before = self.moved_vars.last().cloned().unwrap_or_default();

                        self.push_scope();
                        let then_result = self.analyze_expr(then_branch);
                        let moved_after_then = self.moved_vars.last().cloned().unwrap_or_default();
                        self.pop_scope();
                        then_result?;

                        let moved_after_else = if let Some(else_expr) = else_branch {
                            self.push_scope();
                            let else_result = self.analyze_expr(else_expr);
                            let moved_after = self.moved_vars.last().cloned().unwrap_or_default();
                            self.pop_scope();
                            else_result?;
                            moved_after
                        } else {
                            moved_before.clone()
                        };

                        if let Some(current_scope) = self.moved_vars.last_mut() {
                            for var in &moved_after_then {
                                if !current_scope.contains(var) {
                                    current_scope.push(var.clone());
                                }
                            }
                            for var in &moved_after_else {
                                if !current_scope.contains(var) {
                                    current_scope.push(var.clone());
                                }
                            }
                        }
                    }
                    Expr::Match { .. } | Expr::TryCatch { .. } => {
                        self.push_scope();
                        let result = self.analyze_expr(expr);
                        self.pop_scope();
                        result?;
                    }
                    Expr::For { .. } | Expr::While { .. } => {
                        self.analyze_expr(expr)?;
                    }
                    _ => {
                        self.analyze_expr(expr)?;
                    }
                }
            }
            Stmt::Return { value } => {
                let expected_type = self.current_return_type.clone().unwrap_or(Type::Void);
                match (value, &expected_type) {
                    (Some(_expr), Type::Void) => {
                        return Err(CompileError::simple(
                            "Cannot return a value from a void function",
                            0, 0, "", ErrorCode::E0002,
                        ).with_suggestion(
                            "Remove the return value or change the function return type",
                        ));
                    }
                    (None, Type::Void) => {}
                    (Some(expr), expected) => {
                        let actual_type = self.analyze_expr_with_context(expr, Some(expected))?;
                        let can_return = actual_type.can_coerce_to(expected)
                            || matches!(&actual_type, Type::Borrow(inner) if (**inner).can_coerce_to(expected))
                            || matches!(&actual_type, Type::MutBorrow(inner) if (**inner).can_coerce_to(expected));
                        if !can_return && *expected != Type::Unknown {
                            return Err(CompileError::simple(
                                &format!(
                                    "Return type mismatch: expected {}, found {}",
                                    expected, actual_type
                                ),
                                0, 0, "", ErrorCode::E0002,
                            ).with_suggestion(&format!(
                                "Change the return statement to match {} or change the function signature",
                                expected
                            )));
                        }
                    }
                    (None, expected) => {
                        return Err(CompileError::simple(
                            &format!("Missing return value: function should return {}", expected),
                            0, 0, "", ErrorCode::E0002,
                        ).with_suggestion("Add a return statement with the appropriate value"));
                    }
                }
            }
            Stmt::Print { expr } => { self.analyze_expr(expr)?; }
            Stmt::Break | Stmt::Continue => {}
            Stmt::Defer { stmt } => {
                let mut captured = HashSet::new();
                self.collect_deferred_captures(stmt, &mut captured);
                if let Some(scope) = self.deferred_captures.last_mut() {
                    for var in &captured {
                        scope.insert(var.clone());
                    }
                }
                self.analyze_stmt(stmt)?;
            }
            Stmt::Spawn { body } => {
                self.push_scope();
                for s in body { self.analyze_stmt(s)?; }
                self.pop_scope();
            }
            Stmt::Parallel { blocks } => {
                for block in blocks {
                    self.push_scope();
                    for s in block { self.analyze_stmt(s)?; }
                    self.pop_scope();
                }
            }
            Stmt::ChannelDecl { name } => {
                self.declare_variable(name, Type::channel(Type::Unknown), false)?;
            }
            Stmt::Send { channel, value } => {
                let _ = self.lookup_variable(channel).ok_or_else(|| {
                    CompileError::simple(
                        &format!("Undefined channel '{}'", channel),
                        0, 0, "", ErrorCode::E0003,
                    )
                })?;
                self.analyze_expr(value)?;
            }
            Stmt::Receive { channel, target } => {
                let _ = self.lookup_variable(channel).ok_or_else(|| {
                    CompileError::simple(
                        &format!("Undefined channel '{}'", channel),
                        0, 0, "", ErrorCode::E0003,
                    )
                })?;
                if !target.is_empty() {
                    if let Some((Type::Channel(element_type), _)) = self.lookup_variable(channel) {
                        self.declare_variable(target, *element_type, false)?;
                    }
                }
            }
            Stmt::UnsafeBlock { body } => {
                self.push_scope();
                for s in body { self.analyze_stmt(s)?; }
                self.pop_scope();
            }
            Stmt::RegionBlock { name: _, body } => {
                self.push_scope();
                for s in body { self.analyze_stmt(s)?; }
                self.pop_scope();
            }
            Stmt::Import { .. } => {}
            Stmt::ArrayAssign { array, index, value } => {
                // Mirrors the checks in `Expr::ArrayAccess`, which were
                // added during the hardening pass but never propagated to
                // the write path. Without these, `xs[1.5] := 99` and
                // `xs[-1] := 99` compile silently.
                let (array_type, _) = self.lookup_variable(array).ok_or_else(|| {
                    CompileError::simple(
                        &format!("Undefined array '{}'", array),
                        0, 0, "", ErrorCode::E0003,
                    )
                })?;

                let element_type = match &array_type {
                    Type::List(elem) => (**elem).clone(),
                    Type::Unknown => Type::Unknown,
                    other => {
                        return Err(CompileError::simple(
                            &format!("Array assignment requires list, found {}", other),
                            0, 0, "", ErrorCode::E0002,
                        ));
                    }
                };

                // Analyze the index and check its type.
                let index_type = self.analyze_expr(index)?;

                if index_type != Type::Int && index_type != Type::Unknown {
                    return Err(CompileError::simple(
                        &format!("Array index must be Int, found {}", index_type),
                        0, 0, "", ErrorCode::E0002,
                    ).with_suggestion(&format!(
                        "Use an Int index or convert {} with int({})",
                        index_type, index_type
                    )));
                }

                // Literal index: bounds-check against known list length.
                let literal_index: Option<i64> = match index {
                    Expr::Int(v) => Some(*v),
                    Expr::Number(f) => Some(*f as i64),
                    _ => None,
                };
                if let Some(idx_val) = literal_index {
                    if let Some(list_len) = self.lookup_list_length(array) {
                        if idx_val < 0 || (idx_val as usize) >= list_len {
                            return Err(CompileError::simple(
                                &format!(
                                    "Array index out of bounds: index {} is out of bounds for '{}' with length {}",
                                    idx_val, array, list_len
                                ),
                                0, 0, "", ErrorCode::E0004,
                            ).with_suggestion(&format!(
                                "Valid indices are 0..{} for array of length {}",
                                list_len - 1, list_len
                            )));
                        }
                    }
                }

                // Value type must be assignable to the element type.
                let value_type = self.analyze_expr(value)?;
                if element_type != Type::Unknown
                    && value_type != Type::Unknown
                    && !value_type.can_coerce_to(&element_type)
                {
                    return Err(CompileError::simple(
                        &format!(
                            "Array assignment type mismatch: '{}' has element type {}, but value is {}",
                            array, element_type, value_type
                        ),
                        0, 0, "", ErrorCode::E0002,
                    ).with_suggestion(&format!(
                        "Assign a value of type {} to elements of '{}'",
                        element_type, array
                    )));
                }
            }
        }
        Ok(())
    }
    pub(super) fn bind_pattern_variables(&mut self, pattern: &Pattern, value_type: &Type) {
        match pattern {
            Pattern::Binding(var) => {
                self.declare_variable(var, value_type.clone(), false).ok();
            }
            Pattern::Some(var) => {
                if let Type::Option(inner) = value_type {
                    self.declare_variable(var, *inner.clone(), false).ok();
                } else {
                    self.declare_variable(var, Type::Unknown, false).ok();
                }
            }
            Pattern::SomeNested(inner) => {
                if let Type::Option(inner_type) = value_type {
                    self.bind_pattern_variables(inner, inner_type);
                }
            }
            Pattern::Ok(var) => {
                if let Type::Result { ok, .. } = value_type {
                    self.declare_variable(var, *ok.clone(), false).ok();
                } else {
                    self.declare_variable(var, Type::Unknown, false).ok();
                }
            }
            Pattern::OkNested(inner) => {
                if let Type::Result { ok, .. } = value_type {
                    self.bind_pattern_variables(inner, ok);
                }
            }
            Pattern::Error(var) => {
                if let Type::Result { error, .. } = value_type {
                    self.declare_variable(var, *error.clone(), false).ok();
                } else {
                    self.declare_variable(var, Type::Unknown, false).ok();
                }
            }
            Pattern::ErrorNested(inner) => {
                if let Type::Result { error, .. } = value_type {
                    self.bind_pattern_variables(inner, error);
                }
            }
            Pattern::Guarded { pattern, .. } => {
                self.bind_pattern_variables(pattern, value_type);
            }
            _ => {}
        }
    }
}