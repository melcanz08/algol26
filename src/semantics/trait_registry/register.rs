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

        // A generic impl is signalled by any of:
        //   - type params on the impl block (`impl<T> ... for List<T>`)
        //   - type-arg syntax on the target (`Pair<Int>`)
        //   - a `<` in the target_type string (callers that
        //     construct ImplBlock directly, e.g. tests, may
        //     leave the args embedded in the string)
        let target_has_angle = impl_block.target_type.contains('<');
        let is_generic = !impl_block.type_params.is_empty()
            || !impl_block.target_type_args.is_empty()
            || target_has_angle;

        if is_generic {
            let pattern = if !impl_block.target_type_args.is_empty() {
                let args: Vec<TypePattern> = impl_block
                    .target_type_args
                    .iter()
                    .map(|a| self.parse_type_pattern_syntax(a))
                    .collect();
                TypePattern::Generic(impl_block.target_type.clone(), args)
            } else if target_has_angle {
                self.parse_type_pattern(&impl_block.target_type)
            } else {
                TypePattern::Concrete(impl_block.target_type.clone())
            };
            self.generic_impls.push(GenericImpl {
                trait_name,
                type_pattern: pattern,
                methods: impl_block.methods.clone(),
                type_params: impl_block.type_params.clone(),
                where_clauses: impl_block.where_clauses.clone(),
            });
        } else {
            self.impls.insert(key, impl_block);
        }
    }

    /// Fallback for callers that construct `ImplBlock` with the
    /// generic form embedded in `target_type` as a string
    /// (`"List<T>"`). The parser produces the split form
    /// (`target_type = "List"`, `target_type_args = [Named("T")]`),
    /// but tests and ad-hoc callers may not.
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
