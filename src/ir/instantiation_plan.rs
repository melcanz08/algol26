// src/ir/instantiation_plan.rs
//
// Stage 3.2a of ADR 0013. The instantiation plan is the data
// structure the IR builder will consume (Stages 3.2b–3.2e) to
// emit concrete specializations of generic functions.
//
// See docs/decisions/0013-executable-ir-generic-invariant.md for
// the full design.

use crate::common::types::Type;
use crate::frontend::ast::ExprId;
use crate::semantics::analyzer::Instantiation;
use std::collections::HashMap;

/// A concrete specialization of a generic function, e.g.
/// `identity<Borrow<Float>>` mangled to `identity_Borrow_Float`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Specialization {
    /// Mangled name, unique per `(function, type_args)`. The IR
    /// builder emits this name in `Call` instructions targeting the
    /// specialization.
    pub mangled_name: String,

    /// The function's declared type parameter names, in declaration
    /// order. `type_args[i]` binds `type_params[i]`.
    pub type_params: Vec<String>,

    /// The concrete type arguments. Never contains `TypeVar` or
    /// `Unknown` for a specialization stored in a plan.
    pub type_args: Vec<Type>,
}

impl Specialization {
    /// Type parameter bindings as a `HashMap`, suitable for
    /// substitution during IR construction.
    pub fn bindings(&self) -> HashMap<String, Type> {
        self.type_params
            .iter()
            .cloned()
            .zip(self.type_args.iter().cloned())
            .collect()
    }
}

/// A generic call site's recorded instantiation. `type_args` may
/// contain `TypeVar` when the call is inside another generic
/// function's body; the IR builder substitutes under the enclosing
/// specialization's context before consulting the plan.
#[derive(Debug, Clone)]
pub struct CallSiteInstantiation {
    pub function: String,
    pub type_args: Vec<Type>,
}

/// The complete set of generic specializations a program needs,
/// plus the call-site table that maps each generic call to its
/// specialization.
#[derive(Debug, Clone, Default)]
pub struct InstantiationPlan {
    /// Concrete specializations, keyed by mangled name. Every
    /// `type_args` entry on every stored `Specialization` is
    /// concrete — `from_instantiations` skips symbolic entries.
    pub specializations: HashMap<String, Vec<Specialization>>,

    /// Every generic call site, keyed by the call expression's
    /// stable `ExprId`. Entries may be symbolic.
    pub call_sites: HashMap<ExprId, CallSiteInstantiation>,
}

impl InstantiationPlan {
    /// Build a plan from the analyzer's `Instantiation` records.
    ///
    /// Symbolic entries (containing `TypeVar`) are recorded on the
    /// call-site side but do not produce concrete specializations;
    /// closure under transitive generic calls is a follow-up step.
    pub fn from_instantiations(instantiations: &[Instantiation]) -> Self {
        let mut plan = Self::default();

        for inst in instantiations {
            plan.call_sites.insert(
                inst.call_site,
                CallSiteInstantiation {
                    function: inst.function.clone(),
                    type_args: inst.type_args.clone(),
                },
            );

            if !inst.type_args.iter().any(has_unresolved) {
                let spec = Specialization {
                    mangled_name: mangled_name(&inst.function, &inst.type_args),
                    type_params: inst.type_params.clone(),
                    type_args: inst.type_args.clone(),
                };
                plan.insert_specialization(spec);
            }
        }

        plan
    }

    fn insert_specialization(&mut self, spec: Specialization) {
        let list = self
            .specializations
            .entry(spec.mangled_name.clone())
            .or_default();
        if !list.iter().any(|s| s.type_args == spec.type_args) {
            list.push(spec);
        }
    }

    /// Look up a concrete specialization. Returns `None` if the
    /// plan does not contain the requested combination — which is
    /// a bug for a closed plan.
    pub fn specialization_for(
        &self,
        function: &str,
        type_args: &[Type],
    ) -> Option<&Specialization> {
        let key = mangled_name(function, type_args);
        self.specializations
            .get(&key)
            .and_then(|list| list.iter().find(|s| s.type_args == type_args))
    }

    /// Iterate every concrete specialization.
    pub fn all_specializations(&self) -> impl Iterator<Item = &Specialization> {
        self.specializations.values().flatten()
    }

    pub fn is_empty(&self) -> bool {
        self.specializations.is_empty() && self.call_sites.is_empty()
    }
}

/// Mangled name for a specialization: `identity_Borrow_Float`.
/// Callers must supply `type_args` in the function's declared type
/// parameter order.
pub fn mangled_name(function: &str, type_args: &[Type]) -> String {
    let parts: Vec<String> = type_args.iter().map(mangled_type_name).collect();
    format!("{}_{}", function, parts.join("_"))
}

