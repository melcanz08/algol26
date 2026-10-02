// src/semantics/analyzer/expr.rs

use super::*;

impl SemanticAnalyzer {
    pub(super) fn analyze_expr(&mut self, expr: &Expr) -> Result<Type> {
        self.analyze_expr_with_context(expr, None)
    }

    // ─── UNIFY TYPES ───────────────────────────────────────────────────────
    // Public entry: calls the inner analyzer and records the resulting type.
    pub(super) fn analyze_expr_with_context(
        &mut self,
        expr: &Expr,
        expected_type: Option<&Type>,
    ) -> Result<Type> {
        // Record the current node's span for error sites that don't
        // have the node in hand. No save/restore — the innermost
        // error site should use the innermost node's span.
        self.current_span = expr.span();
        let ty = self.analyze_expr_inner(expr, expected_type)?;
        self.type_table_id.insert(expr.id, ty.clone());
        Ok(ty)
    }

    // The actual match arm dispatch (renamed from the original).
    pub(super) fn analyze_expr_inner(
        &mut self,
        expr: &Expr,
        expected_type: Option<&Type>,
    ) -> Result<Type> {
        match &expr.kind {
            ExprKind::Borrow { expr, .. } => {
                if let ExprKind::Var(name, _) = &expr.as_ref().kind {
                    self.check_borrow_rules(name, false)?;
                    self.mark_borrowed(name);
                    let inner_type = self.analyze_expr(expr)?;
                    return Ok(Type::borrow(inner_type));
                }
                let inner_type = self.analyze_expr(expr)?;
                Ok(Type::borrow(inner_type))
            }
            ExprKind::MutBorrow { expr, .. } => {
                if let ExprKind::Var(name, _) = &expr.as_ref().kind {
                    // Do NOT release existing borrows here — that would undo
                    // the very borrow we just registered. `check_borrow_rules`
                    // will correctly reject a second mut-borrow of the same source.
                    self.check_borrow_rules(name, true)?;
                    // Mirror the Borrow arm: analyze the inner expression so its
                    // type lands in the ExprId-keyed table. Returning a looked-up
                    // type directly skips the table write and leaves `x` untyped
                    // in `&mut x`, which the completeness check now rejects.
                    let inner_type = self.analyze_expr(expr)?;
                    return Ok(Type::mut_borrow(inner_type));
                }
                let inner_type = self.analyze_expr(expr)?;
                Ok(Type::mut_borrow(inner_type))
            }
            ExprKind::Deref { expr, .. } => {
                // Rule: dereferencing a value statically known to be
                // null is a compile-time safety error. The language
                // permits `null` as a value of type `Ptr`; it does not
                // permit a deref whose operand is provably null.
                if matches!(&expr.as_ref().kind, ExprKind::NullPtr(_)) {
                    return Err(CompileError::at(
                        self.current_span,
                        "Cannot dereference a null pointer",
                        ErrorCode::E0007,
                    )
                    .with_suggestion(
                        "Check the pointer for null before dereferencing, \
                         e.g. with `if p != null then ...`",
                    ));
                }
                if let ExprKind::Var(name, _) = &expr.as_ref().kind {
                    let is_known_null = self
                        .null_bindings
                        .iter()
                        .rev()
                        .any(|scope| scope.contains(name));
                    if is_known_null {
                        return Err(CompileError::at(
                            self.current_span,
                            &format!(
                                "Cannot dereference '{}': it is statically known to be null",
                                name
                            ),
                            ErrorCode::E0007,
                        )
                        .with_suggestion(&format!(
                            "Check '{}' for null before dereferencing",
                            name
                        )));
                    }
                }

                let inner_type = self.analyze_expr(expr)?;
                match inner_type {
                    Type::Pointer(t) => {
                        // ADR 0015: raw pointer dereference requires
                        // an unsafe block. `Borrow<T>` and `MutBorrow<T>`
                        // are the safe reference forms and pass through
                        // unchecked; only the raw-pointer variant is
                        // gated. The null checks above fire first, so
                        // `*null` still reports the null deref.
                        if self.unsafe_depth == 0 {
                            return Err(CompileError::at(
                                self.current_span,
                                "Cannot dereference a raw pointer outside `unsafe`",
                                ErrorCode::E0007,
                            )
                            .with_suggestion(
                                "Wrap the dereference in `unsafe { ... }`, or \
                                 use `&T` / `&mut T` for a checked reference",
                            ));
                        }
                        Ok(*t)
                    }
                    Type::Borrow(t) => Ok(*t),
                    Type::MutBorrow(t) => Ok(*t),
                    _ => Ok(Type::Unknown),
                }
            }
            ExprKind::AddrOf { expr, .. } => {
                // Only expressions with stable storage can be
                // addressed. `&x` where x is a variable is fine;
                // `&(a + b)` or `&f()` are not.
                if !matches!(
                    &expr.as_ref().kind,
                    ExprKind::Var(_, _)
                        | ExprKind::ArrayAccess { .. }
                        | ExprKind::FieldAccess { .. }
                        | ExprKind::Deref { .. }
                ) {
                    return Err(CompileError::at(
                        self.current_span,
                        "Cannot take the address of a temporary value; \
                         address-of requires a variable, array element, \
                         field, or dereference",
                        ErrorCode::E0007,
                    )
                    .with_suggestion(
                        "Bind the value to a variable first, then take \
                         its address",
                    ));
                }
                let inner_type = self.analyze_expr(expr)?;
                Ok(Type::pointer(inner_type))
            }
            ExprKind::Number(_, _) => Ok(Type::Float),
            ExprKind::Int(_, _) => Ok(Type::Int),
            ExprKind::String(_, _) => Ok(Type::String),
            ExprKind::Bool(_, _) => Ok(Type::Bool),
            ExprKind::List(elements, _) => {
                if elements.is_empty() {
                    // An empty list has no elements to infer from. When the
                    // expected type provides one — `var xs: List<Int> := []` —
                    // bind to that. Falls back to `List<Unknown>` otherwise,
                    // matching the behavior of an empty list with no context.
                    if let Some(Type::List(inner)) = expected_type {
                        return Ok(Type::list((**inner).clone()));
                    }
                    return Ok(Type::list(Type::Unknown));
                }
                let first_type = self.analyze_expr(&elements[0])?;
                let mut list_type = first_type.clone();
                for elem in &elements[1..] {
                    let elem_type = self.analyze_expr(elem)?;
                    list_type = list_type.common_supertype(&elem_type);
                }
                Ok(Type::list(list_type))
            }
            ExprKind::RecordLiteral {
                name,
                type_args,
                fields,
                ..
            } => {
                let rec = self.records.get(name).cloned().ok_or_else(|| {
                    CompileError::at(
                        self.current_span,
                        &format!("Unknown record '{}'", name),
                        ErrorCode::E0003,
                    )
                    .with_suggestion(&format!("Declare it with `rec {}` before using it", name))
                })?;

                // Type-arg arity check.
                if !type_args.is_empty() && type_args.len() != rec.type_params.len() {
                    return Err(CompileError::at(
                        self.current_span,
                        &format!(
                            "Record '{}' expects {} type argument(s), got {}",
                            name,
                            rec.type_params.len(),
                            type_args.len()
                        ),
                        ErrorCode::E0002,
                    ));
                }

                // Build the substitution T -> concrete type from the literal's type_args.
                let mut subs: HashMap<String, Type> = HashMap::new();
                for (param, arg) in rec.type_params.iter().zip(type_args.iter()) {
                    subs.insert(param.clone(), arg.to_type());
                }

                // Every field must appear exactly once, with a coercible value.
                let mut seen = HashSet::new();
                for (field_name, value_expr) in fields {
                    let (_, field_ty) = rec
                        .fields
                        .iter()
                        .find(|(n, _)| n == field_name)
                        .ok_or_else(|| {
                            CompileError::at(
                                self.current_span,
                                &format!("Record '{}' has no field '{}'", name, field_name),
                                ErrorCode::E0004,
                            )
                        })?;
                    let expected = self.substitute_type_vars(field_ty, &subs);
                    let actual = self.analyze_expr_with_context(value_expr, Some(&expected))?;
                    if !actual.can_coerce_to(&expected) && expected != Type::Unknown {
                        return Err(CompileError::at(
                            self.current_span,
                            &format!(
                                "Field '{}' of '{}': expected {}, found {}",
                                field_name, name, expected, actual
                            ),
                            ErrorCode::E0002,
                        ));
                    }
                    seen.insert(field_name.clone());
                }
                for (field_name, _) in &rec.fields {
                    if !seen.contains(field_name) {
                        return Err(CompileError::at(
                            self.current_span,
                            &format!("Missing field '{}' in literal for '{}'", field_name, name),
                            ErrorCode::E0002,
                        ));
                    }
                }

                Ok(Type::record(
                    name,
                    type_args.iter().map(|t| t.to_type()).collect(),
                ))
            }
            ExprKind::Some { value, .. } => {
                let inner = self.analyze_expr(value)?;
                Ok(Type::option(inner))
            }
            ExprKind::None(_) => {
                if let Some(Type::Option(inner)) = expected_type {
                    Ok(Type::option((**inner).clone()))
                } else {
                    Ok(Type::option(Type::Unknown))
                }
            }
            ExprKind::Ok { value, .. } => {
                // Compute the payload type independently. `expected_type`
                // is used only to learn the error type of the enclosing
                // Result, not to influence the payload's analysis.
                let inner = self.analyze_expr(value)?;

                let error_type = match expected_type {
                    Some(Type::Result { error, .. }) => (**error).clone(),
                    _ => Type::Unknown,
                };

                Ok(Type::result(inner, error_type))
            }
            ExprKind::Error { value, .. } => {
                let inner = self.analyze_expr(value)?;

                let ok_type = match expected_type {
                    Some(Type::Result { ok, .. }) => (**ok).clone(),
                    _ => Type::Unknown,
                };

                Ok(Type::result(ok_type, inner))
            }
            ExprKind::Block {
                statements,
                trailing_expr,
                ..
            } => {
                for s in statements {
                    self.analyze_stmt(s)?;
                }
                let result = if let Some(expr) = trailing_expr {
                    self.analyze_expr(expr)?
                } else {
                    Type::Void
                };
                Ok(result)
            }
            ExprKind::If {
                condition,
                then_branch,
                else_branch,
                ..
            } => {
                let cond_type = self.analyze_expr(condition)?;
                if cond_type != Type::Bool && cond_type != Type::Unknown && cond_type != Type::Void
                {
                    return Err(CompileError::at(
                        self.current_span,
                        "If condition must be Bool",
                        ErrorCode::E0002,
                    ));
                }

                let entry_state = self.state.fork();

                let (then_result, then_exit) = self.in_branch(|a| {
                    a.push_scope();
                    let r = a.analyze_expr(then_branch);
                    a.pop_scope();
                    r
                });
                let then_type = then_result?;

                let (else_type_opt, else_exit) = match else_branch {
                    Some(else_expr) => {
                        let (r, s) = self.in_branch(|a| {
                            a.push_scope();
                            let r = a.analyze_expr(else_expr);
                            a.pop_scope();
                            r
                        });
                        (Some(r?), s)
                    }
                    None => (None, entry_state),
                };

                let result_type = match else_type_opt {
                    Some(else_type) => {
                        let then_is_void = then_type == Type::Void;
                        let else_is_void = else_type == Type::Void;
                        if then_is_void != else_is_void {
                            return Err(CompileError::at(self.current_span, "if branches produce inconsistent results: one branch yields a value, the other does not", ErrorCode::E0002)
                            .with_suggestion(
                                "Ensure both branches end with an expression, or neither does",
                            ));
                        }
                        then_type.common_supertype(&else_type)
                    }
                    None => Type::Void,
                };

                self.state = SemanticState::join(&then_exit, &else_exit);
                Ok(result_type)
            }
            ExprKind::Match { value, cases, .. } => {
                let value_type = self.analyze_expr(value)?;
                self.check_match_exhaustiveness(&value_type, cases)?;

                if cases.is_empty() {
                    return Err(CompileError::at(
                        self.current_span,
                        "Match expression must have at least one case",
                        ErrorCode::E0002,
                    ));
                }

                let mut arm_types: Vec<Type> = Vec::with_capacity(cases.len());
                let mut arm_exits: Vec<SemanticState> = Vec::with_capacity(cases.len());

                for case in cases {
                    self.check_pattern_type(&case.pattern, &value_type)?;
                    // Register the pattern literal's type so the completeness
                    // pass finds an entry for its ExprId. `check_pattern_type`
                    // infers the type but doesn't visit the expression through
                    // `analyze_expr`, so without this the literal would be
                    // missing from the type table.
                    if let Pattern::Literal(lit) = &case.pattern {
                        self.analyze_expr(lit)?;
                    }
                    let (arm_result, arm_exit) = self.in_branch(|a| {
                        a.push_scope();
                        let r: Result<Type> = (|| {
                            a.bind_pattern_variables(&case.pattern, &value_type)?;

                            if let Pattern::Guarded { condition, .. } = &case.pattern {
                                let cond_type = a.analyze_expr(condition)?;
                                if cond_type != Type::Bool && cond_type != Type::Unknown {
                                    return Err(CompileError::at(
                                        a.current_span,
                                        "Pattern guard must be boolean",
                                        ErrorCode::E0002,
                                    ));
                                }
                            }

                            a.analyze_expr(&case.body)
                        })();
                        a.pop_scope();
                        r
                    });

                    arm_types.push(arm_result?);
                    arm_exits.push(arm_exit);
                }

                let first_is_void = arm_types[0] == Type::Void;
                for t in &arm_types[1..] {
                    if (t == &Type::Void) != first_is_void {
                        return Err(CompileError::at(self.current_span, "match arms produce inconsistent results: some arms yield a value, others do not", ErrorCode::E0002)
                        .with_suggestion("Ensure all arms end with an expression, or none do"));
                    }
                }

                let result_type = arm_types[1..]
                    .iter()
                    .fold(arm_types[0].clone(), |acc, t| acc.common_supertype(t));

                self.state = SemanticState::join_all(&arm_exits);
                Ok(result_type)
            }
            ExprKind::TryCatch {
                try_branch,
                catch_var,
                catch_branch,
                finally_body,
                ..
            } => {
                let try_type = self.analyze_expr(try_branch)?;

                // The try body must evaluate to a Result<T, E>.
                let (ok_type, err_type) = match &try_type {
                    Type::Result { ok, error } => ((**ok).clone(), (**error).clone()),
                    Type::Unknown => (Type::Unknown, Type::Unknown),
                    other => {
                        return Err(CompileError::at(
                            self.current_span,
                            &format!("try body must produce a Result<T, E>, found {}", other),
                            ErrorCode::E0002,
                        )
                        .with_suggestion(
                            "Wrap the try body's final expression in Ok(...), \
                             or have it call a function that returns Result",
                        ));
                    }
                };

                // Bind the catch variable to the Result's error type.
                self.push_scope();
                if let Some(var) = catch_var {
                    self.declare_variable(var, err_type.clone(), false)?;
                }
                let catch_result = self.analyze_expr_with_context(catch_branch, Some(&ok_type));
                self.pop_scope();
                let catch_type = catch_result?;

                // The catch branch must produce something coercible to T.
                if catch_type != Type::Unknown
                    && ok_type != Type::Unknown
                    && !catch_type.can_coerce_to(&ok_type)
                {
                    return Err(CompileError::at(
                        self.current_span,
                        &format!(
                            "try/catch type mismatch: try body yields {}, catch yields {}",
                            ok_type, catch_type
                        ),
                        ErrorCode::E0002,
                    )
                    .with_suggestion(
                        "The catch branch must produce a value of the same type as Ok(...)",
                    ));
                }

                // Finally runs after the try/catch expression; analyze it
                // in the enclosing scope.
                if let Some(body) = finally_body {
                    for stmt in body {
                        self.analyze_stmt(stmt)?;
                    }
                }

                Ok(ok_type)
            }
            // See the module doc comment "Loop ownership analysis".
            // Borrows are restored to the pre-loop state; moves
            // inside the body are rejected (see below).
            ExprKind::For {
                var,
                iterable,
                body,
                trailing_expr,
                span,
            } => {
                let iter_type = self.analyze_expr(iterable)?;
                let elem_type = if let Type::List(t) = iter_type.clone() {
                    *t
                } else if iter_type != Type::Unknown {
                    return Err(CompileError::at(
                        *span,
                        &format!("For loop requires list, found {}", iter_type),
                        ErrorCode::E0002,
                    ));
                } else {
                    Type::Unknown
                };

                let entry_state = self.state.fork();

                // Names visible before entering the loop body. Only moves of
                // these names count as "move in loop body" — a variable declared
                // inside the body is recreated each iteration and can be moved
                // freely.
                let outer_vars: HashSet<String> =
                    self.scopes.iter().flat_map(|s| s.keys().cloned()).collect();

                let (body_result, body_exit) = self.in_branch(|a| {
                    a.loop_stack.push(LoopContext {
                        region_depth_at_entry: a.region_depth,
                    });
                    a.push_scope();
                    let r: Result<Type> = (|| {
                        a.declare_variable(var, elem_type.clone(), false)?;
                        for s in body {
                            a.analyze_stmt(s)?;
                        }
                        if let Some(expr) = trailing_expr {
                            a.analyze_expr(expr)
                        } else {
                            Ok(Type::Void)
                        }
                    })();
                    a.pop_scope();
                    a.loop_stack.pop();
                    r
                });
                let result_type = body_result?;

                // `for` bodies execute at least once on any non-empty iterable,
                // so a move of an outer variable would fire again on the next
                // iteration. Detect by comparing the entry snapshot against the
                // body exit: any outer variable that transitioned from
                // not-moved to moved is a move-in-loop.
                let new_moves: Vec<String> = outer_vars
                    .iter()
                    .filter(|name| {
                        let was_owned = entry_state.vars.get(*name).map_or(true, |s| !s.is_moved());
                        let now_moved = body_exit.vars.get(*name).is_some_and(|s| s.is_moved());
                        was_owned && now_moved
                    })
                    .cloned()
                    .collect();

                if let Some(moved_var) = new_moves.first() {
                    return Err(CompileError::at(
                        *span,
                        &format!("Cannot move '{}' in loop body", moved_var),
                        ErrorCode::E0008,
                    ));
                }

                // No move of an outer variable happened (we rejected otherwise),
                // so ownership agrees between entry and exit. Join anyway so
                // borrows introduced in the body follow the loop-scope rule.
                self.state = SemanticState::join(&entry_state, &body_exit);

                Ok(result_type)
            }
            // See the module doc comment "Loop ownership analysis".
            // Borrows are restored; moves are propagated outward
            // because the loop may run zero times.
            ExprKind::While {
                condition,
                body,
                trailing_expr,
                span,
            } => {
                let cond_type = self.analyze_expr(condition)?;
                if cond_type != Type::Bool && cond_type != Type::Unknown {
                    return Err(CompileError::at(
                        *span,
                        &format!("While condition must be Bool, found {}", cond_type),
                        ErrorCode::E0002,
                    ));
                }

                let entry_state = self.state.fork();

                let (body_result, body_exit) = self.in_branch(|a| {
                    a.loop_stack.push(LoopContext {
                        region_depth_at_entry: a.region_depth,
                    });
                    a.push_scope();
                    let r: Result<Type> = (|| {
                        for s in body {
                            a.analyze_stmt(s)?;
                        }
                        if let Some(expr) = trailing_expr {
                            a.analyze_expr(expr)
                        } else {
                            Ok(Type::Void)
                        }
                    })();
                    a.pop_scope();
                    a.loop_stack.pop();
                    r
                });
                let result_type = body_result?;

                // The loop may run zero times. Joining entry with exit yields
                // MaybeMoved for anything moved in the body — the same
                // conservative answer the old code produced by hand, now
                // derived from the state join. Fixpoint iteration belongs in
                // Phase 3 (per-function CFG/dataflow), not here.
                self.state = SemanticState::join(&entry_state, &body_exit);

                Ok(result_type)
            }
            ExprKind::Var(name, span) => {
                if self.is_moved(name) {
                    let mut err = CompileError::at(
                        *span,
                        &format!("Use of moved variable '{}'", name),
                        ErrorCode::E0007,
                    )
                    .with_suggestion(
                        "Variable ownership was transferred and cannot be used in this scope",
                    );
                    if let Some(moved_span) = self.moved_at(name) {
                        err = err.with_secondary(moved_span, format!("`{}` moved here", name));
                    }
                    return Err(err);
                }
                if self.is_mutably_borrowed(name) && !self.in_mut_borrow {
                    return Err(CompileError::at(
                        *span,
                        &format!("Cannot read '{}' while it is mutably borrowed", name),
                        ErrorCode::E0007,
                    )
                    .with_suggestion("Wait for the mutable borrow to end before reading"));
                }
                self.lookup_variable(name).map(|(t, _)| t).ok_or_else(|| {
                    CompileError::at(
                        *span,
                        &format!("Undefined variable '{}'", name),
                        ErrorCode::E0003,
                    )
                    .with_suggestion(&format!(
                        "Declare '{}' with 'var {} := ...' or 'val {} := ...' in this scope",
                        name, name, name
                    ))
                })
            }
            ExprKind::ArrayAccess { array, index, .. } => {
                let array_type = self.analyze_expr(array)?;
                let element_type = match array_type {
                    Type::List(element_type) => *element_type,
                    Type::Unknown => Type::Unknown,
                    _ => {
                        return Err(CompileError::at(
                            self.current_span,
                            &format!("Array access requires list, found {}", array_type),
                            ErrorCode::E0002,
                        ));
                    }
                };
                let index_type = self.analyze_expr(index)?;

                let mut out_of_bounds: Option<(i64, usize, String)> = None;
                let literal_index: Option<i64> = match &index.as_ref().kind {
                    ExprKind::Int(v, _) => Some(*v),
                    ExprKind::Number(f, _) => Some(*f as i64),
                    _ => None,
                };
                if let Some(idx_val) = literal_index {
                    if let ExprKind::Var(var_name, _) = &array.as_ref().kind {
                        if let Some(list_len) = self.lookup_list_length(var_name) {
                            if idx_val < 0 || (idx_val as usize) >= list_len {
                                out_of_bounds = Some((idx_val, list_len, var_name.clone()));
                            }
                        }
                    }
                    if let ExprKind::List(elements, _) = &array.as_ref().kind {
                        let list_len = elements.len();
                        if idx_val < 0 || (idx_val as usize) >= list_len {
                            out_of_bounds = Some((idx_val, list_len, "list literal".to_string()));
                        }
                    }
                }
                if let Some((idx_val, len, var_name)) = out_of_bounds {
                    return Err(CompileError::at(self.current_span, &format!(
                            "Array index out of bounds: index {} is out of bounds for '{}' with length {}",
                            idx_val, var_name, len
                        ), ErrorCode::E0004).with_suggestion(&format!(
                        "Valid indices are 0..{} for array of length {}", len - 1, len
                    )));
                }
                if index_type != Type::Int && index_type != Type::Unknown {
                    return Err(CompileError::at(
                        self.current_span,
                        &format!("Array index must be Int, found {}", index_type),
                        ErrorCode::E0002,
                    )
                    .with_suggestion(&format!(
                        "Use an Int index or convert {} with int({})",
                        index_type, index_type
                    )));
                }
                Ok(element_type)
            }
            ExprKind::Binary {
                left, op, right, ..
            } => {
                let mut left_type = self.analyze_expr(left)?;
                let mut right_type = self.analyze_expr(right)?;

                if let Type::Borrow(inner) = &left_type {
                    left_type = (**inner).clone();
                }
                if let Type::MutBorrow(inner) = &left_type {
                    left_type = (**inner).clone();
                }
                if let Type::Borrow(inner) = &right_type {
                    right_type = (**inner).clone();
                }
                if let Type::MutBorrow(inner) = &right_type {
                    right_type = (**inner).clone();
                }

                match op {
                    BinOp::Add => {
                        if left_type == Type::String && right_type == Type::String {
                            Ok(Type::String)
                        } else if left_type.is_numeric() && right_type.is_numeric() {
                            Ok(left_type.common_supertype(&right_type))
                        } else {
                            Err(CompileError::at(
                                self.current_span,
                                &format!(
                                    "Addition requires matching types, found {} and {}",
                                    left_type, right_type
                                ),
                                ErrorCode::E0002,
                            )
                            .with_suggestion("Use matching types or add type conversion"))
                        }
                    }
                    BinOp::Subtract | BinOp::Multiply | BinOp::Divide => {
                        if left_type.is_numeric() && right_type.is_numeric() {
                            Ok(left_type.common_supertype(&right_type))
                        } else {
                            Err(CompileError::at(
                                self.current_span,
                                &format!(
                                    "Arithmetic requires numeric types, found {} and {}",
                                    left_type, right_type
                                ),
                                ErrorCode::E0002,
                            )
                            .with_suggestion("Both operands must be numeric (Int or Float)"))
                        }
                    }
                    BinOp::Greater | BinOp::Less | BinOp::GreaterEqual | BinOp::LessEqual => {
                        if left_type.is_numeric() && right_type.is_numeric() {
                            Ok(Type::Bool)
                        } else {
                            Err(CompileError::at(
                                self.current_span,
                                &format!(
                                    "Comparison requires numeric types, found {} and {}",
                                    left_type, right_type
                                ),
                                ErrorCode::E0002,
                            )
                            .with_suggestion("Use numeric types for comparison"))
                        }
                    }
                    BinOp::Equal | BinOp::NotEqual => {
                        // ADR 0029: nominal types do not auto-implement
                        // equality. Structural `PartialEq` would accept
                        // `a == b` when both sides have the same
                        // `NominalTypeId`, so the guard is explicit.
                        if matches!(left_type, Type::Distinct { .. })
                            || matches!(right_type, Type::Distinct { .. })
                        {
                            return Err(CompileError::at(
                                self.current_span,
                                &format!(
                                    "Equality on nominal types requires a trait impl; \
                                     found {} and {}",
                                    left_type, right_type
                                ),
                                ErrorCode::E0002,
                            )
                            .with_suggestion(
                                "Write an `impl Eq for T` (or the equivalent) to \
                                 enable equality on this nominal type",
                            ));
                        }
                        if left_type == right_type
                            || (left_type.is_numeric() && right_type.is_numeric())
                        {
                            Ok(Type::Bool)
                        } else {
                            Err(CompileError::at(
                                self.current_span,
                                &format!(
                                    "Equality requires matching types, found {} and {}",
                                    left_type, right_type
                                ),
                                ErrorCode::E0002,
                            )
                            .with_suggestion("Use matching types for equality comparison"))
                        }
                    }
                    BinOp::And | BinOp::Or => {
                        if left_type == Type::Bool && right_type == Type::Bool {
                            Ok(Type::Bool)
                        } else {
                            Err(CompileError::at(
                                self.current_span,
                                &format!(
                                    "Logical operators require boolean operands, found {} and {}",
                                    left_type, right_type
                                ),
                                ErrorCode::E0002,
                            )
                            .with_suggestion("Use 'and' and 'or' only with boolean values"))
                        }
                    }
                }
            }
            ExprKind::FunctionCall { name, args, .. } => {
                let clean_name = name.trim_end_matches("()");

                // ADR 0029: Nominal type constructor. `UserId.from_base(x)`.
                // The receiver is a *type name*, not a variable, so this
                // must run before the ordinary dotted-name dispatch below.
                if let Some(ty) = self.try_nominal_from_base(clean_name, args)? {
                    return Ok(ty);
                }

                // ADR 0030: Enum ordinal constructor. `Day.from_ordinal(x)`.
                // Same shape as nominal `from_base`.
                if let Some(ty) = self.try_enum_from_ordinal(clean_name, args)? {
                    return Ok(ty);
                }

                if clean_name.contains('.') {
                    let parts: Vec<&str> = clean_name.split('.').collect();
                    if parts.len() == 2 {
                        let receiver = parts[0];
                        let method_name = parts[1];

                        if let Some((receiver_type, mutable)) = self.lookup_variable(receiver) {
                            // ADR 0030: Enum ordinal extraction.
                            // `d.to_ordinal()` where d has an Enum type.
                            if let Type::Enum { .. } = &receiver_type {
                                if method_name == "to_ordinal" {
                                    if !args.is_empty() {
                                        return Err(CompileError::at(
                                            self.current_span,
                                            &format!(
                                                "to_ordinal takes no arguments, got {}",
                                                args.len()
                                            ),
                                            ErrorCode::E0002,
                                        ));
                                    }
                                    return Ok(Type::Int);
                                }
                            }
                            // ADR 0029: Nominal type instance conversion.
                            // `x.to_base()` where x has a Distinct type.
                            // Yields the base type; consumes the receiver
                            // if the base is non-Copy.
                            if let Type::Distinct { base, .. } = &receiver_type {
                                if method_name == "to_base" {
                                    if !args.is_empty() {
                                        return Err(CompileError::at(
                                            self.current_span,
                                            &format!(
                                                "to_base takes no arguments, got {}",
                                                args.len()
                                            ),
                                            ErrorCode::E0002,
                                        ));
                                    }
                                    let base_ty = (**base).clone();
                                    if !self.is_type_copy(&base_ty) {
                                        let span = self.current_span;
                                        self.mark_moved(receiver, span);
                                    }
                                    return Ok(base_ty);
                                }
                            }
                            // ─── List.append dispatch ───
                            // Like Map methods, `List.append` needs the receiver's
                            // concrete element type and a mutability check, which the
                            // generic builtin path doesn't provide.
                            if let Type::List(elem_ty) = &receiver_type {
                                if method_name == "append" {
                                    if !mutable {
                                        return Err(CompileError::at(
                                            self.current_span,
                                            &format!(
                                                "Cannot call 'append' on immutable variable '{}'",
                                                receiver
                                            ),
                                            ErrorCode::E0007,
                                        )
                                        .with_suggestion(&format!(
                                            "Declare '{}' with 'var' instead of 'val'",
                                            receiver
                                        )));
                                    }
                                    if args.len() != 1 {
                                        return Err(CompileError::at(
                                            self.current_span,
                                            &format!(
                                                "List.append expects 1 argument, got {}",
                                                args.len()
                                            ),
                                            ErrorCode::E0002,
                                        ));
                                    }
                                    let arg_ty =
                                        self.analyze_expr_with_context(&args[0], Some(elem_ty))?;
                                    if !elem_ty.is_unknown()
                                        && !arg_ty.is_unknown()
                                        && !arg_ty.can_coerce_to(elem_ty)
                                    {
                                        return Err(CompileError::at(self.current_span, &format!(
                                                "List.append element type mismatch: expected {}, found {}",
                                                elem_ty, arg_ty
                                            ), ErrorCode::E0002));
                                    }
                                    // The list is now longer than the analyzer previously
                                    // knew; drop the tracked length so a stale OOB check
                                    // doesn't fire on a now-valid index.
                                    self.clear_list_length(receiver);
                                    return Ok(Type::Void);
                                }
                            }
                            // ─── Map method dispatch ───
                            // Map methods need the concrete K and V from the receiver to
                            // check argument types precisely. The generic builtin path
                            // below only checks arg count, so Map gets its own dispatch
                            // (ADR 0027). `&Box<Type>` deref-coerces to `&Type` at the
                            // call site.
                            if let Type::Map(k, v) = &receiver_type {
                                return self.analyze_map_method_call(
                                    receiver,
                                    mutable,
                                    method_name,
                                    args,
                                    k,
                                    v,
                                );
                            }

                            // ─── Built-in method form ───
                            // `list.length()` → `List.length`. The call dispatches
                            // through the built-in registry, not the trait registry.
                            if let Some(base) = Self::base_type_name(&receiver_type) {
                                let builtin_form = format!("{}.{}", base, method_name);
                                if let Some(func_info) = self.functions.get(&builtin_form).cloned()
                                {
                                    // Analyze each explicit arg. The receiver is
                                    // implicitly the first argument at IR-build time,
                                    // so its type is already known.
                                    for arg in args {
                                        self.analyze_expr(arg)?;
                                        self.register_call_arg_temporary(arg);
                                    }
                                    // Optional strict check: if the built-in takes N
                                    // params and the receiver counts as one, then
                                    // `args.len() + 1 == N` should hold.
                                    let expected_extra = func_info.params.len().saturating_sub(1);
                                    if args.len() != expected_extra {
                                        return Err(CompileError::at(self.current_span, &format!(
                                                "Method '{}' expects {} argument(s) after the receiver, got {}",
                                                method_name, expected_extra, args.len()
                                            ), ErrorCode::E0002));
                                    }
                                    return Ok(func_info.return_type);
                                }
                            }

                            // ─── Trait-based method resolution ───
                            if let Some(method) =
                                self.resolve_trait_method(&receiver_type, method_name)
                            {
                                if args.len() != method.params.len() {
                                    return Err(CompileError::at(
                                        self.current_span,
                                        &format!(
                                            "Method '{}' expects {} arguments, got {}",
                                            method_name,
                                            method.params.len(),
                                            args.len()
                                        ),
                                        ErrorCode::E0002,
                                    ));
                                }
                                for (arg, (param_name, param_type)) in
                                    args.iter().zip(&method.params)
                                {
                                    let arg_type = self.analyze_expr(arg)?;
                                    self.register_call_arg_temporary(arg);
                                    let expected_type = match param_type {
                                        Some(s) => self.resolve_type_syntax(s)?,
                                        None => Type::Unknown,
                                    };
                                    if !arg_type.can_coerce_to(&expected_type)
                                        && expected_type != Type::Unknown
                                    {
                                        return Err(CompileError::at(self.current_span, &format!(
                                                "Argument '{}' type mismatch: expected {}, found {}",
                                                param_name, expected_type, arg_type
                                            ), ErrorCode::E0002));
                                    }
                                }
                                return Ok(method
                                    .return_type
                                    .as_ref()
                                    .map(|t| t.to_type())
                                    .unwrap_or(Type::Void));
                            }

                            // ─── Neither built-in nor trait method ───
                            return Err(CompileError::at(
                                self.current_span,
                                &format!(
                                    "Type {} does not have method '{}'",
                                    receiver_type, method_name
                                ),
                                ErrorCode::E0004,
                            )
                            .with_suggestion(&format!(
                                "Implement a trait for {} that provides method '{}', \
                                 or check if '{}' is a built-in",
                                receiver_type, method_name, method_name
                            )));
                        }
                    }
                }

                // ADR 0015: `alloc` and `free` manipulate raw memory
                // directly and require an unsafe block. Both are
                // registered builtins, so `clean_name` is stable.
                // Checked before the arity check — "you need unsafe"
                // is a more useful first diagnostic than "wrong
                // argument count" when the surrounding code is
                // already outside a safety boundary.
                if (clean_name == "alloc" || clean_name == "free") && self.unsafe_depth == 0 {
                    return Err(CompileError::at(
                        self.current_span,
                        &format!("`{}` requires an `unsafe` block", clean_name),
                        ErrorCode::E0007,
                    )
                    .with_suggestion(&format!(
                        "Wrap the `{}` call in `unsafe {{ ... }}`",
                        clean_name,
                    )));
                }

                let func_info = self.functions.get(clean_name).cloned().ok_or_else(|| {
                    CompileError::at(
                        self.current_span,
                        &format!("Undefined function '{}'", name),
                        ErrorCode::E0004,
                    )
                    .with_suggestion(&format!(
                        "Check if function '{}' is defined or imported",
                        name
                    ))
                })?;

                // Variadic extern functions accept any number of
                // arguments at or above the fixed count. Extra
                // arguments are the variadic tail — their types
                // are not checked (matching C). Non-variadic
                // functions still require exact arity.
                let is_variadic = self.variadic_functions.contains(clean_name);
                let arity_ok = if is_variadic {
                    args.len() >= func_info.params.len()
                } else {
                    args.len() == func_info.params.len()
                };
                if !arity_ok {
                    let expected_msg = if is_variadic {
                        format!("at least {} argument(s)", func_info.params.len())
                    } else {
                        format!("exactly {} argument(s)", func_info.params.len())
                    };
                    return Err(CompileError::at(
                        self.current_span,
                        &format!(
                            "Function '{}' expects {}, got {}",
                            name,
                            expected_msg,
                            args.len()
                        ),
                        ErrorCode::E0002,
                    )
                    .with_suggestion(&format!("Provide {} to '{}'", expected_msg, name)));
                }

                // Variadic tail: any args past `params.len()` have no
                // declared type. Analyze them in an uncontexted way so
                // their types land in `type_table` (otherwise
                // TypeTableCompletePass warns about every extra arg).
                // Their types are not checked — this matches C's
                // variadic ABI where extra arg types are the caller's
                // responsibility.
                if is_variadic {
                    for arg in args.iter().skip(func_info.params.len()) {
                        self.analyze_expr(arg)?;
                        self.register_call_arg_temporary(arg);
                    }
                }

                let mut type_bindings: HashMap<String, Type> = HashMap::new();
                for (arg, (param_name, param_type)) in args.iter().zip(&func_info.params) {
                    let arg_type = self.analyze_expr_with_context(arg, Some(param_type))?;
                    self.register_call_arg_temporary(arg);
                    let resolved_param_type = self.resolve_type(param_type);

                    // Bind any type variables the parameter contains. Recursive —
                    // `List<T>` against `List<Int>` binds T = Int; `TypeVar("T")`
                    // directly binds as before.
                    self.unify_types(&resolved_param_type, &arg_type, &mut type_bindings)?;

                    // The coercion check only fires when the parameter type is
                    // fully concrete. A generic parameter (`List<T>`, `Option<U>`)
                    // was checked structurally by `unify_types` above; re-checking
                    // it with `can_coerce_to` would treat the type variable as a
                    // concrete non-supertype and produce spurious mismatches.
                    if !resolved_param_type.contains_type_var()
                        && !resolved_param_type.contains_unknown()
                        && !arg_type.contains_unknown()
                        && !arg_type.can_coerce_to(&resolved_param_type)
                    {
                        return Err(CompileError::at(
                            self.current_span,
                            &format!(
                                "Argument '{}' type mismatch: expected {}, found {}",
                                param_name, resolved_param_type, arg_type
                            ),
                            ErrorCode::E0002,
                        )
                        .with_suggestion(&format!(
                            "Convert the argument to {} or change the function signature",
                            resolved_param_type
                        )));
                    }
                }

                // Record a generic instantiation fact. `InstantiationPlan`
                // consumes these in `type_check_program`; the IR builder
                // reads the closed plan to emit one specialization per
                // concrete type-argument combination. Only functions
                // with non-empty `type_params` are recorded — a call to
                // a non-generic function is fully resolved here and
                // needs no entry.
                //
                // `type_args` may contain `Type::Unknown` if an
                // argument's type could not be inferred; the
                // executable-IR verifier is responsible for rejecting
                // such cases. See ADR 0013.
                if !func_info.type_params.is_empty() {
                    let type_params = func_info.type_params.clone();
                    let type_args: Vec<Type> = type_params
                        .iter()
                        .map(|p| type_bindings.get(p).cloned().unwrap_or(Type::Unknown))
                        .collect();
                    self.instantiations.push(Instantiation {
                        call_site: expr.id,
                        function: clean_name.to_string(),
                        type_params,
                        type_args,
                    });
                }

                let return_type = self.substitute_type_vars(&func_info.return_type, &type_bindings);
                Ok(return_type)
            }
            ExprKind::Unary { op, expr, .. } => {
                let operand_type = self.analyze_expr_with_context(expr, expected_type)?;
                match op {
                    crate::frontend::ast::UnaryOp::Negate => {
                        if operand_type.is_numeric() || operand_type == Type::Unknown {
                            Ok(operand_type)
                        } else {
                            Err(CompileError::at(
                                self.current_span,
                                &format!("Cannot negate non-numeric type {}", operand_type),
                                ErrorCode::E0002,
                            )
                            .with_suggestion("Negation requires Int or Float operand"))
                        }
                    }
                    crate::frontend::ast::UnaryOp::Not => {
                        if operand_type == Type::Bool || operand_type == Type::Unknown {
                            Ok(Type::Bool)
                        } else {
                            Err(CompileError::at(
                                self.current_span,
                                &format!("Logical not requires Bool, found {}", operand_type),
                                ErrorCode::E0002,
                            )
                            .with_suggestion("Use 'not' only with boolean values"))
                        }
                    }
                }
            }
            ExprKind::PtrLiteral(_, _) => Ok(Type::Ptr),
            ExprKind::NullPtr(_) => Ok(Type::Ptr),

            // ─── UNIFY TYPES ─── give Range and FieldAccess proper inferred types.
            ExprKind::Range { start, end, .. } => {
                let start_type = match start {
                    Some(e) => self.analyze_expr(e)?,
                    None => Type::Int,
                };
                let end_type = match end {
                    Some(e) => self.analyze_expr(e)?,
                    None => start_type.clone(),
                };
                Ok(Type::list(start_type.common_supertype(&end_type)))
            }
            ExprKind::FieldAccess { object, field, .. } => {
                let obj_ty = self.analyze_expr(object)?;

                // ADR 0030: Enum ordinal extraction, no-parens form.
                if let Type::Enum { .. } = &obj_ty {
                    if field == "to_ordinal" {
                        return Ok(Type::Int);
                    }
                }

                // ADR 0029: Nominal type instance conversion, no-parens
                // form. `x.to_base` where x has a Distinct type.
                if let Type::Distinct { base, .. } = &obj_ty {
                    if field == "to_base" {
                        let base_ty = (**base).clone();
                        if !self.is_type_copy(&base_ty) {
                            if let ExprKind::Var(name, _) = &object.as_ref().kind {
                                let span = self.current_span;
                                self.mark_moved(name, span);
                            }
                        }
                        return Ok(base_ty);
                    }
                }

                // The parser produces `FieldAccess` for both `p.x`
                // (record field) and `s.length` (zero-argument method
                // call, the pre-records form). Disambiguate by type:
                // records use field lookup, everything else falls
                // through to built-in / trait method dispatch — the
                // same code path `s.length()` already uses.
                if let Type::Record(rec_name, rec_args) = &obj_ty {
                    let rec = self.records.get(rec_name).cloned().ok_or_else(|| {
                        CompileError::at(
                            self.current_span,
                            &format!("Unknown record '{}'", rec_name),
                            ErrorCode::E0003,
                        )
                    })?;
                    let (_, field_ty) =
                        rec.fields.iter().find(|(n, _)| n == field).ok_or_else(|| {
                            CompileError::at(
                                self.current_span,
                                &format!("Record '{}' has no field '{}'", rec_name, field),
                                ErrorCode::E0004,
                            )
                        })?;
                    let mut subs = HashMap::new();
                    for (p, a) in rec.type_params.iter().zip(rec_args.iter()) {
                        subs.insert(p.clone(), a.clone());
                    }
                    return Ok(self.substitute_type_vars(field_ty, &subs));
                }

                // ─── Map methods in the zero-arg form ───
                // `m.length`, `m.keys`, `m.values` (no parens). The
                // argument-taking methods are rejected here with a
                // "requires parentheses" diagnostic.
                if let Type::Map(k, v) = &obj_ty {
                    return self.analyze_map_field_access(field, k, v);
                }

                // Unknown receiver type: don't guess. Type-table
                // completeness catches the real problem elsewhere.
                if obj_ty == Type::Unknown {
                    return Ok(Type::Unknown);
                }

                // Zero-argument method-call form `x.method`. The
                // receiver is the implicit first argument, so the
                // built-in must take exactly one parameter.
                if let Some(base) = Self::base_type_name(&obj_ty) {
                    let builtin_form = format!("{}.{}", base, field);
                    if let Some(func_info) = self.functions.get(&builtin_form).cloned() {
                        if func_info.params.len() != 1 {
                            return Err(CompileError::at(
                                self.current_span,
                                &format!(
                                    "Method '{}' on {} expects {} argument(s); \
                                     `x.{}` (no parens) is only valid for zero-argument methods",
                                    field,
                                    obj_ty,
                                    func_info.params.len().saturating_sub(1),
                                    field,
                                ),
                                ErrorCode::E0002,
                            ));
                        }
                        return Ok(func_info.return_type);
                    }
                }

                if let Some(method) = self.resolve_trait_method(&obj_ty, field) {
                    if !method.params.is_empty() {
                        return Err(CompileError::at(
                            self.current_span,
                            &format!(
                                "Method '{}' on {} expects {} argument(s); \
                                 `x.{}` (no parens) is only valid for zero-argument methods",
                                field,
                                obj_ty,
                                method.params.len(),
                                field,
                            ),
                            ErrorCode::E0002,
                        ));
                    }
                    return Ok(method
                        .return_type
                        .as_ref()
                        .map(|t| t.to_type())
                        .unwrap_or(Type::Void));
                }

                Err(CompileError::at(
                    self.current_span,
                    &format!("Type {} has no field or method '{}'", obj_ty, field),
                    ErrorCode::E0004,
                )
                .with_suggestion(
                    "Field access requires a record value; zero-argument method calls \
                     accept `x.method` or `x.method()`",
                ))
            }
            ExprKind::MapLiteral {
                key_type: key_syntax,
                value_type: value_syntax,
                entries,
                span,
            } => {
                // ─── Explicit type arguments: `Map<K, V> { ... }` ───
                // The declared K and V are authoritative; each entry's key
                // and value must coerce to them.
                if let (Some(k_syntax), Some(v_syntax)) = (key_syntax, value_syntax) {
                    let declared_key = k_syntax.to_type();
                    let declared_value = v_syntax.to_type();

                    if !Self::is_hashable_key(&declared_key) {
                        return Err(CompileError::at(
                            *span,
                            &format!(
                                "Map keys must be Int, String, or Bool, found {}",
                                declared_key
                            ),
                            ErrorCode::E0002,
                        ));
                    }

                    for (k_expr, v_expr) in entries {
                        let k_ty = self.analyze_expr_with_context(k_expr, Some(&declared_key))?;
                        let v_ty = self.analyze_expr_with_context(v_expr, Some(&declared_value))?;
                        if declared_key != Type::Unknown && !k_ty.can_coerce_to(&declared_key) {
                            return Err(CompileError::at(
                                self.current_span,
                                &format!("Map key: expected {}, found {}", declared_key, k_ty),
                                ErrorCode::E0002,
                            ));
                        }
                        if declared_value != Type::Unknown && !v_ty.can_coerce_to(&declared_value) {
                            return Err(CompileError::at(
                                self.current_span,
                                &format!("Map value: expected {}, found {}", declared_value, v_ty),
                                ErrorCode::E0002,
                            ));
                        }
                    }

                    return Ok(Type::map(declared_key, declared_value));
                }

                // ─── Inferred form: `Map { ... }` ───
                // Empty entries: the type must come from context.
                if entries.is_empty() {
                    if let Some(Type::Map(k, v)) = expected_type {
                        return Ok(Type::map((**k).clone(), (**v).clone()));
                    }
                    return Err(CompileError::at(
                        *span,
                        "Empty map literal needs a type annotation",
                        ErrorCode::E0002,
                    )
                    .with_suggestion(
                        "Write `Map<K, V> {}` for an explicit type, or annotate the binding, \
                         e.g. `var m: Map<String, Int> := Map {}`",
                    ));
                }

                // Non-empty: infer K and V by unifying the entries. Follows
                // the same permissive rule as list literals — mixed entry
                // types that have no common supertype produce `Unknown`, not
                // an error. A stricter rule can be added later if needed.
                let mut inferred_key: Option<Type> = None;
                let mut inferred_value: Option<Type> = None;
                for (k_expr, v_expr) in entries {
                    let k_ty = self.analyze_expr(k_expr)?;
                    let v_ty = self.analyze_expr(v_expr)?;
                    inferred_key = Some(match inferred_key {
                        None => k_ty,
                        Some(prev) => prev.common_supertype(&k_ty),
                    });
                    inferred_value = Some(match inferred_value {
                        None => v_ty,
                        Some(prev) => prev.common_supertype(&v_ty),
                    });
                }
                let key_ty = inferred_key.unwrap_or(Type::Unknown);
                let value_ty = inferred_value.unwrap_or(Type::Unknown);

                if !Self::is_hashable_key(&key_ty) {
                    return Err(CompileError::at(
                        *span,
                        &format!("Map keys must be Int, String, or Bool, found {}", key_ty),
                        ErrorCode::E0002,
                    ));
                }

                Ok(Type::map(key_ty, value_ty))
            }
        }
    }

