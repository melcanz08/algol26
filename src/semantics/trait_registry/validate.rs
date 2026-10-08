// src/semantics/trait_registry/validate.rs

use super::*;

impl TraitRegistry {
    pub fn validate_impl(&self, impl_block: &ImplBlock) -> Result<(), String> {
        let Some(trait_name) = &impl_block.trait_name else {
            // Inherent impls have no trait to validate against. ADR 0033.
            return Ok(());
        };

        // Reject impls of traits that were never declared.
        let trait_decl = self
            .traits
            .get(trait_name)
            .ok_or_else(|| format!("Impl references undefined trait '{}'", trait_name))?;

        let required_methods = &trait_decl.methods;
        let provided_methods: HashMap<&String, &FunctionDecl> =
            impl_block.methods.iter().map(|m| (&m.name, m)).collect();

        // ADR 0034: for a generic impl (`impl<T> Showable for Pair<T>`),
        // `Self` must substitute to the *full* target type, including
        // its type arguments. The AST carries the base name and the
        // args separately; reconstruct the display form here.
        let target_str = if impl_block.target_type_args.is_empty() {
            impl_block.target_type.clone()
        } else {
            let args: Vec<String> = impl_block
                .target_type_args
                .iter()
                .map(|t| t.to_string_rep())
                .collect();
            format!("{}<{}>", impl_block.target_type, args.join(", "))
        };

        for required in required_methods {
            if let Some(provided) = provided_methods.get(&required.name) {
                self.validate_method_signature(required, provided, &target_str)?;
            } else {
                let has_default = self
                    .default_methods
                    .get(trait_name)
                    .is_some_and(|defaults| defaults.contains_key(&required.name));

                if !has_default {
                    return Err(format!(
                        "Impl for trait '{}' is missing method '{}'",
                        trait_name, required.name
                    ));
                }
            }
        }

        Ok(())
    }
    pub(super) fn validate_method_signature(
        &self,
        required: &TraitMethod,
        provided: &FunctionDecl,
        target_type: &str,
    ) -> Result<(), String> {
        // Check parameter count
        if required.params.len() != provided.params.len() {
            return Err(format!(
                "Method '{}' has wrong number of parameters: expected {}, got {}",
                required.name,
                required.params.len(),
                provided.params.len()
            ));
        }

        // ADR 0033: substitute `Self` with the impl's target type before
        // comparing. The trait's declared signature may contain `Self` at
        // any depth (`Self`, `Borrow<Self>`, `MutBorrow<Self>`, ...); a
        // bare `Self` check was the previous behavior and only handled
        // the outer case.
        let substitute = |s: String| s.replace("Self", target_type);

        // Check parameter types
        for (i, ((_req_name, req_type), (_prov_name, prov_type))) in
            required.params.iter().zip(&provided.params).enumerate()
        {
            if let (Some(req_t), Some(prov_t)) = (req_type, prov_type) {
                let req_str = substitute(req_t.to_string_rep());
                let prov_str = prov_t.to_string_rep();

                if req_str != prov_str {
                    return Err(format!(
                        "Method '{}' parameter {} type mismatch: expected {}, got {}",
                        required.name, i, req_str, prov_str
                    ));
                }
            }
        }

        // Check return type
        if let (Some(req_ret), Some(prov_ret)) = (&required.return_type, &provided.return_type) {
            let req_str = substitute(req_ret.to_string_rep());
            let prov_str = prov_ret.to_string_rep();

            if req_str != prov_str {
                return Err(format!(
                    "Method '{}' return type mismatch: expected {}, got {}",
                    required.name, req_str, prov_str
                ));
            }
        }

        Ok(())
    }
}

// ─── ADR 0038: object safety ────────────────────────────────────────

use crate::common::diagnostics::{CompileError, ErrorCode};
use crate::common::span::Span;
use crate::frontend::ast::{TraitMethod, TypeSyntax};

