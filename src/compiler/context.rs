// src/compiler/context.rs

use crate::common::diagnostics::Diagnostic;
use crate::compiler::capabilities::{BackendKind, CapabilityMatrix};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OptLevel {
    O0,
    O1,
    O2,
    O3,
}

/// Distinct from `crate::compiler::capabilities::Feature` — that one
/// describes what a *backend* supports. This one describes what the
/// *frontend and semantics* are allowed to accept.
///
/// The two are separate axes. A compilation may have
/// `LangFeature::Channels` enabled (the language accepts channel
/// syntax and semantics), while the LLVM backend's capability
/// matrix says `Support::None` for its own `Feature::Channels`
/// (the backend cannot lower it). The capability check refuses the
/// program at the backend boundary; it does not stop the frontend
/// from parsing or the analyzer from checking it.
///
/// Feature names overlap where the concepts align — `Channels`,
/// `Regions` — but the enums are not mirrors of each other. Do
/// not assume one variant exists because the other does.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum LangFeature {
    Generics,
    Traits,
    Regions,
    Channels,
}

#[derive(Debug, Clone, Default)]
pub struct FeatureSet {
    pub generics: bool,
    pub traits: bool,
    pub regions: bool,
    pub channels: bool,
    pub experimental_incremental: bool,
}

impl FeatureSet {
    /// Everything this compiler currently supports, on by default.
    /// This is what `CompilerConfig::default()` uses.
    pub fn all() -> Self {
        Self {
            generics: true,
            traits: true,
            regions: true,
            channels: true,
            experimental_incremental: false,
        }
    }

    /// Only the core language. Useful for fuzzing: if a program
    /// parses under `minimal`, it does not depend on an experimental
    /// or optional subsystem.
    pub fn minimal() -> Self {
        Self {
            generics: false,
            traits: false,
            regions: false,
            channels: false,
            experimental_incremental: false,
        }
    }

    pub fn enabled(&self, f: LangFeature) -> bool {
        match f {
            LangFeature::Generics => self.generics,
            LangFeature::Traits => self.traits,
            LangFeature::Regions => self.regions,
            LangFeature::Channels => self.channels,
        }
    }

    pub fn set(&mut self, f: LangFeature, on: bool) {
        match f {
            LangFeature::Generics => self.generics = on,
            LangFeature::Traits => self.traits = on,
            LangFeature::Regions => self.regions = on,
            LangFeature::Channels => self.channels = on,
        }
    }
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
            features: FeatureSet::all(),
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

    pub fn target(&self) -> BackendKind {
        self.config.target
    }

    pub fn push_diagnostic(&mut self, d: Diagnostic) {
        self.diagnostics.push(d);
    }

    pub fn push_error(&mut self, e: crate::common::diagnostics::CompileError) {
        self.diagnostics.push(Diagnostic::Error(e));
    }

    pub fn push_warning(&mut self, msg: impl Into<String>) {
        self.diagnostics.push(Diagnostic::Warning(msg.into()));
    }

    /// Only `Diagnostic::Error` is fatal. Warnings do not stop the
    /// pipeline.
    pub fn has_fatal_diagnostics(&self) -> bool {
        self.diagnostics
            .iter()
            .any(|d| matches!(d, Diagnostic::Error(_)))
    }

    /// Same as `has_fatal_diagnostics`, but only considers
    /// diagnostics at index `start` or later. Used by the scheduler
    /// so a reused context does not carry warnings from previous
    /// pipeline runs into the fatal check.
    pub fn has_fatal_diagnostics_since(&self, start: usize) -> bool {
        self.diagnostics[start.min(self.diagnostics.len())..]
            .iter()
            .any(|d| matches!(d, Diagnostic::Error(_)))
    }

    pub fn feature_enabled(&self, f: LangFeature) -> bool {
        self.config.features.enabled(f)
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

    /// Render every diagnostic collected so far as a single string.
    ///
    /// Useful for a future `inspect` command or for error paths that
    /// want to print once rather than per-diagnostic.
    pub fn render_diagnostics(&self) -> String {
        crate::diagnostics::renderer::render_all(&self.diagnostics)
    }

    /// Check whether `program` uses only features the configured
    /// target backend supports.
    ///
    /// Delegates to the scan in `backends::capabilities`, feeding it
    /// a `BackendCapabilities` reconstructed from this context's
    /// matrix. When config-driven overrides exist (e.g. `--no-spawn`),
    /// they will be reflected here without touching the scan.
    pub fn check_backend(
        &self,
        program: &crate::ir::semantic_ir::SemanticProgram,
    ) -> crate::common::diagnostics::Result<()> {
        let caps = self.capabilities.caps_for(self.config.target);
        crate::backends::capabilities::check_backend(program, &caps)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_config_enables_all_features() {
        let cfg = CompilerConfig::default();
        for f in [
            LangFeature::Generics,
            LangFeature::Traits,
            LangFeature::Regions,
            LangFeature::Channels,
        ] {
            assert!(cfg.features.enabled(f), "{:?} should default to on", f);
        }
    }

    #[test]
    fn minimal_feature_set_disables_optional_features() {
        let fs = FeatureSet::minimal();
        for f in [
            LangFeature::Generics,
            LangFeature::Traits,
            LangFeature::Regions,
            LangFeature::Channels,
        ] {
            assert!(!fs.enabled(f), "{:?} should be off in minimal", f);
        }
    }

    #[test]
    fn context_forwards_feature_queries_to_config() {
        let mut cfg = CompilerConfig::default();
        cfg.features.set(LangFeature::Generics, false);
        let ctx = CompilerContext::new(cfg);
        assert!(!ctx.feature_enabled(LangFeature::Generics));
        assert!(ctx.feature_enabled(LangFeature::Traits));
    }
}
