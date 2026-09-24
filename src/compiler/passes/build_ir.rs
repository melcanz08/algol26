// src/compiler/passes/build_ir.rs

use crate::compiler::context::CompilerContext;
use crate::compiler::pass::{IrLevel, Pass, PassContract, PassError, PassId, PassKind, PassResult};
use crate::compiler::program::Program;

pub struct BuildSemanticIRPass;

impl Pass<Program> for BuildSemanticIRPass {
    fn contract(&self) -> &PassContract {
        static C: PassContract = PassContract {
            id: PassId("ir.build"),
            kind: PassKind::Lowering,
            input: IrLevel::Ast,
            output: IrLevel::SemanticIr,
            requires: &["typed AST with analyzer-produced type table"],
            guarantees: &[
                "every function is lowered to a SemanticFunction",
                "every expression has an assigned block id",
                "the resulting program passes cfg verification",
            ],
            may_change: &["program.semantic_ir"],
            must_preserve: &["program.ast", "program.typed", "source spans"],
            may_fail: true,
        };
        &C
    }

    fn run(&self, _ctx: &mut CompilerContext, program: &mut Program) -> PassResult {
        // The type table lives on the analyzer's output, not the AST.
        // `TypedProgram::functions` shares its `Rc` with
        // `AstPayload::functions`, so lowering the same functions the
        // analyzer visited is guaranteed by the addressing invariant.
        let typed = program.typed.as_ref().ok_or_else(|| {
            PassError::new(
                PassId("ir.build"),
                "contract violation: typed AST not present (run TypeCheckPass first)",
            )
        })?;

        match crate::compiler::build_semantic_ir_program(
            &typed.functions,
            typed.type_table_id.clone(),
            typed.plan.clone(),
        ) {
            Ok(sem) => {
                program.semantic_ir = Some(sem);
                Ok(())
            }
            Err(e) => Err(PassError::new(PassId("ir.build"), e.message)),
        }
    }
}
