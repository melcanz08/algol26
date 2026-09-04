// ALGOL26 - Library entry point
// Exposes compiler modules for testing and external use

pub use crate::frontend::ast::{TraitDecl, TraitMethod, ImplBlock};

pub mod common;
pub mod frontend;
pub mod compiler;
pub mod semantics;
pub mod ir;
pub mod backends;
pub mod runtime;
pub mod ffi;
