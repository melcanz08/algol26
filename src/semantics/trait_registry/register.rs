// src/semantics/trait_registry/register.rs

use super::*;

impl TraitRegistry {
    pub fn register_trait(&mut self, trait_decl: TraitDecl) {
        self.traits.insert(trait_decl.name.clone(), trait_decl);
    }
    pub fn register_impl(&mut self, impl_block: ImplBlock) {
        // Inherent impls (`impl User`) have no trait; the trait registry
        // only tracks trait impls. ADR 0033.
        let Some(trait_name) = impl_block.trait_name.clone() else {
            return;
        };

        let key = (trait_name.clone(), impl_block.target_type.clone());

        // A generic impl is signalled by either type params on the
        // impl block or type-arg syntax on the target. The parser
        // stores `Pair<T>` as `target_type = "Pair"` and
        // `target_type_args = [Named("T")]`, so checking the string
        // for `<` is not enough — it never contains one.
        let is_generic =
            !impl_block.type_params.is_empty() || !impl_block.target_type_args.is_empty();

        if is_generic {
            let pattern = if impl_block.target_type_args.is_empty() {
                TypePattern::Concrete(impl_block.target_type.clone())
            } else {
                let args: Vec<TypePattern> = impl_block
                    .target_type_args
                    .iter()
                    .map(|a| self.parse_type_pattern_syntax(a))
                    .collect();
                TypePattern::Generic(impl_block.target_type.clone(), args)
            };
            self.generic_impls.push(GenericImpl {
                trait_name,
                type_pattern: pattern,
                methods: impl_block.methods.clone(),
                type_params: impl_block.type_params.clone(),
            });
        } else {
            self.impls.insert(key, impl_block);
        }
    }

    /// Convert a `TypeSyntax` into a `TypePattern`. Recurses through
    /// generic args so `Pair<List<T>>` produces a nested pattern.
    /// `Named("T")` (single uppercase letter) becomes `Concrete("T")`,
    /// which `type_matches_pattern` treats as a wildcard.
    fn parse_type_pattern_syntax(&self, syntax: &crate::frontend::ast::TypeSyntax) -> TypePattern {
        use crate::frontend::ast::TypeSyntax;
        match syntax {
            TypeSyntax::Named(name) => TypePattern::Concrete(name.clone()),
            TypeSyntax::Generic { name, args } => {
                let args: Vec<TypePattern> = args
                    .iter()
                    .map(|a| self.parse_type_pattern_syntax(a))
                    .collect();
                TypePattern::Generic(name.clone(), args)
            }
            TypeSyntax::Unknown => TypePattern::Any,
        }
    }
    pub fn register_default_method(&mut self, trait_name: &str, method: FunctionDecl) {
        self.default_methods
            .entry(trait_name.to_string())
            .or_default()
            .insert(method.name.clone(), method);
    }
}
