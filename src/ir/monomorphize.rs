// src/ir/monomorphize.rs

use crate::common::types::Type;
use crate::frontend::ast::BinOp;
use crate::frontend::ast::{Expr, FunctionDecl, Stmt, TypeSyntax};
use std::collections::HashMap;

pub struct Monomorphizer {
    /// Maps (function_name, type_args) -> specialized_function_name
    instantiations: HashMap<String, HashMap<Vec<Type>, String>>,
    /// Collected type bindings: function_name -> list of type arg combinations
    type_bindings: HashMap<String, Vec<Vec<Type>>>,
}

/// True if the type contains `Unknown` or `TypeVar` anywhere in its
/// structure. Used by the monomorphizer to refuse specializations
/// whose concrete type arguments could not be determined.
fn has_unresolved(t: &Type) -> bool {
    match t {
        Type::Unknown | Type::TypeVar(_) => true,
        Type::List(inner)
        | Type::Option(inner)
        | Type::Pointer(inner)
        | Type::Borrow(inner)
        | Type::MutBorrow(inner)
        | Type::Channel(inner)
        | Type::Array(inner, _) => has_unresolved(inner),
        Type::Result { ok, error } => has_unresolved(ok) || has_unresolved(error),
        Type::Tuple(elems) => elems.iter().any(has_unresolved),
        _ => false,
    }
}

impl Monomorphizer {
    pub fn new() -> Self {
        Monomorphizer {
            instantiations: HashMap::new(),
            type_bindings: HashMap::new(),
        }
    }

    pub fn collect_instantiations(&mut self, functions: &[FunctionDecl]) {
        for func in functions {
            for stmt in &func.body {
                self.collect_from_stmt(stmt);
            }
        }
    }

    fn collect_from_stmt(&mut self, stmt: &Stmt) {
        match stmt {
            Stmt::VarDecl { value, .. } => self.collect_from_expr(value),
            Stmt::Assign { value, .. } => self.collect_from_expr(value),
            Stmt::Expression(expr) => self.collect_from_expr(expr),
            Stmt::Print { expr, .. } => self.collect_from_expr(expr),
            Stmt::Return {
                value: Some(expr), ..
            } => {
                self.collect_from_expr(expr);
            }
            Stmt::Return { .. } => {}
            _ => {}
        }
    }

    fn collect_from_expr(&mut self, expr: &Expr) {
        match expr {
            Expr::FunctionCall { name, args, .. } => {
                let mut type_args = Vec::new();
                for arg in args {
                    type_args.push(self.infer_expr_type(arg));
                }

                let clean_name = name.trim_end_matches("()").to_string();
                self.type_bindings
                    .entry(clean_name)
                    .or_default()
                    .push(type_args);

                for arg in args {
                    self.collect_from_expr(arg);
                }
            }
            Expr::Binary { left, right, .. } => {
                self.collect_from_expr(left);
                self.collect_from_expr(right);
            }
            Expr::If {
                condition,
                then_branch,
                else_branch,
                ..
            } => {
                self.collect_from_expr(condition);
                self.collect_from_expr(then_branch);
                if let Some(e) = else_branch {
                    self.collect_from_expr(e);
                }
            }
            Expr::Block {
                statements,
                trailing_expr,
                ..
            } => {
                for s in statements {
                    self.collect_from_stmt(s);
                }
                if let Some(e) = trailing_expr {
                    self.collect_from_expr(e);
                }
            }
            Expr::List(elements, _) => {
                for e in elements {
                    self.collect_from_expr(e);
                }
            }
            Expr::Some { value, .. } => self.collect_from_expr(value),
            Expr::Ok { value, .. } => self.collect_from_expr(value),
            Expr::Error { value, .. } => self.collect_from_expr(value),
            _ => {}
        }
    }

