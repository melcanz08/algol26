// src/compiler/passes/optimize.rs

use crate::compiler::context::CompilerContext;
use crate::compiler::pass::{IrLevel, Pass, PassContract, PassError, PassId, PassKind, PassResult};
use crate::compiler::program::{IrState, Program};
use crate::ir::optimizer::Optimizer;

pub struct OptimizePass;

impl Pass<Program> for OptimizePass {
    fn contract(&self) -> &PassContract {
        static C: PassContract = PassContract {
            id: PassId("ir.optimize"),
            kind: PassKind::Transform,
            input: IrLevel::VerifiedIr,
            output: IrLevel::VerifiedIr,
            requires: &["verified IR present"],
            guarantees: &[
                "same observable behavior",
                "IR remains well-formed",
                "program.ir stays IrState::Verified",
            ],
            may_change: &[
                "instructions within blocks",
                "block structure",
                "program.ir payload",
            ],
            must_preserve: &[
                "program semantics",
                "function signatures",
                "types",
                "source spans",
            ],
            may_fail: false,
        };
        &C
    }

    fn run(&self, _ctx: &mut CompilerContext, program: &mut Program) -> PassResult {
        let taken = std::mem::replace(&mut program.ir, IrState::Absent);
        let verified = match taken {
            IrState::Verified(v) => v,
            other => {
                program.ir = other;
                return Err(PassError::new(
                    PassId("ir.optimize"),
                    "contract violation: optimization requires verified IR",
                ));
            }
        };

        let optimized = verified
            .mutate(|p| {
                let mut opt = Optimizer::new();
                opt.optimize(p);
            })
            .map_err(|e| {
                PassError::new(
                    PassId("ir.optimize"),
                    format!("optimization produced invalid IR: {}", e),
                )
            })?;

        program.ir = IrState::Verified(optimized);
        Ok(())
    }
}
