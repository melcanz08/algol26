// src/ir/semantic_verifier.rs
//
// Instruction-level semantic verification of a SemanticProgram. Runs
// after the structural CFG check (cfg_verifier.rs) and rejects IR that
// would be well-formed but semantically invalid.
//
// Coverage:
//   - `Declare` — value type must coerce to the declared type.
//     `Void` is accepted as an uninitialized placeholder; the later
//     `Assign` (or branch assignment) is where the real type is
//     checked. This is what allows `__result_N`-style variables to be
//     declared in a parent block and assigned in a branch.
//   - `Assign` — target must be declared and mutable; assigned value
//     must be compatible with the target's declared type. Uses the
//     same wildcard-aware compatibility check as `Call`, so partial
//     types (e.g. `Result<Unknown, Unknown>`) match concrete ones.
//   - `Print` — value must verify.
//   - `Return` — value must coerce to the function's return type.
//   - `Branch` — condition must be Bool.
//   - `Switch` — switch value must not be Void; `Ok`/`Error`/`Some`
//     pattern bindings are introduced into the environment of the
//     target block only.
//   - `IteratorNext` — loop variable is bound in the body block only.
//   - `Call` — function must be known (user-defined or built-in), arg
//     count must match, arg types must be compatible with parameters,
//     claimed return type must match the signature.
//
// Recursive value verification:
//   `Variable`, `List`, `BinaryOp`, `Cast`, `ArrayAccess`, `Borrow`,
//   `MutBorrow`, `Deref`, `AddrOf`, `Call`. Each verifies its operands
//   and cross-checks its self-described type against the computed type.
//
// Not yet verified:
//   - `ArrayAssign`, `IteratorInit`, `Send`/`Receive`, `ChannelDecl`,
//     `Allocate`, `Free`.
//   - `Cast` legality (only structural recursion; a String→Int cast
//     is not rejected here).
//   - `Spawn`/`Fork` capture semantics.
//   - Data-flow joins at CFG merges — the current DFS uses a
//     first-visited-wins environment, not a proper fixed-point join.
//     This is sufficient to catch the common bugs but not complete.
//   - Absolute bounds proofs for `ArrayAccess`. Only type-level checks
//     are performed; the runtime (LLVM and interpreter) is responsible
//     for bounds enforcement.
//
// Built-in signatures (Math.*, String.*, File.*, List.*, alloc, free)
// are registered here rather than appearing as SemanticFunction
// entries, matching how the IR builder dispatches built-in calls.

use crate::common::types::Type;
use crate::ir::semantic_ir::{
    Instruction, SemanticBinOp, SemanticFunction, SemanticProgram, Terminator, TypedIRValue,
};
use std::collections::{HashMap, HashSet};

// ─────────────────────────────────────────────────────────────────────
// Public entry point
// ─────────────────────────────────────────────────────────────────────

/// Signatures for built-in functions that the IR builder registers
/// but that do not appear as `SemanticFunction` entries in the program.
/// Keep this in sync with `SemanticIRBuilder::build_impl`.
fn builtin_signatures() -> HashMap<String, FunctionSignature> {
    let mut m = HashMap::new();

    let mut reg = |name: &str, params: Vec<(&str, Type)>, ret: Type| {
        m.insert(
            name.to_string(),
            FunctionSignature {
                params: params.into_iter().map(|(n, t)| (n.to_string(), t)).collect(),
                return_type: ret,
            },
        );
    };

    // Math
    for name in &["Math.sqrt", "Math.sin", "Math.cos", "Math.tan",
                  "Math.abs", "Math.floor", "Math.ceil", "Math.exp", "Math.log"] {
        reg(name, vec![("x", Type::Float)], Type::Float);
    }
    reg("Math.pow", vec![("x", Type::Float), ("y", Type::Float)], Type::Float);

    // String
    reg("String.length",    vec![("s", Type::String)], Type::Int);
    reg("String.concat",    vec![("s1", Type::String), ("s2", Type::String)], Type::String);
    reg("String.substring", vec![("s", Type::String), ("start", Type::Int), ("length", Type::Int)], Type::String);
    reg("String.to_upper",  vec![("s", Type::String)], Type::String);
    reg("String.to_lower",  vec![("s", Type::String)], Type::String);

    // File
    reg("File.read",   vec![("path", Type::String)], Type::String);
    reg("File.write",  vec![("path", Type::String), ("content", Type::String)], Type::Int);
    reg("File.append", vec![("path", Type::String), ("content", Type::String)], Type::Int);

    // List
    reg("List.length", vec![("arr", Type::list(Type::Unknown))], Type::Int);
    reg("List.sum",    vec![("arr", Type::list(Type::Unknown))], Type::Float);
    reg("List.max",    vec![("arr", Type::list(Type::Unknown))], Type::Float);
    reg("List.min",    vec![("arr", Type::list(Type::Unknown))], Type::Float);

    // Raw memory
    reg("alloc", vec![("size", Type::Int)], Type::pointer(Type::Unknown));
    reg("free",  vec![("ptr", Type::pointer(Type::Unknown))], Type::Void);

    m
}

