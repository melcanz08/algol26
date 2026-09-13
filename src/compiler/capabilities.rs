// src/compiler/capabilities.rs

use std::collections::HashMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum BackendKind { Interpreter, Llvm, Wasm }

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
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Support {
    Full,
    /// Supported but semantically divergent, or requires a fallback.
    Partial,
    None,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Feature {
    Ownership, Spawn, Ffi, Channels, Regions,
    Generics, Traits, Defer, TryCatch,
}

impl Feature {
    pub fn all() -> &'static [Feature] {
        use Feature::*;
        &[Ownership, Spawn, Ffi, Channels, Regions,
          Generics, Traits, Defer, TryCatch]
    }
    pub fn name(self) -> &'static str {
        match self {
            Feature::Ownership => "ownership",
            Feature::Spawn     => "spawn",
            Feature::Ffi       => "ffi",
            Feature::Channels  => "channels",
            Feature::Regions   => "regions",
            Feature::Generics  => "generics",
            Feature::Traits    => "traits",
            Feature::Defer     => "defer",
            Feature::TryCatch  => "try/catch",
        }
    }
}

pub struct CapabilityMatrix {
    entries: HashMap<(Feature, BackendKind), Support>,
}

impl CapabilityMatrix {
    pub fn new() -> Self { Self { entries: HashMap::new() } }

    pub fn set(&mut self, f: Feature, b: BackendKind, s: Support) {
        self.entries.insert((f, b), s);
    }

    pub fn get(&self, f: Feature, b: BackendKind) -> Support {
        self.entries.get(&(f, b)).copied().unwrap_or(Support::None)
    }

    pub fn supports(&self, f: Feature, b: BackendKind) -> bool {
        matches!(self.get(f, b), Support::Full)
    }

    /// The baseline matrix. **Keep this honest** — every `None` here should
    /// correspond to a refusal in the relevant backend, not a silent miscompile.
    ///
    ///   Ownership  : semantic, backend-independent -> Full everywhere
    ///   Generics   : monomorphized before lowering  -> Full everywhere
    ///   Spawn      : only the interpreter runs it; LLVM/WASM refuse
    ///                (see tests/conformance/valid/spawn_llvm_refused.gol)
    ///   FFI        : LLVM only (C ABI); interpreter/WASM refuse
    pub fn standard() -> Self {
        use BackendKind::*;
        use Feature::*;
        use Support::*;

        let mut m = Self::new();

        // Cross-backend features.
        for b in BackendKind::all() {
            m.set(Ownership, b, Full);
            m.set(Generics,  b, Full);
            m.set(Traits,    b, Full);
            m.set(Regions,   b, Full);
            m.set(Defer,     b, Full);
        }

        // Backend-specific features.
        m.set(Spawn,    Interpreter, Full);
        m.set(Spawn,    Llvm,        None);
        m.set(Spawn,    Wasm,        Partial);

        m.set(Channels, Interpreter, Full);
        m.set(Channels, Llvm,        None);
        m.set(Channels, Wasm,        None);

        m.set(Ffi,      Interpreter, None);
        m.set(Ffi,      Llvm,        Full);
        m.set(Ffi,      Wasm,        None);

        m.set(TryCatch, Interpreter, Full);
        m.set(TryCatch, Llvm,        Full);
        m.set(TryCatch, Wasm,        None);

        m
    }

    /// Text rendering for `algol26 inspect --capabilities`.
    pub fn render_table(&self) -> String {
        let features = Feature::all();
        let backends = BackendKind::all();
        let mut out = String::new();
        out.push_str("feature       ");
        for b in backends { out.push_str(&format!("{:>12} ", b.name())); }
        out.push('\n');
        for f in features {
            out.push_str(&format!("{:<14}", f.name()));
            for b in backends {
                let cell = match self.get(*f, b) {
                    Support::Full => "✓",
                    Support::Partial => "⚠",
                    Support::None => "✗",
                };
                out.push_str(&format!("{:>12} ", cell));
            }
            out.push('\n');
        }
        out
    }
}

impl Default for CapabilityMatrix { fn default() -> Self { Self::new() } }