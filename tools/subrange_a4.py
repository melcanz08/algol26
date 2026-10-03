#!/usr/bin/env python3
"""
A4: subrange construction T(v) and literal coercion.

Two additions to src/semantics/analyzer/expr.rs:
1. try_subrange_construct — recognizes `T(v)` where T is a
   registered subrange type. Compile-time range check for literal
   arguments.
2. Literal coercion in analyze_expr_with_context: `val p: Percentage
   := 75` binds directly when 75 is in range.
"""

from pathlib import Path

PATH = Path("src/semantics/analyzer/expr.rs")
src = PATH.read_text()

def rep(old, new, label):
    global src
    n = src.count(old)
    if n != 1:
        print(f"FAIL: {label} — matched {n} times")
        print("-" * 60)
        print(old[:250])
        print("-" * 60)
        PATH.write_text(src)
        raise SystemExit(1)
    src = src.replace(old, new, 1)
    PATH.write_text(src)
    print(f"OK: {label}")

# 1. Construction intercept in FunctionCall
rep(
    """                // ADR 0029: Nominal type constructor. `UserId.from_base(x)`.
                // The receiver is a *type name*, not a variable, so this
                // must run before the ordinary dotted-name dispatch below.
                if let Some(ty) = self.try_nominal_from_base(clean_name, args)? {
                    return Ok(ty);
                }""",
    """                // ADR 0031: Subrange constructor. `Percentage(75)`.
                // Same reasoning as nominal from_base — the callee is
                // a type name, not a variable.
                if let Some(ty) = self.try_subrange_construct(clean_name, args)? {
                    return Ok(ty);
                }

                // ADR 0029: Nominal type constructor. `UserId.from_base(x)`.
                // The receiver is a *type name*, not a variable, so this
                // must run before the ordinary dotted-name dispatch below.
                if let Some(ty) = self.try_nominal_from_base(clean_name, args)? {
                    return Ok(ty);
                }""",
    "construction intercept",
)

# 2. try_subrange_construct helper — insert before try_enum_from_ordinal
SUB_METHOD = r'''    /// ADR 0031. If `clean_name` is the name of a registered
    /// subrange type, analyze `T(v)` as a construction. Returns
    /// `Ok(None)` when the name is not a subrange.
    ///
    /// Rules:
    /// - arity: exactly 1 argument
    /// - argument type must coerce to the base
    /// - literal Int arguments to Int bases are range-checked at
    ///   compile time
    ///
    /// Runtime bounds for non-literal arguments are enforced by
    /// `Instruction::BoundsCheck`, emitted by the IR builder in A5.
    fn try_subrange_construct(
        &mut self,
        clean_name: &str,
        args: &[Expr],
    ) -> Result<Option<Type>> {
        // Only bare identifiers, not dotted names like `X.foo`.
        if clean_name.contains('.') {
            return Ok(None);
        }
        let Some(subrange) = self.subrange_types.get(clean_name).cloned() else {
            return Ok(None);
        };
        let Type::Subrange {
            id,
            name,
            base,
            low,
            high,
        } = subrange
        else {
            return Ok(None);
        };

        if args.len() != 1 {
            return Err(CompileError::at(
                self.current_span,
                &format!("{} expects 1 argument, got {}", name, args.len()),
                ErrorCode::E0002,
            ));
        }

        // Compile-time range check for literal Int args over Int bases.
        if matches!(*base, Type::Int) {
            if let ExprKind::Int(n, span) = &args[0].kind {
                if *n < low || *n > high {
                    return Err(CompileError::at(
                        *span,
                        &format!(
                            "{}: {} is out of range {}..{}",
                            name, n, low, high
                        ),
                        ErrorCode::E0002,
                    ));
                }
            }
        }

        // Analyze the argument against the base. The analyzer's
        // type for the argument should coerce to the base.
        let arg_ty = self.analyze_expr_with_context(&args[0], Some(&base))?;
        if !arg_ty.is_unknown() && !arg_ty.can_coerce_to(&base) {
            return Err(CompileError::at(
                self.current_span,
                &format!("{} expects a value of type {}, found {}", name, base, arg_ty),
                ErrorCode::E0002,
            ));
        }

        Ok(Some(Type::subrange(id, &name, *base, low, high)))
    }

'''

rep(
    "    /// ADR 0030. If `clean_name` is `T.from_ordinal` where `T` is",
    SUB_METHOD + "    /// ADR 0030. If `clean_name` is `T.from_ordinal` where `T` is",
    "insert try_subrange_construct",
)

# 3. Literal coercion in analyze_expr_with_context
rep(
    """    pub(super) fn analyze_expr_with_context(
        &mut self,
        expr: &Expr,
        expected_type: Option<&Type>,
    ) -> Result<Type> {
        // Record the current node's span for error sites that don't
        // have the node in hand. No save/restore — the innermost
        // error site should use the innermost node's span.
        self.current_span = expr.span();
        let ty = self.analyze_expr_inner(expr, expected_type)?;
        self.type_table_id.insert(expr.id, ty.clone());
        Ok(ty)
    }""",
    """    pub(super) fn analyze_expr_with_context(
        &mut self,
        expr: &Expr,
        expected_type: Option<&Type>,
    ) -> Result<Type> {
        // Record the current node's span for error sites that don't
        // have the node in hand. No save/restore — the innermost
        // error site should use the innermost node's span.
        self.current_span = expr.span();

        // ADR 0031: literal coercion. An Int literal in range binds
        // directly to a subrange type when the expected type is a
        // subrange over Int. This is a narrow, value-dependent rule
        // that cannot live in `can_coerce_to` — `can_coerce_to` is
        // type-only. Out-of-range literals produce a targeted error
        // rather than falling through to a generic mismatch.
        if let Some(Type::Subrange {
            id, name, base, low, high,
        }) = expected_type
        {
            if matches!(**base, Type::Int) {
                if let ExprKind::Int(n, _) = &expr.kind {
                    if *n >= *low && *n <= *high {
                        let ty = Type::subrange(*id, name, (**base).clone(), *low, *high);
                        self.type_table_id.insert(expr.id, ty.clone());
                        return Ok(ty);
                    } else {
                        return Err(CompileError::at(
                            expr.span(),
                            &format!("{}: {} is out of range {}..{}", name, n, low, high),
                            ErrorCode::E0002,
                        ));
                    }
                }
            }
        }

        let ty = self.analyze_expr_inner(expr, expected_type)?;
        self.type_table_id.insert(expr.id, ty.clone());
        Ok(ty)
    }""",
    "literal coercion",
)

print()
print("OK: expr.rs patched")
