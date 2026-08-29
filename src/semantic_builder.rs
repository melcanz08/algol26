#![allow(dead_code)]
#![allow(unused_variables)]
#![allow(unused_mut)]
#![allow(unused_imports)]
#![allow(unused_assignments)]

use crate::ast::{Expr, FunctionDecl, Stmt, BinOp, MatchCase};
use std::collections::HashMap;
use crate::semantic_type::SemanticType;
use crate::flow_result::{FlowResult, LoopContext, DeferContext, CaptureMode, TerminatorKind};
use crate::semantic_ir::{
    SemanticProgram, SemanticFunction, SemanticBlock, SemanticInstruction,
    TypedIRValue, SemanticBinOp, SemanticPattern
};
use crate::type_checker::TypeChecker;
use crate::flow_analyzer::FlowAnalyzer;
use crate::expr_translator::ExprTranslator;
use crate::control_flow::ControlFlowTranslator;

#[derive(Debug, Clone)]
pub struct VariableInfo {
    pub type_: SemanticType,
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
}

#[derive(Debug, Clone)]
struct FunctionSignature {
    params: Vec<(String, SemanticType)>,
    return_type: SemanticType,
}

impl SemanticIRBuilder {
    pub fn build(functions: &[FunctionDecl]) -> (SemanticProgram, Vec<String>) {
        let mut builder = SemanticIRBuilder {
            scopes: vec![HashMap::new()],
            function_types: HashMap::new(),
            iter_counter: 0,
            diagnostics: Vec::new(),
            loop_stack: Vec::new(),
            defer_stack: Vec::new(),
        };
        let program = builder.build_impl(functions);
        (program, builder.diagnostics)
    }
    
    fn push_scope(&mut self) { self.scopes.push(HashMap::new()); }
    fn pop_scope(&mut self) {
        assert!(self.scopes.len() > 1);
        self.scopes.pop();
    }
    
    fn declare_var(&mut self, name: &str, type_: SemanticType, mutable: bool) {
        if let Some(scope) = self.scopes.last_mut() {
            if scope.contains_key(name) {
                self.diagnostics.push(format!(
                    "Variable '{}' is already declared in this scope",
                    name
                ));
            } else {
                scope.insert(name.to_string(), VariableInfo { 
                    type_, 
                    mutable,
                    capture_mode: None,
                });
            }
        }
    }
    
    fn lookup_var(&self, name: &str) -> Option<&VariableInfo> {
        for scope in self.scopes.iter().rev() {
            if let Some(info) = scope.get(name) { return Some(info); }
        }
        None
    }
    