pub fn verify(program: &SemanticProgram) -> Result<(), String> {
    // 1. Structural CFG check (unchanged).
    crate::ir::cfg_verifier::verify(program)?;

    // 2. Collect all function signatures up front so `Call` nodes can be
    //    verified against known parameter and return types.
    //
    //    Start with the built-in table — user-defined functions can
    //    shadow them (though the analyzer prevents that), so we insert
    //    user signatures second.
    let mut signatures: HashMap<String, FunctionSignature> = builtin_signatures();
    for func in &program.functions {
        signatures.insert(
            func.name.clone(),
            FunctionSignature {
                params: func.params.clone(),
                return_type: func.return_type.clone(),
            },
        );
    }

    // 3. Per-function semantic checks.
    for func in &program.functions {
        verify_function(func, &signatures)?;
    }

    Ok(())
}

// ─────────────────────────────────────────────────────────────────────
// Environment
// ─────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
struct FunctionSignature {
    params: Vec<(String, Type)>,
    return_type: Type,
}

#[derive(Clone)]
struct VerifyEnv {
    variables: HashMap<String, Type>,
    mutability: HashMap<String, bool>,
    function_sigs: HashMap<String, FunctionSignature>,
}

impl VerifyEnv {
    fn new_for(
        func: &SemanticFunction,
        sigs: &HashMap<String, FunctionSignature>,
    ) -> Self {
        let mut variables = HashMap::new();
        let mut mutability = HashMap::new();
        for (name, ty) in &func.params {
            variables.insert(name.clone(), ty.clone());
            mutability.insert(name.clone(), true);
        }
        VerifyEnv {
            variables,
            mutability,
            function_sigs: sigs.clone(),
        }
    }
}

// ─────────────────────────────────────────────────────────────────────
// Function verification
// ─────────────────────────────────────────────────────────────────────

fn verify_function(
    func: &SemanticFunction,
    sigs: &HashMap<String, FunctionSignature>,
) -> Result<(), String> {
    // Extern functions have no body to verify.
    if func.is_extern {
        return Ok(());
    }

    if func.blocks.is_empty() {
        return Err(format!("Function '{}' has no blocks", func.name));
    }

    if func.return_type != Type::Void {
        verify_return_paths(func)?;
    }

    let env = VerifyEnv::new_for(func, sigs);
    let mut visited: HashSet<usize> = HashSet::new();

    verify_block_dfs(func, func.entry_block, env, &mut visited)
}

/// Compute each successor of a terminator along with the environment
/// that successor should be verified in.
///
/// Most terminators pass the current env through unchanged. `Switch`
/// introduces pattern bindings that are only visible in the successor
/// block the pattern targets — `Ok(v)` binds `v` in the ok branch,
/// `Error(e)` binds `e` in the error branch. `IteratorNext` similarly
/// binds the loop variable in the body.
fn successors_with_envs(
    term: &Terminator,
    env: &VerifyEnv,
) -> Vec<(usize, VerifyEnv)> {
    use crate::ir::semantic_ir::SemanticPattern;

    match term {
        Terminator::Switch {
            cases,
            default_block,
            ..
        } => {
            let mut result = Vec::new();
            for (pattern, target) in cases {
                let mut succ_env = env.clone();
                let binding = match pattern {
                    SemanticPattern::Some { binding }
                    | SemanticPattern::Ok { binding }
                    | SemanticPattern::Error { binding } => Some(binding),
                    _ => None,
                };
                if let Some(name) = binding {
                    // The pattern introduces `name` in the target block
                    // only. Its type is Unknown here — the switch
                    // runtime is responsible for supplying a value of
                    // the correct shape.
                    succ_env.variables.insert(name.clone(), Type::Unknown);
                    succ_env.mutability.insert(name.clone(), false);
                }
                result.push((*target, succ_env));
            }
            if let Some(default) = default_block {
                result.push((*default, env.clone()));
            }
            result
        }

        Terminator::IteratorNext {
            target,
            body_block,
            exit_block,
            ..
        } => {
            let mut body_env = env.clone();
            body_env.variables.insert(target.clone(), Type::Unknown);
            body_env.mutability.insert(target.clone(), false);
            vec![(*body_block, body_env), (*exit_block, env.clone())]
        }

        _ => term
            .successors()
            .into_iter()
            .map(|s| (s, env.clone()))
            .collect(),
    }
}