    /// ADR 0030. If `clean_name` is `T.from_ordinal` where `T` is
    /// a registered enum type, analyze `args` and produce the enum
    /// type. Returns `Ok(None)` if the name is not an enum
    /// constructor.
    ///
    /// Same reasoning as `try_nominal_from_base`: the receiver is a
    /// type name, not a variable. A literal out-of-range argument is
    /// rejected at compile time; runtime out-of-range is not checked
    /// in v1.
    fn try_enum_from_ordinal(&mut self, clean_name: &str, args: &[Expr]) -> Result<Option<Type>> {
        let Some((receiver, method)) = clean_name.split_once('.') else {
            return Ok(None);
        };
        if method != "from_ordinal" {
            return Ok(None);
        }
        let Some(ty) = self.enum_types.get(receiver).cloned() else {
            return Ok(None);
        };
        let Type::Enum { id, name, variants } = ty else {
            return Ok(None);
        };

        if args.len() != 1 {
            return Err(CompileError::at(
                self.current_span,
                &format!(
                    "{}.from_ordinal expects 1 argument, got {}",
                    name,
                    args.len()
                ),
                ErrorCode::E0002,
            ));
        }

        // Compile-time range check for literal arguments.
        if let ExprKind::Int(n, span) = &args[0].kind {
            if *n < 0 || (*n as usize) >= variants.len() {
                let max = variants.len().saturating_sub(1);
                return Err(CompileError::at(
                    *span,
                    &format!(
                        "ordinal {} is out of range for {}; valid range is 0..{}",
                        n, name, max
                    ),
                    ErrorCode::E0002,
                ));
            }
        }

        let arg_ty = self.analyze_expr_with_context(&args[0], Some(&Type::Int))?;
        if !arg_ty.is_unknown() && !arg_ty.can_coerce_to(&Type::Int) {
            return Err(CompileError::at(
                self.current_span,
                &format!("{}.from_ordinal expects an Int, found {}", name, arg_ty),
                ErrorCode::E0002,
            ));
        }

        Ok(Some(Type::enum_type(id, &name, variants)))
    }

