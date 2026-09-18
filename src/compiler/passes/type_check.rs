// src/compiler/passes/type_check.rs

use crate::compiler::context::CompilerContext;
use crate::compiler::pass::{IrLevel, Pass, PassContract, PassError, PassId, PassKind, PassResult};
use crate::compiler::program::Program;

pub struct TypeCheckPass;

impl Pass<Program> for TypeCheckPass {
    fn contract(&self) -> &PassContract {
        static C: PassContract = PassContract {
            id: PassId("ast.type_check"),
            kind: PassKind::Annotation,
            input: IrLevel::Ast,
            output: IrLevel::Ast,
            requires: &["parsed AST with traits and impls", "span map"],
            guarantees: &[
                "every expression reachable from a function body has a type in type_table",
                "no data races detected between spawned functions",
            ],
            may_change: &["program.typed"],
            must_preserve: &["program.ast", "source spans"],
            may_fail: true,
        };
        &C
    }

    fn run(&self, _ctx: &mut CompilerContext, program: &mut Program) -> PassResult {
        let ast = program.ast.as_ref().ok_or_else(|| {
            PassError::new(
                PassId("ast.type_check"),
                "contract violation: AstPayload not present",
            )
        })?;

        match crate::compiler::type_check_program(
            &ast.functions,
            &ast.traits,
            &ast.impls,
            &ast.span_map,
        ) {
            Ok(typed) => {
                program.typed = Some(typed);
                Ok(())
            }
            Err(e) => Err(PassError::from_compile_error(PassId("ast.type_check"), e)),
        }
    }
}