    fn infer_expr_type(&self, expr: &Expr) -> Type {
        match expr {
            Expr::Int(_, _) => Type::Int,
            Expr::Number(_, _) => Type::Float,
            Expr::String(_, _) => Type::String,
            Expr::Bool(_, _) => Type::Bool,
            Expr::List(elements, _) => {
                if elements.is_empty() {
                    return Type::list(Type::Unknown);
                }
                let first_type = self.infer_expr_type(&elements[0]);
                let mut common = first_type.clone();
                for elem in &elements[1..] {
                    common = common.common_supertype(&self.infer_expr_type(elem));
                }
                Type::list(common)
            }
            Expr::Binary {
                left, op, right, ..
            } => {
                let lt = self.infer_expr_type(left);
                let rt = self.infer_expr_type(right);
                match op {
                    BinOp::Add | BinOp::Subtract | BinOp::Multiply | BinOp::Divide => {
                        if lt.is_numeric() && rt.is_numeric() {
                            lt.common_supertype(&rt)
                        } else if lt == Type::String && rt == Type::String {
                            Type::String
                        } else {
                            Type::Unknown
                        }
                    }
                    BinOp::Greater
                    | BinOp::Less
                    | BinOp::GreaterEqual
                    | BinOp::LessEqual
                    | BinOp::Equal
                    | BinOp::NotEqual => Type::Bool,
                    BinOp::And | BinOp::Or => Type::Bool,
                }
            }
            Expr::FunctionCall { name, args, .. } => {
                if let Some(instantiations) = self.instantiations.get(name) {
                    let arg_types: Vec<Type> =
                        args.iter().map(|a| self.infer_expr_type(a)).collect();
                    if let Some(_specialized) = instantiations.get(&arg_types) {
                        return Type::Unknown;
                    }
                }

                match name.as_str() {
                    "Math.sqrt" | "Math.sin" | "Math.cos" | "Math.tan" | "Math.exp"
                    | "Math.log" | "Math.floor" | "Math.ceil" | "Math.abs" => Type::Float,
                    "Math.pow" => Type::Float,
                    "String.length" | "List.length" => Type::Int,
                    "String.concat" => Type::String,
                    _ => Type::Unknown,
                }
            }
            Expr::Some { value, .. } => Type::option(self.infer_expr_type(value)),
            Expr::None(_) => Type::option(Type::Unknown),
            Expr::Ok { value, .. } => Type::result(self.infer_expr_type(value), Type::Unknown),
            Expr::Error { value, .. } => Type::result(Type::Unknown, self.infer_expr_type(value)),
            Expr::ArrayAccess { array, .. } => match self.infer_expr_type(array) {
                Type::List(inner) => *inner,
                Type::Array(inner, _) => *inner,
                _ => Type::Unknown,
            },
            Expr::Borrow { expr, .. } => Type::borrow(self.infer_expr_type(expr)),
            Expr::MutBorrow { expr, .. } => Type::mut_borrow(self.infer_expr_type(expr)),
            Expr::Deref { expr, .. } => match self.infer_expr_type(expr) {
                Type::Borrow(inner) | Type::MutBorrow(inner) | Type::Pointer(inner) => *inner,
                _ => Type::Unknown,
            },
            Expr::AddrOf { expr, .. } => Type::pointer(self.infer_expr_type(expr)),
            _ => Type::Unknown,
        }
    }

    pub fn specialized_name(&self, func_name: &str, type_args: &[Type]) -> String {
        let type_str: Vec<String> = type_args
            .iter()
            .map(|t| self.specialized_type_name(t))
            .collect();
        format!("{}_{}", func_name, type_str.join("_"))
    }

    fn specialized_type_name(&self, type_: &Type) -> String {
        match type_ {
            Type::Int => "Int".to_string(),
            Type::Float => "Float".to_string(),
            Type::String => "String".to_string(),
            Type::Bool => "Bool".to_string(),
            Type::List(inner) => format!("List_{}", self.specialized_type_name(inner)),
            Type::Option(inner) => format!("Option_{}", self.specialized_type_name(inner)),
            Type::TypeVar(v) => v.clone(),
            _ => "Unknown".to_string(),
        }
    }

    pub fn substitute_type_string(
        &self,
        type_str: &str,
        type_bindings: &HashMap<String, Type>,
    ) -> String {
        let trimmed = type_str.trim();
        if trimmed.len() == 1 {
            if let Some(c) = trimmed.chars().next() {
                if c.is_uppercase() {
                    if let Some(concrete) = type_bindings.get(trimmed) {
                        return concrete.to_string();
                    }
                }
            }
        }
        trimmed.to_string()
    }