    /// ADR 0029. If `clean_name` is `T.from_base` where `T` is a
    /// registered nominal type, analyze `args` and produce the
    /// nominal type. Returns `Ok(None)` if the name is not a
    /// nominal constructor, so the caller falls through to the
    /// ordinary function dispatch.
    ///
    /// Runs before the dotted-name path because `UserId` in
    /// `UserId.from_base(...)` is a type name, not a variable. The
    /// ordinary dispatch would report "Undefined function" otherwise.
    fn try_nominal_from_base(&mut self, clean_name: &str, args: &[Expr]) -> Result<Option<Type>> {
        let Some((receiver, method)) = clean_name.split_once('.') else {
            return Ok(None);
        };
        if method != "from_base" {
            return Ok(None);
        }
        let Some(ty) = self.nominal_types.get(receiver).cloned() else {
            return Ok(None);
        };
        let Type::Distinct { id, name, base } = ty else {
            return Ok(None);
        };

        if args.len() != 1 {
            return Err(CompileError::at(
                self.current_span,
                &format!("{}.from_base expects 1 argument, got {}", name, args.len()),
                ErrorCode::E0002,
            ));
        }

        let arg_ty = self.analyze_expr_with_context(&args[0], Some(&*base))?;
        if !arg_ty.is_unknown() && !arg_ty.can_coerce_to(&base) {
            return Err(CompileError::at(
                self.current_span,
                &format!(
                    "{}.from_base expects a value of type {}, found {}",
                    name, base, arg_ty
                ),
                ErrorCode::E0002,
            ));
        }

        // Consumption: mark the argument moved if non-Copy. For
        // Copy bases (Int, Float, Bool) the value is duplicated.
        if let ExprKind::Var(arg_name, _) = &args[0].kind {
            if !self.is_type_copy(&arg_ty) {
                let span = args[0].span();
                self.mark_moved(arg_name, span);
            }
        }

        Ok(Some(Type::distinct(id, &name, *base)))
    }