    fn build_impl(&mut self, functions: &[FunctionDecl]) -> SemanticProgram {
        let mut program = SemanticProgram::new();
        
        // Register Math functions
        self.function_types.insert("Math.sqrt".to_string(), FunctionSignature {
            params: vec![("x".to_string(), SemanticType::Float)],
            return_type: SemanticType::Float,
        });
        self.function_types.insert("Math.pow".to_string(), FunctionSignature {
            params: vec![("x".to_string(), SemanticType::Float), ("y".to_string(), SemanticType::Float)],
            return_type: SemanticType::Float,
        });
        self.function_types.insert("Math.sin".to_string(), FunctionSignature {
            params: vec![("x".to_string(), SemanticType::Float)],
            return_type: SemanticType::Float,
        });
        self.function_types.insert("Math.cos".to_string(), FunctionSignature {
            params: vec![("x".to_string(), SemanticType::Float)],
            return_type: SemanticType::Float,
        });
        self.function_types.insert("Math.abs".to_string(), FunctionSignature {
            params: vec![("x".to_string(), SemanticType::Float)],
            return_type: SemanticType::Float,
        });
        self.function_types.insert("Math.floor".to_string(), FunctionSignature {
            params: vec![("x".to_string(), SemanticType::Float)],
            return_type: SemanticType::Float,
        });
        self.function_types.insert("Math.ceil".to_string(), FunctionSignature {
            params: vec![("x".to_string(), SemanticType::Float)],
            return_type: SemanticType::Float,
        });
        self.function_types.insert("Math.exp".to_string(), FunctionSignature {
            params: vec![("x".to_string(), SemanticType::Float)],
            return_type: SemanticType::Float,
        });
        self.function_types.insert("Math.log".to_string(), FunctionSignature {
            params: vec![("x".to_string(), SemanticType::Float)],
            return_type: SemanticType::Float,
        });
        self.function_types.insert("Math.tan".to_string(), FunctionSignature {
            params: vec![("x".to_string(), SemanticType::Float)],
            return_type: SemanticType::Float,
        });
        
        // Register String functions
        self.function_types.insert("String.length".to_string(), FunctionSignature {
            params: vec![("s".to_string(), SemanticType::String)],
            return_type: SemanticType::Int,
        });
        self.function_types.insert("String.concat".to_string(), FunctionSignature {
            params: vec![("s1".to_string(), SemanticType::String), ("s2".to_string(), SemanticType::String)],
            return_type: SemanticType::String,
        });
        self.function_types.insert("String.substring".to_string(), FunctionSignature {
            params: vec![("s".to_string(), SemanticType::String), ("start".to_string(), SemanticType::Int), ("length".to_string(), SemanticType::Int)],
            return_type: SemanticType::String,
        });
        self.function_types.insert("String.to_upper".to_string(), FunctionSignature {
            params: vec![("s".to_string(), SemanticType::String)],
            return_type: SemanticType::String,
        });
        self.function_types.insert("String.to_lower".to_string(), FunctionSignature {
            params: vec![("s".to_string(), SemanticType::String)],
            return_type: SemanticType::String,
        });
        
        // Register File functions
        self.function_types.insert("File.read".to_string(), FunctionSignature {
            params: vec![("path".to_string(), SemanticType::String)],
            return_type: SemanticType::String,
        });
        
        // Register List functions
        self.function_types.insert("List.length".to_string(), FunctionSignature {
            params: vec![("arr".to_string(), SemanticType::List(Box::new(SemanticType::Float)))],
            return_type: SemanticType::Int,
        });
        self.function_types.insert("List.sum".to_string(), FunctionSignature {
            params: vec![("arr".to_string(), SemanticType::List(Box::new(SemanticType::Float)))],
            return_type: SemanticType::Float,
        });
        self.function_types.insert("List.max".to_string(), FunctionSignature {
            params: vec![("arr".to_string(), SemanticType::List(Box::new(SemanticType::Float)))],
            return_type: SemanticType::Float,
        });
        self.function_types.insert("List.min".to_string(), FunctionSignature {
            params: vec![("arr".to_string(), SemanticType::List(Box::new(SemanticType::Float)))],
            return_type: SemanticType::Float,
        });
        self.function_types.insert("File.write".to_string(), FunctionSignature {
            params: vec![("path".to_string(), SemanticType::String), ("content".to_string(), SemanticType::String)],
            return_type: SemanticType::Int,
        });
        self.function_types.insert("File.append".to_string(), FunctionSignature {
            params: vec![("path".to_string(), SemanticType::String), ("content".to_string(), SemanticType::String)],
            return_type: SemanticType::Int,
        });
        
        for func in functions {
            let return_type = func.return_type.as_deref()
                .map(SemanticType::from_str)
                .unwrap_or(SemanticType::Void);
            let params = func.params.iter()
                .map(|(n, t)| (n.clone(), SemanticType::from_str(t)))
                .collect();
            
            if self.function_types.contains_key(&func.name) {
                self.diagnostics.push(format!("Duplicate function declaration '{}'", func.name));
            } else {
                self.function_types.insert(func.name.clone(), FunctionSignature { params, return_type });
            }
        }
        
        for func in functions {
            self.push_scope();
            for (name, type_str) in &func.params {
                let param_type = SemanticType::from_str(type_str);
                if self.scopes.last().unwrap().contains_key(name) {
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
                params: func.params.iter().map(|(n, t)| (n.clone(), SemanticType::from_str(t))).collect(),
                return_type: func.return_type.as_deref().map(SemanticType::from_str).unwrap_or(SemanticType::Void),
                blocks: vec![SemanticBlock { id: entry_id, instructions: Vec::new() }],
                entry_block: entry_id,
            };
            
            let flow = self.translate_block(&mut program, &mut semantic_func, entry_id, &func.body);
            
            // Missing return detection
            if semantic_func.return_type != SemanticType::Void
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
                Stmt::If { condition, then_body, else_body } => {
                    self.translate_if(program, func, current_block, condition, then_body, else_body.as_deref())
                }
                Stmt::While { condition, body } => {
                    self.translate_while(program, func, current_block, condition, body)
                }
                Stmt::For { var, iterable, body } => {
                    self.translate_for(program, func, current_block, var, iterable, body)
                }
                Stmt::Match { value, cases } => {
                    self.translate_match(program, func, current_block, value, cases)
                }
                Stmt::Spawn { body } => {
                    self.translate_spawn(program, func, current_block, body)
                }
                Stmt::Parallel { blocks } => {
                    self.translate_parallel(program, func, current_block, blocks)
                }
                Stmt::Defer { stmt } => {
                    self.translate_defer(program, func, current_block, stmt)
                }
                Stmt::UnsafeBlock { body } => {
                    self.push_scope();
                    let flow = self.translate_block(program, func, current_block, body);
                    self.pop_scope();
                    flow
                }
                Stmt::RegionBlock { name: _, body } => {
                    self.push_scope();
                    let flow = self.translate_block(program, func, current_block, body);
                    self.pop_scope();
                    flow
                }
                Stmt::Break => {
                    if let Some(loop_ctx) = self.loop_stack.last().copied() {
                        if let Some(block) = func.blocks.iter_mut().find(|b| b.id == current_block) {
                            block.instructions.push(SemanticInstruction::Jump { block: loop_ctx.break_block });
                        }
                        FlowResult::Unreachable
                    } else {
                        self.diagnostics.push("Break outside of loop".to_string());
                        FlowResult::Reachable(current_block)
                    }
                }
                Stmt::Continue => {
                    if let Some(loop_ctx) = self.loop_stack.last().copied() {
                        if let Some(block) = func.blocks.iter_mut().find(|b| b.id == current_block) {
                            block.instructions.push(SemanticInstruction::Jump { block: loop_ctx.continue_block });
                        }
                        FlowResult::Unreachable
                    } else {
                        self.diagnostics.push("Continue outside of loop".to_string());
                        FlowResult::Reachable(current_block)
                    }
                }

                _ => {
                    self.translate_simple_stmt(program, func, current_block, stmt)
                }
            };
        }
        
        current_flow
    }
    