    pub fn substitute_in_function(
        &self,
        func: &FunctionDecl,
        type_bindings: &HashMap<String, Type>,
    ) -> FunctionDecl {
        let mut new_func = func.clone();
        new_func.params = func
            .params
            .iter()
            .map(|(name, t)| {
                let type_ = t.as_ref().map(|s| {
                    TypeSyntax::from_string(
                        &self.substitute_type_string(&s.to_string_rep(), type_bindings),
                    )
                });
                (name.clone(), type_)
            })
            .collect();
        new_func.return_type = func.return_type.as_ref().map(|t| {
            TypeSyntax::from_string(&self.substitute_type_string(&t.to_string_rep(), type_bindings))
        });
        new_func.body = func
            .body
            .iter()
            .map(|s| self.substitute_in_stmt(s, type_bindings))
            .collect();
        new_func.type_params = Vec::new();
        new_func.where_clauses = Vec::new();

        // Derive the mangled type args in **type parameter
        // declaration order**, not in `type_bindings` iteration order.
        // `type_bindings` is a `HashMap<String, Type>` whose iteration
        // order is unspecified and changes between process runs —
        // which would make `f<A, B>` sometimes mangle to `f_Int_Float`
        // and sometimes to `f_Float_Int`, breaking instantiation lookup
        // at call sites.
        let type_args: Vec<Type> = func
            .type_params
            .iter()
            .filter_map(|p| type_bindings.get(p).cloned())
            .collect();
        new_func.name = self.specialized_name(&func.name, &type_args);
        new_func
    }

    fn substitute_in_stmt(&self, stmt: &Stmt, type_bindings: &HashMap<String, Type>) -> Stmt {
        match stmt {
            Stmt::VarDecl {
                name,
                value,
                type_annotation,
                mutable,
                span,
            } => Stmt::VarDecl {
                name: name.clone(),
                value: self.substitute_in_expr(value, type_bindings),
                type_annotation: type_annotation.as_ref().map(|t| {
                    TypeSyntax::from_string(
                        &self.substitute_type_string(&t.to_string_rep(), type_bindings),
                    )
                }),
                mutable: *mutable,
                span: *span,
            },
            Stmt::Assign { name, value, span } => Stmt::Assign {
                name: name.clone(),
                value: self.substitute_in_expr(value, type_bindings),
                span: *span,
            },
            Stmt::ArrayAssign {
                array,
                index,
                value,
                span,
            } => Stmt::ArrayAssign {
                array: array.clone(),
                index: self.substitute_in_expr(index, type_bindings),
                value: self.substitute_in_expr(value, type_bindings),
                span: *span,
            },
            Stmt::Expression(expr) => {
                Stmt::Expression(self.substitute_in_expr(expr, type_bindings))
            }
            Stmt::Print { expr, span } => Stmt::Print {
                expr: self.substitute_in_expr(expr, type_bindings),
                span: *span,
            },
            Stmt::Return { value, span } => Stmt::Return {
                value: value
                    .as_ref()
                    .map(|v| self.substitute_in_expr(v, type_bindings)),
                span: *span,
            },
            Stmt::RegionBlock { name, body, span } => Stmt::RegionBlock {
                name: name.clone(),
                body: body
                    .iter()
                    .map(|s| self.substitute_in_stmt(s, type_bindings))
                    .collect(),
                span: *span,
            },
            Stmt::UnsafeBlock { body, span } => Stmt::UnsafeBlock {
                body: body
                    .iter()
                    .map(|s| self.substitute_in_stmt(s, type_bindings))
                    .collect(),
                span: *span,
            },
            Stmt::Spawn { body, span } => Stmt::Spawn {
                body: body
                    .iter()
                    .map(|s| self.substitute_in_stmt(s, type_bindings))
                    .collect(),
                span: *span,
            },
            Stmt::Parallel { blocks, span } => Stmt::Parallel {
                blocks: blocks
                    .iter()
                    .map(|b| {
                        b.iter()
                            .map(|s| self.substitute_in_stmt(s, type_bindings))
                            .collect()
                    })
                    .collect(),
                span: *span,
            },
            Stmt::Send {
                channel,
                value,
                span,
            } => Stmt::Send {
                channel: channel.clone(),
                value: self.substitute_in_expr(value, type_bindings),
                span: *span,
            },
            Stmt::Defer { stmt, span } => Stmt::Defer {
                stmt: Box::new(self.substitute_in_stmt(stmt, type_bindings)),
                span: *span,
            },
            _ => stmt.clone(),
        }
    }

