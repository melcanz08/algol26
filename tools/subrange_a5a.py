#!/usr/bin/env python3
"""
A5a: Instruction::BoundsCheck — enum variant + verifier + IR display
+ IR builder emit.

The backends (interpreter, LLVM) land in A5b.
"""

from pathlib import Path

INSTR = Path("src/ir/semantic_ir/instructions.rs")
VERIFIER = Path("src/ir/verifier/instruction.rs")
DISPLAY = Path("src/ir/semantic_ir/display.rs")
BUILDER_EXPR = Path("src/semantics/builder/expr.rs")


def patch(path, edits):
    src = path.read_text()
    for old, new, label in edits:
        n = src.count(old)
        if n != 1:
            print(f"FAIL: {path} — {label} matched {n} times")
            print("-" * 60)
            print(old[:250])
            print("-" * 60)
            path.write_text(src)
            raise SystemExit(1)
        src = src.replace(old, new, 1)
        path.write_text(src)
        print(f"OK: {path} — {label}")


# 1. Instruction variant
patch(INSTR, [
    (
        """    /// Exit a `region NAME` block.
    RegionExit {
        name: String,
    },
}""",
        """    /// Exit a `region NAME` block.
    RegionExit {
        name: String,
    },
    /// ADR 0031: subrange bounds check. `value` is evaluated; if it
    /// falls outside `low..high` (inclusive), `message` is printed
    /// and the enclosing function returns (or, in the interpreter,
    /// evaluation fails with `EvalError::Runtime`). Emitted by the
    /// IR builder for non-literal subrange construction where the
    /// analyzer cannot range-check at compile time.
    ///
    /// At runtime the value is an `Int` for Int-based subranges and
    /// the enum's ordinal (`i64`) for enum-based subranges. Both
    /// reach the check as integers.
    BoundsCheck {
        value: TypedIRValue,
        low: i64,
        high: i64,
        message: String,
    },
}""",
        "Instruction variant",
    ),
])

# 2. Verifier arm — value must be Int or an enum (both erase to i64)
patch(VERIFIER, [
    (
        """        // Region enter/exit carry no operands to verify. The
        // name is metadata; scope discipline is enforced by the
        // interpreter at runtime (or by the LLVM codegen treating
        // both as no-ops).
        Instruction::RegionEnter { .. } | Instruction::RegionExit { .. } => Ok(()),""",
        """        // Region enter/exit carry no operands to verify. The
        // name is metadata; scope discipline is enforced by the
        // interpreter at runtime (or by the LLVM codegen treating
        // both as no-ops).
        Instruction::RegionEnter { .. } | Instruction::RegionExit { .. } => Ok(()),
        // ADR 0031: BoundsCheck. The value must be an Int or an enum
        // (both erase to i64 at runtime). Out-of-range at runtime is
        // the whole point of the check; the verifier only confirms
        // the operand is the right shape.
        Instruction::BoundsCheck { value, low, high, .. } => {
            if *low > *high {
                return Err(format!(
                    "Function '{}': BoundsCheck has low > high ({}..{})",
                    func.name, low, high
                ));
            }
            let v_ty = verify_value(value, env)?;
            match v_ty {
                Type::Int | Type::Enum { .. } | Type::Unknown => Ok(()),
                other => Err(format!(
                    "Function '{}': BoundsCheck value must be Int or an enum, found {:?}",
                    func.name, other
                )),
            }
        }""",
        "verifier arm",
    ),
])

# 3. IR display arm
patch(DISPLAY, [
    (
        """        Instruction::RegionExit { name } => {
            write!(out, "}} // end region {}", name).unwrap();
        }
    }
}""",
        """        Instruction::RegionExit { name } => {
            write!(out, "}} // end region {}", name).unwrap();
        }
        Instruction::BoundsCheck {
            value, low, high, ..
        } => {
            out.push_str("bounds_check(");
            format_value(out, value);
            write!(out, ", {}..{})", low, high).unwrap();
        }
    }
}""",
        "IR display arm",
    ),
])

# 4. IR builder emit — modify the subrange intercept
patch(BUILDER_EXPR, [
    (
        """                if !clean_name.contains('.') {
                    if let Some(subrange) = self.subrange_types.get(clean_name).cloned() {
                        let inner = if let Some(arg) = args.first() {
                            self.translate_expr(program, func, current_block, arg)
                        } else {
                            TypedIRValue::Void
                        };
                        return TypedIRValue::Cast {
                            value: Box::new(inner),
                            target_type: subrange,
                        };
                    }
                }""",
        """                if !clean_name.contains('.') {
                    if let Some(subrange) = self.subrange_types.get(clean_name).cloned() {
                        let inner = if let Some(arg) = args.first() {
                            self.translate_expr(program, func, current_block, arg)
                        } else {
                            TypedIRValue::Void
                        };
                        // ADR 0031 A5: emit a runtime bounds check for
                        // non-literal arguments. The analyzer already
                        // range-checked literals at compile time.
                        if let Type::Subrange {
                            name: sr_name,
                            low,
                            high,
                            ..
                        } = &subrange
                        {
                            let is_literal = args.first().map_or(false, |a| {
                                matches!(
                                    a.kind,
                                    crate::frontend::ast::ExprKind::Int(_, _)
                                )
                            });
                            if !is_literal {
                                let message =
                                    format!("{}: value out of range {}..{}", sr_name, low, high);
                                self.safe_push_instruction(
                                    func,
                                    current_block,
                                    SemanticInstruction::BoundsCheck {
                                        value: inner.clone(),
                                        low: *low,
                                        high: *high,
                                        message,
                                    },
                                );
                            }
                        }
                        return TypedIRValue::Cast {
                            value: Box::new(inner),
                            target_type: subrange,
                        };
                    }
                }""",
        "IR builder emit",
    ),
])
