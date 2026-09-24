// src/ir/verifier/invariants.rs
//
// ADR 0014. Machine-checked invariant of executable IR: no
// `Type::TypeVar` appears in a function's parameters, return type,
// or any value carried by an instruction or terminator.
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
    SemanticFunction, SemanticInstruction, SemanticPattern, SemanticProgram, Terminator,
    TypedIRValue,
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
                "function `{}` contains TypeVar in executable IR ({})",
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
        if contains_type_var(ty) {
            errors.push(InvariantError::TypeVarInExecutableIr {
                function: func.name.clone(),
                location: format!("parameter `{}` has type `{}`", name, ty),
            });
        }
    }
    if contains_type_var(&func.return_type) {
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
    instr: &SemanticInstruction,
    function: &str,
    block_id: usize,
    idx: usize,
    errors: &mut Vec<InvariantError>,
) {
    let location = |suffix: &str| format!("block {} instr {}: {}", block_id, idx, suffix);

    match instr {
        SemanticInstruction::Call { args, .. } => {
            for a in args {
                check_value(a, function, &location("Call arg"), errors);
            }
        }
        SemanticInstruction::Declare { type_, value, .. } => {
            if contains_type_var(type_) {
                errors.push(InvariantError::TypeVarInExecutableIr {
                    function: function.to_string(),
                    location: location(&format!("Declare type `{}`", type_)),
                });
            }
            check_value(value, function, &location("Declare value"), errors);
        }
        SemanticInstruction::Assign { value, .. } => {
            check_value(value, function, &location("Assign"), errors);
        }
        SemanticInstruction::Print { value } => {
            check_value(value, function, &location("Print"), errors);
        }
        SemanticInstruction::ArrayAssign {
            array,
            index,
            value,
        } => {
            check_value(array, function, &location("ArrayAssign array"), errors);
            check_value(index, function, &location("ArrayAssign index"), errors);
            check_value(value, function, &location("ArrayAssign value"), errors);
        }
        SemanticInstruction::SendChannel { value, .. } => {
            check_value(value, function, &location("SendChannel"), errors);
        }
        SemanticInstruction::Allocate { size, type_, .. } => {
            if contains_type_var(type_) {
                errors.push(InvariantError::TypeVarInExecutableIr {
                    function: function.to_string(),
                    location: location(&format!("Allocate type `{}`", type_)),
                });
            }
            check_value(size, function, &location("Allocate size"), errors);
        }
        SemanticInstruction::Free { ptr } => {
            check_value(ptr, function, &location("Free"), errors);
        }
        _ => {}
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
            if contains_type_var(type_) {
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
        _ => {}
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
            if contains_type_var(return_type) {
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
            if contains_type_var(result_type) {
                errors.push(InvariantError::TypeVarInExecutableIr {
                    function: function.to_string(),
                    location: format!("{}: BinaryOp result type `{}`", location, result_type),
                });
            }
            check_value(left, function, location, errors);
            check_value(right, function, location, errors);
        }
        TypedIRValue::Cast { value, target_type } => {
            if contains_type_var(target_type) {
                errors.push(InvariantError::TypeVarInExecutableIr {
                    function: function.to_string(),
                    location: format!("{}: Cast target type `{}`", location, target_type),
                });
            }
            check_value(value, function, location, errors);
        }
        TypedIRValue::Variable(_, ty) => {
            if contains_type_var(ty) {
                errors.push(InvariantError::TypeVarInExecutableIr {
                    function: function.to_string(),
                    location: format!("{}: variable of type `{}`", location, ty),
                });
            }
        }
        TypedIRValue::List(items, elem) => {
            if contains_type_var(elem) {
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
            if contains_type_var(target_type) {
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
            if contains_type_var(option_type) {
                errors.push(InvariantError::TypeVarInExecutableIr {
                    function: function.to_string(),
                    location: format!("{}: None of type `{}`", location, option_type),
                });
            }
        }
        TypedIRValue::Ok { value, result_type } | TypedIRValue::Error { value, result_type } => {
            if contains_type_var(result_type) {
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
            if contains_type_var(element_type) {
                errors.push(InvariantError::TypeVarInExecutableIr {
                    function: function.to_string(),
                    location: format!("{}: array element type `{}`", location, element_type),
                });
            }
            check_value(array, function, location, errors);
            check_value(index, function, location, errors);
        }
        _ => {}
    }
}

/// Structural `TypeVar` detection. Duplicated from any similar helper
/// on `Type` by design — the verifier must not depend on the same
/// code the builder uses.
fn contains_type_var(ty: &Type) -> bool {
    match ty {
        Type::TypeVar(_) => true,
        Type::List(inner)
        | Type::Option(inner)
        | Type::Pointer(inner)
        | Type::Borrow(inner)
        | Type::MutBorrow(inner)
        | Type::Channel(inner) => contains_type_var(inner),
        Type::Array(inner, _) => contains_type_var(inner),
        Type::Tuple(elems) => elems.iter().any(contains_type_var),
        Type::Result { ok, error } => contains_type_var(ok) || contains_type_var(error),
        Type::Function {
            params,
            return_type,
        } => params.iter().any(contains_type_var) || contains_type_var(return_type),
        Type::Generic { args, .. } => args.iter().any(contains_type_var),
        _ => false,
    }
}