impl TraitRegistry {
    /// ADR 0038. A trait is usable as `&dyn Trait` iff every method
    /// has a `&Self` or `&mut Self` receiver. There are no generic
    /// methods and no associated types in v1, so those clauses of
    /// Rust's object-safety rule are structurally vacuous here and
    /// are not checked.
    ///
    /// The gate fires as soon as `dyn Trait` is resolved, not later
    /// at the coercion site. The ADR calls for the coercion site;
    /// resolving earlier produces the same diagnostic at an earlier
    /// position, which is strictly better UX and does not require
    /// threading span information into the coercion path.
    pub fn ensure_object_safe(
        &self,
        trait_name: &str,
        span: Span,
    ) -> std::result::Result<(), CompileError> {
        let Some(trait_decl) = self.traits.get(trait_name) else {
            return Err(CompileError::at(
                span,
                &format!("unknown trait `{}`", trait_name),
                ErrorCode::E0003,
            )
            .with_suggestion("check the trait name and ensure it is in scope"));
        };

        for method in &trait_decl.methods {
            ensure_method_is_object_safe(trait_name, method, span)?;
        }
        Ok(())
    }
}

/// One method's contribution to its trait's object safety.
///
/// The receiver convention is `self: &Self` or `self: &mut Self`,
/// written as the first parameter in the trait declaration (see the
/// `Shape` example in ADR 0038 and ADR 0033). Anything else — no
/// receiver, a differently-named first parameter, a bare `Self`, or
/// an unrelated type — makes the method undispatchable through a
/// fat pointer, and hence the trait non-object-safe.
fn ensure_method_is_object_safe(
    trait_name: &str,
    method: &TraitMethod,
    span: Span,
) -> std::result::Result<(), CompileError> {
    let Some((first_name, first_ty)) = method.params.first() else {
        return Err(non_object_safe(
            trait_name,
            &method.name,
            "has no receiver parameter",
            span,
        ));
    };
    if first_name != "self" {
        return Err(non_object_safe(
            trait_name,
            &method.name,
            &format!("first parameter is named `{}`, expected `self`", first_name),
            span,
        ));
    }
    let Some(first_ty) = first_ty else {
        return Err(non_object_safe(
            trait_name,
            &method.name,
            "receiver `self` has no type annotation; write `self: &Self` \
             or `self: &mut Self`",
            span,
        ));
    };
    if !is_borrowed_self(first_ty) {
        return Err(non_object_safe(
            trait_name,
            &method.name,
            "receiver must be `&Self` or `&mut Self` — a `dyn Trait` \
             cannot be dispatched through a by-value or consuming receiver",
            span,
        ));
    }
    Ok(())
}

/// True for `&Self` or `&mut Self` as parsed by `parse_type_syntax`:
/// `Generic { name: "Borrow" | "MutBorrow", args: [Named("Self")] }`.
/// Case-insensitive on the constructor name to match `to_type`'s
/// tolerance elsewhere in the analyzer.
fn is_borrowed_self(ty: &TypeSyntax) -> bool {
    let TypeSyntax::Generic { name, args } = ty else {
        return false;
    };
    if args.len() != 1 {
        return false;
    }
    let lower = name.to_lowercase();
    let is_borrow = lower == "borrow" || lower == "mutborrow" || lower == "mut_borrow";
    if !is_borrow {
        return false;
    }
    matches!(&args[0], TypeSyntax::Named(n) if n == "Self")
}

fn non_object_safe(trait_name: &str, method_name: &str, reason: &str, span: Span) -> CompileError {
    CompileError::at(
        span,
        &format!(
            "trait `{}` is not object-safe: method `{}` {}",
            trait_name, method_name, reason
        ),
        ErrorCode::E0002,
    )
    .with_suggestion(
        "traits used as `&dyn Trait` may only declare methods with \
         `&Self` or `&mut Self` receivers",
    )
}
