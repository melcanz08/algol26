// src/sematic.rs

use std::collections::{HashMap, HashSet};
use crate::ast::{Expr, FunctionDecl, Stmt, BinOp};
#[allow(unused_imports)]
use crate::ast::{MatchCase, Pattern};
use crate::diagnostics::{CompileError, ErrorCode, Result};

pub struct SemanticAnalyzer {
    scopes: Vec<HashMap<String, (Type, bool)>>,
    moved_vars: Vec<Vec<String>>,  // Stack of moved variables per scope
    borrowed_vars: Vec<HashSet<String>>,  // Immutable borrows per scope
    mutably_borrowed: Vec<HashSet<String>>,  // Mutable borrows per scope
    functions: HashMap<String, FunctionInfo>,
    current_return_type: Option<Type>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Type {
    Int,
    Float,
    String,
    Bool,
    List,
    Void,
    Unknown,
    Option(Box<Type>),
    Result(Box<Type>, Box<Type>),
    Borrow(Box<Type>),
    MutBorrow(Box<Type>),
    Pointer(Box<Type>),
}

impl Type {
    fn from_str(s: &str) -> Self {
        match s {
            "int" => Type::Int,
            "float" => Type::Float,
            "string" => Type::String,
            "bool" => Type::Bool,
            "void" => Type::Void,
            "&float" => Type::Borrow(Box::new(Type::Float)),
            "&int" => Type::Borrow(Box::new(Type::Int)),
            "&string" => Type::Borrow(Box::new(Type::String)),
            "&bool" => Type::Borrow(Box::new(Type::Bool)),
            "&mut float" => Type::MutBorrow(Box::new(Type::Float)),
            "&mut int" => Type::MutBorrow(Box::new(Type::Int)),
            "*float" => Type::Pointer(Box::new(Type::Float)),
            "*int" => Type::Pointer(Box::new(Type::Int)),
            "*string" => Type::Pointer(Box::new(Type::String)),
            "*bool" => Type::Pointer(Box::new(Type::Bool)),
            _ => Type::Unknown,
        }
    }
    
    fn name(&self) -> String {
        match self {
            Type::Int => "Int".to_string(),
            Type::Float => "Float".to_string(),
            Type::String => "String".to_string(),
            Type::Bool => "Bool".to_string(),
            Type::List => "List".to_string(),
            Type::Void => "Void".to_string(),
            Type::Unknown => "Unknown".to_string(),
            Type::Option(t) => format!("Option<{}>", t.name().as_str()),
            Type::Result(t, e) => format!("Result<{}, {}>", t.name().as_str(), e.name().as_str()),
            Type::Borrow(t) => format!("&{}", t.name().as_str()),
            Type::MutBorrow(t) => format!("&mut {}", t.name().as_str()),
            Type::Pointer(t) => format!("*{}", t.name().as_str()),
        }
    }
}

#[derive(Debug, Clone)]
struct FunctionInfo {
    params: Vec<(String, Type)>,
    return_type: Type,
}

impl SemanticAnalyzer {
    #[allow(dead_code)]
    pub fn new() -> Self {
        SemanticAnalyzer {
            scopes: vec![HashMap::new()],
            functions: HashMap::new(),
            moved_vars: vec![Vec::new()],  // Start with one scope
            borrowed_vars: vec![HashSet::new()],
            mutably_borrowed: vec![HashSet::new()],
            current_return_type: None,
        }
    }

    fn is_numeric(&self, type_: &Type) -> bool {
        matches!(type_, Type::Int | Type::Float)
    }

    fn push_scope(&mut self) {
        self.scopes.push(HashMap::new());
        self.moved_vars.push(Vec::new());
        self.borrowed_vars.push(HashSet::new());
        self.mutably_borrowed.push(HashSet::new());
    }

