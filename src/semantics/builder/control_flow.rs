// src/semantics/semantic_builder/control_flow.rs

use super::*;

impl SemanticIRBuilder {
    pub(super) fn translate_block(
        &mut self,
        program: &mut SemanticProgram,
        func: &mut SemanticFunction,
        block_id: usize,
        statements: &[Stmt],
    ) -> FlowResult {
        let mut current_flow = FlowResult::Reachable(block_id);

        for stmt in statements {
            let current_block = match current_flow {
                FlowResult::Reachable(id) => id,
                FlowResult::Unreachable => break,
            };

            if let Some(block) = func.blocks.iter().find(|b| b.id == current_block) {
                if Self::is_terminated(block) {
                    current_flow = FlowResult::Unreachable;
                    break;
                }
            }

            current_flow = match stmt {
                Stmt::Expression(Expr::If {
                    condition,
                    then_branch,
                    else_branch,
                }) => {
                    // Borrow branch statements directly from the AST.
                    // Cloning them would allocate new nodes whose
                    // addresses don't match the analyzer's type-table
                    // keys, silently breaking type lookups inside.
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
                    self.translate_if(
                        program,
                        func,
                        current_block,
                        condition,
                        then_stmts,
                        else_stmts,
                    )
                }
                Stmt::Expression(Expr::For {
                    var,
                    iterable,
                    body,
                    trailing_expr,
                    ..
                }) => {
                    let _val = self.translate_for_expr(
                        program,
                        func,
                        current_block,
                        var,
                        iterable,
                        body,
                        trailing_expr,
                    );
                    if let Some(merge) = self.pending_merge.take() {
                        FlowResult::Reachable(merge)
                    } else {
                        FlowResult::Reachable(current_block)
                    }
                }
                Stmt::Expression(Expr::While {
                    condition,
                    body,
                    trailing_expr,
                    ..
                }) => {
                    let _val = self.translate_while_expr(
                        program,
                        func,
                        current_block,
                        condition,
                        body,
                        trailing_expr,
                    );
                    if let Some(merge) = self.pending_merge.take() {
                        FlowResult::Reachable(merge)
                    } else {
                        FlowResult::Reachable(current_block)
                    }
                }
                Stmt::Spawn { body } => self.translate_spawn(program, func, current_block, body),
                Stmt::Parallel { blocks } => {
                    self.translate_parallel(program, func, current_block, blocks)
                }
                Stmt::Defer { stmt } => self.translate_defer(program, func, current_block, stmt),
                Stmt::RegionBlock { name: _, body } => {
                    self.push_scope();
                    let flow = self.translate_block(program, func, current_block, body);
                    self.pop_scope();
                    flow
                }
                Stmt::UnsafeBlock { body } => {
                    self.push_scope();
                    let flow = self.translate_block(program, func, current_block, body);
                    self.pop_scope();
                    flow
                }
                Stmt::Break => {
                    if let Some(loop_ctx) = self.loop_stack.last().copied() {
                        let _ = self.safe_set_terminator(
                            func,
                            current_block,
                            Terminator::Jump {
                                block: loop_ctx.break_block,
                            },
                        );
                        FlowResult::Unreachable
                    } else {
                        self.diagnostics.push("Break outside of loop".to_string());
                        FlowResult::Reachable(current_block)
                    }
                }
                Stmt::Continue => {
                    if let Some(loop_ctx) = self.loop_stack.last().copied() {
                        let _ = self.safe_set_terminator(
                            func,
                            current_block,
                            Terminator::Jump {
                                block: loop_ctx.continue_block,
                            },
                        );
                        FlowResult::Unreachable
                    } else {
                        self.diagnostics
                            .push("Continue outside of loop".to_string());
                        FlowResult::Reachable(current_block)
                    }
                }
                _ => self.translate_simple_stmt(program, func, current_block, stmt),
            };
        }

        current_flow
    }
    pub(super) fn translate_if(
        &mut self,
        program: &mut SemanticProgram,
        func: &mut SemanticFunction,
        current_block: usize,
        condition: &Expr,
        then_body: &[Stmt],
        else_body: Option<&[Stmt]>,
    ) -> FlowResult {
        let cond = self.translate_expr(program, func, current_block, condition);
        let cond_type = cond.type_of();
        if cond_type != Type::Bool && cond_type != Type::Unknown {
            self.diagnostics.push(format!(
                "If condition type mismatch: expected Bool, found {:?}",
                cond_type
            ));
        }

        // See the comment in translate_if_with_target: the condition
        // may have branched (short-circuit and/or, or a nested
        // value-producing if), so the outer Branch attaches to its
        // merge block, not the original current_block.
        let branch_from = self.pending_merge.take().unwrap_or(current_block);

        let then_id = program.new_block_id();
        let else_id = program.new_block_id();

        let _ = self.safe_set_terminator(
            func,
            branch_from,
            Terminator::Branch {
                condition: cond,
                then_block: then_id,
                else_block: else_id,
            },
        );

        func.blocks.push(SemanticBlock {
            id: then_id,
            instructions: Vec::new(),
            terminator: None,
        });
        self.push_scope();
        let then_flow = self.translate_block(program, func, then_id, then_body);
        self.pop_scope();

        func.blocks.push(SemanticBlock {
            id: else_id,
            instructions: Vec::new(),
            terminator: None,
        });
        let else_flow = if let Some(else_stmts) = else_body {
            self.push_scope();
            let flow = self.translate_block(program, func, else_id, else_stmts);
            self.pop_scope();
            flow
        } else {
            FlowResult::Reachable(else_id)
        };

        match (then_flow, else_flow) {
            (FlowResult::Unreachable, FlowResult::Unreachable) => FlowResult::Unreachable,
            (t_flow, e_flow) => {
                let merge_id = program.new_block_id();
                if let FlowResult::Reachable(id) = t_flow {
                    let _ =
                        self.safe_set_terminator(func, id, Terminator::Jump { block: merge_id });
                }
                if let FlowResult::Reachable(id) = e_flow {
                    let _ =
                        self.safe_set_terminator(func, id, Terminator::Jump { block: merge_id });
                }
                func.blocks.push(SemanticBlock {
                    id: merge_id,
                    instructions: Vec::new(),
                    terminator: None,
                });
                FlowResult::Reachable(merge_id)
            }
        }
    }
    pub(super) fn translate_if_with_target(
        &mut self,
        program: &mut SemanticProgram,
        func: &mut SemanticFunction,
        current_block: usize,
        condition: &Expr,
        then_branch: &Expr,
        else_branch: Option<&Expr>,
        target: &str,
        target_type: Type,
    ) -> FlowResult {
        let cond = self.translate_expr(program, func, current_block, condition);
        let cond_type = cond.type_of();
        if cond_type != Type::Bool && cond_type != Type::Unknown {
            self.diagnostics.push(format!(
                "If condition type mismatch: expected Bool, found {:?}",
                cond_type
            ));
        }

        // The condition may have introduced its own branching (e.g. a
        // short-circuit `and`/`or` lowers to a two-branch CFG with a
        // merge block). If so, the outer `if`'s Branch must attach to
        // that merge block — attaching to the original current_block
        // would land after the short-circuit's own terminator and be
        // dropped.
        let branch_from = self.pending_merge.take().unwrap_or(current_block);

        let then_id = program.new_block_id();
        let else_id = program.new_block_id();
        let merge_id = program.new_block_id();

        // Create all blocks before setting terminators.
        func.blocks.push(SemanticBlock { id: then_id, instructions: Vec::new(), terminator: None });
        func.blocks.push(SemanticBlock { id: else_id, instructions: Vec::new(), terminator: None });
        func.blocks.push(SemanticBlock { id: merge_id, instructions: Vec::new(), terminator: None });

        // Set the branch from the current block.
        let _ = self.safe_set_terminator(
            func,
            branch_from,
            Terminator::Branch {
                condition: cond,
                then_block: then_id,
                else_block: else_id,
            },
        );

        // ─── Then branch ───
        // Borrow statements directly from the AST when possible. Cloning
        // allocates new nodes whose addresses don't match the analyzer's
        // type-table keys, silently breaking type lookups inside.
        self.push_scope();
        let then_final = {
            let then_stmts: Cow<'_, [Stmt]> = match then_branch {
                Expr::Block { statements, .. } => Cow::Borrowed(statements.as_slice()),
                // Parser guarantees Block for if-branches. This fallback
                // is defensive only; the synthesized node's type-table
                // entry will not exist, but the parser prevents this path.
                other => Cow::Owned(vec![Stmt::Expression(other.clone())]),
            };
            let then_trailing: Option<&Expr> = match then_branch {
                Expr::Block { trailing_expr, .. } => trailing_expr.as_deref(),
                _ => None,
            };
            self.translate_block_with_result(
                program,
                func,
                then_id,
                then_stmts.as_ref(),
                then_trailing,
                target,
                target_type.clone(),
            )
        };
        self.pop_scope();

