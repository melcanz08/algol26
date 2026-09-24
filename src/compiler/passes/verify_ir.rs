// src/compiler/passes/verify_ir.rs
//
// ADR 0017. Two passes:
//
//   `VerifyIrPass`   — SemanticIr -> VerifiedIr. Promotion.
//                      Consumes `IrState::Built`, runs the full
//                      check suite, stores `IrState::Verified`.
//
//   `ReVerifyPass`   — VerifiedIr -> VerifiedIr. Re-check after a
//                      transform. Consumes and re-stores
//                      `IrState::Verified`.
//
// Both share `run_checks`, the suite that runs dataflow,
// invariants, and the instruction-level verifier.

use crate::common::diagnostics::{CompileError, ErrorCode};
use crate::compiler::context::CompilerContext;
use crate::compiler::pass::{IrLevel, Pass, PassContract, PassError, PassId, PassKind, PassResult};
use crate::compiler::program::{IrState, Program};
use crate::ir::cfg::{build_cfgs_from_semantic_program, DataflowEngine, OwnershipTransfer};
use crate::ir::instantiation_plan::InstantiationPlan;
use crate::ir::semantic_ir::SemanticProgram;
use crate::ir::verified_ir::VerifiedIR;

/// Run the full verification suite on `sem`. Emits diagnostics and
/// returns `Err` on failure. Shared between the promotion pass and
/// the re-check pass so both apply the same checks.
fn run_checks(
    ctx: &mut CompilerContext,
    sem: &SemanticProgram,
    plan: &InstantiationPlan,
    pass_id: PassId,
) -> PassResult {
    // ADR 0016: per-function dataflow.
    let cfgs = build_cfgs_from_semantic_program(sem);
    let engine = DataflowEngine::new(OwnershipTransfer);
    let result = engine.run_all(&cfgs);

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
        return Err(PassError::recoverable(pass_id, msg));
    }

    // ADR 0014: executable-IR invariants.
    if let Err(invariant_errors) = crate::ir::verifier::invariants::check_invariants(sem, plan) {
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
        return Err(PassError::recoverable(pass_id, msg));
    }

    // Instruction-level verifier.
    sem.verify().map_err(|msg| {
        ctx.push_error(CompileError::simple(
            &format!("IR verification failed: {}", msg),
            0,
            0,
            "",
            ErrorCode::E0002,
        ));
        PassError::recoverable(pass_id, msg)
    })
}

fn plan_ref(program: &Program) -> InstantiationPlan {
    program
        .typed
        .as_ref()
        .map(|t| t.plan.clone())
        .unwrap_or_default()
}

// ─── VerifyIrPass — SemanticIr -> VerifiedIr ────────────────────────

pub struct VerifyIrPass;

impl Pass<Program> for VerifyIrPass {
    fn contract(&self) -> &PassContract {
        static C: PassContract = PassContract {
            id: PassId("ir.verify"),
            kind: PassKind::Lowering,
            input: IrLevel::SemanticIr,
            output: IrLevel::VerifiedIr,
            requires: &["semantic IR built", "types resolved", "CFG valid"],
            guarantees: &[
                "instruction operands are well-typed",
                "branch conditions are Bool",
                "returns coerce to function return type",
                "calls resolve to known signatures",
                "no TypeVar in executable IR",
                "program.ir transitions to IrState::Verified",
            ],
            may_change: &["diagnostics", "program.ir"],
            must_preserve: &["program", "source spans"],
            may_fail: true,
        };
        &C
    }

    fn run(&self, ctx: &mut CompilerContext, program: &mut Program) -> PassResult {
        // Consume the current IR. If it is already Verified, this is
        // idempotent — a second VerifyIrPass on an already-verified
        // program is a no-op.
        let taken = std::mem::replace(&mut program.ir, IrState::Absent);
        let sem = match taken {
            IrState::Built(p) => p,
            IrState::Verified(v) => {
                program.ir = IrState::Verified(v);
                return Ok(());
            }
            IrState::Absent => {
                return Err(PassError::new(
                    PassId("ir.verify"),
                    "contract violation: no IR to verify",
                ));
            }
        };

        let plan = plan_ref(program);
        run_checks(ctx, &sem, &plan, PassId("ir.verify"))?;

        let verified = VerifiedIR::from_verify_pass(sem);
        program.ir = IrState::Verified(verified);
        Ok(())
    }
}

// ─── ReVerifyPass — VerifiedIr -> VerifiedIr ────────────────────────

pub struct ReVerifyPass;

impl Pass<Program> for ReVerifyPass {
    fn contract(&self) -> &PassContract {
        static C: PassContract = PassContract {
            id: PassId("ir.reverify"),
            kind: PassKind::Verification,
            input: IrLevel::VerifiedIr,
            output: IrLevel::VerifiedIr,
            requires: &["verified IR present", "optimization applied"],
            guarantees: &[
                "optimized IR still passes dataflow",
                "optimized IR still passes invariants",
                "optimized IR still passes instruction-level verifier",
            ],
            may_change: &["diagnostics"],
            must_preserve: &["program", "source spans"],
            may_fail: true,
        };
        &C
    }

    fn run(&self, ctx: &mut CompilerContext, program: &mut Program) -> PassResult {
        let taken = std::mem::replace(&mut program.ir, IrState::Absent);
        let verified = match taken {
            IrState::Verified(v) => v,
            other => {
                // Put the state back before erroring so the caller
                // sees an unchanged program.
                program.ir = other;
                return Err(PassError::new(
                    PassId("ir.reverify"),
                    "contract violation: re-verification requires verified IR",
                ));
            }
        };

        let plan = plan_ref(program);
        run_checks(ctx, verified.program(), &plan, PassId("ir.reverify"))?;

        program.ir = IrState::Verified(verified);
        Ok(())
    }
}
