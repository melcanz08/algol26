// src/semantic.rs - Orthogonal + unified types + is_copy fix

use std::collections::{HashMap, HashSet};
use crate::frontend::ast::{Expr, FunctionDecl, Stmt, BinOp, Pattern, WhereClause, TraitDecl, ImplBlock};
use crate::common::diagnostics::{CompileError, ErrorCode, Result};
use crate::common::types::Type;
use crate::semantics::trait_registry::TraitRegistry;

pub struct SemanticAnalyzer {
    span_map: std::collections::HashMap<usize, (usize, usize)>,
    scopes: Vec<HashMap<String, (Type, bool)>>,
    moved_vars: Vec<Vec<String>>,
    borrowed_vars: Vec<HashSet<String>>,
    mutably_borrowed: Vec<HashSet<String>>,
    functions: HashMap<String, FunctionInfo>,
    current_return_type: Option<Type>,
    list_lengths: Vec<HashMap<String, usize>>,
    list_values: Vec<HashMap<String, Vec<Expr>>>,
     // NEW: Generic type support
    type_params: Vec<HashMap<String, Type>>,  // Stack of type param scopes
    type_constraints: Vec<HashMap<String, Vec<String>>>,  // T -> [Comparable, ...]
    trait_registry: TraitRegistry,
}

#[derive(Debug, Clone)]
struct FunctionInfo {
    params: Vec<(String, Type)>,
    return_type: Type,
}

impl SemanticAnalyzer {
    pub fn new() -> Self {
        SemanticAnalyzer {
            span_map: std::collections::HashMap::new(),
            scopes: vec![HashMap::new()],
            functions: HashMap::new(),
            moved_vars: vec![Vec::new()],
            borrowed_vars: vec![HashSet::new()],
            mutably_borrowed: vec![HashSet::new()],
            current_return_type: None,
            list_lengths: vec![HashMap::new()],
            list_values: vec![HashMap::new()],
            type_params: vec![HashMap::new()],
            type_constraints: vec![HashMap::new()],
            trait_registry: TraitRegistry::new(),
        }
    }

    fn push_scope(&mut self) {
        self.scopes.push(HashMap::new());
        self.moved_vars.push(Vec::new());
        self.borrowed_vars.push(HashSet::new());
        self.mutably_borrowed.push(HashSet::new());
        self.list_lengths.push(HashMap::new());
        self.list_values.push(HashMap::new());
        self.type_params.push(HashMap::new());
        self.type_constraints.push(HashMap::new());
    }

    fn pop_scope(&mut self) {
        self.scopes.pop();
        self.borrowed_vars.pop();
        self.mutably_borrowed.pop();
        self.moved_vars.pop();
        self.list_lengths.pop();
        self.list_values.pop();
        self.type_params.pop();
        self.type_constraints.pop();
    }

    fn lookup_list_length(&self, name: &str) -> Option<usize> {
        for scope in self.list_lengths.iter().rev() {
            if let Some(len) = scope.get(name) {
                return Some(*len);
            }
        }
        None
    }

    fn declare_list_length(&mut self, name: &str, len: usize) {
        if let Some(scope) = self.list_lengths.last_mut() {
            scope.insert(name.to_string(), len);
        }
    }

    fn lookup_list_values(&self, name: &str) -> Option<Vec<Expr>> {
        for scope in self.list_values.iter().rev() {
            if let Some(vals) = scope.get(name) {
                return Some(vals.clone());
            }
        }
        None
    }

    fn declare_list_values(&mut self, name: &str, vals: Vec<Expr>) {
        if let Some(scope) = self.list_values.last_mut() {
            scope.insert(name.to_string(), vals);
        }
    }

    fn is_moved(&self, name: &str) -> bool {
        self.moved_vars.iter().any(|scope| scope.iter().any(|v| v == name))
    }

    fn mark_moved(&mut self, name: &str) {
        if let Some(scope) = self.moved_vars.last_mut() {
            if !scope.contains(&name.to_string()) {
                scope.push(name.to_string());
            }
        }
    }
    
    fn mark_borrowed(&mut self, name: &str) {
        if let Some(scope) = self.borrowed_vars.last_mut() {
            scope.insert(name.to_string());
        }
    }
    
    fn mark_mutably_borrowed(&mut self, name: &str) {
        if let Some(scope) = self.mutably_borrowed.last_mut() {
            scope.insert(name.to_string());
        }
    }
    
    fn is_mutably_borrowed(&self, name: &str) -> bool {
        self.mutably_borrowed.iter().any(|scope| scope.contains(name))
    }
    
    fn is_borrowed(&self, name: &str) -> bool {
        self.borrowed_vars.iter().any(|scope| scope.contains(name))
    }
    
