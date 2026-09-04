// SemanticVerifier — Comprehensive IR verification

use crate::ir::semantic_ir::{SemanticProgram, SemanticInstruction, TypedIRValue};
use crate::common::types::Type;

pub struct SemanticVerifier;

impl SemanticVerifier {
    /// Recursively check for Type::Unknown
    pub fn verify_value_no_unknown(value: &TypedIRValue, context: &str) -> Result<(), String> {
        if value.type_of() == Type::Unknown {
            return Err(format!("{}: Type::Unknown found", context));
        }
        
        match value {
            TypedIRValue::List(elements) => {
                for (i, elem) in elements.iter().enumerate() {
                    Self::verify_value_no_unknown(elem, &format!("{}[{}]", context, i))?;
                }
            }
            TypedIRValue::Some(v) => Self::verify_value_no_unknown(v, &format!("{}.inner", context))?,
            TypedIRValue::Ok(v) => Self::verify_value_no_unknown(v, &format!("{}.ok", context))?,
            TypedIRValue::Error(v) => Self::verify_value_no_unknown(v, &format!("{}.error", context))?,
            TypedIRValue::Cast { value, target_type } => {
                if *target_type == Type::Unknown {
                    return Err(format!("{}: Cast to Unknown", context));
                }
                Self::verify_value_no_unknown(value, &format!("{}.cast", context))?;
            }
            TypedIRValue::BinaryOp { left, right, result_type, .. } => {
                if *result_type == Type::Unknown {
                    return Err(format!("{}: BinaryOp result Unknown", context));
                }
                Self::verify_value_no_unknown(left, &format!("{}.left", context))?;
                Self::verify_value_no_unknown(right, &format!("{}.right", context))?;
            }
            TypedIRValue::Call { args, return_type, .. } => {
                if *return_type == Type::Unknown {
                    return Err(format!("{}: Call return Unknown", context));
                }
                for (i, arg) in args.iter().enumerate() {
                    Self::verify_value_no_unknown(arg, &format!("{}.arg{}", context, i))?;
                }
            }
            _ => {}
        }
        Ok(())
    }

    /// Verify all semantic invariants
    pub fn verify(program: &SemanticProgram) -> Result<(), String> {
        program.verify()?;
        
        for func in &program.functions {
            for block in &func.blocks {
                for instr in &block.instructions {
                    match instr {
                        SemanticInstruction::Branch { condition, .. } => {
                            if condition.type_of() != Type::Bool {
                                return Err(format!("{}:{}: Branch condition must be Bool", func.name, block.id));
                            }
                        }
                        SemanticInstruction::Return { value, type_ } => {
                            if *type_ == Type::Unknown {
                                return Err(format!("{}:{}: Return type Unknown", func.name, block.id));
                            }
                            if let Some(v) = value {
                                if v.type_of() == Type::Unknown {
                                    return Err(format!("{}:{}: Return value Unknown", func.name, block.id));
                                }
                            }
                        }
                        SemanticInstruction::Assign { target: _, value } => {
                            Self::verify_value_no_unknown(value, "assign.value")?;
                        }
                        SemanticInstruction::ArrayAssign { array, index, value } => {
                            Self::verify_value_no_unknown(array, "array_assign.array")?;
                            Self::verify_value_no_unknown(index, "array_assign.index")?;
                            Self::verify_value_no_unknown(value, "array_assign.value")?;
                        }
                        SemanticInstruction::Print { value } => {
                            Self::verify_value_no_unknown(value, "print.value")?;
                        }
                        SemanticInstruction::Declare { value, type_, .. } => {
                            if *type_ == Type::Unknown {
                                return Err(format!("{}:{}: Declare type Unknown", func.name, block.id));
                            }
                            Self::verify_value_no_unknown(value, "declare.value")?;
                        }
                        SemanticInstruction::Call { args, return_type, .. } => {
                            if *return_type == Type::Unknown {
                                return Err(format!("{}:{}: Call return Unknown", func.name, block.id));
                            }
                            for (i, arg) in args.iter().enumerate() {
                                Self::verify_value_no_unknown(arg, &format!("call.arg{}", i))?;
                            }
                        }
                        SemanticInstruction::MethodCall { receiver, args, return_type, .. } => {
                            if *return_type == Type::Unknown {
                                return Err(format!("{}:{}: MethodCall return Unknown", func.name, block.id));
                            }
                            Self::verify_value_no_unknown(receiver, "method_call.receiver")?;
                            for (i, arg) in args.iter().enumerate() {
                                Self::verify_value_no_unknown(arg, &format!("method_call.arg{}", i))?;
                            }
                        }
                        _ => {}
                    }
                }
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_verify_value_rejects_unknown() {
        let value = TypedIRValue::Variable("x".to_string(), Type::Unknown);
        let result = SemanticVerifier::verify_value_no_unknown(&value, "test");
        assert!(result.is_err());
    }
    
    #[test]
    fn test_verify_value_accepts_int() {
        let value = TypedIRValue::Int(42);
        let result = SemanticVerifier::verify_value_no_unknown(&value, "test");
        assert!(result.is_ok());
    }
    
    #[test]
    fn test_verify_value_rejects_nested_unknown() {
        let value = TypedIRValue::List(vec![
            TypedIRValue::Int(1),
            TypedIRValue::Variable("unknown".to_string(), Type::Unknown),
        ]);
        let result = SemanticVerifier::verify_value_no_unknown(&value, "test");
        assert!(result.is_err());
    }
}
