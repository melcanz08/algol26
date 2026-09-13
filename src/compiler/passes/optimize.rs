// src/compiler/passes/optimize.rs

use crate::compiler::context::CompilerContext;
use crate::compiler::pass::{
    IrLevel, Pass, PassContract, PassError, PassId, PassKind, PassResult,
};
use crate::compiler::program::Program;
use crate::ir::optimizer::Optimizer;

pub struct OptimizePass;

impl Pass<Program> for OptimizePass {
    fn contract(&self) -> &PassContract {
        static C: PassContract = PassContract {
            id: PassId("ir.optimize"),
            kind: PassKind::Transform,
            input: IrLevel::SemanticIr,
            output: IrLevel::SemanticIr,
            requires: &["semantic IR verified at least once"],
            guarantees: &["same observable behavior", "IR remains well-formed"],
            may_change: &["instructions within blocks", "block structure"],
            must_preserve: &[
                "program semantics",
                "function signatures",
                "types",
                "source spans",
            ],
            // The optimizer cannot fail — it is a pure rewrite.
            may_fail: false,
        };
        &C
    }

    fn run(&self, _ctx: &mut CompilerContext, program: &mut Program) -> PassResult {
        let sem = program.semantic_ir.as_mut().ok_or_else(|| {
            PassError::new(
                PassId("ir.optimize"),
                "contract violation: semantic IR not built",
            )
        })?;

        let mut optimizer = Optimizer::new();
        optimizer.optimize(sem);
        Ok(())
    }
}