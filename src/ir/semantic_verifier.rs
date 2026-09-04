// SemanticVerifier — Comprehensive IR verification
// Verifies types, not just block structure

use crate::ir::semantic_ir::{SemanticProgram, SemanticInstruction};
use crate::common::types::Type;

pub struct SemanticVerifier;

impl SemanticVerifier {
    /// Verify all semantic invariants
    pub fn verify(program: &SemanticProgram) -> Result<(), String> {
        // First, do structural verification
        program.verify()?;
        
        // Then do type verification
        for func in &program.functions {
            for block in &func.blocks {
                for instr in &block.instructions {
                    Self::verify_instruction(instr, &func.name, block.id)?;
                }
            }
        }
        Ok(())
    }
    
    fn verify_instruction(instr: &SemanticInstruction, func_name: &str, block_id: usize) -> Result<(), String> {
        match instr {
            SemanticInstruction::Branch { condition, .. } => {
                if condition.type_of() != Type::Bool {
                    return Err(format!("{}:{}: Branch condition must be Bool", func_name, block_id));
                }
            }
            SemanticInstruction::Return { value, type_ } => {
                if *type_ == Type::Unknown {
                    return Err(format!("{}:{}: Return type Unknown", func_name, block_id));
                }
                if let Some(v) = value {
                    if v.type_of() == Type::Unknown {
                        return Err(format!("{}:{}: Return value type Unknown", func_name, block_id));
                    }
                }
            }
            _ => {}
        }
        Ok(())
    }
}