    pub(super) fn substitute_type_vars(
        &self,
        type_: &Type,
        bindings: &HashMap<String, Type>,
    ) -> Type {
        match type_ {
            Type::TypeVar(name) => bindings.get(name).cloned().unwrap_or_else(|| type_.clone()),
            Type::List(inner) => Type::list(self.substitute_type_vars(inner, bindings)),
            Type::Array(inner, size) => {
                Type::array(self.substitute_type_vars(inner, bindings), *size)
            }
            Type::Tuple(elements) => Type::tuple(
                elements
                    .iter()
                    .map(|e| self.substitute_type_vars(e, bindings))
                    .collect(),
            ),
            Type::Option(inner) => Type::option(self.substitute_type_vars(inner, bindings)),
            Type::Result { ok, error } => Type::result(
                self.substitute_type_vars(ok, bindings),
                self.substitute_type_vars(error, bindings),
            ),
            Type::Pointer(inner) => Type::pointer(self.substitute_type_vars(inner, bindings)),
            Type::Borrow(inner) => Type::borrow(self.substitute_type_vars(inner, bindings)),
            Type::MutBorrow(inner) => Type::mut_borrow(self.substitute_type_vars(inner, bindings)),
            Type::Channel(inner) => Type::channel(self.substitute_type_vars(inner, bindings)),
            _ => type_.clone(),
        }
    }

