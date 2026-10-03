#!/usr/bin/env python3
"""
A6: enum ordinal conversion intrinsics.

Analyzer + IR builder recognize:
    Day.from_ordinal(n)   -> Day
    d.to_ordinal()        -> Int
    d.to_ordinal          -> Int          (no-parens form)

Compile-time check: literal out-of-range `from_ordinal(7)` on a
3-variant enum is rejected. Runtime out-of-range is not checked in
v1 (documented in the ADR update).

LLVM and interpreter Cast handlers get explicit Type::Enum arms.
"""

from pathlib import Path

ANALYZER = Path("src/semantics/analyzer/expr.rs")
BUILDER = Path("src/semantics/builder/expr.rs")
LLVM_VALUE = Path("src/backends/llvm_codegen/value.rs")
INTERP = Path("src/backends/interpreter/eval.rs")


ANALYZER_EDITS = [
    # 1. from_ordinal intercept in FunctionCall, after from_base
    (
        """                // ADR 0029: Nominal type constructor. `UserId.from_base(x)`.
                // The receiver is a *type name*, not a variable, so this
                // must run before the ordinary dotted-name dispatch below.
                if let Some(ty) = self.try_nominal_from_base(clean_name, args)? {
                    return Ok(ty);
                }

                if clean_name.contains('.') {""",
        """                // ADR 0029: Nominal type constructor. `UserId.from_base(x)`.
                // The receiver is a *type name*, not a variable, so this
                // must run before the ordinary dotted-name dispatch below.
                if let Some(ty) = self.try_nominal_from_base(clean_name, args)? {
                    return Ok(ty);
                }

                // ADR 0030: Enum ordinal constructor. `Day.from_ordinal(x)`.
                // Same shape as nominal `from_base`.
                if let Some(ty) = self.try_enum_from_ordinal(clean_name, args)? {
                    return Ok(ty);
                }

                if clean_name.contains('.') {""",
    ),
    # 2. to_ordinal intercept in dotted dispatch, before to_base
    (
        """                        if let Some((receiver_type, mutable)) = self.lookup_variable(receiver) {
                            // ADR 0029: Nominal type instance conversion.
                            // `x.to_base()` where x has a Distinct type.
                            // Yields the base type; consumes the receiver
                            // if the base is non-Copy.
                            if let Type::Distinct { base, .. } = &receiver_type {""",
        """                        if let Some((receiver_type, mutable)) = self.lookup_variable(receiver) {
                            // ADR 0030: Enum ordinal extraction.
                            // `d.to_ordinal()` where d has an Enum type.
                            if let Type::Enum { .. } = &receiver_type {
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
                            // ADR 0029: Nominal type instance conversion.
                            // `x.to_base()` where x has a Distinct type.
                            // Yields the base type; consumes the receiver
                            // if the base is non-Copy.
                            if let Type::Distinct { base, .. } = &receiver_type {""",
    ),
    # 3. to_ordinal intercept in FieldAccess, before to_base
    (
        """            ExprKind::FieldAccess { object, field, .. } => {
                let obj_ty = self.analyze_expr(object)?;

                // ADR 0029: Nominal type instance conversion, no-parens
                // form. `x.to_base` where x has a Distinct type.
                if let Type::Distinct { base, .. } = &obj_ty {""",
        """            ExprKind::FieldAccess { object, field, .. } => {
                let obj_ty = self.analyze_expr(object)?;

                // ADR 0030: Enum ordinal extraction, no-parens form.
                if let Type::Enum { .. } = &obj_ty {
                    if field == "to_ordinal" {
                        return Ok(Type::Int);
                    }
                }

                // ADR 0029: Nominal type instance conversion, no-parens
                // form. `x.to_base` where x has a Distinct type.
                if let Type::Distinct { base, .. } = &obj_ty {""",
    ),
    # 4. try_enum_from_ordinal helper, added next to try_nominal_from_base
    (
        """    /// ADR 0029. If `clean_name` is `T.from_base` where `T` is a
    /// registered nominal type, analyze `args` and produce the
    /// nominal type. Returns `Ok(None)` if the name is not a
    /// nominal constructor, so the caller falls through to the
    /// ordinary function dispatch.
    ///
    /// Runs before the dotted-name path because `UserId` in
    /// `UserId.from_base(...)` is a type name, not a variable. The
    /// ordinary dispatch would report "Undefined function" otherwise.
    fn try_nominal_from_base(&mut self, clean_name: &str, args: &[Expr]) -> Result<Option<Type>> {""",
        """    /// ADR 0030. If `clean_name` is `T.from_ordinal` where `T` is
    /// a registered enum type, analyze `args` and produce the enum
    /// type. Returns `Ok(None)` if the name is not an enum
    /// constructor.
    ///
    /// Same reasoning as `try_nominal_from_base`: the receiver is a
    /// type name, not a variable. A literal out-of-range argument is
    /// rejected at compile time; runtime out-of-range is not checked
    /// in v1.
    fn try_enum_from_ordinal(
        &mut self,
        clean_name: &str,
        args: &[Expr],
    ) -> Result<Option<Type>> {
        let Some((receiver, method)) = clean_name.split_once('.') else {
            return Ok(None);
        };
        if method != "from_ordinal" {
            return Ok(None);
        }
        let Some(ty) = self.enum_types.get(receiver).cloned() else {
            return Ok(None);
        };
        let Type::Enum { id, name, variants } = ty else {
            return Ok(None);
        };

        if args.len() != 1 {
            return Err(CompileError::at(
                self.current_span,
                &format!(
                    "{}.from_ordinal expects 1 argument, got {}",
                    name,
                    args.len()
                ),
                ErrorCode::E0002,
            ));
        }

        // Compile-time range check for literal arguments.
        if let ExprKind::Int(n, span) = &args[0].kind {
            if *n < 0 || (*n as usize) >= variants.len() {
                let max = variants.len().saturating_sub(1);
                return Err(CompileError::at(
                    *span,
                    &format!(
                        "ordinal {} is out of range for {}; valid range is 0..{}",
                        n, name, max
                    ),
                    ErrorCode::E0002,
                ));
            }
        }

        let arg_ty = self.analyze_expr_with_context(&args[0], Some(&Type::Int))?;
        if !arg_ty.is_unknown() && !arg_ty.can_coerce_to(&Type::Int) {
            return Err(CompileError::at(
                self.current_span,
                &format!("{}.from_ordinal expects an Int, found {}", name, arg_ty),
                ErrorCode::E0002,
            ));
        }

        Ok(Some(Type::enum_type(id, &name, variants)))
    }

    /// ADR 0029. If `clean_name` is `T.from_base` where `T` is a
    /// registered nominal type, analyze `args` and produce the
    /// nominal type. Returns `Ok(None)` if the name is not a
    /// nominal constructor, so the caller falls through to the
    /// ordinary function dispatch.
    ///
    /// Runs before the dotted-name path because `UserId` in
    /// `UserId.from_base(...)` is a type name, not a variable. The
    /// ordinary dispatch would report "Undefined function" otherwise.
    fn try_nominal_from_base(&mut self, clean_name: &str, args: &[Expr]) -> Result<Option<Type>> {""",
    ),
]