        if let Some(id) = then_final {
            if !self.block_is_terminated(func, id) {
                let _ = self.safe_set_terminator(func, id, Terminator::Jump { block: merge_id });
            }
        }

        // ─── Else branch ───
        if let Some(else_expr) = else_branch {
            self.push_scope();
            let else_final = {
                let else_stmts: Cow<'_, [Stmt]> = match else_expr {
                    Expr::Block { statements, .. } => Cow::Borrowed(statements.as_slice()),
                    other => Cow::Owned(vec![Stmt::Expression(other.clone())]),
                };
                let else_trailing: Option<&Expr> = match else_expr {
                    Expr::Block { trailing_expr, .. } => trailing_expr.as_deref(),
                    _ => None,
                };
                self.translate_block_with_result(
                    program,
                    func,
                    else_id,
                    else_stmts.as_ref(),
                    else_trailing,
                    target,
                    target_type.clone(),
                )
            };
            self.pop_scope();

            if let Some(id) = else_final {
                if !self.block_is_terminated(func, id) {
                    let _ = self.safe_set_terminator(func, id, Terminator::Jump { block: merge_id });
                }
            }
        } else {
            // No else branch: jump directly to merge.
            let _ = self.safe_set_terminator(func, else_id, Terminator::Jump { block: merge_id });
        }

        self.pending_merge = Some(merge_id);
        FlowResult::Reachable(merge_id)
    }
    #[allow(dead_code)]
    pub(super) fn translate_while(
        &mut self,
        program: &mut SemanticProgram,
        func: &mut SemanticFunction,
        current_block: usize,
        condition: &Expr,
        body: &[Stmt],
    ) -> FlowResult {
        let cond_id = program.new_block_id();
        let body_id = program.new_block_id();
        let merge_id = program.new_block_id();

        let _ = self.safe_set_terminator(func, current_block, Terminator::Jump { block: cond_id });

        func.blocks.push(SemanticBlock {
            id: cond_id,
            instructions: Vec::new(),
            terminator: None,
        });

        let cond = self.translate_expr(program, func, cond_id, condition);
        let cond_type = cond.type_of();
        if cond_type != Type::Bool && cond_type != Type::Unknown {
            self.diagnostics.push(format!(
                "While condition type mismatch: expected Bool, found {:?}",
                cond_type
            ));
        }

        let _ = self.safe_set_terminator(
            func,
            cond_id,
            Terminator::Branch {
                condition: cond,
                then_block: body_id,
                else_block: merge_id,
            },
        );

        func.blocks.push(SemanticBlock {
            id: body_id,
            instructions: Vec::new(),
            terminator: None,
        });

        self.push_scope();
        self.loop_stack.push(LoopContext {
            break_block: merge_id,
            continue_block: cond_id,
        });
        let body_flow = self.translate_block(program, func, body_id, body);
        self.loop_stack.pop();
        self.pop_scope();

        if let FlowResult::Reachable(id) = body_flow {
            if let Some(block) = func.blocks.iter_mut().find(|b| b.id == id) {
                if !Self::is_terminated(block) {
                    let _ = self.safe_set_terminator(func, id, Terminator::Jump { block: cond_id });
                }
            }
        }

        func.blocks.push(SemanticBlock {
            id: merge_id,
            instructions: Vec::new(),
            terminator: None,
        });
        FlowResult::Reachable(merge_id)
    }
    pub(super) fn translate_while_expr(
        &mut self,
        program: &mut SemanticProgram,
        func: &mut SemanticFunction,
        current_block: usize,
        condition: &Expr,
        body: &[Stmt],
        trailing_expr: &Option<Box<Expr>>,
    ) -> TypedIRValue {
        let result_name = format!("__while_result_{}", self.iter_counter);
        self.iter_counter += 1;

        let _ = self.safe_push_instruction(
            func,
            current_block,
            SemanticInstruction::Declare {
                name: result_name.clone(),
                mutable: true,
                type_: Type::Void,
                value: TypedIRValue::Void,
            },
        );

        let cond_id = program.new_block_id();
        let body_id = program.new_block_id();
        let merge_id = program.new_block_id();

        // Push ALL blocks BEFORE translating
        func.blocks.push(SemanticBlock {
            id: cond_id,
            instructions: Vec::new(),
            terminator: None,
        });
        func.blocks.push(SemanticBlock {
            id: body_id,
            instructions: Vec::new(),
            terminator: None,
        });
        func.blocks.push(SemanticBlock {
            id: merge_id,
            instructions: Vec::new(),
            terminator: None,
        });

        let _ = self.safe_set_terminator(func, current_block, Terminator::Jump { block: cond_id });

        let cond = self.translate_expr(program, func, cond_id, condition);
        let branch_from = self.pending_merge.take().unwrap_or(cond_id);

        let _ = self.safe_set_terminator(
            func,
            branch_from,
            Terminator::Branch {
                condition: cond,
                then_block: body_id,
                else_block: merge_id,
            },
        );

        self.push_scope();
        self.loop_stack.push(LoopContext {
            break_block: merge_id,
            continue_block: cond_id,
        });
        let body_flow = self.translate_block(program, func, body_id, body);

        if let FlowResult::Reachable(bid) = body_flow {
            if let Some(te) = trailing_expr {
                let te_val = self.translate_expr(program, func, bid, te);
                let _ = self.safe_push_instruction(
                    func,
                    bid,
                    SemanticInstruction::Assign {
                        target: result_name.clone(),
                        value: te_val,
                    },
                );
            }
            if let Some(block) = func.blocks.iter_mut().find(|b| b.id == bid) {
                if !Self::is_terminated(block) {
                    let _ =
                        self.safe_set_terminator(func, bid, Terminator::Jump { block: cond_id });
                }
            }
        }

        self.loop_stack.pop();
        self.pop_scope();

        self.pending_merge = Some(merge_id);
        TypedIRValue::Variable(result_name, Type::Void)
    }
    pub(super) fn translate_while_expr_with_target(
        &mut self,
        program: &mut SemanticProgram,
        func: &mut SemanticFunction,
        current_block: usize,
        condition: &Expr,
        body: &[Stmt],
        trailing_expr: &Option<Box<Expr>>,
        target_name: &str,
    ) -> TypedIRValue {
        let cond_id = program.new_block_id();
        let body_id = program.new_block_id();
        let merge_id = program.new_block_id();

        func.blocks.push(SemanticBlock {
            id: cond_id,
            instructions: Vec::new(),
            terminator: None,
        });
        func.blocks.push(SemanticBlock {
            id: body_id,
            instructions: Vec::new(),
            terminator: None,
        });
        func.blocks.push(SemanticBlock {
            id: merge_id,
            instructions: Vec::new(),
            terminator: None,
        });

        let _ = self.safe_set_terminator(func, current_block, Terminator::Jump { block: cond_id });

        let cond = self.translate_expr(program, func, cond_id, condition);
        let branch_from = self.pending_merge.take().unwrap_or(cond_id);

        let _ = self.safe_set_terminator(
            func,
            branch_from,
            Terminator::Branch {
                condition: cond,
                then_block: body_id,
                else_block: merge_id,
            },
        );

        self.push_scope();
        self.loop_stack.push(LoopContext {
            break_block: merge_id,
            continue_block: cond_id,
        });

        let body_flow = self.translate_block(program, func, body_id, body);

        self.loop_stack.pop();
        self.pop_scope();

        if let FlowResult::Reachable(id) = body_flow {
            if let Some(te) = trailing_expr {
                let te_val = self.translate_expr(program, func, id, te);
                let _ = self.safe_push_instruction(
                    func,
                    id,
                    SemanticInstruction::Assign {
                        target: target_name.to_string(),
                        value: te_val,
                    },
                );
            }

            if let Some(block) = func.blocks.iter().find(|b| b.id == id) {
                if !Self::is_terminated(block) {
                    let _ = self.safe_set_terminator(func, id, Terminator::Jump { block: cond_id });
                }
            }
        }

        self.pending_merge = Some(merge_id);
        TypedIRValue::Variable(target_name.to_string(), Type::Void)
    }
    #[allow(dead_code)]
    pub(super)fn translate_for(
        &mut self,
        program: &mut SemanticProgram,
        func: &mut SemanticFunction,
        current_block: usize,
        var: &str,
        iterable: &Expr,
        body: &[Stmt],
    ) -> FlowResult {
        let init_id = program.new_block_id();
        let cond_id = program.new_block_id();
        let body_id = program.new_block_id();
        let merge_id = program.new_block_id();

        let _ = self.safe_set_terminator(func, current_block, Terminator::Jump { block: init_id });

        func.blocks.push(SemanticBlock {
            id: init_id,
            instructions: Vec::new(),
            terminator: None,
        });

        let iterable_val = self.translate_expr(program, func, init_id, iterable);
        let elem_type = match iterable_val.type_of() {
            Type::List(elem) => *elem,
            Type::Unknown => Type::Unknown,
            other => {
                self.diagnostics.push(format!(
                    "For loop iterable type mismatch: expected List, found {:?}",
                    other
                ));
                Type::Unknown
            }
        };

        self.iter_counter += 1;
        let iter_name = format!("__iter_{}_{}", var, self.iter_counter);

        let _ = self.safe_push_instruction(
            func,
            init_id,
            Instruction::IteratorInit {
                iterator: iter_name.clone(),
                iterable: iterable_val,
            },
        );

        let _ = self.safe_set_terminator(func, init_id, Terminator::Jump { block: cond_id });

        func.blocks.push(SemanticBlock {
            id: cond_id,
            instructions: Vec::new(),
            terminator: Some(Terminator::IteratorNext {
                iterator: iter_name,
                target: var.to_string(),
                body_block: body_id,
                exit_block: merge_id,
            }),
        });

        self.push_scope();
        self.declare_var(var, elem_type, false);

        func.blocks.push(SemanticBlock {
            id: body_id,
            instructions: Vec::new(),
            terminator: None,
        });

        self.loop_stack.push(LoopContext {
            break_block: merge_id,
            continue_block: cond_id,
        });
        let body_flow = self.translate_block(program, func, body_id, body);
        self.loop_stack.pop();

        if let FlowResult::Reachable(id) = body_flow {
            if let Some(block) = func.blocks.iter_mut().find(|b| b.id == id) {
                if !Self::is_terminated(block) {
                    let _ = self.safe_set_terminator(func, id, Terminator::Jump { block: cond_id });
                }
            }
        }

        self.pop_scope();

        func.blocks.push(SemanticBlock {
            id: merge_id,
            instructions: Vec::new(),
            terminator: None,
        });
        FlowResult::Reachable(merge_id)
    }
    pub(super) fn translate_for_expr(
        &mut self,
        program: &mut SemanticProgram,
        func: &mut SemanticFunction,
        current_block: usize,
        var: &str,
        iterable: &Expr,
        body: &[Stmt],
        trailing_expr: &Option<Box<Expr>>,
    ) -> TypedIRValue {
        let result_name = format!("__for_result_{}", self.iter_counter);
        self.iter_counter += 1;

        let _ = self.safe_push_instruction(
            func,
            current_block,
            SemanticInstruction::Declare {
                name: result_name.clone(),
                mutable: true,
                type_: Type::Void,
                value: TypedIRValue::Void,
            },
        );

        let init_id = program.new_block_id();
        let cond_id = program.new_block_id();
        let body_id = program.new_block_id();
        let merge_id = program.new_block_id();

        // Push ALL blocks BEFORE referencing them
        func.blocks.push(SemanticBlock {
            id: init_id,
            instructions: Vec::new(),
            terminator: None,
        });
        func.blocks.push(SemanticBlock {
            id: cond_id,
            instructions: Vec::new(),
            terminator: None,
        });
        func.blocks.push(SemanticBlock {
            id: body_id,
            instructions: Vec::new(),
            terminator: None,
        });
        func.blocks.push(SemanticBlock {
            id: merge_id,
            instructions: Vec::new(),
            terminator: None,
        });

        let _ = self.safe_set_terminator(func, current_block, Terminator::Jump { block: init_id });

        let iterable_val = self.translate_expr(program, func, init_id, iterable);
        let elem_type = match iterable_val.type_of() {
            Type::List(e) => *e,
            _ => Type::Unknown,
        };

        self.push_scope();
        self.declare_var(var, elem_type.clone(), false);

        let iter_name = format!("__iter_{}_{}", var, self.iter_counter);
        self.iter_counter += 1;

        let _ = self.safe_push_instruction(
            func,
            init_id,
            Instruction::IteratorInit {
                iterator: iter_name.clone(),
                iterable: iterable_val,
            },
        );

        let _ = self.safe_set_terminator(func, init_id, Terminator::Jump { block: cond_id });

        let _ = self.safe_set_terminator(
            func,
            cond_id,
            Terminator::IteratorNext {
                iterator: iter_name,
                target: var.to_string(),
                body_block: body_id,
                exit_block: merge_id,
            },
        );

        self.loop_stack.push(LoopContext {
            break_block: merge_id,
            continue_block: cond_id,
        });

        let body_flow = self.translate_block(program, func, body_id, body);

        self.loop_stack.pop();
        self.pop_scope();

        if let FlowResult::Reachable(id) = body_flow {
            if let Some(te) = trailing_expr {
                let te_val = self.translate_expr(program, func, id, te);
                let _ = self.safe_push_instruction(
                    func,
                    id,
                    SemanticInstruction::Assign {
                        target: result_name.clone(),
                        value: te_val,
                    },
                );
            }

            if let Some(block) = func.blocks.iter().find(|b| b.id == id) {
                if !Self::is_terminated(block) {
                    let _ = self.safe_set_terminator(func, id, Terminator::Jump { block: cond_id });
                }
            }
        }

        self.pending_merge = Some(merge_id);
        TypedIRValue::Variable(result_name, Type::Void)
    }
    pub(super) fn translate_for_expr_with_target(
        &mut self,
        program: &mut SemanticProgram,
        func: &mut SemanticFunction,
        current_block: usize,
        var: &str,
        iterable: &Expr,
        body: &[Stmt],
        trailing_expr: &Option<Box<Expr>>,
        target_name: &str,
    ) -> TypedIRValue {
        let init_id = program.new_block_id();
        let cond_id = program.new_block_id();
        let body_id = program.new_block_id();
        let merge_id = program.new_block_id();

        func.blocks.push(SemanticBlock {
            id: init_id,
            instructions: Vec::new(),
            terminator: None,
        });
        func.blocks.push(SemanticBlock {
            id: cond_id,
            instructions: Vec::new(),
            terminator: None,
        });
        func.blocks.push(SemanticBlock {
            id: body_id,
            instructions: Vec::new(),
            terminator: None,
        });
        func.blocks.push(SemanticBlock {
            id: merge_id,
            instructions: Vec::new(),
            terminator: None,
        });

        let _ = self.safe_set_terminator(func, current_block, Terminator::Jump { block: init_id });

        let iterable_val = self.translate_expr(program, func, init_id, iterable);
        let elem_type = match iterable_val.type_of() {
            Type::List(e) => *e,
            _ => Type::Unknown,
        };

        self.push_scope();
        self.declare_var(var, elem_type.clone(), false);

        let iter_name = format!("__iter_{}_{}", var, self.iter_counter);
        self.iter_counter += 1;

        let _ = self.safe_push_instruction(
            func,
            init_id,
            Instruction::IteratorInit {
                iterator: iter_name.clone(),
                iterable: iterable_val,
            },
        );

        let _ = self.safe_set_terminator(func, init_id, Terminator::Jump { block: cond_id });

        let _ = self.safe_set_terminator(
            func,
            cond_id,
            Terminator::IteratorNext {
                iterator: iter_name,
                target: var.to_string(),
                body_block: body_id,
                exit_block: merge_id,
            },
        );

        self.loop_stack.push(LoopContext {
            break_block: merge_id,
            continue_block: cond_id,
        });

        let body_flow = self.translate_block(program, func, body_id, body);

        self.loop_stack.pop();
        self.pop_scope();

        if let FlowResult::Reachable(id) = body_flow {
            if let Some(te) = trailing_expr {
                let te_val = self.translate_expr(program, func, id, te);
                let _ = self.safe_push_instruction(
                    func,
                    id,
                    SemanticInstruction::Assign {
                        target: target_name.to_string(),
                        value: te_val,
                    },
                );
            }

            if let Some(block) = func.blocks.iter().find(|b| b.id == id) {
                if !Self::is_terminated(block) {
                    let _ = self.safe_set_terminator(func, id, Terminator::Jump { block: cond_id });
                }
            }
        }

        self.pending_merge = Some(merge_id);
        TypedIRValue::Variable(target_name.to_string(), Type::Void)
    }
    #[allow(dead_code)]
    pub(super) fn translate_match(
        &mut self,
        program: &mut SemanticProgram,
        func: &mut SemanticFunction,
        current_block: usize,
        value: &Expr,
        cases: &[MatchCaseExpr],
    ) -> FlowResult {
        let typed_value = self.translate_expr(program, func, current_block, value);
        // The match value may have branched (short-circuit and/or, or
        // a nested value-producing expression). The Switch terminator
        // attaches to the value's merge block.
        let switch_from = self.pending_merge.take().unwrap_or(current_block);
        let typed_value_for_binding = typed_value.clone();
        let merge_id = program.new_block_id();

        let mut case_triplets = Vec::new();
        for case in cases {
            let case_id = program.new_block_id();
            let pattern = match &case.pattern {
                crate::frontend::ast::Pattern::Some(v) => {
                    SemanticPattern::Some { binding: v.clone() }
                }
                crate::frontend::ast::Pattern::None => SemanticPattern::None,
                crate::frontend::ast::Pattern::Ok(v) => SemanticPattern::Ok { binding: v.clone() },
                crate::frontend::ast::Pattern::Error(v) => {
                    SemanticPattern::Error { binding: v.clone() }
                }
                crate::frontend::ast::Pattern::Wildcard => SemanticPattern::Wildcard,
                crate::frontend::ast::Pattern::Binding(_) => SemanticPattern::Wildcard,
                crate::frontend::ast::Pattern::Literal(e) => {
                    SemanticPattern::Literal(self.translate_expr(program, func, current_block, e))
                }
                _ => SemanticPattern::Wildcard,
            };
            case_triplets.push((pattern, case_id, case.body.clone()));
        }

        let switch_cases = case_triplets
            .iter()
            .map(|(pat, id, _)| (pat.clone(), *id))
            .collect();

        let default_block = Some(merge_id);

        let _ = self.safe_set_terminator(
            func,
            switch_from,
            Terminator::Switch {
                value: typed_value,
                cases: switch_cases,
                default_block,
            },
        );

        let mut all_unreachable = true;
        for (pattern_ref, case_id, body) in case_triplets.iter() {
            func.blocks.push(SemanticBlock {
                id: *case_id,
                instructions: Vec::new(),
                terminator: None,
            });
            self.push_scope();

            match pattern_ref {
                SemanticPattern::Some { binding } => {
                    self.declare_var(binding, Type::Unknown, false);
                }
                SemanticPattern::Ok { binding } => {
                    self.declare_var(binding, Type::Unknown, false);
                }
                SemanticPattern::Error { binding } => {
                    self.declare_var(binding, Type::Unknown, false);
                }
                _ => {}
            }

            let body_stmts = match &body {
                Expr::Block { statements, .. } => statements.clone(),
                other => vec![Stmt::Expression((*other).clone())],
            };
            let case_flow = self.translate_block(program, func, *case_id, &body_stmts);
            self.pop_scope();

            match case_flow {
                FlowResult::Reachable(id) => {
                    all_unreachable = false;
                    if let Some(block) = func.blocks.iter_mut().find(|b| b.id == id) {
                        if !Self::is_terminated(block) {
                            let _ = self.safe_set_terminator(
                                func,
                                id,
                                Terminator::Jump { block: merge_id },
                            );
                        }
                    }
                }
                FlowResult::Unreachable => {}
            }
        }

        func.blocks.push(SemanticBlock {
            id: merge_id,
            instructions: Vec::new(),
            terminator: None,
        });

        if all_unreachable {
            FlowResult::Unreachable
        } else {
            FlowResult::Reachable(merge_id)
        }
    }
    pub(super) fn translate_spawn(
        &mut self,
        program: &mut SemanticProgram,
        func: &mut SemanticFunction,
        current_block: usize,
        body: &[Stmt],
    ) -> FlowResult {
        let spawn_entry = program.new_block_id();
        let continuation_id = program.new_block_id();

        func.blocks.push(SemanticBlock {
            id: continuation_id,
            instructions: Vec::new(),
            terminator: None,
        });

        let _ = self.safe_set_terminator(
            func,
            current_block,
            Terminator::Spawn {
                entry_block: spawn_entry,
            },
        );

        func.blocks.push(SemanticBlock {
            id: spawn_entry,
            instructions: Vec::new(),
            terminator: None,
        });

        self.push_scope();
        let spawn_flow = self.translate_block(program, func, spawn_entry, body);
        self.pop_scope();

        if let FlowResult::Reachable(id) = spawn_flow {
            if let Some(block) = func.blocks.iter_mut().find(|b| b.id == id) {
                if !Self::is_terminated(block) {
                    let _ = self.safe_set_terminator(
                        func,
                        id,
                        Terminator::Jump {
                            block: continuation_id,
                        },
                    );
                }
            }
        }

        FlowResult::Reachable(continuation_id)
    }
    pub(super) fn translate_parallel(
        &mut self,
        program: &mut SemanticProgram,
        func: &mut SemanticFunction,
        current_block: usize,
        blocks: &[Vec<Stmt>],
    ) -> FlowResult {
        if blocks.is_empty() {
            return FlowResult::Reachable(current_block);
        }

        let merge_id = program.new_block_id();
        let mut entry_blocks = Vec::new();

        for block_stmts in blocks {
            let entry_id = program.new_block_id();
            entry_blocks.push(entry_id);

            func.blocks.push(SemanticBlock {
                id: entry_id,
                instructions: Vec::new(),
                terminator: None,
            });
            self.push_scope();
            let block_flow = self.translate_block(program, func, entry_id, block_stmts);
            self.pop_scope();

            if let FlowResult::Reachable(id) = block_flow {
                if let Some(block) = func.blocks.iter_mut().find(|b| b.id == id) {
                    if !Self::is_terminated(block) {
                        let _ = self.safe_set_terminator(
                            func,
                            id,
                            Terminator::Jump { block: merge_id },
                        );
                    }
                }
            }
        }

        let _ = self.safe_set_terminator(
            func,
            current_block,
            Terminator::Fork {
                blocks: entry_blocks,
                join_block: merge_id,
            },
        );

        func.blocks.push(SemanticBlock {
            id: merge_id,
            instructions: Vec::new(),
            terminator: None,
        });
        FlowResult::Reachable(merge_id)
    }
    pub(super) fn translate_defer(
        &mut self,
        program: &mut SemanticProgram,
        func: &mut SemanticFunction,
        current_block: usize,
        stmt: &Stmt,
    ) -> FlowResult {
        // A defer is a *pending action*, not a terminator. If we set a
        // terminator here, `translate_block` sees the block as ended
        // and drops every subsequent statement — including the eventual
        // `return`.
        //
        // Instead: build the cleanup block, push its id onto the defer
        // stack, and continue. The eventual terminator emitter (Return
        // for now; Jump/Branch in a future revision) will chain the
        // pending cleanups before emitting the real terminator.
        let cleanup_id = program.new_block_id();

        func.blocks.push(SemanticBlock {
            id: cleanup_id,
            instructions: Vec::new(),
            terminator: None,
        });
        self.push_scope();
        let cleanup_flow =
            self.translate_block(program, func, cleanup_id, std::slice::from_ref(stmt));
        self.pop_scope();

        // Ensure the cleanup block has a terminator of its own. A
        // single Return will replace it during chaining; for now, give
        // it a Jump to itself so verification doesn't reject it as
        // unterminated.
        if let FlowResult::Reachable(id) = cleanup_flow {
            if !self.block_is_terminated(func, id) {
                let _ = self.safe_set_terminator(
                    func,
                    id,
                    Terminator::Jump { block: id },
                );
            }
        }

        if let Some(defer_ctx) = self.defer_stack.last_mut() {
            defer_ctx.cleanup_blocks.push(cleanup_id);
        } else {
            let mut defer_ctx = DeferContext::default();
            defer_ctx.cleanup_blocks.push(cleanup_id);
            self.defer_stack.push(defer_ctx);
        }

        FlowResult::Reachable(current_block)
    }
}