    /// Unify a parameter pattern against a concrete argument type,
    /// populating `bindings` for any type variables the pattern contains.
    ///
    /// Unlike the direct `Type::TypeVar` match it replaces, this recurses
    /// into composite types: `List<T>` against `List<Int>` binds
    /// `T = Int`; `Map<K, V>` against `Map<String, Int>` binds both;
    /// `Result<List<T>, E>` against `Result<List<Int>, String>` binds
    /// both at the right depths.
    ///
    /// Types with no type variables in the pattern are a no-op — nothing
    /// to bind.
    fn unify_types(
        &self,
        pattern: &Type,
        concrete: &Type,
        bindings: &mut HashMap<String, Type>,
    ) -> Result<()> {
        match (pattern, concrete) {
            (Type::TypeVar(name), concrete) => {
                if let Some(existing) = bindings.get(name) {
                    if existing != concrete
                        && existing != &Type::Unknown
                        && concrete != &Type::Unknown
                    {
                        return Err(CompileError::simple(
                            &format!(
                                "Type mismatch for generic parameter '{}': expected {}, found {}",
                                name, existing, concrete
                            ),
                            0,
                            0,
                            "",
                            ErrorCode::E0002,
                        ));
                    }
                } else {
                    bindings.insert(name.clone(), concrete.clone());
                }
                Ok(())
            }
            (Type::List(p), Type::List(c)) => self.unify_types(p, c, bindings),
            (Type::Option(p), Type::Option(c)) => self.unify_types(p, c, bindings),
            (Type::Result { ok: p1, error: p2 }, Type::Result { ok: c1, error: c2 }) => {
                self.unify_types(p1, c1, bindings)?;
                self.unify_types(p2, c2, bindings)
            }
            (Type::Map(p1, p2), Type::Map(c1, c2)) => {
                self.unify_types(p1, c1, bindings)?;
                self.unify_types(p2, c2, bindings)
            }
            (Type::Pointer(p), Type::Pointer(c)) => self.unify_types(p, c, bindings),
            (Type::Borrow(p), Type::Borrow(c)) => self.unify_types(p, c, bindings),
            (Type::MutBorrow(p), Type::MutBorrow(c)) => self.unify_types(p, c, bindings),
            (Type::Array(p, _), Type::Array(c, _)) => self.unify_types(p, c, bindings),
            (Type::Channel(p), Type::Channel(c)) => self.unify_types(p, c, bindings),
            (Type::Generic { name: n1, args: a1 }, Type::Generic { name: n2, args: a2 })
                if n1 == n2 && a1.len() == a2.len() =>
            {
                for (p, c) in a1.iter().zip(a2.iter()) {
                    self.unify_types(p, c, bindings)?;
                }
                Ok(())
            }
            (Type::Record(n1, a1), Type::Record(n2, a2)) if n1 == n2 && a1.len() == a2.len() => {
                for (p, c) in a1.iter().zip(a2.iter()) {
                    self.unify_types(p, c, bindings)?;
                }
                Ok(())
            }
            _ => Ok(()),
        }
    }