    fn translate_if(
        &mut self, program: &mut SemanticProgram, func: &mut SemanticFunction,
        current_block: usize, condition: &Expr, then_body: &[Stmt], else_body: Option<&[Stmt]>,
    ) -> FlowResult {
        let cond = self.translate_expr(condition);
        let cond_type = cond.type_of();
        if cond_type != SemanticType::Bool && cond_type != SemanticType::Unknown {
            self.diagnostics.push(format!(
                "If condition type mismatch: expected Bool, found {:?}",
                cond_type
            ));
        }

        let then_id = program.new_block_id();
        let else_id = program.new_block_id();
        
        if let Some(block) = func.blocks.iter_mut().find(|b| b.id == current_block) {
            block.instructions.push(SemanticInstruction::Branch { condition: cond, then_block: then_id, else_block: else_id });
        }
        
        // Translate Then Branch
        func.blocks.push(SemanticBlock { id: then_id, instructions: Vec::new() });
        self.push_scope();
        let then_flow = self.translate_block(program, func, then_id, then_body);
        self.pop_scope();

        // Translate Else Branch
        func.blocks.push(SemanticBlock { id: else_id, instructions: Vec::new() });
        let else_flow = if let Some(else_stmts) = else_body {
            self.push_scope();
            let flow = self.translate_block(program, func, else_id, else_stmts);
            self.pop_scope();
            flow
        } else {
            FlowResult::Reachable(else_id)
        };

        match (then_flow, else_flow) {
            (FlowResult::Unreachable, FlowResult::Unreachable) => {
                FlowResult::Unreachable
            }
            (t_flow, e_flow) => {
                let merge_id = program.new_block_id();
                if let FlowResult::Reachable(id) = t_flow {
                    if let Some(block) = func.blocks.iter_mut().find(|b| b.id == id) {
                        block.instructions.push(SemanticInstruction::Jump { block: merge_id });
                    }
                }
                if let FlowResult::Reachable(id) = e_flow {
                    if let Some(block) = func.blocks.iter_mut().find(|b| b.id == id) {
                        block.instructions.push(SemanticInstruction::Jump { block: merge_id });
                    }
                }
                func.blocks.push(SemanticBlock { id: merge_id, instructions: Vec::new() });
                FlowResult::Reachable(merge_id)
            }
        }
    }
    
