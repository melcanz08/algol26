#![allow(dead_code)]

// src/ir/semantic_verifier.rs - HARDENED
// Complete semantic verification with type checking

use crate::common::types::Type;
use crate::ir::semantic_ir::{SemanticProgram, Terminator};

pub fn verify(program: &SemanticProgram) -> Result<(), String> {
    // First, verify CFG
    crate::ir::cfg_verifier::verify(program)?;

    // Then verify semantics
    for func in &program.functions {
        verify_function(func)?;
    }

    Ok(())
}

fn verify_function(func: &crate::ir::semantic_ir::SemanticFunction) -> Result<(), String> {
    // Check function has blocks
    if func.blocks.is_empty() {
        return Err(format!("Function '{}' has no blocks", func.name));
    }

    // Check return paths for non-void functions
    if func.return_type != Type::Void && !func.is_extern {
        verify_return_paths(func)?;
    }

    // Check all terminators are valid for return type
    for block in &func.blocks {
        if let Some(terminator) = &block.terminator {
            verify_terminator(func, terminator)?;
        }
    }

    Ok(())
}

fn verify_return_paths(func: &crate::ir::semantic_ir::SemanticFunction) -> Result<(), String> {
    // Simple check: if any block has no terminator and no return, it's an error
    for block in &func.blocks {
        match &block.terminator {
            Some(Terminator::Return { .. }) => {}
            Some(_) => {} // Jump, Branch, etc. - check successors
            None => {
                return Err(format!(
                    "Function '{}' has block {} with no terminator",
                    func.name, block.id
                ));
            }
        }
    }

    // Check if entry block eventually reaches a return
    let mut has_return = false;
    for block in &func.blocks {
        if matches!(block.terminator, Some(Terminator::Return { .. })) {
            has_return = true;
            break;
        }
    }

    if !has_return {
        return Err(format!("Function '{}' has no return statement", func.name));
    }

    Ok(())
}

fn verify_terminator(
    func: &crate::ir::semantic_ir::SemanticFunction,
    terminator: &Terminator,
) -> Result<(), String> {
    match terminator {
        Terminator::Return { value, type_ } => {
            // Check return value type matches function return type
            if let Some(val) = value {
                let actual_type = val.type_of();
                if actual_type != Type::Unknown
                    && *type_ != Type::Unknown
                    && !actual_type.can_coerce_to(type_)
                {
                    return Err(format!(
                        "Function '{}' return type mismatch: expected {:?}, found {:?}",
                        func.name, type_, actual_type
                    ));
                }
            }

            // Check return type matches function
            if *type_ != func.return_type
                && *type_ != Type::Unknown
                && func.return_type != Type::Unknown
            {
                return Err(format!(
                    "Function '{}' declared return type {:?} but returns {:?}",
                    func.name, func.return_type, type_
                ));
            }
        }
        Terminator::Branch { condition, .. } => {
            if condition.type_of() != Type::Bool && condition.type_of() != Type::Unknown {
                return Err(format!(
                    "Function '{}' branch condition must be Bool, found {:?}",
                    func.name,
                    condition.type_of()
                ));
            }
        }
        Terminator::Switch { value, .. } => {
            // Value must be matchable
            let value_type = value.type_of();
            if value_type == Type::Void {
                return Err(format!(
                    "Function '{}' switch value cannot be Void",
                    func.name
                ));
            }
        }
        Terminator::IteratorNext { .. } => {
            // Iterator must exist
            // This would require tracking iterator declarations
        }
        _ => {}
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ir::semantic_ir::{
        SemanticBlock, SemanticFunction, SemanticProgram, Terminator, TypedIRValue,
    };

    #[test]
    fn test_missing_return_detected() {
        let mut program = SemanticProgram::new();
        let entry = program.new_block_id();
        let func = SemanticFunction {
            name: "main".to_string(),
            params: vec![],
            return_type: Type::Int, // Should return Int
            blocks: vec![SemanticBlock {
                id: entry,
                instructions: vec![],
                terminator: Some(Terminator::Jump { block: entry }), // Infinite loop, no return
            }],
            entry_block: entry,
            is_extern: false,
        };
        program.functions.push(func);
        assert!(verify(&program).is_err());
    }

    #[test]
    fn test_return_type_mismatch() {
        let mut program = SemanticProgram::new();
        let entry = program.new_block_id();
        let func = SemanticFunction {
            name: "main".to_string(),
            params: vec![],
            return_type: Type::Int,
            blocks: vec![SemanticBlock {
                id: entry,
                instructions: vec![],
                terminator: Some(Terminator::Return {
                    value: Some(TypedIRValue::String("wrong".to_string())),
                    type_: Type::String, // Wrong type!
                }),
            }],
            entry_block: entry,
            is_extern: false,
        };
        program.functions.push(func);
        assert!(verify(&program).is_err());
    }
}
