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
// Stage 2 additionally verifies:
//   - `ArrayAssign` — array is a list, index is Int, value coerces
//     to the element type.
//   - `Call` (instruction form) — resolves to a known signature,
//     args match, result variable is bound to the return type.
//   - `MethodCall` (instruction form) — receiver is declared, args
//     verify, result is bound. Full method resolution is the
//     analyzer's responsibility.
//   - `IteratorInit` — iterable is a list; the element type is
//     recorded so `IteratorNext` can bind the loop variable with its
//     real type.
//   - `ChannelDecl`, `Send`, `Receive`, `ChannelSend`,
//     `ChannelReceive` — channel variables must have `Channel<T>`
//     type; receive targets are bound to the element type.
//   - `Allocate` — size is Int; the target is registered with the
//     declared type.
//   - `Free` — operand is `Ptr`-compatible.
//   - `Cast` — source type must `can_cast_to` the target type when
//     both are known.
//   - `MethodCall` (value form), `Array`, `Range`, `FieldAccess`
//     receive structural checks and return their claimed type.
//
// Still not verified:
//   - `Spawn`/`Fork` capture semantics (ownership transfer into a
//     spawned block).
//   - Data-flow joins at CFG merges — the current DFS uses a
//     first-visited-wins environment, not a proper fixed-point join.
//   - Absolute bounds proofs for `ArrayAccess`. Only type-level
//     checks are performed; the runtime is responsible for
//     enforcement.
//
// Built-in signatures (Math.*, String.*, File.*, List.*, alloc, free)
// are registered here rather than appearing as SemanticFunction
// entries, matching how the IR builder dispatches built-in calls.

use crate::common::types::Type;
use crate::ir::semantic_ir::{
    Instruction, SemanticBinOp, SemanticFunction, SemanticProgram, Terminator, TypedIRValue,
};
use std::collections::{HashMap, HashSet};
use builtins::{builtin_signatures, contains_type_var};
use instruction::verify_instruction;
use value::{verify_value, types_compatible_for_call};
use terminator::verify_terminator;

mod instruction;
mod value;
mod terminator;
mod builtins;
#[cfg(test)]
mod tests;

// ─────────────────────────────────────────────────────────────────────
// Public entry point
// ─────────────────────────────────────────────────────────────────────

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
pub(super) struct FunctionSignature {
    params: Vec<(String, Type)>,
    return_type: Type,
}

#[derive(Clone)]
pub(super) struct VerifyEnv {
    pub(super) variables: HashMap<String, Type>,
    pub(super) mutability: HashMap<String, bool>,
    pub(super) function_sigs: HashMap<String, FunctionSignature>,
    /// Element type of each active iterator, recorded by `IteratorInit`
    /// and consumed by `IteratorNext` to bind the loop variable with
    /// its real type instead of `Unknown`.
    pub(super) iterator_elem_types: HashMap<String, Type>,
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
            iterator_elem_types: HashMap::new(),
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
            iterator,
            target,
            body_block,
            exit_block,
            ..
        } => {
            let mut body_env = env.clone();
            // The element type was recorded by `IteratorInit` in the
            // block that precedes the loop's condition block. If it is
            // missing, fall back to `Unknown` — the analyzer will have
            // already rejected programs where the type was truly
            // unknowable.
            let elem_ty = env
                .iterator_elem_types
                .get(iterator)
                .cloned()
                .unwrap_or(Type::Unknown);
            body_env.variables.insert(target.clone(), elem_ty);
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