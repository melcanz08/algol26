// src/semantics/trait_registry/resolve.rs

use super::*;

impl TraitRegistry {
    pub fn type_implements_trait(&self, type_: &Type, trait_name: &str) -> bool {
        // Check concrete impls
        let type_name = type_.to_string();
        let key = (trait_name.to_string(), type_name.clone());
        if self.impls.contains_key(&key) {
            return true;
        }

        // Check generic impls
        for generic_impl in &self.generic_impls {
            if generic_impl.trait_name == trait_name
                && self.type_matches_pattern(type_, &generic_impl.type_pattern)
            {
                return true;
            }
        }
        false
    }
    pub(super) fn type_matches_pattern(&self, type_: &Type, pattern: &TypePattern) -> bool {
        match pattern {
            TypePattern::Any => true,
            TypePattern::Concrete(name) => {
                // Single uppercase letter = type variable (T, U, V), matches anything
                if name.len() == 1
                    && name
                        .chars()
                        .next()
                        .map(|c| c.is_uppercase())
                        .unwrap_or(false)
                {
                    return true;
                }
                type_.to_string() == *name
            }
            TypePattern::Generic(name, args) => {
                // Handle type variables (T, U, etc.) - they match anything
                if name.len() == 1 && name.chars().next().map(|c| c.is_uppercase()).unwrap_or(false) {
                    return true;
                }

                match type_ {
                    Type::List(inner) => {
                        name == "List"
                            && (args.is_empty()
                                || (args.len() == 1 && self.type_matches_pattern(inner, &args[0])))
                    }
                    Type::Option(inner) => {
                        name == "Option"
                            && (args.is_empty()
                                || (args.len() == 1 && self.type_matches_pattern(inner, &args[0])))
                    }
                    Type::Result { ok, error } => {
                        name == "Result"
                            && (args.len() == 2
                                && self.type_matches_pattern(ok, &args[0])
                                && self.type_matches_pattern(error, &args[1]))
                    }
                    _ => false,
                }
            }
        }
    }
    pub fn resolve_method(&self, type_: &Type, method_name: &str) -> Option<&FunctionDecl> {
        let type_name = type_.to_string();

        // Search concrete impls
        for ((_trait_name, target_type), impl_block) in &self.impls {
            if target_type == &type_name {
                for method in &impl_block.methods {
                    if method.name == method_name {
                        return Some(method);
                    }
                }
            }
        }

        // Search generic impls
        for generic_impl in &self.generic_impls {
            if self.type_matches_pattern(type_, &generic_impl.type_pattern) {
                for method in &generic_impl.methods {
                    if method.name == method_name {
                        return Some(method);
                    }
                }
            }
        }

        // Check default methods
        for (trait_name, methods) in &self.default_methods {
            if self.type_implements_trait(type_, trait_name) {
                if let Some(method) = methods.get(method_name) {
                    return Some(method);
                }
            }
        }

        None
    }
}