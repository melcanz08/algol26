// src/ir/semantic_ir/instructions.rs
//
// `Instruction` — a single non-terminator IR instruction.
// Terminators live in `terminators.rs`.

use super::values::TypedIRValue;
use crate::common::types::Type;

#[derive(Debug, Clone)]
pub enum Instruction {
    Nop,
    Declare {
        name: String,
        mutable: bool,
        type_: Type,
        value: TypedIRValue,
    },
    Assign {
        target: String,
        value: TypedIRValue,
    },
    /// Write through a `MutBorrow(T)` reference variable: load the
    /// pointer the reference holds, then store `value` at the
    /// pointed-to location. This is the canonical form of ALGOL26's
    /// write-through syntax (`p := v` where `p: MutBorrow(T)`).
    ///
    /// Before this variant existed, the IR builder emitted a plain
    /// `Assign` for such statements. Both backends then stored the
    /// value into the reference variable's own slot — overwriting
    /// the pointer — instead of writing through it. See ADR 0010
    /// Phase 2 for the discovery and the fix plan.
    WriteReference {
        reference: TypedIRValue,
        value: TypedIRValue,
    },
    ArrayAssign {
        array: Box<TypedIRValue>,
        index: Box<TypedIRValue>,
        value: TypedIRValue,
    },
    Print {
        value: TypedIRValue,
    },
    Call {
        func: String,
        args: Vec<TypedIRValue>,
        result: Option<String>,
    },
    IteratorInit {
        iterator: String,
        iterable: TypedIRValue,
    },
    ChannelDecl {
        name: String,
        type_: Type,
    },
    SendChannel {
        channel: String,
        value: TypedIRValue,
    },
    ReceiveChannel {
        channel: String,
        target: String,
    },
    Allocate {
        target: String,
        size: TypedIRValue,
        type_: Type,
    },
    Free {
        ptr: TypedIRValue,
    },

    /// Enter a `region NAME` block. The interpreter pushes a new
    /// region frame; allocations between `RegionEnter` and the
    /// matching `RegionExit` are attributed to it and freed
    /// automatically on exit. LLVM treats both as no-ops — the
    /// capability check refuses any program that actually allocs,
    /// so a region without alloc has no runtime meaning.
    RegionEnter {
        name: String,
    },
    /// Exit a `region NAME` block.
    RegionExit {
        name: String,
    },
}
