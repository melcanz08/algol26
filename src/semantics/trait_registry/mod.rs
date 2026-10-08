// src/semantics/trait_registry/mod.rs
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

use crate::common::types::{TraitId, Type};
use crate::frontend::ast::{FunctionDecl, ImplBlock, TraitDecl, TraitMethod, WhereClause};
use std::collections::HashMap;

mod register;
mod resolve;
#[cfg(test)]
mod tests;
mod validate;

#[derive(Debug, Clone)]
pub struct TraitRegistry {
    pub(super) traits: HashMap<String, TraitDecl>,
    pub(super) impls: HashMap<(String, String), ImplBlock>,
    // NEW: Default methods
    pub(super) default_methods: HashMap<String, HashMap<String, FunctionDecl>>,
    // NEW: Generic impls
    pub(super) generic_impls: Vec<GenericImpl>,
    /// ADR 0038. Stable per-registry `TraitId` assignment, keyed by
    /// trait name. Assigned in `register_trait` in registration
    /// order, so the id set is deterministic for a given source.
    /// The reverse lookup (id -> declaration) is available by
    /// scanning `traits`, and is only needed by diagnostic code
    /// that already holds a `TraitId`.
    pub(super) trait_ids: HashMap<String, TraitId>,
    /// Next id to assign. Monotone; never decremented. Kept as a
    /// field rather than derived from `trait_ids` so that
    /// re-registration (a duplicate trait declaration in the same
    /// unit) preserves the first-assigned id rather than
    /// renumbering.
    pub(super) next_trait_id: u32,
}
#[derive(Debug, Clone)]
pub(super) struct GenericImpl {
    trait_name: String,
    type_pattern: TypePattern,
    methods: Vec<FunctionDecl>,
    /// Declared type parameters on the impl block
    /// (`impl<T> ... for Pair<T>` → `["T"]`). The analyzer uses
    /// these to unify the declared self against a concrete receiver
    /// and record a monomorphization instantiation.
    type_params: Vec<String>,
    /// `where` clauses on the impl block
    /// (`impl<T> Trait for List<T> where T: Ord`). Checked at
    /// method resolution against the receiver's concrete type
    /// arguments. See ADR 0025 bound enforcement.
    where_clauses: Vec<WhereClause>,
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
            trait_ids: HashMap::new(),
            next_trait_id: 0,
        }
    }
    pub fn get_trait_methods(&self, trait_name: &str) -> Option<&Vec<TraitMethod>> {
        self.traits.get(trait_name).map(|t| &t.methods)
    }
    /// ADR 0038 D4b. The trait's own declaration of `method_name`,
    /// without consulting any impl. Used by the analyzer to resolve
    /// a call through a `&dyn Trait` receiver.
    pub fn trait_method(&self, trait_name: &str, method_name: &str) -> Option<&TraitMethod> {
        self.traits
            .get(trait_name)?
            .methods
            .iter()
            .find(|m| m.name == method_name)
    }
    /// ADR 0038 D4b. Slot index of `method_name` in the trait's
    /// declaration order. `None` if the trait or the method is
    /// unknown. Shared by the analyzer and the IR builder so both
    /// agree on vtable layout.
    pub fn trait_method_slot(&self, trait_name: &str, method_name: &str) -> Option<usize> {
        self.traits
            .get(trait_name)?
            .methods
            .iter()
            .position(|m| m.name == method_name)
    }
    pub fn trait_exists(&self, trait_name: &str) -> bool {
        self.traits.contains_key(trait_name)
    }
    /// ADR 0038. Look up the `TraitId` assigned to `name`, or
    /// `None` if no such trait is registered. Returned by value
    /// because `TraitId` is `Copy`.
    pub fn resolve_trait_id(&self, name: &str) -> Option<TraitId> {
        self.trait_ids.get(name).copied()
    }
    /// Presentation-only reverse lookup: the trait name for a
    /// `TraitId`, or `None` if the id was not assigned by this
    /// registry. Used by diagnostics that need to name the trait
    /// given only the id.
    /// ADR 0038 D6. All trait-name -> `TraitId` assignments made so
    /// far. Used by the analyzer to forward the map to the IR
    /// builder so `Type::DynTrait` can be reconstructed from a bare
    /// annotation. Returned by value (cloned) because the registry
    /// keeps being used after this call.
    pub fn trait_ids(&self) -> std::collections::HashMap<String, TraitId> {
        self.trait_ids.clone()
    }
    /// ADR 0038 D6-2. Trait declarations, in registration order is
    /// not guaranteed (HashMap iteration); the vtable gather pass
    /// only needs name-keyed lookup for method names, so ordering
    /// is not significant.
    pub fn trait_decls(&self) -> Vec<TraitDecl> {
        self.traits.values().cloned().collect()
    }
    pub fn trait_name_for_id(&self, id: TraitId) -> Option<&str> {
        self.trait_ids
            .iter()
            .find_map(|(name, candidate)| (*candidate == id).then_some(name.as_str()))
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
