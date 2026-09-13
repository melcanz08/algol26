// src/compiler/pipeline.rs

use crate::compiler::pass::{IrLevel, Pass, PassId, PassKind};
use std::fmt;

pub struct Pipeline<Prog> {
    stages: Vec<Box<dyn Pass<Prog>>>,
}

impl<Prog> Pipeline<Prog> {
    pub fn stages(&self) -> &[Box<dyn Pass<Prog>>] { &self.stages }
    pub fn len(&self) -> usize { self.stages.len() }
    pub fn is_empty(&self) -> bool { self.stages.is_empty() }
}

impl<Prog: 'static> Pipeline<Prog> {
    pub fn builder() -> PipelineBuilder<Prog> {
        PipelineBuilder { stages: Vec::new() }
    }
}

pub struct PipelineBuilder<Prog> {
    stages: Vec<Box<dyn Pass<Prog>>>,
}

impl<Prog: 'static> PipelineBuilder<Prog> {
    pub fn add<P: Pass<Prog> + 'static>(mut self, pass: P) -> Self {
        self.stages.push(Box::new(pass));
        self
    }

    pub fn add_boxed(mut self, pass: Box<dyn Pass<Prog>>) -> Self {
        self.stages.push(pass);
        self
    }

    pub fn build(self) -> Result<Pipeline<Prog>, PipelineError> {
        Self::validate_chain(&self.stages)?;
        Ok(Pipeline { stages: self.stages })
    }

    /// Contract-chain validation.
    ///
    /// Rules:
    ///   - `Analysis`      : no constraint on `input`/`output` (must be equal).
    ///   - `Transform`     : `input == output`, must match current level.
    ///   - `Verification`  : `input == output`, must match current level.
    ///   - `Lowering`      : `input == current`, advances current to `output`.
    ///
    /// The first non-Analysis pass establishes the initial level.
    fn validate_chain(stages: &[Box<dyn Pass<Prog>>]) -> Result<(), PipelineError> {
        let mut current: Option<IrLevel> = None;

        for stage in stages {
            let c = stage.contract();
            match c.kind {
                PassKind::Analysis => {
                    // Analysis reads the current representation and
                    // produces metadata, not a new IR level.
                    if c.input != c.output {
                        return Err(PipelineError::AnalysisChangedLevel(
                            c.id, c.input, c.output,
                        ));
                    }
                    if let Some(cur) = current {
                        if cur != c.input {
                            return Err(PipelineError::AnalysisAtWrongLevel(
                                c.id, cur, c.input,
                            ));
                        }
                    }
                    current = Some(c.input);
                }
                PassKind::Transform | PassKind::Verification | PassKind::Annotation => {
                    if c.input != c.output {
                        return Err(PipelineError::NonLoweringChangedLevel(
                            c.id, c.input, c.output,
                        ));
                    }
                    if let Some(cur) = current {
                        if cur != c.input {
                            return Err(PipelineError::ChainMismatch(c.id, cur, c.input));
                        }
                    }
                    current = Some(c.input);
                }
                PassKind::Lowering => {
                    if let Some(cur) = current {
                        if cur != c.input {
                            return Err(PipelineError::ChainMismatch(c.id, cur, c.input));
                        }
                    }
                    if c.output <= c.input {
                        return Err(PipelineError::LoweringDidNotAdvance(
                            c.id, c.input, c.output,
                        ));
                    }
                    current = Some(c.output);
                }
            }
        }
        Ok(())
    }
}

#[derive(Debug)]
pub enum PipelineError {
    ChainMismatch(PassId, IrLevel, IrLevel),
    NonLoweringChangedLevel(PassId, IrLevel, IrLevel),
    AnalysisChangedLevel(PassId, IrLevel, IrLevel),
    /// A `Lowering` pass declared `output <= input`, which does not
    /// advance the IR level. `Lowering` must move strictly forward.
    LoweringDidNotAdvance(PassId, IrLevel, IrLevel),
    /// An `Analysis` pass declared an `input` level that does not
    /// match the pipeline's current level. Analyses run on the
    /// current representation; out-of-band inspection is not
    /// supported.
    AnalysisAtWrongLevel(PassId, IrLevel, IrLevel),
    UnknownPass(PassId),
}

