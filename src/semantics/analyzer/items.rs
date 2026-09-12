// src/semantics/analyzer/items.rs

use super::*;

impl SemanticAnalyzer {
    pub(super) fn register_builtin_functions(&mut self) {
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
    pub(super) fn register_user_functions(&mut self, functions: &[FunctionDecl]) {
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
    pub(super) fn parse_type_param(&self, type_str: &str) -> Option<String> {
        let trimmed = type_str.trim();
        if trimmed.len() == 1 && trimmed.chars().next().is_some_and(|c| c.is_uppercase()) {
            Some(trimmed.to_string())
        } else {
            None
        }
    }
    pub(super) fn analyze_function(&mut self, func: &FunctionDecl) -> Result<()> {
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
    pub(super) fn parse_type_annotation(&self, annot: &crate::frontend::ast::TypeSyntax) -> Type {
        let ty = annot.to_type();
        // Resolve type variables against declared type params (e.g. `T` → its bound).
        if let Type::TypeVar(name) = &ty {
            if let Some(resolved) = self.lookup_type_param(name) {
                return resolved;
            }
        }
        ty
    }
    pub(super) fn check_all_paths_return(&self, stmts: &[Stmt]) -> bool {
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
}