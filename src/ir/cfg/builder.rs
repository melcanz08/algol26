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
pub fn build_cfgs_from_semantic_program(program: &SemanticProgram) -> Vec<FunctionCfg> {
    let mut out = Vec::new();

    for func in &program.functions {
        if func.is_extern {
            continue;
        }

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
                    I::Call {
                        func: callee,
                        args,
                        result,
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
