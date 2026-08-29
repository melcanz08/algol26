// ALGOL26 - Type Checker
// Responsible for type validation and coercion

use crate::semantic_type::SemanticType;
use crate::ast::{BinOp, Expr};

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
        left_type: &SemanticType,
        right_type: &SemanticType,
    ) -> SemanticType {
        match op {
            BinOp::Add | BinOp::Subtract | BinOp::Multiply | BinOp::Divide => {
                if *left_type == SemanticType::Int && *right_type == SemanticType::Int {
                    SemanticType::Int
                } else if *left_type == SemanticType::Float && *right_type == SemanticType::Float {
                    SemanticType::Float
                } else if *left_type == SemanticType::Int && *right_type == SemanticType::Float {
                    SemanticType::Float
                } else if *left_type == SemanticType::Float && *right_type == SemanticType::Int {
                    SemanticType::Float
                } else {
                    if *left_type != SemanticType::Unknown && *right_type != SemanticType::Unknown {
                        self.diagnostics.push(format!(
                            "Invalid operands for binary operator {:?}: {:?} and {:?}",
                            op, left_type, right_type
                        ));
                    }
                    SemanticType::Unknown
                }
            }
            BinOp::Greater | BinOp::Less | BinOp::GreaterEqual | BinOp::LessEqual => {
                if (*left_type == SemanticType::Int || *left_type == SemanticType::Float)
                    && (*right_type == SemanticType::Int || *right_type == SemanticType::Float) {
                    SemanticType::Bool
                } else {
                    if *left_type != SemanticType::Unknown && *right_type != SemanticType::Unknown {
                        self.diagnostics.push(format!(
                            "Invalid operands for comparison {:?}: {:?} and {:?}",
                            op, left_type, right_type
                        ));
                    }
                    SemanticType::Bool
                }
            }
            BinOp::Equal | BinOp::NotEqual => {
                if left_type.can_coerce_to(right_type) && right_type.can_coerce_to(left_type) {
                    SemanticType::Bool
                } else {
                    self.diagnostics.push(format!(
                        "Type mismatch for equality comparison: {:?} and {:?}",
                        left_type, right_type
                    ));
                    SemanticType::Bool
                }
            }
            BinOp::And | BinOp::Or => {
                if *left_type != SemanticType::Bool && *left_type != SemanticType::Unknown {
                    self.diagnostics.push(format!(
                        "Logical operator requires Bool left operand, found {:?}", left_type
                    ));
                }
                if *right_type != SemanticType::Bool && *right_type != SemanticType::Unknown {
                    self.diagnostics.push(format!(
                        "Logical operator requires Bool right operand, found {:?}", right_type
                    ));
                }
                SemanticType::Bool
            }
        }
    }
    
    pub fn needs_int_to_float_coercion(
        left_type: &SemanticType,
        right_type: &SemanticType,
    ) -> Option<bool> {
        // Returns Some(true) if left needs coercion, Some(false) if right needs it
        if *left_type == SemanticType::Int && *right_type == SemanticType::Float {
            Some(true)
        } else if *left_type == SemanticType::Float && *right_type == SemanticType::Int {
            Some(false)
        } else {
            None
        }
    }
    
    pub fn infer_list_element_type(&mut self, elements: &[Expr]) -> SemanticType {
        let mut result = SemanticType::Unknown;
        
        for elem in elements {
            let elem_type = self.infer_expr_type(elem);
            if result == SemanticType::Unknown {
                result = elem_type;
            } else if result == SemanticType::Float && elem_type == SemanticType::Int {
                // Keep Float
            } else if result == SemanticType::Int && elem_type == SemanticType::Float {
                result = SemanticType::Float;
            } else if result != elem_type && elem_type != SemanticType::Unknown {
                self.diagnostics.push(format!(
                    "Heterogeneous list element types: {:?} and {:?}",
                    result, elem_type
                ));
            }
        }
        
        if result == SemanticType::Unknown {
            result = SemanticType::Float;
        }
        
        result
    }
    
    pub fn infer_expr_type(&self, expr: &Expr) -> SemanticType {
        match expr {
            Expr::Borrow { .. } => SemanticType::Unknown,
            Expr::MutBorrow { .. } => SemanticType::Unknown,
            Expr::Number(_) => SemanticType::Float,
            Expr::Int(_) => SemanticType::Int,
            Expr::String(_) => SemanticType::String,
            Expr::Bool(_) => SemanticType::Bool,
            Expr::List(elements) => {
                if let Some(first) = elements.first() {
                    self.infer_expr_type(first)
                } else {
                    SemanticType::Float
                }
            }
            Expr::Some { value } => self.infer_expr_type(value),
            Expr::None => SemanticType::Void,
            Expr::Ok { value } => self.infer_expr_type(value),
            Expr::Error { value } => self.infer_expr_type(value),
            _ => SemanticType::Unknown,
        }
    }
}

impl Default for TypeChecker {
    fn default() -> Self {
        Self::new()
    }
}
