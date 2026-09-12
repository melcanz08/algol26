// src/toolchain/mod.rs
//
// Host toolchain interface. Currently wraps `clang` for linking LLVM IR
// into native executables. Not part of the compiler core — this is the
// boundary between ALGOL26 and the system it runs on.

pub mod linker;

pub use linker::{link_llvm_ir, run_binary};