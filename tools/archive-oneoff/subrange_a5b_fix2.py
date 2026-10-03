#!/usr/bin/env python3
"""A5b fix 2: BoundsCheck arm in the CFG builder."""

from pathlib import Path

PATH = Path("src/ir/cfg/builder.rs")
src = PATH.read_text()

old = """                    I::Nop => instrs.push(CfgInstruction::Nop),
                }"""
new = """                    I::BoundsCheck { value, .. } => {
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
                    I::Nop => instrs.push(CfgInstruction::Nop),
                }"""

n = src.count(old)
if n != 1:
    print(f"FAIL: matched {n} times; expected 1")
    print("-" * 60)
    print(old)
    print("-" * 60)
    raise SystemExit(1)

PATH.write_text(src.replace(old, new, 1))
print("OK: patched src/ir/cfg/builder.rs")
