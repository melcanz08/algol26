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
                    if c.input != c.output {
                        return Err(PipelineError::AnalysisChangedLevel(
                            c.id, c.input, c.output,
                        ));
                    }
                }
                PassKind::Transform | PassKind::Verification => {
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
        }
    }
}

impl std::error::Error for PipelineError {}