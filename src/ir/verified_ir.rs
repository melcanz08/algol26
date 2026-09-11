// src/ir/verified_ir.rs
//
// `VerifiedIR::new` runs semantic_verifier::verify before returning.
// The type is a compile-time promise: a `VerifiedIR` value can only
// exist if verification succeeded. `BackendInput` is an alias for
// this type, so backend entry points that take `&VerifiedIR` are
// statically guaranteed to receive verified input.
//
// Note that "verified" here means "passed semantic_verifier", whose
// coverage is documented in that module.

use crate::ir::semantic_ir::SemanticProgram;
use std::fmt;

#[derive(Debug, Clone)]
pub struct VerifiedIR {
    program: SemanticProgram,
}

impl VerifiedIR {
    pub fn new(program: SemanticProgram) -> Result<Self, String> {
        // Run full verification
        crate::ir::semantic_verifier::verify(&program)?;
        Ok(VerifiedIR { program })
    }

    pub fn program(&self) -> &SemanticProgram {
        &self.program
    }

    pub fn into_program(self) -> SemanticProgram {
        self.program
    }

    pub fn verify(&self) -> Result<(), String> {
        crate::ir::semantic_verifier::verify(&self.program)
    }

    pub fn function_count(&self) -> usize {
        self.program.functions.len()
    }

    pub fn block_count(&self) -> usize {
        self.program.functions.iter().map(|f| f.blocks.len()).sum()
    }
}

impl fmt::Display for VerifiedIR {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "VerifiedIR(functions: {}, blocks: {})",
            self.function_count(),
            self.block_count()
        )
    }
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
            blocks: vec![SemanticBlock {
                id: entry,
                instructions: vec![],
                terminator: Some(Terminator::Return {
                    value: None,
                    type_: Type::Void,
                }),
            }],
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
            blocks: vec![SemanticBlock {
                id: entry,
                instructions: vec![],
                terminator: None,
            }],
            entry_block: entry,
            is_extern: false,
        };
        program.functions.push(func);
        assert!(VerifiedIR::new(program).is_err());
    }

    #[test]
    fn test_verified_ir_rejects_unreachable() {
        let mut program = SemanticProgram::new();
        let entry = program.new_block_id();
        let unreachable = program.new_block_id();
        let func = SemanticFunction {
            name: "main".to_string(),
            params: vec![],
            return_type: Type::Void,
            blocks: vec![
                SemanticBlock {
                    id: entry,
                    instructions: vec![],
                    terminator: Some(Terminator::Return {
                        value: None,
                        type_: Type::Void,
                    }),
                },
                SemanticBlock {
                    id: unreachable,
                    instructions: vec![],
                    terminator: Some(Terminator::Return {
                        value: None,
                        type_: Type::Void,
                    }),
                },
            ],
            entry_block: entry,
            is_extern: false,
        };
        program.functions.push(func);
        assert!(VerifiedIR::new(program).is_err());
    }

    #[test]
    fn test_verified_ir_display() {
        let mut program = SemanticProgram::new();
        let entry = program.new_block_id();
        let func = SemanticFunction {
            name: "main".to_string(),
            params: vec![],
            return_type: Type::Void,
            blocks: vec![SemanticBlock {
                id: entry,
                instructions: vec![],
                terminator: Some(Terminator::Return {
                    value: None,
                    type_: Type::Void,
                }),
            }],
            entry_block: entry,
            is_extern: false,
        };
        program.functions.push(func);
        let verified = VerifiedIR::new(program).expect("IR verification failed in test");
        assert_eq!(verified.to_string(), "VerifiedIR(functions: 1, blocks: 1)");
    }
}
