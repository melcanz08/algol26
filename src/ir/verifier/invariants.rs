// src/ir/verifier/invariants.rs
//
// ADR 0014. Machine-checked invariant of executable IR: no
// `Type::TypeVar`, `Type::Generic`, or unnormalized `Type::Associated`
// appears in a function's parameters, return type, or any value
// carried by an instruction or terminator.
//
// `Type::Unknown` is deliberately *not* rejected. The analyzer uses
// it as "no opinion" inside composites — `Result<Int, Unknown>`
// means the error half is unconstrained, `List<Unknown>` means the
// element type was never narrowed. Those are valid programs;
// rejecting them here would make the verifier stricter than the
// analyzer, which is not its job. The LLVM backend's own `map_type`
// panics on `Unknown` (that is the fail-closed contract from A1),
// but only after the capability check has had a chance to refuse
// the program for unrelated reasons — e.g. `Result` is refused
// outright, so `Result<Int, Unknown>` never reaches codegen.
//
// The three variants we do reject are the ones the analyzer's
// monomorphization and ADR 0041 normalization passes are supposed
// to eliminate: if any of them survives into executable IR, the
// producing pass has a bug, and it is this verifier's job to catch
// that. `Unknown` surviving is expected.
//
// Call-target resolution is NOT checked here. `SemanticProgram::verify`
// already emits "Call to undefined function 'X'" and knows about
// builtins (List.length, Math.sqrt, alloc, free, …). Duplicating that
// check here would require duplicating the builtin whitelist, which
// is exactly the kind of coupling ADR 0014 warns against.
//
// This walker is independent of `SemanticIRBuilder` and
// `InstantiationPlan::close` by design: the verifier must not share
// traversal logic with the producers whose bugs it exists to catch.

use crate::common::types::Type;
use crate::ir::instantiation_plan::InstantiationPlan;
use crate::ir::semantic_ir::{
    Instruction, SemanticFunction, SemanticPattern, SemanticProgram, Terminator, TypedIRValue,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InvariantError {
    TypeVarInExecutableIr { function: String, location: String },
}

impl std::fmt::Display for InvariantError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            InvariantError::TypeVarInExecutableIr { function, location } => write!(
                f,
                "function `{}` contains unresolved TypeVar or Associated in executable IR ({}); the analyzer did not normalize this type (ADR 0041)",
                function, location
            ),
        }
    }
}

/// ADR 0014 invariants. The `plan` argument is accepted for symmetry
/// with the caller and to keep the entry point stable if a plan-level
/// check is added later; it is not read today.
pub fn check_invariants(
    program: &SemanticProgram,
    _plan: &InstantiationPlan,
) -> Result<(), Vec<InvariantError>> {
    let mut errors = Vec::new();

    for func in &program.functions {
        check_function(func, &mut errors);
    }

    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors)
    }
}

fn check_function(func: &SemanticFunction, errors: &mut Vec<InvariantError>) {
    for (name, ty) in &func.params {
        if is_unresolved(ty) {
            errors.push(InvariantError::TypeVarInExecutableIr {
                function: func.name.clone(),
                location: format!("parameter `{}` has type `{}`", name, ty),
            });
        }
    }
    if is_unresolved(&func.return_type) {
        errors.push(InvariantError::TypeVarInExecutableIr {
            function: func.name.clone(),
            location: format!("return type `{}`", func.return_type),
        });
    }

    for block in &func.blocks {
        for (idx, instr) in block.instructions.iter().enumerate() {
            check_instruction(instr, &func.name, block.id, idx, errors);
        }
        if let Some(term) = &block.terminator {
            check_terminator(term, &func.name, block.id, errors);
        }
    }
}

