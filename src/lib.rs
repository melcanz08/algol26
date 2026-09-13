#![deny(clippy::panic)]
#![deny(clippy::unwrap_used)]
#![cfg_attr(test, allow(clippy::unwrap_used, clippy::panic))]
#![allow(clippy::expect_used)] // LLVM builder ICE
#![allow(clippy::too_many_arguments)]
#![allow(clippy::if_same_then_else)]
#![allow(clippy::should_implement_trait)]

// src/lib.rs - ALGOL26 - Library entry point
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
pub mod toolchain;
