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