BUILDER_EDITS = [
    # Extend the intrinsic block to also handle from_ordinal / to_ordinal.
    (
        """                // ADR 0029: nominal type conversion intrinsics.
                // Lower to a no-op Cast that carries the nominal type.
                if let Some(dot) = clean_name.find('.') {
                    let (receiver, method) = (&clean_name[..dot], &clean_name[dot + 1..]);

                    // T.from_base(x): x already has the base
                    // representation; wrap it with the nominal type.
                    if method == "from_base" {
                        if let Some(nominal) = self.nominal_types.get(receiver).cloned() {
                            let inner = if let Some(arg) = args.first() {
                                self.translate_expr(program, func, current_block, arg)
                            } else {
                                TypedIRValue::Void
                            };
                            return TypedIRValue::Cast {
                                value: Box::new(inner),
                                target_type: nominal,
                            };
                        }
                    }

                    // v.to_base(): the receiver's nominal type wraps
                    // its base at runtime; unwrap the type annotation.
                    if method == "to_base" && args.is_empty() {
                        if let Some(info) = self.lookup_var(receiver) {
                            if let Type::Distinct { base, .. } = &info.type_ {
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
                    }
                }""",
        """                // ADR 0029/0030: conversion intrinsics. Each lowers
                // to a no-op Cast that carries the target type.
                if let Some(dot) = clean_name.find('.') {
                    let (receiver, method) = (&clean_name[..dot], &clean_name[dot + 1..]);

                    // T.from_base(x): x already has the base
                    // representation; wrap it with the nominal type.
                    if method == "from_base" {
                        if let Some(nominal) = self.nominal_types.get(receiver).cloned() {
                            let inner = if let Some(arg) = args.first() {
                                self.translate_expr(program, func, current_block, arg)
                            } else {
                                TypedIRValue::Void
                            };
                            return TypedIRValue::Cast {
                                value: Box::new(inner),
                                target_type: nominal,
                            };
                        }
                    }

                    // T.from_ordinal(x): the enum value's runtime
                    // representation is the ordinal, an Int. Wrap it
                    // with the enum type. See ADR 0030.
                    if method == "from_ordinal" {
                        if let Some(enum_ty) = self.enum_types.get(receiver).cloned() {
                            let inner = if let Some(arg) = args.first() {
                                self.translate_expr(program, func, current_block, arg)
                            } else {
                                TypedIRValue::Void
                            };
                            return TypedIRValue::Cast {
                                value: Box::new(inner),
                                target_type: enum_ty,
                            };
                        }
                    }

                    // v.to_base(): unwrap the nominal type to its base.
                    if method == "to_base" && args.is_empty() {
                        if let Some(info) = self.lookup_var(receiver) {
                            if let Type::Distinct { base, .. } = &info.type_ {
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
                    }

                    // v.to_ordinal(): unwrap the enum to its ordinal.
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
                }""",
    ),
    # FieldAccess no-parens to_ordinal
    (
        """                // ADR 0029: `v.to_base` (no-parens form). Same as the
                // parenthesized FunctionCall form above.
                if field == "to_base" {
                    if let Type::Distinct { base, .. } = &obj_ty {
                        let base_ty = (**base).clone();
                        return TypedIRValue::Cast {
                            value: Box::new(obj),
                            target_type: base_ty,
                        };
                    }
                }""",
        """                // ADR 0029: `v.to_base` (no-parens form). Same as the
                // parenthesized FunctionCall form above.
                if field == "to_base" {
                    if let Type::Distinct { base, .. } = &obj_ty {
                        let base_ty = (**base).clone();
                        return TypedIRValue::Cast {
                            value: Box::new(obj),
                            target_type: base_ty,
                        };
                    }
                }

                // ADR 0030: `v.to_ordinal` (no-parens form).
                if field == "to_ordinal" {
                    if let Type::Enum { .. } = &obj_ty {
                        return TypedIRValue::Cast {
                            value: Box::new(obj),
                            target_type: Type::Int,
                        };
                    }
                }""",
    ),
]


