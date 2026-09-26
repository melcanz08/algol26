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
                    return Err(CompileError::simple(
                        "Cannot dereference a null pointer",
                        self.current_span.start_line,
                        self.current_span.start_column,
                        "",
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
                        return Err(CompileError::simple(
                            &format!(
                                "Cannot dereference '{}': it is statically known to be null",
                                name
                            ),
                            self.current_span.start_line,
                            self.current_span.start_column,
                            "",
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
                            return Err(CompileError::simple(
                                "Cannot dereference a raw pointer outside `unsafe`",
                                self.current_span.start_line,
                                self.current_span.start_column,
                                "",
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
                    return Err(CompileError::simple(
                        "Cannot take the address of a temporary value; \
                         address-of requires a variable, array element, \
                         field, or dereference",
                        self.current_span.start_line,
                        self.current_span.start_column,
                        "",
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
                    CompileError::simple(
                        &format!("Unknown record '{}'", name),
                        self.current_span.start_line,
                        self.current_span.start_column,
                        "",
                        ErrorCode::E0003,
                    )
                    .with_suggestion(&format!("Declare it with `rec {}` before using it", name))
                })?;

                // Type-arg arity check.
                if !type_args.is_empty() && type_args.len() != rec.type_params.len() {
                    return Err(CompileError::simple(
                        &format!(
                            "Record '{}' expects {} type argument(s), got {}",
                            name,
                            rec.type_params.len(),
                            type_args.len()
                        ),
                        self.current_span.start_line,
                        self.current_span.start_column,
                        "",
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
                            CompileError::simple(
                                &format!("Record '{}' has no field '{}'", name, field_name),
                                self.current_span.start_line,
                                self.current_span.start_column,
                                "",
                                ErrorCode::E0004,
                            )
                        })?;
                    let expected = self.substitute_type_vars(field_ty, &subs);
                    let actual = self.analyze_expr_with_context(value_expr, Some(&expected))?;
                    if !actual.can_coerce_to(&expected) && expected != Type::Unknown {
                        return Err(CompileError::simple(
                            &format!(
                                "Field '{}' of '{}': expected {}, found {}",
                                field_name, name, expected, actual
                            ),
                            self.current_span.start_line,
                            self.current_span.start_column,
                            "",
                            ErrorCode::E0002,
                        ));
                    }
                    seen.insert(field_name.clone());
                }
                for (field_name, _) in &rec.fields {
                    if !seen.contains(field_name) {
                        return Err(CompileError::simple(
                            &format!("Missing field '{}' in literal for '{}'", field_name, name),
                            self.current_span.start_line,
                            self.current_span.start_column,
                            "",
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
                    return Err(CompileError::simple(
                        "If condition must be Bool",
                        self.current_span.start_line,
                        self.current_span.start_column,
                        "",
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
                            return Err(CompileError::simple(
                                "if branches produce inconsistent results: one branch yields a value, the other does not",
                                self.current_span.start_line,
                                self.current_span.start_column,
                                "",
                                ErrorCode::E0002,
                            )
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
                    return Err(CompileError::simple(
                        "Match expression must have at least one case",
                        self.current_span.start_line,
                        self.current_span.start_column,
                        "",
                        ErrorCode::E0002,
                    ));
                }

                let mut arm_types: Vec<Type> = Vec::with_capacity(cases.len());
                let mut arm_exits: Vec<SemanticState> = Vec::with_capacity(cases.len());

                for case in cases {
                    self.check_pattern_type(&case.pattern, &value_type)?;

                    let (arm_result, arm_exit) = self.in_branch(|a| {
                        a.push_scope();
                        let r: Result<Type> = (|| {
                            a.bind_pattern_variables(&case.pattern, &value_type)?;

                            if let Pattern::Guarded { condition, .. } = &case.pattern {
                                let cond_type = a.analyze_expr(condition)?;
                                if cond_type != Type::Bool && cond_type != Type::Unknown {
                                    return Err(CompileError::simple(
                                        "Pattern guard must be boolean",
                                        a.current_span.start_line,
                                        a.current_span.start_column,
                                        "",
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
                        return Err(CompileError::simple(
                            "match arms produce inconsistent results: some arms yield a value, others do not",
                            self.current_span.start_line,
                            self.current_span.start_column,
                            "",
                            ErrorCode::E0002,
                        )
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
                        return Err(CompileError::simple(
                            &format!("try body must produce a Result<T, E>, found {}", other),
                            self.current_span.start_line,
                            self.current_span.start_column,
                            "",
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
                    return Err(CompileError::simple(
                        &format!(
                            "try/catch type mismatch: try body yields {}, catch yields {}",
                            ok_type, catch_type
                        ),
                        self.current_span.start_line,
                        self.current_span.start_column,
                        "",
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
                    return Err(CompileError::simple(
                        &format!("For loop requires list, found {}", iter_type),
                        span.start_line,
                        span.start_column,
                        "",
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
                    return Err(CompileError::simple(
                        &format!("Cannot move '{}' in loop body", moved_var),
                        span.start_line,
                        span.start_column,
                        "",
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
                    return Err(CompileError::simple(
                        &format!("While condition must be Bool, found {}", cond_type),
                        span.start_line,
                        span.start_column,
                        "",
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
                let line = span.start_line;
                let column = span.start_column;
                if self.is_moved(name) {
                    return Err(CompileError::simple(
                        &format!("Use of moved variable '{}'", name),
                        line,
                        column,
                        "",
                        ErrorCode::E0007,
                    )
                    .with_suggestion(
                        "Variable ownership was transferred and cannot be used in this scope",
                    ));
                }
                if self.is_mutably_borrowed(name) && !self.in_mut_borrow {
                    return Err(CompileError::simple(
                        &format!("Cannot read '{}' while it is mutably borrowed", name),
                        line,
                        column,
                        "",
                        ErrorCode::E0007,
                    )
                    .with_suggestion("Wait for the mutable borrow to end before reading"));
                }
                self.lookup_variable(name).map(|(t, _)| t).ok_or_else(|| {
                    CompileError::simple(
                        &format!("Undefined variable '{}'", name),
                        line,
                        column,
                        "",
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
                        return Err(CompileError::simple(
                            &format!("Array access requires list, found {}", array_type),
                            self.current_span.start_line,
                            self.current_span.start_column,
                            "",
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
                    return Err(CompileError::simple(
                        &format!(
                            "Array index out of bounds: index {} is out of bounds for '{}' with length {}",
                            idx_val, var_name, len
                        ),
                        self.current_span.start_line, self.current_span.start_column, "", ErrorCode::E0004,
                    ).with_suggestion(&format!(
                        "Valid indices are 0..{} for array of length {}", len - 1, len
                    )));
                }
                if index_type != Type::Int && index_type != Type::Unknown {
                    return Err(CompileError::simple(
                        &format!("Array index must be Int, found {}", index_type),
                        self.current_span.start_line,
                        self.current_span.start_column,
                        "",
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
                            Err(CompileError::simple(
                                &format!(
                                    "Addition requires matching types, found {} and {}",
                                    left_type, right_type
                                ),
                                self.current_span.start_line,
                                self.current_span.start_column,
                                "",
                                ErrorCode::E0002,
                            )
                            .with_suggestion("Use matching types or add type conversion"))
                        }
                    }
                    BinOp::Subtract | BinOp::Multiply | BinOp::Divide => {
                        if left_type.is_numeric() && right_type.is_numeric() {
                            Ok(left_type.common_supertype(&right_type))
                        } else {
                            Err(CompileError::simple(
                                &format!(
                                    "Arithmetic requires numeric types, found {} and {}",
                                    left_type, right_type
                                ),
                                self.current_span.start_line,
                                self.current_span.start_column,
                                "",
                                ErrorCode::E0002,
                            )
                            .with_suggestion("Both operands must be numeric (Int or Float)"))
                        }
                    }
                    BinOp::Greater | BinOp::Less | BinOp::GreaterEqual | BinOp::LessEqual => {
                        if left_type.is_numeric() && right_type.is_numeric() {
                            Ok(Type::Bool)
                        } else {
                            Err(CompileError::simple(
                                &format!(
                                    "Comparison requires numeric types, found {} and {}",
                                    left_type, right_type
                                ),
                                self.current_span.start_line,
                                self.current_span.start_column,
                                "",
                                ErrorCode::E0002,
                            )
                            .with_suggestion("Use numeric types for comparison"))
                        }
                    }
                    BinOp::Equal | BinOp::NotEqual => {
                        if left_type == right_type
                            || (left_type.is_numeric() && right_type.is_numeric())
                        {
                            Ok(Type::Bool)
                        } else {
                            Err(CompileError::simple(
                                &format!(
                                    "Equality requires matching types, found {} and {}",
                                    left_type, right_type
                                ),
                                self.current_span.start_line,
                                self.current_span.start_column,
                                "",
                                ErrorCode::E0002,
                            )
                            .with_suggestion("Use matching types for equality comparison"))
                        }
                    }
                    BinOp::And | BinOp::Or => {
                        if left_type == Type::Bool && right_type == Type::Bool {
                            Ok(Type::Bool)
                        } else {
                            Err(CompileError::simple(
                                &format!(
                                    "Logical operators require boolean operands, found {} and {}",
                                    left_type, right_type
                                ),
                                self.current_span.start_line,
                                self.current_span.start_column,
                                "",
                                ErrorCode::E0002,
                            )
                            .with_suggestion("Use 'and' and 'or' only with boolean values"))
                        }
                    }
                }
            }
            ExprKind::FunctionCall { name, args, .. } => {
                let clean_name = name.trim_end_matches("()");

                if clean_name.contains('.') {
                    let parts: Vec<&str> = clean_name.split('.').collect();
                    if parts.len() == 2 {
                        let receiver = parts[0];
                        let method_name = parts[1];

                        if let Some((receiver_type, _)) = self.lookup_variable(receiver) {
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
                                        return Err(CompileError::simple(
                                            &format!(
                                                "Method '{}' expects {} argument(s) after the receiver, got {}",
                                                method_name, expected_extra, args.len()
                                            ),
                                            self.current_span.start_line, self.current_span.start_column, "", ErrorCode::E0002,
                                        ));
                                    }
                                    return Ok(func_info.return_type);
                                }
                            }

                            // ─── Trait-based method resolution ───
                            if let Some(method) =
                                self.resolve_trait_method(&receiver_type, method_name)
                            {
                                if args.len() != method.params.len() {
                                    return Err(CompileError::simple(
                                        &format!(
                                            "Method '{}' expects {} arguments, got {}",
                                            method_name,
                                            method.params.len(),
                                            args.len()
                                        ),
                                        self.current_span.start_line,
                                        self.current_span.start_column,
                                        "",
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
                                        return Err(CompileError::simple(
                                            &format!(
                                                "Argument '{}' type mismatch: expected {}, found {}",
                                                param_name, expected_type, arg_type
                                            ),
                                            self.current_span.start_line, self.current_span.start_column, "", ErrorCode::E0002,
                                        ));
                                    }
                                }
                                return Ok(method
                                    .return_type
                                    .as_ref()
                                    .map(|t| t.to_type())
                                    .unwrap_or(Type::Void));
                            }

                            // ─── Neither built-in nor trait method ───
                            return Err(CompileError::simple(
                                &format!(
                                    "Type {} does not have method '{}'",
                                    receiver_type, method_name
                                ),
                                self.current_span.start_line,
                                self.current_span.start_column,
                                "",
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
                    return Err(CompileError::simple(
                        &format!("`{}` requires an `unsafe` block", clean_name),
                        self.current_span.start_line,
                        self.current_span.start_column,
                        "",
                        ErrorCode::E0007,
                    )
                    .with_suggestion(&format!(
                        "Wrap the `{}` call in `unsafe {{ ... }}`",
                        clean_name,
                    )));
                }

                let func_info = self.functions.get(clean_name).cloned().ok_or_else(|| {
                    CompileError::simple(
                        &format!("Undefined function '{}'", name),
                        self.current_span.start_line,
                        self.current_span.start_column,
                        "",
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
                    return Err(CompileError::simple(
                        &format!(
                            "Function '{}' expects {}, got {}",
                            name,
                            expected_msg,
                            args.len()
                        ),
                        self.current_span.start_line,
                        self.current_span.start_column,
                        "",
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
                    if let Type::TypeVar(tv) = &resolved_param_type {
                        if let Some(existing_binding) = type_bindings.get(tv) {
                            if existing_binding != &arg_type && existing_binding != &Type::Unknown {
                                return Err(CompileError::simple(
                                    &format!(
                                        "Type mismatch for generic parameter '{}': expected {}, found {}",
                                        tv, existing_binding, arg_type
                                    ),
                                    self.current_span.start_line, self.current_span.start_column, "", ErrorCode::E0002,
                                ));
                            }
                        } else {
                            type_bindings.insert(tv.clone(), arg_type.clone());
                        }
                    } else if !arg_type.can_coerce_to(&resolved_param_type)
                        && resolved_param_type != Type::Unknown
                        && !matches!(resolved_param_type, Type::List(_))
                    {
                        return Err(CompileError::simple(
                            &format!(
                                "Argument '{}' type mismatch: expected {}, found {}",
                                param_name, resolved_param_type, arg_type
                            ),
                            self.current_span.start_line,
                            self.current_span.start_column,
                            "",
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
                            Err(CompileError::simple(
                                &format!("Cannot negate non-numeric type {}", operand_type),
                                self.current_span.start_line,
                                self.current_span.start_column,
                                "",
                                ErrorCode::E0002,
                            )
                            .with_suggestion("Negation requires Int or Float operand"))
                        }
                    }
                    crate::frontend::ast::UnaryOp::Not => {
                        if operand_type == Type::Bool || operand_type == Type::Unknown {
                            Ok(Type::Bool)
                        } else {
                            Err(CompileError::simple(
                                &format!("Logical not requires Bool, found {}", operand_type),
                                self.current_span.start_line,
                                self.current_span.start_column,
                                "",
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

                // The parser produces `FieldAccess` for both `p.x`
                // (record field) and `s.length` (zero-argument method
                // call, the pre-records form). Disambiguate by type:
                // records use field lookup, everything else falls
                // through to built-in / trait method dispatch — the
                // same code path `s.length()` already uses.
                if let Type::Record(rec_name, rec_args) = &obj_ty {
                    let rec = self.records.get(rec_name).cloned().ok_or_else(|| {
                        CompileError::simple(
                            &format!("Unknown record '{}'", rec_name),
                            self.current_span.start_line,
                            self.current_span.start_column,
                            "",
                            ErrorCode::E0003,
                        )
                    })?;
                    let (_, field_ty) =
                        rec.fields.iter().find(|(n, _)| n == field).ok_or_else(|| {
                            CompileError::simple(
                                &format!("Record '{}' has no field '{}'", rec_name, field),
                                self.current_span.start_line,
                                self.current_span.start_column,
                                "",
                                ErrorCode::E0004,
                            )
                        })?;
                    let mut subs = HashMap::new();
                    for (p, a) in rec.type_params.iter().zip(rec_args.iter()) {
                        subs.insert(p.clone(), a.clone());
                    }
                    return Ok(self.substitute_type_vars(field_ty, &subs));
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
                            return Err(CompileError::simple(
                                &format!(
                                    "Method '{}' on {} expects {} argument(s); \
                                     `x.{}` (no parens) is only valid for zero-argument methods",
                                    field,
                                    obj_ty,
                                    func_info.params.len().saturating_sub(1),
                                    field,
                                ),
                                self.current_span.start_line,
                                self.current_span.start_column,
                                "",
                                ErrorCode::E0002,
                            ));
                        }
                        return Ok(func_info.return_type);
                    }
                }

                if let Some(method) = self.resolve_trait_method(&obj_ty, field) {
                    if !method.params.is_empty() {
                        return Err(CompileError::simple(
                            &format!(
                                "Method '{}' on {} expects {} argument(s); \
                                 `x.{}` (no parens) is only valid for zero-argument methods",
                                field,
                                obj_ty,
                                method.params.len(),
                                field,
                            ),
                            self.current_span.start_line,
                            self.current_span.start_column,
                            "",
                            ErrorCode::E0002,
                        ));
                    }
                    return Ok(method
                        .return_type
                        .as_ref()
                        .map(|t| t.to_type())
                        .unwrap_or(Type::Void));
                }

                Err(CompileError::simple(
                    &format!("Type {} has no field or method '{}'", obj_ty, field),
                    self.current_span.start_line,
                    self.current_span.start_column,
                    "",
                    ErrorCode::E0004,
                )
                .with_suggestion(
                    "Field access requires a record value; zero-argument method calls \
                     accept `x.method` or `x.method()`",
                ))
            }
        }
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

    // The pattern-type checker is unchanged.
    pub(super) fn check_pattern_type(&self, pattern: &Pattern, value_type: &Type) -> Result<()> {
        match pattern {
            Pattern::None => {
                if let Type::Option(_) = value_type {
                    Ok(())
                } else {
                    Err(CompileError::simple(
                        &format!("Cannot match None against {}", value_type),
                        self.current_span.start_line,
                        self.current_span.start_column,
                        "",
                        ErrorCode::E0002,
                    ))
                }
            }
            Pattern::Some(_) | Pattern::SomeNested(_) => {
                if let Type::Option(_) = value_type {
                    Ok(())
                } else {
                    Err(CompileError::simple(
                        &format!("Cannot match Some against {}", value_type),
                        self.current_span.start_line,
                        self.current_span.start_column,
                        "",
                        ErrorCode::E0002,
                    ))
                }
            }
            Pattern::Ok(_) | Pattern::OkNested(_) => {
                if let Type::Result { .. } = value_type {
                    Ok(())
                } else {
                    Err(CompileError::simple(
                        &format!("Cannot match Ok against {}", value_type),
                        self.current_span.start_line,
                        self.current_span.start_column,
                        "",
                        ErrorCode::E0002,
                    ))
                }
            }
            Pattern::Error(_) | Pattern::ErrorNested(_) => {
                if let Type::Result { .. } = value_type {
                    Ok(())
                } else {
                    Err(CompileError::simple(
                        &format!("Cannot match Error against {}", value_type),
                        self.current_span.start_line,
                        self.current_span.start_column,
                        "",
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
                    Err(CompileError::simple(
                        &format!(
                            "Cannot match literal of type {} against {}",
                            lit_type, value_type
                        ),
                        self.current_span.start_line,
                        self.current_span.start_column,
                        "",
                        ErrorCode::E0002,
                    ))
                }
            }
            Pattern::Record { name, .. } => {
                if let Type::Record(n, _) = value_type {
                    if n == name {
                        Ok(())
                    } else {
                        Err(CompileError::simple(
                            &format!(
                                "Cannot match pattern '{}' against value of type {}",
                                name, value_type
                            ),
                            self.current_span.start_line,
                            self.current_span.start_column,
                            "",
                            ErrorCode::E0002,
                        ))
                    }
                } else {
                    Err(CompileError::simple(
                        &format!("Cannot match record pattern against {}", value_type),
                        self.current_span.start_line,
                        self.current_span.start_column,
                        "",
                        ErrorCode::E0002,
                    ))
                }
            }
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
                _ => {}
            }
        }

        if has_fallback {
            return Ok(());
        }

        match value_type {
            Type::Option(_) => {
                if !(has_some && has_none) {
                    return Err(CompileError::simple(
                        "match on Option must handle both Some and None, or have a `case _` fallback",
                        self.current_span.start_line,
                        self.current_span.start_column,
                        "",
                        ErrorCode::E0002,
                    )
                    .with_suggestion(
                        "Add a `case None` arm, or a `case _` arm for the unmatched variant",
                    ));
                }
            }
            Type::Result { .. } => {
                if !(has_ok && has_error) {
                    return Err(CompileError::simple(
                        "match on Result must handle both Ok and Error, or have a `case _` fallback",
                        self.current_span.start_line,
                        self.current_span.start_column,
                        "",
                        ErrorCode::E0002,
                    )
                    .with_suggestion(
                        "Add an `case Error(e)` arm, or a `case _` arm for the unmatched variant",
                    ));
                }
            }
            Type::Bool => {
                if !(has_true && has_false) {
                    return Err(CompileError::simple(
                        "match on Bool must handle both true and false, or have a `case _` fallback",
                        self.current_span.start_line,
                        self.current_span.start_column,
                        "",
                        ErrorCode::E0002,
                    )
                    .with_suggestion(
                        "Add both `case true` and `case false`, or a `case _` arm",
                    ));
                }
            }
            _ => {}
        }
        Ok(())
    }
}
