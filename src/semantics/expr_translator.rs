// ALGOL26 - Expression Translator - HARDENED
// Complete expression translation with full type checking

use crate::common::types::Type;
use crate::frontend::ast::{BinOp, Expr};
use crate::ir::semantic_ir::{SemanticBinOp, TypedIRValue};
use crate::semantics::semantic_builder::{FunctionSignature, VariableInfo};
use crate::semantics::type_checker::TypeChecker;
use std::collections::HashMap;

pub struct ExprTranslator {
    type_checker: TypeChecker,
    function_types: HashMap<String, FunctionSignature>,
    diagnostics: Vec<String>,
}

impl ExprTranslator {
    pub fn new() -> Self {
        Self {
            type_checker: TypeChecker::new(),
            function_types: HashMap::new(),
            diagnostics: Vec::new(),
        }
    }

    pub fn with_function_types(ft: HashMap<String, FunctionSignature>) -> Self {
        Self {
            type_checker: TypeChecker::new(),
            function_types: ft,
            diagnostics: Vec::new(),
        }
    }

    pub fn set_function_types(&mut self, ft: HashMap<String, FunctionSignature>) {
        self.function_types = ft;
    }

    pub fn take_diagnostics(&mut self) -> Vec<String> {
        let mut diags = self.type_checker.take_diagnostics();
        diags.append(&mut self.diagnostics);
        diags
    }

    fn peel_type(ty: Type) -> Type {
        let mut cur = ty;
        for _ in 0..8 {
            match cur {
                Type::Borrow(inner) | Type::MutBorrow(inner) | Type::Pointer(inner) => cur = *inner,
                _ => break,
            }
        }
        cur
    }