fn check_instruction(
    instr: &Instruction,
    function: &str,
    block_id: usize,
    idx: usize,
    errors: &mut Vec<InvariantError>,
) {
    let location = |suffix: &str| format!("block {} instr {}: {}", block_id, idx, suffix);

    match instr {
        Instruction::Call { args, .. } => {
            for a in args {
                check_value(a, function, &location("Call arg"), errors);
            }
        }
        Instruction::Declare { type_, value, .. } => {
            if is_unresolved(type_) {
                errors.push(InvariantError::TypeVarInExecutableIr {
                    function: function.to_string(),
                    location: location(&format!("Declare type `{}`", type_)),
                });
            }
            check_value(value, function, &location("Declare value"), errors);
        }
        Instruction::Assign { value, .. } => {
            check_value(value, function, &location("Assign"), errors);
        }
        Instruction::Print { value } => {
            check_value(value, function, &location("Print"), errors);
        }
        Instruction::ArrayAssign {
            array,
            index,
            value,
        } => {
            check_value(array, function, &location("ArrayAssign array"), errors);
            check_value(index, function, &location("ArrayAssign index"), errors);
            check_value(value, function, &location("ArrayAssign value"), errors);
        }
        Instruction::SendChannel { value, .. } => {
            check_value(value, function, &location("SendChannel"), errors);
        }
        Instruction::Allocate { size, type_, .. } => {
            if is_unresolved(type_) {
                errors.push(InvariantError::TypeVarInExecutableIr {
                    function: function.to_string(),
                    location: location(&format!("Allocate type `{}`", type_)),
                });
            }
            check_value(size, function, &location("Allocate size"), errors);
        }
        Instruction::Free { ptr } => {
            check_value(ptr, function, &location("Free"), errors);
        }
        Instruction::WriteReference { reference, value } => {
            check_value(reference, function, &location("WriteReference ref"), errors);
            check_value(value, function, &location("WriteReference value"), errors);
        }
        Instruction::FieldAssign { value, .. } => {
            check_value(value, function, &location("FieldAssign"), errors);
        }
        Instruction::IteratorInit { iterable, .. } => {
            check_value(iterable, function, &location("IteratorInit"), errors);
        }
        Instruction::ChannelDecl { type_, .. } => {
            if is_unresolved(type_) {
                errors.push(InvariantError::TypeVarInExecutableIr {
                    function: function.to_string(),
                    location: location(&format!("ChannelDecl type `{}`", type_)),
                });
            }
        }
        Instruction::ReceiveChannel { .. } => {}
        Instruction::RegionEnter { .. } | Instruction::RegionExit { .. } => {}
        Instruction::VirtualCall {
            receiver,
            args,
            return_type,
            ..
        } => {
            if is_unresolved(return_type) {
                errors.push(InvariantError::TypeVarInExecutableIr {
                    function: function.to_string(),
                    location: location(&format!("VirtualCall return type `{}`", return_type)),
                });
            }
            check_value(
                receiver,
                function,
                &location("VirtualCall receiver"),
                errors,
            );
            for a in args {
                check_value(a, function, &location("VirtualCall arg"), errors);
            }
        }
        Instruction::BoundsCheck { value, .. } => {
            check_value(value, function, &location("BoundsCheck"), errors);
        }
        Instruction::UnionConstruct { payload, .. } => {
            for p in payload {
                check_value(p, function, &location("UnionConstruct payload"), errors);
            }
        }
        Instruction::Nop => {}
    }
}

fn check_terminator(
    term: &Terminator,
    function: &str,
    block_id: usize,
    errors: &mut Vec<InvariantError>,
) {
    let location = |suffix: &str| format!("block {} terminator: {}", block_id, suffix);

    match term {
        Terminator::Return {
            value: Some(v),
            type_,
        } => {
            if is_unresolved(type_) {
                errors.push(InvariantError::TypeVarInExecutableIr {
                    function: function.to_string(),
                    location: location(&format!("Return type `{}`", type_)),
                });
            }
            check_value(v, function, &location("Return value"), errors);
        }
        Terminator::Branch { condition, .. } => {
            check_value(condition, function, &location("Branch condition"), errors);
        }
        Terminator::Switch { value, cases, .. } => {
            check_value(value, function, &location("Switch value"), errors);
            for (pat, _) in cases {
                if let SemanticPattern::Literal(v) = pat {
                    check_value(v, function, &location("Switch case literal"), errors);
                }
            }
        }
        // `Return { value: None, type_: T }` — T can carry an unresolved
        // type even without a value. Split off from the Some arm above
        // so the None case still gets its return type checked.
        Terminator::Return { value: None, type_ } => {
            if is_unresolved(type_) {
                errors.push(InvariantError::TypeVarInExecutableIr {
                    function: function.to_string(),
                    location: location(&format!("Return type `{}`", type_)),
                });
            }
        }
        Terminator::Jump { .. } => {}
        Terminator::IteratorNext { .. } => {}
        Terminator::Spawn { .. } => {}
        Terminator::Fork { .. } => {}
    }
}