    /// True when `ty` is a legal `Map` key type — `Int`, `String`,
    /// `Bool`, or `Unknown` (not yet inferred). See ADR 0027.
    ///
    /// ADR 0029: a nominal type is a valid key iff its base is.
    pub(super) fn is_hashable_key(ty: &Type) -> bool {
        match ty {
            Type::Int | Type::String | Type::Bool | Type::Unknown => true,
            Type::Distinct { base, .. } => Self::is_hashable_key(base),
            _ => false,
        }
    }

    /// Dispatch a `Map` method call with arguments. Unlike the generic
    /// builtin path (which only checks argument count), this handler
    /// checks argument types against the receiver's concrete `K` and
    /// `V`, and enforces that `insert` has a `var` receiver.
    fn analyze_map_method_call(
        &mut self,
        receiver: &str,
        receiver_mutable: bool,
        method_name: &str,
        args: &[Expr],
        key_type: &Type,
        value_type: &Type,
    ) -> Result<Type> {
        // Key-type gate. Idempotent; the first method call on a
        // badly-typed map produces the diagnostic.
        if !Self::is_hashable_key(key_type) {
            return Err(CompileError::at(
                self.current_span,
                &format!("Map keys must be Int, String, or Bool, found {}", key_type),
                ErrorCode::E0002,
            ));
        }

        match method_name {
            "insert" => {
                if !receiver_mutable {
                    return Err(CompileError::at(
                        self.current_span,
                        &format!("Cannot call 'insert' on immutable variable '{}'", receiver),
                        ErrorCode::E0007,
                    )
                    .with_suggestion(&format!(
                        "Declare '{}' with 'var' instead of 'val'",
                        receiver
                    )));
                }
                if args.len() != 2 {
                    return Err(CompileError::at(
                        self.current_span,
                        &format!("Map.insert expects 2 arguments, got {}", args.len()),
                        ErrorCode::E0002,
                    ));
                }
                let arg_key = self.analyze_expr_with_context(&args[0], Some(key_type))?;
                let arg_value = self.analyze_expr_with_context(&args[1], Some(value_type))?;
                if key_type != &Type::Unknown && !arg_key.can_coerce_to(key_type) {
                    return Err(CompileError::at(
                        self.current_span,
                        &format!(
                            "Map.insert key type mismatch: expected {}, found {}",
                            key_type, arg_key
                        ),
                        ErrorCode::E0002,
                    ));
                }
                if value_type != &Type::Unknown && !arg_value.can_coerce_to(value_type) {
                    return Err(CompileError::at(
                        self.current_span,
                        &format!(
                            "Map.insert value type mismatch: expected {}, found {}",
                            value_type, arg_value
                        ),
                        ErrorCode::E0002,
                    ));
                }
                Ok(Type::Void)
            }
            "get" => {
                if args.len() != 1 {
                    return Err(CompileError::at(
                        self.current_span,
                        &format!("Map.get expects 1 argument, got {}", args.len()),
                        ErrorCode::E0002,
                    ));
                }
                let arg_key = self.analyze_expr_with_context(&args[0], Some(key_type))?;
                if key_type != &Type::Unknown && !arg_key.can_coerce_to(key_type) {
                    return Err(CompileError::at(
                        self.current_span,
                        &format!(
                            "Map.get key type mismatch: expected {}, found {}",
                            key_type, arg_key
                        ),
                        ErrorCode::E0002,
                    ));
                }
                Ok(Type::option(value_type.clone()))
            }
            "contains" => {
                if args.len() != 1 {
                    return Err(CompileError::at(
                        self.current_span,
                        &format!("Map.contains expects 1 argument, got {}", args.len()),
                        ErrorCode::E0002,
                    ));
                }
                let arg_key = self.analyze_expr_with_context(&args[0], Some(key_type))?;
                if key_type != &Type::Unknown && !arg_key.can_coerce_to(key_type) {
                    return Err(CompileError::at(
                        self.current_span,
                        &format!(
                            "Map.contains key type mismatch: expected {}, found {}",
                            key_type, arg_key
                        ),
                        ErrorCode::E0002,
                    ));
                }
                Ok(Type::Bool)
            }
            "keys" => {
                if !args.is_empty() {
                    return Err(CompileError::at(
                        self.current_span,
                        &format!("Map.keys takes no arguments, got {}", args.len()),
                        ErrorCode::E0002,
                    ));
                }
                Ok(Type::list(key_type.clone()))
            }
            "values" => {
                if !args.is_empty() {
                    return Err(CompileError::at(
                        self.current_span,
                        &format!("Map.values takes no arguments, got {}", args.len()),
                        ErrorCode::E0002,
                    ));
                }
                Ok(Type::list(value_type.clone()))
            }
            "length" => {
                if !args.is_empty() {
                    return Err(CompileError::at(
                        self.current_span,
                        &format!("Map.length takes no arguments, got {}", args.len()),
                        ErrorCode::E0002,
                    ));
                }
                Ok(Type::Int)
            }
            other => Err(CompileError::at(
                self.current_span,
                &format!("Map has no method '{}'", other),
                ErrorCode::E0004,
            )
            .with_suggestion("Available Map methods: insert, get, contains, keys, values, length")),
        }
    }

