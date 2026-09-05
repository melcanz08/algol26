// ============================================================================
// ALGOL26 Semantic IR — CANONICAL SEMANTIC REPRESENTATION
// ============================================================================
//
// This file defines the Semantic IR (Intermediate Representation) that serves
// as the single source of truth between semantic analysis and all backends.
//
// ## WHAT THIS IR REPRESENTS
//
// A fully type-checked, borrow-checked, semantically valid ALGOL26 program.
// Every node has a known type. Every function has validated control flow.
//
// ## INVARIANTS (must always hold)
//
// 1. Backend Independence:
//    - No LLVM types, WASM types, or interpreter-specific data
//    - No target-specific lowering
//    - No register allocation
//    - No machine-specific optimizations
//
// 2. Type Correctness:
//    - Every TypedIRValue has a known Type via type_of()
//    - Every SemanticInstruction has consistent types
//    - No Type::Unknown in validated IR (except for legitimate dynamic types)
//
// 3. Control Flow Validity:
//    - Every Block has a unique id
//    - Every Jump/Branch target block exists
//    - No instructions after a terminator (Jump, Branch, Return)
//    - Entry block exists and is reachable
//
// 4. No Frontend Leakage:
//    - No AST nodes in this IR
//    - No parser tokens
//    - No source spans (for now — may be added later for diagnostics)
//    - No generic type parameters (already monomorphized)
//    - No trait bounds (already resolved)
//
// 5. Lowering Completeness:
//    - No defer statements (already lowered)
//    - No loops (already desugared)
//    - No pattern matching sugar (already resolved to Switch)
//
// ## WHAT BACKENDS MUST DO
//
// Each backend (LLVM, Interpreter, WASM) must:
// 1. Consume SemanticProgram as input
// 2. Translate each SemanticInstruction independently
// 3. Handle every TypedIRValue variant
// 4. Produce the same observable behavior as the interpreter
//
// ## WHAT BACKENDS MUST NOT DO
//
// Backends must NOT:
// 1. Import from parser.rs, lexer.rs, ast.rs, semantic.rs
// 2. Perform type checking (already done)
// 3. Perform borrow checking (already done)
// 4. Modify the IR (it's immutable input)
// 5. Assume specific target architecture details in the IR
//
// ============================================================================

#![allow(dead_code)]

use crate::common::types::Type;

// ---------------------------------------------------------------------------
// TypedIRValue — A value with a known type
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq)]
pub enum TypedIRValue {
    Int(i64),
    Float(f64),
    String(String),
    Bool(bool),
    Void,
    PtrLiteral(usize),
    NullPtr,
    List(Vec<TypedIRValue>, Type),
    Some(Box<TypedIRValue>),
    None {
        option_type: Type,
    },
    Ok {
        value: Box<TypedIRValue>,
        result_type: Type,
    },
    Error {
        value: Box<TypedIRValue>,
        result_type: Type,
    },
    Variable(String, Type),
    Cast {
        value: Box<TypedIRValue>,
        target_type: Type,
    },
    BinaryOp {
        op: SemanticBinOp,
        left: Box<TypedIRValue>,
        right: Box<TypedIRValue>,
        result_type: Type,
    },
    Call {
        function: String,
        args: Vec<TypedIRValue>,
        return_type: Type,
    },
    ArrayAccess {
        array: Box<TypedIRValue>,
        index: Box<TypedIRValue>,
        element_type: Type,
    },
    Borrow {
        expr: Box<TypedIRValue>,
        target_type: Type,
    },
    MutBorrow {
        expr: Box<TypedIRValue>,
        target_type: Type,
    },
    Deref {
        expr: Box<TypedIRValue>,
        target_type: Type,
    },
    AddrOf {
        expr: Box<TypedIRValue>,
        target_type: Type,
    },
    /// Method call on a type (trait method dispatch)
    /// receiver_type is the type that implements the trait
    /// method_name is the trait method being called
    /// The backend resolves this to "TypeName_methodName"
    MethodCall {
        receiver: Box<TypedIRValue>,
        receiver_type: Type,
        method_name: String,
        args: Vec<TypedIRValue>,
        return_type: Type,
    },
}