/// Recursively walk a `TypedIRValue`, reporting any TypeVar that
/// appears in a carried type.
fn check_value(
    value: &TypedIRValue,
    function: &str,
    location: &str,
    errors: &mut Vec<InvariantError>,
) {
    match value {
        TypedIRValue::Call {
            args, return_type, ..
        } => {
            if is_unresolved(return_type) {
                errors.push(InvariantError::TypeVarInExecutableIr {
                    function: function.to_string(),
                    location: format!("{}: Call return type `{}`", location, return_type),
                });
            }
            for a in args {
                check_value(a, function, location, errors);
            }
        }
        TypedIRValue::BinaryOp {
            left,
            right,
            result_type,
            ..
        } => {
            if is_unresolved(result_type) {
                errors.push(InvariantError::TypeVarInExecutableIr {
                    function: function.to_string(),
                    location: format!("{}: BinaryOp result type `{}`", location, result_type),
                });
            }
            check_value(left, function, location, errors);
            check_value(right, function, location, errors);
        }
        TypedIRValue::Cast { value, target_type } => {
            if is_unresolved(target_type) {
                errors.push(InvariantError::TypeVarInExecutableIr {
                    function: function.to_string(),
                    location: format!("{}: Cast target type `{}`", location, target_type),
                });
            }
            check_value(value, function, location, errors);
        }
        TypedIRValue::Variable(_, ty) => {
            if is_unresolved(ty) {
                errors.push(InvariantError::TypeVarInExecutableIr {
                    function: function.to_string(),
                    location: format!("{}: variable of type `{}`", location, ty),
                });
            }
        }
        TypedIRValue::List(items, elem) => {
            if is_unresolved(elem) {
                errors.push(InvariantError::TypeVarInExecutableIr {
                    function: function.to_string(),
                    location: format!("{}: List element type `{}`", location, elem),
                });
            }
            for v in items {
                check_value(v, function, location, errors);
            }
        }
        TypedIRValue::BorrowShared { expr, target_type }
        | TypedIRValue::BorrowMutable { expr, target_type }
        | TypedIRValue::ReadReference { expr, target_type }
        | TypedIRValue::AddrOf { expr, target_type } => {
            if is_unresolved(target_type) {
                errors.push(InvariantError::TypeVarInExecutableIr {
                    function: function.to_string(),
                    location: format!("{}: reference type `{}`", location, target_type),
                });
            }
            check_value(expr, function, location, errors);
        }
        TypedIRValue::Some(inner) => {
            check_value(inner, function, location, errors);
        }
        TypedIRValue::None { option_type } => {
            if is_unresolved(option_type) {
                errors.push(InvariantError::TypeVarInExecutableIr {
                    function: function.to_string(),
                    location: format!("{}: None of type `{}`", location, option_type),
                });
            }
        }
        TypedIRValue::Ok { value, result_type } | TypedIRValue::Error { value, result_type } => {
            if is_unresolved(result_type) {
                errors.push(InvariantError::TypeVarInExecutableIr {
                    function: function.to_string(),
                    location: format!("{}: Result type `{}`", location, result_type),
                });
            }
            check_value(value, function, location, errors);
        }
        TypedIRValue::ArrayAccess {
            array,
            index,
            element_type,
        } => {
            if is_unresolved(element_type) {
                errors.push(InvariantError::TypeVarInExecutableIr {
                    function: function.to_string(),
                    location: format!("{}: array element type `{}`", location, element_type),
                });
            }
            check_value(array, function, location, errors);
            check_value(index, function, location, errors);
        }
        TypedIRValue::VirtualCall {
            receiver,
            args,
            return_type,
            ..
        } => {
            if is_unresolved(return_type) {
                errors.push(InvariantError::TypeVarInExecutableIr {
                    function: function.to_string(),
                    location: format!("{}: VirtualCall return type `{}`", location, return_type),
                });
            }
            check_value(receiver, function, location, errors);
            for a in args {
                check_value(a, function, location, errors);
            }
        }
        TypedIRValue::Record {
            fields,
            record_type,
            ..
        } => {
            if is_unresolved(record_type) {
                errors.push(InvariantError::TypeVarInExecutableIr {
                    function: function.to_string(),
                    location: format!("{}: Record type `{}`", location, record_type),
                });
            }
            for (_, v) in fields {
                check_value(v, function, location, errors);
            }
        }
        TypedIRValue::Map {
            entries, map_type, ..
        } => {
            if is_unresolved(map_type) {
                errors.push(InvariantError::TypeVarInExecutableIr {
                    function: function.to_string(),
                    location: format!("{}: Map type `{}`", location, map_type),
                });
            }
            for (k, v) in entries {
                check_value(k, function, location, errors);
                check_value(v, function, location, errors);
            }
        }
        TypedIRValue::Array(elements, elem_type, _) => {
            if is_unresolved(elem_type) {
                errors.push(InvariantError::TypeVarInExecutableIr {
                    function: function.to_string(),
                    location: format!("{}: Array element type `{}`", location, elem_type),
                });
            }
            for e in elements {
                check_value(e, function, location, errors);
            }
        }
        TypedIRValue::Range(start, end) => {
            check_value(start, function, location, errors);
            check_value(end, function, location, errors);
        }
        TypedIRValue::FieldAccess {
            object,
            field,
            field_type,
        } => {
            if is_unresolved(field_type) {
                errors.push(InvariantError::TypeVarInExecutableIr {
                    function: function.to_string(),
                    location: format!(
                        "{}: FieldAccess `{}` type `{}`",
                        location, field, field_type
                    ),
                });
            }
            check_value(object, function, location, errors);
        }
        TypedIRValue::Set { element_type, .. } => {
            if is_unresolved(element_type) {
                errors.push(InvariantError::TypeVarInExecutableIr {
                    function: function.to_string(),
                    location: format!("{}: Set element type `{}`", location, element_type),
                });
            }
        }
        TypedIRValue::SetSingleton {
            element,
            element_type,
        } => {
            if is_unresolved(element_type) {
                errors.push(InvariantError::TypeVarInExecutableIr {
                    function: function.to_string(),
                    location: format!("{}: SetSingleton element type `{}`", location, element_type),
                });
            }
            check_value(element, function, location, errors);
        }
        TypedIRValue::DynTrait {
            data, target_type, ..
        } => {
            if is_unresolved(target_type) {
                errors.push(InvariantError::TypeVarInExecutableIr {
                    function: function.to_string(),
                    location: format!("{}: DynTrait type `{}`", location, target_type),
                });
            }
            check_value(data, function, location, errors);
        }
        TypedIRValue::UnionConstruct {
            payload,
            target_type,
            ..
        } => {
            if is_unresolved(target_type) {
                errors.push(InvariantError::TypeVarInExecutableIr {
                    function: function.to_string(),
                    location: format!("{}: UnionConstruct type `{}`", location, target_type),
                });
            }
            for p in payload {
                check_value(p, function, location, errors);
            }
        }
        // Leaf values: no type, or a concrete scalar type, nothing to check.
        TypedIRValue::Int(_)
        | TypedIRValue::Float(_)
        | TypedIRValue::String(_)
        | TypedIRValue::Bool(_)
        | TypedIRValue::Void
        | TypedIRValue::PtrLiteral(_)
        | TypedIRValue::NullPtr => {}
    }
}