fn verify_block_dfs(
    func: &SemanticFunction,
    block_id: usize,
    mut env: VerifyEnv,
    visited: &mut HashSet<usize>,
) -> Result<(), String> {
    if !visited.insert(block_id) {
        // Already verified on a previous DFS path. We don't re-verify
        // with the current env — this is the linear-scan approximation.
        return Ok(());
    }

    let block = func
        .blocks
        .iter()
        .find(|b| b.id == block_id)
        .ok_or_else(|| format!("Function '{}': block {} not found", func.name, block_id))?;

    for instr in &block.instructions {
        verify_instruction(func, instr, &mut env)?;
    }

    let term = block
        .terminator
        .as_ref()
        .ok_or_else(|| format!("Function '{}': block {} has no terminator", func.name, block_id))?;

    verify_terminator(func, term, &mut env)?;

    // Recurse into successors. Each successor sees a clone of the env so
    // modifications in one branch don't leak into a sibling branch.
    for (succ, succ_env) in successors_with_envs(term, &env) {
        verify_block_dfs(func, succ, succ_env, visited)?;
    }

    Ok(())
}

fn verify_return_paths(func: &SemanticFunction) -> Result<(), String> {
    for block in &func.blocks {
        if block.terminator.is_none() {
            return Err(format!(
                "Function '{}' has block {} with no terminator",
                func.name, block.id
            ));
        }
    }

    let has_return = func
        .blocks
        .iter()
        .any(|b| matches!(b.terminator, Some(Terminator::Return { .. })));

    if !has_return {
        return Err(format!("Function '{}' has no return statement", func.name));
    }

    Ok(())
}

// ─────────────────────────────────────────────────────────────────────
// Instruction verification
// ─────────────────────────────────────────────────────────────────────

fn verify_instruction(
    func: &SemanticFunction,
    instr: &Instruction,
    env: &mut VerifyEnv,
) -> Result<(), String> {
    match instr {
        Instruction::Nop => Ok(()),

        Instruction::Declare {
            name,
            type_,
            value,
            mutable,
        } => {
            let value_ty = verify_value(value, env)?;

            // `Void` is a legitimate placeholder for variables that are
            // declared in one block and assigned in another — the
            // canonical example being the result variable of a
            // value-producing if/match/try. The eventual `Assign`
            // instruction is where the type is really checked.
            //
            // Every other value is checked against the declared type.
            if !matches!(value, TypedIRValue::Void)
                && *type_ != Type::Unknown
                && value_ty != Type::Unknown
                && !value_ty.can_coerce_to(type_)
                && value_ty != *type_
            {
                return Err(format!(
                    "Function '{}': Declare '{}' as {:?} but value has type {:?}",
                    func.name, name, type_, value_ty
                ));
            }

            env.variables.insert(name.clone(), type_.clone());
            env.mutability.insert(name.clone(), *mutable);
            Ok(())
        }

        Instruction::Assign { target, value } => {
            let target_ty = env
                .variables
                .get(target)
                .ok_or_else(|| {
                    format!(
                        "Function '{}': Assign to undefined variable '{}'",
                        func.name, target
                    )
                })?
                .clone();

            if !env.mutability.get(target).copied().unwrap_or(false) {
                return Err(format!(
                    "Function '{}': Assign to immutable variable '{}'",
                    func.name, target
                ));
            }

            let value_ty = verify_value(value, env)?;

            // Assigning through a mut-borrow writes the inner type.
            let expected = match &target_ty {
                Type::MutBorrow(inner) => (**inner).clone(),
                _ => target_ty.clone(),
            };

            // Use the same recursive compatibility check as Call: treat
            // Unknown (at the top level or nested inside a composite) as
            // a wildcard. This is what allows `__result_N` / `__try_value_N`
            // placeholder variables to be declared with partially-unknown
            // types and assigned concrete ones in their branches.
            if !types_compatible_for_call(&value_ty, &expected) {
                return Err(format!(
                    "Function '{}': Assign to '{}' expected {:?}, found {:?}",
                    func.name, target, expected, value_ty
                ));
            }

            Ok(())
        }

        Instruction::Print { value } => {
            verify_value(value, env)?;
            Ok(())
        }

        // Other instructions are verified in later stages.
        _ => Ok(()),
    }
}

