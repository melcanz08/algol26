// src/semantics/trait_registry/validate.rs

use super::*;

impl TraitRegistry {
    pub fn validate_impl(&self, impl_block: &ImplBlock) -> Result<(), String> {
        let trait_name = &impl_block.trait_name;

        // Reject impls of traits that were never declared. Without this,
        // `impl MadeUpTrait for Int { ... }` would silently be accepted.
        let trait_decl = self.traits.get(trait_name).ok_or_else(|| {
            format!(
                "Impl references undefined trait '{}'",
                trait_name
            )
        })?;

        let required_methods = &trait_decl.methods;
        let provided_methods: HashMap<&String, &FunctionDecl> =
            impl_block.methods.iter().map(|m| (&m.name, m)).collect();

        for required in required_methods {
            if let Some(provided) = provided_methods.get(&required.name) {
                self.validate_method_signature(required, provided)?;
            } else {
                // A missing method is permitted only if there's a default
                // implementation registered for it.
                let has_default = self
                    .default_methods
                    .get(trait_name)
                    .map_or(false, |defaults| defaults.contains_key(&required.name));

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

        // Check parameter types
        for (i, ((_req_name, req_type), (_prov_name, prov_type))) in
            required.params.iter().zip(&provided.params).enumerate()
        {
            if let (Some(req_t), Some(prov_t)) = (req_type, prov_type) {
                let req_str = req_t.to_string_rep();
                let prov_str = prov_t.to_string_rep();

                if req_str != prov_str && req_str != "Self" {
                    return Err(format!(
                        "Method '{}' parameter {} type mismatch: expected {}, got {}",
                        required.name, i, req_str, prov_str
                    ));
                }
            }
        }

        // Check return type
        if let (Some(req_ret), Some(prov_ret)) = (&required.return_type, &provided.return_type) {
            let req_str = req_ret.to_string_rep();
            let prov_str = prov_ret.to_string_rep();

            if req_str != prov_str && req_str != "Self" {
                return Err(format!(
                    "Method '{}' return type mismatch: expected {}, got {}",
                    required.name, req_str, prov_str
                ));
            }
        }

        Ok(())
    }
}