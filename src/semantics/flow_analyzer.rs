#![allow(dead_code)]

// src/semantics/flow_analyzer.rs
//
// CFG well-formedness helpers. This module currently exposes only
// `is_terminated`. A full semantic flow analyzer — definite assignment,
// reachability of variable uses, borrow state across joins — is not
// implemented yet and must not be assumed from this API.
//
// When that work begins, this module is the right home for it. Until
// then, the surface area stays minimal so nobody trusts a no-op.

use crate::ir::semantic_ir::SemanticBlock;

pub struct FlowAnalyzer;

impl FlowAnalyzer {
    pub fn is_terminated(block: &SemanticBlock) -> bool {
        block.terminator.is_some()
    }
}