// ─────────────────────────────────────────────────────────────────────
// Value verification
// ─────────────────────────────────────────────────────────────────────

fn verify_value(value: &TypedIRValue, env: &VerifyEnv) -> Result<Type, String> {
    Ok(match value {
        TypedIRValue::Int(_) => Type::Int,
        TypedIRValue::Float(_) => Type::Float,
        TypedIRValue::Bool(_) => Type::Bool,
        TypedIRValue::String(_) => Type::String,
        TypedIRValue::Void => Type::Void,
        TypedIRValue::NullPtr => Type::Ptr,
        TypedIRValue::PtrLiteral(_) => Type::Ptr,

        TypedIRValue::Variable(name, claimed) => {
            let actual = env
                .variables
                .get(name)
                .ok_or_else(|| format!("Use of undefined variable '{}' in verified IR", name))?
                .clone();

            if !claimed.is_unknown() && !actual.is_unknown() && claimed != &actual {
                return Err(format!(
                    "Variable '{}' claimed type {:?} but declared as {:?}",
                    name, claimed, actual
                ));
            }
            actual
        }

        TypedIRValue::List(elements, claimed_elem) => {
            let mut common: Option<Type> = None;
            for elem in elements {
                let t = verify_value(elem, env)?;
                common = Some(match common {
                    None => t,
                    Some(prev) => prev.common_supertype(&t),
                });
            }
            let elem_ty = common.unwrap_or(Type::Unknown);
            if !claimed_elem.is_unknown()
                && !elem_ty.is_unknown()
                && claimed_elem != &elem_ty
            {
                return Err(format!(
                    "List claims element type {:?} but elements imply {:?}",
                    claimed_elem, elem_ty
                ));
            }
            Type::list(elem_ty)
        }

        TypedIRValue::BinaryOp {
            op,
            left,
            right,
            result_type,
        } => {
            let lt = verify_value(left, env)?;
            let rt = verify_value(right, env)?;
            let expected = compute_binop_type(op, &lt, &rt)?;

            if !result_type.is_unknown()
                && !expected.is_unknown()
                && result_type != &expected
            {
                return Err(format!(
                    "BinaryOp claims result {:?} but {:?} {:?} {:?} implies {:?}",
                    result_type, lt, op, rt, expected
                ));
            }
            expected
        }

        TypedIRValue::Cast { value, target_type } => {
            // Stage 1: recurse for structural soundness, but don't enforce
            // cast legality yet. That's Stage 3 work.
            let _ = verify_value(value, env)?;
            target_type.clone()
        }

        TypedIRValue::ArrayAccess {
            array,
            index,
            element_type,
        } => {
            let arr_ty = verify_value(array, env)?;
            let idx_ty = verify_value(index, env)?;

            if !idx_ty.is_unknown() && idx_ty != Type::Int {
                return Err(format!("Array index must be Int, found {:?}", idx_ty));
            }

            let elem = match arr_ty {
                Type::List(t) => *t,
                Type::Array(t, _) => *t,
                Type::Unknown => Type::Unknown,
                other => {
                    return Err(format!("Array access on non-list type {:?}", other))
                }
            };

            if !element_type.is_unknown() && !elem.is_unknown() && element_type != &elem {
                return Err(format!(
                    "ArrayAccess claims element {:?} but array yields {:?}",
                    element_type, elem
                ));
            }
            elem
        }

        TypedIRValue::Borrow { expr, target_type } => {
            let inner = verify_value(expr, env)?;
            let expected = Type::borrow(inner);
            if !target_type.is_unknown() && !expected.is_unknown() && target_type != &expected {
                return Err(format!(
                    "Borrow claims {:?} but operand is {:?}",
                    target_type, expected
                ));
            }
            expected
        }

        TypedIRValue::MutBorrow { expr, target_type } => {
            let inner = verify_value(expr, env)?;
            let expected = Type::mut_borrow(inner);
            if !target_type.is_unknown() && !expected.is_unknown() && target_type != &expected {
                return Err(format!(
                    "MutBorrow claims {:?} but operand is {:?}",
                    target_type, expected
                ));
            }
            expected
        }

        TypedIRValue::Deref { expr, target_type } => {
            let inner = verify_value(expr, env)?;
            let expected = match inner {
                Type::Pointer(t) | Type::Borrow(t) | Type::MutBorrow(t) => *t,
                Type::Unknown => Type::Unknown,
                other => {
                    return Err(format!("Deref on non-pointer type {:?}", other))
                }
            };
            if !target_type.is_unknown() && !expected.is_unknown() && target_type != &expected {
                return Err(format!(
                    "Deref claims {:?} but pointer points to {:?}",
                    target_type, expected
                ));
            }
            expected
        }

        TypedIRValue::AddrOf { expr, target_type } => {
            let inner = verify_value(expr, env)?;
            let expected = Type::pointer(inner);
            if !target_type.is_unknown() && !expected.is_unknown() && target_type != &expected {
                return Err(format!(
                    "AddrOf claims {:?} but expected {:?}",
                    target_type, expected
                ));
            }
            expected
        }

        TypedIRValue::Call {
            function,
            args,
            return_type,
        } => {
            let arg_types: Result<Vec<_>, _> =
                args.iter().map(|a| verify_value(a, env)).collect();
            let arg_types = arg_types?;

            let sig = env.function_sigs.get(function).ok_or_else(|| {
                format!("Call to undefined function '{}' in verified IR", function)
            })?;

            if arg_types.len() != sig.params.len() {
                return Err(format!(
                    "Call to '{}' expects {} args, found {}",
                    function,
                    sig.params.len(),
                    arg_types.len()
                ));
            }

            for (i, ((_, param_ty), arg_ty)) in
                sig.params.iter().zip(&arg_types).enumerate()
            {
                if arg_ty.is_unknown() || param_ty.is_unknown() {
                    continue;
                }
                // `List<Unknown>` (and similar) act as wildcards for built-ins
                // such as List.length. Treat a parameter type that is a
                // composite containing Unknown as matching any instantiation.
                if types_compatible_for_call(arg_ty, param_ty) {
                    continue;
                }
                return Err(format!(
                    "Call to '{}' arg {} type mismatch: expected {:?}, found {:?}",
                    function, i, param_ty, arg_ty
                ));
            }

            if !return_type.is_unknown()
                && !sig.return_type.is_unknown()
                && return_type != &sig.return_type
            {
                return Err(format!(
                    "Call to '{}' claims return {:?} but signature says {:?}",
                    function, return_type, sig.return_type
                ));
            }
            sig.return_type.clone()
        }

        // Variants verified in later stages fall through with their
        // self-described type. This preserves the current behavior for
        // those nodes while the verifier is grown incrementally.
        _ => value.type_of(),
    })
}