/// Structural unresolved-type detection. Duplicated from any similar
/// helper on `Type` by design — the verifier must not depend on the
/// same code the builder uses.
///
/// The three variants this rejects are the ones the analyzer is
/// supposed to eliminate before IR reaches the verifier: `TypeVar`
/// (monomorphization should have bound it), `Generic` (resolution
/// should have replaced it), and `Associated` (ADR 0041's
/// normalization pass should have rewritten it). None has a runtime
/// representation, so any of them surviving into executable IR is a
/// compiler bug — the job of this walker is to make that bug surface
/// where the diagnostic can name the function, rather than at
/// codegen, where the only available signal is a panic.
///
/// `Type::Unknown` is not in this set. See the module-level comment
/// for why.
fn is_unresolved(ty: &Type) -> bool {
    match ty {
        Type::TypeVar(_) | Type::Generic { .. } => true,
        Type::Associated { .. } => true,
        Type::List(inner)
        | Type::Option(inner)
        | Type::Pointer(inner)
        | Type::Borrow(inner)
        | Type::MutBorrow(inner)
        | Type::Channel(inner) => is_unresolved(inner),
        Type::Array(inner, _) => is_unresolved(inner),
        Type::Tuple(elems) => elems.iter().any(is_unresolved),
        Type::Result { ok, error } => is_unresolved(ok) || is_unresolved(error),
        Type::Function {
            params,
            return_type,
        } => params.iter().any(is_unresolved) || is_unresolved(return_type),
        _ => false,
    }
}