impl TypedIRValue {
    /// Extract a constant float value (for constant folding)
    pub fn as_constant_f64(&self) -> Option<f64> {
        match self {
            TypedIRValue::Float(f) => Some(*f),
            TypedIRValue::Int(i) => Some(*i as f64),
            _ => None,
        }
    }

    /// Get the type of this value
    /// INVARIANT: Must never return Type::Unknown for validated IR
    pub fn type_of(&self) -> Type {
        match self {
            TypedIRValue::Int(_) => Type::Int,
            TypedIRValue::Float(_) => Type::Float,
            TypedIRValue::String(_) => Type::String,
            TypedIRValue::Bool(_) => Type::Bool,
            TypedIRValue::Void => Type::Void,
            TypedIRValue::PtrLiteral(_) => Type::Ptr,
            TypedIRValue::NullPtr => Type::Ptr,
            TypedIRValue::List(_, element_type) => Type::list(element_type.clone()),
            TypedIRValue::Some(v) => Type::option(v.type_of()),
            TypedIRValue::None { option_type } => option_type.clone(),
            TypedIRValue::Ok { result_type, .. } => result_type.clone(),
            TypedIRValue::Error { result_type, .. } => result_type.clone(),
            TypedIRValue::Variable(_, t) => t.clone(),
            TypedIRValue::Cast { target_type, .. } => target_type.clone(),
            TypedIRValue::BinaryOp { result_type, .. } => result_type.clone(),
            TypedIRValue::Call { return_type, .. } => return_type.clone(),
            TypedIRValue::ArrayAccess { element_type, .. } => element_type.clone(),
            TypedIRValue::Borrow { target_type, .. } => target_type.clone(),
            TypedIRValue::MutBorrow { target_type, .. } => target_type.clone(),
            TypedIRValue::Deref { target_type, .. } => target_type.clone(),
            TypedIRValue::AddrOf { target_type, .. } => target_type.clone(),
            TypedIRValue::MethodCall { return_type, .. } => return_type.clone(),
        }
    }
}

// ---------------------------------------------------------------------------
// SemanticBinOp — Binary operators in the IR
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq)]
pub enum SemanticBinOp {
    Add,
    Subtract,
    Multiply,
    Divide,
    Greater,
    Less,
    GreaterEqual,
    LessEqual,
    Equal,
    NotEqual,
    And,
    Or,
}

// ---------------------------------------------------------------------------
// SemanticPattern — Patterns for Switch instruction
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub enum SemanticPattern {
    Some { binding: String },
    None,
    Ok { binding: String },
    Error { binding: String },
    Literal(TypedIRValue),
    Wildcard,
}

// ---------------------------------------------------------------------------
// SemanticInstruction — A single operation in the IR
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub enum SemanticInstruction {
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
    ArrayAssign {
        array: Box<TypedIRValue>,
        index: Box<TypedIRValue>,
        value: TypedIRValue,
    },
    Print {
        value: TypedIRValue,
    },
    Return {
        value: Option<TypedIRValue>,
        type_: Type,
    },
    Branch {
        condition: TypedIRValue,
        then_block: usize,
        else_block: usize,
    },
    Jump {
        block: usize,
    },
    Switch {
        value: TypedIRValue,
        cases: Vec<(SemanticPattern, usize)>,
        default_block: Option<usize>,
    },
    Call {
        result: Option<String>,
        function: String,
        args: Vec<TypedIRValue>,
        return_type: Type,
    },
    IteratorInit {
        iterator: String,
        iterable: TypedIRValue,
    },
    IteratorNext {
        iterator: String,
        target: String,
        body_block: usize,
        exit_block: usize,
    },
    Spawn {
        entry_block: usize,
    },
    Fork {
        blocks: Vec<usize>,
        join_block: usize,
    },
    Defer {
        cleanup_block: usize,
    },
    ChannelDecl {
        name: String,
        type_: Type,
    },
    Send {
        channel: String,
        value: TypedIRValue,
    },
    Receive {
        channel: String,
        target: String,
    },
    MethodCall {
        result: Option<String>,
        receiver: TypedIRValue,
        receiver_type: Type,
        method_name: String,
        args: Vec<TypedIRValue>,
        return_type: Type,
    },
}

// ---------------------------------------------------------------------------
// SemanticBlock — A basic block in the control flow graph
// ---------------------------------------------------------------------------

