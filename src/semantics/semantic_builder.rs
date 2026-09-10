#![allow(unused_variables)]
#![allow(unused_mut)]
#![allow(unused_imports)]
#![allow(unused_assignments)]

// src/semantics/semantic_builder.rs

use crate::common::span::Span;
use crate::common::types::Type;
use crate::frontend::ast::Pattern;
use crate::frontend::ast::{BinOp, Expr, FunctionDecl, MatchCaseExpr, Stmt};
use crate::ir::semantic_ir::{
    Instruction, SemanticBinOp, SemanticBlock, SemanticFunction, SemanticInstruction,
    SemanticPattern, SemanticProgram, Terminator, TypedIRValue,
};
use crate::semantics::control_flow::ControlFlowTranslator;
use crate::semantics::flow_analyzer::FlowAnalyzer;
use crate::semantics::flow_result::{
    CaptureMode, DeferContext, FlowResult, LoopContext, TerminatorKind,
};
use std::collections::HashMap;

#[derive(Debug, Clone)]
pub struct VariableInfo {
    pub type_: Type,
    pub mutable: bool,
    pub capture_mode: Option<CaptureMode>,
}

pub struct SemanticIRBuilder {
    scopes: Vec<HashMap<String, VariableInfo>>,
    function_types: HashMap<String, FunctionSignature>,
    iter_counter: usize,
    pub diagnostics: Vec<String>,
    loop_stack: Vec<LoopContext>,
    defer_stack: Vec<DeferContext>,
    list_values: HashMap<String, Vec<Expr>>,
    pending_merge: Option<usize>,
    type_table: HashMap<usize, Type>,
}

#[derive(Debug, Clone)]
pub struct FunctionSignature {
    pub params: Vec<(String, Type)>,
    pub return_type: Type,
}

impl SemanticIRBuilder {
    pub fn build(
        functions: &[FunctionDecl],
        type_table: HashMap<usize, Type>,
    ) -> (SemanticProgram, Vec<String>) {
        let mut builder = SemanticIRBuilder {
            scopes: vec![HashMap::new()],
            function_types: HashMap::new(),
            iter_counter: 0,
            diagnostics: Vec::new(),
            loop_stack: Vec::new(),
            defer_stack: Vec::new(),
            list_values: HashMap::new(),
            pending_merge: None,
            type_table, // ─── UNIFY TYPES ───
        };
        let program = builder.build_impl(functions);
        (program, builder.diagnostics)
    }

