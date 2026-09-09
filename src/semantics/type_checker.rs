// src/semantics/type_checker.rs - Fixed to match architecture tests

use crate::common::types::Type;
use crate::frontend::ast::{BinOp, Expr};

/// DEPRECATED: TypeChecker is superseded by SemanticIRBuilder::validate_binary_op
/// Kept for test compatibility only. Do not use in new code paths.
pub struct TypeChecker {
    diagnostics: Vec<String>,
    type_variables: std::collections::HashMap<String, Type>,
}

impl TypeChecker {
    pub fn new() -> Self {
        TypeChecker {
            diagnostics: Vec::new(),
            type_variables: std::collections::HashMap::new(),
        }
    }

    pub fn take_diagnostics(&mut self) -> Vec<String> {
        std::mem::take(&mut self.diagnostics)
    }

    pub fn validate_binary_op(&mut self, op: &BinOp, left_type: &Type, right_type: &Type) -> Type {
        match op {
            BinOp::Add => {
                // Numeric addition
                if left_type.is_numeric() && right_type.is_numeric() {
                    return left_type.common_supertype(right_type);
                }

                // String concatenation
                if *left_type == Type::String && *right_type == Type::String {
                    return Type::String;
                }

                // List concatenation
                if let (Type::List(inner1), Type::List(inner2)) = (left_type, right_type) {
                    if inner1.can_coerce_to(inner2) || inner2.can_coerce_to(inner1) {
                        return Type::list(inner1.common_supertype(inner2));
                    }
                }

                if !left_type.is_unknown() && !right_type.is_unknown() {
                    self.diagnostics.push(format!(
                        "Invalid operands for addition: {} and {}",
                        left_type, right_type
                    ));
                }
                Type::Unknown
            }
            BinOp::Subtract | BinOp::Multiply | BinOp::Divide => {
                if left_type.is_numeric() && right_type.is_numeric() {
                    left_type.common_supertype(right_type)
                } else {
                    if !left_type.is_unknown() && !right_type.is_unknown() {
                        self.diagnostics.push(format!(
                            "Invalid operands for arithmetic operation {:?}: {} and {}",
                            op, left_type, right_type
                        ));
                    }
                    Type::Unknown
                }
            }
            BinOp::Greater | BinOp::Less | BinOp::GreaterEqual | BinOp::LessEqual => {
                if left_type.is_numeric() && right_type.is_numeric() {
                    Type::Bool
                } else if *left_type == Type::String && *right_type == Type::String {
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
                } else if left_type.is_unknown() || right_type.is_unknown() {
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
                        "Logical operator requires Bool left operand, found {}",
                        left_type
                    ));
                }
                if *right_type != Type::Bool && !right_type.is_unknown() {
                    self.diagnostics.push(format!(
                        "Logical operator requires Bool right operand, found {}",
                        right_type
                    ));
                }
                Type::Bool
            }
        }
    }

    pub fn needs_int_to_float_coercion(left_type: &Type, right_type: &Type) -> Option<bool> {
        if *left_type == Type::Int && *right_type == Type::Float {
            Some(true) // Coerce left to right
        } else if *left_type == Type::Float && *right_type == Type::Int {
            Some(false) // Coerce right to left
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
            let next_result = result.common_supertype(&elem_type);

            if next_result == Type::Unknown {
                self.diagnostics.push(format!(
                    "Heterogeneous list element types: {} and {}",
                    result,
                    elem_type
                ));
            }
            result = next_result;
        }

        result
    }

    pub fn infer_expr_type(&mut self, expr: &Expr) -> Type {
        self.infer_expr_type_with_context(expr, None)
    }
    
    pub fn infer_expr_type_with_context(&mut self, expr: &Expr, expected: Option<&Type>) -> Type {
        match expr {
            Expr::Borrow { expr: inner, .. } => {
                let inner_type = self.infer_expr_type(inner);
                Type::borrow(inner_type)
            }
            Expr::MutBorrow { expr: inner, .. } => {
                let inner_type = self.infer_expr_type(inner);
                Type::mut_borrow(inner_type)
            }
            Expr::Deref { expr: inner, .. } => {
                let inner_type = self.infer_expr_type(inner);
                match inner_type {
                    Type::Borrow(inner) | Type::MutBorrow(inner) => (*inner).clone(),
                    Type::Ptr => Type::Unknown,
                    _ => Type::Unknown,
                }
            }
            Expr::AddrOf { expr: inner, .. } => {
                let inner_type = self.infer_expr_type(inner);
                Type::pointer(inner_type)
            }
            Expr::Number(_) => Type::Float,
            Expr::Int(_) => Type::Int,
            Expr::String(_) => Type::String,
            Expr::Bool(_) => Type::Bool,
            Expr::PtrLiteral(_) => Type::Ptr,
            Expr::NullPtr => Type::Ptr,
            Expr::Cast {
                expr: cast_expr,
                target_type,
            } => {
                let source_type = self.infer_expr_type(cast_expr);
                let target = Type::from_str(target_type);

                // Check if cast is valid
                if source_type != Type::Unknown && target != Type::Unknown {
                    if !source_type.can_cast_to(&target) {
                        // This would need access to diagnostics
                        // self.diagnostics.push(...)
                    }
                }

                target
            }
            Expr::List(elements) => {
                self.infer_list_element_type(elements)
            }
            Expr::Some { value } => {
                let inner_type = self.infer_expr_type(value);
                Type::option(inner_type)
            }
            Expr::None => {
                if let Some(Type::Option(inner)) = expected {
                    Type::option((**inner).clone())
                } else {
                    Type::option(Type::Unknown)
                }
            }
            Expr::Ok { value } => {
                let inner = self.infer_expr_type_with_context(
                    value,
                    expected.and_then(|t| match t {
                        Type::Result { ok, .. } => Some(ok.as_ref()),
                        _ => None,
                    }),
                );
                if let Some(Type::Result { error, .. }) = expected {
                    Type::result(inner, (**error).clone())
                } else {
                    Type::result(inner, Type::Unknown)
                }
            }
            Expr::Error { value } => {
                let inner = self.infer_expr_type_with_context(
                    value,
                    expected.and_then(|t| match t {
                        Type::Result { error, .. } => Some(error.as_ref()),
                        _ => None,
                    }),
                );
                if let Some(Type::Result { ok, .. }) = expected {
                    Type::result((**ok).clone(), inner)
                } else {
                    Type::result(Type::Unknown, inner)
                }
            }
            Expr::Var(name, _) => {
                // Look up variable type from type variables
                self.type_variables
                    .get(name)
                    .cloned()
                    .unwrap_or(Type::Unknown)
            }
            Expr::Binary { op, left, right } => {
                let left_type = self.infer_expr_type(left);
                let right_type = self.infer_expr_type(right);
                self.validate_binary_op(op, &left_type, &right_type)
            }
            Expr::Unary { op, expr, .. } => {
                let inner_type = self.infer_expr_type(expr);
                match op {
                    crate::frontend::ast::UnaryOp::Negate => {
                        if inner_type.is_numeric() {
                            inner_type
                        } else {
                            Type::Unknown
                        }
                    }
                    crate::frontend::ast::UnaryOp::Not => {
                        if inner_type == Type::Bool {
                            Type::Bool
                        } else {
                            Type::Unknown
                        }
                    }
                }
            }
            Expr::FunctionCall {
                name: _, args: _, ..
            } => {
                // This would need function signature lookup
                // For now, return Unknown
                Type::Unknown
            }
            Expr::ArrayAccess {
                array: collection, ..
            } => {
                let collection_type = self.infer_expr_type(collection);
                match collection_type {
                    Type::List(inner) => (*inner).clone(),
                    Type::Array(inner, _) => (*inner).clone(),
                    _ => Type::Unknown,
                }
            }
            _ => Type::Unknown,
        }
    }

    pub fn set_variable_type(&mut self, name: &str, ty: Type) {
        self.type_variables.insert(name.to_string(), ty);
    }

    pub fn get_variable_type(&self, name: &str) -> Option<&Type> {
        self.type_variables.get(name)
    }
}

impl Default for TypeChecker {
    fn default() -> Self {
        Self::new()
    }
}