fn compute_binop_type(op: &SemanticBinOp, lt: &Type, rt: &Type) -> Result<Type, String> {
    match op {
        SemanticBinOp::Add => {
            if *lt == Type::String && *rt == Type::String {
                Ok(Type::String)
            } else if lt.is_numeric() && rt.is_numeric() {
                Ok(lt.common_supertype(rt))
            } else if lt.is_unknown() || rt.is_unknown() {
                Ok(Type::Unknown)
            } else {
                Err(format!(
                    "Add: incompatible operand types {:?} and {:?}",
                    lt, rt
                ))
            }
        }
        SemanticBinOp::Subtract
        | SemanticBinOp::Multiply
        | SemanticBinOp::Divide => {
            if lt.is_numeric() && rt.is_numeric() {
                Ok(lt.common_supertype(rt))
            } else if lt.is_unknown() || rt.is_unknown() {
                Ok(Type::Unknown)
            } else {
                Err(format!(
                    "Arithmetic: incompatible operand types {:?} and {:?}",
                    lt, rt
                ))
            }
        }
        SemanticBinOp::Greater
        | SemanticBinOp::Less
        | SemanticBinOp::GreaterEqual
        | SemanticBinOp::LessEqual => {
            if (lt.is_numeric() && rt.is_numeric()) || lt.is_unknown() || rt.is_unknown() {
                Ok(Type::Bool)
            } else {
                Err(format!(
                    "Comparison: incompatible operand types {:?} and {:?}",
                    lt, rt
                ))
            }
        }
        SemanticBinOp::Equal | SemanticBinOp::NotEqual => Ok(Type::Bool),
        SemanticBinOp::And | SemanticBinOp::Or => Ok(Type::Bool),
    }
}

