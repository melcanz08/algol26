// src/ir/cfg/builder.rs - v0.9-I EXHAUSTIVE
// E: CFG foundation, F: ownership/region/escape + Use tracking,
// H: no silent fallbacks, I: exhaustive match (compile-time guard)
//
// Note on fail-closed design: the match below is exhaustive —
// every Instruction variant has an explicit arm. Adding a new
// variant to `Instruction` without adding an arm here is a
// compile error. This is stronger than a `_ => Unsupported`
// catch-all because it fails at build time, not runtime.

use super::dataflow::{Cfg, CfgBlock, CfgInstruction, FunctionCfg};
use crate::ir::semantic_ir::{SemanticProgram, Terminator, TypedIRValue};
use std::collections::HashSet;

fn extract_var_name(v: &TypedIRValue) -> Option<String> {
    match v {
        TypedIRValue::Variable(name, _) => Some(name.clone()),
        TypedIRValue::BorrowShared { expr, .. } | TypedIRValue::BorrowMutable { expr, .. } => {
            if let TypedIRValue::Variable(name, _) = expr.as_ref() {
                Some(name.clone())
            } else {
                extract_var_name(expr)
            }
        }
        _ => None,
    }
}

fn collect_all_vars(v: &TypedIRValue, out: &mut Vec<String>) {
    match v {
        TypedIRValue::Variable(name, _) => out.push(name.clone()),
        TypedIRValue::BorrowShared { expr, .. } | TypedIRValue::BorrowMutable { expr, .. } => {
            collect_all_vars(expr, out);
        }
        // NEW: recurse into record literal field values so a
        // `Point { x: a, y: b }` RHS contributes `a` and `b` as
        // uses.
        TypedIRValue::Record { fields, .. } => {
            for (_, fv) in fields {
                collect_all_vars(fv, out);
            }
        }
        TypedIRValue::Map { entries, .. } => {
            for (k, v) in entries {
                collect_all_vars(k, out);
                collect_all_vars(v, out);
            }
        }
        TypedIRValue::FieldAccess { object, .. } => {
            // `p.x` as an RHS is a use of `p`.
            collect_all_vars(object, out);
        }
        _ => {}
    }
}

/// ADR 0046. Recursively collect the names of every variable that
/// is bound to a reference at *some* depth inside `v`. A direct
/// `&x` produces `["x"]`; a record `Bag { inner: &x }` produces
/// `["x"]` too. Used by the `Terminator::Return` arm to emit
/// `ReturnRef` for references that would otherwise escape
/// unnoticed because they are buried inside a composite value.
///
/// The walker stops at non-reference leaves. A `Record` with a
/// plain `Int` field contributes nothing; only reference-typed
/// values contribute. Nested records and list elements recurse.
fn collect_reference_places(v: &TypedIRValue, out: &mut Vec<String>) {
    match v {
        TypedIRValue::BorrowShared { expr, .. } | TypedIRValue::BorrowMutable { expr, .. } => {
            if let Some(name) = extract_var_name(expr) {
                out.push(name);
            }
        }
        TypedIRValue::Record { fields, .. } => {
            for (_, fv) in fields {
                collect_reference_places(fv, out);
            }
        }
        TypedIRValue::List(items, _) | TypedIRValue::Array(items, _, _) => {
            for item in items {
                collect_reference_places(item, out);
            }
        }
        TypedIRValue::Some(inner) => collect_reference_places(inner, out),
        TypedIRValue::Ok { value, .. } | TypedIRValue::Error { value, .. } => {
            collect_reference_places(value, out);
        }
        // All other variants carry non-reference payloads at the
        // language level (Int, Float, String, Bool, Void, Map with
        // non-reference keys/values, etc.). None can hold a
        // reference that escapes.
        _ => {}
    }
}

fn is_borrow(v: &TypedIRValue) -> Option<(String, bool)> {
    match v {
        TypedIRValue::BorrowShared { expr, .. } => {
            extract_var_name(expr).map(|place| (place, false))
        }
        TypedIRValue::BorrowMutable { expr, .. } => {
            extract_var_name(expr).map(|place| (place, true))
        }
        _ => None,
    }
}

