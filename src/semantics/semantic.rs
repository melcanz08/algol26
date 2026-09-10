// src/semantics/semantic.rs - Orthogonal + unified types + type table

use crate::common::diagnostics::{CompileError, ErrorCode, Result};
use crate::common::types::Type;
use crate::frontend::ast::{
    BinOp, Expr, FunctionDecl, ImplBlock, Pattern, Stmt, TraitDecl, WhereClause,
};
use crate::semantics::trait_registry::TraitRegistry;
use std::collections::{HashMap, HashSet};

// ─── Borrow-checking model ──────────────────────────────────────────────
//
// Borrow lifetimes are *lexical*. A borrow of `x` created inside scope S
// stays alive until S is popped. This is the same model Rust used before
// NLL (Non-Lexical Lifetimes) landed.
//
// Bookkeeping lives in four parallel vectors, each one an entry per scope
// on the scope stack:
//
//   borrowed_vars      — immutable borrows of a variable, per scope
//   mutably_borrowed   — mutable borrows of a variable, per scope
//   mutable_borrows    — reference-name → source-name, per scope
//   moved_vars         — variables whose ownership was transferred, per scope
//
// Because each scope has its own entry, `pop_scope` releases all borrows
// introduced in that scope automatically — no explicit cleanup needed.
//
// Known limitation: NLL is not implemented, so a borrow lives until the
// end of its enclosing block, not until the last use of the reference.
// Programs that rely on NLL may be rejected conservatively.
// ────────────────────────────────────────────────────────────────────────
pub struct SemanticAnalyzer {
    span_map: std::collections::HashMap<usize, (usize, usize)>,
    scopes: Vec<HashMap<String, (Type, bool)>>,
    moved_vars: Vec<Vec<String>>,
    borrowed_vars: Vec<HashSet<String>>,
    mutably_borrowed: Vec<HashSet<String>>,
    in_mut_borrow: bool,
    mutable_borrows: Vec<HashMap<String, String>>,
    functions: HashMap<String, FunctionInfo>,
    current_return_type: Option<Type>,
    list_lengths: Vec<HashMap<String, usize>>,
    list_values: Vec<HashMap<String, Vec<Expr>>>,
    type_params: Vec<HashMap<String, Type>>,
    type_constraints: Vec<HashMap<String, Vec<String>>>,
    trait_registry: TraitRegistry,
    deferred_captures: Vec<HashSet<String>>,

    // ─── UNIFY TYPES ─── New: inferred type of each expression, keyed by address.
    // Addresses are stable because the analyzer and IR builder walk the *same*
    // AST without cloning.
    pub type_table: HashMap<usize, Type>,
}

#[derive(Debug, Clone)]
struct FunctionInfo {
    params: Vec<(String, Type)>,
    return_type: Type,
}

impl Default for SemanticAnalyzer {
    fn default() -> Self {
        Self::new()
    }
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
            in_mut_borrow: false,
            mutable_borrows: vec![HashMap::new()],
            current_return_type: None,
            list_lengths: vec![HashMap::new()],
            list_values: vec![HashMap::new()],
            type_params: vec![HashMap::new()],
            type_constraints: vec![HashMap::new()],
            trait_registry: TraitRegistry::new(),
            deferred_captures: vec![HashSet::new()],

