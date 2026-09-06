// ALGOL26 - Expression Translator - Single Authority Version
use crate::common::types::Type;
use crate::frontend::ast::{BinOp, Expr};
use crate::ir::semantic_ir::{SemanticBinOp, TypedIRValue};
use crate::semantics::semantic_builder::{FunctionSignature, VariableInfo};
use crate::semantics::type_checker::TypeChecker;
use std::collections::HashMap;

pub struct ExprTranslator {
    type_checker: TypeChecker,
    function_types: HashMap<String, FunctionSignature>,
}

impl ExprTranslator {
    pub fn new() -> Self {
        Self {
            type_checker: TypeChecker::new(),
            function_types: HashMap::new(),
        }
    }
}

impl Default for ExprTranslator {
    fn default() -> Self {
        Self::new()
    }
}

impl ExprTranslator {
    pub fn with_function_types(ft: HashMap<String, FunctionSignature>) -> Self {
        Self {
            type_checker: TypeChecker::new(),
            function_types: ft,
        }
    }
    pub fn set_function_types(&mut self, ft: HashMap<String, FunctionSignature>) {
        self.function_types = ft;
    }
    pub fn take_diagnostics(&mut self) -> Vec<String> {
        self.type_checker.take_diagnostics()
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

    pub fn translate(
        &mut self,
        expr: &Expr,
        scopes: &[HashMap<String, VariableInfo>],
    ) -> TypedIRValue {
        match expr {
            Expr::Unary { op, expr, .. } => {
                let inner = self.translate(expr, scopes);
                let it = inner.type_of();
                match op {
                    crate::frontend::ast::UnaryOp::Negate => TypedIRValue::BinaryOp {
                        op: SemanticBinOp::Subtract,
                        left: Box::new(TypedIRValue::Int(0)),
                        right: Box::new(inner),
                        result_type: it,
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
                let inner = self.translate(expr, scopes);
                let t = inner.type_of();
                TypedIRValue::Borrow {
                    expr: Box::new(inner),
                    target_type: Type::borrow(t),
                }
            }
            Expr::MutBorrow { expr } => {
                let inner = self.translate(expr, scopes);
                let t = inner.type_of();
                TypedIRValue::MutBorrow {
                    expr: Box::new(inner),
                    target_type: Type::mut_borrow(t),
                }
            }
            Expr::Deref { expr } => {
                let inner = self.translate(expr, scopes);
                let it = inner.type_of();
                let target = match it {
                    Type::Borrow(t) | Type::MutBorrow(t) | Type::Pointer(t) => *t,
                    _ => Type::Unknown,
                };
                TypedIRValue::Deref {
                    expr: Box::new(inner),
                    target_type: target,
                }
            }
            Expr::AddrOf { expr } => {
                let inner = self.translate(expr, scopes);
                let t = inner.type_of();
                TypedIRValue::AddrOf {
                    expr: Box::new(inner),
                    target_type: Type::pointer(t),
                }
            }
            Expr::Number(n) => TypedIRValue::Float(*n),
            Expr::Int(i) => TypedIRValue::Int(*i),
            Expr::String(s) => TypedIRValue::String(s.clone()),
            Expr::Bool(b) => TypedIRValue::Bool(*b),
            Expr::Var(name, _) => {
                for scope in scopes.iter().rev() {
                    if let Some(info) = scope.get(name) {
                        return TypedIRValue::Variable(name.clone(), info.type_.clone());
                    }
                }
                TypedIRValue::Variable(name.clone(), Type::Unknown)
            }
            Expr::List(elements) => {
                let values: Vec<TypedIRValue> =
                    elements.iter().map(|e| self.translate(e, scopes)).collect();
                let elem = values.first().map(|v| v.type_of()).unwrap_or(Type::Unknown);
                TypedIRValue::List(values, elem)
            }
            Expr::Binary { left, op, right } => {
                let l = self.translate(left, scopes);
                let r = self.translate(right, scopes);
                self.translate_binary_op(op, l, r)
            }
            Expr::FunctionCall { name, args, .. } => {
                let targs: Vec<TypedIRValue> =
                    args.iter().map(|a| self.translate(a, scopes)).collect();
                let ret = self
                    .function_types
                    .get(name)
                    .map(|s| s.return_type.clone())
                    .unwrap_or(Type::Unknown);
                TypedIRValue::Call {
                    function: name.clone(),
                    args: targs,
                    return_type: ret,
                }
            }
            Expr::ArrayAccess { array, index } => {
                let arr = self.translate(array, scopes);
                let idx = self.translate(index, scopes);
                let peeled = Self::peel_type(arr.type_of());
                let elem_type = match peeled {
                    Type::List(e) => *e,
                    Type::Unknown => Type::Unknown,
                    _ => Type::Unknown,
                };
                TypedIRValue::ArrayAccess {
                    array: Box::new(arr),
                    index: Box::new(idx),
                    element_type: elem_type,
                }
            }
            Expr::Some { value } => TypedIRValue::Some(Box::new(self.translate(value, scopes))),
            Expr::None => TypedIRValue::None {
                option_type: Type::option(Type::Void),
            },
            Expr::Ok { value } => TypedIRValue::Ok {
                value: Box::new(self.translate(value, scopes)),
                result_type: Type::result(Type::Void, Type::Void),
            },
            Expr::Block { trailing_expr, .. } => {
                if let Some(e) = trailing_expr {
                    self.translate(e, scopes)
                } else {
                    TypedIRValue::Void
                }
            }
            Expr::If { then_branch, .. } => self.translate(then_branch, scopes),
            Expr::Match { cases, .. } => {
                if let Some(f) = cases.first() {
                    self.translate(&f.body, scopes)
                } else {
                    TypedIRValue::Void
                }
            }
            Expr::TryCatch { try_branch, .. } => self.translate(try_branch, scopes),
            Expr::Error { value } => TypedIRValue::Error {
                value: Box::new(self.translate(value, scopes)),
                result_type: Type::result(Type::Void, Type::Void),
            },
            Expr::For {
                iterable,
                trailing_expr,
                ..
            } => {
                let _ = self.translate(iterable, scopes);
                if let Some(te) = trailing_expr {
                    self.translate(te, scopes)
                } else {
                    TypedIRValue::Variable("__for_result".into(), Type::Unknown)
                }
            }
            Expr::While {
                condition,
                trailing_expr,
                ..
            } => {
                let _ = self.translate(condition, scopes);
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
                TypedIRValue::Cast {
                    value: Box::new(inner),
                    target_type: Type::from_str(target_type),
                }
            }
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
        let res = self.type_checker.validate_binary_op(op, &lt, &rt);
        let (cl, cr) = match TypeChecker::needs_int_to_float_coercion(&lt, &rt) {
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
        let bop = match op {
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
            op: bop,
            left: Box::new(cl),
            right: Box::new(cr),
            result_type: res,
        }
    }
}