/// ADR 0016: build one `FunctionCfg` per non-extern function. Each
/// function's blocks keep their `SemanticProgram`-local IDs and its
/// declared entry block. Block IDs are unique within a program, so
/// no renumbering is needed.
/// ADR 0046. Compute the set of local variable names in a function
/// whose current value is, or contains, a reference. Runs a
/// flow-insensitive fixed point: a variable is reference-bearing
/// if it is ever assigned a value containing a reference, or is
/// ever assigned another reference-bearing variable.
///
/// A conservative over-approximation — a variable that holds a
/// reference on one path and a plain int on another is still
/// treated as reference-bearing. For an escape check that is the
/// correct direction: over-report, never miss.
///
/// Exists because `Terminator::Return { Variable(n) }` cannot see
/// inside `n`'s value at CFG-build time. Without this pre-pass, a
/// reference stored into a local record and returned by the
/// record's name reaches codegen unchecked.
fn compute_reference_bearing_vars(
    func: &crate::ir::semantic_ir::SemanticFunction,
) -> HashSet<String> {
    use crate::ir::semantic_ir::Instruction as I;

    let mut ref_bearing: HashSet<String> = HashSet::new();
    let mut changed = true;
    while changed {
        changed = false;
        for sblock in &func.blocks {
            for instr in &sblock.instructions {
                let (name, value) = match instr {
                    I::Declare { name, value, .. } => (name.clone(), value),
                    I::Assign { target, value } => (target.clone(), value),
                    _ => continue,
                };
                let mut places = Vec::new();
                collect_reference_places(value, &mut places);
                if !places.is_empty() && ref_bearing.insert(name.clone()) {
                    changed = true;
                    continue;
                }
                if let Some(src) = extract_var_name(value) {
                    if ref_bearing.contains(&src) && ref_bearing.insert(name.clone()) {
                        changed = true;
                    }
                }
            }
        }
    }
    ref_bearing
}

