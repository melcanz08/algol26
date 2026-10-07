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
                if name.len() == 1
                    && name
                        .chars()
                        .next()
                        .map(|c| c.is_uppercase())
                        .unwrap_or(false)
                {
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
                    // User-defined records: `Pair<Int>` matches the
                    // pattern `Pair<T>` when the outer names agree
                    // and each concrete type argument matches its
                    // pattern. This was missing before the UFCS
                    // mangling change; the previous mangling hid the
                    // gap because trait impls on generic records
                    // masqueraded as inherent methods under the
                    // name `Pair_method`.
                    Type::Record(rec_name, rec_args) => {
                        name == rec_name
                            && (args.is_empty()
                                || (args.len() == rec_args.len()
                                    && args
                                        .iter()
                                        .zip(rec_args.iter())
                                        .all(|(p, c)| self.type_matches_pattern(c, p))))
                    }
                    // Generic instantiation: same rule as records.
                    Type::Generic {
                        name: g_name,
                        args: g_args,
                    } => {
                        name == g_name
                            && (args.is_empty()
                                || (args.len() == g_args.len()
                                    && args
                                        .iter()
                                        .zip(g_args.iter())
                                        .all(|(p, c)| self.type_matches_pattern(c, p))))
                    }
                    _ => false,
                }
            }
        }
    }
    /// Trait-scoped method lookup. Unlike `resolve_method`, which
    /// finds a method on the type across *all* traits, this only
    /// considers the named trait. Used by UFCS (`Trait::method(u, x)`)
    /// to disambiguate when two traits provide the same method name
    /// for the same type.
    pub fn resolve_trait_method_for_trait(
        &self,
        trait_name: &str,
        type_: &Type,
        method_name: &str,
    ) -> Option<&FunctionDecl> {
        let type_name = type_.to_string();

        // Concrete impls: keyed by (trait_name, type_name).
        let key = (trait_name.to_string(), type_name.clone());
        if let Some(impl_block) = self.impls.get(&key) {
            for method in &impl_block.methods {
                if method.name == method_name {
                    return Some(method);
                }
            }
        }

        // Generic impls: filter by trait name first, then pattern.
        for generic_impl in &self.generic_impls {
            if generic_impl.trait_name != trait_name {
                continue;
            }
            if self.type_matches_pattern(type_, &generic_impl.type_pattern) {
                for method in &generic_impl.methods {
                    if method.name == method_name {
                        return Some(method);
                    }
                }
            }
        }

        // Default methods: trait name is the outer key, so no
        // cross-trait leakage is possible.
        if self.type_implements_trait(type_, trait_name) {
            if let Some(methods) = self.default_methods.get(trait_name) {
                if let Some(method) = methods.get(method_name) {
                    return Some(method);
                }
            }
        }

        None
    }

    /// All trait impls that provide `method_name` for `type_`,
    /// each paired with the trait that owns it. Used by the
    /// analyzer to detect cross-trait ambiguity at a call site:
    /// two or more entries mean the bare `x.method()` form must be
    /// rejected in favor of UFCS.
    pub fn methods_for(
        &self,
        type_: &Type,
        method_name: &str,
    ) -> Vec<(String, &FunctionDecl, Vec<String>, Vec<WhereClause>)> {
        let type_name = type_.to_string();
        let mut result: Vec<(String, &FunctionDecl, Vec<String>, Vec<WhereClause>)> = Vec::new();

        for ((trait_name, target_type), impl_block) in &self.impls {
            if target_type == &type_name {
                for method in &impl_block.methods {
                    if method.name == method_name {
                        result.push((trait_name.clone(), method, Vec::new(), Vec::new()));
                    }
                }
            }
        }

        for generic_impl in &self.generic_impls {
            if self.type_matches_pattern(type_, &generic_impl.type_pattern) {
                for method in &generic_impl.methods {
                    if method.name == method_name {
                        result.push((
                            generic_impl.trait_name.clone(),
                            method,
                            generic_impl.type_params.clone(),
                            generic_impl.where_clauses.clone(),
                        ));
                    }
                }
            }
        }

        for (trait_name, methods) in &self.default_methods {
            if self.type_implements_trait(type_, trait_name) {
                if let Some(method) = methods.get(method_name) {
                    result.push((trait_name.clone(), method, Vec::new(), Vec::new()));
                }
            }
        }

        result
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