    fn substitute_in_expr(&self, expr: &Expr, type_bindings: &HashMap<String, Type>) -> Expr {
        match expr {
            Expr::FunctionCall { name, args, span } => {
                let new_args: Vec<Expr> = args
                    .iter()
                    .map(|a| self.substitute_in_expr(a, type_bindings))
                    .collect();
                let clean_name = name.trim_end_matches("()");
                let mut new_name = clean_name.to_string();

                if let Some(instantiations) = self.instantiations.get(clean_name) {
                    let arg_types: Vec<Type> =
                        new_args.iter().map(|a| self.infer_expr_type(a)).collect();
                    if let Some(specialized) = instantiations.get(&arg_types) {
                        new_name = specialized.clone();
                    }
                }

                Expr::FunctionCall {
                    name: new_name,
                    args: new_args,
                    span: *span,
                }
            }
            Expr::Binary {
                left,
                op,
                right,
                span,
            } => Expr::Binary {
                left: Box::new(self.substitute_in_expr(left, type_bindings)),
                op: op.clone(),
                right: Box::new(self.substitute_in_expr(right, type_bindings)),
                span: *span,
            },
            Expr::Unary { op, expr, span } => Expr::Unary {
                op: op.clone(),
                expr: Box::new(self.substitute_in_expr(expr, type_bindings)),
                span: *span,
            },
            Expr::If {
                condition,
                then_branch,
                else_branch,
                span,
            } => Expr::If {
                condition: Box::new(self.substitute_in_expr(condition, type_bindings)),
                then_branch: Box::new(self.substitute_in_expr(then_branch, type_bindings)),
                else_branch: else_branch
                    .as_ref()
                    .map(|e| Box::new(self.substitute_in_expr(e, type_bindings))),
                span: *span,
            },
            Expr::Block {
                statements,
                trailing_expr,
                span,
            } => Expr::Block {
                statements: statements
                    .iter()
                    .map(|s| self.substitute_in_stmt(s, type_bindings))
                    .collect(),
                trailing_expr: trailing_expr
                    .as_ref()
                    .map(|e| Box::new(self.substitute_in_expr(e, type_bindings))),
                span: *span,
            },
            Expr::List(elements, span) => Expr::List(
                elements
                    .iter()
                    .map(|e| self.substitute_in_expr(e, type_bindings))
                    .collect(),
                *span,
            ),
            Expr::ArrayAccess {
                array,
                index,
                span,
            } => Expr::ArrayAccess {
                array: Box::new(self.substitute_in_expr(array, type_bindings)),
                index: Box::new(self.substitute_in_expr(index, type_bindings)),
                span: *span,
            },
            Expr::Borrow { expr, span } => Expr::Borrow {
                expr: Box::new(self.substitute_in_expr(expr, type_bindings)),
                span: *span,
            },
            Expr::MutBorrow { expr, span } => Expr::MutBorrow {
                expr: Box::new(self.substitute_in_expr(expr, type_bindings)),
                span: *span,
            },
            Expr::Deref { expr, span } => Expr::Deref {
                expr: Box::new(self.substitute_in_expr(expr, type_bindings)),
                span: *span,
            },
            Expr::AddrOf { expr, span } => Expr::AddrOf {
                expr: Box::new(self.substitute_in_expr(expr, type_bindings)),
                span: *span,
            },
            Expr::Some { value, span } => Expr::Some {
                value: Box::new(self.substitute_in_expr(value, type_bindings)),
                span: *span,
            },
            Expr::Ok { value, span } => Expr::Ok {
                value: Box::new(self.substitute_in_expr(value, type_bindings)),
                span: *span,
            },
            Expr::Error { value, span } => Expr::Error {
                value: Box::new(self.substitute_in_expr(value, type_bindings)),
                span: *span,
            },
            Expr::Match { value, cases, span } => Expr::Match {
                value: Box::new(self.substitute_in_expr(value, type_bindings)),
                cases: cases
                    .iter()
                    .map(|c| crate::frontend::ast::MatchCaseExpr {
                        pattern: c.pattern.clone(),
                        body: self.substitute_in_expr(&c.body, type_bindings),
                    })
                    .collect(),
                span: *span,
            },
            Expr::TryCatch {
                try_branch,
                catch_var,
                catch_branch,
                finally_body,
                span,
            } => Expr::TryCatch {
                try_branch: Box::new(self.substitute_in_expr(try_branch, type_bindings)),
                catch_var: catch_var.clone(),
                catch_branch: Box::new(self.substitute_in_expr(catch_branch, type_bindings)),
                finally_body: finally_body.as_ref().map(|body| {
                    body.iter()
                        .map(|s| self.substitute_in_stmt(s, type_bindings))
                        .collect()
                }),
                span: *span,
            },
            Expr::For {
                var,
                iterable,
                body,
                trailing_expr,
                span,
            } => Expr::For {
                var: var.clone(),
                iterable: Box::new(self.substitute_in_expr(iterable, type_bindings)),
                body: body
                    .iter()
                    .map(|s| self.substitute_in_stmt(s, type_bindings))
                    .collect(),
                trailing_expr: trailing_expr
                    .as_ref()
                    .map(|e| Box::new(self.substitute_in_expr(e, type_bindings))),
                span: *span,
            },
            Expr::While {
                condition,
                body,
                trailing_expr,
                span,
            } => Expr::While {
                condition: Box::new(self.substitute_in_expr(condition, type_bindings)),
                body: body
                    .iter()
                    .map(|s| self.substitute_in_stmt(s, type_bindings))
                    .collect(),
                trailing_expr: trailing_expr
                    .as_ref()
                    .map(|e| Box::new(self.substitute_in_expr(e, type_bindings))),
                span: *span,
            },
            Expr::Range {
                start,
                end,
                inclusive,
                span,
            } => Expr::Range {
                start: start
                    .as_ref()
                    .map(|e| Box::new(self.substitute_in_expr(e, type_bindings))),
                end: end
                    .as_ref()
                    .map(|e| Box::new(self.substitute_in_expr(e, type_bindings))),
                inclusive: *inclusive,
                span: *span,
            },
            Expr::FieldAccess { object, field, span } => Expr::FieldAccess {
                object: Box::new(self.substitute_in_expr(object, type_bindings)),
                field: field.clone(),
                span: *span,
            },
            _ => expr.clone(),
        }
    }

