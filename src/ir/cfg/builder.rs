// src/ir/cfg/builder.rs - v0.9-I EXHAUSTIVE
// E: CFG foundation, F: ownership/region/escape + Use tracking,
// H: no silent fallbacks, I: exhaustive match (compile-time guard)
//
// Note on fail-closed design: the match below is exhaustive —
// every Instruction variant has an explicit arm. Adding a new
// variant to `Instruction` without adding an arm here is a
// compile error. This is stronger than a `_ => Unsupported`
// catch-all because it fails at build time, not runtime.

use super::dataflow::{BlockId, Cfg, CfgBlock, CfgInstruction};
use crate::ir::semantic_ir::{SemanticProgram, Terminator, TypedIRValue};
use std::collections::HashMap;

fn extract_var_name(v: &TypedIRValue) -> Option<String> {
    match v {
        TypedIRValue::Variable(name, _) => Some(name.clone()),
        TypedIRValue::Borrow { expr, .. }
        | TypedIRValue::MutBorrow { expr, .. }
        | TypedIRValue::BorrowShared { expr, .. }
        | TypedIRValue::BorrowMutable { expr, .. } => {
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
        TypedIRValue::Borrow { expr, .. }
        | TypedIRValue::MutBorrow { expr, .. }
        | TypedIRValue::BorrowShared { expr, .. }
        | TypedIRValue::BorrowMutable { expr, .. } => {
            collect_all_vars(expr, out);
        }
        _ => {}
    }
}

fn is_borrow(v: &TypedIRValue) -> Option<(String, bool)> {
    match v {
        TypedIRValue::Borrow { expr, .. } => extract_var_name(expr).map(|place| (place, false)),
        TypedIRValue::MutBorrow { expr, .. } => extract_var_name(expr).map(|place| (place, true)),
        TypedIRValue::BorrowShared { expr, .. } => {
            extract_var_name(expr).map(|place| (place, false))
        }
        TypedIRValue::BorrowMutable { expr, .. } => {
            extract_var_name(expr).map(|place| (place, true))
        }
        _ => None,
    }
}

pub fn build_cfg_from_semantic_program(program: &SemanticProgram) -> Cfg {
    let mut cfg = Cfg::new(0);
    let mut block_id_map: HashMap<usize, BlockId> = HashMap::new();
    let mut next_id = 0;
    for func in &program.functions {
        if func.is_extern {
            continue;
        }
        for block in &func.blocks {
            block_id_map.insert(block.id, next_id);
            next_id += 1;
        }
    }
    for func in &program.functions {
        if func.is_extern {
            continue;
        }
        for sblock in &func.blocks {
            let id = block_id_map[&sblock.id];
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
                    I::Call { func, args, result } => {
                        for arg in args {
                            if let Some((place, mutable)) = is_borrow(arg) {
                                instrs.push(CfgInstruction::Borrow {
                                    borrower: format!("__tmp_call_{}_{}", id, place),
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
                            name: func.clone(),
                            args: args.iter().filter_map(extract_var_name).collect(),
                        })
                    }
                    I::ChannelSend { channel, value } | I::Send { channel, value } => {
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
                    // Iterating a variable: use the iterable, declare
                    // the loop variable. Neither participates in
                    // ownership transfer in the current model.
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

                    // Channel declaration: channels are disjoint from
                    // the ownership domain (they cannot hold references),
                    // so no dataflow instruction is emitted. Tracked as
                    // a declared name anyway so uses of the channel
                    // don't trip E-INIT-001.
                    I::ChannelDecl { name, .. } => {
                        instrs.push(CfgInstruction::Declare { name: name.clone() });
                    }

                    // Receive: binds `target` to a value read from the
                    // channel. Like ChannelDecl, no ownership transfer
                    // crosses this boundary.
                    I::Receive { channel, target } | I::ChannelReceive { channel, target } => {
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
                    I::Nop => instrs.push(CfgInstruction::Nop),
                }
            }
            if let Some(term) = &sblock.terminator {
                match term {
                    Terminator::Return { value: Some(v), .. } => {
                        if let Some((place, _)) = is_borrow(v) {
                            instrs.push(CfgInstruction::ReturnRef { place });
                        } else if let Some(var_name) = extract_var_name(v) {
                            instrs.push(CfgInstruction::Use { name: var_name });
                        } else {
                            let mut vars = Vec::new();
                            collect_all_vars(v, &mut vars);
                            for var_name in vars {
                                instrs.push(CfgInstruction::Use { name: var_name });
                            }
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
                id,
                instructions: instrs,
            });
        }
    }
    for func in &program.functions {
        if func.is_extern {
            continue;
        }
        for sblock in &func.blocks {
            let from = block_id_map[&sblock.id];
            if let Some(term) = &sblock.terminator {
                match term {
                    Terminator::Jump { block } => {
                        if let Some(to) = block_id_map.get(block) {
                            cfg.add_edge(from, *to);
                        }
                    }
                    Terminator::Branch {
                        then_block,
                        else_block,
                        ..
                    } => {
                        if let Some(to) = block_id_map.get(then_block) {
                            cfg.add_edge(from, *to);
                        }
                        if let Some(to) = block_id_map.get(else_block) {
                            cfg.add_edge(from, *to);
                        }
                    }
                    Terminator::Switch {
                        cases,
                        default_block,
                        ..
                    } => {
                        for (_, target) in cases {
                            if let Some(to) = block_id_map.get(target) {
                                cfg.add_edge(from, *to);
                            }
                        }
                        if let Some(d) = default_block {
                            if let Some(to) = block_id_map.get(d) {
                                cfg.add_edge(from, *to);
                            }
                        }
                    }
                    Terminator::IteratorNext {
                        body_block,
                        exit_block,
                        ..
                    } => {
                        if let Some(to) = block_id_map.get(body_block) {
                            cfg.add_edge(from, *to);
                        }
                        if let Some(to) = block_id_map.get(exit_block) {
                            cfg.add_edge(from, *to);
                        }
                    }
                    Terminator::Spawn { entry_block } => {
                        if let Some(to) = block_id_map.get(entry_block) {
                            cfg.add_edge(from, *to);
                        }
                    }
                    Terminator::Fork { blocks, join_block } => {
                        for b in blocks {
                            if let Some(to) = block_id_map.get(b) {
                                cfg.add_edge(from, *to);
                            }
                        }
                        if let Some(to) = block_id_map.get(join_block) {
                            cfg.add_edge(from, *to);
                        }
                    }
                    Terminator::Return { .. } => {}
                }
            }
        }
    }
    cfg.entry = 0;
    cfg
}
