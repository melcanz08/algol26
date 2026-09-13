// src/compiler/passes/verify_ir.rs

//! Wraps the existing instruction-level semantic verifier
//! (`crate::ir::verifier::verify`) as a `Pass`.
//!
//! This is the *only* place that calls the verifier through the new
//! pipeline. The old call site in `Compiler::compile` is untouched.
//! A test in `tests/compiler_pipeline_equiv.rs` asserts both paths
//! return the same result on every conformance input.

use crate::common::diagnostics::{CompileError, ErrorCode};
use crate::compiler::context::CompilerContext;
use crate::compiler::pass::{
    IrLevel, Pass, PassContract, PassError, PassId, PassKind, PassResult,
};
use crate::compiler::program::Program;

pub struct VerifyIrPass;

impl Pass<Program> for VerifyIrPass {
    fn contract(&self) -> &PassContract {
        static C: PassContract = PassContract {
            id: PassId("ir.verify"),
            kind: PassKind::Verification,
            input: IrLevel::SemanticIr,
            output: IrLevel::SemanticIr,
            requires: &["semantic IR built", "types resolved", "CFG valid"],
            guarantees: &[
                "instruction operands are well-typed",
                "branch conditions are Bool",
                "returns coerce to function return type",
                "calls resolve to known signatures",
            ],
            may_change: &["diagnostics"],
            must_preserve: &["program", "source spans"],
            may_fail: true,
        };
        &C
    }

    fn run(&self, ctx: &mut CompilerContext, program: &mut Program) -> PassResult {
        let sem = program.semantic_ir.as_ref().ok_or_else(|| {
            PassError::new(
                PassId("ir.verify"),
                "contract violation: semantic IR not built",
            )
        })?;

        match sem.verify() {
            Ok(()) => Ok(()),
            Err(msg) => {
                // Mirror the driver's existing wording so the equivalence
                // test compares like-for-like. When the verifier is
                // migrated to emit `CompileError` directly, this becomes
                // a pass-through.
                ctx.push_error(CompileError::simple(
                    &format!("IR verification failed: {}", msg),
                    0, 0, "",
                    ErrorCode::E0002,
                ));
                Err(PassError::recoverable(PassId("ir.verify"), msg))
            }
        }
    }
}