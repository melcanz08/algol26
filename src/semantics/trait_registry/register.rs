// src/semantics/trait_registry/register.rs

use super::*;

impl TraitRegistry {
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
    pub(super) fn parse_type_pattern(&self, type_str: &str) -> TypePattern {
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
    pub fn register_default_method(&mut self, trait_name: &str, method: FunctionDecl) {
        self.default_methods
            .entry(trait_name.to_string())
            .or_default()
            .insert(method.name.clone(), method);
    }
}