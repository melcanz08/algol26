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
        let ty = self.analyze_expr_inner(expr, expected_type)?;

        self.type_table
            .insert(expr as *const Expr as usize, ty.clone());
        Ok(ty)
    }
    // The actual match arm dispatch (renamed from the original).
    pub(super) fn analyze_expr_inner(
        &mut self,
        expr: &Expr,
        expected_type: Option<&Type>,
    ) -> Result<Type> {
        match expr {
            Expr::Borrow { expr } => {
                if let Expr::Var(name, _) = expr.as_ref() {
                    self.check_borrow_rules(name, false)?;
                    self.mark_borrowed(name);
                    let inner_type = self.analyze_expr(expr)?;
                    return Ok(Type::borrow(inner_type));
                }
                let inner_type = self.analyze_expr(expr)?;
                Ok(Type::borrow(inner_type))
            }
            Expr::MutBorrow { expr } => {
                if let Expr::Var(name, _) = expr.as_ref() {
                    // Do NOT release existing borrows here — that would undo
                    // the very borrow we just registered. `check_borrow_rules`
                    // will correctly reject a second mut-borrow of the same source.
                    self.check_borrow_rules(name, true)?;
                    let inner_type = self.lookup_variable(name).map(|(t, _)| t).unwrap_or(Type::Unknown);
                    return Ok(Type::mut_borrow(inner_type));
                }
                let inner_type = self.analyze_expr(expr)?;
                Ok(Type::mut_borrow(inner_type))
            }
            Expr::Deref { expr } => {
                // Rule: dereferencing a value statically known to be
                // null is a compile-time safety error. The language
                // permits `null` as a value of type `Ptr`; it does not
                // permit a deref whose operand is provably null.
                if matches!(expr.as_ref(), Expr::NullPtr) {
                    return Err(CompileError::simple(
                        "Cannot dereference a null pointer",
                        0, 0, "", ErrorCode::E0007,
                    ).with_suggestion(
                        "Check the pointer for null before dereferencing, \
                         e.g. with `if p != null then ...`",
                    ));
                }
                if let Expr::Var(name, _) = expr.as_ref() {
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
                            0, 0, "", ErrorCode::E0007,
                        ).with_suggestion(&format!(
                            "Check '{}' for null before dereferencing",
                            name
                        )));
                    }
                }

                let inner_type = self.analyze_expr(expr)?;
                match inner_type {
                    Type::Pointer(t) => Ok(*t),
                    Type::Borrow(t) => Ok(*t),
                    Type::MutBorrow(t) => Ok(*t),
                    _ => Ok(Type::Unknown),
                }
            }
            Expr::AddrOf { expr } => {
                let inner_type = self.analyze_expr(expr)?;
                Ok(Type::pointer(inner_type))
            }
            Expr::Number(_) => Ok(Type::Float),
            Expr::Int(_) => Ok(Type::Int),
            Expr::String(_) => Ok(Type::String),
            Expr::Bool(_) => Ok(Type::Bool),
            Expr::List(elements) => {
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
            Expr::Some { value } => {
                let inner = self.analyze_expr(value)?;
                Ok(Type::option(inner))
            }
            Expr::None => {
                if let Some(Type::Option(inner)) = expected_type {
                    Ok(Type::option((**inner).clone()))
                } else {
                    Ok(Type::option(Type::Unknown))
                }
            }
            Expr::Ok { value } => {
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
            Expr::Error { value } => {
                let inner = self.analyze_expr(value)?;

                let ok_type = match expected_type {
                    Some(Type::Result { ok, .. }) => (**ok).clone(),
                    _ => Type::Unknown,
                };

                Ok(Type::result(ok_type, inner))
            }
            Expr::Block { statements, trailing_expr } => {
                for s in statements { self.analyze_stmt(s)?; }
                let result = if let Some(expr) = trailing_expr {
                    self.analyze_expr(expr)?
                } else {
                    Type::Void
                };
                Ok(result)
            }
            Expr::If { condition, then_branch, else_branch } => {
                let cond_type = self.analyze_expr(condition)?;
                if cond_type != Type::Bool && cond_type != Type::Unknown && cond_type != Type::Void {
                    return Err(CompileError::simple(
                        "If condition must be Bool", 0, 0, "", ErrorCode::E0002,
                    ));
                }
                self.push_scope();
                let then_type = self.analyze_expr(then_branch)?;
                self.pop_scope();
                if let Some(else_expr) = else_branch {
                    self.push_scope();
                    let else_type = self.analyze_expr(else_expr)?;
                    self.pop_scope();

                    // A value-producing `if` must have both branches agree
                    // on whether they produce a value. If one is Void and
                    // the other isn't, the program is ill-formed — the
                    // result type is neither a well-defined value nor a
                    // deliberate void.
                    let then_is_void = then_type == Type::Void;
                    let else_is_void = else_type == Type::Void;
                    if then_is_void != else_is_void {
                        return Err(CompileError::simple(
                            "if branches produce inconsistent results: one branch yields a value, the other does not",
                            0, 0, "", ErrorCode::E0002,
                        ).with_suggestion(
                            "Ensure both branches end with an expression, or neither does",
                        ));
                    }

                    Ok(then_type.common_supertype(&else_type))
                } else {
                    Ok(Type::Void)
                }
            }
            Expr::Match { value, cases } => {
                let value_type = self.analyze_expr(value)?;
                if let Some(first_case) = cases.first() {
                    self.check_pattern_type(&first_case.pattern, &value_type)?;
                    self.push_scope();
                    self.bind_pattern_variables(&first_case.pattern, &value_type);
                    if let Pattern::Guarded { condition, .. } = &first_case.pattern {
                        let cond_type = self.analyze_expr(condition)?;
                        if cond_type != Type::Bool && cond_type != Type::Unknown {
                            return Err(CompileError::simple(
                                "Pattern guard must be boolean", 0, 0, "", ErrorCode::E0002,
                            ));
                        }
                    }
                    let first_type = self.analyze_expr(&first_case.body)?;
                    self.pop_scope();
                    let first_is_void = first_type == Type::Void;
                    let mut result_type = first_type.clone();

                    for case in &cases[1..] {
                        self.check_pattern_type(&case.pattern, &value_type)?;
                        self.push_scope();
                        self.bind_pattern_variables(&case.pattern, &value_type);
                        if let Pattern::Guarded { condition, .. } = &case.pattern {
                            let cond_type = self.analyze_expr(condition)?;
                            if cond_type != Type::Bool && cond_type != Type::Unknown {
                                return Err(CompileError::simple(
                                    "Pattern guard must be boolean", 0, 0, "", ErrorCode::E0002,
                                ));
                            }
                        }
                        let case_type = self.analyze_expr(&case.body)?;
                        self.pop_scope();

                        // All match arms must agree on whether they
                        // produce a value. A mix of Void and non-Void
                        // is a semantic error, not an Unknown type.
                        let case_is_void = case_type == Type::Void;
                        if case_is_void != first_is_void {
                            return Err(CompileError::simple(
                                "match arms produce inconsistent results: some arms yield a value, others do not",
                                0, 0, "", ErrorCode::E0002,
                            ).with_suggestion(
                                "Ensure all arms end with an expression, or none do",
                            ));
                        }

                        result_type = result_type.common_supertype(&case_type);
                    }
                    Ok(result_type)
                } else {
                    Err(CompileError::simple(
                        "Match expression must have at least one case", 0, 0, "", ErrorCode::E0002,
                    ))
                }
            }
            Expr::TryCatch { try_branch, catch_var, catch_branch, finally_body } => {
                let try_type = self.analyze_expr(try_branch)?;

                // The try body must evaluate to a Result<T, E>.
                let (ok_type, err_type) = match &try_type {
                    Type::Result { ok, error } => ((**ok).clone(), (**error).clone()),
                    Type::Unknown => (Type::Unknown, Type::Unknown),
                    other => {
                        return Err(CompileError::simple(
                            &format!(
                                "try body must produce a Result<T, E>, found {}",
                                other
                            ),
                            0, 0, "", ErrorCode::E0002,
                        ).with_suggestion(
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
                let catch_result =
                    self.analyze_expr_with_context(catch_branch, Some(&ok_type));
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
                        0, 0, "", ErrorCode::E0002,
                    ).with_suggestion(
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
            Expr::For { var, iterable, body, trailing_expr, span } => {
                let iter_type = self.analyze_expr(iterable)?;
                let elem_type = if let Type::List(t) = iter_type.clone() {
                    *t
                } else if iter_type != Type::Unknown {
                    return Err(CompileError::simple(
                        &format!("For loop requires list, found {}", iter_type),
                        span.start_line, span.start_column, "", ErrorCode::E0002,
                    ));
                } else {
                    Type::Unknown
                };
                let outer_borrowed = self.borrowed_vars.last().cloned().unwrap_or_default();
                let outer_mutably_borrowed = self.mutably_borrowed.last().cloned().unwrap_or_default();

                self.push_scope();
                self.declare_variable(var, elem_type, false)?;
                let moves_before = self.all_moved_vars();
                for s in body { self.analyze_stmt(s)?; }
                let moves_after = self.all_moved_vars();
                let new_moves: Vec<String> = moves_after.iter()
                    .filter(|v| !moves_before.contains(v))
                    .cloned().collect();

                let result_type = if let Some(expr) = trailing_expr {
                    self.analyze_expr(expr)?
                } else {
                    Type::Void
                };
                self.pop_scope();
                if let Some(scope) = self.borrowed_vars.last_mut() {
                    *scope = outer_borrowed;
                }
                if let Some(scope) = self.mutably_borrowed.last_mut() {
                    *scope = outer_mutably_borrowed;
                }
                // `for` bodies execute at least once on any non-empty
                // iterable, so a move here would fire again on the next
                // iteration. Reject rather than propagate.
                if !new_moves.is_empty() {
                    let moved_var = new_moves[0].clone();
                    return Err(CompileError::simple(
                        &format!("Cannot move '{}' in loop body", moved_var),
                        0, 0, "", ErrorCode::E0008,
                    ));
                }
                Ok(result_type)
            }
            // See the module doc comment "Loop ownership analysis".
            // Borrows are restored; moves are propagated outward
            // because the loop may run zero times.
            Expr::While { condition, body, trailing_expr, span } => {
                let cond_type = self.analyze_expr(condition)?;
                if cond_type != Type::Bool && cond_type != Type::Unknown {
                    return Err(CompileError::simple(
                        &format!("While condition must be Bool, found {}", cond_type),
                        span.start_line, span.start_column, "", ErrorCode::E0002,
                    ));
                }
                let outer_borrowed = self.borrowed_vars.last().cloned().unwrap_or_default();
                let outer_mutably_borrowed = self.mutably_borrowed.last().cloned().unwrap_or_default();

                self.push_scope();
                for s in body { self.analyze_stmt(s)?; }
                let moved_in_loop = self.moved_vars.last().cloned().unwrap_or_default();
                let result_type = if let Some(expr) = trailing_expr {
                    self.analyze_expr(expr)?
                } else {
                    Type::Void
                };
                self.pop_scope();
                if let Some(scope) = self.borrowed_vars.last_mut() {
                    *scope = outer_borrowed;
                }
                if let Some(scope) = self.mutably_borrowed.last_mut() {
                    *scope = outer_mutably_borrowed;
                }
                // `while` may run zero times, so we cannot prove the move
                // happened. Mark the variable as potentially moved in the
                // enclosing scope; subsequent uses will error.
                if let Some(parent_scope) = self.moved_vars.last_mut() {
                    for var in &moved_in_loop {
                        if !parent_scope.contains(var) {
                            parent_scope.push(var.clone());
                        }
                    }
                }
                Ok(result_type)
            }
            Expr::Var(name, span) => {
                let line = span.start_line;
                let column = span.start_column;
                if self.is_moved(name) {
                    return Err(CompileError::simple(
                        &format!("Use of moved variable '{}'", name),
                        line, column, "", ErrorCode::E0007,
                    ).with_suggestion(
                        "Variable ownership was transferred and cannot be used in this scope",
                    ));
                }
                if self.is_mutably_borrowed(name) && !self.in_mut_borrow {
                    return Err(CompileError::simple(
                        &format!("Cannot read '{}' while it is mutably borrowed", name),
                        line, column, "", ErrorCode::E0007,
                    ).with_suggestion("Wait for the mutable borrow to end before reading"));
                }
                self.lookup_variable(name).map(|(t, _)| t).ok_or_else(|| {
                    CompileError::simple(
                        &format!("Undefined variable '{}'", name),
                        line, column, "", ErrorCode::E0003,
                    ).with_suggestion(&format!(
                        "Declare '{}' with 'var {} := ...' or 'val {} := ...' in this scope",
                        name, name, name
                    ))
                })
            }
            Expr::ArrayAccess { array, index } => {
                let array_type = self.analyze_expr(array)?;
                let element_type = match array_type {
                    Type::List(element_type) => *element_type,
                    Type::Unknown => Type::Unknown,
                    _ => {
                        return Err(CompileError::simple(
                            &format!("Array access requires list, found {}", array_type),
                            0, 0, "", ErrorCode::E0002,
                        ));
                    }
                };
                let index_type = self.analyze_expr(index)?;

                let mut out_of_bounds: Option<(i64, usize, String)> = None;
                let literal_index: Option<i64> = match index.as_ref() {
                    Expr::Int(v) => Some(*v),
                    Expr::Number(f) => Some(*f as i64),
                    _ => None,
                };
                if let Some(idx_val) = literal_index {
                    if let Expr::Var(var_name, _) = array.as_ref() {
                        if let Some(list_len) = self.lookup_list_length(var_name) {
                            if idx_val < 0 || (idx_val as usize) >= list_len {
                                out_of_bounds = Some((idx_val, list_len, var_name.clone()));
                            }
                        }
                    }
                    if let Expr::List(elements) = array.as_ref() {
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
                        0, 0, "", ErrorCode::E0004,
                    ).with_suggestion(&format!(
                        "Valid indices are 0..{} for array of length {}", len - 1, len
                    )));
                }
                if index_type != Type::Int && index_type != Type::Unknown {
                    return Err(CompileError::simple(
                        &format!("Array index must be Int, found {}", index_type),
                        0, 0, "", ErrorCode::E0002,
                    ).with_suggestion(&format!(
                        "Use an Int index or convert {} with int({})", index_type, index_type
                    )));
                }
                Ok(element_type)
            }
            Expr::Binary { left, op, right } => {
                let mut left_type = self.analyze_expr(left)?;
                let mut right_type = self.analyze_expr(right)?;

                if let Type::Borrow(inner) = &left_type { left_type = (**inner).clone(); }
                if let Type::MutBorrow(inner) = &left_type { left_type = (**inner).clone(); }
                if let Type::Borrow(inner) = &right_type { right_type = (**inner).clone(); }
                if let Type::MutBorrow(inner) = &right_type { right_type = (**inner).clone(); }

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
                                0, 0, "", ErrorCode::E0002,
                            ).with_suggestion("Use matching types or add type conversion"))
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
                                0, 0, "", ErrorCode::E0002,
                            ).with_suggestion("Both operands must be numeric (Int or Float)"))
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
                                0, 0, "", ErrorCode::E0002,
                            ).with_suggestion("Use numeric types for comparison"))
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
                                0, 0, "", ErrorCode::E0002,
                            ).with_suggestion("Use matching types for equality comparison"))
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
                                0, 0, "", ErrorCode::E0002,
                            ).with_suggestion("Use 'and' and 'or' only with boolean values"))
                        }
                    }
                }
            }
            Expr::FunctionCall { name, args, .. } => {
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
                                if let Some(func_info) = self.functions.get(&builtin_form).cloned() {
                                    // Analyze each explicit arg. The receiver is
                                    // implicitly the first argument at IR-build time,
                                    // so its type is already known.
                                    for arg in args {
                                        self.analyze_expr(arg)?;
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
                                            0, 0, "", ErrorCode::E0002,
                                        ));
                                    }
                                    return Ok(func_info.return_type);
                                }
                            }

                            // ─── Trait-based method resolution ───
                            if let Some(method) = self.resolve_trait_method(&receiver_type, method_name) {
                                if args.len() != method.params.len() {
                                    return Err(CompileError::simple(
                                        &format!(
                                            "Method '{}' expects {} arguments, got {}",
                                            method_name, method.params.len(), args.len()
                                        ),
                                        0, 0, "", ErrorCode::E0002,
                                    ));
                                }
                                for (arg, (param_name, param_type)) in args.iter().zip(&method.params) {
                                    let arg_type = self.analyze_expr(arg)?;
                                    let expected_type = match param_type {
                                        Some(s) => s.to_type(),
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
                                            0, 0, "", ErrorCode::E0002,
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
                                0, 0, "", ErrorCode::E0004,
                            ).with_suggestion(&format!(
                                "Implement a trait for {} that provides method '{}', \
                                 or check if '{}' is a built-in",
                                receiver_type, method_name, method_name
                            )));
                        }
                    }
                }

                let func_info = self.functions.get(clean_name).cloned().ok_or_else(|| {
                    CompileError::simple(
                        &format!("Undefined function '{}'", name),
                        0, 0, "", ErrorCode::E0004,
                    ).with_suggestion(&format!(
                        "Check if function '{}' is defined or imported", name
                    ))
                })?;

                if args.len() != func_info.params.len() {
                    return Err(CompileError::simple(
                        &format!(
                            "Function '{}' expects {} arguments, got {}",
                            name, func_info.params.len(), args.len()
                        ),
                        0, 0, "", ErrorCode::E0002,
                    ).with_suggestion(&format!(
                        "Provide exactly {} argument(s) to '{}'",
                        func_info.params.len(), name
                    )));
                }

                let mut type_bindings: HashMap<String, Type> = HashMap::new();
                for (arg, (param_name, param_type)) in args.iter().zip(&func_info.params) {
                    let arg_type = self.analyze_expr_with_context(arg, Some(param_type))?;
                    let resolved_param_type = self.resolve_type(param_type);
                    if let Type::TypeVar(tv) = &resolved_param_type {
                        if let Some(existing_binding) = type_bindings.get(tv) {
                            if existing_binding != &arg_type && existing_binding != &Type::Unknown {
                                return Err(CompileError::simple(
                                    &format!(
                                        "Type mismatch for generic parameter '{}': expected {}, found {}",
                                        tv, existing_binding, arg_type
                                    ),
                                    0, 0, "", ErrorCode::E0002,
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
                            0, 0, "", ErrorCode::E0002,
                        ).with_suggestion(&format!(
                            "Convert the argument to {} or change the function signature",
                            resolved_param_type
                        )));
                    }
                }
                let return_type = self.substitute_type_vars(&func_info.return_type, &type_bindings);
                Ok(return_type)
            }
            Expr::Unary { op, expr, .. } => {
                let operand_type = self.analyze_expr_with_context(expr, expected_type)?;
                match op {
                    crate::frontend::ast::UnaryOp::Negate => {
                        if operand_type.is_numeric() || operand_type == Type::Unknown {
                            Ok(operand_type)
                        } else {
                            Err(CompileError::simple(
                                &format!("Cannot negate non-numeric type {}", operand_type),
                                0, 0, "", ErrorCode::E0002,
                            ).with_suggestion("Negation requires Int or Float operand"))
                        }
                    }
                    crate::frontend::ast::UnaryOp::Not => {
                        if operand_type == Type::Bool || operand_type == Type::Unknown {
                            Ok(Type::Bool)
                        } else {
                            Err(CompileError::simple(
                                &format!("Logical not requires Bool, found {}", operand_type),
                                0, 0, "", ErrorCode::E0002,
                            ).with_suggestion("Use 'not' only with boolean values"))
                        }
                    }
                }
            }
            Expr::PtrLiteral(_) => Ok(Type::Ptr),
            Expr::NullPtr => Ok(Type::Ptr),
            Expr::Cast { expr: cast_expr, target_type } => {
                let _source_type = self.analyze_expr(cast_expr)?;
                Ok(Type::from_str(target_type))
            }

            // ─── UNIFY TYPES ─── give Range and FieldAccess proper inferred types.
            Expr::Range { start, end, .. } => {
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
            Expr::FieldAccess { object, field, .. } => {
                // No struct system yet — analyze the object, then report.
                let _obj_type = self.analyze_expr(object)?;
                Err(CompileError::simple(
                    &format!("Field access '.{}' is not supported yet", field),
                    0, 0, "", ErrorCode::E0002,
                ).with_suggestion(
                    "Field access requires struct support, which is not yet implemented",
                ))
            }
            Expr::MethodCall { receiver, method_name, args, .. } => {
                // Treat like a dotted function call for now.
                let recv_type = self.analyze_expr(receiver)?;
                if let Some(method) = self.resolve_trait_method(&recv_type, method_name) {
                    for (arg, (_, param_type)) in args.iter().zip(&method.params) {
                        let arg_type = self.analyze_expr(arg)?;
                        let expected_type = match param_type {
                            Some(s) => s.to_type(),
                            None => Type::Unknown,
                        };
                        if !arg_type.can_coerce_to(&expected_type)
                            && expected_type != Type::Unknown
                        {
                            return Err(CompileError::simple(
                                &format!(
                                    "Method '{}' argument type mismatch: expected {}, found {}",
                                    method_name, expected_type, arg_type
                                ),
                                0, 0, "", ErrorCode::E0002,
                            ));
                        }
                    }
                    Ok(method
                        .return_type
                        .as_ref()
                        .map(|t| t.to_type())
                        .unwrap_or(Type::Void))
                } else {
                    Err(CompileError::simple(
                        &format!("Type {} has no method '{}'", recv_type, method_name),
                        0, 0, "", ErrorCode::E0004,
                    ))
                }
            }

            // ─── UNIFY TYPES ─── StructLiteral is currently unsupported.
            Expr::StructLiteral { type_name, .. } => Err(CompileError::simple(
                &format!("Struct literal '{}' is not supported yet", type_name),
                0, 0, "", ErrorCode::E0002,
            )),

            // ─── UNIFY TYPES ─── TypeAssert is currently unsupported.
            Expr::TypeAssert { type_name, .. } => Err(CompileError::simple(
                &format!("Type assertion '{}' is not supported yet", type_name),
                0, 0, "", ErrorCode::E0002,
            )),
        }
    }
    pub(super) fn substitute_type_vars(&self, type_: &Type, bindings: &HashMap<String, Type>) -> Type {
        match type_ {
            Type::TypeVar(name) => bindings.get(name).cloned().unwrap_or_else(|| type_.clone()),
            Type::List(inner) => Type::list(self.substitute_type_vars(inner, bindings)),
            Type::Array(inner, size) => {
                Type::array(self.substitute_type_vars(inner, bindings), *size)
            }
            Type::Tuple(elements) => Type::tuple(
                elements.iter().map(|e| self.substitute_type_vars(e, bindings)).collect(),
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
                if let Type::Option(_) = value_type { Ok(()) } else {
                    Err(CompileError::simple(
                        &format!("Cannot match None against {}", value_type),
                        0, 0, "", ErrorCode::E0002,
                    ))
                }
            }
            Pattern::Some(_) | Pattern::SomeNested(_) => {
                if let Type::Option(_) = value_type { Ok(()) } else {
                    Err(CompileError::simple(
                        &format!("Cannot match Some against {}", value_type),
                        0, 0, "", ErrorCode::E0002,
                    ))
                }
            }
            Pattern::Ok(_) | Pattern::OkNested(_) => {
                if let Type::Result { .. } = value_type { Ok(()) } else {
                    Err(CompileError::simple(
                        &format!("Cannot match Ok against {}", value_type),
                        0, 0, "", ErrorCode::E0002,
                    ))
                }
            }
            Pattern::Error(_) | Pattern::ErrorNested(_) => {
                if let Type::Result { .. } = value_type { Ok(()) } else {
                    Err(CompileError::simple(
                        &format!("Cannot match Error against {}", value_type),
                        0, 0, "", ErrorCode::E0002,
                    ))
                }
            }
            Pattern::Literal(lit) => {
                let lit_type = match lit {
                    crate::frontend::ast::Expr::Int(_) => Type::Int,
                    crate::frontend::ast::Expr::Number(_) => Type::Float,
                    crate::frontend::ast::Expr::String(_) => Type::String,
                    crate::frontend::ast::Expr::Bool(_) => Type::Bool,
                    _ => Type::Unknown,
                };
                if lit_type.can_coerce_to(value_type) { Ok(()) } else {
                    Err(CompileError::simple(
                        &format!(
                            "Cannot match literal of type {} against {}",
                            lit_type, value_type
                        ),
                        0, 0, "", ErrorCode::E0002,
                    ))
                }
            }
            _ => Ok(()),
        }
    }
}