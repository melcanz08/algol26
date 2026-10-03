#!/usr/bin/env python3
"""
A6 (extraction): subrange `.to_base()` / `.to_base`.

Same shape as the nominal `to_base` from ADR 0029. Returns the base
type; lowers to a no-op Cast at IR.
"""

from pathlib import Path

ANALYZER = Path("src/semantics/analyzer/expr.rs")
BUILDER = Path("src/semantics/builder/expr.rs")


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


# ─── Analyzer ──────────────────────────────────────────────────────
patch(ANALYZER, [
    # 1. FunctionCall: to_base() for subranges, before nominal to_base
    (
        """                            if let Type::Enum { .. } = &receiver_type {
                                if method_name == "to_ordinal" {
                                    if !args.is_empty() {
                                        return Err(CompileError::at(
                                            self.current_span,
                                            &format!(
                                                "to_ordinal takes no arguments, got {}",
                                                args.len()
                                            ),
                                            ErrorCode::E0002,
                                        ));
                                    }
                                    return Ok(Type::Int);
                                }
                            }""",
        """                            if let Type::Enum { .. } = &receiver_type {
                                if method_name == "to_ordinal" {
                                    if !args.is_empty() {
                                        return Err(CompileError::at(
                                            self.current_span,
                                            &format!(
                                                "to_ordinal takes no arguments, got {}",
                                                args.len()
                                            ),
                                            ErrorCode::E0002,
                                        ));
                                    }
                                    return Ok(Type::Int);
                                }
                            }
                            // ADR 0031: subrange extraction.
                            // `p.to_base()` where p has a subrange type.
                            if let Type::Subrange { base, .. } = &receiver_type {
                                if method_name == "to_base" {
                                    if !args.is_empty() {
                                        return Err(CompileError::at(
                                            self.current_span,
                                            &format!(
                                                "to_base takes no arguments, got {}",
                                                args.len()
                                            ),
                                            ErrorCode::E0002,
                                        ));
                                    }
                                    return Ok((**base).clone());
                                }
                            }""",
        "FunctionCall to_base intercept",
    ),
    # 2. FieldAccess: to_base no-parens for subranges
    (
        """                // ADR 0030: Enum ordinal extraction, no-parens form.
                if let Type::Enum { .. } = &obj_ty {
                    if field == "to_ordinal" {
                        return Ok(Type::Int);
                    }
                }""",
        """                // ADR 0030: Enum ordinal extraction, no-parens form.
                if let Type::Enum { .. } = &obj_ty {
                    if field == "to_ordinal" {
                        return Ok(Type::Int);
                    }
                }

                // ADR 0031: Subrange extraction, no-parens form.
                if let Type::Subrange { base, .. } = &obj_ty {
                    if field == "to_base" {
                        return Ok((**base).clone());
                    }
                }""",
        "FieldAccess to_base intercept",
    ),
])

# ─── IR builder ────────────────────────────────────────────────────
patch(BUILDER, [
    # 3. FunctionCall: to_base() for subranges, before nominal to_base
    (
        """                    // v.to_ordinal(): unwrap the enum to its ordinal.
                    if method == "to_ordinal" && args.is_empty() {
                        if let Some(info) = self.lookup_var(receiver) {
                            if let Type::Enum { .. } = &info.type_ {
                                let receiver_value = TypedIRValue::Variable(
                                    receiver.to_string(),
                                    info.type_.clone(),
                                );
                                return TypedIRValue::Cast {
                                    value: Box::new(receiver_value),
                                    target_type: Type::Int,
                                };
                            }
                        }
                    }""",
        """                    // v.to_ordinal(): unwrap the enum to its ordinal.
                    if method == "to_ordinal" && args.is_empty() {
                        if let Some(info) = self.lookup_var(receiver) {
                            if let Type::Enum { .. } = &info.type_ {
                                let receiver_value = TypedIRValue::Variable(
                                    receiver.to_string(),
                                    info.type_.clone(),
                                );
                                return TypedIRValue::Cast {
                                    value: Box::new(receiver_value),
                                    target_type: Type::Int,
                                };
                            }
                        }
                    }

                    // ADR 0031: v.to_base() where v has a subrange
                    // type. No-op Cast to the base.
                    if method == "to_base" && args.is_empty() {
                        if let Some(info) = self.lookup_var(receiver) {
                            if let Type::Subrange { base, .. } = &info.type_ {
                                let receiver_value = TypedIRValue::Variable(
                                    receiver.to_string(),
                                    info.type_.clone(),
                                );
                                let base_ty = (**base).clone();
                                return TypedIRValue::Cast {
                                    value: Box::new(receiver_value),
                                    target_type: base_ty,
                                };
                            }
                        }
                    }""",
        "FunctionCall to_base IR",
    ),
    # 4. FieldAccess: to_base no-parens for subranges
    (
        """                // ADR 0030: `v.to_ordinal` (no-parens form).
                if field == "to_ordinal" {
                    if let Type::Enum { .. } = &obj_ty {
                        return TypedIRValue::Cast {
                            value: Box::new(obj),
                            target_type: Type::Int,
                        };
                    }
                }""",
        """                // ADR 0030: `v.to_ordinal` (no-parens form).
                if field == "to_ordinal" {
                    if let Type::Enum { .. } = &obj_ty {
                        return TypedIRValue::Cast {
                            value: Box::new(obj),
                            target_type: Type::Int,
                        };
                    }
                }

                // ADR 0031: `v.to_base` (no-parens form) for subranges.
                if field == "to_base" {
                    if let Type::Subrange { base, .. } = &obj_ty {
                        let base_ty = (**base).clone();
                        return TypedIRValue::Cast {
                            value: Box::new(obj),
                            target_type: base_ty,
                        };
                    }
                }""",
        "FieldAccess to_base IR",
    ),
])