/// True if `arg_ty` is acceptable for a parameter of type `param_ty`.
///
/// Differs from `can_coerce_to` in two ways:
/// 1. `Unknown` inside a composite parameter (e.g. `List<Unknown>` from
///    a built-in signature) matches any concrete instantiation.
/// 2. Numeric coercions (Int→Float) are permitted as before.
fn types_compatible_for_call(arg_ty: &Type, param_ty: &Type) -> bool {
    // Numeric coercion.
    if arg_ty.is_numeric() && param_ty.is_numeric() {
        return true;
    }

    // Exact match.
    if arg_ty == param_ty {
        return true;
    }

    // Structural match with Unknown-as-wildcard.
    match (arg_ty, param_ty) {
        (_, Type::Unknown) | (Type::Unknown, _) => true,
        (Type::List(a), Type::List(p)) => types_compatible_for_call(a, p),
        (Type::Array(a, _), Type::Array(p, _)) => types_compatible_for_call(a, p),
        (Type::Option(a), Type::Option(p)) => types_compatible_for_call(a, p),
        (Type::Result { ok: ao, error: ae }, Type::Result { ok: po, error: pe }) => {
            types_compatible_for_call(ao, po) && types_compatible_for_call(ae, pe)
        }
        (Type::Pointer(a), Type::Pointer(p)) => types_compatible_for_call(a, p),
        (Type::Borrow(a), Type::Borrow(p)) => types_compatible_for_call(a, p),
        (Type::MutBorrow(a), Type::MutBorrow(p)) => types_compatible_for_call(a, p),
        (Type::Channel(a), Type::Channel(p)) => types_compatible_for_call(a, p),
        _ => arg_ty.can_coerce_to(param_ty),
    }
}

// ─────────────────────────────────────────────────────────────────────
// Terminator verification
// ─────────────────────────────────────────────────────────────────────

fn verify_terminator(
    func: &SemanticFunction,
    terminator: &Terminator,
    env: &mut VerifyEnv,
) -> Result<(), String> {
    match terminator {
        Terminator::Return { value, type_ } => {
            if let Some(val) = value {
                let actual_type = verify_value(val, env)?;
                if !actual_type.is_unknown()
                    && !type_.is_unknown()
                    && actual_type != *type_
                    && !actual_type.can_coerce_to(type_)
                {
                    return Err(format!(
                        "Function '{}' return type mismatch: expected {:?}, found {:?}",
                        func.name, type_, actual_type
                    ));
                }
            }

            if !type_.is_unknown()
                && !func.return_type.is_unknown()
                && *type_ != func.return_type
            {
                return Err(format!(
                    "Function '{}' declared return type {:?} but returns {:?}",
                    func.name, func.return_type, type_
                ));
            }
            Ok(())
        }

        Terminator::Branch { condition, .. } => {
            let cond_ty = verify_value(condition, env)?;
            if !cond_ty.is_unknown() && cond_ty != Type::Bool {
                return Err(format!(
                    "Function '{}' branch condition must be Bool, found {:?}",
                    func.name, cond_ty
                ));
            }
            Ok(())
        }

        Terminator::Switch { value, .. } => {
            let value_type = verify_value(value, env)?;
            if value_type == Type::Void {
                return Err(format!(
                    "Function '{}' switch value cannot be Void",
                    func.name
                ));
            }
            Ok(())
        }

        Terminator::IteratorNext { target, .. } => {
            // The iterator target is declared by this terminator. Insert
            // it with Unknown type; later stages can recover the element
            // type from IteratorInit.
            env.variables.insert(target.clone(), Type::Unknown);
            env.mutability.insert(target.clone(), false);
            Ok(())
        }

        _ => Ok(()),
    }
}

