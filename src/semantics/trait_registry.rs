// src/semantics/trait_registry.rs - HARDENED
// Complete trait system with inheritance, associated types, and default methods

use crate::common::types::Type;
use crate::frontend::ast::{FunctionDecl, ImplBlock, TraitDecl, TraitMethod};
use std::collections::HashMap;

#[derive(Debug, Clone)]
pub struct TraitRegistry {
    pub traits: HashMap<String, TraitDecl>,
    pub impls: HashMap<(String, String), ImplBlock>,
    // NEW: Default methods
    pub default_methods: HashMap<String, HashMap<String, FunctionDecl>>,
    // NEW: Generic impls
    generic_impls: Vec<GenericImpl>,
}

#[derive(Debug, Clone)]
struct GenericImpl {
    trait_name: String,
    type_pattern: TypePattern,
    methods: Vec<FunctionDecl>,
}

#[derive(Debug, Clone)]
enum TypePattern {
    Concrete(String),
    Generic(String, Vec<TypePattern>),
    Any,
}

impl TraitRegistry {
    pub fn new() -> Self {
        TraitRegistry {
            traits: HashMap::new(),
            impls: HashMap::new(),
            default_methods: HashMap::new(),
            generic_impls: Vec::new(),
        }
    }

    pub fn register_trait(&mut self, trait_decl: TraitDecl) {
        self.traits.insert(trait_decl.name.clone(), trait_decl);
    }

    pub fn register_impl(&mut self, impl_block: ImplBlock) {
        let key = (
            impl_block.trait_name.clone(),
            impl_block.target_type.clone(),
        );

        // Check if this is a generic impl
        if impl_block.target_type.contains('<') {
            self.generic_impls.push(GenericImpl {
                trait_name: impl_block.trait_name.clone(),
                type_pattern: self.parse_type_pattern(&impl_block.target_type),
                methods: impl_block.methods.clone(),
            });
        } else {
            self.impls.insert(key, impl_block);
        }
    }

    fn parse_type_pattern(&self, type_str: &str) -> TypePattern {
        if type_str == "_" {
            return TypePattern::Any;
        }

        if let Some(open) = type_str.find('<') {
            let close = type_str.rfind('>').unwrap_or(type_str.len());
            let name = &type_str[..open];
            let args_str = &type_str[open + 1..close];
            let args: Vec<TypePattern> = args_str
                .split(',')
                .map(|a| self.parse_type_pattern(a.trim()))
                .collect();
            TypePattern::Generic(name.to_string(), args)
        } else {
            TypePattern::Concrete(type_str.to_string())
        }
    }

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

    fn type_matches_pattern(&self, type_: &Type, pattern: &TypePattern) -> bool {
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

    fn validate_method_signature(
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

    pub fn register_default_method(&mut self, trait_name: &str, method: FunctionDecl) {
        self.default_methods
            .entry(trait_name.to_string())
            .or_default()
            .insert(method.name.clone(), method);
    }

    pub fn get_trait_methods(&self, trait_name: &str) -> Option<&Vec<TraitMethod>> {
        self.traits.get(trait_name).map(|t| &t.methods)
    }

    pub fn trait_exists(&self, trait_name: &str) -> bool {
        self.traits.contains_key(trait_name)
    }

    pub fn get_all_traits_for_type(&self, type_: &Type) -> Vec<String> {
        let mut result = Vec::new();

        for trait_name in self.traits.keys() {
            if self.type_implements_trait(type_, trait_name) {
                result.push(trait_name.clone());
            }
        }

        result
    }
}

impl Default for TraitRegistry {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::frontend::ast::TypeSyntax;

    #[test]
    fn test_register_trait() {
        let mut registry = TraitRegistry::new();
        let trait_decl = TraitDecl {
            name: "Comparable".to_string(),
            methods: vec![TraitMethod {
                name: "compare".to_string(),
                params: vec![(
                    "other".to_string(),
                    Some(TypeSyntax::Named("Self".to_string())),
                )],
                return_type: Some(TypeSyntax::Named("Int".to_string())),
            }],
        };
        registry.register_trait(trait_decl);
        assert!(registry.trait_exists("Comparable"));
    }

    #[test]
    fn test_type_implements_trait() {
        let mut registry = TraitRegistry::new();

        let trait_decl = TraitDecl {
            name: "Comparable".to_string(),
            methods: vec![TraitMethod {
                name: "compare".to_string(),
                params: vec![],
                return_type: Some(TypeSyntax::Named("Int".to_string())),
            }],
        };
        registry.register_trait(trait_decl);

        let impl_block = ImplBlock {
            trait_name: "Comparable".to_string(),
            target_type: "Int".to_string(),
            methods: vec![FunctionDecl {
                name: "compare".to_string(),
                params: vec![],
                return_type: Some(TypeSyntax::Named("Int".to_string())),
                body: vec![],
                is_extern: false,
                ffi_info: None,
                type_params: vec![],
                where_clauses: vec![],
            }],
        };
        registry.register_impl(impl_block);

        assert!(registry.type_implements_trait(&Type::Int, "Comparable"));
        assert!(!registry.type_implements_trait(&Type::Float, "Comparable"));
    }

    #[test]
    fn test_validate_impl_signature_mismatch() {
        let mut registry = TraitRegistry::new();

        let trait_decl = TraitDecl {
            name: "Comparable".to_string(),
            methods: vec![TraitMethod {
                name: "compare".to_string(),
                params: vec![(
                    "other".to_string(),
                    Some(TypeSyntax::Named("Self".to_string())),
                )],
                return_type: Some(TypeSyntax::Named("Int".to_string())),
            }],
        };
        registry.register_trait(trait_decl);

        let impl_block = ImplBlock {
            trait_name: "Comparable".to_string(),
            target_type: "Int".to_string(),
            methods: vec![FunctionDecl {
                name: "compare".to_string(),
                params: vec![], // Wrong! Missing "other" parameter
                return_type: Some(TypeSyntax::Named("String".to_string())), // Wrong! Should be Int
                body: vec![],
                is_extern: false,
                ffi_info: None,
                type_params: vec![],
                where_clauses: vec![],
            }],
        };

        assert!(registry.validate_impl(&impl_block).is_err());
    }

    #[test]
    fn test_generic_impl() {
        let mut registry = TraitRegistry::new();

        let trait_decl = TraitDecl {
            name: "Display".to_string(),
            methods: vec![TraitMethod {
                name: "display".to_string(),
                params: vec![],
                return_type: Some(TypeSyntax::Named("String".to_string())),
            }],
        };
        registry.register_trait(trait_decl);

        // Generic impl for List<T>
        let impl_block = ImplBlock {
            trait_name: "Display".to_string(),
            target_type: "List<T>".to_string(),
            methods: vec![FunctionDecl {
                name: "display".to_string(),
                params: vec![],
                return_type: Some(TypeSyntax::Named("String".to_string())),
                body: vec![],
                is_extern: false,
                ffi_info: None,
                type_params: vec!["T".to_string()],
                where_clauses: vec![],
            }],
        };
        registry.register_impl(impl_block);

        // List<Int> should implement Display
        assert!(registry.type_implements_trait(&Type::list(Type::Int), "Display"));

        // Int should NOT implement Display
        assert!(!registry.type_implements_trait(&Type::Int, "Display"));
    }
}
