// src/semantics/semantic_builder/expr.rs

use super::*;

impl SemanticIRBuilder {
    pub(super) fn translate_simple_stmt(
        &mut self,
        program: &mut SemanticProgram,
        func: &mut SemanticFunction,
        current_block: usize,
        stmt: &Stmt,
    ) -> FlowResult {
        if let Stmt::Expression(Expr::If {
            condition,
            then_branch,
            else_branch,
        }) = stmt
        {
            // Borrow, don't clone — the type table is keyed by node
            // address and clone() invalidates the keys.
            let then_stmts: &[Stmt] = match then_branch.as_ref() {
                Expr::Block { statements, .. } => statements.as_slice(),
                _ => &[],
            };
            let else_stmts: Option<&[Stmt]> = else_branch.as_ref().map(|e| {
                match e.as_ref() {
                    Expr::Block { statements, .. } => statements.as_slice(),
                    _ => &[],
                }
            });
            return self.translate_if(
                program,
                func,
                current_block,
                condition,
                then_stmts,
                else_stmts,
            );
        }

        let instruction = match stmt {
            Stmt::VarDecl {
                name,
                value,
                mutable,
                type_annotation,
                ..
            } => {
                if matches!(value, Expr::For { .. } | Expr::While { .. }) {
                    let decl_type = if let Some(t) = type_annotation {
                        Type::from_str(t)
                    } else {
                        Type::Void
                    };
                    self.declare_var(name, decl_type.clone(), *mutable);
                    let _ = self.safe_push_instruction(
                        func,
                        current_block,
                        SemanticInstruction::Declare {
                            name: name.clone(),
                            mutable: *mutable,
                            type_: decl_type,
                            value: TypedIRValue::None {
                                option_type: Type::option(Type::Void),
                            },
                        },
                    );
                    let _ = match value {
                        Expr::For {
                            var,
                            iterable,
                            body,
                            trailing_expr,
                            ..
                        } => self.translate_for_expr_with_target(
                            program,
                            func,
                            current_block,
                            var,
                            iterable,
                            body,
                            trailing_expr,
                            name,
                        ),
                        Expr::While {
                            condition,
                            body,
                            trailing_expr,
                            ..
                        } => self.translate_while_expr_with_target(
                            program,
                            func,
                            current_block,
                            condition,
                            body,
                            trailing_expr,
                            name,
                        ),
                        other => {
                            self.diagnostics
                                .push(format!("Unexpected for/while pattern: {:?}", other));
                            return FlowResult::Unreachable;
                        }
                    };
                    if let Some(merge) = self.pending_merge.take() {
                        return FlowResult::Reachable(merge);
                    }
                    return FlowResult::Reachable(current_block);
                }

                if let Expr::List(elements) = value {
                    self.list_values.insert(name.clone(), elements.clone());
                }
                let typed_value = self.translate_expr(program, func, current_block, value);

                if let Expr::FunctionCall {
                    name: func_name,
                    args,
                    ..
                } = value
                {
                    let typed_args: Vec<TypedIRValue> = args
                        .iter()
                        .map(|a| self.translate_expr(program, func, current_block, a))
                        .collect();
                    let _ = self.safe_push_instruction(
                        func,
                        current_block,
                        Instruction::Call {
                            func: func_name.clone(),
                            args: typed_args,
                            result: Some(name.clone()),
                        },
                    );
                }

                let value_type = typed_value.type_of();
                let type_ = if let Some(type_str) = type_annotation {
                    let declared_type = Type::from_str(type_str);
                    if value_type != Type::Unknown
                        && declared_type != Type::Unknown
                        && !value_type.can_coerce_to(&declared_type)
                    {
                        self.diagnostics.push(format!(
                            "Variable '{}' declared as {:?}, but initializer has type {:?}",
                            name, declared_type, value_type
                        ));
                    }
                    declared_type
                } else {
                    value_type
                };

                self.declare_var(name, type_.clone(), *mutable);

                // If the initializer's translation branched (because it
                // contained an `if` / `match` / `try` expression), the
                // `Declare` instruction must go into the *merge* block
                // that the value-producing expression created. Pushing
                // it into `current_block` would land it after the
                // Branch terminator, effectively deleting the rest of
                // the enclosing block.
                if let Some(merge) = self.pending_merge.take() {
                    let _ = self.safe_push_instruction(
                        func,
                        merge,
                        SemanticInstruction::Declare {
                            name: name.clone(),
                            mutable: *mutable,
                            type_,
                            value: typed_value,
                        },
                    );
                    return FlowResult::Reachable(merge);
                }

                SemanticInstruction::Declare {
                    name: name.clone(),
                    mutable: *mutable,
                    type_,
                    value: typed_value,
                }
            }
            Stmt::Assign { name, value } => {
                let var_info = match self.lookup_var(name) {
                    Some(info) => info.clone(),
                    None => {
                        self.diagnostics
                            .push(format!("Assignment to undeclared variable '{}'", name));
                        VariableInfo {
                            type_: Type::Unknown,
                            mutable: true,
                            capture_mode: None,
                        }
                    }
                };

                if !var_info.mutable {
                    self.diagnostics
                        .push(format!("Cannot assign to immutable variable '{}'", name));
                }

                let expected_type = var_info.type_;
                let typed_value = self.translate_expr(program, func, current_block, value);
                let actual_type = typed_value.type_of();

                if expected_type != Type::Unknown
                    && actual_type != Type::Unknown
                    && !actual_type.can_coerce_to(&expected_type)
                {
                    self.diagnostics.push(format!(
                        "Assignment type mismatch for '{}': expected {:?}, found {:?}",
                        name, expected_type, actual_type
                    ));
                }

                SemanticInstruction::Assign {
                    target: name.clone(),
                    value: typed_value,
                }
            }
            Stmt::Print { expr } => {
                let typed_value = self.translate_expr(program, func, current_block, expr);
                SemanticInstruction::Print { value: typed_value }
            }
            Stmt::Return { value } => {
                let typed_value = value
                    .as_ref()
                    .map(|v| self.translate_expr(program, func, current_block, v));
                let coerced_value = typed_value.map(|v| self.coerce_value(v, &func.return_type));
                let type_ = coerced_value
                    .as_ref()
                    .map(|v| v.type_of())
                    .unwrap_or(Type::Void);
                if type_ != Type::Unknown
                    && func.return_type != Type::Unknown
                    && !type_.can_coerce_to(&func.return_type)
                {
                    self.diagnostics.push(format!(
                        "Return type mismatch in function '{}': expected {:?}, found {:?}",
                        func.name, func.return_type, type_
                    ));
                }
                // If any defers are pending in the enclosing scope,
                // chain them LIFO before the actual return. Each cleanup
                // block runs its body and jumps to the next; the last
                // one emits the real Return with the captured value.
                if let Some(defer_ctx) = self.defer_stack.last() {
                    if !defer_ctx.cleanup_blocks.is_empty() {
                        let cleanups: Vec<usize> =
                            defer_ctx.cleanup_blocks.iter().rev().copied().collect();

                        // Current block jumps to the first cleanup.
                        let _ = self.safe_set_terminator(
                            func,
                            current_block,
                            Terminator::Jump { block: cleanups[0] },
                        );

                        // Each cleanup jumps to the next.
                        for i in 0..cleanups.len() - 1 {
                            let _ = self.safe_set_terminator(
                                func,
                                cleanups[i],
                                Terminator::Jump { block: cleanups[i + 1] },
                            );
                        }

                        // Last cleanup emits the actual return.
                        let _ = self.safe_set_terminator(
                            func,
                            *cleanups.last().unwrap(),
                            Terminator::Return {
                                value: coerced_value,
                                type_,
                            },
                        );

                        return FlowResult::Unreachable;
                    }
                }

                // No pending defers: normal return.
                let _ = self.safe_set_terminator(
                    func,
                    current_block,
                    Terminator::Return {
                        value: coerced_value,
                        type_,
                    },
                );
                return FlowResult::Unreachable;
            }
            Stmt::ArrayAssign {
                array,
                index,
                value,
            } => {
                let arr_expr = Expr::Var(array.clone(), Span::default());
                let arr_val = self.translate_expr(program, func, current_block, &arr_expr);
                let idx_val = self.translate_expr(program, func, current_block, index);
                let val = self.translate_expr(program, func, current_block, value);
                SemanticInstruction::ArrayAssign {
                    array: Box::new(arr_val),
                    index: Box::new(idx_val),
                    value: val,
                }
            }
            Stmt::ChannelDecl { name } => {
                let chan_type = Type::Channel(Box::new(Type::Unknown));
                self.declare_var(name, chan_type.clone(), true);
                SemanticInstruction::ChannelDecl {
                    name: name.clone(),
                    type_: chan_type,
                }
            }
            Stmt::Send { channel, value } => {
                let typed_value = self.translate_expr(program, func, current_block, value);
                SemanticInstruction::Send {
                    channel: channel.clone(),
                    value: typed_value,
                }
            }
            Stmt::Receive { channel, target } => SemanticInstruction::Receive {
                channel: channel.clone(),
                target: target.clone(),
            },
            Stmt::UnsafeBlock { body } => {
                for s in body {
                    let _ = self.translate_simple_stmt(program, func, current_block, s);
                }
                SemanticInstruction::Nop
            }
            Stmt::Import { .. } => SemanticInstruction::Nop,
            Stmt::Expression(expr) => {
                let _typed_value = self.translate_expr(program, func, current_block, expr);
                if let Some(merge) = self.pending_merge.take() {
                    return FlowResult::Reachable(merge);
                }
                SemanticInstruction::Nop
            }
            _ => {
                self.diagnostics
                    .push("Control flow statement not intercepted".to_string());
                return FlowResult::Unreachable;
            }
        };

        let _ = self.safe_push_instruction(func, current_block, instruction);

        match &stmt {
            Stmt::Return { .. } => FlowResult::Unreachable,
            _ => FlowResult::Reachable(current_block),
        }
    } 
    pub(super) fn translate_expr(
        &mut self,
        program: &mut SemanticProgram,
        func: &mut SemanticFunction,
        current_block: usize,
        expr: &Expr,
    ) -> TypedIRValue {
        match expr {
            Expr::Unary { op, expr, .. } => {
                let inner = self.translate_expr(program, func, current_block, expr);
                let inner_type = inner.type_of();
                match op {
                    crate::frontend::ast::UnaryOp::Negate => {
                        let zero = match &inner_type {
                            Type::Float => TypedIRValue::Float(0.0),
                            _ => TypedIRValue::Int(0),
                        };
                        TypedIRValue::BinaryOp {
                            op: SemanticBinOp::Subtract,
                            left: Box::new(zero),
                            right: Box::new(inner),
                            result_type: inner_type,
                        }
                    }
                    crate::frontend::ast::UnaryOp::Not => TypedIRValue::BinaryOp {
                        op: SemanticBinOp::Equal,
                        left: Box::new(inner),
                        right: Box::new(TypedIRValue::Bool(false)),
                        result_type: Type::Bool,
                    },
                }
            }
            Expr::Borrow { expr } => {
                let inner = self.translate_expr(program, func, current_block, expr);
                let inner_type = inner.type_of();
                TypedIRValue::Borrow {
                    expr: Box::new(inner),
                    target_type: Type::borrow(inner_type),
                }
            }
            Expr::MutBorrow { expr } => {
                let inner = self.translate_expr(program, func, current_block, expr);
                let inner_type = inner.type_of();
                TypedIRValue::MutBorrow {
                    expr: Box::new(inner),
                    target_type: Type::mut_borrow(inner_type),
                }
            }
            Expr::Deref { expr } => {
                let inner = self.translate_expr(program, func, current_block, expr);
                let inner_type = inner.type_of();
                let target_type = match inner_type {
                    Type::Borrow(t) | Type::MutBorrow(t) | Type::Pointer(t) => *t,
                    Type::Unknown => Type::Unknown,
                    other => {
                        self.diagnostics.push(format!(
                            "Cannot dereference non-pointer value of type {:?}",
                            other
                        ));
                        Type::Unknown
                    }
                };
                TypedIRValue::Deref {
                    expr: Box::new(inner),
                    target_type,
                }
            }
            Expr::AddrOf { expr } => {
                let inner = self.translate_expr(program, func, current_block, expr);
                let inner_type = inner.type_of();
                TypedIRValue::AddrOf {
                    expr: Box::new(inner),
                    target_type: Type::pointer(inner_type),
                }
            }
            Expr::Number(n) => TypedIRValue::Float(*n),
            Expr::Int(i) => TypedIRValue::Int(*i),
            Expr::String(s) => TypedIRValue::String(s.clone()),
            Expr::Bool(b) => TypedIRValue::Bool(*b),
            Expr::Var(name, span) => {
                // ─── UNIFY TYPES ─── prefer analyzer type, fall back to local scope.
                let ty = self.type_of_expr(expr)
                    .cloned()
                    .or_else(|| self.lookup_var(name).map(|i| i.type_.clone()))
                    .unwrap_or(Type::Unknown);

                if ty == Type::Unknown && self.lookup_var(name).is_none() {
                    self.diagnostics.push(format!(
                        "Use of undeclared variable '{}' at {}:{}",
                        name, span.start_line, span.start_column
                    ));
                }
                TypedIRValue::Variable(name.clone(), ty)
            }
            Expr::List(elements) => {
                let mut values: Vec<TypedIRValue> = elements
                    .iter()
                    .map(|e| self.translate_expr(program, func, current_block, e))
                    .collect();
                let has_float = values.iter().any(|v| v.type_of() == Type::Float);
                let has_int = values.iter().any(|v| v.type_of() == Type::Int);
                if has_float && has_int {
                    for val in &mut values {
                        if val.type_of() == Type::Int {
                            *val = TypedIRValue::Cast {
                                value: Box::new(val.clone()),
                                target_type: Type::Float,
                            };
                        }
                    }
                }
                // ─── UNIFY TYPES ─── analyzer knows the list's element type.
                let elem_type = match self.type_of_expr(expr) {
                    Some(Type::List(inner)) => (**inner).clone(),
                    _ => values.first().map(|v| v.type_of()).unwrap_or(Type::Unknown),
                };
                TypedIRValue::List(values, elem_type)
            }
            Expr::Binary { left, op, right } => match op {
                BinOp::And | BinOp::Or => {
                    self.translate_short_circuit(
                        program, func, current_block, left, right, op,
                    )
                }
                _ => {
                    let l = self.translate_expr(program, func, current_block, left);
                    let r = self.translate_expr(program, func, current_block, right);

                    // ─── UNIFY TYPES ─── Type comes from the analyzer.
                    let result_type = self.type_of_expr(expr)
                        .cloned()
                        .unwrap_or(Type::Unknown);

                    // Keep IR self-consistent by inserting Int→Float coercions.
                    let (cast_l, cast_r) = match (l.type_of(), r.type_of()) {
                        (Type::Int, Type::Float) => (
                            TypedIRValue::Cast { value: Box::new(l), target_type: Type::Float },
                            r,
                        ),
                        (Type::Float, Type::Int) => (
                            l,
                            TypedIRValue::Cast { value: Box::new(r), target_type: Type::Float },
                        ),
                        _ => (l, r),
                    };

                    let semantic_op = match op {
                        BinOp::Add => SemanticBinOp::Add,
                        BinOp::Subtract => SemanticBinOp::Subtract,
                        BinOp::Multiply => SemanticBinOp::Multiply,
                        BinOp::Divide => SemanticBinOp::Divide,
                        BinOp::Greater => SemanticBinOp::Greater,
                        BinOp::Less => SemanticBinOp::Less,
                        BinOp::GreaterEqual => SemanticBinOp::GreaterEqual,
                        BinOp::LessEqual => SemanticBinOp::LessEqual,
                        BinOp::Equal => SemanticBinOp::Equal,
                        BinOp::NotEqual => SemanticBinOp::NotEqual,
                        BinOp::And | BinOp::Or => unreachable!(
                            "And/Or handled by translate_short_circuit arm above"
                        ),
                    };
                    TypedIRValue::BinaryOp {
                        op: semantic_op,
                        left: Box::new(cast_l),
                        right: Box::new(cast_r),
                        result_type,
                    }
                }
            },
            Expr::FunctionCall { name, args, .. } => {
                let clean_name = name.trim_end_matches("()");

                // ─── METHOD CALL DISAMBIGUATION ───
                // A dotted name is a *method call* only if:
                //   1) the full dotted name is NOT a registered function (Math.sqrt,
                //      String.concat, List.sum, File.read, user functions with dots…), AND
                //   2) the first component names a variable in scope.
                // Otherwise it's a regular function call — this is how all built-ins
                // (Math.*, String.*, List.*, File.*) are dispatched.
                let is_known_function = self.function_types.contains_key(clean_name);

                if !is_known_function && clean_name.contains('.') {
                    let parts: Vec<&str> = clean_name.split('.').collect();
                    if parts.len() == 2 {
                        let receiver_name = parts[0];
                        let method_name = parts[1];

                        if let Some(info) = self.lookup_var(receiver_name) {
                            let receiver_type = info.type_.clone();

                            if let Some(resolved_name) =
                                self.resolve_method_call(&receiver_type, method_name)
                            {
                                // Receiver is passed as the first argument — same
                                // convention expand_impl_methods uses for `self`.
                                let receiver_value = TypedIRValue::Variable(
                                    receiver_name.to_string(),
                                    receiver_type.clone(),
                                );

                                let mut call_args = vec![receiver_value];
                                for arg in args {
                                    call_args.push(
                                        self.translate_expr(program, func, current_block, arg),
                                    );
                                }

                                // ─── UNIFY TYPES ─── analyzer already inferred the return type.
                                let return_type = self
                                    .type_of_expr(expr)
                                    .cloned()
                                    .unwrap_or(Type::Unknown);

                                return TypedIRValue::Call {
                                    function: resolved_name,
                                    args: call_args,
                                    return_type,
                                };
                            } else {
                                self.diagnostics.push(format!(
                                    "Type {} has no method '{}'",
                                    receiver_type, method_name
                                ));
                                return TypedIRValue::Void;
                            }
                        }
                    }
                }

                // ─── Regular function call (built-ins + user functions) ───
                let typed_args: Vec<TypedIRValue> = args
                    .iter()
                    .map(|a| self.translate_expr(program, func, current_block, a))
                    .collect();

                // ─── UNIFY TYPES ─── return type comes from the analyzer.
                let return_type = self
                    .type_of_expr(expr)
                    .cloned()
                    .unwrap_or(Type::Unknown);

                let coerced_args = if let Some(sig) = self.function_types.get(clean_name).cloned() {
                    typed_args
                        .into_iter()
                        .zip(sig.params.iter())
                        .map(|(a, (_, t))| self.coerce_value(a, t))
                        .collect()
                } else {
                    typed_args
                };

                TypedIRValue::Call {
                    function: clean_name.to_string(),
                    args: coerced_args,
                    return_type,
                }
            }
            Expr::ArrayAccess { array, index } => {
                let array_value = self.translate_expr(program, func, current_block, array);
                let index_value = self.translate_expr(program, func, current_block, index);
                let element_type = match array_value.type_of() {
                    Type::List(elem) => *elem,
                    _ => Type::Unknown,
                };
                TypedIRValue::ArrayAccess {
                    array: Box::new(array_value),
                    index: Box::new(index_value),
                    element_type,
                }
            }
            Expr::Some { value } => {
                let inner = self.translate_expr(program, func, current_block, value);
                TypedIRValue::Some(Box::new(inner))
            }
            Expr::None => {
                // ─── UNIFY TYPES ─── read the outer Option type from the table.
                let option_type = self
                    .type_of_expr(expr)
                    .cloned()
                    .unwrap_or(Type::option(Type::Unknown));
                TypedIRValue::None { option_type }
            }
            Expr::Ok { value } => {
                let inner = self.translate_expr(program, func, current_block, value);
                let result_type = self
                    .type_of_expr(expr)
                    .cloned()
                    .unwrap_or(Type::result(Type::Unknown, Type::Unknown));
                TypedIRValue::Ok {
                    value: Box::new(inner),
                    result_type,
                }
            }
            Expr::Error { value } => {
                let inner = self.translate_expr(program, func, current_block, value);
                let result_type = self
                    .type_of_expr(expr)
                    .cloned()
                    .unwrap_or(Type::result(Type::Unknown, Type::Unknown));
                TypedIRValue::Error {
                    value: Box::new(inner),
                    result_type,
                }
            }
            Expr::Block {
                statements,
                trailing_expr,
            } => {
                let mut flow = FlowResult::Reachable(current_block);

                for s in statements {
                    let current = match flow {
                        FlowResult::Reachable(id) => id,
                        FlowResult::Unreachable => break,
                    };

                    // Don't append instructions to a block that already has a terminator
                    // (e.g. after a `return`, `break`, or an exhaustive if/match).
                    if self.block_is_terminated(func, current) {
                        flow = FlowResult::Unreachable;
                        break;
                    }

                    match s {
                        Stmt::Break => {
                            if let Some(loop_ctx) = self.loop_stack.last().copied() {
                                let _ = self.safe_set_terminator(
                                    func,
                                    current,
                                    Terminator::Jump {
                                        block: loop_ctx.break_block,
                                    },
                                );
                            } else {
                                self.diagnostics.push("Break outside of loop".to_string());
                            }
                            flow = FlowResult::Unreachable;
                        }
                        Stmt::Continue => {
                            if let Some(loop_ctx) = self.loop_stack.last().copied() {
                                let _ = self.safe_set_terminator(
                                    func,
                                    current,
                                    Terminator::Jump {
                                        block: loop_ctx.continue_block,
                                    },
                                );
                            } else {
                                self.diagnostics.push("Continue outside of loop".to_string());
                            }
                            flow = FlowResult::Unreachable;
                        }
                        _ => {
                            flow = self.translate_simple_stmt(program, func, current, s);
                        }
                    }
                }

                // The trailing expression's value is the block's value.
                // If the block already became unreachable, there's no value.
                if let Some(expr) = trailing_expr {
                    if let FlowResult::Reachable(id) = flow {
                        self.translate_expr(program, func, id, expr)
                    } else {
                        TypedIRValue::Void
                    }
                } else {
                    TypedIRValue::Void
                }
            }
            Expr::If { condition, then_branch, else_branch } => {
                 // ─── UNIFY TYPES ───
                let result_type = self.type_of_expr(expr)
                    .cloned()
                    .unwrap_or(Type::Unknown);

                let result_var = self.allocate_result_var(func, current_block, result_type.clone());

                let then_flow = self.translate_if_with_target(
                    program, func, current_block, condition, then_branch, else_branch.as_deref(),
                    &result_var, result_type.clone(),
                );

                if let FlowResult::Reachable(merge_id) = then_flow {
                    // Return the variable that holds the result
                    TypedIRValue::Variable(result_var, result_type)
                } else {
                    self.diagnostics.push("If expression has no reachable branch".to_string());
                    TypedIRValue::Void
                }
            }
            Expr::Match { value, cases } => {
                // Translate the value being matched
                let match_value = self.translate_expr(program, func, current_block, value);

                // ─── UNIFY TYPES ───
                let result_type = self.type_of_expr(expr)
                    .cloned()
                    .unwrap_or(Type::Unknown);

                // Allocate a result variable in the current scope
                let result_var = self.allocate_result_var(func, current_block, result_type.clone());

                // Create a merge block for all arms
                let merge_id = program.new_block_id();
                func.blocks.push(SemanticBlock {
                    id: merge_id,
                    instructions: Vec::new(),
                    terminator: None,
                });

                // Prepare switch terminator: map patterns to block ids
                let mut switch_cases = Vec::new();
                let mut case_block_ids = Vec::new();

                // First, create block for each case and translate it
                for case in cases {
                    let case_block_id = program.new_block_id();
                    func.blocks.push(SemanticBlock {
                        id: case_block_id,
                        instructions: Vec::new(),
                        terminator: None,
                    });

                    // Convert pattern to SemanticPattern (reuse existing logic from `translate_match`)
                    let sem_pattern = match &case.pattern {
                        crate::frontend::ast::Pattern::Some(v) => SemanticPattern::Some { binding: v.clone() },
                        crate::frontend::ast::Pattern::None => SemanticPattern::None,
                        crate::frontend::ast::Pattern::Ok(v) => SemanticPattern::Ok { binding: v.clone() },
                        crate::frontend::ast::Pattern::Error(v) => SemanticPattern::Error { binding: v.clone() },
                        crate::frontend::ast::Pattern::Wildcard => SemanticPattern::Wildcard,
                        crate::frontend::ast::Pattern::Binding(_) => SemanticPattern::Wildcard,
                        crate::frontend::ast::Pattern::Literal(e) => {
                            SemanticPattern::Literal(self.translate_expr(program, func, current_block, e))
                        }
                        _ => SemanticPattern::Wildcard,
                    };

                    switch_cases.push((sem_pattern, case_block_id));
                    case_block_ids.push(case_block_id);
                }

                // Add a default block (for unmatched patterns) - for now, jump to merge
                let default_block_id = program.new_block_id();
                func.blocks.push(SemanticBlock {
                    id: default_block_id,
                    instructions: Vec::new(),
                    terminator: None,
                });
                let _ = self.safe_set_terminator(
                    func,
                    default_block_id,
                    Terminator::Jump { block: merge_id },
                );

                // Set the switch terminator on the current block
                let _ = self.safe_set_terminator(
                    func,
                    current_block,
                    Terminator::Switch {
                        value: match_value,
                        cases: switch_cases,
                        default_block: Some(default_block_id),
                    },
                );

                // Translate each case body and assign result to result_var
                for (idx, case) in cases.iter().enumerate() {
                    let case_block_id = case_block_ids[idx];

                    self.push_scope();

                    // Bind pattern variables (simplified; we only handle Some, Ok, Error bindings)
                    if let Some(binding) = match &case.pattern {
                        crate::frontend::ast::Pattern::Some(v) => Some(v.clone()),
                        crate::frontend::ast::Pattern::Ok(v) => Some(v.clone()),
                        crate::frontend::ast::Pattern::Error(v) => Some(v.clone()),
                        _ => None,
                    } {
                        self.declare_var(&binding, Type::Unknown, false);
                    }

                    // Extract body statements and trailing expr from case.body (which is Expr::Block)
                    let (body_stmts, body_trailing) = match &case.body {
                        Expr::Block { statements, trailing_expr } => (statements.clone(), trailing_expr.as_deref()),
                        other => (vec![Stmt::Expression(other.clone())], None),
                    };

                    // Translate the body with result target
                    if let Some(final_block) = self.translate_block_with_result(
                        program,
                        func,
                        case_block_id,
                        &body_stmts,
                        body_trailing,
                        &result_var,
                        result_type.clone(),
                    ) {
                        // Jump to merge if not already terminated
                        self.block_is_terminated(func, final_block);
                        let _ = self.safe_set_terminator(
                            func,
                            final_block,
                            Terminator::Jump { block: merge_id },
                        );   
                    }

                    self.pop_scope();
                }

                // Set pending merge so that subsequent statements continue at merge block
                self.pending_merge = Some(merge_id);

                // Return the result variable as the value of the match expression
                TypedIRValue::Variable(result_var, result_type)
            }
            Expr::TryCatch {
                try_branch,
                catch_var,
                catch_branch,
                finally_body,
            } => {
                // ─── Result-based try/catch ───
                //
                // Semantics: the try body must evaluate to Result<T, E>.
                // The whole expression has type T:
                //   Ok(v)  →  result_var := v
                //   Error(e) →  bind catch_var to e; result_var := catch body's value
                //
                // CFG:
                //   current → evaluate try body into __try_value
                //           → Switch on __try_value
                //               Ok(__ok_payload)  → ok_block
                //               Error(catch_var) → err_block
                //   ok_block: result_var := __ok_payload; Jump merge
                //   err_block: translate catch body; result_var := its value; Jump merge
                //   merge: result_var holds the value

                let result_type = self
                    .type_of_expr(expr)
                    .cloned()
                    .unwrap_or(Type::Unknown);

                let result_var =
                    self.allocate_result_var(func, current_block, result_type.clone());

                // Fresh names for the try-body's value and the Ok payload.
                let try_value_name = format!("__try_value_{}", self.iter_counter);
                self.iter_counter += 1;
                let ok_payload_name = format!("__ok_payload_{}", self.iter_counter);
                self.iter_counter += 1;

                // Evaluate the try body, storing its Result value in try_value.
                let (try_stmts, try_trailing): (Vec<Stmt>, Option<Box<Expr>>) =
                    match try_branch.as_ref() {
                        Expr::Block { statements, trailing_expr } => {
                            (statements.clone(), trailing_expr.clone())
                        }
                        other => (vec![Stmt::Expression((*other).clone())], None),
                    };

                // Allocate the try_value variable before branching.
                let _ = self.safe_push_instruction(
                    func,
                    current_block,
                    SemanticInstruction::Declare {
                        name: try_value_name.clone(),
                        mutable: true,
                        type_: Type::result(Type::Unknown, Type::Unknown),
                        value: TypedIRValue::Void,
                    },
                );

                let try_flow = self.translate_block_with_result(
                    program,
                    func,
                    current_block,
                    &try_stmts,
                    try_trailing.as_deref(),
                    &try_value_name,
                    Type::result(Type::Unknown, Type::Unknown),
                );

                let after_try = match try_flow {
                    Some(id) => id,
                    None => {
                        // Try body always diverges — no merge needed.
                        return TypedIRValue::Void;
                    }
                };

                // Create the two branches and the merge.
                let ok_block_id = program.new_block_id();
                let err_block_id = program.new_block_id();
                let merge_id = program.new_block_id();
                func.blocks.push(SemanticBlock {
                    id: ok_block_id,
                    instructions: Vec::new(),
                    terminator: None,
                });
                func.blocks.push(SemanticBlock {
                    id: err_block_id,
                    instructions: Vec::new(),
                    terminator: None,
                });
                func.blocks.push(SemanticBlock {
                    id: merge_id,
                    instructions: Vec::new(),
                    terminator: None,
                });

                // Switch on the Result value's tag.
                let catch_binding = catch_var
                    .clone()
                    .unwrap_or_else(|| format!("__err_unused_{}", self.iter_counter));
                self.iter_counter += 1;

                let switch_value =
                    TypedIRValue::Variable(try_value_name.clone(), Type::result(Type::Unknown, Type::Unknown));

                let _ = self.safe_set_terminator(
                    func,
                    after_try,
                    Terminator::Switch {
                        value: switch_value,
                        cases: vec![
                            (
                                SemanticPattern::Ok { binding: ok_payload_name.clone() },
                                ok_block_id,
                            ),
                            (
                                SemanticPattern::Error { binding: catch_binding.clone() },
                                err_block_id,
                            ),
                        ],
                        default_block: Some(err_block_id),
                    },
                );

                // ─── Ok block: assign payload to result_var ───
                self.declare_var(&ok_payload_name, result_type.clone(), false);
                let _ = self.safe_push_instruction(
                    func,
                    ok_block_id,
                    SemanticInstruction::Assign {
                        target: result_var.clone(),
                        value: TypedIRValue::Variable(ok_payload_name.clone(), result_type.clone()),
                    },
                );
                let _ = self.safe_set_terminator(
                    func,
                    ok_block_id,
                    Terminator::Jump { block: merge_id },
                );

                // ─── Err block: translate catch body ───
                // The switch's Error pattern binds catch_var at runtime;
                // declare it in the IR builder's scope so the catch body
                // can reference it.
                if catch_var.is_some() {
                    self.declare_var(&catch_binding, Type::Unknown, false);
                }

                self.push_scope();
                let (catch_stmts, catch_trailing): (Vec<Stmt>, Option<Box<Expr>>) =
                    match catch_branch.as_ref() {
                        Expr::Block { statements, trailing_expr } => {
                            (statements.clone(), trailing_expr.clone())
                        }
                        other => (vec![Stmt::Expression((*other).clone())], None),
                    };

                let catch_flow = self.translate_block_with_result(
                    program,
                    func,
                    err_block_id,
                    &catch_stmts,
                    catch_trailing.as_deref(),
                    &result_var,
                    result_type.clone(),
                );
                self.pop_scope();

                if let Some(final_block) = catch_flow {
                    if !self.block_is_terminated(func, final_block) {
                        let _ = self.safe_set_terminator(
                            func,
                            final_block,
                            Terminator::Jump { block: merge_id },
                        );
                    }
                } else {
                    // Catch body always diverges — merge is only reached
                    // from the Ok path, but we still want it to exist as
                    // a target. Add an unreachable jump so verification
                    // doesn't complain.
                    let _ = self.safe_set_terminator(
                        func,
                        err_block_id,
                        Terminator::Jump { block: merge_id },
                    );
                }

                // ─── Finally: runs after both branches at the merge ───
                let final_reachable = if let Some(finally_stmts) = finally_body {
                    self.push_scope();
                    let finally_flow =
                        self.translate_block(program, func, merge_id, finally_stmts);
                    self.pop_scope();
                    match finally_flow {
                        FlowResult::Reachable(id) => id,
                        FlowResult::Unreachable => merge_id,
                    }
                } else {
                    merge_id
                };

                self.pending_merge = Some(final_reachable);
                TypedIRValue::Variable(result_var, result_type)
            }
            Expr::For {
                var,
                iterable,
                body,
                trailing_expr,
                ..
            } => self.translate_for_expr(
                program,
                func,
                current_block,
                var,
                iterable,
                body,
                trailing_expr,
            ),
            Expr::While {
                condition,
                body,
                trailing_expr,
                ..
            } => self.translate_while_expr(
                program,
                func,
                current_block,
                condition,
                body,
                trailing_expr,
            ),
            Expr::PtrLiteral(val) => TypedIRValue::PtrLiteral(*val),
            Expr::NullPtr => TypedIRValue::NullPtr,
            Expr::Cast {
                expr: cast_expr,
                target_type,
            } => {
                let inner = self.translate_expr(program, func, current_block, cast_expr);
                let target = Type::from_str(target_type);
                TypedIRValue::Cast {
                    value: Box::new(inner),
                    target_type: target,
                }
            }
            Expr::Range { start, end, inclusive: _ } => {
                // For now, represent a range as a list containing the start and end values.
                // This is not a full range implementation, but avoids silent Void.
                let start_val = start.as_ref()
                    .map(|e| self.translate_expr(program, func, current_block, e))
                    .unwrap_or(TypedIRValue::Void);
                let end_val = end.as_ref()
                    .map(|e| self.translate_expr(program, func, current_block, e))
                    .unwrap_or(TypedIRValue::Void);
                let elem_type = start_val.type_of().common_supertype(&end_val.type_of());
                TypedIRValue::List(vec![start_val, end_val], elem_type)
            }
            Expr::FieldAccess { object, field, .. } => {
                // Field access is not yet supported; emit an error and return Void.
                self.diagnostics.push(format!(
                    "Field access '{}.{}' is not supported yet",
                    match object.as_ref() {
                        Expr::Var(name, _) => name.clone(),
                        _ => "<expr>".to_string(),
                    },
                    field
                ));
                TypedIRValue::Void
            }
            _ => TypedIRValue::Void,
        }
    }
    ///```text
    /// Lower `a and b` / `a or b` to a proper short-circuit CFG.
    ///
    /// Pattern (for `and`):
    ///
    ///     current_block:
    ///         left_val := eval(a)
    ///         Branch left_val → eval_right, short
    ///     eval_right:
    ///         right_val := eval(b)
    ///         result_var := right_val
    ///         Jump merge
    ///     short:
    ///         result_var := false
    ///         Jump merge
    ///     merge:
    ///         (result_var holds `a and b`)
    ///
    /// `or` swaps the two branches, with the short-circuit constant
    /// being `true`.
    ///
    /// The pattern handles nested short-circuits correctly by consulting
    /// `pending_merge` after translating each operand — the same way
    /// `Stmt::VarDecl` handles a branching initializer.
    ///```
    pub(super) fn translate_short_circuit(
        &mut self,
        program: &mut SemanticProgram,
        func: &mut SemanticFunction,
        current_block: usize,
        left: &Expr,
        right: &Expr,
        op: &BinOp,
    ) -> TypedIRValue {
        let result_var = self.allocate_result_var(func, current_block, Type::Bool);

        // Evaluate the left operand. If it branches (because it's a
        // nested short-circuit or a value-producing if/match), the
        // actual continuation block is where its merge landed.
        let left_val = self.translate_expr(program, func, current_block, left);
        let branch_block = self.pending_merge.take().unwrap_or(current_block);

        // Create the three blocks this pattern needs.
        let eval_right_id = program.new_block_id();
        let short_id = program.new_block_id();
        let merge_id = program.new_block_id();
        func.blocks.push(SemanticBlock { id: eval_right_id, instructions: Vec::new(), terminator: None });
        func.blocks.push(SemanticBlock { id: short_id, instructions: Vec::new(), terminator: None });
        func.blocks.push(SemanticBlock { id: merge_id, instructions: Vec::new(), terminator: None });

        // `and`: left=true → evaluate right; left=false → short-circuit
        // `or`:  left=true → short-circuit (result=true); left=false → evaluate right
        let (then_blk, else_blk) = match op {
            BinOp::And => (eval_right_id, short_id),
            BinOp::Or => (short_id, eval_right_id),
            _ => unreachable!("translate_short_circuit called with non-And/Or op"),
        };

        let _ = self.safe_set_terminator(func, branch_block, Terminator::Branch {
            condition: left_val,
            then_block: then_blk,
            else_block: else_blk,
        });

        // Right branch: evaluate the right operand, then assign its
        // result. A nested short-circuit inside `right` sets
        // pending_merge to its own merge block, so we must use that
        // as the block to append the Assign to.
        let right_val = self.translate_expr(program, func, eval_right_id, right);
        let right_end = self.pending_merge.take().unwrap_or(eval_right_id);
        let _ = self.safe_push_instruction(func, right_end, SemanticInstruction::Assign {
            target: result_var.clone(),
            value: right_val,
        });
        let _ = self.safe_set_terminator(func, right_end, Terminator::Jump { block: merge_id });

        // Short branch: assign the short-circuit constant.
        let short_const = match op {
            BinOp::And => TypedIRValue::Bool(false),
            BinOp::Or => TypedIRValue::Bool(true),
            _ => unreachable!(),
        };
        let _ = self.safe_push_instruction(func, short_id, SemanticInstruction::Assign {
            target: result_var.clone(),
            value: short_const,
        });
        let _ = self.safe_set_terminator(func, short_id, Terminator::Jump { block: merge_id });

        self.pending_merge = Some(merge_id);
        TypedIRValue::Variable(result_var, Type::Bool)
    }
}