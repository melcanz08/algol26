// src/semantics/trait_registry.rs
//
// Trait and impl registration, method lookup, and generic pattern
// matching.
//
// Implemented:
//   - trait declaration and registration
//   - concrete impls
//   - generic impls with single-uppercase-letter type variables as
//     wildcards (e.g. `impl Display for List<T>`)
//   - default method registration and validation
//   - validate_impl rejects unknown trait names
//
// Not implemented:
//   - supertrait syntax and inheritance (the always-empty field was
//     removed; supertrait support returns with the feature)
//   - associated types
//   - coherence checking beyond the current validate_impl
//   - formal generic constraint checking. Single-uppercase-letter
//     patterns act as wildcards; they do not enforce that the bound
//     type variable is instantiated consistently across a program.

use crate::common::types::Type;
use crate::frontend::ast::{FunctionDecl, ImplBlock, TraitDecl, TraitMethod};
use std::collections::HashMap;

mod register;
mod resolve;
mod validate;
#[cfg(test)]
mod tests;

#[derive(Debug, Clone)]
pub struct TraitRegistry {
    pub(super) traits: HashMap<String, TraitDecl>,
    pub(super) impls: HashMap<(String, String), ImplBlock>,
    // NEW: Default methods
    pub(super) default_methods: HashMap<String, HashMap<String, FunctionDecl>>,
    // NEW: Generic impls
    pub(super)generic_impls: Vec<GenericImpl>,
}
#[derive(Debug, Clone)]
pub(super) struct GenericImpl {
    trait_name: String,
    type_pattern: TypePattern,
    methods: Vec<FunctionDecl>,
}
#[derive(Debug, Clone)]
pub(super) enum TypePattern {
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
