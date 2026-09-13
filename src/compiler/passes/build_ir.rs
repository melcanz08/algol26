// src/compiler/passes/build_ir.rs

use crate::compiler::context::CompilerContext;
use crate::compiler::pass::{
    IrLevel, Pass, PassContract, PassError, PassId, PassKind, PassResult,
};
use crate::compiler::program::Program;

pub struct BuildSemanticIRPass;

impl Pass<Program> for BuildSemanticIRPass {
    fn contract(&self) -> &PassContract {
        static C: PassContract = PassContract {
            id: PassId("ir.build"),
            kind: PassKind::Lowering,
            input: IrLevel::Ast,
            output: IrLevel::SemanticIr,
            requires: &["typed AST", "type table from analyzer"],
            guarantees: &[
                "every function is lowered to a SemanticFunction",
                "every expression has an assigned block id",
                "the resulting program passes cfg verification",
            ],
            may_change: &["program.semantic_ir"],
            must_preserve: &["program.ast", "source spans"],
            may_fail: true,
        };
        &C
    }

    fn run(&self, _ctx: &mut CompilerContext, program: &mut Program) -> PassResult {
        let ast = program.ast.as_ref().ok_or_else(|| {
            PassError::new(
                PassId("ir.build"),
                "contract violation: AstPayload not present",
            )
        })?;

        match crate::compiler::build_semantic_ir_program(&ast.functions, ast.type_table.clone()) {
            Ok(sem) => {
                program.semantic_ir = Some(sem);
                Ok(())
            }
            Err(e) => {
                // The builder already printed its diagnostics to stderr.
                // Surface the same error message to the scheduler so the
                // equivalence test can compare failures.
                Err(PassError::new(PassId("ir.build"), e.message))
            }
        }
    }
}