    /// Base name of a type, ignoring generic arguments: `List<Float>` → `"List"`.
    fn base_type_name(ty: &Type) -> Option<&'static str> {
        match ty {
            Type::Int => Some("Int"),
            Type::Float => Some("Float"),
            Type::String => Some("String"),
            Type::Bool => Some("Bool"),
            Type::Void => Some("Void"),
            Type::List(_) => Some("List"),
            Type::Option(_) => Some("Option"),
            Type::Result { .. } => Some("Result"),
            Type::Channel(_) => Some("Channel"),
            Type::Pointer(_) => Some("Pointer"),
            Type::Borrow(_) => Some("Borrow"),
            Type::MutBorrow(_) => Some("MutBorrow"),
            Type::Ptr => Some("Ptr"),
            _ => None,
        }
    }

    /// Resolve `receiver.method` to the mangled name that actually exists
    /// in `function_types`. Tries both `Type.method` (built-ins) and
    /// `Type_method` (impl-derived names).
    fn resolve_method_call(&self, receiver_type: &Type, method_name: &str) -> Option<String> {
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

    // ─── UNIFY TYPES ─── Lookup helper.
    fn type_of_expr(&self, expr: &Expr) -> Option<&Type> {
        self.type_table.get(&(expr as *const Expr as usize))
    }

    fn push_scope(&mut self) {
        self.scopes.push(HashMap::new());
    }

    fn safe_push_instruction(
        &mut self,
        func: &mut SemanticFunction,
        block_id: usize,
        instruction: Instruction,
    ) {
        if let Some(block) = func.blocks.iter_mut().find(|b| b.id == block_id) {
            block.instructions.push(instruction);
        } else {
            self.diagnostics
                .push(format!("block {} not found", block_id));
        }
    }

    fn safe_set_terminator(
        &mut self,
        func: &mut SemanticFunction,
        block_id: usize,
        term: Terminator,
    ) {
        if let Some(block) = func.blocks.iter_mut().find(|b| b.id == block_id) {
            block.terminator = Some(term);
        } else {
            self.diagnostics
                .push(format!("block {} not found", block_id));
        }
    }

    fn pop_scope(&mut self) {
        assert!(self.scopes.len() > 1);
        self.scopes.pop();
    }

    fn declare_var(&mut self, name: &str, type_: Type, mutable: bool) {
        if let Some(scope) = self.scopes.last_mut() {
            if scope.contains_key(name) {
                self.diagnostics.push(format!(
                    "Variable '{}' is already declared in this scope",
                    name
                ));
            } else {
                scope.insert(
                    name.to_string(),
                    VariableInfo {
                        type_,
                        mutable,
                        capture_mode: None,
                    },
                );
            }
        }
    }

    fn lookup_var(&self, name: &str) -> Option<&VariableInfo> {
        for scope in self.scopes.iter().rev() {
            if let Some(info) = scope.get(name) {
                return Some(info);
            }
        }
        None
    }

    fn allocate_result_var(&mut self, func: &mut SemanticFunction, current_block: usize, type_hint: Type) -> String {
        let name = format!("__result_{}", self.iter_counter);
        self.iter_counter += 1;
        // Declare the variable in the current scope (and in the IR)
        self.declare_var(&name, type_hint.clone(), true);
        let _ = self.safe_push_instruction(
            func,
            current_block,
            SemanticInstruction::Declare {
                name: name.clone(),
                mutable: true,
                type_: type_hint,
                value: TypedIRValue::Void,
            },
        );
        name
    }

    #[allow(dead_code)]
    fn stmt_has_complex_cf(stmt: &Stmt) -> bool {
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
    fn expr_has_complex_cf(expr: &Expr) -> bool {
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

    fn build_impl(&mut self, functions: &[FunctionDecl]) -> SemanticProgram {
        let mut program = SemanticProgram::new();

        // Register Math functions
        self.function_types.insert(
            "Math.sqrt".to_string(),
            FunctionSignature {
                params: vec![("x".to_string(), Type::Float)],
                return_type: Type::Float,
            },
        );
        self.function_types.insert(
            "Math.pow".to_string(),
            FunctionSignature {
                params: vec![
                    ("x".to_string(), Type::Float),
                    ("y".to_string(), Type::Float),
                ],
                return_type: Type::Float,
            },
        );
        self.function_types.insert(
            "Math.sin".to_string(),
            FunctionSignature {
                params: vec![("x".to_string(), Type::Float)],
                return_type: Type::Float,
            },
        );
        self.function_types.insert(
            "Math.cos".to_string(),
            FunctionSignature {
                params: vec![("x".to_string(), Type::Float)],
                return_type: Type::Float,
            },
        );
        self.function_types.insert(
            "Math.abs".to_string(),
            FunctionSignature {
                params: vec![("x".to_string(), Type::Float)],
                return_type: Type::Float,
            },
        );
        self.function_types.insert(
            "Math.floor".to_string(),
            FunctionSignature {
                params: vec![("x".to_string(), Type::Float)],
                return_type: Type::Float,
            },
        );
        self.function_types.insert(
            "Math.ceil".to_string(),
            FunctionSignature {
                params: vec![("x".to_string(), Type::Float)],
                return_type: Type::Float,
            },
        );
        self.function_types.insert(
            "Math.exp".to_string(),
            FunctionSignature {
                params: vec![("x".to_string(), Type::Float)],
                return_type: Type::Float,
            },
        );
        self.function_types.insert(
            "Math.log".to_string(),
            FunctionSignature {
                params: vec![("x".to_string(), Type::Float)],
                return_type: Type::Float,
            },
        );
        self.function_types.insert(
            "Math.tan".to_string(),
            FunctionSignature {
                params: vec![("x".to_string(), Type::Float)],
                return_type: Type::Float,
            },
        );

        // Register String functions
        self.function_types.insert(
            "String.length".to_string(),
            FunctionSignature {
                params: vec![("s".to_string(), Type::String)],
                return_type: Type::Int,
            },
        );
        self.function_types.insert(
            "String.concat".to_string(),
            FunctionSignature {
                params: vec![
                    ("s1".to_string(), Type::String),
                    ("s2".to_string(), Type::String),
                ],
                return_type: Type::String,
            },
        );
        self.function_types.insert(
            "String.substring".to_string(),
            FunctionSignature {
                params: vec![
                    ("s".to_string(), Type::String),
                    ("start".to_string(), Type::Int),
                    ("length".to_string(), Type::Int),
                ],
                return_type: Type::String,
            },
        );
        self.function_types.insert(
            "String.to_upper".to_string(),
            FunctionSignature {
                params: vec![("s".to_string(), Type::String)],
                return_type: Type::String,
            },
        );
        self.function_types.insert(
            "String.to_lower".to_string(),
            FunctionSignature {
                params: vec![("s".to_string(), Type::String)],
                return_type: Type::String,
            },
        );

        // Register File functions
        self.function_types.insert(
            "File.read".to_string(),
            FunctionSignature {
                params: vec![("path".to_string(), Type::String)],
                return_type: Type::String,
            },
        );

        // Register Raw memory functions
        self.function_types.insert(
            "alloc".to_string(),
            FunctionSignature {
                params: vec![("size".to_string(), Type::Int)],
                return_type: Type::Pointer(Box::new(Type::Unknown)),
            },
        );
        self.function_types.insert(
            "free".to_string(),
            FunctionSignature {
                params: vec![("ptr".to_string(), Type::Pointer(Box::new(Type::Unknown)))],
                return_type: Type::Void,
            },
        );

        // Register List functions
        self.function_types.insert(
            "List.length".to_string(),
            FunctionSignature {
                params: vec![("arr".to_string(), Type::List(Box::new(Type::Float)))],
                return_type: Type::Int,
            },
        );
        self.function_types.insert(
            "List.sum".to_string(),
            FunctionSignature {
                params: vec![("arr".to_string(), Type::List(Box::new(Type::Float)))],
                return_type: Type::Float,
            },
        );
        self.function_types.insert(
            "List.max".to_string(),
            FunctionSignature {
                params: vec![("arr".to_string(), Type::List(Box::new(Type::Float)))],
                return_type: Type::Float,
            },
        );
        self.function_types.insert(
            "List.min".to_string(),
            FunctionSignature {
                params: vec![("arr".to_string(), Type::List(Box::new(Type::Float)))],
                return_type: Type::Float,
            },
        );
        self.function_types.insert(
            "File.write".to_string(),
            FunctionSignature {
                params: vec![
                    ("path".to_string(), Type::String),
                    ("content".to_string(), Type::String),
                ],
                return_type: Type::Int,
            },
        );
        self.function_types.insert(
            "File.append".to_string(),
            FunctionSignature {
                params: vec![
                    ("path".to_string(), Type::String),
                    ("content".to_string(), Type::String),
                ],
                return_type: Type::Int,
            },
        );

        // Register user-defined functions (including extern)
        for func in functions {
            let return_type = func
                .return_type
                .as_ref()
                .map(|t| Type::from_str(&t.to_string_rep()))
                .unwrap_or(Type::Void);
            let params = func
                .params
                .iter()
                .map(|(n, t)| {
                    let type_ = match t {
                        Some(s) => Type::from_str(&s.to_string_rep()),
                        None => Type::Unknown,
                    };
                    (n.clone(), type_)
                })
                .collect();

            self.function_types.insert(
                func.name.clone(),
                FunctionSignature {
                    params,
                    return_type,
                },
            );
        }

        for func in functions {
            self.push_scope();
            for (name, type_str) in &func.params {
                let param_type = match type_str {
                    Some(s) => Type::from_str(&s.to_string_rep()),
                    None => Type::Unknown,
                };
                if self.scopes.last().is_some_and(|s| s.contains_key(name)) {
                    self.diagnostics.push(format!(
                        "Duplicate parameter '{}' in function '{}'",
                        name, func.name
                    ));
                } else {
                    self.declare_var(name, param_type, false);
                }
            }

            let entry_id = program.new_block_id();
            let mut semantic_func = SemanticFunction {
                name: func.name.clone(),
                params: func
                    .params
                    .iter()
                    .map(|(n, t)| {
                        let type_ = match t {
                            Some(s) => Type::from_str(&s.to_string_rep()),
                            None => Type::Unknown,
                        };
                        (n.clone(), type_)
                    })
                    .collect(),
                return_type: func
                    .return_type
                    .as_ref()
                    .map(|t| Type::from_str(&t.to_string_rep()))
                    .unwrap_or(Type::Void),
                blocks: vec![SemanticBlock {
                    id: entry_id,
                    instructions: Vec::new(),
                    terminator: None,
                }],
                entry_block: entry_id,
                is_extern: func.is_extern,
            };

            let flow = self.translate_block(&mut program, &mut semantic_func, entry_id, &func.body);

            match flow {
                FlowResult::Reachable(final_id) => {
                    if let Some(b) = semantic_func.blocks.iter_mut().find(|b| b.id == final_id) {
                        if b.terminator.is_none() {
                            b.terminator = Some(Terminator::Return {
                                value: None,
                                type_: Type::Void,
                            });
                        }
                    }
                }
                FlowResult::Unreachable => {}
            }

            for block in &mut semantic_func.blocks {
                if block.terminator.is_none() && semantic_func.return_type == Type::Void {
                    block.terminator = Some(Terminator::Return {
                        value: None,
                        type_: Type::Void,
                    });
                }
            }

            let is_impl_method = func
                .params
                .first()
                .map(|(name, _)| name == "self")
                .unwrap_or(false);

            if !func.is_extern
                && !is_impl_method
                && semantic_func.return_type != Type::Void
                && flow.is_reachable()
            {
                self.diagnostics.push(format!(
                    "Function '{}' may reach end without returning a value",
                    func.name
                ));
            }

            self.pop_scope();
            program.functions.push(semantic_func);
        }

        program
    }

    fn is_terminated(block: &SemanticBlock) -> bool {
        FlowAnalyzer::is_terminated(block)
    }

    fn translate_block(
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
                    let then_stmts = match then_branch.as_ref() {
                        Expr::Block { statements, .. } => statements.clone(),
                        _ => vec![],
                    };
                    let else_stmts = else_branch.as_ref().map(|e| match e.as_ref() {
                        Expr::Block { statements, .. } => statements.clone(),
                        _ => vec![],
                    });
                    self.translate_if(
                        program,
                        func,
                        current_block,
                        condition,
                        &then_stmts,
                        else_stmts.as_deref(),
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

    fn translate_if(
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

        let then_id = program.new_block_id();
        let else_id = program.new_block_id();

        let _ = self.safe_set_terminator(
            func,
            current_block,
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

    #[allow(dead_code)]
    fn translate_while(
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

    fn translate_while_expr(
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

        let _ = self.safe_set_terminator(
            func,
            cond_id,
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

    #[allow(dead_code)]
    fn translate_for(
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

    fn translate_for_expr(
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

    #[allow(dead_code)]
    fn translate_match(
        &mut self,
        program: &mut SemanticProgram,
        func: &mut SemanticFunction,
        current_block: usize,
        value: &Expr,
        cases: &[MatchCaseExpr],
    ) -> FlowResult {
        let typed_value = self.translate_expr(program, func, current_block, value);
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
            current_block,
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

    #[allow(dead_code)]
    fn compile_pattern_match(&self, pattern: &Pattern, value: &TypedIRValue) -> bool {
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
    fn evaluate_guard(&self, _condition: &Expr) -> bool {
        true
    }

    fn translate_spawn(
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

    fn translate_parallel(
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

    fn translate_defer(
        &mut self,
        program: &mut SemanticProgram,
        func: &mut SemanticFunction,
        current_block: usize,
        stmt: &Stmt,
    ) -> FlowResult {
        let cleanup_id = program.new_block_id();

        func.blocks.push(SemanticBlock {
            id: cleanup_id,
            instructions: Vec::new(),
            terminator: None,
        });
        self.push_scope();
        let _cleanup_flow =
            self.translate_block(program, func, cleanup_id, std::slice::from_ref(stmt));
        self.pop_scope();

        if let Some(defer_ctx) = self.defer_stack.last_mut() {
            defer_ctx.cleanup_blocks.push(cleanup_id);
        } else {
            let mut defer_ctx = DeferContext::default();
            defer_ctx.cleanup_blocks.push(cleanup_id);
            self.defer_stack.push(defer_ctx);
        }

        let _ = self.safe_set_terminator(
            func,
            current_block,
            Terminator::Defer {
                cleanup_block: cleanup_id,
            },
        );

        FlowResult::Reachable(current_block)
    }

    fn coerce_value(&self, value: TypedIRValue, target: &Type) -> TypedIRValue {
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

    fn translate_simple_stmt(
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
            let then_stmts = match then_branch.as_ref() {
                Expr::Block { statements, .. } => statements.clone(),
                _ => vec![Stmt::Expression((**then_branch).clone())],
            };
            let else_stmts = else_branch.as_ref().map(|e| match e.as_ref() {
                Expr::Block { statements, .. } => statements.clone(),
                _ => vec![Stmt::Expression((**e).clone())],
            });
            return self.translate_if(
                program,
                func,
                current_block,
                condition,
                &then_stmts,
                else_stmts.as_deref(),
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

    fn translate_for_expr_with_target(
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

    fn translate_while_expr_with_target(
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

        let _ = self.safe_set_terminator(
            func,
            cond_id,
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

    fn translate_block_with_result(
        &mut self,
        program: &mut SemanticProgram,
        func: &mut SemanticFunction,
        start_block: usize,
        statements: &[Stmt],
        trailing_expr: Option<&Expr>,
        target: &str,
        target_type: Type,
    ) -> Option<usize> {
        // Translate the statements
        let flow = self.translate_block(program, func, start_block, statements);
        let final_block = match flow {
            FlowResult::Reachable(id) => id,
            FlowResult::Unreachable => return None,
        };

        // If there is a trailing expression, translate it and assign to target
        if let Some(expr) = trailing_expr {
            let value = self.translate_expr(program, func, final_block, expr);
            // Optionally coerce the value to the target type
            let coerced = self.coerce_value(value, &target_type);
            let _ = self.safe_push_instruction(
                func,
                final_block,
                SemanticInstruction::Assign {
                    target: target.to_string(),
                    value: coerced,
                },
            );
        } else {
            // No trailing expression: assign a default value (Void or a default of target_type)
            // For now, we assign Void; this may be insufficient for types that need a value.
            let _ = self.safe_push_instruction(
                func,
                final_block,
                SemanticInstruction::Assign {
                    target: target.to_string(),
                    value: TypedIRValue::Void,
                },
            );
        }

        Some(final_block)
    }

    fn block_is_terminated(&self, func: &SemanticFunction, id: usize) -> bool {
        func.blocks
            .iter()
            .find(|b| b.id == id)
            .map_or(false, |b| Self::is_terminated(b))
    }

    fn translate_if_with_target(
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

        let then_id = program.new_block_id();
        let else_id = program.new_block_id();
        let merge_id = program.new_block_id();

        // Create all blocks before setting terminators
        func.blocks.push(SemanticBlock { id: then_id, instructions: Vec::new(), terminator: None });
        func.blocks.push(SemanticBlock { id: else_id, instructions: Vec::new(), terminator: None });
        func.blocks.push(SemanticBlock { id: merge_id, instructions: Vec::new(), terminator: None });

        // Set the branch from current block
        let _ = self.safe_set_terminator(
            func,
            current_block,
            Terminator::Branch {
                condition: cond,
                then_block: then_id,
                else_block: else_id,
            },
        );

        // Translate then branch
        self.push_scope();
        let then_final = {
            // Extract statements and trailing expression from the branch
            let (stmts, trailing) = match then_branch {
                Expr::Block { statements, trailing_expr } => (statements.clone(), trailing_expr.as_deref()),
                other => (vec![Stmt::Expression(other.clone())], None),
            };
            self.translate_block_with_result(
                program, func, then_id, &stmts, trailing,
                target, target_type.clone()
            )
        };
        self.pop_scope();

        if let Some(id) = then_final {
            if !self.block_is_terminated(func, id) {
                let _ = self.safe_set_terminator(func, id, Terminator::Jump { block: merge_id });
            }
        }

        // Translate else branch (if present)
        if let Some(else_expr) = else_branch {
            self.push_scope();
            let else_final = {
                let (stmts, trailing) = match else_expr {
                    Expr::Block { statements, trailing_expr } => (statements.clone(), trailing_expr.as_deref()),
                    other => (vec![Stmt::Expression(other.clone())], None),
                };
                self.translate_block_with_result(
                    program, func, else_id, &stmts, trailing,
                    target, target_type.clone()
                )
            };
            self.pop_scope();

            if let Some(id) = else_final {
                if !self.block_is_terminated(func, id) {
                    let _ = self.safe_set_terminator(func, id, Terminator::Jump { block: merge_id });
                }
            }
        } else {
            // No else branch: jump directly to merge
            let _ = self.safe_set_terminator(func, else_id, Terminator::Jump { block: merge_id });
        }

        self.pending_merge = Some(merge_id);
        FlowResult::Reachable(merge_id)
    }

    fn translate_expr(
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
                    crate::frontend::ast::UnaryOp::Negate => TypedIRValue::BinaryOp {
                        op: SemanticBinOp::Subtract,
                        left: Box::new(TypedIRValue::Int(0)),
                        right: Box::new(inner),
                        result_type: inner_type,
                    },
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
            Expr::Binary { left, op, right } => {
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
                    BinOp::And => SemanticBinOp::And,
                    BinOp::Or => SemanticBinOp::Or,
                };
                TypedIRValue::BinaryOp {
                    op: semantic_op,
                    left: Box::new(cast_l),
                    right: Box::new(cast_r),
                    result_type,
                }
            }
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
            Expr::None => TypedIRValue::None {
                option_type: Type::option(Type::Void),
            },
            Expr::Ok { value } => {
                let inner = self.translate_expr(program, func, current_block, value);
                TypedIRValue::Ok {
                    value: Box::new(inner),
                    result_type: Type::result(Type::Void, Type::Void),
                }
            }
            Expr::Error { value } => {
                let inner = self.translate_expr(program, func, current_block, value);
                TypedIRValue::Error {
                    value: Box::new(inner),
                    result_type: Type::result(Type::Void, Type::Void),
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
                // ─── UNIFY TYPES ───
                let result_type = self.type_of_expr(expr)
                    .cloned()
                    .unwrap_or(Type::Unknown);

                // Allocate result variable
                let result_var = self.allocate_result_var(func, current_block, result_type.clone());

                // Create blocks
                let try_block_id = program.new_block_id();
                let catch_block_id = program.new_block_id();
                let merge_id = program.new_block_id();

                func.blocks.push(SemanticBlock { id: try_block_id, instructions: Vec::new(), terminator: None });
                func.blocks.push(SemanticBlock { id: catch_block_id, instructions: Vec::new(), terminator: None });
                func.blocks.push(SemanticBlock { id: merge_id, instructions: Vec::new(), terminator: None });

                // Set jump from current block to try
                let _ = self.safe_set_terminator(
                    func,
                    current_block,
                    Terminator::Jump { block: try_block_id },
                );

                // Translate try branch
                self.push_scope();
                let try_stmts = match try_branch.as_ref() {
                    Expr::Block { statements, .. } => statements.clone(),
                    other => vec![Stmt::Expression((*other).clone())],
                };
                let try_trailing = match try_branch.as_ref() {
                    Expr::Block { trailing_expr, .. } => trailing_expr.as_deref(),
                    _ => None,
                };
                if let Some(final_block) = self.translate_block_with_result(
                    program,
                    func,
                    try_block_id,
                    &try_stmts,
                    try_trailing,
                    &result_var,
                    result_type.clone(),
                ){
                    self.block_is_terminated(func, final_block);
                    let _ = self.safe_set_terminator(
                        func,
                        final_block,
                        Terminator::Jump { block: merge_id },
                    );
                }
                self.pop_scope();

                // Translate catch branch
                self.push_scope();
                if let Some(var_name) = catch_var {
                    self.declare_var(var_name, Type::Unknown, false);
                }
                let catch_stmts = match catch_branch.as_ref() {
                    Expr::Block { statements, .. } => statements.clone(),
                    other => vec![Stmt::Expression((*other).clone())],
                };
                let catch_trailing = match catch_branch.as_ref() {
                    Expr::Block { trailing_expr, .. } => trailing_expr.as_deref(),
                    _ => None,
                };
                if let Some(final_block) = self.translate_block_with_result(
                    program,
                    func,
                    catch_block_id,
                    &catch_stmts,
                    catch_trailing,
                    &result_var,
                    result_type.clone(),
                ){
                    self.block_is_terminated(func, final_block);
                    let _ = self.safe_set_terminator(
                        func,
                        final_block,
                        Terminator::Jump { block: merge_id },
                    );
                }
                self.pop_scope();

                // Handle finally block (if present)
                // For simplicity, we translate the finally body in the merge block,
                // but note that finally should execute before leaving try/catch.
                // A full implementation would insert finally before jumps to merge.
                if let Some(finally_stmts) = finally_body {
                    self.push_scope();
                    let finally_flow = self.translate_block(program, func, merge_id, finally_stmts);
                    self.pop_scope();
                    // If the finally block is reachable, it may create new blocks;
                    // adjust pending_merge accordingly.
                    match finally_flow {
                        FlowResult::Reachable(id) => self.pending_merge = Some(id),
                        FlowResult::Unreachable => self.pending_merge = Some(merge_id),
                    }
                } else {
                    self.pending_merge = Some(merge_id);
                }

                // Return the result variable
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
}
