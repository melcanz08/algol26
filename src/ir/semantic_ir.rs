// src/ir/semantic_ir.rs
//
// The IR data types. The implementation is split across submodules
// under `semantic_ir/`:
//
//     values.rs        — TypedIRValue, SemanticBinOp
//     patterns.rs      — SemanticPattern
//     instructions.rs  — Instruction
//     terminators.rs   — Terminator
//     core.rs          — SemanticProgram, SemanticFunction,
//                        SemanticBlock, SemanticInstruction
//
// Every existing `use crate::ir::semantic_ir::*` continues to work
// through the re-exports below. New code can import directly from
// `crate::ir::semantic_ir::<module>` when it wants to be explicit.
//
// Original doc comment preserved for archaeology:
//
//   The IR data types: SemanticProgram, SemanticFunction,
//   SemanticBlock, Instruction, Terminator, SemanticPattern,
//   SemanticBinOp, and TypedIRValue.
//
//   `TypedIRValue::type_of()` returns the type *claimed* by the
//   value node itself. It does not prove that the claim is true —
//   that is the job of verifier. Downstream consumers should treat
//   the claimed type as authoritative only after verification
//   passes.

mod core;
mod instructions;
mod patterns;
mod terminators;
mod values;

pub use self::core::{SemanticBlock, SemanticFunction, SemanticInstruction, SemanticProgram};
pub use self::instructions::Instruction;
pub use self::patterns::SemanticPattern;
pub use self::terminators::Terminator;
pub use self::values::{SemanticBinOp, TypedIRValue};
