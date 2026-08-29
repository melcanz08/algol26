// ALGOL26 - Expression Translator
// Responsible for translating AST expressions to Semantic IR values

use crate::ast::{Expr, BinOp};
use crate::semantic_type::SemanticType;
use crate::semantic_ir::{TypedIRValue, SemanticBinOp};
use crate::type_checker::TypeChecker;
use std::collections::HashMap;

#[derive(Debug, Clone)]
pub struct VariableInfo {
    pub type_: SemanticType,
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
            Expr::Number(n) => TypedIRValue::Float(*n),
            Expr::Int(i) => TypedIRValue::Int(*i),
            Expr::String(s) => TypedIRValue::String(s.clone()),
            Expr::Bool(b) => TypedIRValue::Bool(*b),
            
            Expr::Var(name) => {
                for scope in scopes.iter().rev() {
                    if let Some(info) = scope.get(name) {
                        return TypedIRValue::Variable(name.clone(), info.type_.clone());
                    }
                }
                TypedIRValue::Variable(name.clone(), SemanticType::Unknown)
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
            
            Expr::FunctionCall { name, args } => {
                let typed_args: Vec<TypedIRValue> = args
                    .iter()
                    .map(|a| self.translate(a, scopes))
                    .collect();
                TypedIRValue::Call {
                    function: name.clone(),
                    args: typed_args,
                    return_type: SemanticType::Unknown,
                }
            }
            
            Expr::ArrayAccess { array, index } => {
                let arr = self.translate(array, scopes);
                let idx = self.translate(index, scopes);
                TypedIRValue::ArrayAccess {
                    array: Box::new(arr),
                    index: Box::new(idx),
                    element_type: SemanticType::Float,
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
            
            Expr::Error { value } => {
                let inner = self.translate(value, scopes);
                TypedIRValue::Error(Box::new(inner))
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
                    target_type: SemanticType::Float,
                },
                right,
            ),
            Some(false) => (
                left,
                TypedIRValue::Cast {
                    value: Box::new(right),
                    target_type: SemanticType::Float,
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
