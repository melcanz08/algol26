// src/trait_registry.rs - Trait Registry for method resolution and bounds checking

use std::collections::HashMap;
use crate::frontend::ast::{TraitDecl, TraitMethod, ImplBlock, FunctionDecl};
use crate::common::types::Type;

#[derive(Debug, Clone)]
pub struct TraitRegistry {
    /// Trait name -> TraitDecl
    pub traits: HashMap<String, TraitDecl>,
    /// (trait_name, target_type) -> ImplBlock
    pub impls: HashMap<(String, String), ImplBlock>,
}

impl TraitRegistry {
    pub fn new() -> Self {
        TraitRegistry {
            traits: HashMap::new(),
            impls: HashMap::new(),
        }
    }
    
    /// Register a trait declaration
    pub fn register_trait(&mut self, trait_decl: TraitDecl) {
        self.traits.insert(trait_decl.name.clone(), trait_decl);
    }
    
    /// Register an impl block
    pub fn register_impl(&mut self, impl_block: ImplBlock) {
        let key = (impl_block.trait_name.clone(), impl_block.target_type.clone());
        self.impls.insert(key, impl_block);
    }
    
    /// Check if a type implements a trait
    pub fn type_implements_trait(&self, type_: &Type, trait_name: &str) -> bool {
        let type_name = type_.to_string();
        let key = (trait_name.to_string(), type_name);
        self.impls.contains_key(&key)
    }
    
    /// Get the impl block for a type and trait
    pub fn get_impl(&self, type_: &Type, trait_name: &str) -> Option<&ImplBlock> {
        let type_name = type_.to_string();
        let key = (trait_name.to_string(), type_name);
        self.impls.get(&key)
    }
    
    /// Resolve a method call on a type
    pub fn resolve_method(&self, type_: &Type, method_name: &str) -> Option<&FunctionDecl> {
        let type_name = type_.to_string();
        
        // Search all impls for this type that provide the method
        for ((_trait_name, target_type), impl_block) in &self.impls {
            if target_type == &type_name {
                for method in &impl_block.methods {
                    if method.name == method_name {
                        return Some(method);
                    }
                }
            }
        }
        None
    }
    
    /// Check if a trait exists
    pub fn trait_exists(&self, trait_name: &str) -> bool {
        self.traits.contains_key(trait_name)
    }
    
    /// Get all methods required by a trait
    pub fn get_trait_methods(&self, trait_name: &str) -> Option<&Vec<TraitMethod>> {
        self.traits.get(trait_name).map(|t| &t.methods)
    }
    
    /// Verify that an impl block implements all required methods
    pub fn validate_impl(&self, impl_block: &ImplBlock) -> Result<(), String> {
        let trait_name = &impl_block.trait_name;
        
        if let Some(trait_decl) = self.traits.get(trait_name) {
            let required_methods = &trait_decl.methods;
            let provided_methods: Vec<&String> = impl_block.methods.iter()
                .map(|m| &m.name)
                .collect();
            
            for required in required_methods {
                if !provided_methods.contains(&&required.name) {
                    return Err(format!(
                        "Impl for trait '{}' is missing method '{}'",
                        trait_name, required.name
                    ));
                }
            }
        }
        
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_register_trait() {
        let mut registry = TraitRegistry::new();
        let trait_decl = TraitDecl {
            name: "Comparable".to_string(),
            methods: vec![TraitMethod {
                name: "compare".to_string(),
                params: vec![("other".to_string(), "Self".to_string())],
                return_type: Some("Int".to_string()),
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
                return_type: Some("Int".to_string()),
            }],
        };
        registry.register_trait(trait_decl);
        
        let impl_block = ImplBlock {
            trait_name: "Comparable".to_string(),
            target_type: "Int".to_string(),
            methods: vec![FunctionDecl {
                name: "compare".to_string(),
                params: vec![],
                return_type: Some("Int".to_string()),
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
}