    fn check_borrow_rules(&self, name: &str, mutable: bool) -> Result<()> {
        if self.is_moved(name) {
            return Err(CompileError::new(
                &format!("Cannot borrow moved variable '{}'", name),
                0, 0, "",
                ErrorCode::E0007,
            ).with_suggestion("The variable has been moved and is no longer available"));
        }
        
        if mutable && self.is_mutably_borrowed(name) {
            return Err(CompileError::new(
                &format!("Cannot mutably borrow '{}' more than once", name),
                0, 0, "",
                ErrorCode::E0007,
            ).with_suggestion("Only one mutable borrow is allowed at a time"));
        }
        
        if mutable && self.is_borrowed(name) {
            return Err(CompileError::new(
                &format!("Cannot mutably borrow '{}' while immutably borrowed", name),
                0, 0, "",
                ErrorCode::E0007,
            ).with_suggestion("Wait for the immutable borrow to end"));
        }
        
        if !mutable && self.is_mutably_borrowed(name) {
            return Err(CompileError::new(
                &format!("Cannot read '{}' while it is mutably borrowed", name),
                0, 0, "",
                ErrorCode::E0007,
            ).with_suggestion("Wait for the mutable borrow to end before reading"));
        }
        
        Ok(())
    }
    
    pub fn analyze(&mut self, functions: &[FunctionDecl]) -> Result<()> {
        self.analyze_with_spans(functions, &[], &[], &std::collections::HashMap::new())
    }
    
    pub fn analyze_with_spans(
        &mut self,
        functions: &[FunctionDecl],
        traits: &[TraitDecl],      
        impls: &[ImplBlock],        
        span_map: &std::collections::HashMap<usize, (usize, usize)>,
    ) -> Result<()> {
        self.span_map = span_map.clone();
        self.register_builtin_functions();
        self.register_user_functions(functions);
        
        // Register traits and impls
        for trait_decl in traits {
            self.trait_registry.register_trait(trait_decl.clone());
        }
        for impl_block in impls {
            self.trait_registry.register_impl(impl_block.clone());
        }
        
        // Validate impls
        for impl_block in impls {
            if let Err(err) = self.trait_registry.validate_impl(impl_block) {
                return Err(CompileError::new(
                    &err,
                    0, 0, "",
                    ErrorCode::E0002,
                ));
            }
        }
        
        for func in functions {
            self.analyze_function(func)?;
        }
        
        Ok(())
    }

    // New method with traits
    pub fn analyze_with_traits(
        &mut self,
        functions: &[FunctionDecl],
        traits: &[TraitDecl],
        impls: &[ImplBlock],
        span_map: &std::collections::HashMap<usize, (usize, usize)>,
    ) -> Result<()> {
        self.span_map = span_map.clone();
        self.register_builtin_functions();
        self.register_user_functions(functions);
        
        // Register traits and impls
        for trait_decl in traits {
            self.trait_registry.register_trait(trait_decl.clone());
        }
        for impl_block in impls {
            self.trait_registry.register_impl(impl_block.clone());
        }
        
        // Validate impls
        for impl_block in impls {
            if let Err(err) = self.trait_registry.validate_impl(impl_block) {
                return Err(CompileError::new(
                    &err,
                    0, 0, "",
                    ErrorCode::E0002,
                ));
            }
        }
        
        for func in functions {
            self.analyze_function(func)?;
        }
        
        Ok(())
    }

    // Check trait bounds for generics
    fn check_trait_bounds(&self, _type_params: &[String], where_clauses: &[WhereClause]) -> Result<()> {
        for clause in where_clauses {
            let trait_name = &clause.trait_name;
            
            if !self.trait_registry.trait_exists(trait_name) {
                return Err(CompileError::new(
                    &format!("Unknown trait '{}'", trait_name),
                    0, 0, "",
                    ErrorCode::E0004,
                ).with_suggestion(&format!("Define trait '{}' before using it as a constraint", trait_name)));
            }
            
            // For now, just check that the trait exists
            // Full type checking of bounds happens during monomorphization
        }
        Ok(())
    }
    
    // Resolve trait method call
    fn resolve_trait_method(&self, type_: &Type, method_name: &str) -> Option<FunctionDecl> {
        for ((_, _), _) in &self.trait_registry.impls {
        }
        self.trait_registry.resolve_method(type_, method_name).cloned()
    }
    
    // Check if type implements trait
    #[allow(dead_code)]
    fn check_trait_implementation(&self, type_: &Type, trait_name: &str) -> Result<()> {
        if !self.trait_registry.type_implements_trait(type_, trait_name) {
            return Err(CompileError::new(
                &format!("Type {} does not implement trait '{}'", type_, trait_name),
                0, 0, "",
                ErrorCode::E0002,
            ).with_suggestion(&format!(
                "Add 'impl {} for {}' with the required methods",
                trait_name, type_
            )));
        }
        Ok(())
    }
    
