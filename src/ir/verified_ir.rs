// src/ir/verified_ir.rs
//
// `VerifiedIR::new` runs verifier::verify before returning.
// The type is a compile-time promise: a `VerifiedIR` value can only
// exist if verification succeeded. `BackendInput` is an alias for
// this type, so backend entry points that take `&VerifiedIR` are
// statically guaranteed to receive verified input.
//
// Note that "verified" here means "passed verifier", whose
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
        crate::ir::verifier::verify(&program)?;
        Ok(VerifiedIR { program })
    }

    /// Wrap a program that has *just* been checked by `VerifyIrPass`.
    ///
    /// # Safety discipline (not `unsafe`, but a contract)
    ///
    /// This bypasses verification. It must only be called on a program
    /// the caller has verified in the *same* call, with no intervening
    /// mutation.
    ///
    /// Permitted call sites (all in `compiler.rs`):
    ///   - `run_verify_pass` — after `VerifyIrPass` runs
    ///   - `run_optimize_pass` — after `OptimizePass` + `VerifyIrPass`
    ///
    /// Adding a new caller requires justifying the "already verified"
    /// claim. In debug builds, the contract is enforced by re-running
    /// the verifier; in release, we trust the caller.
    pub(crate) fn from_verify_pass(program: SemanticProgram) -> Self {
        debug_assert!(
            crate::ir::verifier::verify(&program).is_ok(),
            "VerifiedIR::from_verify_pass called on a program that fails verification"
        );
        VerifiedIR { program }
    }

    pub fn program(&self) -> &SemanticProgram {
        &self.program
    }

    /// Consume this wrapper and return the inner program.
    ///
    /// This fully unwraps the typestate — the returned `SemanticProgram`
    /// is no longer guaranteed to be verified. The single legitimate use
    /// is the optimize sandwich in `Compiler::run_optimize_pass`:
    /// `verified.into_program()` is immediately followed by mutation,
    /// then re-verification, then re-wrapping via
    /// `VerifiedIR::from_verify_pass`.
    ///
    /// Any other call site is a bug. Use `program()` (which borrows) or
    /// `verify()` (which re-checks) instead.
    pub fn into_program(self) -> SemanticProgram {
        self.program
    }

    pub fn verify(&self) -> Result<(), String> {
        crate::ir::verifier::verify(&self.program)
    }

    /// Consume this `VerifiedIR`, apply `f` to the inner program, and
    /// re-run the verifier on the result.
    ///
    /// Takes `self` by value and returns a fresh `VerifiedIR` on
    /// success. On failure, the invalid program is dropped with the
    /// consumed `self`; the caller never sees a `VerifiedIR` that has
    /// not passed verification.
    ///
    /// This is the only sanctioned way to mutate a `VerifiedIR`. It
    /// replaces the earlier `into_program` escape hatch: the caller
    /// supplies a mutation function and gets back a re-verified
    /// wrapper, rather than peeling the wrapper off, mutating, and
    /// hoping to remember to re-wrap.
    ///
    /// # Errors
    ///
    /// Returns the verifier's error message if `f` produced IR that
    /// fails verification. The consumed `self` is dropped; the caller
    /// must construct a fresh `VerifiedIR` if it wants to continue.
    pub fn mutate<F>(mut self, f: F) -> Result<Self, String>
    where
        F: FnOnce(&mut SemanticProgram),
    {
        f(&mut self.program);
        crate::ir::verifier::verify(&self.program)?;
        Ok(self)
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