LLVM_EDITS = [
    (
        """                    // String is a `char*` pointer in LLVM. A cast whose
                    // source is already a pointer and whose target is
                    // `String` is a no-op — this is the shape the
                    // nominal unwrap `Distinct<String> -> String`
                    // produces (ADR 0029).
                    (BasicValueEnum::PointerValue(_), Type::String) => v,
                    // ADR 0029: nominal wrap/unwrap.""",
        """                    // String is a `char*` pointer in LLVM. A cast whose
                    // source is already a pointer and whose target is
                    // `String` is a no-op — this is the shape the
                    // nominal unwrap `Distinct<String> -> String`
                    // produces (ADR 0029).
                    (BasicValueEnum::PointerValue(_), Type::String) => v,
                    // ADR 0030: enum wrap. The value's LLVM type is
                    // i64 (matching `map_type(Type::Enum)`), so the
                    // cast is a no-op. No runtime bounds check in v1;
                    // the analyzer rejects out-of-range literals.
                    (BasicValueEnum::IntValue(_), Type::Enum { .. }) => v,
                    // ADR 0029: nominal wrap/unwrap.""",
    ),
]


INTERP_EDITS = [
    (
        """                    // ADR 0029: nominal wrap/unwrap. The runtime
                    // representation of a nominal type is identical
                    // to its base, so the value flows through
                    // unchanged. Explicit arm so the intent is
                    // visible without tracing the catch-all.
                    (v, Type::Distinct { .. }) => v,
                    (v, _) => v,""",
        """                    // ADR 0029: nominal wrap/unwrap. The runtime
                    // representation of a nominal type is identical
                    // to its base, so the value flows through
                    // unchanged. Explicit arm so the intent is
                    // visible without tracing the catch-all.
                    (v, Type::Distinct { .. }) => v,
                    // ADR 0030: enum wrap/unwrap. Same shape — the
                    // runtime value is just the ordinal as an Int.
                    (v, Type::Enum { .. }) => v,
                    (v, _) => v,""",
    ),
]


def patch(path, edits):
    if not path.exists():
        print(f"ERROR: {path} not found.")
        raise SystemExit(1)
    src = path.read_text()
    for i, (old, new) in enumerate(edits, start=1):
        count = src.count(old)
        if count != 1:
            print(f"FAIL: {path} — edit {i} matched {count} times; expected 1.")
            print("-" * 60)
            print(old[:250])
            print("-" * 60)
            raise SystemExit(1)
        src = src.replace(old, new, 1)
    path.write_text(src)
    print(f"OK: {path}")


def main():
    patch(ANALYZER, ANALYZER_EDITS)
    patch(BUILDER, BUILDER_EDITS)
    patch(LLVM_VALUE, LLVM_EDITS)
    patch(INTERP, INTERP_EDITS)
    print()
    print("NEXT: cargo build --all-targets 2>&1 | head -40")


if __name__ == "__main__":
    main()
