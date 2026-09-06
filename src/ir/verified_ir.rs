
use crate::ir::semantic_ir::SemanticProgram;
#[derive(Debug, Clone)]
pub struct VerifiedIR { program: SemanticProgram }
impl VerifiedIR {
    pub fn new(program: SemanticProgram) -> Result<Self, String> {
        program.verify()?;
        Ok(VerifiedIR { program })
    }
    pub fn program(&self) -> &SemanticProgram { &self.program }
}
pub type BackendInput = VerifiedIR;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::common::types::Type;
    use crate::ir::semantic_ir::{SemanticBlock, SemanticFunction, Terminator};
    #[test]
    fn test_verified_ir_accepts_valid_program() {
        let mut program = SemanticProgram::new();
        let entry = program.new_block_id();
        let func = SemanticFunction {
            name: "main".to_string(),
            params: vec![],
            return_type: Type::Void,
            blocks: vec![SemanticBlock { id: entry, instructions: vec![], terminator: Some(Terminator::Return { value: None, type_: Type::Void }) }],
            entry_block: entry,
            is_extern: false,
        };
        program.functions.push(func);
        assert!(VerifiedIR::new(program).is_ok());
    }
    #[test]
    fn test_verified_ir_rejects_invalid_program() {
        let mut program = SemanticProgram::new();
        let entry = program.new_block_id();
        let func = SemanticFunction {
            name: "main".to_string(),
            params: vec![],
            return_type: Type::Void,
            blocks: vec![SemanticBlock { id: entry, instructions: vec![], terminator: None }],
            entry_block: entry,
            is_extern: false,
        };
        program.functions.push(func);
        assert!(VerifiedIR::new(program).is_err());
    }
}