    /// Check that concrete types satisfy trait bounds
    pub fn check_trait_bounds_for_instantiation(
        &self,
        func: &FunctionDecl,
        type_args: &[Type],
    ) -> Result<(), String> {
        for clause in &func.where_clauses {
            let trait_name = &clause.trait_name;
            let type_param = &clause.type_param;

            if let Some(param_index) = func.type_params.iter().position(|p| p == type_param) {
                if let Some(concrete_type) = type_args.get(param_index) {
                    let implements = match trait_name.as_str() {
                        "Comparable" => {
                            matches!(concrete_type, Type::Int | Type::Float)
                        }
                        "Display" => {
                            matches!(
                                concrete_type,
                                Type::Int | Type::Float | Type::String | Type::Bool
                            )
                        }
                        "Add" => {
                            matches!(concrete_type, Type::Int | Type::Float)
                        }
                        _ => true,
                    };

                    if !implements {
                        return Err(format!(
                            "Type '{}' does not implement trait '{}' (required by '{}')",
                            concrete_type, trait_name, type_param
                        ));
                    }
                }
            }
        }
        Ok(())
    }

    pub fn monomorphize(&mut self, functions: &[FunctionDecl]) -> Vec<FunctionDecl> {
        let mut result = Vec::new();

        for func in functions {
            if func.type_params.is_empty() {
                result.push(func.clone());
            } else {
                // Keep the generic function in the output. If a call
                // site's type args could not be resolved at monomorphize
                // time, the analyzer looks the generic up by name and
                // binds type variables from the real argument types.
                result.push(func.clone());

                if let Some(all_type_args) = self.type_bindings.get(&func.name).cloned() {
                    for type_args in &all_type_args {
                        if type_args.iter().any(has_unresolved) {
                            continue;
                        }

                        let mut bindings = HashMap::new();
                        for (i, param) in func.type_params.iter().enumerate() {
                            if let Some(concrete) = type_args.get(i) {
                                bindings.insert(param.clone(), concrete.clone());
                            }
                        }
                        if let Err(err) =
                            self.check_trait_bounds_for_instantiation(func, type_args)
                        {
                            eprintln!("Trait bound violation: {}", err);
                            continue;
                        }

                        let specialized = self.substitute_in_function(func, &bindings);
                        let name = specialized.name.clone();
                        result.push(specialized);
                        self.instantiations
                            .entry(func.name.clone())
                            .or_default()
                            .insert(type_args.clone(), name);
                    }
                }
            }
        }

        for func in result.iter_mut() {
            if func.type_params.is_empty() {
                func.body = func
                    .body
                    .iter()
                    .map(|s| self.substitute_in_stmt_with_instantiations(s))
                    .collect();
            }
        }

        result
    }

