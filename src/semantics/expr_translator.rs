// ALGOL26 - Expression Translator
// Responsible for translating AST expressions to Semantic IR values
// 100% Orthogonal: For/While are expressions

use crate::frontend::ast::{Expr, BinOp};
use crate::common::types::Type;
use crate::ir::semantic_ir::{TypedIRValue, SemanticBinOp};
use crate::semantics::type_checker::TypeChecker;
use std::collections::HashMap;

#[derive(Debug, Clone)]
pub struct VariableInfo {
    pub type_: Type,
    pub mutable: bool,
}

pub struct ExprTranslator {
    type_checker: TypeChecker,
}

impl ExprTranslator {
    pub fn new() -> Self {
        ExprTranslator {
            type_checker: TypeChecker::new(),
        }
    }
    
    pub fn take_diagnostics(&mut self) -> Vec<String> {
        self.type_checker.take_diagnostics()
    }
    
    pub fn translate(
        &mut self,
        expr: &Expr,
        scopes: &[HashMap<String, VariableInfo>],
    ) -> TypedIRValue {
        match expr {
            Expr::Borrow { expr } => {
                let inner = self.translate(expr, scopes);
                let inner_type = inner.type_of();
                TypedIRValue::Cast {
                    value: Box::new(inner),
                    target_type: Type::borrow(inner_type),
                }
            }
            Expr::MutBorrow { expr } => {
                let inner = self.translate(expr, scopes);
                let inner_type = inner.type_of();
                TypedIRValue::Cast {
                    value: Box::new(inner),
                    target_type: Type::mut_borrow(inner_type),
                }
            }
            Expr::Deref { expr } => {
                let inner = self.translate(expr, scopes);
                let inner_type = inner.type_of();
                match inner_type {
                    Type::Borrow(t) | Type::MutBorrow(t) | Type::Pointer(t) => {
                        TypedIRValue::Cast {
                            value: Box::new(inner),
                            target_type: *t,
                        }
                    }
                    _ => inner,
                }
            }
            Expr::AddrOf { expr } => {
                let inner = self.translate(expr, scopes);
                let inner_type = inner.type_of();
                TypedIRValue::Cast {
                    value: Box::new(inner),
                    target_type: Type::pointer(inner_type),
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
                let values: Vec<TypedIRValue> = elements
                    .iter()
                    .map(|e| self.translate(e, scopes))
                    .collect();
                TypedIRValue::List(values)
            }
            
            Expr::Binary { left, op, right } => {
                let l = self.translate(left, scopes);
                let r = self.translate(right, scopes);
                self.translate_binary_op(op, l, r)
            }
            
            Expr::FunctionCall { name, args, .. } => {
                let typed_args: Vec<TypedIRValue> = args
                    .iter()
                    .map(|a| self.translate(a, scopes))
                    .collect();
                TypedIRValue::Call {
                    function: name.clone(),
                    args: typed_args,
                    return_type: Type::Unknown,
                }
            }
            
            Expr::ArrayAccess { array, index } => {
                let arr = self.translate(array, scopes);
                let idx = self.translate(index, scopes);
                TypedIRValue::ArrayAccess {
                    array: Box::new(arr),
                    index: Box::new(idx),
                    element_type: Type::Float,
                }
            }
            
            Expr::Some { value } => {
                let inner = self.translate(value, scopes);
                TypedIRValue::Some(Box::new(inner))
            }
            
            Expr::None => TypedIRValue::None,
            
            Expr::Ok { value } => {
                let inner = self.translate(value, scopes);
                TypedIRValue::Ok(Box::new(inner))
            }
            
            Expr::Block { trailing_expr, .. } => {
                if let Some(expr) = trailing_expr {
                    self.translate(expr, scopes)
                } else {
                    TypedIRValue::Float(0.0)
                }
            }
            Expr::If { then_branch, .. } => self.translate(then_branch, scopes),
            Expr::Match { cases, .. } => {
                if let Some(first) = cases.first() {
                    self.translate(&first.body, scopes)
                } else {
                    TypedIRValue::Float(0.0)
                }
            }
            Expr::TryCatch { try_branch, .. } => self.translate(try_branch, scopes),
            Expr::Error { value } => {
                let inner = self.translate(value, scopes);
                TypedIRValue::Error(Box::new(inner))
            }
            // --- ORTHOGONAL FIX: Loops as expressions ---
            Expr::For { iterable, trailing_expr, .. } => {
                // Analyze iterable for type checking side-effects
                let _ = self.translate(iterable, scopes);
                // If loop has trailing expr like `for x in xs do x + 1`, return its type
                // Otherwise it's Void / last iteration value - represented as Unknown temp
                if let Some(te) = trailing_expr {
                    self.translate(te, scopes)
                } else {
                    TypedIRValue::Variable("__for_result".to_string(), Type::Unknown)
                }
            }
            Expr::While { condition, trailing_expr, .. } => {
                let _ = self.translate(condition, scopes);
                if let Some(te) = trailing_expr {
                    self.translate(te, scopes)
                } else {
                    TypedIRValue::Variable("__while_result".to_string(), Type::Unknown)
                }
            }
            Expr::PtrLiteral(val) => {
                // Translate as an integer constant for now
                self.translate(&Expr::Int(*val as i64), scopes)
            }

            Expr::NullPtr => {
                // Translate as integer 0 for now
                self.translate(&Expr::Int(0), scopes)
            }

            Expr::Cast { expr: cast_expr, target_type: _ } => {
                // For now, just translate the inner expression
                self.translate(cast_expr, scopes)
            }
        }
    }
    
    fn translate_binary_op(
        &mut self,
        op: &BinOp,
        left: TypedIRValue,
        right: TypedIRValue,
    ) -> TypedIRValue {
        let left_type = left.type_of();
        let right_type = right.type_of();
        
        let result_type = self.type_checker.validate_binary_op(op, &left_type, &right_type);
        
        let (cast_left, cast_right) = match TypeChecker::needs_int_to_float_coercion(&left_type, &right_type) {
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
            left: Box::new(cast_left),
            right: Box::new(cast_right),
            result_type,
        }
    }
}

impl Default for ExprTranslator {
    fn default() -> Self {
        Self::new()
    }
}
