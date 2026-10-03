#!/usr/bin/env python3
"""
A5: match exhaustiveness for user enums.

Extends check_match_exhaustiveness to require that a match on
Type::Enum covers every variant, unless a wildcard/binding fallback
is present.
"""

from pathlib import Path

PATH = Path("src/semantics/analyzer/expr.rs")

OLD = '''    pub(super) fn check_match_exhaustiveness(
        &self,
        value_type: &Type,
        cases: &[MatchCaseExpr],
    ) -> Result<()> {
        let mut has_some = false;
        let mut has_none = false;
        let mut has_ok = false;
        let mut has_error = false;
        let mut has_true = false;
        let mut has_false = false;
        let mut has_fallback = false;

        for case in cases {
            // A guarded arm's coverage depends on its guard, which we
            // cannot decide here. Skip.
            let pattern = match &case.pattern {
                Pattern::Guarded { .. } => continue,
                p => p,
            };
            match pattern {
                Pattern::Some(_) | Pattern::SomeNested(_) => has_some = true,
                Pattern::None => has_none = true,
                Pattern::Ok(_) | Pattern::OkNested(_) => has_ok = true,
                Pattern::Error(_) | Pattern::ErrorNested(_) => has_error = true,
                Pattern::Literal(Expr {
                    kind: ExprKind::Bool(true, _),
                    ..
                }) => has_true = true,
                Pattern::Literal(Expr {
                    kind: ExprKind::Bool(false, _),
                    ..
                }) => has_false = true,
                Pattern::Wildcard | Pattern::Binding(_) => has_fallback = true,
                _ => {}
            }
        }

        if has_fallback {
            return Ok(());
        }

        match value_type {
            Type::Option(_) => {
                if !(has_some && has_none) {
                    return Err(CompileError::at(self.current_span, "match on Option must handle both Some and None, or have a `case _` fallback", ErrorCode::E0002)
                    .with_suggestion(
                        "Add a `case None` arm, or a `case _` arm for the unmatched variant",
                    ));
                }
            }
            Type::Result { .. } => {
                if !(has_ok && has_error) {
                    return Err(CompileError::at(self.current_span, "match on Result must handle both Ok and Error, or have a `case _` fallback", ErrorCode::E0002)
                    .with_suggestion(
                        "Add an `case Error(e)` arm, or a `case _` arm for the unmatched variant",
                    ));
                }
            }
            Type::Bool => {
                if !(has_true && has_false) {
                    return Err(CompileError::at(self.current_span, "match on Bool must handle both true and false, or have a `case _` fallback", ErrorCode::E0002)
                    .with_suggestion(
                        "Add both `case true` and `case false`, or a `case _` arm",
                    ));
                }
            }
            _ => {}
        }
        Ok(())
    }'''

NEW = '''    pub(super) fn check_match_exhaustiveness(
        &self,
        value_type: &Type,
        cases: &[MatchCaseExpr],
    ) -> Result<()> {
        let mut has_some = false;
        let mut has_none = false;
        let mut has_ok = false;
        let mut has_error = false;
        let mut has_true = false;
        let mut has_false = false;
        let mut has_fallback = false;
        // ADR 0030: user enum variant coverage. A HashSet because
        // the domain is only known from `value_type` — a match on
        // `Day` populates `covered` with `Monday`, `Tuesday`, ...
        // and the check compares against `variants`.
        let mut covered_variants: HashSet<String> = HashSet::new();

        for case in cases {
            // A guarded arm's coverage depends on its guard, which we
            // cannot decide here. Skip.
            let pattern = match &case.pattern {
                Pattern::Guarded { .. } => continue,
                p => p,
            };
            match pattern {
                Pattern::Some(_) | Pattern::SomeNested(_) => has_some = true,
                Pattern::None => has_none = true,
                Pattern::Ok(_) | Pattern::OkNested(_) => has_ok = true,
                Pattern::Error(_) | Pattern::ErrorNested(_) => has_error = true,
                Pattern::Literal(Expr {
                    kind: ExprKind::Bool(true, _),
                    ..
                }) => has_true = true,
                Pattern::Literal(Expr {
                    kind: ExprKind::Bool(false, _),
                    ..
                }) => has_false = true,
                Pattern::Wildcard | Pattern::Binding(_) => has_fallback = true,
                Pattern::Variant(name) => {
                    covered_variants.insert(name.clone());
                }
                _ => {}
            }
        }

        if has_fallback {
            return Ok(());
        }

        match value_type {
            Type::Option(_) => {
                if !(has_some && has_none) {
                    return Err(CompileError::at(self.current_span, "match on Option must handle both Some and None, or have a `case _` fallback", ErrorCode::E0002)
                    .with_suggestion(
                        "Add a `case None` arm, or a `case _` arm for the unmatched variant",
                    ));
                }
            }
            Type::Result { .. } => {
                if !(has_ok && has_error) {
                    return Err(CompileError::at(self.current_span, "match on Result must handle both Ok and Error, or have a `case _` fallback", ErrorCode::E0002)
                    .with_suggestion(
                        "Add an `case Error(e)` arm, or a `case _` arm for the unmatched variant",
                    ));
                }
            }
            Type::Bool => {
                if !(has_true && has_false) {
                    return Err(CompileError::at(self.current_span, "match on Bool must handle both true and false, or have a `case _` fallback", ErrorCode::E0002)
                    .with_suggestion(
                        "Add both `case true` and `case false`, or a `case _` arm",
                    ));
                }
            }
            // ADR 0030: user enum. Every variant must appear as a
            // `case VariantName`, or the match must have a wildcard
            // fallback. Reporting the *first* missing variant is
            // enough — the user adds one, recompiles, and the next
            // missing one appears. Listing all of them would be
            // noisier than useful for a feature where the fix is
            // usually one arm.
            Type::Enum { name: enum_name, variants, .. } => {
                for variant in variants {
                    if !covered_variants.contains(variant) {
                        return Err(CompileError::at(
                            self.current_span,
                            &format!(
                                "match on enum '{}' is missing a case for variant '{}'",
                                enum_name, variant
                            ),
                            ErrorCode::E0002,
                        )
                        .with_suggestion(&format!(
                            "Add `case {}`, or a `case _` fallback arm",
                            variant
                        )));
                    }
                }
            }
            _ => {}
        }
        Ok(())
    }'''

if not PATH.exists():
    print(f"ERROR: {PATH} not found.")
    raise SystemExit(1)

src = PATH.read_text()
if src.count(OLD) != 1:
    print(f"FAIL: matched {src.count(OLD)} times; expected 1.")
    print("-" * 60)
    print(OLD[:250])
    print("-" * 60)
    raise SystemExit(1)

PATH.write_text(src.replace(OLD, NEW, 1))
print(f"OK: patched {PATH}")