    fn register_builtin_functions(&mut self) {
        let math_functions = [
            ("Math.sqrt", vec![("x", Type::Float)], Type::Float),
            ("Math.pow", vec![("x", Type::Float), ("y", Type::Float)], Type::Float),
            ("Math.sin", vec![("x", Type::Float)], Type::Float),
            ("Math.cos", vec![("x", Type::Float)], Type::Float),
            ("Math.abs", vec![("x", Type::Float)], Type::Float),
            ("Math.floor", vec![("x", Type::Float)], Type::Float),
            ("Math.ceil", vec![("x", Type::Float)], Type::Float),
            ("Math.exp", vec![("x", Type::Float)], Type::Float),
            ("Math.log", vec![("x", Type::Float)], Type::Float),
            ("Math.tan", vec![("x", Type::Float)], Type::Float),
        ];
        
        for (name, params, return_type) in math_functions {
            self.functions.insert(
                name.to_string(),
                FunctionInfo {
                    params: params.into_iter().map(|(n, t)| (n.to_string(), t)).collect(),
                    return_type,
                }
            );
        }
        
        let string_functions = [
            ("String.length", vec![("s", Type::String)], Type::Int),
            ("String.concat", vec![("s1", Type::String), ("s2", Type::String)], Type::String),
            ("String.substring", vec![("s", Type::String), ("start", Type::Int), ("length", Type::Int)], Type::String),
            ("String.to_upper", vec![("s", Type::String)], Type::String),
            ("String.to_lower", vec![("s", Type::String)], Type::String),
        ];
        
        for (name, params, return_type) in string_functions {
            self.functions.insert(
                name.to_string(),
                FunctionInfo {
                    params: params.into_iter().map(|(n, t)| (n.to_string(), t)).collect(),
                    return_type,
                }
            );
        }
        
        let file_functions = [
            ("File.read", vec![("path", Type::String)], Type::String),
            ("File.write", vec![("path", Type::String), ("content", Type::String)], Type::Int),
            ("File.append", vec![("path", Type::String), ("content", Type::String)], Type::Int),
        ];
        
        for (name, params, return_type) in file_functions {
            self.functions.insert(
                name.to_string(),
                FunctionInfo {
                    params: params.into_iter().map(|(n, t)| (n.to_string(), t)).collect(),
                    return_type,
                }
            );
        }
        
        let list_functions = [
            ("List.length", vec![("arr", Type::list(Type::Unknown))], Type::Int),
            ("List.sum", vec![("arr", Type::list(Type::Unknown))], Type::Float),
            ("List.max", vec![("arr", Type::list(Type::Unknown))], Type::Float),
            ("List.min", vec![("arr", Type::list(Type::Unknown))], Type::Float),
        ];
        
        for (name, params, return_type) in list_functions {
            self.functions.insert(
                name.to_string(),
                FunctionInfo {
                    params: params.into_iter().map(|(n, t)| (n.to_string(), t)).collect(),
                    return_type,
                }
            );
        }
        
        self.functions.insert("alloc".to_string(), FunctionInfo {
            params: vec![("size".to_string(), Type::Int)],
            return_type: Type::pointer(Type::Unknown),
        });
        self.functions.insert("free".to_string(), FunctionInfo {
            params: vec![("ptr".to_string(), Type::pointer(Type::Unknown))],
            return_type: Type::Void,
        });
    }
    
    fn register_user_functions(&mut self, functions: &[FunctionDecl]) {
        for func in functions {
            let params = func.params.iter()
                .map(|(name, t)| {
                    let type_ = if let Some(type_param) = self.parse_type_param(t) {
                        Type::TypeVar(type_param)
                    } else {
                        Type::from_str(t)
                    };
                    (name.clone(), type_)
                })
                .collect();
            
            let return_type = func.return_type
                .as_deref()
                .map(|t| {
                    if let Some(type_param) = self.parse_type_param(t) {
                        Type::TypeVar(type_param)
                    } else {
                        Type::from_str(t)
                    }
                })
                .unwrap_or(Type::Void);
            
            let clean_name = func.name.trim_end_matches("()").to_string();
            self.functions.insert(
                clean_name,
                FunctionInfo { params, return_type }
            );
        }
    }

    fn parse_type_param(&self, type_str: &str) -> Option<String> {
        // Check if the type string is a type parameter (single uppercase letter)
        let trimmed = type_str.trim();
        if trimmed.len() == 1 && trimmed.chars().next().unwrap().is_uppercase() {
            Some(trimmed.to_string())
        } else {
            None
        }
    }

    fn analyze_function(&mut self, func: &FunctionDecl) -> Result<()> {
        if func.is_extern {
            return Ok(());
        }

        self.push_scope();

            // Check trait bounds on type parameters
        self.check_trait_bounds(&func.type_params, &func.where_clauses)?;
        
        // Register type parameters
        for type_param in &func.type_params {
            self.declare_type_param(type_param, Type::TypeVar(type_param.clone()));
        }
        
        // Register type constraints
        for clause in &func.where_clauses {
            self.declare_type_constraint(&clause.type_param, &clause.trait_name);
        }
        
        let return_type = self.resolve_type(
            &func.return_type.as_deref().map(Type::from_str).unwrap_or(Type::Void)
        );
        self.current_return_type = Some(return_type.clone());
        
        // Resolve parameter types with type variables
        for (name, type_str) in &func.params {
            let param_type = self.resolve_type(&Type::from_str(type_str));
            self.declare_variable(name, param_type, false)?;
        }
        
        for stmt in &func.body {
            self.analyze_stmt(stmt)?;
        }
        
        self.pop_scope();
        self.current_return_type = None;
        Ok(())
    }
    
