// src/semantic/lib.rs - ALGOL26 - Library entry point
// Exposes compiler modules for testing and external use

pub use crate::frontend::ast::{ImplBlock, TraitDecl, TraitMethod};

pub mod backends;
pub mod common;
pub mod compiler;
pub mod ffi;
pub mod frontend;
pub mod ir;
pub mod runtime;
pub mod semantics;