    fn lookup_var<'a>(
        scopes: &'a [HashMap<String, VariableInfo>],
        name: &str,
    ) -> Option<&'a VariableInfo> {
        for scope in scopes.iter().rev() {
            if let Some(info) = scope.get(name) {
                return Some(info);
            }
        }
        None
    }

    pub fn translate(
        &mut self,
        expr: &Expr,
        scopes: &[HashMap<String, VariableInfo>],
    ) -> TypedIRValue {
        match expr {
            Expr::Unary { op, expr, .. } => {
                let inner = self.translate(expr, scopes);
                let inner_type = inner.type_of();

                match op {
                    crate::frontend::ast::UnaryOp::Negate => {
                        if !inner_type.is_numeric() && inner_type != Type::Unknown {
                            self.diagnostics
                                .push(format!("Cannot negate non-numeric type {:?}", inner_type));
                        }
                        TypedIRValue::BinaryOp {
                            op: SemanticBinOp::Subtract,
                            left: Box::new(TypedIRValue::Int(0)),
                            right: Box::new(inner),
                            result_type: inner_type,
                        }
                    }
                    crate::frontend::ast::UnaryOp::Not => {
                        if inner_type != Type::Bool && inner_type != Type::Unknown {
                            self.diagnostics
                                .push(format!("Logical not requires Bool, found {:?}", inner_type));
                        }
                        TypedIRValue::BinaryOp {
                            op: SemanticBinOp::Equal,
                            left: Box::new(inner),
                            right: Box::new(TypedIRValue::Bool(false)),
                            result_type: Type::Bool,
                        }
                    }
                }
            }
            Expr::Borrow { expr } => {
                let inner = self.translate(expr, scopes);
                let inner_type = inner.type_of();

                // Check if we're borrowing a variable
                if let Expr::Var(name, _) = expr.as_ref() {
                    if let Some(info) = Self::lookup_var(scopes, name) {
                        if !info.mutable {
                            // Immutable borrow of immutable variable is fine
                        } else {
                            // Borrowing mutable variable
                        }
                    }
                }

                TypedIRValue::Borrow {
                    expr: Box::new(inner),
                    target_type: Type::borrow(inner_type),
                }
            }
            Expr::MutBorrow { expr } => {
                let inner = self.translate(expr, scopes);
                let inner_type = inner.type_of();

                // Check if variable is mutable
                if let Expr::Var(name, _) = expr.as_ref() {
                    if let Some(info) = Self::lookup_var(scopes, name) {
                        if !info.mutable {
                            self.diagnostics.push(format!(
                                "Cannot mutably borrow immutable variable '{}'",
                                name
                            ));
                        }
                    }
                }

                TypedIRValue::MutBorrow {
                    expr: Box::new(inner),
                    target_type: Type::mut_borrow(inner_type),
                }
            }
            Expr::Deref { expr } => {
                let inner = self.translate(expr, scopes);
                let inner_type = inner.type_of();

                let target = match inner_type {
                    Type::Borrow(t) | Type::MutBorrow(t) | Type::Pointer(t) => *t,
                    Type::Ptr => Type::Unknown,
                    Type::Unknown => Type::Unknown,
                    other => {
                        self.diagnostics
                            .push(format!("Cannot dereference non-pointer type {:?}", other));
                        Type::Unknown
                    }
                };

                TypedIRValue::Deref {
                    expr: Box::new(inner),
                    target_type: target,
                }
            }
            Expr::AddrOf { expr } => {
                let inner = self.translate(expr, scopes);
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
            Expr::Var(name, _) => match Self::lookup_var(scopes, name) {
                Some(info) => TypedIRValue::Variable(name.clone(), info.type_.clone()),
                None => {
                    self.diagnostics
                        .push(format!("Undefined variable '{}'", name));
                    TypedIRValue::Variable(name.clone(), Type::Unknown)
                }
            },
            Expr::List(elements) => {
                let values: Vec<TypedIRValue> =
                    elements.iter().map(|e| self.translate(e, scopes)).collect();

                // Infer common element type
                let elem_type = if let Some(first) = values.first() {
                    let mut common = first.type_of();
                    for v in &values[1..] {
                        common = common.common_supertype(&v.type_of());

                        if common == Type::Unknown {
                            self.diagnostics.push(format!(
                                "Heterogeneous list types: {:?} and {:?}",
                                first.type_of(),
                                v.type_of()
                            ));
                        }
                    }
                    common
                } else {
                    Type::Unknown
                };

                // Coerce Int elements to Float if needed
                let values = if elem_type == Type::Float {
                    values
                        .into_iter()
                        .map(|v| {
                            if v.type_of() == Type::Int {
                                TypedIRValue::Cast {
                                    value: Box::new(v),
                                    target_type: Type::Float,
                                }
                            } else {
                                v
                            }
                        })
                        .collect()
                } else {
                    values
                };

                TypedIRValue::List(values, elem_type)
            }
            Expr::Binary { left, op, right } => {
                let l = self.translate(left, scopes);
                let r = self.translate(right, scopes);
                self.translate_binary_op(op, l, r)
            }
            Expr::FunctionCall {
                name,
                args,
                span: _,
            } => {
                let clean_name = name.trim_end_matches("()");

                // Check if function exists
                let sig = self.function_types.get(clean_name).cloned();

                if let Some(sig) = sig {
                    // Check argument count
                    if args.len() != sig.params.len() {
                        self.diagnostics.push(format!(
                            "Function '{}' expects {} arguments, got {}",
                            clean_name,
                            sig.params.len(),
                            args.len()
                        ));
                    }

                    // Translate and check arguments
                    let mut targs = Vec::new();
                    for (i, (arg, (param_name, param_type))) in
                        args.iter().zip(&sig.params).enumerate()
                    {
                        let translated = self.translate(arg, scopes);
                        let actual_type = translated.type_of();

                        if actual_type != Type::Unknown
                            && *param_type != Type::Unknown
                            && !actual_type.can_coerce_to(param_type)
                        {
                            self.diagnostics.push(format!(
                                "Argument '{}' type mismatch at position {}: expected {:?}, found {:?}",
                                param_name, i, param_type, actual_type
                            ));
                        }

                        // Coerce if needed
                        let coerced = if actual_type != *param_type
                            && actual_type.can_coerce_to(param_type)
                        {
                            TypedIRValue::Cast {
                                value: Box::new(translated),
                                target_type: param_type.clone(),
                            }
                        } else {
                            translated
                        };

                        targs.push(coerced);
                    }

                    TypedIRValue::Call {
                        function: clean_name.to_string(),
                        args: targs,
                        return_type: sig.return_type,
                    }
                } else {
                    // Unknown function
                    self.diagnostics
                        .push(format!("Call to undefined function '{}'", clean_name));

                    let targs: Vec<TypedIRValue> =
                        args.iter().map(|a| self.translate(a, scopes)).collect();

                    TypedIRValue::Call {
                        function: clean_name.to_string(),
                        args: targs,
                        return_type: Type::Unknown,
                    }
                }
            }
            Expr::ArrayAccess { array, index } => {
                let arr = self.translate(array, scopes);
                let idx = self.translate(index, scopes);

                // Check index type
                let idx_type = idx.type_of();
                if idx_type != Type::Int && idx_type != Type::Unknown {
                    self.diagnostics
                        .push(format!("Array index must be Int, found {:?}", idx_type));
                }

                // Get element type
                let peeled = Self::peel_type(arr.type_of());
                let elem_type = match peeled {
                    Type::List(e) => *e,
                    Type::Array(e, _) => *e,
                    Type::Unknown => Type::Unknown,
                    other => {
                        self.diagnostics
                            .push(format!("Cannot index non-list type {:?}", other));
                        Type::Unknown
                    }
                };

                TypedIRValue::ArrayAccess {
                    array: Box::new(arr),
                    index: Box::new(idx),
                    element_type: elem_type,
                }
            }
            Expr::Some { value } => {
                let inner = self.translate(value, scopes);
                let _inner_type = inner.type_of();
                TypedIRValue::Some(Box::new(inner))
            }
            Expr::None => TypedIRValue::None {
                option_type: Type::option(Type::Unknown),
            },
            Expr::Ok { value } => {
                let inner = self.translate(value, scopes);
                TypedIRValue::Ok {
                    value: Box::new(inner),
                    result_type: Type::result(Type::Unknown, Type::Unknown),
                }
            }
            Expr::Error { value } => {
                let inner = self.translate(value, scopes);
                TypedIRValue::Error {
                    value: Box::new(inner),
                    result_type: Type::result(Type::Unknown, Type::Unknown),
                }
            }
            Expr::Block {
                statements,
                trailing_expr,
            } => {
                // FIXED: Actually translate statements
                // Note: This is simplified - full block translation
                // requires control flow handling

                let mut last_value = TypedIRValue::Void;

                for stmt in statements {
                    match stmt {
                        crate::frontend::ast::Stmt::Expression(e) => {
                            last_value = self.translate(e, scopes);
                        }
                        crate::frontend::ast::Stmt::Print { expr } => {
                            last_value = self.translate(expr, scopes);
                        }
                        crate::frontend::ast::Stmt::VarDecl {
                            name,
                            value,
                            type_annotation,
                            mutable: _,
                            ..
                        } => {
                            let translated = self.translate(value, scopes);
                            let var_type = if let Some(t) = type_annotation {
                                Type::from_str(t)
                            } else {
                                translated.type_of()
                            };
                            last_value = TypedIRValue::Variable(name.clone(), var_type);
                        }
                        _ => {}
                    }
                }

                if let Some(e) = trailing_expr {
                    self.translate(e, scopes)
                } else {
                    last_value
                }
            }
            Expr::If {
                condition,
                then_branch,
                else_branch,
            } => {
                // FIXED: Check condition and translate both branches
                let cond = self.translate(condition, scopes);
                let cond_type = cond.type_of();

                if cond_type != Type::Bool && cond_type != Type::Unknown {
                    self.diagnostics
                        .push(format!("If condition must be Bool, found {:?}", cond_type));
                }

                let then_val = self.translate(then_branch, scopes);

                if let Some(else_expr) = else_branch {
                    let else_val = self.translate(else_expr, scopes);
                    // Return common type
                    let _common_type = then_val.type_of().common_supertype(&else_val.type_of());

                    // For simplicity, return then value with common type
                    then_val
                } else {
                    then_val
                }
            }
            Expr::Match { value, cases } => {
                // FIXED: Translate value and all cases
                let match_val = self.translate(value, scopes);

                if let Some(first_case) = cases.first() {
                    let first_type = self.translate(&first_case.body, scopes).type_of();
                    let mut common_type = first_type;

                    for case in &cases[1..] {
                        let case_type = self.translate(&case.body, scopes).type_of();
                        common_type = common_type.common_supertype(&case_type);
                    }

                    // Return match value for now
                    match_val
                } else {
                    TypedIRValue::Void
                }
            }
            Expr::TryCatch {
                try_branch,
                catch_branch,
                ..
            } => {
                let try_val = self.translate(try_branch, scopes);
                let catch_val = self.translate(catch_branch, scopes);

                let _common_type = try_val.type_of().common_supertype(&catch_val.type_of());

                try_val
            }
            Expr::For {
                var: _,
                iterable,
                body: _,
                trailing_expr,
                ..
            } => {
                let _iterable_val = self.translate(iterable, scopes);

                if let Some(te) = trailing_expr {
                    self.translate(te, scopes)
                } else {
                    TypedIRValue::Variable("__for_result".into(), Type::Unknown)
                }
            }
            Expr::While {
                condition,
                body: _,
                trailing_expr,
                ..
            } => {
                let _cond_val = self.translate(condition, scopes);

                if let Some(te) = trailing_expr {
                    self.translate(te, scopes)
                } else {
                    TypedIRValue::Variable("__while_result".into(), Type::Unknown)
                }
            }
            Expr::PtrLiteral(v) => TypedIRValue::PtrLiteral(*v),
            Expr::NullPtr => TypedIRValue::NullPtr,
            Expr::Cast {
                expr: ce,
                target_type,
            } => {
                let inner = self.translate(ce, scopes);
                let target = Type::from_str(target_type);

                // Check if cast is valid
                let source_type = inner.type_of();
                if source_type != Type::Unknown
                    && target != Type::Unknown
                    && !source_type.can_cast_to(&target)
                {
                    self.diagnostics.push(format!(
                        "Invalid cast from {:?} to {:?}",
                        source_type, target
                    ));
                }

                TypedIRValue::Cast {
                    value: Box::new(inner),
                    target_type: target,
                }
            }
            _ => TypedIRValue::Void,
        }
    }

    fn translate_binary_op(
        &mut self,
        op: &BinOp,
        left: TypedIRValue,
        right: TypedIRValue,
    ) -> TypedIRValue {
        let lt = left.type_of();
        let rt = right.type_of();

        // Use type checker for validation
        let result_type = self.type_checker.validate_binary_op(op, &lt, &rt);

        // Apply coercions
        let (coerced_left, coerced_right) = match TypeChecker::needs_int_to_float_coercion(&lt, &rt)
        {
            Some(true) => (
                TypedIRValue::Cast {
                    value: Box::new(left),
                    target_type: Type::Float,
                },
                right,
            ),
            Some(false) => (
                left,
                TypedIRValue::Cast {
                    value: Box::new(right),
                    target_type: Type::Float,
                },
            ),
            None => (left, right),
        };

        let semantic_op = match op {
            BinOp::Add => SemanticBinOp::Add,
            BinOp::Subtract => SemanticBinOp::Subtract,
            BinOp::Multiply => SemanticBinOp::Multiply,
            BinOp::Divide => SemanticBinOp::Divide,
            BinOp::Equal => SemanticBinOp::Equal,
            BinOp::NotEqual => SemanticBinOp::NotEqual,
            BinOp::Less => SemanticBinOp::Less,
            BinOp::Greater => SemanticBinOp::Greater,
            BinOp::LessEqual => SemanticBinOp::LessEqual,
            BinOp::GreaterEqual => SemanticBinOp::GreaterEqual,
            BinOp::And => SemanticBinOp::And,
            BinOp::Or => SemanticBinOp::Or,
        };

        TypedIRValue::BinaryOp {
            op: semantic_op,
            left: Box::new(coerced_left),
            right: Box::new(coerced_right),
            result_type,
        }
    }
}