    fn substitute_in_stmt_with_instantiations(&self, stmt: &Stmt) -> Stmt {
        match stmt {
            Stmt::VarDecl {
                name,
                value,
                type_annotation,
                mutable,
                span,
            } => Stmt::VarDecl {
                name: name.clone(),
                value: self.substitute_in_expr_with_instantiations(value),
                type_annotation: type_annotation.clone(),
                mutable: *mutable,
                span: *span,
            },
            Stmt::Assign { name, value, span } => Stmt::Assign {
                name: name.clone(),
                value: self.substitute_in_expr_with_instantiations(value),
                span: *span,
            },
            Stmt::ArrayAssign {
                array,
                index,
                value,
                span,
            } => Stmt::ArrayAssign {
                array: array.clone(),
                index: self.substitute_in_expr_with_instantiations(index),
                value: self.substitute_in_expr_with_instantiations(value),
                span: *span,
            },
            Stmt::Expression(expr) => {
                Stmt::Expression(self.substitute_in_expr_with_instantiations(expr))
            }
            Stmt::Print { expr, span } => Stmt::Print {
                expr: self.substitute_in_expr_with_instantiations(expr),
                span: *span,
            },
            Stmt::Return { value, span } => Stmt::Return {
                value: value
                    .as_ref()
                    .map(|v| self.substitute_in_expr_with_instantiations(v)),
                span: *span,
            },
            Stmt::RegionBlock { name, body, span } => Stmt::RegionBlock {
                name: name.clone(),
                body: body
                    .iter()
                    .map(|s| self.substitute_in_stmt_with_instantiations(s))
                    .collect(),
                span: *span,
            },
            Stmt::UnsafeBlock { body, span } => Stmt::UnsafeBlock {
                body: body
                    .iter()
                    .map(|s| self.substitute_in_stmt_with_instantiations(s))
                    .collect(),
                span: *span,
            },
            Stmt::Spawn { body, span } => Stmt::Spawn {
                body: body
                    .iter()
                    .map(|s| self.substitute_in_stmt_with_instantiations(s))
                    .collect(),
                span: *span,
            },
            Stmt::Parallel { blocks, span } => Stmt::Parallel {
                blocks: blocks
                    .iter()
                    .map(|b| {
                        b.iter()
                            .map(|s| self.substitute_in_stmt_with_instantiations(s))
                            .collect()
                    })
                    .collect(),
                span: *span,
            },
            Stmt::Send {
                channel,
                value,
                span,
            } => Stmt::Send {
                channel: channel.clone(),
                value: self.substitute_in_expr_with_instantiations(value),
                span: *span,
            },
            Stmt::Defer { stmt, span } => Stmt::Defer {
                stmt: Box::new(self.substitute_in_stmt_with_instantiations(stmt)),
                span: *span,
            },
            _ => stmt.clone(),
        }
    }

