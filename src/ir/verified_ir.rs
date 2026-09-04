// ALGOL26 VerifiedIR — Type-safe verified IR wrapper
// Makes it impossible to pass unverified IR to a backend

use crate::ir::semantic_ir::SemanticProgram;

/// VerifiedIR wraps a SemanticProgram that has passed verification.
/// The ONLY way to create a VerifiedIR is through `VerifiedIR::new()`.
#[derive(Debug, Clone)]
pub struct VerifiedIR {
    /// The verified program
    program: SemanticProgram,
}

impl VerifiedIR {
    /// Create a VerifiedIR by verifying the given program.
    /// Returns Err if verification fails.
    pub fn new(program: SemanticProgram) -> Result<Self, String> {
        // Verify the program
        program.verify()?;
        
        Ok(VerifiedIR { program })
    }
    
    /// Get a reference to the verified program
    pub fn program(&self) -> &SemanticProgram {
        &self.program
    }
    
    /// Get the verified program (consumes the wrapper)
    pub fn into_program(self) -> SemanticProgram {
        self.program
    }
}

/// Backend input type — VerifiedIR is the only accepted input
pub type BackendInput = VerifiedIR;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ir::semantic_ir::{SemanticProgram, SemanticFunction, SemanticBlock, SemanticInstruction};
    use crate::common::types::Type;
    
    #[test]
    fn test_verified_ir_accepts_valid_program() {
        let mut program = SemanticProgram::new();
        let entry = program.new_block_id();
        let func = SemanticFunction {
            name: "main".to_string(),
            params: vec![],
            return_type: Type::Void,
            blocks: vec![SemanticBlock {
                id: entry,
                instructions: vec![SemanticInstruction::Return { value: None, type_: Type::Void }],
            }],
            entry_block: entry,
            is_extern: false,
        };
        program.functions.push(func);
        
        let verified = VerifiedIR::new(program);
        assert!(verified.is_ok());
    }
    
    #[test]
    fn test_verified_ir_rejects_invalid_program() {
        let mut program = SemanticProgram::new();
        let block_id = program.new_block_id();
        let func = SemanticFunction {
            name: "main".to_string(),
            params: vec![],
            return_type: Type::Void,
            blocks: vec![
                SemanticBlock { id: block_id, instructions: vec![] },
                SemanticBlock { id: block_id, instructions: vec![] }, // Duplicate!
            ],
            entry_block: block_id,
            is_extern: false,
        };
        program.functions.push(func);
        
        let verified = VerifiedIR::new(program);
        assert!(verified.is_err());
    }
    
    #[test]
    fn test_verified_ir_into_program() {
        let mut program = SemanticProgram::new();
        let entry = program.new_block_id();
        let func = SemanticFunction {
            name: "main".to_string(),
            params: vec![],
            return_type: Type::Void,
            blocks: vec![SemanticBlock {
                id: entry,
                instructions: vec![],
            }],
            entry_block: entry,
            is_extern: false,
        };
        program.functions.push(func);
        
        let verified = VerifiedIR::new(program).unwrap();
        let extracted = verified.into_program();
        assert_eq!(extracted.functions.len(), 1);
    }
}
