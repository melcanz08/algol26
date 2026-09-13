// src/compiler/context.rs

use crate::compiler::capabilities::{BackendKind, CapabilityMatrix};
use crate::common::diagnostics::Diagnostic;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OptLevel { O0, O1, O2, O3 }

#[derive(Debug, Clone, Default)]
pub struct FeatureSet {
    pub generics: bool,
    pub traits: bool,
    pub regions: bool,
    pub channels: bool,
    pub experimental_incremental: bool,
}

#[derive(Debug, Clone)]
pub struct CompilerConfig {
    pub target: BackendKind,
    pub opt_level: OptLevel,
    pub debug_info: bool,
    pub strict_mode: bool,
    pub features: FeatureSet,
}

impl Default for CompilerConfig {
    fn default() -> Self {
        Self {
            target: BackendKind::Interpreter,
            opt_level: OptLevel::O0,
            debug_info: false,
            strict_mode: true,
            features: FeatureSet::default(),
        }
    }
}

pub struct CompilerContext {
    pub config: CompilerConfig,
    pub capabilities: CapabilityMatrix,
    pub diagnostics: Vec<Diagnostic>,
    next_id: u64,
}

impl CompilerContext {
    pub fn new(config: CompilerConfig) -> Self {
        Self {
            config,
            capabilities: CapabilityMatrix::standard(),
            diagnostics: Vec::new(),
            next_id: 0,
        }
    }

    pub fn fresh_id(&mut self) -> u64 {
        let id = self.next_id;
        self.next_id += 1;
        id
    }

    pub fn target(&self) -> BackendKind { self.config.target }

    pub fn push_diagnostic(&mut self, d: Diagnostic) {
        self.diagnostics.push(d);
    }

    pub fn push_error(&mut self, e: crate::common::diagnostics::CompileError) {
        self.diagnostics.push(Diagnostic::Error(e));
    }

    pub fn push_warning(&mut self, msg: impl Into<String>) {
        self.diagnostics.push(Diagnostic::Warning(msg.into()));
    }

    /// Only `Diagnostic::Error` is fatal. Warnings do not stop the pipeline.
    pub fn has_fatal_diagnostics(&self) -> bool {
        self.diagnostics
            .iter()
            .any(|d| matches!(d, Diagnostic::Error(_)))
    }

    pub fn error_count(&self) -> usize {
        self.diagnostics
            .iter()
            .filter(|d| matches!(d, Diagnostic::Error(_)))
            .count()
    }

    pub fn warning_count(&self) -> usize {
        self.diagnostics
            .iter()
            .filter(|d| matches!(d, Diagnostic::Warning(_)))
            .count()
    }
}