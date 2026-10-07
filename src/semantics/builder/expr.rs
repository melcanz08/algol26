// src/semantics/builder/expr.rs

use super::*;

impl SemanticIRBuilder {
    pub(super) fn translate_simple_stmt(
        &mut self,
        program: &mut SemanticProgram,
        func: &mut SemanticFunction,
        current_block: usize,
        stmt: &Stmt,
    ) -> FlowResult {
        if let Stmt::Expression(Expr {
            kind:
                ExprKind::If {
                    condition,
                    then_branch,
                    else_branch,
                    ..
                },
            ..
        }) = stmt
        {
            // Collect each branch's statements, including the block's
            // trailing expression as a final statement. A branch like
            //
            //     if c
            //         small.append(x)
            //
            // parses as a block with empty `statements` and a
            // `trailing_expr`; without this, the call would be dropped
            // when the if is in statement position.
            fn collect_branch_stmts(block: &Expr) -> Vec<Stmt> {
                match &block.kind {
                    ExprKind::Block {
                        statements,
                        trailing_expr,
                        ..
                    } => {
                        let mut all = statements.clone();
                        if let Some(te) = trailing_expr {
                            all.push(Stmt::Expression((**te).clone()));
                        }
                        all
                    }
                    _ => vec![Stmt::Expression(block.clone())],
                }
            }

            let then_stmts_owned = collect_branch_stmts(then_branch);
            let else_stmts_owned = else_branch.as_ref().map(|e| collect_branch_stmts(e));

            return self.translate_if(
                program,
                func,
                current_block,
                condition,
                &then_stmts_owned,
                else_stmts_owned.as_deref(),
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
                if matches!(&value.kind, ExprKind::For { .. } | ExprKind::While { .. }) {
                    let decl_type = if let Some(t) = type_annotation {
                        self.resolve_type_syntax(t)
                    } else {
                        Type::Void
                    };
                    self.declare_var(name, decl_type.clone(), *mutable);
                    self.safe_push_instruction(
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
                    let _ = match &value.kind {
                        ExprKind::For {
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
                        ExprKind::While {
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

                // `val p := alloc(n)` — Allocate does the binding itself.
                if let ExprKind::FunctionCall {
                    name: fn_name,
                    args,
                    ..
                } = &value.kind
                {
                    if fn_name == "alloc" && args.len() == 1 {
                        let size = self.translate_expr(program, func, current_block, &args[0]);
                        let ptr_ty = Type::pointer(Type::Unknown);
                        self.declare_var(name, ptr_ty.clone(), *mutable);
                        self.safe_push_instruction(
                            func,
                            current_block,
                            SemanticInstruction::Allocate {
                                target: name.clone(),
                                size,
                                type_: ptr_ty,
                            },
                        );
                        return FlowResult::Reachable(current_block);
                    }
                }

                if let ExprKind::List(elements, _) = &value.kind {
                    self.list_values.insert(name.clone(), elements.clone());
                }

                let typed_value = self.translate_expr(program, func, current_block, value);
                let value_type = typed_value.type_of();

                let type_ = if let Some(annot) = type_annotation {
                    let declared_type = self.resolve_type_syntax(annot);
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

                // If the initializer produced a Call, push it as a standalone
                // Instruction::Call (DCE preserves those unconditionally, so the
                // side effect survives even when the binding is unused) and have
                // the Declare reference the resulting binding rather than
                // re-evaluating the call.
                //
                // The callee name and arguments are taken directly from
                // `typed_value`, which `translate_expr` has already resolved:
                // method dispatch (`xs.length()` → `List.length`,
                // `m.get(k)` → `Map.get`), generic mangling (`f<Int>` → `f_Int`),
                // and builtin lowering all happen there. Reconstructing the callee
                // from the raw source name here would produce `xs.length` rather
                // than `List.length` and break both the interpreter and the IR
                // shape tests.
                let declare_value = match typed_value {
                    TypedIRValue::Call {
                        function,
                        args: call_args,
                        ..
                    } => {
                        self.safe_push_instruction(
                            func,
                            current_block,
                            Instruction::Call {
                                func: function,
                                args: call_args,
                                result: Some(name.clone()),
                            },
                        );
                        TypedIRValue::Variable(name.clone(), type_.clone())
                    }
                    other => other,
                };

                if let Some(merge) = self.pending_merge.take() {
                    self.safe_push_instruction(
                        func,
                        merge,
                        SemanticInstruction::Declare {
                            name: name.clone(),
                            mutable: *mutable,
                            type_,
                            value: declare_value,
                        },
                    );
                    return FlowResult::Reachable(merge);
                }

                SemanticInstruction::Declare {
                    name: name.clone(),
                    mutable: *mutable,
                    type_,
                    value: declare_value,
                }
            }
            Stmt::Assign { name, value, .. } => {
                let var_info = match self.lookup_var(name) {
                    Some(info) => info.clone(),
                    None => {
                        self.diagnostics
                            .push(format!("Assignment to undeclared variable '{}'", name));
                        VariableInfo {
                            type_: Type::Unknown,
                            mutable: true,
                        }
                    }
                };

                if !var_info.mutable {
                    self.diagnostics
                        .push(format!("Cannot assign to immutable variable '{}'", name));
                }

                // `p := alloc(n)` is a memory operation, not a generic
                // assignment. Emit `Instruction::Allocate` (which
                // stores the fresh pointer into `p`'s alloca) so the
                // codegen goes through the malloc lowering and the
                // region tracker sees the pointer. This mirrors what
                // `Stmt::VarDecl` does for `var p := alloc(n)`.
                // Without this interception, `alloc` on an Assign RHS
                // reaches LLVM codegen as a generic Call and fails
                // with "unhandled builtin 'alloc'".
                if let ExprKind::FunctionCall {
                    name: fn_name,
                    args,
                    ..
                } = &value.kind
                {
                    if fn_name == "alloc" && args.len() == 1 {
                        let size = self.translate_expr(program, func, current_block, &args[0]);
                        let ptr_ty = Type::pointer(Type::Unknown);
                        self.safe_push_instruction(
                            func,
                            current_block,
                            SemanticInstruction::Allocate {
                                target: name.clone(),
                                size,
                                type_: ptr_ty,
                            },
                        );
                        if let Some(merge) = self.pending_merge.take() {
                            return FlowResult::Reachable(merge);
                        }
                        return FlowResult::Reachable(current_block);
                    }
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
            Stmt::Print { expr, .. } => {
                let typed_value = self.translate_expr(program, func, current_block, expr);
                SemanticInstruction::Print { value: typed_value }
            }
            Stmt::Return { value, .. } => {
                let typed_value = value
                    .as_ref()
                    .map(|v| self.translate_expr(program, func, current_block, v));

                // If the value's translation branched (match / if /
                // try), the Return belongs in the *merge* block that
                // the value-producing expression created. Placing it
                // in `current_block` would put it after the Branch /
                // Switch terminator and leave the merge block
                // unterminated — which the CFG verifier rejects.
                let return_block = self.pending_merge.take().unwrap_or(current_block);

                let coerced_value = typed_value.map(|v| self.coerce_value(v, &func.return_type));
                let type_ = coerced_value
                    .as_ref()
                    .map(|v| v.type_of())
                    .unwrap_or(Type::Void);
                // The declared return type came from `TypeSyntax::to_type`,
                // which does not know about records. When the declared
                // annotation mentions `Unknown` anywhere, the analyzer's
                // type for the returned expression is authoritative.
                if !func.return_type.contains_unknown()
                    && type_ != Type::Unknown
                    && !type_.can_coerce_to(&func.return_type)
                {
                    self.diagnostics.push(format!(
                        "Return type mismatch in function '{}': expected {:?}, found {:?}",
                        func.name, func.return_type, type_
                    ));
                }

                // If any defers are pending in the enclosing scope,
                // chain them LIFO before the actual return. Each
                // cleanup block runs its body and jumps to the next;
                // the last one emits the real Return with the
                // captured value.
                if let Some(defer_ctx) = self.defer_stack.last() {
                    if !defer_ctx.cleanup_blocks.is_empty() {
                        let cleanups: Vec<usize> =
                            defer_ctx.cleanup_blocks.iter().rev().copied().collect();

                        // Return block jumps to the first cleanup.
                        self.safe_set_terminator(
                            func,
                            return_block,
                            Terminator::Jump { block: cleanups[0] },
                        );

                        // Each cleanup jumps to the next.
                        for i in 0..cleanups.len() - 1 {
                            self.safe_set_terminator(
                                func,
                                cleanups[i],
                                Terminator::Jump {
                                    block: cleanups[i + 1],
                                },
                            );
                        }

                        // Last cleanup emits the actual return.
                        self.safe_set_terminator(
                            func,
                            *cleanups.last().expect("cleanups is non-empty here"),
                            Terminator::Return {
                                value: coerced_value,
                                type_,
                            },
                        );

                        return FlowResult::Unreachable;
                    }
                }

                // No pending defers: normal return on the merge block.
                self.safe_set_terminator(
                    func,
                    return_block,
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
                ..
            } => {
                let arr_expr = Expr::new(ExprKind::Var(array.clone(), Span::default()));
                let arr_val = self.translate_expr(program, func, current_block, &arr_expr);
                let idx_val = self.translate_expr(program, func, current_block, index);
                let val = self.translate_expr(program, func, current_block, value);
                SemanticInstruction::ArrayAssign {
                    array: Box::new(arr_val),
                    index: Box::new(idx_val),
                    value: val,
                }
            }
            Stmt::FieldAssign {
                target,
                field,
                value,
                ..
            } => {
                let typed_value = self.translate_expr(program, func, current_block, value);
                SemanticInstruction::FieldAssign {
                    target: target.clone(),
                    field: field.clone(),
                    value: typed_value,
                }
            }
            Stmt::ChannelDecl { name, .. } => {
                let chan_type = Type::Channel(Box::new(Type::Unknown));
                self.declare_var(name, chan_type.clone(), true);
                SemanticInstruction::ChannelDecl {
                    name: name.clone(),
                    type_: chan_type,
                }
            }
            Stmt::Send { channel, value, .. } => {
                let typed_value = self.translate_expr(program, func, current_block, value);
                SemanticInstruction::SendChannel {
                    channel: channel.clone(),
                    value: typed_value,
                }
            }
            Stmt::Receive {
                channel, target, ..
            } => SemanticInstruction::ReceiveChannel {
                channel: channel.clone(),
                target: target.clone(),
            },
            Stmt::UnsafeBlock { body, .. } => {
                for s in body {
                    let _ = self.translate_simple_stmt(program, func, current_block, s);
                }
                SemanticInstruction::Nop
            }
            Stmt::Import { .. } => SemanticInstruction::Nop,
            Stmt::Expression(expr) => {
                // alloc(n) / free(p) in statement position are memory
                // operations, not generic calls. Intercept before the
                // discarded-call path below. (Step 2 wiring.)
                if let ExprKind::FunctionCall {
                    name: fn_name,
                    args,
                    ..
                } = &expr.kind
                {
                    if fn_name == "alloc" && args.len() == 1 {
                        let size = self.translate_expr(program, func, current_block, &args[0]);
                        let temp = format!("__alloc_{}", self.iter_counter);
                        self.iter_counter += 1;
                        let ptr_ty = Type::pointer(Type::Unknown);
                        self.declare_var(&temp, ptr_ty.clone(), false);
                        self.safe_push_instruction(
                            func,
                            current_block,
                            SemanticInstruction::Allocate {
                                target: temp,
                                size,
                                type_: ptr_ty,
                            },
                        );
                        if let Some(merge) = self.pending_merge.take() {
                            return FlowResult::Reachable(merge);
                        }
                        return FlowResult::Reachable(current_block);
                    }
                    if fn_name == "free" && args.len() == 1 {
                        let ptr = self.translate_expr(program, func, current_block, &args[0]);
                        self.safe_push_instruction(
                            func,
                            current_block,
                            SemanticInstruction::Free { ptr },
                        );
                        if let Some(merge) = self.pending_merge.take() {
                            return FlowResult::Reachable(merge);
                        }
                        return FlowResult::Reachable(current_block);
                    }
                }

                // A discarded function call must still execute its side
                // effects. `translate_expr` returns the `TypedIRValue::Call`
                // without pushing an instruction — the caller pushes it.
                //
                // The callee and args come from `typed_value`, which
                // `translate_expr` has already resolved: method dispatch
                // (`xs.length()` → `List.length`, `m.insert(k, v)` → `Map.insert`),
                // generic mangling (`f<Int>` → `f_Int`), and container prefixes
                // all happen there. Reconstructing the callee from the raw source
                // name here would produce `xs.length` and break both the IR shape
                // invariants and the interpreter's builtin dispatch.
                if matches!(&expr.kind, ExprKind::FunctionCall { .. }) {
                    let typed_value = self.translate_expr(program, func, current_block, expr);
                    if let TypedIRValue::Call {
                        function,
                        args: call_args,
                        ..
                    } = typed_value
                    {
                        self.safe_push_instruction(
                            func,
                            current_block,
                            Instruction::Call {
                                func: function,
                                args: call_args,
                                result: None,
                            },
                        );
                    }
                } else {
                    let _typed_value = self.translate_expr(program, func, current_block, expr);
                }
                if let Some(merge) = self.pending_merge.take() {
                    return FlowResult::Reachable(merge);
                }
                if self.block_is_terminated(func, current_block) {
                    // The expression set a terminator on the current block
                    // without scheduling a merge (e.g. a match whose every
                    // case returns). No following statement in this block
                    // is reachable.
                    return FlowResult::Unreachable;
                }
                SemanticInstruction::Nop
            }
            _ => {
                self.diagnostics
                    .push("Control flow statement not intercepted".to_string());
                return FlowResult::Unreachable;
            }
        };

        self.safe_push_instruction(func, current_block, instruction);

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
        match &expr.kind {
            ExprKind::Unary { op, expr, .. } => {
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
            ExprKind::Borrow { expr, .. } => {
                let inner = self.translate_expr(program, func, current_block, expr);
                let inner_type = inner.type_of();
                TypedIRValue::BorrowShared {
                    expr: Box::new(inner),
                    target_type: Type::borrow(inner_type),
                }
            }
            ExprKind::MutBorrow { expr, .. } => {
                let inner = self.translate_expr(program, func, current_block, expr);
                let inner_type = inner.type_of();
                TypedIRValue::BorrowMutable {
                    expr: Box::new(inner),
                    target_type: Type::mut_borrow(inner_type),
                }
            }
            ExprKind::Deref { expr, .. } => {
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
                TypedIRValue::ReadReference {
                    expr: Box::new(inner),
                    target_type,
                }
            }
            ExprKind::AddrOf { expr, .. } => {
                let inner = self.translate_expr(program, func, current_block, expr);
                let inner_type = inner.type_of();
                TypedIRValue::AddrOf {
                    expr: Box::new(inner),
                    target_type: Type::pointer(inner_type),
                }
            }
            ExprKind::Number(n, _) => TypedIRValue::Float(*n),
            ExprKind::Int(i, _) => {
                // ADR 0031: literal coercion into a subrange. The
                // analyzer recorded the subrange type for this
                // expression when the surrounding context demanded
                // it (`val q: Percentage := 50`). Wrap the literal
                // in a Cast so the IR sees the correct type — same
                // shape as the nominal from_base and subrange T(v)
                // intercepts produce.
                match self.type_of_expr(expr) {
                    Some(ty @ Type::Subrange { .. }) => TypedIRValue::Cast {
                        value: Box::new(TypedIRValue::Int(*i)),
                        target_type: ty,
                    },
                    _ => TypedIRValue::Int(*i),
                }
            }
            ExprKind::String(s, _) => TypedIRValue::String(s.clone()),
            ExprKind::Bool(b, _) => TypedIRValue::Bool(*b),
            ExprKind::Var(name, span) => {
                // Associated constant: `Foo::SIZE`. The parser folds
                // `::` into the identifier, so this is the only place
                // a `::` name can reach the builder. Inline the
                // stored value expression — the recursion handles
                // constants defined in terms of other constants.
                if name.contains("::") {
                    if let Some((_, value_expr)) = self.const_values.get(name).cloned() {
                        return self.translate_expr(program, func, current_block, &value_expr);
                    }
                }

                // ─── UNIFY TYPES ─── prefer analyzer type, fall back to local scope.
                let ty = self
                    .type_of_expr(expr)
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
            ExprKind::List(elements, _) => {
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
                    Some(Type::List(inner)) => *inner,
                    _ => values.first().map(|v| v.type_of()).unwrap_or(Type::Unknown),
                };
                TypedIRValue::List(values, elem_type)
            }
            ExprKind::RecordLiteral { name, fields, .. } => {
                let mut translated = Vec::with_capacity(fields.len());
                for (fname, fexpr) in fields {
                    let fval = self.translate_expr(program, func, current_block, fexpr);
                    translated.push((fname.clone(), fval));
                }
                // ─── UNIFY TYPES ─── the analyzer already produced the record type.
                let record_type = self
                    .type_of_expr(expr)
                    .unwrap_or_else(|| Type::record(name, Vec::new()));
                TypedIRValue::Record {
                    name: name.clone(),
                    fields: translated,
                    record_type,
                }
            }
            ExprKind::Binary {
                left, op, right, ..
            } => match op {
                BinOp::And | BinOp::Or => {
                    self.translate_short_circuit(program, func, current_block, left, right, op)
                }
                _ => {
                    let l = self.translate_expr(program, func, current_block, left);
                    let r = self.translate_expr(program, func, current_block, right);

                    // ADR 0032 A5c: set operations. The analyzer has
                    // already verified the operand shapes (both set
                    // for the eight operators, scalar-in-set for `in`),
                    // so we just pick the SemanticBinOp variant.
                    let l_is_set = matches!(l.type_of(), Type::Set(_));
                    let r_is_set = matches!(r.type_of(), Type::Set(_));
                    if l_is_set || r_is_set {
                        let result_type = self.type_of_expr(expr).unwrap_or(Type::Unknown);
                        // For `in`, shift the element's ordinal down
                        // to its bit position when the set's element
                        // type is a subrange with a non-zero low.
                        // `Byte(5)` where `Byte = Int in 10..73` maps
                        // to bit `5 - 10`... which is negative, so the
                        // analyzer must reject values below `low`. For
                        // the common case `low == 0` (enums, Bool) this
                        // is a no-op.
                        let l = if matches!(op, BinOp::In) {
                            let low = match r.type_of() {
                                Type::Set(el) => match el.as_ref() {
                                    Type::Subrange { low, .. } => *low,
                                    _ => 0,
                                },
                                _ => 0,
                            };
                            if low != 0 {
                                // Constant case: fold `ordinal - low`
                                // to a plain Int here, so no Subtract
                                // node is emitted. Handles the common
                                // `Byte(15)` form, which reaches us as
                                // `Cast { value: Int(15), target_type: Byte }`.
                                if let Some(ordinal) = Self::extract_set_element_ordinal(&l) {
                                    TypedIRValue::Int(ordinal - low)
                                } else {
                                    // Non-constant: unwrap the type tag
                                    // so both Subtract operands are
                                    // Int-typed. Subranges and enums
                                    // erase to Int at runtime, so the
                                    // underlying value is already an
                                    // Int — the Cast/Variable type
                                    // annotation is what we're dropping.
                                    let l_int = match &l {
                                        TypedIRValue::Cast { value, .. } => (**value).clone(),
                                        TypedIRValue::Variable(name, _) => {
                                            TypedIRValue::Variable(name.clone(), Type::Int)
                                        }
                                        other => other.clone(),
                                    };
                                    TypedIRValue::BinaryOp {
                                        op: SemanticBinOp::Subtract,
                                        left: Box::new(l_int),
                                        right: Box::new(TypedIRValue::Int(low)),
                                        result_type: Type::Int,
                                    }
                                }
                            } else {
                                l
                            }
                        } else {
                            l
                        };
                        let semantic_op = match op {
                            BinOp::Add => SemanticBinOp::SetUnion,
                            BinOp::Subtract => SemanticBinOp::SetDifference,
                            BinOp::Multiply => SemanticBinOp::SetIntersection,
                            BinOp::In => SemanticBinOp::SetMember,
                            BinOp::LessEqual => SemanticBinOp::SetSubset,
                            BinOp::Less => SemanticBinOp::SetStrictSubset,
                            BinOp::GreaterEqual => SemanticBinOp::SetSuperset,
                            BinOp::Greater => SemanticBinOp::SetStrictSuperset,
                            // Equality is reused — u64 bit-equality is
                            // correct for sets, so no SetEqual variant.
                            BinOp::Equal => SemanticBinOp::Equal,
                            BinOp::NotEqual => SemanticBinOp::NotEqual,
                            other => unreachable!(
                                "IR builder reached {:?} with set operands — \
                                 the analyzer should have rejected it (ADR 0032)",
                                other
                            ),
                        };
                        return TypedIRValue::BinaryOp {
                            op: semantic_op,
                            left: Box::new(l),
                            right: Box::new(r),
                            result_type,
                        };
                    }

                    // ─── UNIFY TYPES ─── Type comes from the analyzer.
                    let result_type = self.type_of_expr(expr).unwrap_or(Type::Unknown);

                    // Keep IR self-consistent by inserting Int→Float coercions.
                    let (cast_l, cast_r) = match (l.type_of(), r.type_of()) {
                        (Type::Int, Type::Float) => (
                            TypedIRValue::Cast {
                                value: Box::new(l),
                                target_type: Type::Float,
                            },
                            r,
                        ),
                        (Type::Float, Type::Int) => (
                            l,
                            TypedIRValue::Cast {
                                value: Box::new(r),
                                target_type: Type::Float,
                            },
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
                        BinOp::And | BinOp::Or => {
                            unreachable!("And/Or handled by translate_short_circuit arm above")
                        }
                        BinOp::In => unreachable!(
                            "IR builder reached BinOp::In — set operations \
                             are A5-pending (ADR 0032)"
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
            ExprKind::FunctionCall { name, args, .. } => {
                let clean_name = name.trim_end_matches("()");

                // UFCS: `Trait::method(receiver, extra...)`. The
                // flattened function is registered under
                // `{trait}_{type}_{method}` (see `make_impl_method`).
                // Translate the source name here, and wrap the
                // receiver in the borrow the method's self mode
                // expects.
                if let Some((trait_name, method_name)) = clean_name.split_once("::") {
                    let mut typed_args: Vec<TypedIRValue> = args
                        .iter()
                        .map(|a| self.translate_expr(program, func, current_block, a))
                        .collect();
                    if typed_args.is_empty() {
                        return TypedIRValue::Void;
                    }
                    let receiver_type = typed_args[0].type_of();
                    let owner = match &receiver_type {
                        Type::Record(n, _) => n.clone(),
                        Type::Distinct { name, .. } => name.clone(),
                        Type::Enum { name, .. } => name.clone(),
                        other => other.to_string(),
                    };
                    let mangled = format!("{}_{}_{}", trait_name, owner, method_name);
                    if let Some(sig) = self.function_types.get(&mangled) {
                        match sig.params.first().map(|(_, t)| t) {
                            Some(Type::Borrow(_)) => {
                                if !matches!(typed_args[0], TypedIRValue::BorrowShared { .. }) {
                                    let inner = typed_args.remove(0);
                                    typed_args.insert(
                                        0,
                                        TypedIRValue::BorrowShared {
                                            expr: Box::new(inner),
                                            target_type: Type::borrow(receiver_type.clone()),
                                        },
                                    );
                                }
                            }
                            Some(Type::MutBorrow(_)) => {
                                let inner = typed_args.remove(0);
                                typed_args.insert(
                                    0,
                                    TypedIRValue::BorrowMutable {
                                        expr: Box::new(inner),
                                        target_type: Type::mut_borrow(receiver_type.clone()),
                                    },
                                );
                            }
                            _ => {}
                        }
                    }
                    let return_type = self.type_of_expr(expr).unwrap_or(Type::Unknown);
                    return TypedIRValue::Call {
                        function: mangled,
                        args: typed_args,
                        return_type,
                    };
                }

                // ADR 0031: subrange construction. `Percentage(75)`.
                // The callee is a bare identifier, not a dotted name.
                // Emit a Cast that carries the subrange type; the
                // runtime bounds check (A5) will be inserted
                // separately for non-literal arguments.
                if !clean_name.contains('.') {
                    if let Some(subrange) = self.subrange_types.get(clean_name).cloned() {
                        let inner = if let Some(arg) = args.first() {
                            self.translate_expr(program, func, current_block, arg)
                        } else {
                            TypedIRValue::Void
                        };
                        // ADR 0031 A5: emit a runtime bounds check for
                        // non-literal arguments. The analyzer already
                        // range-checked literals at compile time.
                        if let Type::Subrange {
                            name: sr_name,
                            low,
                            high,
                            ..
                        } = &subrange
                        {
                            let is_literal = args.first().is_some_and(|a| {
                                matches!(a.kind, crate::frontend::ast::ExprKind::Int(_, _))
                            });
                            if !is_literal {
                                let message =
                                    format!("{}: value out of range {}..{}", sr_name, low, high);
                                self.safe_push_instruction(
                                    func,
                                    current_block,
                                    SemanticInstruction::BoundsCheck {
                                        value: inner.clone(),
                                        low: *low,
                                        high: *high,
                                        message,
                                    },
                                );
                            }
                        }
                        return TypedIRValue::Cast {
                            value: Box::new(inner),
                            target_type: subrange,
                        };
                    }
                }

                // ADR 0029/0030: conversion intrinsics. Each lowers
                // to a no-op Cast that carries the target type.
                if let Some(dot) = clean_name.find('.') {
                    let (receiver, method) = (&clean_name[..dot], &clean_name[dot + 1..]);

                    // T.from_base(x): x already has the base
                    // representation; wrap it with the nominal type.
                    if method == "from_base" {
                        if let Some(nominal) = self.nominal_types.get(receiver).cloned() {
                            let inner = if let Some(arg) = args.first() {
                                self.translate_expr(program, func, current_block, arg)
                            } else {
                                TypedIRValue::Void
                            };
                            return TypedIRValue::Cast {
                                value: Box::new(inner),
                                target_type: nominal,
                            };
                        }
                    }

                    // T.from_ordinal(x): the enum value's runtime
                    // representation is the ordinal, an Int. Wrap it
                    // with the enum type. See ADR 0030.
                    if method == "from_ordinal" {
                        if let Some(enum_ty) = self.enum_types.get(receiver).cloned() {
                            let inner = if let Some(arg) = args.first() {
                                self.translate_expr(program, func, current_block, arg)
                            } else {
                                TypedIRValue::Void
                            };
                            return TypedIRValue::Cast {
                                value: Box::new(inner),
                                target_type: enum_ty,
                            };
                        }
                    }

                    // v.to_base(): unwrap the nominal type to its base.
                    if method == "to_base" && args.is_empty() {
                        if let Some(info) = self.lookup_var(receiver) {
                            if let Type::Distinct { base, .. } = &info.type_ {
                                let receiver_value = TypedIRValue::Variable(
                                    receiver.to_string(),
                                    info.type_.clone(),
                                );
                                let base_ty = (**base).clone();
                                return TypedIRValue::Cast {
                                    value: Box::new(receiver_value),
                                    target_type: base_ty,
                                };
                            }
                        }
                    }

                    // v.to_ordinal(): unwrap the enum to its ordinal.
                    if method == "to_ordinal" && args.is_empty() {
                        if let Some(info) = self.lookup_var(receiver) {
                            if let Type::Enum { .. } = &info.type_ {
                                let receiver_value = TypedIRValue::Variable(
                                    receiver.to_string(),
                                    info.type_.clone(),
                                );
                                return TypedIRValue::Cast {
                                    value: Box::new(receiver_value),
                                    target_type: Type::Int,
                                };
                            }
                        }
                    }

                    // ADR 0031: v.to_base() where v has a subrange
                    // type. No-op Cast to the base.
                    if method == "to_base" && args.is_empty() {
                        if let Some(info) = self.lookup_var(receiver) {
                            if let Type::Subrange { base, .. } = &info.type_ {
                                let receiver_value = TypedIRValue::Variable(
                                    receiver.to_string(),
                                    info.type_.clone(),
                                );
                                let base_ty = (**base).clone();
                                return TypedIRValue::Cast {
                                    value: Box::new(receiver_value),
                                    target_type: base_ty,
                                };
                            }
                        }
                    }
                }

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

                            // ─── Container method dispatch (ADR 0027, ADR 0028) ───
                            // Map methods and List.append aren't registered in
                            // `function_types`, so the generic method path below
                            // won't find them. Handle them first: the receiver is
                            // prepended as the first arg, and the callee name is
                            // `<Container>.<method>`.
                            let prefix = match &receiver_type {
                                Type::Map(..) => Some("Map"),
                                Type::List(_) if method_name == "append" => Some("List"),
                                _ => None,
                            };
                            if let Some(prefix) = prefix {
                                let receiver_value = TypedIRValue::Variable(
                                    receiver_name.to_string(),
                                    receiver_type.clone(),
                                );
                                let mut call_args = vec![receiver_value];
                                for arg in args {
                                    call_args.push(self.translate_expr(
                                        program,
                                        func,
                                        current_block,
                                        arg,
                                    ));
                                }
                                let return_type = self.type_of_expr(expr).unwrap_or(Type::Unknown);
                                return TypedIRValue::Call {
                                    function: format!("{}.{}", prefix, method_name),
                                    args: call_args,
                                    return_type,
                                };
                            }

                            if let Some(resolved_name) =
                                self.resolve_method_call(&receiver_type, method_name)
                            {
                                let raw_receiver = TypedIRValue::Variable(
                                    receiver_name.to_string(),
                                    receiver_type.clone(),
                                );

                                // Wrap the receiver to match the method's declared self mode.
                                // `User_label(self: &User)` expects `Borrow<User>`; passing a
                                // bare `User` fails the verifier's arg-type check. Same for
                                // `&mut self`. By-value `self: T` passes through unchanged.
                                let receiver_value = match self
                                    .function_types
                                    .get(&resolved_name)
                                    .and_then(|sig| sig.params.first())
                                    .map(|(_, t)| t.clone())
                                {
                                    Some(Type::Borrow(_)) => TypedIRValue::BorrowShared {
                                        expr: Box::new(raw_receiver),
                                        target_type: Type::borrow(receiver_type.clone()),
                                    },
                                    Some(Type::MutBorrow(_)) => TypedIRValue::BorrowMutable {
                                        expr: Box::new(raw_receiver),
                                        target_type: Type::mut_borrow(receiver_type.clone()),
                                    },
                                    _ => raw_receiver,
                                };

                                let mut call_args = vec![receiver_value];
                                for arg in args {
                                    call_args.push(self.translate_expr(
                                        program,
                                        func,
                                        current_block,
                                        arg,
                                    ));
                                }
                                // ADR 0034: rewrite to the specialization's
                                // mangled name if the analyzer recorded an
                                // instantiation for this call site. Non-generic
                                // methods return `resolved_name` unchanged.
                                let emitted_name = self.resolved_callee_name(expr, &resolved_name);
                                let return_type = self.type_of_expr(expr).unwrap_or(Type::Unknown);
                                return TypedIRValue::Call {
                                    function: emitted_name,
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
                let return_type = self.type_of_expr(expr).unwrap_or(Type::Unknown);

                // Rewrite the callee to its mangled specialization name
                // if this call has a plan entry. For non-generic calls
                // this returns `clean_name` unchanged.
                let emitted_name = self.resolved_callee_name(expr, clean_name);

                let coerced_args =
                    if let Some(sig) = self.function_types.get(&emitted_name).cloned() {
                        typed_args
                            .into_iter()
                            .zip(sig.params.iter())
                            .map(|(a, (_, t))| self.coerce_value(a, t))
                            .collect()
                    } else if let Some(sig) = self.function_types.get(clean_name).cloned() {
                        typed_args
                            .into_iter()
                            .zip(sig.params.iter())
                            .map(|(a, (_, t))| self.coerce_value(a, t))
                            .collect()
                    } else {
                        typed_args
                    };

                TypedIRValue::Call {
                    function: emitted_name,
                    args: coerced_args,
                    return_type,
                }
            }
            ExprKind::MethodCall {
                receiver,
                method,
                args,
                ..
            } => {
                // Complex-receiver method call. The receiver is
                // translated as an expression, not looked up by
                // name. `&mut self` on a complex receiver was
                // already rejected by the analyzer, so only
                // shared and by-value receivers reach here.
                let receiver_value = self.translate_expr(program, func, current_block, receiver);
                let receiver_type = receiver_value.type_of();
                // ADR 0029/0030/0031: extraction intrinsics.
                // Same surface syntax as the FunctionCall arm and the
                // analyzer's MethodCall arm. A nominal, enum, or
                // subrange receiver with `to_base` / `to_ordinal`
                // lowers to a no-op Cast that carries the target type.
                match &receiver_type {
                    Type::Distinct { base, .. } if method == "to_base" && args.is_empty() => {
                        let base_ty = (**base).clone();
                        return TypedIRValue::Cast {
                            value: Box::new(receiver_value),
                            target_type: base_ty,
                        };
                    }
                    Type::Enum { .. } if method == "to_ordinal" && args.is_empty() => {
                        return TypedIRValue::Cast {
                            value: Box::new(receiver_value),
                            target_type: Type::Int,
                        };
                    }
                    Type::Subrange { base, .. } if method == "to_base" && args.is_empty() => {
                        let base_ty = (**base).clone();
                        return TypedIRValue::Cast {
                            value: Box::new(receiver_value),
                            target_type: base_ty,
                        };
                    }
                    _ => {}
                }
                if let Some(resolved_name) = self.resolve_method_call(&receiver_type, method) {
                    let wrapped = match self
                        .function_types
                        .get(&resolved_name)
                        .and_then(|sig| sig.params.first())
                        .map(|(_, t)| t.clone())
                    {
                        Some(Type::Borrow(_)) => TypedIRValue::BorrowShared {
                            expr: Box::new(receiver_value),
                            target_type: Type::borrow(receiver_type.clone()),
                        },
                        Some(Type::MutBorrow(_)) => TypedIRValue::BorrowMutable {
                            expr: Box::new(receiver_value),
                            target_type: Type::mut_borrow(receiver_type.clone()),
                        },
                        _ => receiver_value,
                    };

                    let mut call_args = vec![wrapped];
                    for arg in args {
                        call_args.push(self.translate_expr(program, func, current_block, arg));
                    }
                    // ADR 0034: same rewrite as the FunctionCall arm.
                    let emitted_name = self.resolved_callee_name(expr, &resolved_name);
                    let return_type = self.type_of_expr(expr).unwrap_or(Type::Unknown);
                    return TypedIRValue::Call {
                        function: emitted_name,
                        args: call_args,
                        return_type,
                    };
                }

                self.diagnostics
                    .push(format!("Type {} has no method '{}'", receiver_type, method));
                TypedIRValue::Void
            }
            ExprKind::ArrayAccess { array, index, .. } => {
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
            ExprKind::Some { value, .. } => {
                let inner = self.translate_expr(program, func, current_block, value);
                TypedIRValue::Some(Box::new(inner))
            }
            ExprKind::None(_) => {
                // ─── UNIFY TYPES ─── read the outer Option type from the table.
                let option_type = self
                    .type_of_expr(expr)
                    .unwrap_or(Type::option(Type::Unknown));
                TypedIRValue::None { option_type }
            }
            ExprKind::Ok { value, .. } => {
                let inner = self.translate_expr(program, func, current_block, value);
                let result_type = self
                    .type_of_expr(expr)
                    .unwrap_or(Type::result(Type::Unknown, Type::Unknown));
                TypedIRValue::Ok {
                    value: Box::new(inner),
                    result_type,
                }
            }
            ExprKind::Error { value, .. } => {
                let inner = self.translate_expr(program, func, current_block, value);
                let result_type = self
                    .type_of_expr(expr)
                    .unwrap_or(Type::result(Type::Unknown, Type::Unknown));
                TypedIRValue::Error {
                    value: Box::new(inner),
                    result_type,
                }
            }
            ExprKind::Block {
                statements,
                trailing_expr,
                ..
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
                        Stmt::Break(_) => {
                            if let Some(loop_ctx) = self.loop_stack.last().copied() {
                                self.safe_set_terminator(
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
                        Stmt::Continue(_) => {
                            if let Some(loop_ctx) = self.loop_stack.last().copied() {
                                self.safe_set_terminator(
                                    func,
                                    current,
                                    Terminator::Jump {
                                        block: loop_ctx.continue_block,
                                    },
                                );
                            } else {
                                self.diagnostics
                                    .push("Continue outside of loop".to_string());
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
            ExprKind::If {
                condition,
                then_branch,
                else_branch,
                ..
            } => {
                // ─── UNIFY TYPES ───
                let result_type = self.type_of_expr(expr).unwrap_or(Type::Unknown);

                let result_var = self.allocate_result_var(func, current_block, result_type.clone());

                let then_flow = self.translate_if_with_target(
                    program,
                    func,
                    current_block,
                    condition,
                    then_branch,
                    else_branch.as_deref(),
                    &result_var,
                    result_type.clone(),
                );

                if let FlowResult::Reachable(_merge_id) = then_flow {
                    // Return the variable that holds the result
                    TypedIRValue::Variable(result_var, result_type)
                } else {
                    self.diagnostics
                        .push("If expression has no reachable branch".to_string());
                    TypedIRValue::Void
                }
            }
            ExprKind::Match { value, cases, .. } => {
                // Translate the value being matched
                let match_value = self.translate_expr(program, func, current_block, value);
                let matched_type = match_value.type_of();

                // ─── UNIFY TYPES ───
                let result_type = self.type_of_expr(expr).unwrap_or(Type::Unknown);

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

                    // Convert pattern to SemanticPattern
                    let sem_pattern = match &case.pattern {
                        crate::frontend::ast::Pattern::Some(v) => {
                            SemanticPattern::Some { binding: v.clone() }
                        }
                        crate::frontend::ast::Pattern::None => SemanticPattern::None,
                        crate::frontend::ast::Pattern::Ok(v) => {
                            SemanticPattern::Ok { binding: v.clone() }
                        }
                        crate::frontend::ast::Pattern::Error(v) => {
                            SemanticPattern::Error { binding: v.clone() }
                        }
                        crate::frontend::ast::Pattern::Record { name, bindings } => {
                            SemanticPattern::Record {
                                name: name.clone(),
                                bindings: bindings.clone(),
                            }
                        }
                        crate::frontend::ast::Pattern::Wildcard => SemanticPattern::Wildcard,
                        crate::frontend::ast::Pattern::Binding(_) => SemanticPattern::Wildcard,
                        crate::frontend::ast::Pattern::Literal(e) => SemanticPattern::Literal(
                            self.translate_expr(program, func, current_block, e),
                        ),
                        crate::frontend::ast::Pattern::Variant(variant_name) => {
                            // ADR 0030: the ordinal is resolved at
                            // IR-build time from the matched type. The
                            // analyzer has already validated that the
                            // name is a variant of the enum, so a miss
                            // here would be an internal error; fall
                            // back to -1 so a would-be bug becomes a
                            // never-matching case rather than a panic.
                            let ordinal = match &matched_type {
                                Type::Enum { variants, .. } => variants
                                    .iter()
                                    .position(|v| v == variant_name)
                                    .map(|i| i as i64)
                                    .unwrap_or(-1),
                                _ => -1,
                            };
                            SemanticPattern::Variant {
                                name: variant_name.clone(),
                                ordinal,
                            }
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
                self.safe_set_terminator(
                    func,
                    default_block_id,
                    Terminator::Jump { block: merge_id },
                );

                // Set the switch terminator on the current block
                self.safe_set_terminator(
                    func,
                    current_block,
                    Terminator::Switch {
                        value: match_value,
                        cases: switch_cases,
                        default_block: Some(default_block_id),
                    },
                );

                // Translate each case body and assign result to result_var.
                // Track whether any case falls through to the merge —
                // if none does, the merge and default blocks are
                // unreachable and must be removed.
                let mut any_case_reaches_merge = false;
                for (idx, case) in cases.iter().enumerate() {
                    let case_block_id = case_block_ids[idx];

                    self.push_scope();

                    // Bind pattern variables (simplified; we only handle Some, Ok, Error bindings)
                    // Determine the value being matched once, so pattern bindings
                    // can be typed from its inner type rather than declared as
                    // Unknown. Without this, `case Some(v)` gives `v: Unknown`, and
                    // any method call on `v` inside the case body fails IR build.
                    let binding_and_type: Option<(String, Type)> = match &case.pattern {
                        crate::frontend::ast::Pattern::Some(v) => Some((
                            v.clone(),
                            match &matched_type {
                                Type::Option(inner) => (**inner).clone(),
                                _ => Type::Unknown,
                            },
                        )),
                        crate::frontend::ast::Pattern::Ok(v) => Some((
                            v.clone(),
                            match &matched_type {
                                Type::Result { ok, .. } => (**ok).clone(),
                                _ => Type::Unknown,
                            },
                        )),
                        crate::frontend::ast::Pattern::Error(v) => Some((
                            v.clone(),
                            match &matched_type {
                                Type::Result { error, .. } => (**error).clone(),
                                _ => Type::Unknown,
                            },
                        )),
                        _ => None,
                    };
                    if let Some((binding, ty)) = binding_and_type {
                        self.declare_var(&binding, ty, false);
                    }

                    // NEW: record destructure bindings declare each field name
                    // as a variable in the case scope.
                    if let crate::frontend::ast::Pattern::Record { bindings, .. } = &case.pattern {
                        for b in bindings {
                            self.declare_var(b, Type::Unknown, false);
                        }
                    }

                    // Extract body statements and trailing expr from case.body (which is Expr::Block)
                    let (body_stmts, body_trailing) = match &case.body.kind {
                        ExprKind::Block {
                            statements,
                            trailing_expr,
                            ..
                        } => (statements.clone(), trailing_expr.as_deref()),
                        _ => (vec![Stmt::Expression(case.body.clone())], None),
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
                        if !self.block_is_terminated(func, final_block) {
                            self.safe_set_terminator(
                                func,
                                final_block,
                                Terminator::Jump { block: merge_id },
                            );
                            any_case_reaches_merge = true;
                        }
                    }

                    self.pop_scope();
                }

                if any_case_reaches_merge {
                    self.pending_merge = Some(merge_id);
                } else {
                    // Every case body returns or diverges. The merge
                    // block has no reachable predecessor (the only
                    // edge was through the switch's default, and the
                    // analyzer enforces exhaustiveness, so the
                    // default is unreachable). Remove both blocks and
                    // clear the switch's default so the CFG verifier
                    // doesn't flag them as unreachable.
                    func.blocks
                        .retain(|b| b.id != merge_id && b.id != default_block_id);
                    if let Some(block) = func.blocks.iter_mut().find(|b| b.id == current_block) {
                        if let Some(Terminator::Switch { default_block, .. }) =
                            &mut block.terminator
                        {
                            *default_block = None;
                        }
                    }
                    self.pending_merge = None;
                }

                // Return the result variable as the value of the match expression
                TypedIRValue::Variable(result_var, result_type)
            }
            ExprKind::TryCatch {
                try_branch,
                catch_var,
                catch_branch,
                finally_body,
                ..
            } => {
                // ─── Result-based try/catch ───
                //
                // Semantics: the try body evaluates to Result<T, E>.
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

                let result_type = self.type_of_expr(expr).unwrap_or(Type::Unknown);

                let result_var = self.allocate_result_var(func, current_block, result_type.clone());

                // Fresh names for the try-body's value and the Ok payload.
                let try_value_name = format!("__try_value_{}", self.iter_counter);
                self.iter_counter += 1;
                let ok_payload_name = format!("__ok_payload_{}", self.iter_counter);
                self.iter_counter += 1;

                // Evaluate the try body, storing its Result value in try_value.
                let (try_stmts, try_trailing): (Vec<Stmt>, Option<Box<Expr>>) =
                    match &try_branch.kind {
                        ExprKind::Block {
                            statements,
                            trailing_expr,
                            ..
                        } => (statements.clone(), trailing_expr.clone()),
                        _ => (vec![Stmt::Expression(try_branch.as_ref().clone())], None),
                    };

                // Allocate the try_value variable before branching.
                self.safe_push_instruction(
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

                let switch_value = TypedIRValue::Variable(
                    try_value_name.clone(),
                    Type::result(Type::Unknown, Type::Unknown),
                );

                self.safe_set_terminator(
                    func,
                    after_try,
                    Terminator::Switch {
                        value: switch_value,
                        cases: vec![
                            (
                                SemanticPattern::Ok {
                                    binding: ok_payload_name.clone(),
                                },
                                ok_block_id,
                            ),
                            (
                                SemanticPattern::Error {
                                    binding: catch_binding.clone(),
                                },
                                err_block_id,
                            ),
                        ],
                        default_block: Some(err_block_id),
                    },
                );

                // ─── Ok block: assign payload to result_var ───
                self.declare_var(&ok_payload_name, result_type.clone(), false);
                self.safe_push_instruction(
                    func,
                    ok_block_id,
                    SemanticInstruction::Assign {
                        target: result_var.clone(),
                        value: TypedIRValue::Variable(ok_payload_name.clone(), result_type.clone()),
                    },
                );
                self.safe_set_terminator(func, ok_block_id, Terminator::Jump { block: merge_id });

                // ─── Err block: translate catch body ───
                // The switch's Error pattern binds catch_var at runtime;
                // declare it in the IR builder's scope so the catch body
                // can reference it.
                self.push_scope();
                if catch_var.is_some() {
                    self.declare_var(&catch_binding, Type::Unknown, false);
                }

                let (catch_stmts, catch_trailing): (Vec<Stmt>, Option<Box<Expr>>) =
                    match &catch_branch.kind {
                        ExprKind::Block {
                            statements,
                            trailing_expr,
                            ..
                        } => (statements.clone(), trailing_expr.clone()),
                        _ => (vec![Stmt::Expression(catch_branch.as_ref().clone())], None),
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
                        self.safe_set_terminator(
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
                    self.safe_set_terminator(
                        func,
                        err_block_id,
                        Terminator::Jump { block: merge_id },
                    );
                }

                // ─── Finally: runs after both branches at the merge ───
                let final_reachable = if let Some(finally_stmts) = finally_body {
                    self.push_scope();
                    let finally_flow = self.translate_block(program, func, merge_id, finally_stmts);
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
            ExprKind::For {
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
            ExprKind::While {
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
            ExprKind::PtrLiteral(val, _) => TypedIRValue::PtrLiteral(*val),
            ExprKind::NullPtr(_) => TypedIRValue::NullPtr,
            ExprKind::Range { start, end, .. } => {
                // For now, represent a range as a list containing the start and end values.
                // This is not a full range implementation, but avoids silent Void.
                let start_val = start
                    .as_ref()
                    .map(|e| self.translate_expr(program, func, current_block, e))
                    .unwrap_or(TypedIRValue::Void);
                let end_val = end
                    .as_ref()
                    .map(|e| self.translate_expr(program, func, current_block, e))
                    .unwrap_or(TypedIRValue::Void);
                let elem_type = start_val.type_of().common_supertype(&end_val.type_of());
                TypedIRValue::List(vec![start_val, end_val], elem_type)
            }
            ExprKind::FieldAccess { object, field, .. } => {
                // ADR 0032 (A4): qualified enum variant value.
                // `Day.Saturday` has no object to translate — `Day`
                // is a type name. Emit the variant's ordinal as a
                // constant wrapped in a Cast to the enum type, which
                // is the same shape `Day.from_ordinal(5)` produces.
                if let ExprKind::Var(name, _) = &object.as_ref().kind {
                    if let Some(enum_ty) = self.enum_types.get(name).cloned() {
                        if let Type::Enum { variants, .. } = &enum_ty {
                            if let Some(ordinal) = variants.iter().position(|v| v == field) {
                                return TypedIRValue::Cast {
                                    value: Box::new(TypedIRValue::Int(ordinal as i64)),
                                    target_type: enum_ty,
                                };
                            }
                        }
                    }
                }
                let obj = self.translate_expr(program, func, current_block, object);
                let obj_ty = obj.type_of();
                // Auto-deref to match the analyzer's field-access behavior.
                let obj_ty = match &obj_ty {
                    Type::Borrow(inner) | Type::MutBorrow(inner) => (**inner).clone(),
                    _ => obj_ty.clone(),
                };
                // ADR 0029: `v.to_base` (no-parens form). Same as the
                // parenthesized FunctionCall form above.
                if field == "to_base" {
                    if let Type::Distinct { base, .. } = &obj_ty {
                        let base_ty = (**base).clone();
                        return TypedIRValue::Cast {
                            value: Box::new(obj),
                            target_type: base_ty,
                        };
                    }
                }

                // ADR 0030: `v.to_ordinal` (no-parens form).
                if field == "to_ordinal" {
                    if let Type::Enum { .. } = &obj_ty {
                        return TypedIRValue::Cast {
                            value: Box::new(obj),
                            target_type: Type::Int,
                        };
                    }
                }

                // ADR 0031: `v.to_base` (no-parens form) for subranges.
                if field == "to_base" {
                    if let Type::Subrange { base, .. } = &obj_ty {
                        let base_ty = (**base).clone();
                        return TypedIRValue::Cast {
                            value: Box::new(obj),
                            target_type: base_ty,
                        };
                    }
                }

                // ─── Map zero-arg methods (ADR 0027) ───
                // `m.length`, `m.keys`, `m.values`. The argument-taking
                // methods reject this form in the analyzer; only the three
                // nullary ones reach here.
                if let Type::Map(..) = &obj_ty {
                    let callee = format!("Map.{}", field);
                    let return_type = self.type_of_expr(expr).unwrap_or(Type::Unknown);
                    return TypedIRValue::Call {
                        function: callee,
                        args: vec![obj],
                        return_type,
                    };
                }

                // `p.x` on a record is a field read. `s.length` on a
                // String/List is the zero-argument method-call form.
                // The analyzer resolved both to the same result type;
                // here we choose the IR shape.
                if !matches!(obj_ty, Type::Record(..) | Type::Unknown) {
                    if let Some(base) =
                        crate::semantics::analyzer::SemanticAnalyzer::base_type_name(&obj_ty)
                    {
                        let callee = format!("{}.{}", base, field);
                        let return_type = self.type_of_expr(expr).unwrap_or(Type::Unknown);
                        return TypedIRValue::Call {
                            function: callee,
                            args: vec![obj],
                            return_type,
                        };
                    }
                }

                // The analyzer already inferred the field's type — read it
                // back rather than re-deriving it from a record table.
                let field_type = match self.type_of_expr(expr) {
                    Some(t) => t,
                    None => match &obj_ty {
                        Type::Record(..) | Type::Unknown => Type::Unknown,
                        other => {
                            self.diagnostics.push(format!(
                                "Field access '.{}' on non-record type {:?}",
                                field, other
                            ));
                            Type::Unknown
                        }
                    },
                };

                TypedIRValue::FieldAccess {
                    object: Box::new(obj),
                    field: field.clone(),
                    field_type,
                }
            }
            ExprKind::MapLiteral {
                key_type: key_syntax,
                value_type: value_syntax,
                entries,
                ..
            } => {
                let mut translated = Vec::with_capacity(entries.len());
                for (k, v) in entries {
                    let kv = self.translate_expr(program, func, current_block, k);
                    let vv = self.translate_expr(program, func, current_block, v);
                    translated.push((kv, vv));
                }

                // The analyzer already produced a Map<K, V> type and stored
                // it in the type table. If for any reason it didn't, fall
                // back to the declared type args (or Unknown for the
                // inferred form).
                let map_type = self.type_of_expr(expr).unwrap_or_else(|| {
                    let kt = key_syntax
                        .as_ref()
                        .map(|s| self.resolve_type_syntax(s))
                        .unwrap_or(Type::Unknown);
                    let vt = value_syntax
                        .as_ref()
                        .map(|s| self.resolve_type_syntax(s))
                        .unwrap_or(Type::Unknown);
                    Type::map(kt, vt)
                });
                let (key_type, value_type) = match &map_type {
                    Type::Map(k, v) => ((**k).clone(), (**v).clone()),
                    _ => (Type::Unknown, Type::Unknown),
                };

                TypedIRValue::Map {
                    key_type,
                    value_type,
                    entries: translated,
                    map_type,
                }
            }
            ExprKind::SetLiteral {
                element_type: element_syntax,
                elements,
                ..
            } => {
                // ADR 0032 A5b: constant set literal. The parser
                // strips the outer `Set<...>`, so `element_syntax`
                // is just the inner `T` — resolve it via the
                // builder's resolver (which knows about user-declared
                // enums, subranges, etc.).
                let element_type = self.resolve_type_syntax(element_syntax);

                let mut translated = Vec::with_capacity(elements.len());
                for e in elements {
                    translated.push(self.translate_expr(program, func, current_block, e));
                }

                // Split elements into constant and non-constant.
                // Constants fold into a base bitmask; non-constants
                // each become a SetSingleton that the backend lowers
                // to `1 << bit` at runtime.
                let low: i64 = match &element_type {
                    Type::Subrange { low, .. } => *low,
                    _ => 0,
                };

                let mut constant_bits: u64 = 0;
                let mut non_constant: Vec<TypedIRValue> = Vec::new();
                for elem in &translated {
                    match Self::extract_set_element_ordinal(elem) {
                        Some(ord) => {
                            let offset = ord - low;
                            if (0..64).contains(&offset) {
                                constant_bits |= 1u64 << offset;
                            } else {
                                non_constant.push(elem.clone());
                            }
                        }
                        None => non_constant.push(elem.clone()),
                    }
                }

                if non_constant.is_empty() {
                    TypedIRValue::Set {
                        bits: constant_bits,
                        element_type,
                    }
                } else {
                    let result_type = Type::set(element_type.clone());
                    let mut acc = TypedIRValue::Set {
                        bits: constant_bits,
                        element_type: element_type.clone(),
                    };
                    for elem in non_constant {
                        let singleton = TypedIRValue::SetSingleton {
                            element: Box::new(elem),
                            element_type: element_type.clone(),
                        };
                        acc = TypedIRValue::BinaryOp {
                            op: SemanticBinOp::SetUnion,
                            left: Box::new(acc),
                            right: Box::new(singleton),
                            result_type: result_type.clone(),
                        };
                    }
                    acc
                }
            }
        }
    }

    // Lower `a and b` / `a or b` to a proper short-circuit CFG.
    //
    // Pattern (for `and`):
    //
    //     current_block:
    //         left_val := eval(a)
    //         Branch left_val -> eval_right, short
    //     eval_right:
    //         right_val := eval(b)
    //         result_var := right_val
    //         Jump merge
    //     short:
    //         result_var := false
    //         Jump merge
    //     merge:
    //         (result_var holds `a and b`)
    //
    // `or` swaps the two branches, with the short-circuit constant
    // being `true`.
    //
    // The pattern handles nested short-circuits correctly by consulting
    // `pending_merge` after translating each operand — the same way
    // `Stmt::VarDecl` handles a branching initializer.
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
        func.blocks.push(SemanticBlock {
            id: eval_right_id,
            instructions: Vec::new(),
            terminator: None,
        });
        func.blocks.push(SemanticBlock {
            id: short_id,
            instructions: Vec::new(),
            terminator: None,
        });
        func.blocks.push(SemanticBlock {
            id: merge_id,
            instructions: Vec::new(),
            terminator: None,
        });

        // `and`: left=true → evaluate right; left=false → short-circuit
        // `or`:  left=true → short-circuit (result=true); left=false → evaluate right
        let (then_blk, else_blk) = match op {
            BinOp::And => (eval_right_id, short_id),
            BinOp::Or => (short_id, eval_right_id),
            _ => unreachable!("translate_short_circuit called with non-And/Or op"),
        };

        self.safe_set_terminator(
            func,
            branch_block,
            Terminator::Branch {
                condition: left_val,
                then_block: then_blk,
                else_block: else_blk,
            },
        );

        // Right branch: evaluate the right operand, then assign its
        // result. A nested short-circuit inside `right` sets
        // pending_merge to its own merge block, so we must use that
        // as the block to append the Assign to.
        let right_val = self.translate_expr(program, func, eval_right_id, right);
        let right_end = self.pending_merge.take().unwrap_or(eval_right_id);
        self.safe_push_instruction(
            func,
            right_end,
            SemanticInstruction::Assign {
                target: result_var.clone(),
                value: right_val,
            },
        );
        self.safe_set_terminator(func, right_end, Terminator::Jump { block: merge_id });

        // Short branch: assign the short-circuit constant.
        let short_const = match op {
            BinOp::And => TypedIRValue::Bool(false),
            BinOp::Or => TypedIRValue::Bool(true),
            _ => unreachable!(),
        };
        self.safe_push_instruction(
            func,
            short_id,
            SemanticInstruction::Assign {
                target: result_var.clone(),
                value: short_const,
            },
        );
        self.safe_set_terminator(func, short_id, Terminator::Jump { block: merge_id });

        self.pending_merge = Some(merge_id);
        TypedIRValue::Variable(result_var, Type::Bool)
    }
}