    fn substitute_in_expr_with_instantiations(&self, expr: &Expr) -> Expr {
        match expr {
            Expr::FunctionCall { name, args, span } => {
                let new_args: Vec<Expr> = args
                    .iter()
                    .map(|a| self.substitute_in_expr_with_instantiations(a))
                    .collect();
                let clean_name = name.trim_end_matches("()");

                if let Some(instantiations) = self.instantiations.get(clean_name) {
                    let arg_types: Vec<Type> =
                        new_args.iter().map(|a| self.infer_expr_type(a)).collect();
                    if let Some(specialized) = instantiations.get(&arg_types) {
                        return Expr::FunctionCall {
                            name: specialized.clone(),
                            args: new_args,
                            span: *span,
                        };
                    }
                }

                Expr::FunctionCall {
                    name: clean_name.to_string(),
                    args: new_args,
                    span: *span,
                }
            }
            Expr::Binary {
                left,
                op,
                right,
                span,
            } => Expr::Binary {
                left: Box::new(self.substitute_in_expr_with_instantiations(left)),
                op: op.clone(),
                right: Box::new(self.substitute_in_expr_with_instantiations(right)),
                span: *span,
            },
            Expr::Unary { op, expr, span } => Expr::Unary {
                op: op.clone(),
                expr: Box::new(self.substitute_in_expr_with_instantiations(expr)),
                span: *span,
            },
            Expr::If {
                condition,
                then_branch,
                else_branch,
                span,
            } => Expr::If {
                condition: Box::new(self.substitute_in_expr_with_instantiations(condition)),
                then_branch: Box::new(self.substitute_in_expr_with_instantiations(then_branch)),
                else_branch: else_branch
                    .as_ref()
                    .map(|e| Box::new(self.substitute_in_expr_with_instantiations(e))),
                span: *span,
            },
            Expr::Block {
                statements,
                trailing_expr,
                span,
            } => Expr::Block {
                statements: statements
                    .iter()
                    .map(|s| self.substitute_in_stmt_with_instantiations(s))
                    .collect(),
                trailing_expr: trailing_expr
                    .as_ref()
                    .map(|e| Box::new(self.substitute_in_expr_with_instantiations(e))),
                span: *span,
            },
            Expr::List(elements, span) => Expr::List(
                elements
                    .iter()
                    .map(|e| self.substitute_in_expr_with_instantiations(e))
                    .collect(),
                *span,
            ),
            Expr::ArrayAccess {
                array,
                index,
                span,
            } => Expr::ArrayAccess {
                array: Box::new(self.substitute_in_expr_with_instantiations(array)),
                index: Box::new(self.substitute_in_expr_with_instantiations(index)),
                span: *span,
            },
            Expr::Borrow { expr, span } => Expr::Borrow {
                expr: Box::new(self.substitute_in_expr_with_instantiations(expr)),
                span: *span,
            },
            Expr::MutBorrow { expr, span } => Expr::MutBorrow {
                expr: Box::new(self.substitute_in_expr_with_instantiations(expr)),
                span: *span,
            },
            Expr::Deref { expr, span } => Expr::Deref {
                expr: Box::new(self.substitute_in_expr_with_instantiations(expr)),
                span: *span,
            },
            Expr::AddrOf { expr, span } => Expr::AddrOf {
                expr: Box::new(self.substitute_in_expr_with_instantiations(expr)),
                span: *span,
            },
            Expr::Some { value, span } => Expr::Some {
                value: Box::new(self.substitute_in_expr_with_instantiations(value)),
                span: *span,
            },
            Expr::Ok { value, span } => Expr::Ok {
                value: Box::new(self.substitute_in_expr_with_instantiations(value)),
                span: *span,
            },
            Expr::Error { value, span } => Expr::Error {
                value: Box::new(self.substitute_in_expr_with_instantiations(value)),
                span: *span,
            },
            Expr::Match { value, cases, span } => Expr::Match {
                value: Box::new(self.substitute_in_expr_with_instantiations(value)),
                cases: cases
                    .iter()
                    .map(|c| crate::frontend::ast::MatchCaseExpr {
                        pattern: c.pattern.clone(),
                        body: self.substitute_in_expr_with_instantiations(&c.body),
                    })
                    .collect(),
                span: *span,
            },
            Expr::TryCatch {
                try_branch,
                catch_var,
                catch_branch,
                finally_body,
                span,
            } => Expr::TryCatch {
                try_branch: Box::new(self.substitute_in_expr_with_instantiations(try_branch)),
                catch_var: catch_var.clone(),
                catch_branch: Box::new(self.substitute_in_expr_with_instantiations(
                    catch_branch,
                )),
                finally_body: finally_body.as_ref().map(|body| {
                    body.iter()
                        .map(|s| self.substitute_in_stmt_with_instantiations(s))
                        .collect()
                }),
                span: *span,
            },
            Expr::For {
                var,
                iterable,
                body,
                trailing_expr,
                span,
            } => Expr::For {
                var: var.clone(),
                iterable: Box::new(self.substitute_in_expr_with_instantiations(iterable)),
                body: body
                    .iter()
                    .map(|s| self.substitute_in_stmt_with_instantiations(s))
                    .collect(),
                trailing_expr: trailing_expr
                    .as_ref()
                    .map(|e| Box::new(self.substitute_in_expr_with_instantiations(e))),
                span: *span,
            },
            Expr::While {
                condition,
                body,
                trailing_expr,
                span,
            } => Expr::While {
                condition: Box::new(self.substitute_in_expr_with_instantiations(condition)),
                body: body
                    .iter()
                    .map(|s| self.substitute_in_stmt_with_instantiations(s))
                    .collect(),
                trailing_expr: trailing_expr
                    .as_ref()
                    .map(|e| Box::new(self.substitute_in_expr_with_instantiations(e))),
                span: *span,
            },
            Expr::Range {
                start,
                end,
                inclusive,
                span,
            } => Expr::Range {
                start: start
                    .as_ref()
                    .map(|e| Box::new(self.substitute_in_expr_with_instantiations(e))),
                end: end
                    .as_ref()
                    .map(|e| Box::new(self.substitute_in_expr_with_instantiations(e))),
                inclusive: *inclusive,
                span: *span,
            },
            Expr::FieldAccess { object, field, span } => Expr::FieldAccess {
                object: Box::new(self.substitute_in_expr_with_instantiations(object)),
                field: field.clone(),
                span: *span,
            },
            _ => expr.clone(),
        }
    }
}