impl fmt::Display for PipelineError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ChainMismatch(id, expected, got) => write!(
                f,
                "pass `{}` expects input `{}` but pipeline is at `{}`",
                id, got, expected
            ),
            Self::NonLoweringChangedLevel(id, i, o) => write!(
                f,
                "pass `{}` is not a lowering pass but changes level {} -> {}",
                id, i, o
            ),
            Self::AnalysisChangedLevel(id, i, o) => write!(
                f,
                "analysis pass `{}` must not change level ({} -> {})",
                id, i, o
            ),
            Self::UnknownPass(id) => write!(f, "unknown pass `{}`", id),
            Self::LoweringDidNotAdvance(id, input, output) => write!(
                f,
                "lowering pass `{}` does not advance IR level ({} -> {})",
                id, input, output
            ),
            Self::AnalysisAtWrongLevel(id, expected, got) => write!(
                f,
                "analysis pass `{}` expects input `{}` but pipeline is at `{}`",
                id, got, expected
            ),
        }
    }
}

impl std::error::Error for PipelineError {}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::compiler::context::CompilerContext;
    use crate::compiler::pass::{PassContract, PassId, PassResult};
    use crate::compiler::program::Program;

    macro_rules! dummy_pass {
        ($name:ident, $id:expr, $kind:expr, $in:expr, $out:expr) => {
            struct $name;
            impl Pass<Program> for $name {
                fn contract(&self) -> &PassContract {
                    static C: PassContract = PassContract {
                        id: PassId($id),
                        kind: $kind,
                        input: $in,
                        output: $out,
                        requires: &[],
                        guarantees: &[],
                        may_change: &[],
                        must_preserve: &[],
                        may_fail: false,
                    };
                    &C
                }
                fn run(&self, _: &mut CompilerContext, _: &mut Program) -> PassResult {
                    Ok(())
                }
            }
        };
    }

    dummy_pass!(NoAdvance, "test.no_advance",
        PassKind::Lowering, IrLevel::Ast, IrLevel::Ast);

    dummy_pass!(Backwards, "test.backwards",
        PassKind::Lowering, IrLevel::SemanticIr, IrLevel::Ast);

    dummy_pass!(GoodLowering, "test.good_lowering",
        PassKind::Lowering, IrLevel::Ast, IrLevel::SemanticIr);

    dummy_pass!(AstLower, "test.ast_lower",
        PassKind::Lowering, IrLevel::Source, IrLevel::Ast);

    dummy_pass!(AnalysisAtAst, "test.analysis_ast",
        PassKind::Analysis, IrLevel::Ast, IrLevel::Ast);

    #[test]
    fn lowering_must_advance() {
        match Pipeline::builder().add(NoAdvance).build() {
            Err(PipelineError::LoweringDidNotAdvance(_, _, _)) => {}
            Err(other) => panic!("expected LoweringDidNotAdvance, got {:?}", other),
            Ok(_) => panic!("expected LoweringDidNotAdvance, pipeline built successfully"),
        }
    }

    #[test]
    fn lowering_cannot_go_backwards() {
        match Pipeline::builder().add(Backwards).build() {
            Err(PipelineError::LoweringDidNotAdvance(_, _, _)) => {}
            Err(other) => panic!("expected LoweringDidNotAdvance, got {:?}", other),
            Ok(_) => panic!("expected LoweringDidNotAdvance, pipeline built successfully"),
        }
    }

    #[test]
    fn analysis_must_match_current_level() {
        match Pipeline::builder()
            .add(GoodLowering)
            .add(AnalysisAtAst)
            .build()
        {
            Err(PipelineError::AnalysisAtWrongLevel(_, _, _)) => {}
            Err(other) => panic!("expected AnalysisAtWrongLevel, got {:?}", other),
            Ok(_) => panic!("expected AnalysisAtWrongLevel, pipeline built successfully"),
        }
    }

    #[test]
    fn analysis_at_correct_level_is_accepted() {
        Pipeline::builder()
            .add(AstLower)
            .add(AnalysisAtAst)
            .build()
            .expect("analysis at current level should be accepted");
    }
}