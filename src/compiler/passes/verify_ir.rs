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
use crate::compiler::pass::{IrLevel, Pass, PassContract, PassError, PassId, PassKind, PassResult};
use crate::compiler::program::Program;
use crate::ir::cfg::{build_cfg_from_semantic_program, DataflowEngine, OwnershipTransfer};
use crate::semantics::state::SemanticState;

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

        // NEW: fixed-point dataflow check
        let cfg = build_cfg_from_semantic_program(sem);
        let engine = DataflowEngine::new(OwnershipTransfer);
        let result = engine.run(&cfg, SemanticState::new());

        if result.has_errors() {
            let msg = result
                .diagnostics
                .iter()
                .map(|d| d.message.clone())
                .collect::<Vec<_>>()
                .join("\n");
            ctx.push_error(CompileError::simple(
                &format!("Dataflow verification failed:\n{}", msg),
                0,
                0,
                "",
                ErrorCode::E0002,
            ));
            return Err(PassError::recoverable(PassId("ir.verify"), msg));
        }

        // ADR 0014: executable-IR invariants (no TypeVar leaked,
        // every call targets a defined function). Independent of
        // the builder and the plan's closure algorithm.
        let empty_plan = crate::ir::instantiation_plan::InstantiationPlan::default();
        let plan_ref = program
            .typed
            .as_ref()
            .map(|t| &t.plan)
            .unwrap_or(&empty_plan);

        if let Err(invariant_errors) =
            crate::ir::verifier::invariants::check_invariants(sem, plan_ref)
        {
            let msg = invariant_errors
                .iter()
                .map(|e| e.to_string())
                .collect::<Vec<_>>()
                .join("\n");
            ctx.push_error(CompileError::simple(
                &format!("IR invariant check failed:\n{}", msg),
                0,
                0,
                "",
                ErrorCode::E0002,
            ));
            return Err(PassError::recoverable(PassId("ir.verify"), msg));
        }

        // Existing instruction-level verification.
        sem.verify().map_err(|msg| {
            ctx.push_error(CompileError::simple(
                &format!("IR verification failed: {}", msg),
                0,
                0,
                "",
                ErrorCode::E0002,
            ));
            PassError::recoverable(PassId("ir.verify"), msg)
        })
    }
}
