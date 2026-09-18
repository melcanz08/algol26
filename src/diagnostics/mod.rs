// src/diagnostics/mod.rs

//! Structured diagnostic rendering.
//!
//! `common::diagnostics` defines *what* a diagnostic is
//! (`CompileError`, `ErrorCode`, `Diagnostic`). This module defines
//! *how* one is presented to a user: codes, spans, carets, notes,
//! and the batch summary line.

pub mod renderer;