impl SemanticInstruction {
    /// Get successor block IDs for CFG reachability
    pub fn successors(&self) -> Vec<usize> {
        match self {
            SemanticInstruction::Jump { block } => vec![*block],
            SemanticInstruction::Branch {
                then_block,
                else_block,
                ..
            } => vec![*then_block, *else_block],
            SemanticInstruction::Switch {
                cases,
                default_block,
                ..
            } => {
                let mut succ: Vec<usize> = cases.iter().map(|(_, b)| *b).collect();
                if let Some(default) = default_block {
                    succ.push(*default);
                }
                succ
            }
            SemanticInstruction::IteratorNext {
                body_block,
                exit_block,
                ..
            } => vec![*body_block, *exit_block],
            SemanticInstruction::Spawn { entry_block } => vec![*entry_block],
            SemanticInstruction::Fork { blocks, join_block } => {
                let mut succ = blocks.clone();
                succ.push(*join_block);
                succ
            }
            SemanticInstruction::Defer { cleanup_block } => vec![*cleanup_block],
            SemanticInstruction::Return { .. } => vec![],
            _ => vec![],
        }
    }

    /// Returns true if this instruction terminates a basic block.
    /// This is the CANONICAL definition — use everywhere.
    pub fn is_terminator(&self) -> bool {
        matches!(
            self,
            SemanticInstruction::Return { .. }
                | SemanticInstruction::Jump { .. }
                | SemanticInstruction::Branch { .. }
                | SemanticInstruction::Switch { .. }
                | SemanticInstruction::IteratorNext { .. }
                | SemanticInstruction::Fork { .. }
        )
    }
}

#[derive(Debug, Clone)]
pub struct SemanticBlock {
    pub id: usize,
    pub instructions: Vec<SemanticInstruction>,
}

// ---------------------------------------------------------------------------
// SemanticFunction — A fully analyzed function
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct SemanticFunction {
    pub name: String,
    pub params: Vec<(String, Type)>,
    pub return_type: Type,
    pub blocks: Vec<SemanticBlock>,
    pub entry_block: usize,
    pub is_extern: bool,
}

// ---------------------------------------------------------------------------
// SemanticProgram — The complete IR for a program
// ---------------------------------------------------------------------------

#[derive(Debug, Default, Clone)]
pub struct SemanticProgram {
    pub functions: Vec<SemanticFunction>,
    pub next_block_id: usize,
}

impl SemanticProgram {
    pub fn new() -> Self {
        SemanticProgram {
            functions: Vec::new(),
            next_block_id: 0,
        }
    }

    pub fn new_block_id(&mut self) -> usize {
        let id = self.next_block_id;
        self.next_block_id += 1;
        id
    }

    /// Get a function by name
    pub fn get_function(&self, name: &str) -> Option<&SemanticFunction> {
        self.functions.iter().find(|f| f.name == name)
    }

    /// Get the main function (entry point)
    pub fn main_function(&self) -> Option<&SemanticFunction> {
        self.functions.iter().find(|f| f.name == "main")
    }

    /// Verify IR invariants
    /// Returns Ok(()) if all invariants hold, Err(String) otherwise
    pub fn verify(&self) -> Result<(), String> {
        // Delegate to SemanticVerifier for comprehensive verification
        crate::ir::semantic_verifier::SemanticVerifier::verify(self)
    }

    /// Verify CFG reachability - all blocks must be reachable from entry
    pub fn verify_reachability(&self) -> Result<(), String> {
        for func in &self.functions {
            if func.is_extern {
                continue;
            }

            let mut reachable = std::collections::HashSet::new();
            let mut worklist = vec![func.entry_block];

            while let Some(block_id) = worklist.pop() {
                if !reachable.insert(block_id) {
                    continue;
                }

                if let Some(block) = func.blocks.iter().find(|b| b.id == block_id) {
                    for instr in &block.instructions {
                        for succ in instr.successors() {
                            worklist.push(succ);
                        }
                    }
                }
            }

            // Check all blocks are reachable
            for block in &func.blocks {
                if !reachable.contains(&block.id) {
                    return Err(format!(
                        "Function '{}': block {} is unreachable from entry block {}",
                        func.name, block.id, func.entry_block
                    ));
                }
            }
        }
        Ok(())
    }
}

// ============================================================================
// END OF CANONICAL SEMANTIC IR
// ============================================================================