            // ─── UNIFY TYPES ───
            type_table: HashMap::new(),
        }
    }

    // ─── UNIFY TYPES ───────────────────────────────────────────────────────
    /// Look up the inferred type of an expression by its address.
    pub fn type_of(&self, expr: &Expr) -> Option<&Type> {
        self.type_table.get(&(expr as *const Expr as usize))
    }

    /// Take ownership of the type table so it can be handed to the IR builder.
    pub fn take_type_table(&mut self) -> HashMap<usize, Type> {
        std::mem::take(&mut self.type_table)
    }
    // ───────────────────────────────────────────────────────────────────────

    fn push_scope(&mut self) {
        self.deferred_captures.push(HashSet::new());
        self.mutable_borrows.push(HashMap::new());
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
        self.deferred_captures.pop();
        self.scopes.pop();
        self.moved_vars.pop();
        self.borrowed_vars.pop();
        self.mutably_borrowed.pop();
        self.mutable_borrows.pop();
        self.list_lengths.pop();
        self.list_values.pop();
        self.type_params.pop();
        self.type_constraints.pop();
    }

    fn release_mutable_borrow(&mut self, reference: &str) {
        // Find the innermost scope that holds `reference`, and release it
        // *only there*. Removing from all scopes could accidentally clear
        // an outer scope's borrow of the same source.
        let scope_idx = self
            .mutable_borrows
            .iter()
            .enumerate()
            .rev()
            .find(|(_, map)| map.contains_key(reference))
            .map(|(i, _)| i);

        let Some(idx) = scope_idx else {
            return;
        };

        // Extract the source before releasing.
        let source = match self.mutable_borrows[idx].remove(reference) {
            Some(src) => src,
            None => return,
        };

        // Remove the source from the same scope's mutably_borrowed set.
        if let Some(set) = self.mutably_borrowed.get_mut(idx) {
            set.remove(&source);
        }
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

    fn collect_deferred_captures(&self, stmt: &Stmt, captured: &mut HashSet<String>) {
        match stmt {
            Stmt::Print { expr } => self.collect_expr_captures(expr, captured),
            Stmt::Assign { name, value } => {
                captured.insert(name.clone());
                self.collect_expr_captures(value, captured);
            }
            Stmt::Expression(expr) => self.collect_expr_captures(expr, captured),
            Stmt::VarDecl { name: _, value, .. } => {
                self.collect_expr_captures(value, captured);
            }
            _ => {}
        }
    }

    fn collect_expr_captures(&self, expr: &Expr, captured: &mut HashSet<String>) {
        match expr {
            Expr::Var(name, _) => {
                captured.insert(name.clone());
            }
            Expr::Binary { left, right, .. } => {
                self.collect_expr_captures(left, captured);
                self.collect_expr_captures(right, captured);
            }
            Expr::FunctionCall { args, .. } => {
                for arg in args {
                    self.collect_expr_captures(arg, captured);
                }
            }
            Expr::ArrayAccess { array, index } => {
                self.collect_expr_captures(array, captured);
                self.collect_expr_captures(index, captured);
            }
            _ => {}
        }
    }

    fn all_moved_vars(&self) -> Vec<String> {
        let mut result = Vec::new();
        for scope in &self.moved_vars {
            for var in scope {
                if !result.contains(var) {
                    result.push(var.clone());
                }
            }
        }
        result
    }

    fn is_moved(&self, name: &str) -> bool {
        self.moved_vars
            .iter()
            .any(|scope| scope.iter().any(|v| v == name))
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
        self.mutably_borrowed
            .iter()
            .rev()
            .any(|scope| scope.contains(name))
    }

    fn register_mutable_borrow(&mut self, reference: &str, source: &str) -> Result<()> {
        // The source must be declared `var` — you cannot take a mutable
        // borrow of an immutable (`val`) binding.
        match self.lookup_variable(source) {
            Some((_, false)) => {
                return Err(CompileError::simple(
                    &format!("Cannot mutably borrow immutable variable '{}'", source),
                    0, 0, "", ErrorCode::E0007,
                ).with_suggestion(&format!(
                    "Declare '{}' with 'var' instead of 'val'", source
                )));
            }
            Some((_, true)) => {} // mutable, ok
            None => {
                // Source not in scope — let the analyzer report the
                // "undefined variable" error elsewhere.
            }
        }
        if self.is_moved(source) {
            return Err(CompileError::simple(
                &format!("Cannot mutably borrow moved variable '{}'", source),
                0, 0, "", ErrorCode::E0007,
            ).with_suggestion("The variable has already been moved"));
        }
        if self.is_mutably_borrowed(source) {
            return Err(CompileError::simple(
                &format!("Cannot mutably borrow '{}' more than once", source),
                0, 0, "", ErrorCode::E0007,
            ).with_suggestion("Only one mutable borrow is allowed at a time"));
        }
        if self.is_borrowed(source) {
            return Err(CompileError::simple(
                &format!("Cannot mutably borrow '{}' while immutably borrowed", source),
                0, 0, "", ErrorCode::E0007,
            ).with_suggestion("Wait for the immutable borrow to end"));
        }
        self.mark_mutably_borrowed(source);
        if let Some(scope) = self.mutable_borrows.last_mut() {
            scope.insert(reference.to_string(), source.to_string());
        }
        Ok(())
    }

    fn is_borrowed(&self, name: &str) -> bool {
        self.borrowed_vars.iter().rev().any(|scope| scope.contains(name))
    }

    fn check_borrow_rules(&self, name: &str, mutable: bool) -> Result<()> {
        if let Some(scope) = self.deferred_captures.last() {
            if scope.contains(name) {
                return Err(CompileError::simple(
                    &format!("Cannot use '{}' after it was captured by defer", name),
                    0, 0, "", ErrorCode::E0007,
                ).with_suggestion("Deferred statements capture variables at declaration time"));
            }
        }
        if self.is_moved(name) {
            return Err(CompileError::simple(
                &format!("Cannot borrow moved variable '{}'", name),
                0, 0, "", ErrorCode::E0007,
            ).with_suggestion("The variable has been moved and is no longer available"));
        }
        if mutable && self.is_mutably_borrowed(name) {
            return Err(CompileError::simple(
                &format!("Cannot mutably borrow '{}' more than once", name),
                0, 0, "", ErrorCode::E0007,
            ).with_suggestion("Only one mutable borrow is allowed at a time"));
        }
        if mutable && self.is_borrowed(name) {
            return Err(CompileError::simple(
                &format!("Cannot mutably borrow '{}' while immutably borrowed", name),
                0, 0, "", ErrorCode::E0007,
            ).with_suggestion("Wait for the immutable borrow to end"));
        }
        if !mutable && self.is_mutably_borrowed(name) {
            return Err(CompileError::simple(
                &format!("Cannot read '{}' while it is mutably borrowed", name),
                0, 0, "", ErrorCode::E0007,
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

        for trait_decl in traits {
            self.trait_registry.register_trait(trait_decl.clone());
        }
        for impl_block in impls {
            self.trait_registry.register_impl(impl_block.clone());
        }
        for impl_block in impls {
            if let Err(err) = self.trait_registry.validate_impl(impl_block) {
                return Err(CompileError::simple(&err, 0, 0, "", ErrorCode::E0002));
            }
        }
        for func in functions {
            self.analyze_function(func)?;
        }
        Ok(())
    }

    // Keep the old name for compatibility; delegate.
    pub fn analyze_with_traits(
        &mut self,
        functions: &[FunctionDecl],
        traits: &[TraitDecl],
        impls: &[ImplBlock],
        span_map: &std::collections::HashMap<usize, (usize, usize)>,
    ) -> Result<()> {
        self.analyze_with_spans(functions, traits, impls, span_map)
    }

    fn check_trait_bounds(
        &self,
        _type_params: &[String],
        where_clauses: &[WhereClause],
    ) -> Result<()> {
        for clause in where_clauses {
            let trait_name = &clause.trait_name;
            if !self.trait_registry.trait_exists(trait_name) {
                return Err(CompileError::simple(
                    &format!("Unknown trait '{}'", trait_name),
                    0, 0, "", ErrorCode::E0004,
                ).with_suggestion(&format!(
                    "Define trait '{}' before using it as a constraint", trait_name
                )));
            }
        }
        Ok(())
    }

    fn resolve_trait_method(&self, type_: &Type, method_name: &str) -> Option<FunctionDecl> {
        self.trait_registry
            .resolve_method(type_, method_name)
            .cloned()
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
                },
            );
        }

        let string_functions = [
            ("String.length", vec![("s", Type::String)], Type::Int),
            ("String.concat", vec![("s1", Type::String), ("s2", Type::String)], Type::String),
            ("String.substring",
                vec![("s", Type::String), ("start", Type::Int), ("length", Type::Int)],
                Type::String),
            ("String.to_upper", vec![("s", Type::String)], Type::String),
            ("String.to_lower", vec![("s", Type::String)], Type::String),
        ];
        for (name, params, return_type) in string_functions {
            self.functions.insert(
                name.to_string(),
                FunctionInfo {
                    params: params.into_iter().map(|(n, t)| (n.to_string(), t)).collect(),
                    return_type,
                },
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
                },
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
                },
            );
        }

        self.functions.insert(
            "alloc".to_string(),
            FunctionInfo {
                params: vec![("size".to_string(), Type::Int)],
                return_type: Type::pointer(Type::Unknown),
            },
        );
        self.functions.insert(
            "free".to_string(),
            FunctionInfo {
                params: vec![("ptr".to_string(), Type::pointer(Type::Unknown))],
                return_type: Type::Void,
            },
        );
    }

    fn register_user_functions(&mut self, functions: &[FunctionDecl]) {
        for func in functions {
            let params = func.params.iter().map(|(name, t)| {
                let type_ = match t {
                    Some(ts) => ts.to_type(),
                    None => Type::Unknown,
                };
                (name.clone(), type_)
            }).collect();

            let return_type = func.return_type.as_ref()
                .map(|t| t.to_type())
                .unwrap_or(Type::Void);

            let clean_name = func.name.trim_end_matches("()").to_string();
            self.functions.insert(clean_name, FunctionInfo { params, return_type });
        }
    }

    fn parse_type_param(&self, type_str: &str) -> Option<String> {
        let trimmed = type_str.trim();
        if trimmed.len() == 1 && trimmed.chars().next().is_some_and(|c| c.is_uppercase()) {
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

        for type_param in &func.type_params {
            self.declare_type_param(type_param, Type::TypeVar(type_param.clone()));
        }
        self.check_trait_bounds(&func.type_params, &func.where_clauses)?;
        for clause in &func.where_clauses {
            self.declare_type_constraint(&clause.type_param, &clause.trait_name);
            if !self.trait_registry.trait_exists(&clause.trait_name) {
                return Err(CompileError::simple(
                    &format!("Unknown trait '{}' in where clause", clause.trait_name),
                    0, 0, "", ErrorCode::E0004,
                ).with_suggestion(&format!(
                    "Define trait '{}' before using it as a constraint", clause.trait_name
                )));
            }
        }

        let return_type = if let Some(ret_type) = &func.return_type {
            self.parse_type_annotation(ret_type)
        } else {
            Type::Void
        };
        self.current_return_type = Some(return_type.clone());

        for (name, type_annotation) in &func.params {
            let param_type = if let Some(annot) = type_annotation {
                annot.to_type()
            } else {
                Type::Unknown
            };
            self.declare_variable(name, param_type, false)?;
        }

        for stmt in &func.body {
            self.analyze_stmt(stmt)?;
        }

        if return_type != Type::Void {
            let has_return = self.check_all_paths_return(&func.body);
            if !has_return {
                return Err(CompileError::simple(
                    &format!("Function '{}' may not return a value on all paths", func.name),
                    0, 0, "", ErrorCode::E0002,
                ).with_suggestion("Add a return statement to all code paths"));
            }
        }

        self.pop_scope();
        self.current_return_type = None;
        Ok(())
    }

    fn parse_type_annotation(&self, annot: &crate::frontend::ast::TypeSyntax) -> Type {
        let type_str = annot.to_string_rep();
        if let Some(type_param) = self.parse_type_param(&type_str) {
            if let Some(resolved) = self.lookup_type_param(&type_param) {
                return resolved;
            }
            return Type::TypeVar(type_param);
        }
        Type::from_str(&type_str)
    }

    fn check_all_paths_return(&self, stmts: &[Stmt]) -> bool {
        for stmt in stmts {
            match stmt {
                Stmt::Return { .. } => return true,
                Stmt::Expression(Expr::If { then_branch, else_branch, .. }) => {
                    if let Some(else_expr) = else_branch {
                        let then_returns = matches!(then_branch.as_ref(), Expr::Block { statements, .. }
                            if self.check_all_paths_return(statements));
                        let else_returns = matches!(else_expr.as_ref(), Expr::Block { statements, .. }
                            if self.check_all_paths_return(statements));
                        if then_returns && else_returns {
                            return true;
                        }
                    }
                }
                Stmt::Expression(Expr::Block { statements, .. }) => {
                    if self.check_all_paths_return(statements) {
                        return true;
                    }
                }
                _ => {}
            }
        }
        false
    }

    fn analyze_stmt(&mut self, stmt: &Stmt) -> Result<()> {
        self.in_mut_borrow = false;
        match stmt {
            Stmt::VarDecl { name, value, type_annotation, mutable, .. } => {
                // Detect mut-borrow before analyzing so we can set the "allow
                // read during this declaration" flag.
                let mut_borrow_source: Option<String> = if let Expr::MutBorrow { expr } = value {
                    if let Expr::Var(source_name, _) = expr.as_ref() {
                        Some(source_name.clone())
                    } else {
                        None
                    }
                } else {
                    None
                };

                if mut_borrow_source.is_some() {
                    self.in_mut_borrow = true;
                }

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

                // Analyze the value FIRST. If it's a mut-borrow,
                // `check_borrow_rules` inside `Expr::MutBorrow` will fire if
                // the source is already mutably borrowed.
                let value_type = if let Some(annotated) = type_annotation {
                    let expected = Type::from_str(annotated);
                    self.analyze_expr_with_context(value, Some(&expected))?
                } else {
                    self.analyze_expr(value)?
                };

                // NOW register the borrow. Doing this after analysis means
                // the first declaration of `&mut x` sets the flag; the second
                // declaration sees the flag and errors out.
                if let Some(source) = &mut_borrow_source {
                    self.register_mutable_borrow(name, source)?;
                }

                if let Some(annotated) = type_annotation {
                    let expected = Type::from_str(annotated);
                    let is_borrow = matches!(value, Expr::Borrow { .. } | Expr::MutBorrow { .. });
                    if !is_borrow
                        && expected != Type::Unknown
                        && !value_type.can_coerce_to(&expected)
                    {
                        return Err(CompileError::simple(
                            &format!(
                                "Type mismatch: variable '{}' declared as {} but assigned {}",
                                name, expected, value_type
                            ),
                            0, 0, "", ErrorCode::E0002,
                        ).with_suggestion(&format!(
                            "Change the type annotation to {} or change the value to {}",
                            value_type, expected
                        )));
                    }
                }

                self.declare_variable(name, value_type.clone(), *mutable)?;
                self.in_mut_borrow = false;

                if let Expr::Var(source, _) = value {
                    if let Some(scope) = self.deferred_captures.last() {
                        if scope.contains(source) {
                            return Err(CompileError::simple(
                                &format!("Cannot move '{}' after it was captured by defer", source),
                                0, 0, "", ErrorCode::E0007,
                            ).with_suggestion(
                                "Deferred statements capture variables at declaration time",
                            ));
                        }
                    }
                    if source != name && !self.is_moved(source) && !value_type.is_copy() {
                        self.mark_moved(source);
                    }
                }
            }
            Stmt::Assign { name, value } => {
                let (var_type, _mutable) = self.lookup_variable(name).ok_or_else(|| {
                    CompileError::simple(
                        &format!("Undefined variable '{}'", name),
                        0, 0, "", ErrorCode::E0003,
                    ).with_suggestion(&format!(
                        "Declare '{}' with 'var {} := ...' or 'val {} := ...'",
                        name, name, name
                    ))
                })?;

                let target_type = match &var_type {
                    Type::MutBorrow(inner) => (**inner).clone(),
                    _ => var_type.clone(),
                };

                let value_type = self.analyze_expr_with_context(value, Some(&target_type))?;
                if target_type != value_type
                    && target_type != Type::Unknown
                    && !value_type.can_coerce_to(&target_type)
                {
                    return Err(CompileError::simple(
                        &format!(
                            "Type mismatch: cannot assign {} to variable of type {}",
                            value_type, target_type
                        ),
                        0, 0, "", ErrorCode::E0002,
                    ).with_suggestion(&format!(
                        "Change the value to {} or declare variable as {}",
                        target_type, value_type
                    )));
                }
                if self.mutable_borrows.iter().any(|m| m.contains_key(name)) {
                    self.release_mutable_borrow(name);
                }
            }
            Stmt::Expression(expr) => {
                match expr {
                    Expr::If { then_branch, else_branch, condition } => {
                        let cond_type = self.analyze_expr(condition)?;
                        if cond_type != Type::Bool && cond_type != Type::Unknown {
                            return Err(CompileError::simple(
                                "If condition must be Bool", 0, 0, "", ErrorCode::E0002,
                            ));
                        }
                        let moved_before = self.moved_vars.last().cloned().unwrap_or_default();

                        self.push_scope();
                        let then_result = self.analyze_expr(then_branch);
                        let moved_after_then = self.moved_vars.last().cloned().unwrap_or_default();
                        self.pop_scope();
                        then_result?;

                        let moved_after_else = if let Some(else_expr) = else_branch {
                            self.push_scope();
                            let else_result = self.analyze_expr(else_expr);
                            let moved_after = self.moved_vars.last().cloned().unwrap_or_default();
                            self.pop_scope();
                            else_result?;
                            moved_after
                        } else {
                            moved_before.clone()
                        };

                        if let Some(current_scope) = self.moved_vars.last_mut() {
                            for var in &moved_after_then {
                                if !current_scope.contains(var) {
                                    current_scope.push(var.clone());
                                }
                            }
                            for var in &moved_after_else {
                                if !current_scope.contains(var) {
                                    current_scope.push(var.clone());
                                }
                            }
                        }
                    }
                    Expr::Match { .. } | Expr::TryCatch { .. } => {
                        self.push_scope();
                        let result = self.analyze_expr(expr);
                        self.pop_scope();
                        result?;
                    }
                    Expr::For { .. } | Expr::While { .. } => {
                        self.analyze_expr(expr)?;
                    }
                    _ => {
                        self.analyze_expr(expr)?;
                    }
                }
            }
            Stmt::Return { value } => {
                let expected_type = self.current_return_type.clone().unwrap_or(Type::Void);
                match (value, &expected_type) {
                    (Some(_expr), Type::Void) => {
                        return Err(CompileError::simple(
                            "Cannot return a value from a void function",
                            0, 0, "", ErrorCode::E0002,
                        ).with_suggestion(
                            "Remove the return value or change the function return type",
                        ));
                    }
                    (None, Type::Void) => {}
                    (Some(expr), expected) => {
                        let actual_type = self.analyze_expr_with_context(expr, Some(expected))?;
                        let can_return = actual_type.can_coerce_to(expected)
                            || matches!(&actual_type, Type::Borrow(inner) if (**inner).can_coerce_to(expected))
                            || matches!(&actual_type, Type::MutBorrow(inner) if (**inner).can_coerce_to(expected));
                        if !can_return && *expected != Type::Unknown {
                            return Err(CompileError::simple(
                                &format!(
                                    "Return type mismatch: expected {}, found {}",
                                    expected, actual_type
                                ),
                                0, 0, "", ErrorCode::E0002,
                            ).with_suggestion(&format!(
                                "Change the return statement to match {} or change the function signature",
                                expected
                            )));
                        }
                    }
                    (None, expected) => {
                        return Err(CompileError::simple(
                            &format!("Missing return value: function should return {}", expected),
                            0, 0, "", ErrorCode::E0002,
                        ).with_suggestion("Add a return statement with the appropriate value"));
                    }
                }
            }
            Stmt::Print { expr } => { self.analyze_expr(expr)?; }
            Stmt::Break | Stmt::Continue => {}
            Stmt::Defer { stmt } => {
                let mut captured = HashSet::new();
                self.collect_deferred_captures(stmt, &mut captured);
                if let Some(scope) = self.deferred_captures.last_mut() {
                    for var in &captured {
                        scope.insert(var.clone());
                    }
                }
                self.analyze_stmt(stmt)?;
            }
            Stmt::Spawn { body } => {
                self.push_scope();
                for s in body { self.analyze_stmt(s)?; }
                self.pop_scope();
            }
            Stmt::Parallel { blocks } => {
                for block in blocks {
                    self.push_scope();
                    for s in block { self.analyze_stmt(s)?; }
                    self.pop_scope();
                }
            }
            Stmt::ChannelDecl { name } => {
                self.declare_variable(name, Type::channel(Type::Unknown), false)?;
            }
            Stmt::Send { channel, value } => {
                let _ = self.lookup_variable(channel).ok_or_else(|| {
                    CompileError::simple(
                        &format!("Undefined channel '{}'", channel),
                        0, 0, "", ErrorCode::E0003,
                    )
                })?;
                self.analyze_expr(value)?;
            }
            Stmt::Receive { channel, target } => {
                let _ = self.lookup_variable(channel).ok_or_else(|| {
                    CompileError::simple(
                        &format!("Undefined channel '{}'", channel),
                        0, 0, "", ErrorCode::E0003,
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
                for s in body { self.analyze_stmt(s)?; }
                self.pop_scope();
            }
            Stmt::RegionBlock { name: _, body } => {
                self.push_scope();
                for s in body { self.analyze_stmt(s)?; }
                self.pop_scope();
            }
            Stmt::Import { .. } => {}
            Stmt::ArrayAssign { array, index, value } => {
                let (array_type, _) = self.lookup_variable(array).ok_or_else(|| {
                    CompileError::simple(
                        &format!("Undefined array '{}'", array),
                        0, 0, "", ErrorCode::E0003,
                    )
                })?;
                if let Type::List(_) = &array_type {
                } else if array_type != Type::Unknown {
                    return Err(CompileError::simple(
                        &format!("Array assignment requires list, found {}", array_type),
                        0, 0, "", ErrorCode::E0002,
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
            Pattern::Binding(var) => {
                self.declare_variable(var, value_type.clone(), false).ok();
            }
            Pattern::Some(var) => {
                if let Type::Option(inner) = value_type {
                    self.declare_variable(var, *inner.clone(), false).ok();
                } else {
                    self.declare_variable(var, Type::Unknown, false).ok();
                }
            }
            Pattern::SomeNested(inner) => {
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
            _ => {}
        }
    }

    fn analyze_expr(&mut self, expr: &Expr) -> Result<Type> {
        self.analyze_expr_with_context(expr, None)
    }

    // ─── UNIFY TYPES ───────────────────────────────────────────────────────
    // Public entry: calls the inner analyzer and records the resulting type.
    fn analyze_expr_with_context(
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
    fn analyze_expr_inner(
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
                let inner = if let Some(Type::Result { ok, .. }) = expected_type {
                    self.analyze_expr_with_context(value, Some(ok.as_ref()))?
                } else {
                    self.analyze_expr(value)?
                };
                if let Some(Type::Result { error, .. }) = expected_type {
                    Ok(Type::result(inner, (**error).clone()))
                } else {
                    Ok(Type::result(inner, Type::Unknown))
                }
            }
            Expr::Error { value } => {
                let inner = if let Some(Type::Result { error, .. }) = expected_type {
                    self.analyze_expr_with_context(value, Some(error.as_ref()))?
                } else {
                    self.analyze_expr(value)?
                };
                if let Some(Type::Result { ok, .. }) = expected_type {
                    Ok(Type::result((**ok).clone(), inner))
                } else {
                    Ok(Type::result(Type::Unknown, inner))
                }
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
                        result_type = result_type.common_supertype(&case_type);
                    }
                    Ok(result_type)
                } else {
                    Err(CompileError::simple(
                        "Match expression must have at least one case", 0, 0, "", ErrorCode::E0002,
                    ))
                }
            }
            Expr::TryCatch { try_branch, catch_var: _, catch_branch, finally_body: _ } => {
                let try_type = self.analyze_expr(try_branch)?;
                let catch_type = self.analyze_expr(catch_branch)?;
                Ok(try_type.common_supertype(&catch_type))
            }
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
                if !new_moves.is_empty() {
                    let moved_var = new_moves[0].clone();
                    return Err(CompileError::simple(
                        &format!("Cannot move '{}' in loop body", moved_var),
                        0, 0, "", ErrorCode::E0008,
                    ));
                }
                Ok(result_type)
            }
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
                                    .map(|t| Type::from_str(&t.to_string_rep()))
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
                        .map(|t| Type::from_str(&t.to_string_rep()))
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

    fn substitute_type_vars(&self, type_: &Type, bindings: &HashMap<String, Type>) -> Type {
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

    fn declare_variable(&mut self, name: &str, type_: Type, mutable: bool) -> Result<()> {
        if let Some(scope) = self.scopes.last_mut() {
            if scope.contains_key(name) {
                return Err(CompileError::simple(
                    &format!("Variable '{}' already declared", name),
                    0, 0, "", ErrorCode::E0003,
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
            scope
                .entry(type_param.to_string())
                .or_default()
                .push(trait_name.to_string());
        }
    }

    fn resolve_type(&self, type_: &Type) -> Type {
        match type_ {
            Type::TypeVar(name) => self
                .lookup_type_param(name)
                .unwrap_or_else(|| Type::TypeVar(name.clone())),
            Type::List(inner) => Type::list(self.resolve_type(inner)),
            Type::Option(inner) => Type::option(self.resolve_type(inner)),
            Type::Result { ok, error } => {
                Type::result(self.resolve_type(ok), self.resolve_type(error))
            }
            _ => type_.clone(),
        }
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

    // The pattern-type checker is unchanged.
    fn check_pattern_type(&self, pattern: &Pattern, value_type: &Type) -> Result<()> {
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

#[cfg(test)]
mod scope_borrow_tests {
    use super::*;
    use crate::frontend::lexer::Lexer;
    use crate::frontend::parser::Parser;

    fn analyze(source: &str) -> Result<()> {
        let lexer = Lexer::new(source.to_string())?;
        let mut parser = Parser::new(lexer.tokens);
        let program = parser.parse_program()?;
        let mut analyzer = SemanticAnalyzer::new();
        analyzer.analyze_with_spans(
            &program.functions,
            &program.traits,
            &program.impls,
            &std::collections::HashMap::new(),
        )
    }

    #[test]
    fn test_mut_borrow_released_at_scope_exit() {
        // Borrow in the inner block should be released before the second
        // borrow at the outer scope.
        let source = "\
procedure main
    var x := 5.0
    region r
        var y := &mut x
    var z := &mut x
";
        analyze(source).expect("scope-exit release should allow re-borrow");
    }

    #[test]
    fn test_double_mut_borrow_same_scope_fails() {
        let source = "\
procedure main
    var x := 5.0
    var y := &mut x
    var z := &mut x
";
        let result = analyze(source);
        assert!(result.is_err(), "double mut-borrow should fail");
    }
}