fn mangled_type_name(ty: &Type) -> String {
    match ty {
        Type::Int => "Int".to_string(),
        Type::Float => "Float".to_string(),
        Type::String => "String".to_string(),
        Type::Bool => "Bool".to_string(),
        Type::List(inner) => format!("List_{}", mangled_type_name(inner)),
        Type::Option(inner) => format!("Option_{}", mangled_type_name(inner)),
        Type::Borrow(inner) => format!("Borrow_{}", mangled_type_name(inner)),
        Type::MutBorrow(inner) => format!("MutBorrow_{}", mangled_type_name(inner)),
        Type::Pointer(inner) => format!("Pointer_{}", mangled_type_name(inner)),
        Type::TypeVar(v) => v.clone(),
        _ => "Unknown".to_string(),
    }
}

/// True if the type contains `Unknown` or `TypeVar` anywhere.
/// Duplicated from `monomorphize.rs`; that file's version goes away
/// with `Monomorphizer` in Stage 3.2b.
fn has_unresolved(t: &Type) -> bool {
    match t {
        Type::Unknown | Type::TypeVar(_) => true,
        Type::List(inner)
        | Type::Option(inner)
        | Type::Pointer(inner)
        | Type::Borrow(inner)
        | Type::MutBorrow(inner)
        | Type::Channel(inner)
        | Type::Array(inner, _) => has_unresolved(inner),
        Type::Result { ok, error } => has_unresolved(ok) || has_unresolved(error),
        Type::Tuple(elems) => elems.iter().any(has_unresolved),
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn inst(
        call_site: u32,
        function: &str,
        type_params: &[&str],
        type_args: Vec<Type>,
    ) -> Instantiation {
        Instantiation {
            call_site: ExprId(call_site),
            function: function.to_string(),
            type_params: type_params.iter().map(|s| s.to_string()).collect(),
            type_args,
        }
    }

    #[test]
    fn empty_instantiations_produce_empty_plan() {
        let plan = InstantiationPlan::from_instantiations(&[]);
        assert!(plan.is_empty());
    }

    #[test]
    fn one_concrete_instantiation_creates_one_specialization() {
        let insts = vec![inst(42, "identity", &["T"], vec![Type::Int])];
        let plan = InstantiationPlan::from_instantiations(&insts);

        assert_eq!(plan.all_specializations().count(), 1);

        let spec = plan.specialization_for("identity", &[Type::Int]).unwrap();
        assert_eq!(spec.mangled_name, "identity_Int");
        assert_eq!(spec.type_args, vec![Type::Int]);

        let mut expected = HashMap::new();
        expected.insert("T".to_string(), Type::Int);
        assert_eq!(spec.bindings(), expected);
    }

    #[test]
    fn reference_type_arg_mangles_with_borrow_prefix() {
        let insts = vec![inst(
            42,
            "identity",
            &["T"],
            vec![Type::borrow(Type::Float)],
        )];
        let plan = InstantiationPlan::from_instantiations(&insts);

        let spec = plan
            .specialization_for("identity", &[Type::borrow(Type::Float)])
            .unwrap();
        assert_eq!(spec.mangled_name, "identity_Borrow_Float");
    }

    #[test]
    fn two_instantiations_of_same_function_produce_two_specializations() {
        let insts = vec![
            inst(10, "identity", &["T"], vec![Type::Int]),
            inst(20, "identity", &["T"], vec![Type::Float]),
        ];
        let plan = InstantiationPlan::from_instantiations(&insts);

        assert_eq!(plan.all_specializations().count(), 2);
        assert!(plan.specialization_for("identity", &[Type::Int]).is_some());
        assert!(plan
            .specialization_for("identity", &[Type::Float])
            .is_some());
    }

    #[test]
    fn duplicate_instantiations_are_deduplicated() {
        let insts = vec![
            inst(10, "identity", &["T"], vec![Type::Int]),
            inst(20, "identity", &["T"], vec![Type::Int]),
        ];
        let plan = InstantiationPlan::from_instantiations(&insts);

        assert_eq!(plan.all_specializations().count(), 1);
        // Both call sites are still recorded.
        assert!(plan.call_sites.contains_key(&ExprId(10)));
        assert!(plan.call_sites.contains_key(&ExprId(20)));
    }

    #[test]
    fn symbolic_instantiation_records_call_site_but_no_specialization() {
        // A call to `g<T>` inside a generic `f<T>` — the analyzer
        // records `type_args = [TypeVar("T")]`. The plan must
        // record the call site (so the IR builder knows it's
        // generic) but not emit a concrete specialization, since
        // `T` is not resolved.
        let insts = vec![inst(99, "g", &["U"], vec![Type::TypeVar("T".to_string())])];
        let plan = InstantiationPlan::from_instantiations(&insts);

        assert!(plan.call_sites.contains_key(&ExprId(99)));
        assert_eq!(plan.all_specializations().count(), 0);
    }

    #[test]
    fn call_site_entries_are_keyed_by_expr_id() {
        let insts = vec![
            inst(10, "f", &["T"], vec![Type::Int]),
            inst(20, "g", &["U"], vec![Type::Float]),
        ];
        let plan = InstantiationPlan::from_instantiations(&insts);

        assert_eq!(plan.call_sites.len(), 2);
        assert_eq!(plan.call_sites[&ExprId(10)].function, "f");
        assert_eq!(plan.call_sites[&ExprId(20)].function, "g");
    }
}