    /// Dispatch a `Map` method in the bare `x.method` form (no parens).
    /// Only the zero-argument methods are valid; the ones that take
    /// arguments produce a "requires parentheses" diagnostic.
    fn analyze_map_field_access(
        &mut self,
        method_name: &str,
        key_type: &Type,
        value_type: &Type,
    ) -> Result<Type> {
        if !Self::is_hashable_key(key_type) {
            return Err(CompileError::at(
                self.current_span,
                &format!("Map keys must be Int, String, or Bool, found {}", key_type),
                ErrorCode::E0002,
            ));
        }
        match method_name {
            "length" => Ok(Type::Int),
            "keys" => Ok(Type::list(key_type.clone())),
            "values" => Ok(Type::list(value_type.clone())),
            "insert" | "get" | "contains" => Err(CompileError::at(
                self.current_span,
                &format!(
                    "Method '{}' on Map requires parentheses and arguments",
                    method_name
                ),
                ErrorCode::E0002,
            )),
            other => Err(CompileError::at(
                self.current_span,
                &format!("Map has no method '{}'", other),
                ErrorCode::E0004,
            )
            .with_suggestion("Available Map methods: insert, get, contains, keys, values, length")),
        }
    }

    // The pattern-type checker is unchanged.
    pub(super) fn check_pattern_type(&self, pattern: &Pattern, value_type: &Type) -> Result<()> {
        match pattern {
            Pattern::None => {
                if let Type::Option(_) = value_type {
                    Ok(())
                } else {
                    Err(CompileError::at(
                        self.current_span,
                        &format!("Cannot match None against {}", value_type),
                        ErrorCode::E0002,
                    ))
                }
            }
            Pattern::Some(_) | Pattern::SomeNested(_) => {
                if let Type::Option(_) = value_type {
                    Ok(())
                } else {
                    Err(CompileError::at(
                        self.current_span,
                        &format!("Cannot match Some against {}", value_type),
                        ErrorCode::E0002,
                    ))
                }
            }
            Pattern::Ok(_) | Pattern::OkNested(_) => {
                if let Type::Result { .. } = value_type {
                    Ok(())
                } else {
                    Err(CompileError::at(
                        self.current_span,
                        &format!("Cannot match Ok against {}", value_type),
                        ErrorCode::E0002,
                    ))
                }
            }
            Pattern::Error(_) | Pattern::ErrorNested(_) => {
                if let Type::Result { .. } = value_type {
                    Ok(())
                } else {
                    Err(CompileError::at(
                        self.current_span,
                        &format!("Cannot match Error against {}", value_type),
                        ErrorCode::E0002,
                    ))
                }
            }
            Pattern::Literal(lit) => {
                let lit_type = match &lit.kind {
                    crate::frontend::ast::ExprKind::Int(_, _) => Type::Int,
                    crate::frontend::ast::ExprKind::Number(_, _) => Type::Float,
                    crate::frontend::ast::ExprKind::String(_, _) => Type::String,
                    crate::frontend::ast::ExprKind::Bool(_, _) => Type::Bool,
                    _ => Type::Unknown,
                };
                if lit_type.can_coerce_to(value_type) {
                    Ok(())
                } else {
                    Err(CompileError::at(
                        self.current_span,
                        &format!(
                            "Cannot match literal of type {} against {}",
                            lit_type, value_type
                        ),
                        ErrorCode::E0002,
                    ))
                }
            }
            Pattern::Record { name, .. } => {
                if let Type::Record(n, _) = value_type {
                    if n == name {
                        Ok(())
                    } else {
                        Err(CompileError::at(
                            self.current_span,
                            &format!(
                                "Cannot match pattern '{}' against value of type {}",
                                name, value_type
                            ),
                            ErrorCode::E0002,
                        ))
                    }
                } else {
                    Err(CompileError::at(
                        self.current_span,
                        &format!("Cannot match record pattern against {}", value_type),
                        ErrorCode::E0002,
                    ))
                }
            }
            Pattern::Variant(name) => match value_type {
                Type::Enum {
                    name: enum_name,
                    variants,
                    ..
                } => {
                    if variants.iter().any(|v| v == name) {
                        Ok(())
                    } else {
                        Err(CompileError::at(
                            self.current_span,
                            &format!("no variant '{}' on enum '{}'", name, enum_name),
                            ErrorCode::E0004,
                        )
                        .with_suggestion(&format!(
                            "Valid variants of '{}': {}",
                            enum_name,
                            variants.join(", ")
                        )))
                    }
                }
                other => Err(CompileError::at(
                    self.current_span,
                    &format!(
                        "variant pattern requires an enum type; matched type is '{}'",
                        other
                    ),
                    ErrorCode::E0002,
                )),
            },
            _ => Ok(()),
        }
    }
    /// Verify that a `match` on a finite-domain type covers every
    /// variant, unless a wildcard or binding arm provides a fallback.
    ///
    /// Only `Option`, `Result`, and `Bool` are checked — they have
    /// small, well-defined domains. `Int`, `Float`, and `String` are
    /// effectively infinite, so exhaustiveness cannot be decided
    /// statically; those require a fallback to be useful but the
    /// analyzer does not currently enforce it.
    ///
    /// Guarded patterns (`case x if cond`) do not count as covering
    /// anything — the guard may fail, so the arm might not fire.
    #[allow(clippy::collapsible_match)]
    pub(super) fn check_match_exhaustiveness(
        &self,
        value_type: &Type,
        cases: &[MatchCaseExpr],
    ) -> Result<()> {
        let mut has_some = false;
        let mut has_none = false;
        let mut has_ok = false;
        let mut has_error = false;
        let mut has_true = false;
        let mut has_false = false;
        let mut has_fallback = false;
        // ADR 0030: user enum variant coverage. A HashSet because
        // the domain is only known from `value_type` — a match on
        // `Day` populates `covered` with `Monday`, `Tuesday`, ...
        // and the check compares against `variants`.
        let mut covered_variants: HashSet<String> = HashSet::new();

        for case in cases {
            // A guarded arm's coverage depends on its guard, which we
            // cannot decide here. Skip.
            let pattern = match &case.pattern {
                Pattern::Guarded { .. } => continue,
                p => p,
            };
            match pattern {
                Pattern::Some(_) | Pattern::SomeNested(_) => has_some = true,
                Pattern::None => has_none = true,
                Pattern::Ok(_) | Pattern::OkNested(_) => has_ok = true,
                Pattern::Error(_) | Pattern::ErrorNested(_) => has_error = true,
                Pattern::Literal(Expr {
                    kind: ExprKind::Bool(true, _),
                    ..
                }) => has_true = true,
                Pattern::Literal(Expr {
                    kind: ExprKind::Bool(false, _),
                    ..
                }) => has_false = true,
                Pattern::Wildcard | Pattern::Binding(_) => has_fallback = true,
                Pattern::Variant(name) => {
                    covered_variants.insert(name.clone());
                }
                _ => {}
            }
        }

        if has_fallback {
            return Ok(());
        }

        match value_type {
            Type::Option(_) => {
                if !(has_some && has_none) {
                    return Err(CompileError::at(self.current_span, "match on Option must handle both Some and None, or have a `case _` fallback", ErrorCode::E0002)
                    .with_suggestion(
                        "Add a `case None` arm, or a `case _` arm for the unmatched variant",
                    ));
                }
            }
            Type::Result { .. } => {
                if !(has_ok && has_error) {
                    return Err(CompileError::at(self.current_span, "match on Result must handle both Ok and Error, or have a `case _` fallback", ErrorCode::E0002)
                    .with_suggestion(
                        "Add an `case Error(e)` arm, or a `case _` arm for the unmatched variant",
                    ));
                }
            }
            Type::Bool => {
                if !(has_true && has_false) {
                    return Err(CompileError::at(self.current_span, "match on Bool must handle both true and false, or have a `case _` fallback", ErrorCode::E0002)
                    .with_suggestion(
                        "Add both `case true` and `case false`, or a `case _` arm",
                    ));
                }
            }
            // ADR 0030: user enum. Every variant must appear as a
            // `case VariantName`, or the match must have a wildcard
            // fallback. Reporting the *first* missing variant is
            // enough — the user adds one, recompiles, and the next
            // missing one appears. Listing all of them would be
            // noisier than useful for a feature where the fix is
            // usually one arm.
            Type::Enum {
                name: enum_name,
                variants,
                ..
            } => {
                for variant in variants {
                    if !covered_variants.contains(variant) {
                        return Err(CompileError::at(
                            self.current_span,
                            &format!(
                                "match on enum '{}' is missing a case for variant '{}'",
                                enum_name, variant
                            ),
                            ErrorCode::E0002,
                        )
                        .with_suggestion(&format!(
                            "Add `case {}`, or a `case _` fallback arm",
                            variant
                        )));
                    }
                }
            }
            _ => {}
        }
        Ok(())
    }
}