    fn translate_while(
        &mut self, program: &mut SemanticProgram, func: &mut SemanticFunction,
        current_block: usize, condition: &Expr, body: &[Stmt],
    ) -> FlowResult {
        let cond_id = program.new_block_id();
        let body_id = program.new_block_id();
        let merge_id = program.new_block_id();
        
        if let Some(block) = func.blocks.iter_mut().find(|b| b.id == current_block) {
            block.instructions.push(SemanticInstruction::Jump { block: cond_id });
        }
        
        let cond = self.translate_expr(condition);
        let cond_type = cond.type_of();
        if cond_type != SemanticType::Bool && cond_type != SemanticType::Unknown {
            self.diagnostics.push(format!(
                "While condition type mismatch: expected Bool, found {:?}",
                cond_type
            ));
        }

        func.blocks.push(SemanticBlock {
            id: cond_id,
            instructions: vec![SemanticInstruction::Branch { condition: cond, then_block: body_id, else_block: merge_id }],
        });
        
        func.blocks.push(SemanticBlock { id: body_id, instructions: Vec::new() });
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
                    block.instructions.push(SemanticInstruction::Jump { block: cond_id });
                }
            }
        }
        
        func.blocks.push(SemanticBlock { id: merge_id, instructions: Vec::new() });
        FlowResult::Reachable(merge_id)
    }
    
    fn translate_for(
        &mut self, program: &mut SemanticProgram, func: &mut SemanticFunction,
        current_block: usize, var: &str, iterable: &Expr, body: &[Stmt],
    ) -> FlowResult {
        let init_id = program.new_block_id();
        let cond_id = program.new_block_id();
        let body_id = program.new_block_id();
        let merge_id = program.new_block_id();
        
        if let Some(block) = func.blocks.iter_mut().find(|b| b.id == current_block) {
            block.instructions.push(SemanticInstruction::Jump { block: init_id });
        }
        
        let iterable_val = self.translate_expr(iterable);
        let iterable_type = iterable_val.type_of();
        let elem_type = match &iterable_type {
            SemanticType::List(elem) => (**elem).clone(),
            SemanticType::Unknown => SemanticType::Unknown,
            other => {
                self.diagnostics.push(format!(
                    "For loop iterable type mismatch: expected List, found {:?}",
                    other
                ));
                SemanticType::Unknown
            }
        };
        
        self.push_scope();
        self.declare_var(var, elem_type, false);
        
        self.iter_counter += 1;
        let iter_name = format!("__iter_{}_{}", var, self.iter_counter);
        
        func.blocks.push(SemanticBlock {
            id: init_id,
            instructions: vec![
                SemanticInstruction::IteratorInit {
                    iterator: iter_name.clone(),
                    iterable: iterable_val,
                },
                SemanticInstruction::Jump { block: cond_id },
            ],
        });
        
        func.blocks.push(SemanticBlock {
            id: cond_id,
            instructions: vec![
                SemanticInstruction::IteratorNext {
                    iterator: iter_name.clone(),
                    target: var.to_string(),
                    body_block: body_id,
                    exit_block: merge_id,
                }
            ],
        });
        
        func.blocks.push(SemanticBlock { id: body_id, instructions: Vec::new() });
        self.loop_stack.push(LoopContext {
            break_block: merge_id,
            continue_block: cond_id,
        });
        let body_flow = self.translate_block(program, func, body_id, body);
        self.loop_stack.pop();
        if let FlowResult::Reachable(id) = body_flow {
            if let Some(block) = func.blocks.iter_mut().find(|b| b.id == id) {
                if !Self::is_terminated(block) {
                    block.instructions.push(SemanticInstruction::Jump { block: cond_id });
                }
            }
        }
        
        self.pop_scope();
        
        func.blocks.push(SemanticBlock { id: merge_id, instructions: Vec::new() });
        FlowResult::Reachable(merge_id)
    }
    
    fn translate_match(
        &mut self, program: &mut SemanticProgram, func: &mut SemanticFunction,
        current_block: usize, value: &Expr, cases: &[MatchCase],
    ) -> FlowResult {
        let typed_value = self.translate_expr(value);
        let merge_id = program.new_block_id();
        
        let mut case_triplets = Vec::new();
        for case in cases {
            let case_id = program.new_block_id();
            let pattern = match &case.pattern {
                crate::ast::Pattern::Some(v) => SemanticPattern::Some { binding: v.clone() },
                crate::ast::Pattern::None => SemanticPattern::None,
                crate::ast::Pattern::Ok(v) => SemanticPattern::Ok { binding: v.clone() },
                crate::ast::Pattern::Error(v) => SemanticPattern::Error { binding: v.clone() },
                crate::ast::Pattern::Wildcard => SemanticPattern::Wildcard,
                crate::ast::Pattern::Literal(e) => SemanticPattern::Literal(self.translate_expr(e)),
            };
            case_triplets.push((pattern, case_id, case.body.clone()));
        }
        
        let switch_cases = case_triplets.iter().map(|(pat, id, _)| (pat.clone(), *id)).collect();
        
        // Check exhaustiveness: if wildcard pattern exists, match is exhaustive
        let has_wildcard = case_triplets.iter().any(|(pat, _, _)| {
            matches!(pat, SemanticPattern::Wildcard)
        });
        let is_exhaustive = has_wildcard || case_triplets.iter().any(|(pat, _, _)| {
            matches!(pat, SemanticPattern::Some { .. } | SemanticPattern::Ok { .. } | SemanticPattern::Error { .. })
        });
        
        let default_block = if is_exhaustive {
            None
        } else {
            Some(merge_id)
        };
        
        if let Some(block) = func.blocks.iter_mut().find(|b| b.id == current_block) {
            block.instructions.push(SemanticInstruction::Switch {
                value: typed_value,
                cases: switch_cases,
                default_block,
            });
        }
        
        let mut all_unreachable = true;
        for (_pat, case_id, body) in case_triplets {
            func.blocks.push(SemanticBlock { id: case_id, instructions: Vec::new() });
            self.push_scope();
            let case_flow = self.translate_block(program, func, case_id, &body);
            self.pop_scope();
            
            match case_flow {
                FlowResult::Reachable(id) => {
                    all_unreachable = false;
                    if let Some(block) = func.blocks.iter_mut().find(|b| b.id == id) {
                        if !Self::is_terminated(block) {
                            block.instructions.push(SemanticInstruction::Jump { block: merge_id });
                        }
                    }
                }
                FlowResult::Unreachable => {}
            }
        }
        
        func.blocks.push(SemanticBlock { id: merge_id, instructions: Vec::new() });
        if all_unreachable {
            FlowResult::Unreachable
        } else {
            FlowResult::Reachable(merge_id)
        }
    }
    
    fn translate_spawn(
        &mut self, program: &mut SemanticProgram, func: &mut SemanticFunction,
        current_block: usize, body: &[Stmt],
    ) -> FlowResult {
        let spawn_entry = program.new_block_id();
        let continuation_id = program.new_block_id();
        
        if let Some(block) = func.blocks.iter_mut().find(|b| b.id == current_block) {
            block.instructions.push(SemanticInstruction::Spawn { entry_block: spawn_entry });
            block.instructions.push(SemanticInstruction::Jump { block: continuation_id });
        }
        
        func.blocks.push(SemanticBlock { id: spawn_entry, instructions: Vec::new() });
        self.push_scope();
        let spawn_flow = self.translate_block(program, func, spawn_entry, body);
        self.pop_scope();
        
        if let FlowResult::Reachable(id) = spawn_flow {
            if let Some(block) = func.blocks.iter_mut().find(|b| b.id == id) {
                if !Self::is_terminated(block) {
                    block.instructions.push(SemanticInstruction::Return { value: None, type_: SemanticType::Void });
                }
            }
        }
        
        func.blocks.push(SemanticBlock { id: continuation_id, instructions: Vec::new() });
        FlowResult::Reachable(continuation_id)
    }
    
    fn translate_parallel(
        &mut self, program: &mut SemanticProgram, func: &mut SemanticFunction,
        current_block: usize, blocks: &[Vec<Stmt>],
    ) -> FlowResult {
        if blocks.is_empty() {
            return FlowResult::Reachable(current_block);
        }
        
        let merge_id = program.new_block_id();
        let mut entry_blocks = Vec::new();
        
        for block_stmts in blocks {
            let entry_id = program.new_block_id();
            entry_blocks.push(entry_id);
            
            func.blocks.push(SemanticBlock { id: entry_id, instructions: Vec::new() });
            self.push_scope();
            let block_flow = self.translate_block(program, func, entry_id, block_stmts);
            self.pop_scope();
            
            if let FlowResult::Reachable(id) = block_flow {
                if let Some(block) = func.blocks.iter_mut().find(|b| b.id == id) {
                    if !Self::is_terminated(block) {
                        block.instructions.push(SemanticInstruction::Jump { block: merge_id });
                    }
                }
            }
        }
        
        if let Some(block) = func.blocks.iter_mut().find(|b| b.id == current_block) {
            block.instructions.push(SemanticInstruction::Fork { blocks: entry_blocks, join_block: merge_id });
        }
        
        func.blocks.push(SemanticBlock { id: merge_id, instructions: Vec::new() });
        FlowResult::Reachable(merge_id)
    }
    
    fn translate_defer(
        &mut self, program: &mut SemanticProgram, func: &mut SemanticFunction,
        current_block: usize, stmt: &Stmt,
    ) -> FlowResult {
        let cleanup_id = program.new_block_id();
        
        func.blocks.push(SemanticBlock { id: cleanup_id, instructions: Vec::new() });
        self.push_scope();
        let _cleanup_flow = self.translate_block(program, func, cleanup_id, std::slice::from_ref(stmt));
        self.pop_scope();
        
        // Register cleanup block in defer_stack
        if let Some(defer_ctx) = self.defer_stack.last_mut() {
            defer_ctx.cleanup_blocks.push(cleanup_id);
        } else {
            let mut defer_ctx = DeferContext::default();
            defer_ctx.cleanup_blocks.push(cleanup_id);
            self.defer_stack.push(defer_ctx);
        }
        
        if let Some(block) = func.blocks.iter_mut().find(|b| b.id == current_block) {
            block.instructions.push(SemanticInstruction::Defer { cleanup_block: cleanup_id });
        }
        
        FlowResult::Reachable(current_block)
    }
    
    fn validate_binary_op(&mut self, op: &BinOp, l: TypedIRValue, r: TypedIRValue) -> (SemanticType, TypedIRValue, TypedIRValue) {
        let left_t = l.type_of();
        let right_t = r.type_of();
        
        match op {
            BinOp::Add | BinOp::Subtract | BinOp::Multiply | BinOp::Divide => {
                if left_t == SemanticType::Int && right_t == SemanticType::Int {
                    (SemanticType::Int, l, r)
                } else if left_t == SemanticType::Float && right_t == SemanticType::Float {
                    (SemanticType::Float, l, r)
                } else if left_t == SemanticType::Int && right_t == SemanticType::Float {
                    let cast_l = TypedIRValue::Cast { value: Box::new(l), target_type: SemanticType::Float };
                    (SemanticType::Float, cast_l, r)
                } else if left_t == SemanticType::Float && right_t == SemanticType::Int {
                    let cast_r = TypedIRValue::Cast { value: Box::new(r), target_type: SemanticType::Float };
                    (SemanticType::Float, l, cast_r)
                } else {
                    if left_t != SemanticType::Unknown && right_t != SemanticType::Unknown {
                        self.diagnostics.push(format!(
                            "Invalid operands for binary operator {:?}: {:?} and {:?}",
                            op, left_t, right_t
                        ));
                    }
                    (SemanticType::Unknown, l, r)
                }
            }
            BinOp::Greater | BinOp::Less | BinOp::GreaterEqual | BinOp::LessEqual => {
                if left_t == SemanticType::Int && right_t == SemanticType::Int {
                    (SemanticType::Bool, l, r)
                } else if left_t == SemanticType::Float && right_t == SemanticType::Float {
                    (SemanticType::Bool, l, r)
                } else if left_t == SemanticType::Int && right_t == SemanticType::Float {
                    let cast_l = TypedIRValue::Cast { value: Box::new(l), target_type: SemanticType::Float };
                    (SemanticType::Bool, cast_l, r)
                } else if left_t == SemanticType::Float && right_t == SemanticType::Int {
                    let cast_r = TypedIRValue::Cast { value: Box::new(r), target_type: SemanticType::Float };
                    (SemanticType::Bool, l, cast_r)
                } else {
                    if left_t != SemanticType::Unknown && right_t != SemanticType::Unknown {
                        self.diagnostics.push(format!(
                            "Invalid operands for comparison {:?}: {:?} and {:?}",
                            op, left_t, right_t
                        ));
                    }
                    (SemanticType::Bool, l, r)
                }
            }
            BinOp::Equal | BinOp::NotEqual => {
                if left_t.can_coerce_to(&right_t) && right_t.can_coerce_to(&left_t) {
                    if left_t == SemanticType::Int && right_t == SemanticType::Float {
                        let cast_l = TypedIRValue::Cast { value: Box::new(l), target_type: SemanticType::Float };
                        (SemanticType::Bool, cast_l, r)
                    } else if left_t == SemanticType::Float && right_t == SemanticType::Int {
                        let cast_r = TypedIRValue::Cast { value: Box::new(r), target_type: SemanticType::Float };
                        (SemanticType::Bool, l, cast_r)
                    } else {
                        (SemanticType::Bool, l, r)
                    }
                } else {
                    self.diagnostics.push(format!(
                        "Type mismatch for equality comparison: {:?} and {:?}",
                        left_t, right_t
                    ));
                    (SemanticType::Bool, l, r)
                }
            }
            BinOp::And | BinOp::Or => {
                if left_t != SemanticType::Bool && left_t != SemanticType::Unknown {
                    self.diagnostics.push(format!("Logical operator requires Bool left operand, found {:?}", left_t));
                }
                if right_t != SemanticType::Bool && right_t != SemanticType::Unknown {
                    self.diagnostics.push(format!("Logical operator requires Bool right operand, found {:?}", right_t));
                }
                (SemanticType::Bool, l, r)
            }
        }
    }
    
    fn coerce_value(&self, value: TypedIRValue, target: &SemanticType) -> TypedIRValue {
        let value_type = value.type_of();
        if value_type != SemanticType::Unknown
            && *target != SemanticType::Unknown
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
    
    fn validate_call(&mut self, name: &str, args: Vec<TypedIRValue>) -> (SemanticType, Vec<TypedIRValue>) {
        if let Some(sig) = self.function_types.get(name).cloned() {
            let mut coerced_args = Vec::new();
            
            if args.len() != sig.params.len() {
                self.diagnostics.push(format!(
                    "Function '{}' called with {} arguments, but expects {}",
                    name, args.len(), sig.params.len()
                ));
                coerced_args = args;
            } else {
                for (i, (arg, (_, expected_type))) in args.iter().zip(&sig.params).enumerate() {
                    let actual_type = arg.type_of();
                    if !actual_type.can_coerce_to(expected_type) && actual_type != SemanticType::Unknown && *expected_type != SemanticType::Unknown {
                        self.diagnostics.push(format!(
                            "Argument type mismatch at index {} in call to '{}': expected {:?}, found {:?}",
                            i, name, expected_type, actual_type
                        ));
                    }
                    // Apply coercion
                    coerced_args.push(self.coerce_value(arg.clone(), expected_type));
                }
            }
            (sig.return_type.clone(), coerced_args)
        } else {
            self.diagnostics.push(format!("Call to unknown function '{}'", name));
            (SemanticType::Unknown, args)
        }
    }
    
    fn translate_simple_stmt(
        &mut self, program: &mut SemanticProgram, func: &mut SemanticFunction,
        current_block: usize, stmt: &Stmt,
    ) -> FlowResult {
        let instruction = match stmt {
            Stmt::VarDecl { name, value, mutable, type_annotation } => {
                let typed_value = self.translate_expr(value);
                let value_type = typed_value.type_of();
                
                let type_ = if let Some(type_str) = type_annotation {
                    let declared_type = SemanticType::from_str(type_str);
                    if value_type != SemanticType::Unknown
                        && declared_type != SemanticType::Unknown
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
                SemanticInstruction::Declare { name: name.clone(), mutable: *mutable, type_, value: typed_value }
            }
            Stmt::Assign { name, value } => {
                let var_info = match self.lookup_var(name) {
                    Some(info) => info.clone(),
                    None => {
                        self.diagnostics.push(format!("Assignment to undeclared variable '{}'", name));
                        VariableInfo { type_: SemanticType::Unknown, mutable: true, capture_mode: None }
                    }
                };
                
                if !var_info.mutable {
                    self.diagnostics.push(format!("Cannot assign to immutable variable '{}'", name));
                }
                
                let expected_type = var_info.type_;
                let typed_value = self.translate_expr(value);
                let actual_type = typed_value.type_of();
                
                if expected_type != SemanticType::Unknown
                    && actual_type != SemanticType::Unknown
                    && !actual_type.can_coerce_to(&expected_type)
                {
                    self.diagnostics.push(format!(
                        "Assignment type mismatch for '{}': expected {:?}, found {:?}",
                        name, expected_type, actual_type
                    ));
                }
                
                SemanticInstruction::Assign { target: name.clone(), value: typed_value }
            }
            Stmt::Print { expr } => {
                let typed_value = self.translate_expr(expr);
                SemanticInstruction::Print { value: typed_value }
            }
            Stmt::Return { value } => {
                let typed_value = value.as_ref().map(|v| self.translate_expr(v));
                
                // Apply coercion (Int -> Float, etc.)
                let coerced_value = typed_value.map(|v| {
                    self.coerce_value(v, &func.return_type)
                });
                
                let type_ = coerced_value.as_ref()
                    .map(|v| v.type_of())
                    .unwrap_or(SemanticType::Void);
                
                // Only diagnostic if STILL mismatched after coercion
                if type_ != SemanticType::Unknown
                    && func.return_type != SemanticType::Unknown
                    && !type_.can_coerce_to(&func.return_type)
                {
                    self.diagnostics.push(format!(
                        "Return type mismatch in function '{}': expected {:?}, found {:?}",
                        func.name, func.return_type, type_
                    ));
                }
                
                SemanticInstruction::Return { value: coerced_value, type_ }
            }
            Stmt::FunctionCall { name, args } => {
                let typed_args: Vec<TypedIRValue> = args.iter().map(|a| self.translate_expr(a)).collect();
                let (return_type, coerced_args) = self.validate_call(name, typed_args);
                SemanticInstruction::Call { result: None, function: name.clone(), args: coerced_args, return_type }
            }
            Stmt::ArrayAssign { array, index, value } => {
                let arr_expr = Expr::Var(array.clone());
                let arr_val = self.translate_expr(&arr_expr);
                let idx_val = self.translate_expr(index);
                let val = self.translate_expr(value);
                
                let arr_type = arr_val.type_of();
                let idx_type = idx_val.type_of();
                let val_type = val.type_of();
                
                if idx_type != SemanticType::Int && idx_type != SemanticType::Unknown {
                    self.diagnostics.push(format!(
                        "Array index type mismatch for '{}': expected Int, found {:?}",
                        array, idx_type
                    ));
                }
                
                match arr_type {
                    SemanticType::List(elem) => {
                        if !val_type.can_coerce_to(&elem) && val_type != SemanticType::Unknown {
                            self.diagnostics.push(format!(
                                "Array assignment element type mismatch for '{}': expected {:?}, found {:?}",
                                array, *elem, val_type
                            ));
                        }
                    }
                    SemanticType::Unknown => {}
                    other => {
                        self.diagnostics.push(format!(
                            "Array assignment target '{}' is not a list, found {:?}",
                            array, other
                        ));
                    }
                }
                
                SemanticInstruction::ArrayAssign { array: Box::new(arr_val), index: Box::new(idx_val), value: val }
            }
            Stmt::ChannelDecl { name } => {
                let chan_type = SemanticType::Channel(Box::new(SemanticType::Unknown));
                self.declare_var(name, chan_type.clone(), true);
                SemanticInstruction::ChannelDecl { name: name.clone(), type_: chan_type }
            }
            Stmt::Send { channel, value } => {
                let typed_value = self.translate_expr(value);
                let val_type = typed_value.type_of();
                
                match self.lookup_var(channel) {
                    Some(info) => {
                        match &info.type_ {
                            SemanticType::Channel(inner) => {
                                if **inner != SemanticType::Unknown && val_type != SemanticType::Unknown && !val_type.can_coerce_to(inner) {
                                    self.diagnostics.push(format!(
                                        "Channel send type mismatch for '{}': expected Channel<{:?}>, sent {:?}",
                                        channel, **inner, val_type
                                    ));
                                }
                            }
                            SemanticType::Unknown => {}
                            other => {
                                self.diagnostics.push(format!(
                                    "Variable '{}' is not a channel, found {:?}",
                                    channel, other
                                ));
                            }
                        }
                    }
                    None => {
                        self.diagnostics.push(format!("Send to undeclared channel '{}'", channel));
                    }
                }
                SemanticInstruction::Send { channel: channel.clone(), value: typed_value }
            }
            Stmt::Receive { channel, target } => {
                let target_info = match self.lookup_var(target).cloned() {
                    Some(info) => {
                        if !info.mutable {
                            self.diagnostics.push(format!("Cannot receive into immutable variable '{}'", target));
                        }
                        info.clone()
                    }
                    None => {
                        self.diagnostics.push(format!("Receive target variable '{}' is undeclared", target));
                        VariableInfo { type_: SemanticType::Unknown, mutable: true, capture_mode: None }
                    }
                };
                
                match self.lookup_var(channel) {
                    Some(info) => {
                        match &info.type_ {
                            SemanticType::Channel(inner) => {
                                if **inner != SemanticType::Unknown && target_info.type_ != SemanticType::Unknown && !inner.can_coerce_to(&target_info.type_) {
                                    self.diagnostics.push(format!(
                                        "Channel receive type mismatch for '{}': channel carries {:?}, target has type {:?}",
                                        channel, **inner, target_info.type_
                                    ));
                                }
                            }
                            SemanticType::Unknown => {}
                            other => {
                                self.diagnostics.push(format!(
                                    "Variable '{}' is not a channel, found {:?}",
                                    channel, other
                                ));
                            }
                        }
                    }
                    None => {
                        self.diagnostics.push(format!("Receive from undeclared channel '{}'", channel));
                    }
                }
                
                SemanticInstruction::Receive { channel: channel.clone(), target: target.clone() }
            }
            Stmt::Import { path } => {
                // Imports are resolved before IR building
                let _ = path;
                SemanticInstruction::Print { value: TypedIRValue::String("import".to_string()) }
            }
            Stmt::TryCatch { try_body, catch_var, catch_body, finally_body } => {
                // Translate try body in a new scope
                self.push_scope();
                for s in try_body {
                    let _ = self.translate_simple_stmt(program, func, current_block, s);
                }
                self.pop_scope();
                
                // Translate catch body with catch variable in scope
                if let Some(var) = catch_var {
                    self.push_scope();
                    self.declare_var(var, SemanticType::String, false);
                    for s in catch_body {
                        let _ = self.translate_simple_stmt(program, func, current_block, s);
                    }
                    self.pop_scope();
                }
                
                // Translate finally body
                if let Some(finally) = finally_body {
                    self.push_scope();
                    for s in finally {
                        let _ = self.translate_simple_stmt(program, func, current_block, s);
                    }
                    self.pop_scope();
                }
                
                SemanticInstruction::Print { value: TypedIRValue::String("trycatch".to_string()) }
            }
            Stmt::If { .. } | Stmt::While { .. } | Stmt::For { .. } | Stmt::Match { .. }
            | Stmt::Spawn { .. } | Stmt::Parallel { .. } | Stmt::Defer { .. }
            | Stmt::RegionBlock { .. }
            | Stmt::UnsafeBlock { .. }
            | Stmt::Break | Stmt::Continue => {
                unreachable!("Control flow should be intercepted by translate_block");
            }
        };
        
        let block = func.blocks.iter_mut().find(|b| b.id == current_block).unwrap();
        block.instructions.push(instruction);
        
        // Check if this statement terminates the block
        match &stmt {
            Stmt::Return { .. } => FlowResult::Unreachable,
            _ => FlowResult::Reachable(current_block),
        }
    }
    
    fn translate_expr(&mut self, expr: &Expr) -> TypedIRValue {
        match expr {
            Expr::Borrow { expr } => {
                let inner = self.translate_expr(expr);
                TypedIRValue::Cast {
                    value: Box::new(inner),
                    target_type: SemanticType::Unknown,
                }
            }
            Expr::MutBorrow { expr } => {
                let inner = self.translate_expr(expr);
                TypedIRValue::Cast {
                    value: Box::new(inner),
                    target_type: SemanticType::Unknown,
                }
            }
            Expr::Deref { expr } => {
                let inner = self.translate_expr(expr);
                TypedIRValue::Cast {
                    value: Box::new(inner),
                    target_type: SemanticType::Unknown,
                }
            }
            Expr::AddrOf { expr } => {
                let inner = self.translate_expr(expr);
                TypedIRValue::Cast {
                    value: Box::new(inner),
                    target_type: SemanticType::Unknown,
                }
            }
            Expr::Number(n) => TypedIRValue::Float(*n),
            Expr::Int(i) => TypedIRValue::Int(*i),
            Expr::String(s) => TypedIRValue::String(s.clone()),
            Expr::Bool(b) => TypedIRValue::Bool(*b),
            Expr::Var(name) => {
                match self.lookup_var(name) {
                    Some(info) => TypedIRValue::Variable(name.clone(), info.type_.clone()),
                    None => {
                        self.diagnostics.push(format!("Use of undeclared variable '{}'", name));
                        TypedIRValue::Variable(name.clone(), SemanticType::Unknown)
                    }
                }
            }
            Expr::List(elements) => {
                let mut values: Vec<TypedIRValue> = elements.iter().map(|e| self.translate_expr(e)).collect();
                
                // Find common type: Float wins if any element is Float
                let has_float = values.iter().any(|v| v.type_of() == SemanticType::Float);
                let has_int = values.iter().any(|v| v.type_of() == SemanticType::Int);
                
                if has_float && has_int {
                    // Coerce all Int to Float
                    for val in &mut values {
                        if val.type_of() == SemanticType::Int {
                            *val = TypedIRValue::Cast {
                                value: Box::new(val.clone()),
                                target_type: SemanticType::Float,
                            };
                        }
                    }
                } else if let Some(first) = values.first() {
                    let t = first.type_of();
                    for val in &values {
                        if !val.type_of().can_coerce_to(&t) && val.type_of() != SemanticType::Unknown && t != SemanticType::Unknown {
                            self.diagnostics.push(format!(
                                "Heterogeneous list element types found: expected {:?}, found {:?}",
                                t, val.type_of()
                            ));
                        }
                    }
                }
                TypedIRValue::List(values)
            }
            Expr::Binary { left, op, right } => {
                let l = self.translate_expr(left);
                let r = self.translate_expr(right);
                let (result_type, cast_l, cast_r) = self.validate_binary_op(op, l, r);
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
                TypedIRValue::BinaryOp { op: semantic_op, left: Box::new(cast_l), right: Box::new(cast_r), result_type }
            }
            Expr::FunctionCall { name, args } => {
                let typed_args: Vec<TypedIRValue> = args.iter().map(|a| self.translate_expr(a)).collect();
                let (return_type, coerced_args) = self.validate_call(name, typed_args);
                TypedIRValue::Call { function: name.clone(), args: coerced_args, return_type }
            }
            Expr::ArrayAccess { array, index } => {
                let array_value = self.translate_expr(array);
                let index_value = self.translate_expr(index);
                
                let idx_type = index_value.type_of();
                if idx_type != SemanticType::Int && idx_type != SemanticType::Unknown {
                    self.diagnostics.push(format!(
                        "Array access index type mismatch: expected Int, found {:?}",
                        idx_type
                    ));
                }

                let arr_type = array_value.type_of();
                let element_type = match arr_type {
                    SemanticType::List(elem) => *elem,
                    SemanticType::Unknown => SemanticType::Unknown,
                    other => {
                        self.diagnostics.push(format!(
                            "Cannot index value of type {:?}",
                            other
                        ));
                        SemanticType::Unknown
                    }
                };
                
                TypedIRValue::ArrayAccess { array: Box::new(array_value), index: Box::new(index_value), element_type }
            }
            Expr::Some { value } => { let inner = self.translate_expr(value); TypedIRValue::Some(Box::new(inner)) }
            Expr::None => TypedIRValue::None,
            Expr::Ok { value } => { let inner = self.translate_expr(value); TypedIRValue::Ok(Box::new(inner)) }
            Expr::Error { value } => { let inner = self.translate_expr(value); TypedIRValue::Error(Box::new(inner)) }
        }
    }
}