// src/compiler/pass.rs

use crate::compiler::context::CompilerContext;
use std::fmt;

/// Stable identifier. Used in logs, contracts, `inspect --passes`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct PassId(pub &'static str);

impl fmt::Display for PassId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.0)
    }
}

/// Classification. The scheduler uses this to enforce the pipeline shape.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PassKind {
    /// Pure. Must not mutate `Program`. Safe to run on demand, cache, reorder.
    Analysis,
    /// Mutates `Program` but must not change IR level.
    /// Must be followed by a `Verification` pass.
    Transform,
    /// Reads `Program`, never mutates. May emit diagnostics.
    Verification,
    /// Consumes one IR level and produces the next (e.g. AST -> SemanticIr).
    Lowering,
    /// Reads `Program`, produces new metadata at the same IR level.
    /// Does not mutate the primary representation, so no trailing
    /// `Verification` is required — unlike `Transform`.
    Annotation,
}

/// Compilation IR level. The scheduler uses these to validate the pipeline.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum IrLevel {
    Source,
    Ast,
    SemanticIr,
    VerifiedIr,
    OptimizedIr,
    Lowered,
    Backend,
}

impl fmt::Display for IrLevel {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match self {
            IrLevel::Source => "source",
            IrLevel::Ast => "ast",
            IrLevel::SemanticIr => "semantic-ir",
            IrLevel::VerifiedIr => "verified-ir",
            IrLevel::OptimizedIr => "optimized-ir",
            IrLevel::Lowered => "lowered",
            IrLevel::Backend => "backend",
        };
        f.write_str(s)
    }
}

/// The formal contract.
///
/// The `requires` / `guarantees` / `may_change` / `must_preserve`
/// fields are **metadata**: they document what the pass promises,
/// in a form a human can read. The compiler does not execute them.
///
/// What the compiler *does* enforce structurally is `kind`,
/// `input`, and `output` — via `PipelineBuilder::validate_chain`
/// and `Scheduler`. Those three fields are the machine-checked
/// portion of the contract; everything else is a contract with the
/// reader.
#[derive(Debug, Clone)]
pub struct PassContract {
    pub id: PassId,
    pub kind: PassKind,
    pub input: IrLevel,
    pub output: IrLevel,
    pub requires: &'static [&'static str],
    pub guarantees: &'static [&'static str],
    pub may_change: &'static [&'static str],
    pub must_preserve: &'static [&'static str],
    pub may_fail: bool,
}

/// Result of running a pass.
///
/// `Err` is for *infrastructure* failures (contract violation, invariant
/// broken, unexpected panic-as-error). User-facing diagnostics should be
/// pushed onto `ctx.diagnostics` and the pass should return `Ok(())`; the
/// scheduler will detect fatal diagnostics afterwards.
pub type PassResult<T = ()> = Result<T, PassError>;

#[derive(Debug)]
pub struct PassError {
    pub pass: PassId,
    pub message: String,
    pub fatal: bool,
    /// If this failure originated from a `CompileError`, the original
    /// is preserved here so `ErrorCode` and span survive the pass
    /// boundary. `run_*_pass` helpers prefer this over reconstructing.
    pub cause: Option<Box<crate::common::diagnostics::CompileError>>,
}

impl PassError {
    pub fn new(pass: PassId, message: impl Into<String>) -> Self {
        Self {
            pass,
            message: message.into(),
            fatal: true,
            cause: None,
        }
    }
    pub fn recoverable(pass: PassId, message: impl Into<String>) -> Self {
        Self {
            pass,
            message: message.into(),
            fatal: false,
            cause: None,
        }
    }
    /// Construct from a `CompileError`, preserving its code and span.
    pub fn from_compile_error(pass: PassId, err: crate::common::diagnostics::CompileError) -> Self {
        Self {
            pass,
            message: err.message.clone(),
            fatal: true,
            cause: Some(Box::new(err)),
        }
    }
}

impl fmt::Display for PassError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "pass `{}` failed: {}", self.pass, self.message)
    }
}

impl std::error::Error for PassError {}

/// The core trait.
///
/// `Prog` is your compilation unit. In practice you'll want a single
/// `Program` struct holding all representations (ast, semantic_ir,
/// verified_ir, ...) and passes read the level they need and write the
/// level they produce — validated by the contract.
///
/// Passes are stateless. All state lives in `ctx` and `program`.
pub trait Pass<Prog> {
    fn contract(&self) -> &PassContract;
    fn run(&self, ctx: &mut CompilerContext, program: &mut Prog) -> PassResult;
}