impl Default for Monomorphizer {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::frontend::lexer::Lexer;
    use crate::frontend::parser::Parser;

    fn monomorphize(src: &str) -> Vec<FunctionDecl> {
        let lexer = Lexer::new(src.to_string()).unwrap();
        let mut parser = Parser::new(lexer.tokens);
        let program = parser.parse_program().unwrap();
        let mut mono = Monomorphizer::new();
        mono.collect_instantiations(&program.functions);
        mono.monomorphize(&program.functions)
    }

    #[test]
    fn two_param_generic_name_follows_declaration_order() {
        // `pair<A, B>` with A=Int and B=Float must mangle to
        // `pair_Int_Float`. Before the fix, `type_bindings.values()`
        // iterated a HashMap, so the mangled suffix was
        // nondeterministic — sometimes `pair_Int_Float`, sometimes
        // `pair_Float_Int`.
        let src = "\
function pair<A, B>(a: A, b: B) -> Float
    return 0.0

procedure main
    val r := pair(1, 2.0)
    print(r)
";
        let funcs = monomorphize(src);
        let names: Vec<&str> = funcs.iter().map(|f| f.name.as_str()).collect();

        assert!(
            names.contains(&"pair_Int_Float"),
            "expected `pair_Int_Float` in {:?}",
            names
        );
        assert!(
            !names.contains(&"pair_Float_Int"),
            "produced wrong mangled name from a HashMap ordering; got {:?}",
            names
        );
    }

    #[test]
    fn specialized_name_is_stable_across_runs() {
        // Run monomorphize twice on the same source and check the
        // result names match. Nondeterminism would show up as
        // differing sets.
        let src = "\
function pair<A, B>(a: A, b: B) -> Float
    return 0.0

procedure main
    val r := pair(1, 2.0)
    print(r)
";
        let mut names1: Vec<String> = monomorphize(src).iter().map(|f| f.name.clone()).collect();
        let mut names2: Vec<String> = monomorphize(src).iter().map(|f| f.name.clone()).collect();
        names1.sort();
        names2.sort();
        assert_eq!(
            names1, names2,
            "monomorphize produced different names on consecutive runs"
        );
    }

    #[test]
    fn substitution_recurses_into_array_index() {
        // A call to a generic function nested inside an
        // `ArrayAccess` index must still be rewritten to the
        // specialized name. Before the fix, `substitute_in_expr_with_instantiations`
        // handled `ArrayAccess` but not inside its recursive cases
        // for many container forms; `ArrayAccess` was one that DID
        // get handled — this test pins the behavior so it can't
        // regress.
        let src = "\
function identity<T>(x: T) -> T
    return x

procedure main
    val items := [10.0, 20.0, 30.0]
    val idx := identity(1)
    val v := items[idx]
    print(v)
";
        let funcs = monomorphize(src);

        // Find `main` and check that `identity` calls in its body
        // were rewritten to a specialized name.
        let main = funcs
            .iter()
            .find(|f| f.name == "main")
            .expect("main function was dropped by monomorphizer");

        fn contains_call_named(expr: &Expr, name: &str) -> bool {
            match expr {
                Expr::FunctionCall { name: n, args, .. } => {
                    if n == name {
                        return true;
                    }
                    args.iter().any(|a| contains_call_named(a, name))
                }
                Expr::Block { statements, .. } => statements.iter().any(|s| {
                    matches!(s, Stmt::VarDecl { value, .. } if contains_call_named(value, name))
                        || matches!(s, Stmt::Expression(e) if contains_call_named(e, name))
                }),
                _ => false,
            }
        }

        let any_unrewritten = main.body.iter().any(|s| match s {
            Stmt::VarDecl { value, .. } => contains_call_named(value, "identity"),
            Stmt::Expression(e) => contains_call_named(e, "identity"),
            _ => false,
        });

        assert!(
            !any_unrewritten,
            "some call to `identity` was not rewritten to its specialized name"
        );
    }
}