pub fn build_cfgs_from_semantic_program(program: &SemanticProgram) -> Vec<FunctionCfg> {
    let mut out = Vec::new();

    for func in &program.functions {
        if func.is_extern {
            continue;
        }

        // ADR 0046: which locals hold references (see the helper).
        let ref_bearing = compute_reference_bearing_vars(func);

        let mut cfg = Cfg::new(func.entry_block);

        for sblock in &func.blocks {
            let mut instrs = Vec::new();
            for instr in &sblock.instructions {
                use crate::ir::semantic_ir::Instruction as I;
                match instr {
                    I::Declare { name, value, .. } => {
                        if let Some((place, mutable)) = is_borrow(value) {
                            instrs.push(CfgInstruction::Declare { name: name.clone() });
                            instrs.push(CfgInstruction::Borrow {
                                borrower: name.clone(),
                                place,
                                mutable,
                            });
                        } else if let Some(src) = extract_var_name(value) {
                            instrs.push(CfgInstruction::Use { name: src.clone() });
                            instrs.push(CfgInstruction::Declare { name: name.clone() });
                        } else {
                            let mut vars = Vec::new();
                            collect_all_vars(value, &mut vars);
                            for v in vars {
                                instrs.push(CfgInstruction::Use { name: v });
                            }
                            instrs.push(CfgInstruction::Declare { name: name.clone() });
                        }
                    }
                    I::Assign { target, value } => {
                        if let Some((place, mutable)) = is_borrow(value) {
                            instrs.push(CfgInstruction::Borrow {
                                borrower: target.clone(),
                                place,
                                mutable,
                            });
                        } else if let Some(src) = extract_var_name(value) {
                            instrs.push(CfgInstruction::Use { name: src.clone() });
                            instrs.push(CfgInstruction::Assign {
                                name: target.clone(),
                            });
                        } else {
                            let mut vars = Vec::new();
                            collect_all_vars(value, &mut vars);
                            for v in vars {
                                instrs.push(CfgInstruction::Use { name: v });
                            }
                            instrs.push(CfgInstruction::Assign {
                                name: target.clone(),
                            });
                        }
                    }
                    I::WriteReference { reference, value } => {
                        let mut vars = Vec::new();
                        collect_all_vars(reference, &mut vars);
                        collect_all_vars(value, &mut vars);
                        for v in vars {
                            instrs.push(CfgInstruction::Use { name: v });
                        }
                    }
                    // ADR 0038 D4b. The receiver and every argument
                    // are uses; the result binding (if any) is
                    // defined. The vtable pointer is not a variable —
                    // it lives in the fat pointer value, not in the
                    // CFG's variable universe — so only the operands
                    // contribute `Use` edges.
                    I::VirtualCall {
                        receiver,
                        args,
                        result,
                        ..
                    } => {
                        let mut vars = Vec::new();
                        collect_all_vars(receiver, &mut vars);
                        for a in args {
                            collect_all_vars(a, &mut vars);
                        }
                        for v in vars {
                            instrs.push(CfgInstruction::Use { name: v });
                        }
                        if let Some(name) = result {
                            instrs.push(CfgInstruction::Assign { name: name.clone() });
                        }
                    }
                    I::ArrayAssign { array, value, .. } => {
                        let mut vars = Vec::new();
                        collect_all_vars(array, &mut vars);
                        collect_all_vars(value, &mut vars);
                        for v in vars {
                            instrs.push(CfgInstruction::Use { name: v });
                        }
                        if let Some(var_name) = extract_var_name(array) {
                            instrs.push(CfgInstruction::Assign { name: var_name });
                        } else {
                            instrs.push(CfgInstruction::Unsupported {
                                op: format!("ArrayAssign non-var {:?}", array),
                            });
                        }
                    }
                    I::FieldAssign {
                        target,
                        field: _,
                        value,
                    } => {
                        // Field assignment mutates the target record.
                        // Mirror the ArrayAssign arm: mark every
                        // variable mentioned in the RHS as used,
                        // then record the target as assigned so
                        // dataflow sees the write.
                        let mut vars = Vec::new();
                        collect_all_vars(value, &mut vars);
                        for v in vars {
                            instrs.push(CfgInstruction::Use { name: v });
                        }
                        instrs.push(CfgInstruction::Assign {
                            name: target.clone(),
                        });
                    }
                    I::RegionEnter { name } => {
                        instrs.push(CfgInstruction::RegionEnter { name: name.clone() })
                    }
                    I::RegionExit { name } => {
                        instrs.push(CfgInstruction::RegionExit { name: name.clone() })
                    }
                    I::Allocate { target, .. } => instrs.push(CfgInstruction::Declare {
                        name: target.clone(),
                    }),
                    I::Free { ptr } => {
                        if let Some(var_name) = extract_var_name(ptr) {
                            instrs.push(CfgInstruction::Use {
                                name: var_name.clone(),
                            });
                            instrs.push(CfgInstruction::Move { name: var_name });
                        } else {
                            instrs.push(CfgInstruction::Unsupported {
                                op: format!("Free non-var {:?}", ptr),
                            });
                        }
                    }
                    I::Call {
                        func: callee,
                        args,
                        result,
                        ..
                    } => {
                        for arg in args {
                            if let Some((place, mutable)) = is_borrow(arg) {
                                instrs.push(CfgInstruction::Borrow {
                                    borrower: format!("__tmp_call_{}_{}", sblock.id, place),
                                    place,
                                    mutable,
                                });
                            } else if let Some(var_name) = extract_var_name(arg) {
                                instrs.push(CfgInstruction::Use { name: var_name });
                            } else {
                                let mut vars = Vec::new();
                                collect_all_vars(arg, &mut vars);
                                for v in vars {
                                    instrs.push(CfgInstruction::Use { name: v });
                                }
                            }
                        }
                        if let Some(res) = result {
                            instrs.push(CfgInstruction::Declare { name: res.clone() });
                        }
                        instrs.push(CfgInstruction::Call {
                            name: callee.clone(),
                            args: args.iter().filter_map(extract_var_name).collect(),
                        })
                    }
                    I::SendChannel { channel, value } => {
                        if let Some(var_name) = extract_var_name(value) {
                            instrs.push(CfgInstruction::Use {
                                name: var_name.clone(),
                            });
                            instrs.push(CfgInstruction::Escape {
                                from: var_name,
                                to: "channel".into(),
                            });
                        } else {
                            let mut vars = Vec::new();
                            collect_all_vars(value, &mut vars);
                            for v in vars {
                                instrs.push(CfgInstruction::Use { name: v.clone() });
                                instrs.push(CfgInstruction::Escape {
                                    from: v,
                                    to: "channel".into(),
                                });
                            }
                        }
                        instrs.push(CfgInstruction::Use {
                            name: channel.clone(),
                        });
                    }
                    I::IteratorInit { iterator, iterable } => {
                        if let Some(var_name) = extract_var_name(iterable) {
                            instrs.push(CfgInstruction::Use { name: var_name });
                        } else {
                            let mut vars = Vec::new();
                            collect_all_vars(iterable, &mut vars);
                            for v in vars {
                                instrs.push(CfgInstruction::Use { name: v });
                            }
                        }
                        instrs.push(CfgInstruction::Declare {
                            name: iterator.clone(),
                        });
                    }
                    I::ChannelDecl { name, .. } => {
                        instrs.push(CfgInstruction::Declare { name: name.clone() });
                    }
                    I::ReceiveChannel { channel, target } => {
                        instrs.push(CfgInstruction::Use {
                            name: channel.clone(),
                        });
                        instrs.push(CfgInstruction::Declare {
                            name: target.clone(),
                        });
                    }
                    I::Print { value } => {
                        if let Some(var_name) = extract_var_name(value) {
                            instrs.push(CfgInstruction::Use { name: var_name });
                        } else {
                            let mut vars = Vec::new();
                            collect_all_vars(value, &mut vars);
                            for v in vars {
                                instrs.push(CfgInstruction::Use { name: v });
                            }
                        }
                    }
                    I::BoundsCheck { value, .. } => {
                        // ADR 0031: reading the value counts as a
                        // use. Same shape as the Print arm — the
                        // check reads but does not consume.
                        if let Some(var_name) = extract_var_name(value) {
                            instrs.push(CfgInstruction::Use { name: var_name });
                        } else {
                            let mut vars = Vec::new();
                            collect_all_vars(value, &mut vars);
                            for v in vars {
                                instrs.push(CfgInstruction::Use { name: v });
                            }
                        }
                    }
                    I::UnionConstruct { payload, .. } => {
                        let _ = payload;
                        instrs.push(CfgInstruction::Nop);
                    }
                    I::Nop => instrs.push(CfgInstruction::Nop),
                    // TryCatchMarker has no CFG semantics — it is a
                    // capability-scan marker, not control flow.
                    // Lower it to Nop in the CFG view.
                    I::TryCatchMarker => instrs.push(CfgInstruction::Nop),
                }
            }

            if let Some(term) = &sblock.terminator {
                match term {
                    Terminator::Return { value: Some(v), .. } => {
                        // Direct top-level borrow: `return &x`.
                        if let Some((place, _)) = is_borrow(v) {
                            instrs.push(CfgInstruction::ReturnRef { place });
                        } else {
                            // ADR 0046: walk the returned value for
                            // references at any depth. A record
                            // literal containing `&x` produces the
                            // same `ReturnRef` a direct `return &x`
                            // produces.
                            let mut places = Vec::new();
                            collect_reference_places(v, &mut places);
                            for place in places {
                                instrs.push(CfgInstruction::ReturnRef {
                                    place: place.clone(),
                                });
                            }
                            // If the returned value is a bare
                            // variable we determined to be
                            // reference-bearing earlier in the
                            // function, flag it too. The literal
                            // walker above cannot see inside a
                            // variable's current value — this
                            // closes the gap for
                            // `val bag := Bag { inner: &x }; return bag`.
                            if let Some(var_name) = extract_var_name(v) {
                                if ref_bearing.contains(&var_name) {
                                    instrs.push(CfgInstruction::ReturnRef { place: var_name });
                                }
                            }
                        }
                        // Independently, mark every variable the
                        // return value mentions as used. This
                        // subsumes the old `extract_var_name` and
                        // `collect_all_vars` fallbacks: the walker
                        // handles the bare-Variable case (it is a
                        // `Variable` arm in `collect_all_vars`) as
                        // well as the record/list case.
                        let mut vars = Vec::new();
                        collect_all_vars(v, &mut vars);
                        for name in vars {
                            instrs.push(CfgInstruction::Use { name });
                        }
                    }
                    Terminator::Branch { condition, .. } => {
                        if let Some(var_name) = extract_var_name(condition) {
                            instrs.push(CfgInstruction::Use { name: var_name });
                        } else {
                            let mut vars = Vec::new();
                            collect_all_vars(condition, &mut vars);
                            for v in vars {
                                instrs.push(CfgInstruction::Use { name: v });
                            }
                        }
                    }
                    Terminator::Switch { value, .. } => {
                        if let Some(var_name) = extract_var_name(value) {
                            instrs.push(CfgInstruction::Use { name: var_name });
                        }
                    }
                    _ => {}
                }
            }

            cfg.add_block(CfgBlock {
                id: sblock.id,
                instructions: instrs,
            });
        }

        for sblock in &func.blocks {
            if let Some(term) = &sblock.terminator {
                match term {
                    Terminator::Jump { block } => {
                        cfg.add_edge(sblock.id, *block);
                    }
                    Terminator::Branch {
                        then_block,
                        else_block,
                        ..
                    } => {
                        cfg.add_edge(sblock.id, *then_block);
                        cfg.add_edge(sblock.id, *else_block);
                    }
                    Terminator::Switch {
                        cases,
                        default_block,
                        ..
                    } => {
                        for (_, target) in cases {
                            cfg.add_edge(sblock.id, *target);
                        }
                        if let Some(d) = default_block {
                            cfg.add_edge(sblock.id, *d);
                        }
                    }
                    Terminator::IteratorNext {
                        body_block,
                        exit_block,
                        ..
                    } => {
                        cfg.add_edge(sblock.id, *body_block);
                        cfg.add_edge(sblock.id, *exit_block);
                    }
                    Terminator::Spawn { entry_block } => {
                        cfg.add_edge(sblock.id, *entry_block);
                    }
                    Terminator::Fork { blocks, .. } => {
                        for b in blocks {
                            cfg.add_edge(sblock.id, *b);
                        }
                    }
                    Terminator::Return { .. } => {}
                }
            }
        }

        out.push(FunctionCfg {
            name: func.name.clone(),
            cfg,
            params: func.params.iter().map(|(n, _)| n.clone()).collect(),
        });
    }

    out
}
