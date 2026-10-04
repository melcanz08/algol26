// src/semantics/analyzer/items.rs

use super::*;
use crate::common::types::{EnumTypeId, SubrangeTypeId};
use crate::frontend::ast::{RecordDecl, TypeSyntax};

impl SemanticAnalyzer {
    pub(super) fn register_builtin_functions(&mut self) {
        let math_functions = [
            ("Math.sqrt", vec![("x", Type::Float)], Type::Float),
            (
                "Math.pow",
                vec![("x", Type::Float), ("y", Type::Float)],
                Type::Float,
            ),
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
                    params: params
                        .into_iter()
                        .map(|(n, t)| (n.to_string(), t))
                        .collect(),
                    return_type,
                    type_params: Vec::new(),
                },
            );
        }
        let string_functions = [
            ("String.length", vec![("s", Type::String)], Type::Int),
            (
                "String.concat",
                vec![("s1", Type::String), ("s2", Type::String)],
                Type::String,
            ),
            (
                "String.substring",
                vec![
                    ("s", Type::String),
                    ("start", Type::Int),
                    ("length", Type::Int),
                ],
                Type::String,
            ),
            ("String.to_upper", vec![("s", Type::String)], Type::String),
            ("String.to_lower", vec![("s", Type::String)], Type::String),
            ("String.trim", vec![("s", Type::String)], Type::String),
            (
                "String.split",
                vec![("s", Type::String), ("sep", Type::String)],
                Type::list(Type::String),
            ),
            (
                "String.join",
                vec![("xs", Type::list(Type::String)), ("sep", Type::String)],
                Type::String,
            ),
        ];
        for (name, params, return_type) in string_functions {
            self.functions.insert(
                name.to_string(),
                FunctionInfo {
                    params: params
                        .into_iter()
                        .map(|(n, t)| (n.to_string(), t))
                        .collect(),
                    return_type,
                    type_params: Vec::new(),
                },
            );
        }
        let file_functions = [
            ("File.read", vec![("path", Type::String)], Type::String),
            (
                "File.write",
                vec![("path", Type::String), ("content", Type::String)],
                Type::Int,
            ),
            (
                "File.append",
                vec![("path", Type::String), ("content", Type::String)],
                Type::Int,
            ),
        ];
        for (name, params, return_type) in file_functions {
            self.functions.insert(
                name.to_string(),
                FunctionInfo {
                    params: params
                        .into_iter()
                        .map(|(n, t)| (n.to_string(), t))
                        .collect(),
                    return_type,
                    type_params: Vec::new(),
                },
            );
        }
        let list_functions = [
            (
                "List.length",
                vec![("arr", Type::list(Type::Unknown))],
                Type::Int,
            ),
            (
                "List.sum",
                vec![("arr", Type::list(Type::Unknown))],
                Type::Float,
            ),
            (
                "List.max",
                vec![("arr", Type::list(Type::Unknown))],
                Type::Float,
            ),
            (
                "List.min",
                vec![("arr", Type::list(Type::Unknown))],
                Type::Float,
            ),
        ];
        for (name, params, return_type) in list_functions {
            self.functions.insert(
                name.to_string(),
                FunctionInfo {
                    params: params
                        .into_iter()
                        .map(|(n, t)| (n.to_string(), t))
                        .collect(),
                    return_type,
                    type_params: Vec::new(),
                },
            );
        }
        self.functions.insert(
            "alloc".to_string(),
            FunctionInfo {
                params: vec![("size".to_string(), Type::Int)],
                return_type: Type::pointer(Type::Unknown),
                type_params: Vec::new(),
            },
        );
        self.functions.insert(
            "free".to_string(),
            FunctionInfo {
                params: vec![("ptr".to_string(), Type::pointer(Type::Unknown))],
                return_type: Type::Void,
                type_params: Vec::new(),
            },
        );
        self.functions.insert(
            "affirm".to_string(),
            FunctionInfo {
                params: vec![
                    ("cond".to_string(), Type::Bool),
                    ("msg".to_string(), Type::String),
                ],
                return_type: Type::Void,
                type_params: Vec::new(),
            },
        );
        self.functions.insert(
            "args".to_string(),
            FunctionInfo {
                params: vec![],
                return_type: Type::list(Type::String),
                type_params: Vec::new(),
            },
        );
        self.functions.insert(
            "Int.to_string".to_string(),
            FunctionInfo {
                params: vec![("n".to_string(), Type::Int)],
                return_type: Type::String,
                type_params: Vec::new(),
            },
        );
        self.functions.insert(
            "String.to_int".to_string(),
            FunctionInfo {
                params: vec![("s".to_string(), Type::String)],
                return_type: Type::option(Type::Int),
                type_params: Vec::new(),
            },
        );
    }
    /// Register every `enum Name ...` declaration.
    ///
    /// Assigns a fresh `EnumTypeId` per declaration, in declaration
    /// order, so ids are deterministic for a given source set.
    /// Rejects empty enums (already caught at parse time) and
    /// duplicate variant names within a declaration.
    pub(super) fn register_enum_types(
        &mut self,
        decls: &[crate::frontend::ast::EnumDecl],
    ) -> Result<()> {
        for decl in decls {
            if self.enum_types.contains_key(&decl.name) {
                return Err(CompileError::at(
                    decl.span,
                    &format!("Duplicate enum declaration '{}'", decl.name),
                    ErrorCode::E0009,
                ));
            }
            let mut seen = HashSet::new();
            for variant in &decl.variants {
                if !seen.insert(variant.clone()) {
                    return Err(CompileError::at(
                        decl.span,
                        &format!("Duplicate variant '{}' in enum '{}'", variant, decl.name),
                        ErrorCode::E0009,
                    ));
                }
            }
            let id = EnumTypeId(self.next_enum_id);
            self.next_enum_id += 1;
            let ty = Type::enum_type(id, &decl.name, decl.variants.clone());
            self.enum_types.insert(decl.name.clone(), ty);
        }
        Ok(())
    }

    /// Register every `type X Base in Low..High` declaration.
    ///
    /// Assigns a fresh `SubrangeTypeId` per declaration, in
    /// declaration order. Validates: base is Int or a registered
    /// enum; bounds are Int literals (Int base) or variant names
    /// (enum base); low <= high. See ADR 0031.
    pub(super) fn register_subrange_types(
        &mut self,
        decls: &[crate::frontend::ast::SubrangeDecl],
    ) -> Result<()> {
        for decl in decls {
            if self.subrange_types.contains_key(&decl.name) {
                return Err(CompileError::at(
                    decl.span,
                    &format!("Duplicate subrange declaration '{}'", decl.name),
                    ErrorCode::E0009,
                ));
            }
            let base = self.resolve_type_syntax(&decl.base)?;
            let (low, high) = self.resolve_subrange_bounds(&decl.name, &base, decl)?;
            if low > high {
                return Err(CompileError::at(
                    decl.span,
                    &format!(
                        "Subrange '{}' has low > high ({}..{})",
                        decl.name, low, high
                    ),
                    ErrorCode::E0002,
                ));
            }
            let id = SubrangeTypeId(self.next_subrange_id);
            self.next_subrange_id += 1;
            let ty = Type::subrange(id, &decl.name, base, low, high);
            self.subrange_types.insert(decl.name.clone(), ty);
        }
        Ok(())
    }

    /// Resolve subrange bounds. For Int bases, both bounds must be
    /// Int literals. For enum bases, they must be variant names,
    /// resolved to their ordinals. See ADR 0031.
    fn resolve_subrange_bounds(
        &self,
        decl_name: &str,
        base: &Type,
        decl: &crate::frontend::ast::SubrangeDecl,
    ) -> Result<(i64, i64)> {
        use crate::frontend::ast::ExprKind;
        match base {
            Type::Int => {
                let low = match &decl.low.kind {
                    ExprKind::Int(n, _) => *n,
                    _ => {
                        return Err(CompileError::at(
                            decl.low.span(),
                            &format!(
                                "Int subrange '{}' requires an integer literal for its lower bound",
                                decl_name
                            ),
                            ErrorCode::E0002,
                        ));
                    }
                };
                let high = match &decl.high.kind {
                    ExprKind::Int(n, _) => *n,
                    _ => {
                        return Err(CompileError::at(
                            decl.high.span(),
                            &format!(
                                "Int subrange '{}' requires an integer literal for its upper bound",
                                decl_name
                            ),
                            ErrorCode::E0002,
                        ));
                    }
                };
                Ok((low, high))
            }
            Type::Enum {
                name: enum_name,
                variants,
                ..
            } => {
                let resolve = |expr: &crate::frontend::ast::Expr, which: &str| -> Result<i64> {
                    match &expr.kind {
                        ExprKind::Var(n, _) => variants
                            .iter()
                            .position(|v| v == n)
                            .map(|i| i as i64)
                            .ok_or_else(|| {
                                CompileError::at(
                                    expr.span(),
                                    &format!("no variant '{}' on enum '{}'", n, enum_name),
                                    ErrorCode::E0004,
                                )
                            }),
                        _ => Err(CompileError::at(
                            expr.span(),
                            &format!(
                                "enum subrange '{}' requires a variant name for its {} bound",
                                decl_name, which
                            ),
                            ErrorCode::E0002,
                        )),
                    }
                };
                let low = resolve(&decl.low, "lower")?;
                let high = resolve(&decl.high, "upper")?;
                Ok((low, high))
            }
            other => Err(CompileError::at(
                decl.span,
                &format!(
                    "Subrange '{}' requires an Int or enum base, found {}",
                    decl_name, other
                ),
                ErrorCode::E0002,
            )),
        }
    }

    /// Register every `type X distinct Y` declaration.
    ///
    /// Assigns a fresh `NominalTypeId` per declaration, in
    /// declaration order, so ids are deterministic for a given
    /// source set. Validates that the base is one of `Int`,
    /// `Float`, `Bool`, `String` — the v1 constraint from ADR
    /// 0029.
    pub(super) fn register_nominal_types(
        &mut self,
        decls: &[crate::frontend::ast::DistinctDecl],
    ) -> Result<()> {
        for decl in decls {
            if self.nominal_types.contains_key(&decl.name) {
                return Err(CompileError::at(
                    decl.span,
                    &format!("Duplicate nominal type '{}'", decl.name),
                    ErrorCode::E0009,
                ));
            }
            let base = self.resolve_type_syntax(&decl.base)?;
            if !matches!(base, Type::Int | Type::Float | Type::Bool | Type::String) {
                return Err(CompileError::at(
                    decl.span,
                    &format!(
                        "Nominal type '{}' must have a primitive base \
                         (Int, Float, Bool, or String), found {}",
                        decl.name, base
                    ),
                    ErrorCode::E0002,
                ));
            }
            let id = NominalTypeId(self.next_nominal_id);
            self.next_nominal_id += 1;
            let ty = Type::distinct(id, &decl.name, base);
            self.nominal_types.insert(decl.name.clone(), ty);
        }
        Ok(())
    }

    pub(super) fn register_user_functions(&mut self, functions: &[FunctionDecl]) -> Result<()> {
        // Snapshot the record and nominal tables so the closure can
        // read them without conflicting with the `&mut self` of the
        // loop.
        let records_snapshot = self.records.clone();
        let nominals_snapshot = self.nominal_types.clone();
        let enums_snapshot = self.enum_types.clone();
        let subranges_snapshot = self.subrange_types.clone();
        for func in functions {
            let params: Vec<(String, Type)> = func
                .params
                .iter()
                .map(|(name, t)| {
                    let type_ = match t {
                        Some(ts) => Self::resolve_syntax_with_records(
                            ts,
                            &records_snapshot,
                            &nominals_snapshot,
                            &enums_snapshot,
                            &subranges_snapshot,
                        )?,
                        None => Type::Unknown,
                    };
                    Ok::<_, CompileError>((name.clone(), type_))
                })
                .collect::<Result<Vec<_>>>()?;

            let return_type = match &func.return_type {
                Some(t) => Self::resolve_syntax_with_records(
                    t,
                    &records_snapshot,
                    &nominals_snapshot,
                    &enums_snapshot,
                    &subranges_snapshot,
                )?,
                None => Type::Void,
            };

            let clean_name = func.name.trim_end_matches("()").to_string();
            if func.ffi_info.as_ref().is_some_and(|f| f.variadic) {
                self.variadic_functions.insert(clean_name.clone());
            }
            self.functions.insert(
                clean_name,
                FunctionInfo {
                    params,
                    return_type,
                    type_params: func.type_params.clone(),
                },
            );
        }
        Ok(())
    }
    pub(super) fn register_record(&mut self, decl: &RecordDecl) -> Result<()> {
        if self.records.contains_key(&decl.name) {
            return Err(CompileError::simple(
                &format!("Duplicate record declaration '{}'", decl.name),
                0,
                0,
                "",
                ErrorCode::E0009,
            ));
        }

        // Duplicate field names are an error.
        let mut seen = HashSet::new();
        for (field, _) in &decl.fields {
            if !seen.insert(field.clone()) {
                return Err(CompileError::simple(
                    &format!("Duplicate field '{}' in record '{}'", field, decl.name),
                    0,
                    0,
                    "",
                    ErrorCode::E0009,
                ));
            }
        }

        // Resolve field types. Type parameters are visible here as TypeVars.
        self.type_params.push(
            decl.type_params
                .iter()
                .map(|p| (p.clone(), Type::TypeVar(p.clone())))
                .collect(),
        );
        let fields: Vec<(String, Type)> = decl
            .fields
            .iter()
            .map(|(name, ty)| Ok((name.clone(), self.resolve_type_syntax(ty)?)))
            .collect::<Result<Vec<_>>>()?;
        self.type_params.pop();

        self.records.insert(
            decl.name.clone(),
            RecordInfo {
                name: decl.name.clone(),
                type_params: decl.type_params.clone(),
                fields,
            },
        );
        Ok(())
    }
    pub(super) fn analyze_function(&mut self, func: &FunctionDecl) -> Result<()> {
        if func.is_extern {
            // Declaration-site FFI boundary check. A signature no
            // caller could legally use is rejected here — this is
            // strictly better than a call-site check because it
            // catches the bug once, at its source, with a message
            // that points at the declaration rather than an
            // arbitrary call.
            //
            // See docs/status/ffi-boundary.md.
            for (name, type_annotation) in &func.params {
                let ty = match type_annotation {
                    Some(annot) => self.resolve_type_syntax(annot)?,
                    None => Type::Unknown,
                };
                if !ty.is_ffi_compatible() || matches!(ty, Type::Void) {
                    return Err(CompileError::simple(
                        &format!(
                            "E-FFI-001: extern \"C\" parameter '{}' has type `{}`, \
                             which cannot cross the FFI boundary",
                            name, ty
                        ),
                        0,
                        0,
                        "",
                        ErrorCode::E0002,
                    )
                    .with_suggestion(
                        "FFI-compatible parameter types are Int, Float, Bool, \
                         String, Ptr, and *T. Composite types (List, Option, \
                         Record, Map, Array, Tuple) and references (&T, &mut T) \
                         have no C ABI representation.",
                    ));
                }
            }

            if let Some(ret_annot) = &func.return_type {
                let ret = self.resolve_type_syntax(ret_annot)?;
                if !ret.is_ffi_compatible() {
                    return Err(CompileError::simple(
                        &format!(
                            "E-FFI-001: extern \"C\" return type `{}` cannot \
                             cross the FFI boundary",
                            ret
                        ),
                        0,
                        0,
                        "",
                        ErrorCode::E0002,
                    )
                    .with_suggestion(
                        "FFI-compatible return types are Int, Float, Bool, \
                         String, Ptr, *T, and Void.",
                    ));
                }
            }

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
                    0,
                    0,
                    "",
                    ErrorCode::E0004,
                )
                .with_suggestion(&format!(
                    "Define trait '{}' before using it as a constraint",
                    clause.trait_name
                )));
            }
        }

        let return_type = if let Some(ret_type) = &func.return_type {
            self.resolve_type_syntax(ret_type)?
        } else {
            Type::Void
        };
        self.current_return_type = Some(return_type.clone());

        for (name, type_annotation) in &func.params {
            let param_type = if let Some(annot) = type_annotation {
                self.resolve_type_syntax(annot)?
            } else {
                Type::Unknown
            };
            self.declare_variable(name, param_type, true)?;
        }
        for stmt in &func.body {
            self.analyze_stmt(stmt)?;
        }

        if return_type != Type::Void {
            let has_return = self.check_all_paths_return(&func.body);
            if !has_return {
                return Err(CompileError::simple(
                    &format!(
                        "Function '{}' may not return a value on all paths",
                        func.name
                    ),
                    0,
                    0,
                    "",
                    ErrorCode::E0002,
                )
                .with_suggestion("Add a return statement to all code paths"));
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
    /// ADR 0032: verify that `ty` is a legal `Set<T>` element type.
    ///
    /// Legal: `Bool`, `Enum` with ≤ 64 variants, `Subrange` with
    /// ≤ 64 values. Everything else — `Int` (unbounded), `Float`
    /// (not ordinal), `String`, `Distinct`, and every composite
    /// type — is rejected here.
    ///
    /// This is called from two places: `resolve_type_syntax` when
    /// the whole `Set<T>` appears in type-syntax position, and the
    /// analyzer's `SetLiteral` arm when the parser has already
    /// split off the outer `Set<...>` and handed us only `T`.
    pub(super) fn validate_set_element_type(&self, ty: &Type, span: Span) -> Result<()> {
        if ty.set_domain_size().is_some() {
            return Ok(());
        }
        Err(CompileError::at(
            span,
            &format!(
                "`{}` cannot be a set element type: its domain is \
                 unbounded or exceeds 64 values",
                ty
            ),
            ErrorCode::E0002,
        )
        .with_suggestion(
            "Set element types must be Bool, an enum with ≤ 64 variants, \
             or a subrange of ≤ 64 values",
        ))
    }
    /// Resolve a `TypeSyntax` in the analyzer's current context.
    ///
    /// `TypeSyntax::to_type` is a pure syntax-to-type function — it
    /// knows primitives and single-letter type variables but has no
    /// access to the record table. This helper adds record lookup
    /// and, critically, **errors** when a name that looks like a
    /// user type (multi-char, not a primitive) fails to resolve.
    ///
    /// Before this change, such a name silently became
    /// `Type::Unknown`. The failure then surfaced three steps
    /// downstream, at some unrelated operation that used the
    /// `Unknown` value. The records-cross-module bug
    /// (`d7b9e4d`) took an hour to find because of this.
    pub(super) fn resolve_type_syntax(&self, syntax: &TypeSyntax) -> Result<Type> {
        match syntax {
            TypeSyntax::Named(name) => {
                if let Some(enum_ty) = self.enum_types.get(name.as_str()).cloned() {
                    return Ok(enum_ty);
                }
                if let Some(nominal) = self.nominal_types.get(name.as_str()).cloned() {
                    return Ok(nominal);
                }
                if let Some(subrange) = self.subrange_types.get(name.as_str()).cloned() {
                    return Ok(subrange);
                }
                if let Some(rec) = self.records.get(name.as_str()).cloned() {
                    let args: Vec<Type> = rec.type_params.iter().map(|_| Type::Unknown).collect();
                    return Ok(Type::record(name, args));
                }
                let ty = self.parse_type_annotation(syntax);
                if ty == Type::Unknown && is_likely_user_type(name) {
                    return Err(unknown_type_error(name));
                }
                Ok(ty)
            }
            TypeSyntax::Generic { name, args } => {
                if let Some(rec) = self.records.get(name.as_str()).cloned() {
                    if args.len() != rec.type_params.len() {
                        return Ok(Type::Unknown);
                    }
                    let resolved_args: Vec<Type> = args
                        .iter()
                        .map(|a| self.resolve_type_syntax(a))
                        .collect::<Result<Vec<_>>>()?;
                    return Ok(Type::record(name, resolved_args));
                }
                // Not a record. Resolve the args through this same
                // function so nested records (e.g. `Option<Sale>`,
                // `Map<String, Sale>`) aren't lost when the outer
                // generic is a builtin.
                let resolved_args: Vec<Type> = args
                    .iter()
                    .map(|a| self.resolve_type_syntax(a))
                    .collect::<Result<Vec<_>>>()?;
                let ty = match (name.to_lowercase().as_str(), resolved_args.as_slice()) {
                    ("list", [inner]) => Type::list(inner.clone()),
                    ("option", [inner]) => Type::option(inner.clone()),
                    ("result", [ok, err]) => Type::result(ok.clone(), err.clone()),
                    ("pointer", [inner]) | ("ptr", [inner]) => Type::pointer(inner.clone()),
                    ("borrow", [inner]) => Type::borrow(inner.clone()),
                    ("mutborrow", [inner]) | ("mut_borrow", [inner]) => {
                        Type::mut_borrow(inner.clone())
                    }
                    ("channel", [inner]) => Type::channel(inner.clone()),
                    ("map", [k, v]) => Type::map(k.clone(), v.clone()),
                    ("set", [inner]) => {
                        // ADR 0032. The whole `Set<T>` is present in
                        // type-syntax position here, so validation
                        // happens right here.
                        self.validate_set_element_type(inner, Span::new(0, 0, 0, 0))?;
                        Type::set(inner.clone())
                    }
                    _ => syntax.to_type(),
                };
                Ok(ty)
            }
            TypeSyntax::Unknown => Ok(Type::Unknown),
        }
    }
    /// True if every control-flow path through `stmts` ends in a
    /// `return`, `break`, or other diverging statement.
    ///
    /// This is a heuristic, not a real CFG reachability analysis.
    /// It handles the constructs that appear in practice:
    ///
    /// - A bare `return` returns `true` immediately (subsequent
    ///   statements are unreachable).
    /// - Any statement whose top-level expression guarantees a
    ///   return — see `expr_guarantees_return`.
    ///
    /// It does NOT handle `while true { ... }` (needs a break check)
    /// or `for` (may run zero times), so those are conservatively
    /// treated as "not guaranteed to return." A function that relies
    /// on a loop to return must have an explicit `return` after it.
    pub(super) fn check_all_paths_return(&self, stmts: &[Stmt]) -> bool {
        for stmt in stmts {
            match stmt {
                Stmt::Return { .. } => return true,
                Stmt::Expression(expr) if self.expr_guarantees_return(expr) => {
                    return true;
                }
                _ => {}
            }
        }
        false
    }

    /// True if evaluating `expr` always ends in a `return`.
    ///
    /// Recursively descends into the constructs that can guarantee
    /// a return:
    ///
    /// - `Block { statements, trailing_expr }` — either its
    ///   statements always return, or its trailing expression does.
    ///   The trailing-expression case is what makes `else if`
    ///   chains work: the parser represents `else if X` as
    ///   `Block { statements: [], trailing_expr: Some(If { X }) }`,
    ///   so the return-detection must look past the empty
    ///   statement list to the nested `If`.
    /// - `If { then, else: Some(else) }` — both branches must
    ///   guarantee a return.
    /// - `Match { cases }` — non-empty, and every case body must
    ///   guarantee a return.
    /// - `TryCatch { try, catch }` — both branches must guarantee
    ///   a return. `finally` runs after either branch and does not
    ///   affect whether a return happens.
    ///
    /// Everything else returns `false`: the expression might
    /// produce a value without returning, or might diverge, and
    /// this check is conservative by design.
    fn expr_guarantees_return(&self, expr: &Expr) -> bool {
        match &expr.kind {
            ExprKind::Block {
                statements,
                trailing_expr,
                ..
            } => {
                self.check_all_paths_return(statements)
                    || trailing_expr
                        .as_ref()
                        .is_some_and(|te| self.expr_guarantees_return(te))
            }
            ExprKind::If {
                then_branch,
                else_branch: Some(else_branch),
                ..
            } => {
                self.expr_guarantees_return(then_branch) && self.expr_guarantees_return(else_branch)
            }
            ExprKind::If {
                else_branch: None, ..
            } => false,
            ExprKind::Match { cases, .. } => {
                !cases.is_empty() && cases.iter().all(|c| self.expr_guarantees_return(&c.body))
            }
            ExprKind::TryCatch {
                try_branch,
                catch_branch,
                ..
            } => {
                self.expr_guarantees_return(try_branch) && self.expr_guarantees_return(catch_branch)
            }
            _ => false,
        }
    }
    /// The body of `resolve_type_syntax`, but taking the record
    /// table as an argument. Used by `register_user_functions`,
    /// which runs during analysis setup and cannot hold `&self`
    /// while it mutates `self.functions`.
    fn resolve_syntax_with_records(
        syntax: &TypeSyntax,
        records: &HashMap<String, RecordInfo>,
        nominals: &HashMap<String, Type>,
        enums: &HashMap<String, Type>,
        subranges: &HashMap<String, Type>,
    ) -> Result<Type> {
        match syntax {
            TypeSyntax::Named(name) => {
                if let Some(enum_ty) = enums.get(name.as_str()) {
                    return Ok(enum_ty.clone());
                }
                if let Some(nominal) = nominals.get(name.as_str()) {
                    return Ok(nominal.clone());
                }
                if let Some(subrange) = subranges.get(name.as_str()) {
                    return Ok(subrange.clone());
                }
                if let Some(rec) = records.get(name.as_str()) {
                    let args: Vec<Type> = rec.type_params.iter().map(|_| Type::Unknown).collect();
                    return Ok(Type::record(name, args));
                }
                let ty = syntax.to_type();
                if ty == Type::Unknown && is_likely_user_type(name) {
                    return Err(unknown_type_error(name));
                }
                Ok(ty)
            }
            TypeSyntax::Generic { name, args } => {
                if let Some(rec) = records.get(name.as_str()) {
                    if args.len() != rec.type_params.len() {
                        return Ok(Type::Unknown);
                    }
                    let resolved_args: Vec<Type> = args
                        .iter()
                        .map(|a| {
                            Self::resolve_syntax_with_records(
                                a, records, nominals, enums, subranges,
                            )
                        })
                        .collect::<Result<Vec<_>>>()?;
                    return Ok(Type::record(name, resolved_args));
                }
                let resolved_args: Vec<Type> = args
                    .iter()
                    .map(|a| {
                        Self::resolve_syntax_with_records(a, records, nominals, enums, subranges)
                    })
                    .collect::<Result<Vec<_>>>()?;
                let ty = match (name.to_lowercase().as_str(), resolved_args.as_slice()) {
                    ("list", [inner]) => Type::list(inner.clone()),
                    ("option", [inner]) => Type::option(inner.clone()),
                    ("result", [ok, err]) => Type::result(ok.clone(), err.clone()),
                    ("pointer", [inner]) | ("ptr", [inner]) => Type::pointer(inner.clone()),
                    ("borrow", [inner]) => Type::borrow(inner.clone()),
                    ("mutborrow", [inner]) | ("mut_borrow", [inner]) => {
                        Type::mut_borrow(inner.clone())
                    }
                    ("channel", [inner]) => Type::channel(inner.clone()),
                    ("map", [k, v]) => Type::map(k.clone(), v.clone()),
                    _ => syntax.to_type(),
                };
                Ok(ty)
            }
            TypeSyntax::Unknown => Ok(Type::Unknown),
        }
    }
}

/// Heuristic: does this name look like a user-declared type rather
/// than a primitive or single-letter type parameter?
///
/// The check is conservative — a false positive turns a valid
/// program into a compile error — so we only flag names that have
/// no other plausible reading. Builtin type constructors are tried
/// by `TypeSyntax::to_type` before this function runs; a lowercase
/// name that reaches here is a typo of a builtin, not a user type,
/// and is reported the same way.
fn is_likely_user_type(name: &str) -> bool {
    // Single-letter names are the type-parameter convention.
    if name.len() <= 1 {
        return false;
    }
    // Lowercase names are reserved for builtin type constructors
    // (`list`, `option`, `borrow`, `pointer`, ...). Those are
    // tried by `TypeSyntax::to_type` before we get here; if one
    // fell through, it is a typo in a builtin name, not a user
    // type. Report it the same way anyway — the diagnostic names
    // the identifier and the file the user wrote.
    !matches!(
        name,
        "Int" | "Float" | "String" | "Bool" | "Void" | "Ptr" | "Never" | "Self"
    )
}

fn unknown_type_error(name: &str) -> CompileError {
    CompileError::simple(
        &format!(
            "Unknown type `{}`. It is not a primitive, a declared record, \
             or a type parameter in scope.",
            name
        ),
        0,
        0,
        "",
        ErrorCode::E0003,
    )
    .with_suggestion(&format!(
        "Declare the type with `rec {} ...`, check the spelling, or import \
         the file where `{}` is declared",
        name, name
    ))
}
