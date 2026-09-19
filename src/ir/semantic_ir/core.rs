// src/ir/semantic_ir/core.rs
//
// The container types: `SemanticProgram`, `SemanticFunction`, and
// `SemanticBlock`. Plus the `SemanticInstruction` type alias kept
// for backward compatibility (the name predates the split).

use super::instructions::Instruction;
use super::terminators::Terminator;
use crate::common::types::Type;
use std::collections::HashMap;

pub type SemanticInstruction = Instruction;

#[derive(Debug, Clone)]
pub struct SemanticBlock {
    pub id: usize,
    pub instructions: Vec<Instruction>,
    pub terminator: Option<Terminator>,
}

impl SemanticBlock {
    pub fn new(id: usize) -> Self {
        Self {
            id,
            instructions: vec![],
            terminator: None,
        }
    }
    pub fn successors(&self) -> Vec<usize> {
        if let Some(t) = &self.terminator {
            t.successors()
        } else {
            vec![]
        }
    }
    pub fn is_terminated(&self) -> bool {
        self.terminator.is_some()
    }
}

#[derive(Debug, Clone)]
pub struct SemanticFunction {
    pub name: String,
    pub params: Vec<(String, Type)>,
    pub return_type: Type,
    pub blocks: Vec<SemanticBlock>,
    pub entry_block: usize,
    pub is_extern: bool,
}

#[derive(Debug, Clone)]
pub struct SemanticProgram {
    pub functions: Vec<SemanticFunction>,
    pub block_counter: usize,
    /// Map from an extern function's ALGOL26 name to its C symbol
    /// name, when `extern ... as "sym"` was declared. Populated
    /// by the IR builder; consumed by LLVM codegen so the
    /// emitted call uses the C symbol rather than the ALGOL26
    /// name.
    pub ffi_symbols: HashMap<String, String>,
    /// Library names (without `lib` prefix or extension) that
    /// any extern declaration requested via `from "lib"`.
    /// Consumed by the linker driver as `-l<name>` flags.
    pub ffi_libraries: Vec<String>,
    /// Names of extern functions declared variadic
    /// (`extern "C" function f(a: T, ...)`). Consumed by LLVM
    /// codegen so the declared function type is variadic, and
    /// by the IR verifier to relax its arity check.
    pub variadic_functions: std::collections::HashSet<String>,
}

impl Default for SemanticProgram {
    fn default() -> Self {
        Self::new()
    }
}

impl SemanticProgram {
    pub fn new() -> Self {
        Self {
            functions: vec![],
            block_counter: 0,
            ffi_symbols: HashMap::new(),
            ffi_libraries: Vec::new(),
            variadic_functions: std::collections::HashSet::new(),
        }
    }
    pub fn new_block_id(&mut self) -> usize {
        let id = self.block_counter;
        self.block_counter += 1;
        id
    }
    pub fn verify(&self) -> Result<(), String> {
        crate::ir::verifier::verify(self)
    }
}
