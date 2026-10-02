#!/usr/bin/env python3
"""
A5: nominal type conversion intrinsics.

Recognized structurally by the analyzer:
    UserId.from_base(x)  -> UserId   (receiver is a type name)
    x.to_base()          -> base     (receiver is a variable)
    x.to_base            -> base     (no-parens form)

Consumption: for non-Copy bases, the converted-from value is marked
moved after the conversion. For Copy bases (Int, Float, Bool) the
original remains usable, matching ordinary copy semantics.

Not included here (later steps): is_hashable_key recursion for
Distinct keys, moved-receiver check on the to_base receiver,
trait-based equality, automated tests.
"""

from pathlib import Path

EXPR = Path("src/semantics/analyzer/expr.rs")

EDITS = [
    # 1. from_base intercept — top of the FunctionCall arm.
    (
        """            ExprKind::FunctionCall { name, args, .. } => {
                let clean_name = name.trim_end_matches("()");

                if clean_name.contains('.') {""",
        """            ExprKind::FunctionCall { name, args, .. } => {
                let clean_name = name.trim_end_matches("()");

                // ADR 0029: Nominal type constructor. `UserId.from_base(x)`.
                // The receiver is a *type name*, not a variable, so this
                // must run before the ordinary dotted-name dispatch below.
                if let Some(ty) = self.try_nominal_from_base(clean_name, args)? {
                    return Ok(ty);
                }

                if clean_name.contains('.') {""",
    ),

    # 2. to_base() dispatch — inside the dotted receiver block.
    (
        """                        if let Some((receiver_type, mutable)) = self.lookup_variable(receiver) {
                            // ─── List.append dispatch ───""",
        """                        if let Some((receiver_type, mutable)) = self.lookup_variable(receiver) {
                            // ADR 0029: Nominal type instance conversion.
                            // `x.to_base()` where x has a Distinct type.
                            // Yields the base type; consumes the receiver
                            // if the base is non-Copy.
                            if let Type::Distinct { base, .. } = &receiver_type {
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
                                    let base_ty = (**base).clone();
                                    if !self.is_type_copy(&base_ty) {
                                        let span = self.current_span;
                                        self.mark_moved(receiver, span);
                                    }
                                    return Ok(base_ty);
                                }
                            }
                            // ─── List.append dispatch ───""",
    ),

    # 3. to_base no-parens dispatch — at the top of FieldAccess arm.
    (
        """            ExprKind::FieldAccess { object, field, .. } => {
                let obj_ty = self.analyze_expr(object)?;

                // The parser produces `FieldAccess` for both `p.x`""",
        """            ExprKind::FieldAccess { object, field, .. } => {
                let obj_ty = self.analyze_expr(object)?;

                // ADR 0029: Nominal type instance conversion, no-parens
                // form. `x.to_base` where x has a Distinct type.
                if let Type::Distinct { base, .. } = &obj_ty {
                    if field == "to_base" {
                        let base_ty = (**base).clone();
                        if !self.is_type_copy(&base_ty) {
                            if let ExprKind::Var(name, _) = &object.as_ref().kind {
                                let span = self.current_span;
                                self.mark_moved(name, span);
                            }
                        }
                        return Ok(base_ty);
                    }
                }

                // The parser produces `FieldAccess` for both `p.x`""",
    ),

    # 4. Helper method `try_nominal_from_base` — inserted before
    #    substitute_type_vars.
    (
        """    pub(super) fn substitute_type_vars(
        &self,
        type_: &Type,
        bindings: &HashMap<String, Type>,
    ) -> Type {""",
        """    /// ADR 0029. If `clean_name` is `T.from_base` where `T` is a
    /// registered nominal type, analyze `args` and produce the
    /// nominal type. Returns `Ok(None)` if the name is not a
    /// nominal constructor, so the caller falls through to the
    /// ordinary function dispatch.
    ///
    /// Runs before the dotted-name path because `UserId` in
    /// `UserId.from_base(...)` is a type name, not a variable. The
    /// ordinary dispatch would report "Undefined function" otherwise.
    fn try_nominal_from_base(
        &mut self,
        clean_name: &str,
        args: &[Expr],
    ) -> Result<Option<Type>> {
        let Some((receiver, method)) = clean_name.split_once('.') else {
            return Ok(None);
        };
        if method != "from_base" {
            return Ok(None);
        }
        let Some(ty) = self.nominal_types.get(receiver).cloned() else {
            return Ok(None);
        };
        let Type::Distinct { id, name, base } = ty else {
            return Ok(None);
        };

        if args.len() != 1 {
            return Err(CompileError::at(
                self.current_span,
                &format!("{}.from_base expects 1 argument, got {}", name, args.len()),
                ErrorCode::E0002,
            ));
        }

        let arg_ty = self.analyze_expr_with_context(&args[0], Some(&*base))?;
        if !arg_ty.is_unknown() && !arg_ty.can_coerce_to(&*base) {
            return Err(CompileError::at(
                self.current_span,
                &format!(
                    "{}.from_base expects a value of type {}, found {}",
                    name, base, arg_ty
                ),
                ErrorCode::E0002,
            ));
        }

        // Consumption: mark the argument moved if non-Copy. For
        // Copy bases (Int, Float, Bool) the value is duplicated.
        if let ExprKind::Var(arg_name, _) = &args[0].kind {
            if !self.is_type_copy(&arg_ty) {
                let span = args[0].span();
                self.mark_moved(arg_name, span);
            }
        }

        Ok(Some(Type::distinct(id, &name, *base)))
    }

    pub(super) fn substitute_type_vars(
        &self,
        type_: &Type,
        bindings: &HashMap<String, Type>,
    ) -> Type {""",
    ),
]


def main():
    if not EXPR.exists():
        print(f"ERROR: {EXPR} not found. Run from the repo root.")
        raise SystemExit(1)

    src = EXPR.read_text()
    for i, (old, new) in enumerate(EDITS, start=1):
        count = src.count(old)
        if count != 1:
            print(f"FAIL: edit {i} matched {count} times; expected 1.")
            print("-" * 60)
            print(old[:250])
            print("-" * 60)
            print("Nothing written.")
            raise SystemExit(1)
        src = src.replace(old, new, 1)

    EXPR.write_text(src)
    print(f"OK: patched {EXPR}")
    print()
    print("NEXT: cargo build")
    print("      cargo test --release")
    print("      cargo run --release -- build /tmp/nominal_a5.gol")


if __name__ == "__main__":
    main()
