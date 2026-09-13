// src/compiler/capabilities.rs

//! Capability matrix — a view over the backend capability tables.
//!
//! The authoritative declaration of what each backend supports lives
//! in `crate::backends::capabilities::BackendCapabilities`. This
//! module renders that data as a feature × backend grid for the
//! `inspect --capabilities` CLI and, in future, for config-driven
//! overrides.
//!
//! Keep the two in sync: `standard()` derives from the constructors,
//! and `tests::matrix_matches_backend_constructors` asserts they
//! agree.

pub use crate::backends::capabilities::{BackendCapabilities, Feature};

use std::collections::HashMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum BackendKind {
    Interpreter,
    Llvm,
    Wasm,
}

impl BackendKind {
    pub fn all() -> [BackendKind; 3] {
        [BackendKind::Interpreter, BackendKind::Llvm, BackendKind::Wasm]
    }
    pub fn name(self) -> &'static str {
        match self {
            BackendKind::Interpreter => "interpreter",
            BackendKind::Llvm => "llvm",
            BackendKind::Wasm => "wasm",
        }
    }
    pub fn caps(self) -> BackendCapabilities {
        match self {
            BackendKind::Interpreter => BackendCapabilities::interpreter(),
            BackendKind::Llvm => BackendCapabilities::llvm(),
            BackendKind::Wasm => BackendCapabilities::wasm(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Support {
    /// Backend lowers this feature natively.
    Full,
    /// Backend accepts this feature but semantics may diverge, or the
    /// backend requires a fallback. Currently unused by `standard()`;
    /// reserved for config-driven overrides.
    Partial,
    /// Backend refuses this feature.
    None,
}

pub struct CapabilityMatrix {
    entries: HashMap<(Feature, BackendKind), Support>,
}

impl CapabilityMatrix {
    pub fn new() -> Self {
        Self { entries: HashMap::new() }
    }

    pub fn set(&mut self, f: Feature, b: BackendKind, s: Support) {
        self.entries.insert((f, b), s);
    }

    pub fn get(&self, f: Feature, b: BackendKind) -> Support {
        self.entries.get(&(f, b)).copied().unwrap_or(Support::None)
    }

    pub fn supports(&self, f: Feature, b: BackendKind) -> bool {
        matches!(self.get(f, b), Support::Full)
    }

    /// Derive from `BackendCapabilities::llvm()/wasm()/interpreter()`.
    ///
    /// The constructors are the source of truth; this view is what the
    /// renderer and the CLI consume.
    pub fn standard() -> Self {
        let mut m = Self::new();
        for b in BackendKind::all() {
            let caps = b.caps();
            for f in Feature::all() {
                let s = if caps.supported.contains(f) {
                    Support::Full
                } else {
                    Support::None
                };
                m.set(*f, b, s);
            }
        }
        m
    }

    /// Reconstruct a `BackendCapabilities` from the matrix.
    ///
    /// Used by `CompilerContext::check_backend` so that config overrides
    /// (once they exist) reach the scan.
    pub fn caps_for(&self, b: BackendKind) -> BackendCapabilities {
        let mut supported = std::collections::HashSet::new();
        for f in Feature::all() {
            // Only `Full` counts as supported. `Partial` means
            // "the backend accepts this but semantics may diverge,
            // or a fallback is required" — that is not the same
            // promise, and collapsing the two would let a backend
            // run on a program it cannot honestly compile.
            if matches!(self.get(*f, b), Support::Full) {
                supported.insert(*f);
            }
        }
        BackendCapabilities {
            name: b.name(),
            supported,
            has_interpreter_fallback: b != BackendKind::Interpreter,
        }
    }

    /// Text rendering for `algol26 inspect --capabilities`.
    pub fn render_table(&self) -> String {
        let mut out = String::new();
        out.push_str(&format!("{:<14}", "feature"));
        for b in BackendKind::all() {
            out.push_str(&format!("{:>13}", b.name()));
        }
        out.push('\n');

        for f in Feature::all() {
            out.push_str(&format!("{:<14}", f.name()));
            for b in BackendKind::all() {
                let cell = match self.get(*f, b) {
                    Support::Full => "✓",
                    Support::Partial => "⚠",
                    Support::None => "✗",
                };
                out.push_str(&format!("{:>13}", cell));
            }
            out.push('\n');
        }
        out
    }
}

impl Default for CapabilityMatrix {
    fn default() -> Self { Self::standard() }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matrix_matches_backend_constructors() {
        let m = CapabilityMatrix::standard();
        for b in BackendKind::all() {
            let caps = b.caps();
            for f in Feature::all() {
                let from_caps = caps.supported.contains(f);
                let from_matrix = m.supports(*f, b);
                assert_eq!(
                    from_caps, from_matrix,
                    "matrix and constructor disagree on {} for {}",
                    f.name(), b.name()
                );
            }
        }
    }

    #[test]
    fn caps_for_round_trips() {
        let m = CapabilityMatrix::standard();
        for b in BackendKind::all() {
            let derived = m.caps_for(b);
            let original = b.caps();
            assert_eq!(derived.supported, original.supported);
            assert_eq!(derived.has_interpreter_fallback, original.has_interpreter_fallback);
        }
    }
    #[test]
    fn partial_is_not_collapsed_into_supported() {
        use crate::backends::capabilities::Feature;
        let mut m = CapabilityMatrix::new();
        m.set(Feature::Spawn, BackendKind::Llvm, Support::Partial);
        let caps = m.caps_for(BackendKind::Llvm);
        assert!(
            !caps.supported.contains(&Feature::Spawn),
            "Partial must not be treated as Full by caps_for"
        );
    }
}