// ─────────────────────────────────────────────────────────────────────
// Tests
// ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ir::semantic_ir::{SemanticBlock, SemanticFunction, SemanticProgram, Terminator};

    fn single_block_program(
        instructions: Vec<Instruction>,
        terminator: Option<Terminator>,
    ) -> SemanticProgram {
        let mut program = SemanticProgram::new();
        let entry = program.new_block_id();
        program.functions.push(SemanticFunction {
            name: "main".to_string(),
            params: vec![],
            return_type: Type::Void,
            blocks: vec![SemanticBlock {
                id: entry,
                instructions,
                terminator: Some(terminator.unwrap_or(Terminator::Return {
                    value: None,
                    type_: Type::Void,
                })),
            }],
            entry_block: entry,
            is_extern: false,
        });
        program
    }

    #[test]
    fn test_missing_return_detected() {
        let mut program = SemanticProgram::new();
        let entry = program.new_block_id();
        program.functions.push(SemanticFunction {
            name: "main".to_string(),
            params: vec![],
            return_type: Type::Int,
            blocks: vec![SemanticBlock {
                id: entry,
                instructions: vec![],
                terminator: Some(Terminator::Jump { block: entry }),
            }],
            entry_block: entry,
            is_extern: false,
        });
        assert!(verify(&program).is_err());
    }

    #[test]
    fn test_return_type_mismatch() {
        let mut program = SemanticProgram::new();
        let entry = program.new_block_id();
        program.functions.push(SemanticFunction {
            name: "main".to_string(),
            params: vec![],
            return_type: Type::Int,
            blocks: vec![SemanticBlock {
                id: entry,
                instructions: vec![],
                terminator: Some(Terminator::Return {
                    value: Some(TypedIRValue::String("wrong".to_string())),
                    type_: Type::String,
                }),
            }],
            entry_block: entry,
            is_extern: false,
        });
        assert!(verify(&program).is_err());
    }

    // ─── Stage 1: instruction-level checks ───

    #[test]
    fn verifier_rejects_declare_type_mismatch() {
        let program = single_block_program(
            vec![Instruction::Declare {
                name: "x".to_string(),
                mutable: true,
                type_: Type::Int,
                value: TypedIRValue::String("hello".to_string()),
            }],
            None,
        );
        let result = verify(&program);
        assert!(result.is_err(), "expected type mismatch, got: {:?}", result);
    }

    #[test]
    fn verifier_rejects_assign_to_immutable() {
        let program = single_block_program(
            vec![
                Instruction::Declare {
                    name: "x".to_string(),
                    mutable: false,
                    type_: Type::Int,
                    value: TypedIRValue::Int(0),
                },
                Instruction::Assign {
                    target: "x".to_string(),
                    value: TypedIRValue::Int(1),
                },
            ],
            None,
        );
        assert!(verify(&program).is_err());
    }

    #[test]
    fn verifier_rejects_undefined_variable_read() {
        let program = single_block_program(
            vec![Instruction::Print {
                value: TypedIRValue::Variable("missing".to_string(), Type::Int),
            }],
            None,
        );
        assert!(verify(&program).is_err());
    }

    #[test]
    fn verifier_rejects_binary_op_type_mismatch() {
        let program = single_block_program(
            vec![
                Instruction::Declare {
                    name: "x".to_string(),
                    mutable: true,
                    type_: Type::Int,
                    value: TypedIRValue::Int(0),
                },
                Instruction::Declare {
                    name: "y".to_string(),
                    mutable: true,
                    type_: Type::Int,
                    value: TypedIRValue::BinaryOp {
                        op: SemanticBinOp::Add,
                        left: Box::new(TypedIRValue::Variable("x".into(), Type::Int)),
                        right: Box::new(TypedIRValue::String("oops".into())),
                        result_type: Type::Int,
                    },
                },
            ],
            None,
        );
        assert!(verify(&program).is_err());
    }

    #[test]
    fn verifier_accepts_valid_declare() {
        let program = single_block_program(
            vec![Instruction::Declare {
                name: "x".to_string(),
                mutable: true,
                type_: Type::Float,
                value: TypedIRValue::Float(42.0),
            }],
            None,
        );
        assert!(verify(&program).is_ok());
    }

    #[test]
    fn verifier_accepts_int_to_float_coercion() {
        let program = single_block_program(
            vec![Instruction::Declare {
                name: "x".to_string(),
                mutable: true,
                type_: Type::Float,
                value: TypedIRValue::Int(5),
            }],
            None,
        );
        assert!(verify(&program).is_ok());
    }
}