// src/type_checker.rs - Fixed to match architecture tests

use crate::common::types::Type;
use crate::frontend::ast::{BinOp, Expr};

pub struct TypeChecker {
    diagnostics: Vec<String>,
}

impl TypeChecker {
    pub fn new() -> Self {
        TypeChecker {
            diagnostics: Vec::new(),
        }
    }
    
    pub fn take_diagnostics(&mut self) -> Vec<String> {
        std::mem::take(&mut self.diagnostics)
    }
    
    pub fn validate_binary_op(
        &mut self,
        op: &BinOp,
        left_type: &Type,
        right_type: &Type,
    ) -> Type {
        match op {
            BinOp::Add | BinOp::Subtract | BinOp::Multiply | BinOp::Divide => {
                // Check numeric types only
                if left_type.is_numeric() && right_type.is_numeric() {
                    left_type.common_supertype(right_type)
                } else {
                    // Strings and other non-numeric types return Unknown here
                    // (String concatenation is handled in the SemanticAnalyzer)
                    if !left_type.is_unknown() && !right_type.is_unknown() {
                        self.diagnostics.push(format!(
                            "Invalid operands for binary operator {:?}: {} and {}",
                            op, left_type, right_type
                        ));
                    }
                    Type::Unknown
                }
            }
            BinOp::Greater | BinOp::Less | BinOp::GreaterEqual | BinOp::LessEqual => {
                if left_type.is_numeric() && right_type.is_numeric() {
                    Type::Bool
                } else {
                    if !left_type.is_unknown() && !right_type.is_unknown() {
                        self.diagnostics.push(format!(
                            "Invalid operands for comparison {:?}: {} and {}",
                            op, left_type, right_type
                        ));
                    }
                    Type::Bool
                }
            }
            BinOp::Equal | BinOp::NotEqual => {
                if left_type.can_coerce_to(right_type) || right_type.can_coerce_to(left_type) {
                    Type::Bool
                } else {
                    self.diagnostics.push(format!(
                        "Type mismatch for equality comparison: {} and {}",
                        left_type, right_type
                    ));
                    Type::Bool
                }
            }
            BinOp::And | BinOp::Or => {
                if *left_type != Type::Bool && !left_type.is_unknown() {
                    self.diagnostics.push(format!(
                        "Logical operator requires Bool left operand, found {}", left_type
                    ));
                }
                if *right_type != Type::Bool && !right_type.is_unknown() {
                    self.diagnostics.push(format!(
                        "Logical operator requires Bool right operand, found {}", right_type
                    ));
                }
                Type::Bool
            }
        }
    }
    
    pub fn needs_int_to_float_coercion(
        left_type: &Type,
        right_type: &Type,
    ) -> Option<bool> {
        if *left_type == Type::Int && *right_type == Type::Float {
            Some(true)
        } else if *left_type == Type::Float && *right_type == Type::Int {
            Some(false)
        } else {
            None
        }
    }
    
    pub fn infer_list_element_type(&mut self, elements: &[Expr]) -> Type {
        if elements.is_empty() {
            return Type::Unknown;
        }
        
        let mut result = self.infer_expr_type(&elements[0]);
        
        for elem in &elements[1..] {
            let elem_type = self.infer_expr_type(elem);
            result = result.common_supertype(&elem_type);
            
            if result == Type::Unknown {
                self.diagnostics.push(format!(
                    "Heterogeneous list element types: {} and {}",
                    result, elem_type
                ));
            }
        }
        
        result
    }
    
    pub fn infer_expr_type(&self, expr: &Expr) -> Type {
        match expr {
            Expr::Borrow { .. } => Type::Unknown,
            Expr::MutBorrow { .. } => Type::Unknown,
            Expr::Deref { .. } => Type::Unknown,
            Expr::AddrOf { .. } => Type::Unknown,
            Expr::Number(_) => Type::Float,
            Expr::Int(_) => Type::Int,
            Expr::String(_) => Type::String,
            Expr::Bool(_) => Type::Bool,
            // ADD THESE:
            Expr::PtrLiteral(_) => Type::Ptr,
            Expr::NullPtr => Type::Ptr,
            Expr::Cast { expr: _cast_expr, target_type } => {
                // Try to infer from target type string
                Type::from_str(target_type)
            }
            Expr::List(elements) => {
                if let Some(first) = elements.first() {
                    self.infer_expr_type(first)
                } else {
                    Type::Unknown
                }
            }
            Expr::Some { value } => Type::option(self.infer_expr_type(value)),
            Expr::None => Type::option(Type::Unknown),
            Expr::Ok { value } => Type::result(self.infer_expr_type(value), Type::Unknown),
            Expr::Error { value } => Type::result(Type::Unknown, self.infer_expr_type(value)),
            _ => Type::Unknown,
        }
    }
}

impl Default for TypeChecker {
    fn default() -> Self {
        Self::new()
    }
}
