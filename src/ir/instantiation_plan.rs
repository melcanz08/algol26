// src/ir/instantiation_plan.rs
//
// Stage 3.2a of ADR 0013. The instantiation plan is the data
// structure the IR builder consumes (Stages 3.2b–3.2e) to emit
// concrete specializations of generic functions.
//
// See docs/decisions/0013-executable-ir-generic-invariant.md.

use crate::common::types::Type;
use crate::frontend::ast::ExprId;
use crate::semantics::analyzer::Instantiation;
use std::collections::HashMap;

/// A concrete specialization of a generic function, e.g.
/// `identity<Borrow<Float>>` mangled to `identity_Borrow_Float`.
///
/// Construction goes through `Specialization::new`, which asserts
/// that `type_params.len() == type_args.len()`. The invariant is
/// re-checked in `bindings()` so any construction path that
/// bypasses `new` is still caught at first use.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Specialization {
    /// Original function name, e.g. `identity`.
    pub function: String,

    /// Mangled name, e.g. `identity_Borrow_Float`. Unique per
    /// `(function, type_args)` because `mangled_type_name` is
    /// injective.
    pub mangled_name: String,

    /// Declared type parameter names, in declaration order.
    /// `type_args[i]` binds `type_params[i]`.
    ///
    /// Duplicating this on each specialization is deliberate: the
    /// IR builder needs the `T -> Int` mapping at every type-read
    /// site under the current specialization, and carrying it here
    /// avoids a second map lookup. A future refactor may move the
    /// parameter names onto a function-declaration table if a
    /// second consumer would benefit.
    pub type_params: Vec<String>,

    /// The concrete type arguments. Never contains `TypeVar` or
    /// `Unknown` for a specialization stored in a plan.
    pub type_args: Vec<Type>,
}

impl Specialization {
    pub fn new(function: String, type_params: Vec<String>, type_args: Vec<Type>) -> Self {
        assert_eq!(
            type_params.len(),
            type_args.len(),
            "Specialization for `{}` has {} type_params but {} type_args",
            function,
            type_params.len(),
            type_args.len(),
        );
        let mangled_name = mangled_name(&function, &type_args);
        Self {
            function,
            mangled_name,
            type_params,
            type_args,
        }
    }

    /// Type parameter bindings as a `HashMap`, suitable for
    /// substitution during IR construction.
    pub fn bindings(&self) -> HashMap<String, Type> {
        assert_eq!(
            self.type_params.len(),
            self.type_args.len(),
            "Specialization `{}` has mismatched type_params/type_args lengths",
            self.function,
        );
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
#[derive(Debug, Clone, PartialEq, Eq)]
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
    pub specializations: HashMap<String, Specialization>,

    /// Every generic call site, keyed by the call expression's
    /// stable `ExprId`. Entries may be symbolic.
    pub call_sites: HashMap<ExprId, CallSiteInstantiation>,
}

impl InstantiationPlan {
    /// Build a plan from the analyzer's `Instantiation` records.
    ///
    /// Symbolic entries (containing `TypeVar` or `Unknown`) are
    /// recorded on the call-site side but do not produce concrete
    /// specializations; closure under transitive generic calls is a
    /// follow-up step.
    pub fn from_instantiations(instantiations: &[Instantiation]) -> Self {
        let mut plan = Self::default();

        for inst in instantiations {
            let previous = plan.call_sites.insert(
                inst.call_site,
                CallSiteInstantiation {
                    function: inst.function.clone(),
                    type_args: inst.type_args.clone(),
                },
            );

            // ExprId uniqueness is guaranteed by `assign_expr_ids`,
            // so a duplicate key would be an internal error. Assert
            // rather than silently overwrite.
            if let Some(prev) = previous {
                assert!(
                    prev.function == inst.function && prev.type_args == inst.type_args,
                    "duplicate call_site ExprId({}) with conflicting \
                     instantiations: first={:?}/{:?}, second={:?}/{:?}",
                    inst.call_site.0,
                    prev.function,
                    prev.type_args,
                    inst.function,
                    inst.type_args,
                );
            }

            if !inst.type_args.iter().any(|t| t.contains_unresolved()) {
                let spec = Specialization::new(
                    inst.function.clone(),
                    inst.type_params.clone(),
                    inst.type_args.clone(),
                );
                plan.insert_specialization(spec);
            }
        }

        plan
    }

