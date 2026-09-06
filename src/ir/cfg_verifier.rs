#![allow(dead_code)]
use crate::ir::semantic_ir::SemanticProgram;
use std::collections::HashSet;

pub struct CFGVerifier;

impl CFGVerifier {
    pub fn verify(program: &SemanticProgram) -> Result<(), String> {
        for func in &program.functions {
            let mut ids: HashSet<usize> = HashSet::new();
            for b in &func.blocks {
                if !ids.insert(b.id) { return Err(format!("duplicate block id {}", b.id)); }
            }
            if !ids.contains(&func.entry_block) { return Err("entry block missing".into()); }
            for block in &func.blocks {
                // NEW: every block must have a terminator
                if block.terminator.is_none() {
                    return Err(format!("block {} has no terminator", block.id));
                }
                for succ in block.successors() {
                    if !ids.contains(&succ) { return Err(format!("block {} jumps to unknown block {}", block.id, succ)); }
                }
            }
        }
        Ok(())
    }
}
pub fn verify(program: &SemanticProgram) -> Result<(), String> { CFGVerifier::verify(program) }