    fn pop_scope(&mut self) {
        self.scopes.pop();
        self.moved_vars.pop();
        self.borrowed_vars.pop();
        self.mutably_borrowed.pop();
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
    
    #[allow(dead_code)]
    fn mark_borrowed(&mut self, name: &str) {
        if let Some(scope) = self.borrowed_vars.last_mut() {
            scope.insert(name.to_string());
        }
    }
    
    #[allow(dead_code)]
    fn mark_mutably_borrowed(&mut self, name: &str) {
        if let Some(scope) = self.mutably_borrowed.last_mut() {
            scope.insert(name.to_string());
        }
    }
    
    #[allow(dead_code)]
    fn is_borrowed(&self, name: &str) -> bool {
        // Only check current scope (borrows end when scope ends)
        self.borrowed_vars.last().map(|scope| scope.contains(name)).unwrap_or(false)
    }
    
    fn is_mutably_borrowed(&self, name: &str) -> bool {
        // Only check current scope (mutable borrows end when scope ends)
        self.mutably_borrowed.last().map(|scope| scope.contains(name)).unwrap_or(false)
    }
    
    #[allow(dead_code)]
    fn check_borrow_rules(&self, name: &str, mutable: bool) -> Result<()> {
        // Rule 1: Can't borrow a moved variable
        if self.is_moved(name) {
            return Err(CompileError::new(
                &format!("Cannot borrow moved variable '{}'", name),
                0, 0, "",
                ErrorCode::E0007,
            ).with_suggestion("The variable has been moved and is no longer available"));
        }
        
        // Rule 2: Can't mutably borrow twice
        if mutable && self.is_mutably_borrowed(name) {
            return Err(CompileError::new(
                &format!("Cannot mutably borrow '{}' more than once", name),
                0, 0, "",
                ErrorCode::E0007,
            ).with_suggestion("Only one mutable borrow is allowed at a time"));
        }
        
        // Rule 3: Can't read while mutably borrowed
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
        // Register user-defined functions first (including extern)
        for func in functions {
            let params = func.params.iter()
                .map(|(name, t)| (name.clone(), Type::from_str(t)))
                .collect();
            
            let return_type = func.return_type
                .as_deref()
                .map(Type::from_str)
                .unwrap_or(Type::Void);
            
            // Strip parentheses from function name if present
            let clean_name = func.name.trim_end_matches("()").to_string();
            self.functions.insert(
                clean_name,
                FunctionInfo { params, return_type }
            );
        }
        
        // Register Math functions as known functions
        self.functions.insert("Math.sqrt".to_string(), FunctionInfo {
            params: vec![("x".to_string(), Type::Float)],
            return_type: Type::Float,
        });
        self.functions.insert("Math.pow".to_string(), FunctionInfo {
            params: vec![("x".to_string(), Type::Float), ("y".to_string(), Type::Float)],
            return_type: Type::Float,
        });
        self.functions.insert("Math.sin".to_string(), FunctionInfo {
            params: vec![("x".to_string(), Type::Float)],
            return_type: Type::Float,
        });
        self.functions.insert("Math.cos".to_string(), FunctionInfo {
            params: vec![("x".to_string(), Type::Float)],
            return_type: Type::Float,
        });
        self.functions.insert("Math.abs".to_string(), FunctionInfo {
            params: vec![("x".to_string(), Type::Float)],
            return_type: Type::Float,
        });
        self.functions.insert("Math.floor".to_string(), FunctionInfo {
            params: vec![("x".to_string(), Type::Float)],
            return_type: Type::Float,
        });
        self.functions.insert("Math.ceil".to_string(), FunctionInfo {
            params: vec![("x".to_string(), Type::Float)],
            return_type: Type::Float,
        });
        self.functions.insert("Math.exp".to_string(), FunctionInfo {
            params: vec![("x".to_string(), Type::Float)],
            return_type: Type::Float,
        });
        self.functions.insert("Math.log".to_string(), FunctionInfo {
            params: vec![("x".to_string(), Type::Float)],
            return_type: Type::Float,
        });
        self.functions.insert("Math.tan".to_string(), FunctionInfo {
            params: vec![("x".to_string(), Type::Float)],
            return_type: Type::Float,
        });
        
        // Register String functions
        self.functions.insert("String.length".to_string(), FunctionInfo {
            params: vec![("s".to_string(), Type::String)],
            return_type: Type::Int,
        });
        self.functions.insert("String.concat".to_string(), FunctionInfo {
            params: vec![("s1".to_string(), Type::String), ("s2".to_string(), Type::String)],
            return_type: Type::String,
        });
        self.functions.insert("String.substring".to_string(), FunctionInfo {
            params: vec![
                ("s".to_string(), Type::String),
                ("start".to_string(), Type::Int),
                ("length".to_string(), Type::Int)
            ],
            return_type: Type::String,
        });
        self.functions.insert("String.to_upper".to_string(), FunctionInfo {
            params: vec![("s".to_string(), Type::String)],
            return_type: Type::String,
        });
        self.functions.insert("String.to_lower".to_string(), FunctionInfo {
            params: vec![("s".to_string(), Type::String)],
            return_type: Type::String,
        });
        
        // Register File functions
        self.functions.insert("File.read".to_string(), FunctionInfo {
            params: vec![("path".to_string(), Type::String)],
            return_type: Type::String,
        });
        self.functions.insert("File.write".to_string(), FunctionInfo {
            params: vec![("path".to_string(), Type::String), ("content".to_string(), Type::String)],
            return_type: Type::Int,
        });
        self.functions.insert("File.append".to_string(), FunctionInfo {
            params: vec![("path".to_string(), Type::String), ("content".to_string(), Type::String)],
            return_type: Type::Int,
        });
        
        // Register Raw memory functions
        self.functions.insert("alloc".to_string(), FunctionInfo {
            params: vec![("size".to_string(), Type::Int)],
            return_type: Type::Pointer(Box::new(Type::Unknown)),
        });
        self.functions.insert("free".to_string(), FunctionInfo {
            params: vec![("ptr".to_string(), Type::Pointer(Box::new(Type::Unknown)))],
            return_type: Type::Void,
        });
        
        // Register List functions
        self.functions.insert("List.length".to_string(), FunctionInfo {
            params: vec![("arr".to_string(), Type::List)],
            return_type: Type::Int,
        });
        self.functions.insert("List.sum".to_string(), FunctionInfo {
            params: vec![("arr".to_string(), Type::List)],
            return_type: Type::Float,
        });
        self.functions.insert("List.max".to_string(), FunctionInfo {
            params: vec![("arr".to_string(), Type::List)],
            return_type: Type::Float,
        });
        self.functions.insert("List.min".to_string(), FunctionInfo {
            params: vec![("arr".to_string(), Type::List)],
            return_type: Type::Float,
        });
        
        for func in functions {
            let params = func.params.iter()
                .map(|(name, t)| (name.clone(), Type::from_str(t)))
                .collect();
            
            let return_type = func.return_type
                .as_deref()
                .map(Type::from_str)
                .unwrap_or(Type::Void);
            
            // Strip parentheses from function name if present
            let clean_name = func.name.trim_end_matches("()").to_string();
            self.functions.insert(
                clean_name,
                FunctionInfo { params, return_type }
            );
        }
        

        for func in functions {
            self.analyze_function(func)?;
        }
        
        Ok(())
    }

    fn analyze_function(&mut self, func: &FunctionDecl) -> Result<()> {
        if func.is_extern {
            // External functions don't have bodies to analyze
            return Ok(());
        }

        self.push_scope();
        // Set current return type
        let return_type = func.return_type
            .as_deref()
            .map(Type::from_str)
            .unwrap_or(Type::Void);
        self.current_return_type = Some(return_type.clone());
        
        for (name, type_str) in &func.params {
            self.declare_variable(name, Type::from_str(type_str), false)?;
        }
        
        for stmt in &func.body {
            self.analyze_stmt(stmt)?;
        }
        
        self.pop_scope();
        self.current_return_type = None;
        Ok(())
    }
    
    #[allow(unused_variables)]
    fn analyze_stmt(&mut self, stmt: &Stmt) -> Result<()> {
        match stmt {
            Stmt::VarDecl { name, value, type_annotation, mutable } => {
                let value_type = self.analyze_expr(value)?;
                
                if let Some(annotated) = type_annotation {
                    let expected = Type::from_str(annotated);
                    if expected != Type::Unknown && expected != value_type {
                        return Err(CompileError::new(
                            &format!(
                                "Type mismatch: variable '{}' declared as {} but assigned {}",
                                name, expected.name().as_str(), value_type.name().as_str()
                            ),
                            0, 0, "",
                            ErrorCode::E0002,
                        ).with_suggestion(&format!(
                            "Change the type annotation to {} or change the value to {}",
                            value_type.name().as_str(), expected.name().as_str()
                        )));
                    }
                }
                
                self.declare_variable(name, value_type, *mutable)?;
                
                // Mark source as moved AFTER declaring target
                if let Expr::Var(source) = value {
                    if source != name && !self.is_moved(source) {
                        self.mark_moved(source);
                    }
                }
            }
            Stmt::Assign { name, value } => {
                // Note: Simple assignments (a := b) are copies, not moves
                // Move semantics only apply to variable declarations (var y := x)
                
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
                if var_type != value_type && var_type != Type::Unknown {
                    return Err(CompileError::new(
                        &format!(
                            "Type mismatch: cannot assign {} to variable of type {}",
                            value_type.name().as_str(), var_type.name().as_str()
                        ),
                        0, 0, "",
                        ErrorCode::E0002,
                    ));
                }
            }
            Stmt::If { condition, then_body, else_body } => {
                let cond_type = self.analyze_expr(condition)?;
                if cond_type != Type::Bool && cond_type != Type::Unknown {
                    return Err(CompileError::new(
                        &format!("Condition must be boolean, found {}", cond_type.name().as_str()),
                        0, 0, "",
                        ErrorCode::E0002,
                    ).with_suggestion("Use a comparison like 'x > 5' or a boolean variable"));
                }
                
                self.push_scope();
                for s in then_body {
                    self.analyze_stmt(s)?;
                }
                self.pop_scope();
                
                if let Some(else_body) = else_body {
                    self.push_scope();
                    for s in else_body {
                        self.analyze_stmt(s)?;
                    }
                    self.pop_scope();
                }
            }
            Stmt::For { var, iterable, body } => {
                let iter_type = self.analyze_expr(iterable)?;
                if iter_type != Type::List && iter_type != Type::Unknown {
                    return Err(CompileError::new(
                        &format!("For loop requires list, found {}", iter_type.name().as_str()),
                        0, 0, "",
                        ErrorCode::E0002,
                    ).with_suggestion("Use a list literal like [1, 2, 3]"));
                }
                
                self.push_scope();
                self.declare_variable(var, Type::Float, false)?;
                for s in body {
                    self.analyze_stmt(s)?;
                }
                self.pop_scope();
            }
            Stmt::While { condition, body } => {
                let cond_type = self.analyze_expr(condition)?;
                if cond_type != Type::Bool && cond_type != Type::Unknown {
                    return Err(CompileError::new(
                        &format!("Condition must be boolean, found {}", cond_type.name().as_str()),
                        0, 0, "",
                        ErrorCode::E0002,
                    ));
                }
                
                self.push_scope();
                for s in body {
                    self.analyze_stmt(s)?;
                }
                self.pop_scope();
            }
            Stmt::Return { value } => {
                let expected_type = self.current_return_type.clone().unwrap_or(Type::Void);
                
                match (value, &expected_type) {
                    (Some(expr), Type::Void) => {
                        return Err(CompileError::new(
                            "Cannot return a value from a void function",
                            0, 0, "",
                            ErrorCode::E0002,
                        ).with_suggestion("Remove the return value or change the function return type"));
                    }
                    (None, Type::Void) => {
                        // Valid: returning nothing from void function
                    }
                    (Some(expr), expected) => {
                        let actual_type = self.analyze_expr(expr)?;
                        if actual_type != *expected && *expected != Type::Unknown {
                            return Err(CompileError::new(
                                &format!(
                                    "Return type mismatch: expected {}, found {}",
                                    expected.name(), actual_type.name()
                                ),
                                0, 0, "",
                                ErrorCode::E0002,
                            ).with_suggestion(&format!(
                                "Change the return statement to match {} or change the function signature",
                                expected.name()
                            )));
                        }
                    }
                    (None, expected) => {
                        return Err(CompileError::new(
                            &format!("Missing return value: function should return {}", expected.name()),
                            0, 0, "",
                            ErrorCode::E0002,
                        ).with_suggestion("Add a return statement with the appropriate value"));
                    }
                }
            }
            Stmt::Print { expr } => {
                self.analyze_expr(expr)?;
            }
            Stmt::FunctionCall { name, args } => {
                let clean_name = name.trim_end_matches("()");
                let func_info = self.functions.get(clean_name).cloned().ok_or_else(|| {
                    CompileError::new(
                        &format!("Undefined function '{}'", name),
                        0, 0, "",
                        ErrorCode::E0004,
                    )
                })?;
                
                if args.len() != func_info.params.len() {
                    return Err(CompileError::new(
                        &format!(
                            "Function '{}' expects {} arguments, got {}",
                            name, func_info.params.len(), args.len()
                        ),
                        0, 0, "",
                        ErrorCode::E0002,
                    ));
                }
                
                for (arg, (_, param_type)) in args.iter().zip(&func_info.params) {
                    let arg_type = self.analyze_expr(arg)?;
                    if arg_type != *param_type && *param_type != Type::Unknown {
                        return Err(CompileError::new(
                            &format!(
                                "Argument type mismatch: expected {}, found {}",
                                param_type.name().as_str(), arg_type.name().as_str()
                            ),
                            0, 0, "",
                            ErrorCode::E0002,
                        ));
                    }
                }
            }
            Stmt::Match { value, cases } => {
                let value_type = self.analyze_expr(value)?;
                
                for case in cases {
                    self.push_scope();
                    
                    // Infer the inner type from the matched expression
                    let inner_type = match &value_type {
                        Type::Option(inner) => *inner.clone(),
                        Type::Result(ok_type, err_type) => {
                            match &case.pattern {
                                Pattern::Ok(_) => *ok_type.clone(),
                                Pattern::Error(_) => *err_type.clone(),
                                _ => Type::Unknown,
                            }
                        }
                        _ => Type::Unknown,
                    };
                    
                    // Bind pattern variables with the inferred type
                    match &case.pattern {
                        Pattern::Some(var) => {
                            self.declare_variable(var, inner_type.clone(), false)?;
                        }
                        Pattern::None => {}
                        Pattern::Ok(var) => {
                            self.declare_variable(var, inner_type.clone(), false)?;
                        }
                        Pattern::Error(var) => {
                            self.declare_variable(var, inner_type.clone(), false)?;
                        }
                        Pattern::Wildcard => {}
                        Pattern::Literal(_) => {
                            // Validate literal matches the expected type
                            if let Pattern::Literal(lit) = &case.pattern {
                                let lit_type = self.analyze_expr(lit)?;
                                if lit_type != value_type && value_type != Type::Unknown {
                                    return Err(CompileError::new(
                                        &format!(
                                            "Pattern literal type {} doesn't match value type {}",
                                            lit_type.name(), value_type.name()
                                        ),
                                        0, 0, "",
                                        ErrorCode::E0002,
                                    ));
                                }
                            }
                        }
                    }
                    
                    for s in &case.body {
                        self.analyze_stmt(s)?;
                    }
                    self.pop_scope();
                }
            }
            Stmt::Break => {
                // No-op: validated in CFG builder
            }
            Stmt::Continue => {
                // No-op: validated in CFG builder
            }
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
                self.declare_variable(name, Type::Unknown, false)?;
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
            Stmt::Receive { channel, target: _ } => {
                let _ = self.lookup_variable(channel).ok_or_else(|| {
                    CompileError::new(
                        &format!("Undefined channel '{}'", channel),
                        0, 0, "",
                        ErrorCode::E0003,
                    )
                })?;
            }
            Stmt::UnsafeBlock { body } => {
                // Unsafe block: analyze without safety checks
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
            Stmt::Import { path } => {
                // Import statements are resolved before semantic analysis
                let _ = path;
            }
            Stmt::TryCatch { try_body, catch_var, catch_body, finally_body } => {
                self.push_scope();
                for s in try_body {
                    self.analyze_stmt(s)?;
                }
                self.pop_scope();
                
                if let Some(var) = catch_var {
                    self.push_scope();
                    self.declare_variable(var, Type::String, false)?;
                    for s in catch_body {
                        self.analyze_stmt(s)?;
                    }
                    self.pop_scope();
                }
                
                if let Some(finally) = finally_body {
                    self.push_scope();
                    for s in finally {
                        self.analyze_stmt(s)?;
                    }
                    self.pop_scope();
                }
            }
            Stmt::ArrayAssign { array, index, value } => {
                let (array_type, _) = self.lookup_variable(array).ok_or_else(|| {
                    CompileError::new(
                        &format!("Undefined array '{}'", array),
                        0, 0, "",
                        ErrorCode::E0003,
                    )
                })?;
                
                if array_type != Type::List && array_type != Type::Unknown {
                    return Err(CompileError::new(
                        &format!("Array assignment requires list, found {}", array_type.name().as_str()),
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
    
    fn analyze_expr(&mut self, expr: &Expr) -> Result<Type> {
        match expr {
            Expr::Borrow { expr } => {
                // Check the borrow rules
                if let Expr::Var(name) = expr.as_ref() {
                    self.check_borrow_rules(name, false)?;
                    self.mark_borrowed(name);
                    let inner_type = self.analyze_expr(expr)?;
                    return Ok(Type::Borrow(Box::new(inner_type)));
                }
                let inner_type = self.analyze_expr(expr)?;
                Ok(Type::Borrow(Box::new(inner_type)))
            }
            Expr::MutBorrow { expr } => {
                // Check the borrow rules for mutable borrow
                if let Expr::Var(name) = expr.as_ref() {
                    self.check_borrow_rules(name, true)?;
                    self.mark_mutably_borrowed(name);
                    let inner_type = self.analyze_expr(expr)?;
                    return Ok(Type::MutBorrow(Box::new(inner_type)));
                }
                let inner_type = self.analyze_expr(expr)?;
                Ok(Type::MutBorrow(Box::new(inner_type)))
            }
            Expr::Deref { expr } => {
                let inner_type = self.analyze_expr(expr)?;
                match inner_type {
                    Type::Pointer(t) => Ok(*t),
                    _ => Ok(Type::Unknown),
                }
            }
            Expr::AddrOf { expr } => {
                let inner_type = self.analyze_expr(expr)?;
                Ok(Type::Pointer(Box::new(inner_type)))
            }
            Expr::Number(_) => Ok(Type::Float),
            Expr::Int(_) => Ok(Type::Int),
            Expr::String(_) => Ok(Type::String),
            Expr::Bool(_) => Ok(Type::Bool),
            Expr::List(_) => Ok(Type::List),
            Expr::Some { value } => {
                let inner = self.analyze_expr(value)?;
                Ok(Type::Option(Box::new(inner)))
            }
            Expr::None => Ok(Type::Option(Box::new(Type::Unknown))),
            Expr::Ok { value } => {
                let inner = self.analyze_expr(value)?;
                Ok(Type::Result(Box::new(inner), Box::new(Type::Unknown)))
            }
            Expr::Error { value } => {
                let inner = self.analyze_expr(value)?;
                Ok(Type::Result(Box::new(Type::Unknown), Box::new(inner)))
            }
            Expr::Var(name) => {
                if self.is_moved(name) {
                    return Err(CompileError::new(
                        &format!("Use of moved variable '{}'", name),
                        0, 0, "",
                        ErrorCode::E0007,
                    ).with_suggestion("Variable ownership was transferred and cannot be used in this scope"));
                }
                
                // Check if variable is mutably borrowed (can't read)
                if self.is_mutably_borrowed(name) {
                    return Err(CompileError::new(
                        &format!("Cannot read '{}' while mutably borrowed", name),
                        0, 0, "",
                        ErrorCode::E0007,
                    ).with_suggestion("Wait for the mutable borrow to end"));
                }
                
                self.lookup_variable(name)
                    .map(|(t, _)| t)
                    .ok_or_else(|| CompileError::new(
                        &format!("Undefined variable '{}'", name),
                        0, 0, "",
                        ErrorCode::E0003,
                    ).with_suggestion(&format!(
                        "Declare '{}' with 'var {} := ...' or 'val {} := ...' in this scope",
                        name, name, name
                    )))
            }
            Expr::ArrayAccess { array, index } => {
                let array_type = self.analyze_expr(array)?;
                if array_type != Type::List && array_type != Type::Unknown {
                    return Err(CompileError::new(
                        &format!("Array access requires list, found {}", array_type.name().as_str()),
                        0, 0, "",
                        ErrorCode::E0002,
                    ));
                }
                
                let index_type = self.analyze_expr(index)?;
                if index_type != Type::Int && index_type != Type::Float && index_type != Type::Unknown {
                    return Err(CompileError::new(
                        &format!("Array index must be numeric, found {}", index_type.name().as_str()),
                        0, 0, "",
                        ErrorCode::E0002,
                    ));
                }
                
                Ok(Type::Float)
            }
            Expr::Binary { left, op, right } => {
                let left_type = self.analyze_expr(left)?;
                let right_type = self.analyze_expr(right)?;
                
                match op {
                    BinOp::Add => {
                        // Check for string concatenation
                        if left_type == Type::String && right_type == Type::String {
                            Ok(Type::String)
                        }
                        // Check for numeric types (allow Int + Float promotion)
                        else if self.is_numeric(&left_type) && self.is_numeric(&right_type) {
                            if left_type == Type::Float || right_type == Type::Float {
                                Ok(Type::Float)
                            } else {
                                Ok(Type::Int)
                            }
                        }
                        // Error case
                        else {
                            Err(CompileError::new(
                                &format!(
                                    "Addition requires matching types, found {} and {}",
                                    left_type.name(), right_type.name()
                                ),
                                0, 0, "",
                                ErrorCode::E0002,
                            ).with_suggestion("Use matching types or add type conversion"))
                        }
                    }
                    BinOp::Subtract | BinOp::Multiply | BinOp::Divide => {
                        if self.is_numeric(&left_type) && self.is_numeric(&right_type) {
                            if left_type == Type::Float || right_type == Type::Float {
                                Ok(Type::Float)
                            } else {
                                Ok(Type::Int)
                            }
                        } else {
                            Err(CompileError::new(
                                &format!(
                                    "Arithmetic requires numeric types, found {} and {}",
                                    left_type.name(), right_type.name()
                                ),
                                0, 0, "",
                                ErrorCode::E0002,
                            ).with_suggestion("Both operands must be numeric (Int or Float)"))
                        }
                    }
                    BinOp::Greater | BinOp::Less | BinOp::GreaterEqual | BinOp::LessEqual => {
                        if self.is_numeric(&left_type) && self.is_numeric(&right_type) {
                            Ok(Type::Bool)
                        } else {
                            Err(CompileError::new(
                                &format!(
                                    "Comparison requires numeric types, found {} and {}",
                                    left_type.name(), right_type.name()
                                ),
                                0, 0, "",
                                ErrorCode::E0002,
                            ).with_suggestion("Use numeric types for comparison"))
                        }
                    }
                    BinOp::Equal | BinOp::NotEqual => {
                        if left_type == right_type || 
                           (self.is_numeric(&left_type) && self.is_numeric(&right_type)) {
                            Ok(Type::Bool)
                        } else {
                            Err(CompileError::new(
                                &format!(
                                    "Equality requires matching types, found {} and {}",
                                    left_type.name(), right_type.name()
                                ),
                                0, 0, "",
                                ErrorCode::E0002,
                            ).with_suggestion("Use matching types for equality comparison"))
                        }
                    }
                    BinOp::And | BinOp::Or => {
                        if left_type == Type::Bool && right_type == Type::Bool {
                            Ok(Type::Bool)
                        } else {
                            Err(CompileError::new(
                                &format!(
                                    "Logical operators require boolean operands, found {} and {}",
                                    left_type.name(), right_type.name()
                                ),
                                0, 0, "",
                                ErrorCode::E0002,
                            ).with_suggestion("Use 'and' and 'or' only with boolean values"))
                        }
                    }
                }
            }
            
            Expr::FunctionCall { name, args } => {
                let clean_name = name.trim_end_matches("()");
                let func_info = self.functions.get(clean_name).cloned().ok_or_else(|| {
                    CompileError::new(
                        &format!("Undefined function '{}'", name),
                        0, 0, "",
                        ErrorCode::E0004,
                    ).with_suggestion(&format!(
                        "Check if function '{}' is defined or imported", name
                    ))
                })?;
                
                // Validate argument count
                if args.len() != func_info.params.len() {
                    return Err(CompileError::new(
                        &format!(
                            "Function '{}' expects {} arguments, got {}",
                            name, func_info.params.len(), args.len()
                        ),
                        0, 0, "",
                        ErrorCode::E0002,
                    ).with_suggestion(&format!(
                        "Provide exactly {} argument(s) to '{}'",
                        func_info.params.len(), name
                    )));
                }
                
                // Validate argument types
                for (arg, (param_name, param_type)) in args.iter().zip(&func_info.params) {
                    let arg_type = self.analyze_expr(arg)?;
                    if arg_type != *param_type && *param_type != Type::Unknown {
                        return Err(CompileError::new(
                            &format!(
                                "Argument '{}' type mismatch: expected {}, found {}",
                                param_name, param_type.name(), arg_type.name()
                            ),
                            0, 0, "",
                            ErrorCode::E0002,
                        ).with_suggestion(&format!(
                            "Convert the argument to {} or change the function signature",
                            param_type.name()
                        )));
                    }
                }
                
                Ok(func_info.return_type.clone())
            }
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
}