    fn insert_specialization(&mut self, spec: Specialization) {
        // The key is the mangled name. Two specializations with
        // distinct `type_args` produce distinct mangled names
        // because `mangled_type_name` is injective. The only way to
        // collide is a direct bug in the mangler, so assert.
        let previous = self
            .specializations
            .insert(spec.mangled_name.clone(), spec.clone());
        if let Some(prev) = previous {
            assert_eq!(
                prev.type_args,
                spec.type_args,
                "mangler collision: `{}` and `{}` both mangle to `{}`",
                format_type_args(&prev.type_args),
                format_type_args(&spec.type_args),
                spec.mangled_name,
            );
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
        self.specializations.get(&mangled_name(function, type_args))
    }

    /// Iterate every concrete specialization.
    pub fn all_specializations(&self) -> impl Iterator<Item = &Specialization> {
        self.specializations.values()
    }

    pub fn is_empty(&self) -> bool {
        self.specializations.is_empty() && self.call_sites.is_empty()
    }
}

fn format_type_args(args: &[Type]) -> String {
    let strs: Vec<String> = args.iter().map(|a| a.to_string()).collect();
    format!("[{}]", strs.join(", "))
}

/// Mangled name for a specialization: `identity_Borrow_Float`.
/// Callers must supply `type_args` in the function's declared type
/// parameter order.
pub fn mangled_name(function: &str, type_args: &[Type]) -> String {
    let parts: Vec<String> = type_args.iter().map(mangled_type_name).collect();
    format!("{}_{}", function, parts.join("_"))
}

/// Injective string encoding of a `Type` for use in mangled
/// function names.
///
/// **Injective means:** two distinct `Type` values never produce
/// the same string. The plan relies on this — `specializations` is
/// keyed by the mangled name, and a collision would silently merge
/// two different specializations.
///
/// The scheme is exhaustive; there is no `_ => "..."` fallback.
/// Adding a new `Type` variant forces this function to be updated
/// before the crate compiles.
///
/// Structural components with variable arity (tuples, function
/// signatures, generic arguments) are length-prefixed so that
/// `Tuple<Int, Float>` and `Tuple<Tuple<Int>, Float>` encode
/// differently — their inner component counts appear explicitly.
pub fn mangled_type_name(ty: &Type) -> String {
    match ty {
        // Scalars. `Ptr` and `Never` are included so a generic
        // call like `identity(null)` mangles unambiguously.
        Type::Int => "Int".to_string(),
        Type::Float => "Float".to_string(),
        Type::String => "String".to_string(),
        Type::Bool => "Bool".to_string(),
        Type::Void => "Void".to_string(),
        Type::Ptr => "Ptr".to_string(),
        Type::Unknown => "Unknown".to_string(),
        Type::Never => "Never".to_string(),

        // A `TypeVar` reaching this function is a bug — the
        // caller is supposed to have filtered with
        // `contains_unresolved` — but the encoding exists so the
        // assertion in `insert_specialization` can name it.
        Type::TypeVar(name) => format!("TypeVar_{}", name),

        Type::List(inner) => format!("List_{}", mangled_type_name(inner)),

        // Array size is part of the type. `Array<Int, 3>` and
        // `Array<Int, 4>` are different types.
        Type::Array(inner, size) => {
            format!("Array_{}_{}", mangled_type_name(inner), size)
        }

        // Tuple: length-prefixed for injectivity.
        Type::Tuple(elems) => {
            let parts: Vec<String> = elems.iter().map(mangled_type_name).collect();
            format!("Tuple_{}_{}", elems.len(), parts.join("_"))
        }

        Type::Option(inner) => format!("Option_{}", mangled_type_name(inner)),

        Type::Result { ok, error } => format!(
            "Result_{}_{}",
            mangled_type_name(ok),
            mangled_type_name(error)
        ),

        Type::Pointer(inner) => format!("Pointer_{}", mangled_type_name(inner)),
        Type::Borrow(inner) => format!("Borrow_{}", mangled_type_name(inner)),
        Type::MutBorrow(inner) => format!("MutBorrow_{}", mangled_type_name(inner)),
        Type::Channel(inner) => format!("Channel_{}", mangled_type_name(inner)),

        // Function: arity-prefixed for injectivity.
        Type::Function {
            params,
            return_type,
        } => {
            let parts: Vec<String> = params.iter().map(mangled_type_name).collect();
            format!(
                "Function_{}_{}_{}",
                params.len(),
                parts.join("_"),
                mangled_type_name(return_type)
            )
        }

        // Generic: name + arity + arguments.
        Type::Generic { name, args } => {
            let parts: Vec<String> = args.iter().map(mangled_type_name).collect();
            format!("Generic_{}_{}_{}", name, args.len(), parts.join("_"))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::common::types::Type;

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

    // ─── Plan construction ────────────────────────────────────────

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
        assert_eq!(spec.function, "identity");

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
    fn duplicate_concrete_instantiations_are_deduplicated() {
        let insts = vec![
            inst(10, "identity", &["T"], vec![Type::Int]),
            inst(20, "identity", &["T"], vec![Type::Int]),
        ];
        let plan = InstantiationPlan::from_instantiations(&insts);

        assert_eq!(plan.all_specializations().count(), 1);
        assert!(plan.call_sites.contains_key(&ExprId(10)));
        assert!(plan.call_sites.contains_key(&ExprId(20)));
    }

    #[test]
    fn symbolic_instantiation_records_call_site_but_no_specialization() {
        // `g<T>` inside a generic `f<T>` — type_args contains
        // `TypeVar("T")` and must not be treated as concrete.
        let insts = vec![inst(99, "g", &["U"], vec![Type::TypeVar("T".to_string())])];
        let plan = InstantiationPlan::from_instantiations(&insts);

        assert!(plan.call_sites.contains_key(&ExprId(99)));
        assert_eq!(plan.all_specializations().count(), 0);
    }

    #[test]
    fn unknown_type_arg_blocks_specialization() {
        // Same behavior as the symbolic case but for `Unknown`.
        let insts = vec![inst(99, "identity", &["T"], vec![Type::Unknown])];
        let plan = InstantiationPlan::from_instantiations(&insts);
        assert_eq!(plan.all_specializations().count(), 0);
    }

    #[test]
    fn unresolved_inside_function_or_generic_blocks_specialization() {
        // The two cases the previous implementation missed. A
        // `Function` or `Generic` whose *nested* components
        // contain a TypeVar must be treated as symbolic.
        let insts = vec![
            inst(
                10,
                "f",
                &["T"],
                vec![Type::Function {
                    params: vec![Type::TypeVar("T".to_string())],
                    return_type: Box::new(Type::Int),
                }],
            ),
            inst(
                20,
                "g",
                &["T"],
                vec![Type::generic("Box", vec![Type::TypeVar("T".to_string())])],
            ),
        ];
        let plan = InstantiationPlan::from_instantiations(&insts);
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

    #[test]
    #[should_panic(expected = "conflicting instantiations")]
    fn duplicate_call_site_with_conflict_panics() {
        // ExprId uniqueness is guaranteed by `assign_expr_ids`, so
        // this cannot happen in a correct pipeline. The plan
        // asserts rather than silently overwriting.
        let insts = vec![
            inst(10, "f", &["T"], vec![Type::Int]),
            inst(10, "f", &["T"], vec![Type::Float]),
        ];
        let _ = InstantiationPlan::from_instantiations(&insts);
    }

    #[test]
    #[should_panic(expected = "type_params")]
    fn specialization_with_mismatched_arity_panics() {
        let _ = Specialization::new(
            "identity".to_string(),
            vec!["T".to_string(), "U".to_string()],
            vec![Type::Int],
        );
    }

    // ─── Mangling injectivity ─────────────────────────────────────

    #[test]
    fn mangler_covers_every_scalar() {
        assert_eq!(mangled_type_name(&Type::Int), "Int");
        assert_eq!(mangled_type_name(&Type::Float), "Float");
        assert_eq!(mangled_type_name(&Type::String), "String");
        assert_eq!(mangled_type_name(&Type::Bool), "Bool");
        assert_eq!(mangled_type_name(&Type::Void), "Void");
        assert_eq!(mangled_type_name(&Type::Ptr), "Ptr");
        assert_eq!(mangled_type_name(&Type::Never), "Never");
    }

    #[test]
    fn mangler_distinguishes_tuple_shapes() {
        // `Tuple<Int, Float>` and `Tuple<Tuple<Int>, Float>` are
        // different types. Length-prefixing makes their encoded
        // forms distinct.
        let a = mangled_type_name(&Type::tuple(vec![Type::Int, Type::Float]));
        let b = mangled_type_name(&Type::tuple(vec![
            Type::tuple(vec![Type::Int]),
            Type::Float,
        ]));
        assert_ne!(a, b);
    }

    #[test]
    fn mangler_distinguishes_array_sizes() {
        let a = mangled_type_name(&Type::array(Type::Int, 3));
        let b = mangled_type_name(&Type::array(Type::Int, 4));
        assert_ne!(a, b);
    }

    #[test]
    fn mangler_distinguishes_function_signatures() {
        let a = mangled_type_name(&Type::Function {
            params: vec![Type::Int, Type::Float],
            return_type: Box::new(Type::Bool),
        });
        let b = mangled_type_name(&Type::Function {
            params: vec![Type::Int],
            return_type: Box::new(Type::Float),
        });
        assert_ne!(a, b);
    }

    #[test]
    fn mangler_distinguishes_generic_argument_counts() {
        let a = mangled_type_name(&Type::generic("Box", vec![Type::Int]));
        let b = mangled_type_name(&Type::generic("Box", vec![Type::Int, Type::Float]));
        assert_ne!(a, b);
    }

    #[test]
    fn mangler_no_unknown_fallback_for_ptr() {
        // The previous implementation would have mapped `Ptr` and
        // any unhandled variant to `"Unknown"`, colliding with the
        // real `Unknown` variant. This test pins the fix.
        assert_ne!(
            mangled_type_name(&Type::Ptr),
            mangled_type_name(&Type::Unknown)
        );
    }
}