    fn analyze_stmt(&mut self, stmt: &Stmt) -> Result<()> {
        match stmt {
            Stmt::VarDecl { name, value, type_annotation, mutable, .. } => {
                // Track list literals for bounds checking
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

                let value_type = self.analyze_expr(value)?;
                
                if let Some(annotated) = type_annotation {
                    let expected = Type::from_str(annotated);
                    if expected != Type::Unknown && !value_type.can_coerce_to(&expected) {
                        return Err(CompileError::new(
                            &format!(
                                "Type mismatch: variable '{}' declared as {} but assigned {}",
                                name, expected, value_type
                            ),
                            0, 0, "",
                            ErrorCode::E0002,
                        ).with_suggestion(&format!(
                            "Change the type annotation to {} or change the value to {}",
                            value_type, expected
                        )));
                    }
                }
                
                self.declare_variable(name, value_type.clone(), *mutable)?;
                
                if let Expr::Var(source, _) = value {
                    if source != name && !self.is_moved(source) {
                        if !value_type.is_copy() { self.mark_moved(source); }
                    }
                }
            }
            Stmt::Assign { name, value } => {
                let (var_type, mutable) = self.lookup_variable(name).ok_or_else(|| {
                    CompileError::new(
                        &format!("Undefined variable '{}'", name),
                        0, 0, "",
                        ErrorCode::E0003,
                    ).with_suggestion(&format!(
                        "Declare '{}' with 'var {} := ...' or 'val {} := ...'",
                        name, name, name
                    ))
                })?;
                
                if !mutable {
                    return Err(CompileError::new(
                        &format!("Cannot assign to immutable variable '{}'", name),
                        0, 0, "",
                        ErrorCode::E0005,
                    ).with_suggestion(&format!(
                        "Change 'val {}' to 'var {}' if you need to reassign it",
                        name, name
                    )));
                }
                
                let value_type = self.analyze_expr(value)?;
                if var_type != value_type && var_type != Type::Unknown && !value_type.can_coerce_to(&var_type) {
                    return Err(CompileError::new(
                        &format!(
                            "Type mismatch: cannot assign {} to variable of type {}",
                            value_type, var_type
                        ),
                        0, 0, "",
                        ErrorCode::E0002,
                    ));
                }
            }
            Stmt::Expression(expr) => {
                self.analyze_expr(expr)?;
            }
            Stmt::For { var, iterable, body, trailing_expr } => {
                let iter_type = self.analyze_expr(iterable)?;
                if let Type::List(element_type) = &iter_type {
                    self.push_scope();
                    self.declare_variable(var, *element_type.clone(), false)?;
                    for s in body {
                        self.analyze_stmt(s)?;
                    }
                    let _ = if let Some(expr) = trailing_expr {
                        self.analyze_expr(expr)?
                    } else {
                        Type::Void
                    };
                    self.pop_scope();
                } else if iter_type != Type::Unknown {
                    return Err(CompileError::new(
                        &format!("For loop requires list, found {}", iter_type),
                        0, 0, "",
                        ErrorCode::E0002,
                    ).with_suggestion("Use a list literal like [1, 2, 3]"));
                } else {
                    // Unknown iterable - still check body
                    self.push_scope();
                    self.declare_variable(var, Type::Unknown, false)?;
                    for s in body { self.analyze_stmt(s)?; }
                    if let Some(expr) = trailing_expr { self.analyze_expr(expr)?; }
                    self.pop_scope();
                }
            }
            Stmt::While { condition, body, trailing_expr } => {
                let cond_type = self.analyze_expr(condition)?;
                if cond_type != Type::Bool && cond_type != Type::Unknown {
                    return Err(CompileError::new(
                        &format!("Condition must be boolean, found {}", cond_type),
                        0, 0, "",
                        ErrorCode::E0002,
                    ));
                }
                
                self.push_scope();
                for s in body {
                    self.analyze_stmt(s)?;
                }
                let _ = if let Some(expr) = trailing_expr {
                    self.analyze_expr(expr)?
                } else {
                    Type::Void
                };
                self.pop_scope();
            }
            Stmt::Return { value } => {
                let expected_type = self.current_return_type.clone().unwrap_or(Type::Void);
                
                match (value, &expected_type) {
                    (Some(_expr), Type::Void) => {
                        return Err(CompileError::new(
                            "Cannot return a value from a void function",
                            0, 0, "",
                            ErrorCode::E0002,
                        ).with_suggestion("Remove the return value or change the function return type"));
                    }
                    (None, Type::Void) => {}
                    (Some(expr), expected) => {
                        let actual_type = self.analyze_expr(expr)?;
                        if !actual_type.can_coerce_to(expected) && *expected != Type::Unknown {
                            return Err(CompileError::new(
                                &format!(
                                    "Return type mismatch: expected {}, found {}",
                                    expected, actual_type
                                ),
                                0, 0, "",
                                ErrorCode::E0002,
                            ).with_suggestion(&format!(
                                "Change the return statement to match {} or change the function signature",
                                expected
                            )));
                        }
                    }
                    (None, expected) => {
                        return Err(CompileError::new(
                            &format!("Missing return value: function should return {}", expected),
                            0, 0, "",
                            ErrorCode::E0002,
                        ).with_suggestion("Add a return statement with the appropriate value"));
                    }
                }
            }
            Stmt::Print { expr } => {
                self.analyze_expr(expr)?;
            }
            Stmt::Break | Stmt::Continue => {}
            Stmt::Defer { stmt } => {
                self.analyze_stmt(stmt)?;
            }
            Stmt::Spawn { body } => {
                self.push_scope();
                for s in body {
                    self.analyze_stmt(s)?;
                }
                self.pop_scope();
            }
            Stmt::Parallel { blocks } => {
                for block in blocks {
                    self.push_scope();
                    for s in block {
                        self.analyze_stmt(s)?;
                    }
                    self.pop_scope();
                }
            }
            Stmt::ChannelDecl { name } => {
                self.declare_variable(name, Type::channel(Type::Unknown), false)?;
            }
            Stmt::Send { channel, value } => {
                let _ = self.lookup_variable(channel).ok_or_else(|| {
                    CompileError::new(
                        &format!("Undefined channel '{}'", channel),
                        0, 0, "",
                        ErrorCode::E0003,
                    )
                })?;
                self.analyze_expr(value)?;
            }
            Stmt::Receive { channel, target } => {
                let _ = self.lookup_variable(channel).ok_or_else(|| {
                    CompileError::new(
                        &format!("Undefined channel '{}'", channel),
                        0, 0, "",
                        ErrorCode::E0003,
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
                for s in body {
                    self.analyze_stmt(s)?;
                }
                self.pop_scope();
            }
            Stmt::RegionBlock { name: _, body } => {
                self.push_scope();
                for s in body {
                    self.analyze_stmt(s)?;
                }
                self.pop_scope();
            }
            Stmt::Import { .. } => {}
            Stmt::ArrayAssign { array, index, value } => {
                let (array_type, _) = self.lookup_variable(array).ok_or_else(|| {
                    CompileError::new(
                        &format!("Undefined array '{}'", array),
                        0, 0, "",
                        ErrorCode::E0003,
                    )
                })?;
                
                if let Type::List(_) = &array_type {
                } else if array_type != Type::Unknown {
                    return Err(CompileError::new(
                        &format!("Array assignment requires list, found {}", array_type),
                        0, 0, "",
                        ErrorCode::E0002,
                    ));
                }
                
                self.analyze_expr(index)?;
                self.analyze_expr(value)?;
            }
        }
        Ok(())
    }

    fn bind_pattern_variables(&mut self, pattern: &Pattern, value_type: &Type) {
        match pattern {
            Pattern::Some(var) => {
                // For Some(x) matching against Option<Inner>, x has type Inner
                if let Type::Option(inner) = value_type {
                    self.declare_variable(var, *inner.clone(), false).ok();
                } else {
                    self.declare_variable(var, Type::Unknown, false).ok();
                }
            }
            Pattern::SomeNested(inner) => {
                // For Some(Some(x)), bind inner pattern against inner type
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
            _ => {} // None, Wildcard, Literal, Range, ListDestructure don't bind variables
        }
    }
    
    fn analyze_expr(&mut self, expr: &Expr) -> Result<Type> {
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
                    self.check_borrow_rules(name, true)?;
                    self.mark_mutably_borrowed(name);
                    let inner_type = self.analyze_expr(expr)?;
                    return Ok(Type::mut_borrow(inner_type));
                }
                let inner_type = self.analyze_expr(expr)?;
                Ok(Type::mut_borrow(inner_type))
            }
            Expr::Deref { expr } => {
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
            Expr::None => Ok(Type::option(Type::Unknown)),
            Expr::Ok { value } => {
                let inner = self.analyze_expr(value)?;
                Ok(Type::result(inner, Type::Unknown))
            }
            Expr::Error { value } => {
                let inner = self.analyze_expr(value)?;
                Ok(Type::result(Type::Unknown, inner))
            }
            Expr::Block { statements, trailing_expr } => {
                self.push_scope();
                for s in statements {
                    self.analyze_stmt(s)?;
                }
                let result = if let Some(expr) = trailing_expr {
                    self.analyze_expr(expr)?
                } else {
                    Type::Void
                };
                self.pop_scope();
                Ok(result)
            }
            Expr::If { condition, then_branch, else_branch } => {
                let cond_type = self.analyze_expr(condition)?;
                // Allow Void condition (represents else without condition)
                if cond_type != Type::Bool && cond_type != Type::Unknown && cond_type != Type::Void {
                    return Err(CompileError::new("If condition must be Bool", 0, 0, "", ErrorCode::E0002));
                }
                let then_type = self.analyze_expr(then_branch)?;
                if let Some(else_expr) = else_branch {
                    let else_type = self.analyze_expr(else_expr)?;
                    Ok(then_type.common_supertype(&else_type))
                } else {
                    Ok(Type::Void)
                }
            }
            Expr::Match { value, cases } => {
                let _value_type = self.analyze_expr(value)?;
                
                if let Some(first_case) = cases.first() {
                    // NEW: Bind pattern variables for first case
                    self.push_scope();
                    self.bind_pattern_variables(&first_case.pattern, &_value_type);
                    
                    // Analyze pattern guard
                    if let Pattern::Guarded { condition, .. } = &first_case.pattern {
                        let cond_type = self.analyze_expr(condition)?;
                        if cond_type != Type::Bool && cond_type != Type::Unknown {
                            return Err(CompileError::new(
                                "Pattern guard must be boolean",
                                0, 0, "",
                                ErrorCode::E0002,
                            ));
                        }
                    }
                    
                    let first_type = self.analyze_expr(&first_case.body)?;
                    self.pop_scope();
                    
                    let mut result_type = first_type.clone();
                    for case in &cases[1..] {
                        // Bind pattern variables for each case
                        self.push_scope();
                        self.bind_pattern_variables(&case.pattern, &_value_type);
                        
                        // Analyze pattern guard
                        if let Pattern::Guarded { condition, .. } = &case.pattern {
                            let cond_type = self.analyze_expr(condition)?;
                            if cond_type != Type::Bool && cond_type != Type::Unknown {
                                return Err(CompileError::new(
                                    "Pattern guard must be boolean",
                                    0, 0, "",
                                    ErrorCode::E0002,
                                ));
                            }
                        }
                        
                        let case_type = self.analyze_expr(&case.body)?;
                        self.pop_scope();
                        result_type = result_type.common_supertype(&case_type);
                    }
                    Ok(result_type)
                } else {
                    Ok(Type::Unknown)
                }
            }
            Expr::TryCatch { try_branch, catch_var: _, catch_branch, finally_body: _ } => {
                let try_type = self.analyze_expr(try_branch)?;
                let catch_type = self.analyze_expr(catch_branch)?;
                Ok(try_type.common_supertype(&catch_type))
            }
            // --- ORTHOGONAL: For/While as expressions ---
            Expr::For { var, iterable, body, trailing_expr, span } => {
                let iter_type = self.analyze_expr(iterable)?;
                let elem_type = if let Type::List(t) = iter_type.clone() {
                    *t
                } else if iter_type != Type::Unknown {
                    return Err(CompileError::new(
                        &format!("For loop requires list, found {}", iter_type),
                        span.0, span.1, "",
                        ErrorCode::E0002,
                    ));
                } else {
                    Type::Unknown
                };
                self.push_scope();
                self.declare_variable(var, elem_type, false)?;
                for s in body {
                    self.analyze_stmt(s)?;
                }
                let result_type = if let Some(expr) = trailing_expr {
                    self.analyze_expr(expr)?
                } else {
                    Type::Void
                };
                self.pop_scope();
                Ok(result_type)
            }
            Expr::While { condition, body, trailing_expr, span } => {
                let cond_type = self.analyze_expr(condition)?;
                if cond_type != Type::Bool && cond_type != Type::Unknown {
                    return Err(CompileError::new(
                        &format!("While condition must be Bool, found {}", cond_type),
                        span.0, span.1, "",
                        ErrorCode::E0002,
                    ));
                }
                self.push_scope();
                for s in body {
                    self.analyze_stmt(s)?;
                }
                let result_type = if let Some(expr) = trailing_expr {
                    self.analyze_expr(expr)?
                } else {
                    Type::Void
                };
                self.pop_scope();
                Ok(result_type)
            }
            Expr::Var(name, span) => {
                let (line, column) = *span;
                if self.is_moved(name) {
                    return Err(CompileError::new(
                        &format!("Use of moved variable '{}'", name),
                        line, column, "",
                        ErrorCode::E0007,
                    ).with_suggestion("Variable ownership was transferred and cannot be used in this scope"));
                }
                if self.is_mutably_borrowed(name) {
                    return Err(CompileError::new(
                        &format!("Cannot read '{}' while mutably borrowed", name),
                        line, column, "",
                        ErrorCode::E0007,
                    ).with_suggestion("Wait for the mutable borrow to end"));
                }
                
                let result = self.lookup_variable(name)
                    .map(|(t, _)| t)
                    .ok_or_else(|| CompileError::new(
                        &format!("Undefined variable '{}'", name),
                        line, column, "",
                        ErrorCode::E0003,
                    ).with_suggestion(&format!(
                        "Declare '{}' with 'var {} := ...' or 'val {} := ...' in this scope",
                        name, name, name
                    )));
                
                if name == "self" || name == "other" {
                }
                
                result
            }
            Expr::ArrayAccess { array, index } => {
                let array_type = self.analyze_expr(array)?;
                let element_type = match array_type {
                    Type::List(element_type) => *element_type,
                    Type::Unknown => Type::Unknown,
                    _ => {
                        return Err(CompileError::new(
                            &format!("Array access requires list, found {}", array_type),
                            0, 0, "",
                            ErrorCode::E0002,
                        ));
                    }
                };
                let index_type = self.analyze_expr(index)?;

                // --- Bounds checking for constant indices ---
                let mut out_of_bounds: Option<(i64, usize, String)> = None;

                // Extract literal index value if it's Int
                let literal_index: Option<i64> = match index.as_ref() {
                    Expr::Int(v) => Some(*v),
                    Expr::Number(f) => Some(*f as i64),
                    _ => None,
                };

                if let Some(idx_val) = literal_index {
                    // Case 1: arr is a variable with known length
                    if let Expr::Var(var_name, _) = array.as_ref() {
                        if let Some(list_len) = self.lookup_list_length(var_name) {
                            if idx_val < 0 || (idx_val as usize) >= list_len {
                                out_of_bounds = Some((idx_val, list_len, var_name.clone()));
                            }
                        }
                    }
                    // Case 2: arr is a direct list literal [1.0, 2.0, 3.0][10]
                    if let Expr::List(elements) = array.as_ref() {
                        let list_len = elements.len();
                        if idx_val < 0 || (idx_val as usize) >= list_len {
                            out_of_bounds = Some((idx_val, list_len, "list literal".to_string()));
                        }
                    }
                }

                if let Some((idx_val, len, var_name)) = out_of_bounds {
                    return Err(CompileError::new(
                        &format!(
                            "Array index out of bounds: index {} is out of bounds for '{}' with length {}",
                            idx_val, var_name, len
                        ),
                        0, 0, "",
                        ErrorCode::E0004,
                    ).with_suggestion(&format!(
                        "Valid indices are 0..{} for array of length {}",
                        len - 1,
                        len
                    )));
                }

                if index_type != Type::Int && index_type != Type::Unknown {
                    return Err(CompileError::new(
                        &format!("Array index must be Int, found {}", index_type),
                        0, 0, "",
                        ErrorCode::E0002,
                    ));
                }
                Ok(element_type)
            }
            Expr::Binary { left, op, right } => {
                let left_type = self.analyze_expr(left)?;
                let right_type = self.analyze_expr(right)?;
                match op {
                    BinOp::Add => {
                        if left_type == Type::String && right_type == Type::String {
                            Ok(Type::String)
                        } else if left_type.is_numeric() && right_type.is_numeric() {
                            Ok(left_type.common_supertype(&right_type))
                        } else {
                            Err(CompileError::new(
                                &format!("Addition requires matching types, found {} and {}", left_type, right_type),
                                0, 0, "", ErrorCode::E0002,
                            ).with_suggestion("Use matching types or add type conversion"))
                        }
                    }
                    BinOp::Subtract | BinOp::Multiply | BinOp::Divide => {
                        if left_type.is_numeric() && right_type.is_numeric() {
                            Ok(left_type.common_supertype(&right_type))
                        } else {
                            Err(CompileError::new(
                                &format!("Arithmetic requires numeric types, found {} and {}", left_type, right_type),
                                0, 0, "", ErrorCode::E0002,
                            ).with_suggestion("Both operands must be numeric (Int or Float)"))
                        }
                    }
                    BinOp::Greater | BinOp::Less | BinOp::GreaterEqual | BinOp::LessEqual => {
                        if left_type.is_numeric() && right_type.is_numeric() {
                            Ok(Type::Bool)
                        } else {
                            Err(CompileError::new(
                                &format!("Comparison requires numeric types, found {} and {}", left_type, right_type),
                                0, 0, "", ErrorCode::E0002,
                            ).with_suggestion("Use numeric types for comparison"))
                        }
                    }
                    BinOp::Equal | BinOp::NotEqual => {
                        if left_type == right_type || (left_type.is_numeric() && right_type.is_numeric()) {
                            Ok(Type::Bool)
                        } else {
                            Err(CompileError::new(
                                &format!("Equality requires matching types, found {} and {}", left_type, right_type),
                                0, 0, "", ErrorCode::E0002,
                            ).with_suggestion("Use matching types for equality comparison"))
                        }
                    }
                    BinOp::And | BinOp::Or => {
                        if left_type == Type::Bool && right_type == Type::Bool {
                            Ok(Type::Bool)
                        } else {
                            Err(CompileError::new(
                                &format!("Logical operators require boolean operands, found {} and {}", left_type, right_type),
                                0, 0, "", ErrorCode::E0002,
                            ).with_suggestion("Use 'and' and 'or' only with boolean values"))
                        }
                    }
                }
            }
            Expr::FunctionCall { name, args, .. } => {
                let clean_name = name.trim_end_matches("()");
                // Check if this is a method call (contains a dot)
                if clean_name.contains('.') {
                    let parts: Vec<&str> = clean_name.split('.').collect();
                    if parts.len() == 2 {
                        let receiver = parts[0];
                        let method_name = parts[1];
                        
                        // Look up the receiver type
                        if let Some((receiver_type, _)) = self.lookup_variable(receiver) {
                            // Resolve the method using trait registry
                            if let Some(method) = self.resolve_trait_method(&receiver_type, method_name) {
                                // Check arguments
                                if args.len() != method.params.len() {
                                    return Err(CompileError::new(
                                        &format!("Method '{}' expects {} arguments, got {}", method_name, method.params.len(), args.len()),
                                        0, 0, "", ErrorCode::E0002,
                                    ));
                                }
                                
                                // Analyze arguments
                                for (arg, (param_name, param_type)) in args.iter().zip(&method.params) {
                                    let arg_type = self.analyze_expr(arg)?;
                                    let expected_type = Type::from_str(param_type);
                                    if !arg_type.can_coerce_to(&expected_type) && expected_type != Type::Unknown {
                                        return Err(CompileError::new(
                                            &format!("Argument '{}' type mismatch: expected {}, found {}", param_name, expected_type, arg_type),
                                            0, 0, "", ErrorCode::E0002,
                                        ));
                                    }
                                }
                                
                                // Return the method's return type
                                return Ok(method.return_type
                                    .as_deref()
                                    .map(Type::from_str)
                                    .unwrap_or(Type::Void));
                            } else {
                                return Err(CompileError::new(
                                    &format!("Type {} does not have method '{}'", receiver_type, method_name),
                                    0, 0, "", ErrorCode::E0004,
                                ).with_suggestion(&format!(
                                    "Implement a trait for {} that provides method '{}'",
                                    receiver_type, method_name
                                )));
                            }
                        }
                    }
                }
                let func_info = self.functions.get(clean_name).cloned().ok_or_else(|| {
                    CompileError::new(
                        &format!("Undefined function '{}'", name),
                        0, 0, "",
                        ErrorCode::E0004,
                    ).with_suggestion(&format!("Check if function '{}' is defined or imported", name))
                })?;
                
                // Check argument count
                if args.len() != func_info.params.len() {
                    return Err(CompileError::new(
                        &format!("Function '{}' expects {} arguments, got {}", name, func_info.params.len(), args.len()),
                        0, 0, "", ErrorCode::E0002,
                    ).with_suggestion(&format!("Provide exactly {} argument(s) to '{}'", func_info.params.len(), name)));
                }
                
                // Analyze arguments and unify type variables
                let mut type_bindings: HashMap<String, Type> = HashMap::new();
                
                for (arg, (param_name, param_type)) in args.iter().zip(&func_info.params) {
                    let arg_type = self.analyze_expr(arg)?;
                    
                    // Resolve param type (may be TypeVar)
                    let resolved_param_type = self.resolve_type(param_type);
                    
                    // If param is a TypeVar, bind it to the argument type
                    if let Type::TypeVar(tv) = &resolved_param_type {
                        if let Some(existing_binding) = type_bindings.get(tv) {
                            // Type variable already bound - check consistency
                            if existing_binding != &arg_type && existing_binding != &Type::Unknown {
                                return Err(CompileError::new(
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
                    } else if !arg_type.can_coerce_to(&resolved_param_type) && resolved_param_type != Type::Unknown {
                        return Err(CompileError::new(
                            &format!("Argument '{}' type mismatch: expected {}, found {}", param_name, resolved_param_type, arg_type),
                            0, 0, "", ErrorCode::E0002,
                        ).with_suggestion(&format!("Convert the argument to {} or change the function signature", resolved_param_type)));
                    }
                }
                
                // Resolve return type with bindings
                let return_type = self.substitute_type_vars(&func_info.return_type, &type_bindings);
                Ok(return_type)
            }
            Expr::PtrLiteral(_) => {
                Ok(Type::Ptr)
            }

            Expr::NullPtr => {
                Ok(Type::Ptr)
            }

            Expr::Cast { expr: cast_expr, target_type } => {
                // Analyze the source expression
                let _source_type = self.analyze_expr(cast_expr)?;
                // Return the target type
                Ok(Type::from_str(target_type))
            }
        }
    }

    fn substitute_type_vars(&self, type_: &Type, bindings: &HashMap<String, Type>) -> Type {
        match type_ {
            Type::TypeVar(name) => {
                bindings.get(name).cloned().unwrap_or_else(|| type_.clone())
            }
            Type::List(inner) => {
                Type::list(self.substitute_type_vars(inner, bindings))
            }
            Type::Option(inner) => {
                Type::option(self.substitute_type_vars(inner, bindings))
            }
            Type::Result { ok, error } => {
                Type::result(
                    self.substitute_type_vars(ok, bindings),
                    self.substitute_type_vars(error, bindings)
                )
            }
            _ => type_.clone(),
        }
    }
    
    fn declare_variable(&mut self, name: &str, type_: Type, mutable: bool) -> Result<()> {
        if let Some(scope) = self.scopes.last_mut() {
            if scope.contains_key(name) {
                return Err(CompileError::new(
                    &format!("Variable '{}' already declared", name),
                    0, 0, "",
                    ErrorCode::E0003,
                ));
            }
            scope.insert(name.to_string(), (type_, mutable));
        }
        Ok(())
    }
    
    fn lookup_variable(&self, name: &str) -> Option<(Type, bool)> {
        for scope in self.scopes.iter().rev() {
            if let Some((t, m)) = scope.get(name) {
                return Some((t.clone(), *m));
            }
        }
        None
    }

    fn declare_type_param(&mut self, name: &str, type_: Type) {
        if let Some(scope) = self.type_params.last_mut() {
            scope.insert(name.to_string(), type_);
        }
    }

    fn lookup_type_param(&self, name: &str) -> Option<Type> {
        for scope in self.type_params.iter().rev() {
            if let Some(t) = scope.get(name) {
                return Some(t.clone());
            }
        }
        None
    }

    fn declare_type_constraint(&mut self, type_param: &str, trait_name: &str) {
        if let Some(scope) = self.type_constraints.last_mut() {
            scope.entry(type_param.to_string())
                .or_default()
                .push(trait_name.to_string());
        }
    }

    fn resolve_type(&self, type_: &Type) -> Type {
        match type_ {
            Type::TypeVar(name) => {
                self.lookup_type_param(name).unwrap_or_else(|| Type::TypeVar(name.clone()))
            }
            Type::List(inner) => {
                Type::list(self.resolve_type(inner))
            }
            Type::Option(inner) => {
                Type::option(self.resolve_type(inner))
            }
            Type::Result { ok, error } => {
                Type::result(
                    self.resolve_type(ok),
                    self.resolve_type(error)
                )
            }
            _ => type_.clone(),
        }
    }
}