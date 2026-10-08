// src/semantics/analyzer/mod.rs
//
// Ownership and borrow checking, type inference, trait bounds, and
// scope-based lifetime rules. Produces a type table keyed by
// `ExprId`, which the IR builder consumes to avoid re-inferring
// types.

use crate::common::diagnostics::{CompileError, ErrorCode, Result};
use crate::common::span::Span;
use crate::common::types::{NominalTypeId, Type};
use crate::frontend::ast::{
    BinOp, DistinctDecl, EnumDecl, Expr, ExprId, ExprKind, FunctionDecl, ImplBlock, MatchCaseExpr,
    Pattern, RecordDecl, Stmt, SubrangeDecl, TraitDecl, WhereClause,
};
use crate::semantics::state::{BorrowKind, BorrowLifetime, SemanticState, VarState};
use crate::semantics::trait_registry::TraitRegistry;
use std::collections::{HashMap, HashSet};

mod expr;
mod items;
mod ownership;
mod scopes;
mod stmt;
#[cfg(test)]
mod tests;

// ─── Borrow-checking model ──────────────────────────────────────────────
//
// Borrow lifetimes are *lexical*: a borrow created inside scope S stays
// alive until S is popped. Ownership, borrow, and initialization state
// live in a single `SemanticState`, which is joined at every branch
// point (`if`, `match`, loops) via `SemanticState::join`. The analyzer
// does not maintain any parallel bookkeeping — `state` is the sole
// authority.
//
// Known limitation: NLL is not implemented, so a borrow lives until
// the end of its enclosing block, not until the last use of the
// reference. Programs that rely on NLL may be rejected conservatively.
// ────────────────────────────────────────────────────────────────────────
pub struct SemanticAnalyzer {
    scopes: Vec<HashMap<String, (Type, bool)>>,
    /// Variables whose value is statically known to be `null`. Only
    /// `val` bindings appear here: an immutable binding initialized to
    /// `null` cannot be reassigned, so it is permanently null. `var`
    /// bindings are excluded because they can be reassigned and the
    /// analyzer does not perform value-flow tracking.
    null_bindings: Vec<HashSet<String>>,
    in_mut_borrow: bool,
    functions: HashMap<String, FunctionInfo>,
    current_return_type: Option<Type>,
    list_lengths: Vec<HashMap<String, usize>>,
    list_values: Vec<HashMap<String, Vec<Expr>>>,
    type_params: Vec<HashMap<String, Type>>,
    type_constraints: Vec<HashMap<String, Vec<String>>>,
    trait_registry: TraitRegistry,
    deferred_captures: Vec<HashSet<String>>,
    records: HashMap<String, RecordInfo>,
    /// Nominal type declarations (`type X distinct Y`), keyed by the
    /// declared name. Values carry the assigned `NominalTypeId`.
    /// Identity is the id; the name is presentation only. ADR 0029.
    nominal_types: HashMap<String, Type>,
    /// Monotonic counter for `NominalTypeId`.
    next_nominal_id: u32,
    /// Enum type declarations (`enum Name ...`), keyed by the
    /// declared name. Values carry the assigned `EnumTypeId`.
    /// Identity is the id; the name and variants are presentation.
    /// ADR 0030.
    enum_types: HashMap<String, Type>,
    /// Monotonic counter for `EnumTypeId`.
    next_enum_id: u32,
    /// Subrange type declarations (`type X Base in Low..High`),
    /// keyed by the declared name. Values carry the assigned
    /// `SubrangeTypeId` plus bounds. ADR 0031.
    subrange_types: HashMap<String, Type>,
    /// Monotonic counter for `SubrangeTypeId`.
    next_subrange_id: u32,
    /// Associated constants: `"Foo::SIZE"` -> (declared type, value expr).
    /// The analyzer registers these from impl bodies and resolves
    /// `Foo::SIZE` at use sites. The IR builder reads the same map
    /// via `take_const_values` to inline the value.
    const_values: HashMap<String, (Type, Expr)>,
    /// Function names declared variadic via `extern "C" ...(...)`.
    /// Used to relax the arity check from "exactly N" to "at
    /// least N" for those functions.
    variadic_functions: HashSet<String>,

    /// Inferred type of each expression, keyed by its stable `ExprId`.
    /// Written by `analyze_expr_with_context` on every expression the
    /// analyzer visits.
    pub type_table_id: HashMap<ExprId, Type>,
    /// Generic instantiation facts recorded during analysis. Read by
    /// `type_check_program` via `take_instantiations`, which feeds
    /// `InstantiationPlan::from_instantiations`. See `Instantiation`
    /// and ADR 0013.
    instantiations: Vec<Instantiation>,
    // Single source of truth - unified with dataflow engine
    pub(crate) state: SemanticState,
    /// Span of the node currently being analyzed. Updated at the top
    /// of `analyze_expr_with_context` and `analyze_stmt`. Used by
    /// error sites that don't have direct access to the node.
    current_span: Span,
    /// Number of `region NAME` blocks currently open in the
    /// enclosing function. Incremented only in the `Stmt::RegionBlock`
    /// arm of `analyze_stmt`, matching the IR builder's emission of
    /// `Instruction::RegionEnter` / `RegionExit`. Distinct from
    /// `self.scopes.len()` because plain blocks (if-branches, loop
    /// bodies, unsafe blocks) also push a scope but do not open a
    /// region.
    region_depth: usize,
    /// Loop nesting stack. Each entry records the `region_depth` at
    /// the moment the loop was entered. `break` and `continue` are
    /// rejected if the current `region_depth` exceeds the innermost
    /// loop's entry depth — that shape would skip the region's
    /// `RegionExit` and leak its allocations until function return.
    loop_stack: Vec<LoopContext>,
    /// Unsafe-block depth. Zero outside any `unsafe { ... }`. The
    /// operations gated by ADR 0015 — raw pointer deref, `alloc`,
    /// `free` — are permitted only when this is greater than zero.
    /// A depth counter rather than a bool because unsafe blocks
    /// nest.
    unsafe_depth: usize,
    /// ADR 0038 D4b. Method calls through a `&dyn Trait` receiver,
    /// keyed by the call expression's `ExprId`. The IR builder reads
    /// this map at the same call site to emit `Instruction::VirtualCall`
    /// instead of `Instruction::Call`.
    virtual_calls: HashMap<ExprId, VirtualCallInfo>,
}

/// ADR 0038 D4b. What the analyzer resolved at a `dyn Trait` method
/// call site. `slot` is the method's position in the trait's
/// declaration order; `return_type` is the method's declared return
/// type after `resolve_type_syntax`.
#[derive(Debug, Clone)]
pub struct VirtualCallInfo {
    pub trait_name: String,
    pub method_name: String,
    pub slot: usize,
    pub return_type: Type,
}

#[derive(Debug, Clone)]
struct FunctionInfo {
    params: Vec<(String, Type)>,
    return_type: Type,
    /// Declared type parameter names, in declaration order. Empty
    /// for non-generic functions. Used by `ExprKind::FunctionCall`
    /// to record instantiation facts. See ADR 0013.
    type_params: Vec<String>,
    /// Where-clauses on this function. Read by `FunctionCall` to
    /// check bound satisfaction once concrete type arguments are
    /// known. See ADR 0025 (enforce path).
    where_clauses: Vec<WhereClause>,
}

#[derive(Debug, Clone)]
pub struct RecordInfo {
    pub name: String,
    pub type_params: Vec<String>,
    pub fields: Vec<(String, Type)>,
}

/// A recorded generic instantiation. Produced by the analyzer when
/// it resolves a call to a function with non-empty `type_params`;
/// consumed by `InstantiationPlan::from_instantiations` and, after
/// transitive closure, by the IR builder to emit one
/// `SemanticFunction` per specialization. Keyed by the call site's
/// stable `ExprId`.
///
/// See ADR 0013 (`docs/decisions/0013-executable-ir-generic-invariant.md`)
/// for the full design.
#[derive(Debug, Clone)]
pub struct Instantiation {
    /// The call expression where the instantiation occurs. The IR
    /// builder will look up specializations by this ID.
    pub call_site: ExprId,
    /// The name of the generic function being called.
    pub function: String,
    /// The function's declared type parameter names, in declaration
    /// order. `type_args[i]` binds `type_params[i]`. Needed so the
    /// specialization plan can carry `T -> Int`-style mappings, not
    /// just an ordered list of concrete types.
    pub type_params: Vec<String>,
    /// The concrete type arguments the analyzer inferred, in
    /// declaration order. May contain `Type::Unknown` if the
    /// analyzer could not determine an argument's type; the
    /// executable-IR verifier (Stage 3.4) is responsible for
    /// rejecting such cases.
    pub type_args: Vec<Type>,
}

#[derive(Debug, Clone, Copy)]
struct LoopContext {
    region_depth_at_entry: usize,
}

impl Default for SemanticAnalyzer {
    fn default() -> Self {
        Self::new()
    }
}

impl SemanticAnalyzer {
    pub fn new() -> Self {
        SemanticAnalyzer {
            scopes: vec![HashMap::new()],
            functions: HashMap::new(),
            null_bindings: vec![HashSet::new()],
            in_mut_borrow: false,
            current_return_type: None,
            list_lengths: vec![HashMap::new()],
            list_values: vec![HashMap::new()],
            type_params: vec![HashMap::new()],
            type_constraints: vec![HashMap::new()],
            trait_registry: TraitRegistry::new(),
            deferred_captures: vec![HashSet::new()],
            variadic_functions: HashSet::new(),
            type_table_id: HashMap::new(),
            instantiations: Vec::new(),
            state: SemanticState::new(),
            current_span: Span::default(),
            region_depth: 0,
            loop_stack: Vec::new(),
            unsafe_depth: 0,
            records: HashMap::new(),
            virtual_calls: HashMap::new(),
            nominal_types: HashMap::new(),
            next_nominal_id: 0,
            enum_types: HashMap::new(),
            next_enum_id: 0,
            subrange_types: HashMap::new(),
            next_subrange_id: 0,
            const_values: HashMap::new(),
        }
    }

    /// Analyze `f` on a snapshot of the current state; restore the
    /// entry state on return and hand back the branch's exit state.
    /// The caller is responsible for joining branch exits.
    ///
    /// Panics inside `f` leave `self.state` forked, not restored.
    /// The analyzer aborts on error anyway, so this is acceptable;
    /// add a guard here if that ever changes.
    fn in_branch<F, T>(&mut self, f: F) -> (T, SemanticState)
    where
        F: FnOnce(&mut Self) -> T,
    {
        let entry = self.state.fork();
        let result = f(self);
        let exit = std::mem::replace(&mut self.state, entry);
        (result, exit)
    }

    // ─── UNIFY TYPES ───────────────────────────────────────────────────────
    /// Look up the inferred type of an expression by its `ExprId`.
    pub fn type_of(&self, expr: &Expr) -> Option<&Type> {
        self.type_table_id.get(&expr.id)
    }
    /// Take ownership of the type table so it can be handed to the IR builder.
    pub fn take_type_table_id(&mut self) -> HashMap<ExprId, Type> {
        std::mem::take(&mut self.type_table_id)
    }
    /// Read-only view of the recorded generic instantiations.
    pub fn instantiations(&self) -> &[Instantiation] {
        &self.instantiations
    }
    /// Take ownership of the instantiation list so it can be handed
    /// to `TypedProgram`.
    pub fn take_instantiations(&mut self) -> Vec<Instantiation> {
        std::mem::take(&mut self.instantiations)
    }

    /// Take ownership of the resolved nominal type table so it can be
    /// handed to `TypedProgram`. `NominalTypeId` is assigned by
    /// `register_nominal_types`; this is the single source of that
    /// identity. Downstream consumers receive the resolved map and
    /// never reconstruct ids. See ADR 0029.
    pub fn take_nominal_types(&mut self) -> HashMap<String, Type> {
        std::mem::take(&mut self.nominal_types)
    }

    /// Take ownership of the resolved enum type table so it can be
    /// handed to `TypedProgram`. Same single-source-of-truth
    /// discipline as `take_nominal_types`. See ADR 0030.
    pub fn take_enum_types(&mut self) -> HashMap<String, Type> {
        std::mem::take(&mut self.enum_types)
    }

    /// Take ownership of the resolved subrange type table.
    /// See ADR 0031.
    pub fn take_subrange_types(&mut self) -> HashMap<String, Type> {
        std::mem::take(&mut self.subrange_types)
    }

    /// Take the associated-constant map so it can be handed to the
    /// IR builder. Keyed by `"Type::NAME"`.
    pub fn take_const_values(&mut self) -> HashMap<String, (Type, Expr)> {
        std::mem::take(&mut self.const_values)
    }

    /// ADR 0038 D4b. Take the virtual-call map so the IR builder can
    /// emit `Instruction::VirtualCall` at the right call sites.
    pub fn take_virtual_calls(&mut self) -> HashMap<ExprId, VirtualCallInfo> {
        std::mem::take(&mut self.virtual_calls)
    }

    /// ADR 0038 D6. Forward the trait registry's `TraitId` map to the
    /// IR builder so a `TypeSyntax::DynTrait` annotation can be
    /// resolved without re-registering the trait. Cloned, not taken;
    /// the analyzer keeps using the registry after this call.
    pub fn trait_ids(&self) -> HashMap<String, crate::common::types::TraitId> {
        self.trait_registry.trait_ids()
    }

    /// ADR 0038 D6-2. Forward trait declarations to the IR builder
    /// so the vtable gather pass can record each trait's method
    /// names in declaration order.
    pub fn trait_decls(&self) -> Vec<crate::frontend::ast::TraitDecl> {
        self.trait_registry.trait_decls()
    }
    /// Access unified state (for dataflow integration)
    pub fn state(&self) -> &SemanticState {
        &self.state
    }
    pub fn state_mut(&mut self) -> &mut SemanticState {
        &mut self.state
    }

    fn lookup_list_length(&self, name: &str) -> Option<usize> {
        for scope in self.list_lengths.iter().rev() {
            if let Some(len) = scope.get(name) {
                return Some(*len);
            }
        }
        None
    }

    fn declare_list_length(&mut self, name: &str, len: usize) {
        if let Some(scope) = self.list_lengths.last_mut() {
            scope.insert(name.to_string(), len);
        }
    }

    /// Invalidate the statically-tracked length of `name`. Called
    /// after `List.append`, which grows the list past whatever
    /// length the analyzer previously recorded. See ADR 0028.
    fn clear_list_length(&mut self, name: &str) {
        for scope in self.list_lengths.iter_mut() {
            scope.remove(name);
        }
        for scope in self.list_values.iter_mut() {
            scope.remove(name);
        }
    }

    fn lookup_list_values(&self, name: &str) -> Option<Vec<Expr>> {
        for scope in self.list_values.iter().rev() {
            if let Some(vals) = scope.get(name) {
                return Some(vals.clone());
            }
        }
        None
    }

    fn declare_list_values(&mut self, name: &str, vals: Vec<Expr>) {
        if let Some(scope) = self.list_values.last_mut() {
            scope.insert(name.to_string(), vals);
        }
    }

    pub fn analyze(&mut self, functions: &[FunctionDecl]) -> Result<()> {
        self.analyze_with_spans(functions, &[], &[], &[], &[], &[], &[])
    }

    pub fn analyze_with_spans(
        &mut self,
        functions: &[FunctionDecl],
        traits: &[TraitDecl],
        impls: &[ImplBlock],
        records: &[RecordDecl],
        distincts: &[DistinctDecl],
        enums: &[EnumDecl],
        subranges: &[SubrangeDecl],
    ) -> Result<()> {
        debug_assert!(
            crate::compiler::assert_all_numbered(functions),
            "SemanticAnalyzer::analyze_with_spans called with unnumbered AST — \
             call assign_expr_ids(&mut functions) before analyzing"
        );
        self.register_builtin_functions();

        // Enums are registered before nominal types and records: a
        // nominal type or record field could name an enum in a later
        // ADR, and the resolution order in `resolve_type_syntax`
        // matches this. See ADR 0030.
        self.register_enum_types(enums)?;

        // Nominal types are registered before records and user
        // functions: a record field or a function signature may name
        // a nominal type. Base must be primitive (ADR 0029).
        self.register_nominal_types(distincts)?;

        // Subranges are registered after enums and nominal types
        // because their base may be an enum. See ADR 0031.
        self.register_subrange_types(subranges)?;

        // Records must be registered before user functions:
        // function signatures can name records as parameters or
        // return types, and `resolve_type_syntax` consults the
        // record table. Records are registered in declaration
        // order; a record that names a later record as a field
        // type is not yet supported.
        for rec in records {
            self.register_record(rec)?;
        }

        self.register_user_functions(functions)?;
        // ─── ADR 0033: `self` is only legal inside an impl block ───
        // `expand_impl_methods` has already renamed impl methods to
        // `Type_method`, so any function in `functions` with a receiver
        // whose name doesn't match a known impl's mangled form is a
        // top-level declaration that misuses `self`.
        for func in functions {
            if func.receiver.is_none() {
                continue;
            }
            let is_impl_method = impls.iter().any(|b| {
                b.methods.iter().any(|m| {
                    let mangled = match &b.trait_name {
                        Some(t) => format!("{}_{}_{}", t, b.target_type, m.name),
                        None => format!("{}_{}", b.target_type, m.name),
                    };
                    mangled == func.name
                })
            });
            if !is_impl_method {
                return Err(CompileError::simple(
                    &format!(
                        "`self` receiver is only allowed inside an `impl` block, \
                         found in top-level function '{}'",
                        func.name
                    ),
                    0,
                    0,
                    "",
                    ErrorCode::E0002,
                ));
            }
        }
        for trait_decl in traits {
            self.trait_registry.register_trait(trait_decl.clone());
        }
        for impl_block in impls {
            self.trait_registry.register_impl(impl_block.clone());
        }
        for impl_block in impls {
            if let Err(err) = self.trait_registry.validate_impl(impl_block) {
                return Err(CompileError::simple(&err, 0, 0, "", ErrorCode::E0002));
            }
        }

        // ─── ADR 0034: impl coherence ───
        // One impl per (trait_name, target_type). The mangled method
        // name is `{target_type}_{method_name}`; two impls with the
        // same trait and target therefore produce colliding symbols,
        // which previously surfaced later as a confusing "duplicate
        // function name" error from `register_user_functions`.
        //
        // Inherent impls (`trait_name = None`) are subject to the
        // same rule against other inherent impls of the same type.
        //
        // Specialization (a more specific impl winning over a more
        // general one) is out of scope — see ADR 0034 §"What remains
        // out of scope".
        {
            let mut seen: HashMap<(Option<String>, String), ()> = HashMap::new();
            for impl_block in impls {
                let key = (
                    impl_block.trait_name.clone(),
                    impl_block.target_type.clone(),
                );
                if seen.contains_key(&key) {
                    let what = match &impl_block.trait_name {
                        Some(t) => format!("trait '{}' for type '{}'", t, impl_block.target_type),
                        None => format!("inherent impl on type '{}'", impl_block.target_type),
                    };
                    return Err(CompileError::simple(
                        &format!("Conflicting impl: {} is declared more than once", what),
                        0,
                        0,
                        "",
                        ErrorCode::E0002,
                    ));
                }
                seen.insert(key, ());
            }
        }

        // ─── ADR 0033: inherent impl checks ───
        // Field/method collision is a compile error: `impl User { function name }`
        // where `User` already has field `name` would make `u.name` ambiguous
        // between a field read and a zero-argument method call.
        //
        // Only inherent impls (`trait_name.is_none()`) are checked. Trait
        // impls can legitimately share a name with a field, since the
        // trait's method is called through its own resolution path.
        for impl_block in impls {
            if impl_block.trait_name.is_some() {
                continue;
            }
            let Some(rec) = self.records.get(&impl_block.target_type) else {
                // Target may be an enum or nominal type, which have no
                // fields. No collision is possible.
                continue;
            };
            for method in &impl_block.methods {
                if rec.fields.iter().any(|(fname, _)| *fname == method.name) {
                    return Err(CompileError::simple(
                        &format!(
                            "Method '{}' on '{}' collides with a field of the same name",
                            method.name, impl_block.target_type
                        ),
                        0,
                        0,
                        "",
                        ErrorCode::E0002,
                    ));
                }
            }
        }

        // ─── Associated constants ───
        // Two passes because trait impls must verify their
        // constants against the trait's declarations, while
        // inherent impls are self-declaring.
        for impl_block in impls {
            let Some(trait_name) = impl_block.trait_name.as_ref() else {
                // Inherent impl: register each constant directly.
                for imp_const in &impl_block.constants {
                    let declared_ty = self.resolve_type_syntax(&imp_const.type_)?;
                    let actual_ty = self.analyze_expr(&imp_const.value)?;
                    if !actual_ty.can_coerce_to(&declared_ty)
                        && declared_ty != Type::Unknown
                        && actual_ty != Type::Unknown
                    {
                        return Err(CompileError::simple(
                            &format!(
                                "Constant '{}::{}': expected {}, found {}",
                                impl_block.target_type, imp_const.name, declared_ty, actual_ty
                            ),
                            0,
                            0,
                            "",
                            ErrorCode::E0002,
                        ));
                    }
                    let key = format!("{}::{}", impl_block.target_type, imp_const.name);
                    self.const_values
                        .insert(key, (declared_ty, imp_const.value.clone()));
                }
                continue;
            };

            // Trait impl: match each definition against the trait's
            // declaration, and require every declaration to have a
            // definition.
            let trait_decl = traits.iter().find(|t| &t.name == trait_name).cloned();
            let Some(trait_decl) = trait_decl else {
                continue;
            };

            for imp_const in &impl_block.constants {
                let Some(trait_const) = trait_decl
                    .constants
                    .iter()
                    .find(|c| c.name == imp_const.name)
                else {
                    return Err(CompileError::simple(
                        &format!(
                            "Constant '{}' on '{}' is not declared in trait '{}'",
                            imp_const.name, impl_block.target_type, trait_name
                        ),
                        0,
                        0,
                        "",
                        ErrorCode::E0002,
                    ));
                };
                let declared_ty = self.resolve_type_syntax(&trait_const.type_)?;
                let actual_ty = self.analyze_expr(&imp_const.value)?;
                if !actual_ty.can_coerce_to(&declared_ty)
                    && declared_ty != Type::Unknown
                    && actual_ty != Type::Unknown
                {
                    return Err(CompileError::simple(
                        &format!(
                            "Constant '{}::{}': expected {}, found {}",
                            impl_block.target_type, imp_const.name, declared_ty, actual_ty
                        ),
                        0,
                        0,
                        "",
                        ErrorCode::E0002,
                    ));
                }
                let key = format!("{}::{}", impl_block.target_type, imp_const.name);
                self.const_values
                    .insert(key, (declared_ty, imp_const.value.clone()));
            }

            for trait_const in &trait_decl.constants {
                if !impl_block
                    .constants
                    .iter()
                    .any(|c| c.name == trait_const.name)
                {
                    return Err(CompileError::simple(
                        &format!(
                            "Impl of trait '{}' for '{}' does not define constant '{}'",
                            trait_name, impl_block.target_type, trait_const.name
                        ),
                        0,
                        0,
                        "",
                        ErrorCode::E0002,
                    ));
                }
            }
        }

        for func in functions {
            self.analyze_function(func)?;
        }
        Ok(())
    }

    // Keep the old name for compatibility; delegate.
    pub fn analyze_with_traits(
        &mut self,
        functions: &[FunctionDecl],
        traits: &[TraitDecl],
        impls: &[ImplBlock],
        records: &[RecordDecl],
    ) -> Result<()> {
        debug_assert!(
            crate::compiler::assert_all_numbered(functions),
            "SemanticAnalyzer::analyze_with_traits called with unnumbered AST — \
             call assign_expr_ids(&mut functions) before analyzing"
        );
        // The 4-arg form is retained for callers that predate
        // nominal and enum types. Callers with `DistinctDecl`s or
        // `EnumDecl`s in scope should call `analyze_with_spans`
        // directly.
        self.analyze_with_spans(functions, traits, impls, records, &[], &[], &[])
    }

    /// Verify that every `where T: Trait` clause names a trait that
    /// has been declared. This is **not** a satisfaction check: it
    /// does not verify that a concrete type substituted for `T` at
    /// a call site implements the trait. A generic function whose
    /// `where` clause names a declared-but-unimplemented trait
    /// passes analysis. See ADR 0025 for the state and the two
    /// paths (enforce or remove).
    fn check_trait_bounds(
        &self,
        _type_params: &[String],
        where_clauses: &[WhereClause],
    ) -> Result<()> {
        for clause in where_clauses {
            let trait_name = &clause.trait_name;
            if !self.trait_registry.trait_exists(trait_name) {
                return Err(CompileError::simple(
                    &format!("Unknown trait '{}'", trait_name),
                    0,
                    0,
                    "",
                    ErrorCode::E0004,
                )
                .with_suggestion(&format!(
                    "Define trait '{}' before using it as a constraint",
                    trait_name
                )));
            }
        }
        Ok(())
    }

    // ─── ADR 0038: dynamic dispatch coercion ───────────────────────────

    /// True when `ty` is `&dyn Trait` or `&mut dyn Trait`. This is
    /// the syntactic position `dyn Trait` is legal in; the analyzer
    /// rejects any other occurrence at lowering time.
    pub(super) fn is_dyn_trait_ref(ty: &Type) -> bool {
        match ty {
            Type::Borrow(inner) | Type::MutBorrow(inner) => {
                matches!(inner.as_ref(), Type::DynTrait { .. })
            }
            _ => false,
        }
    }

    /// Extended type compatibility: `Type::can_coerce_to` plus the
    /// `&T -> &dyn Trait` / `&mut T -> &mut dyn Trait` rule from
    /// ADR 0038. The type-level `can_coerce_to` cannot express this
    /// rule because it has no access to the trait registry.
    pub(super) fn can_coerce_with_traits(&self, from: &Type, to: &Type) -> bool {
        if from.can_coerce_to(to) {
            return true;
        }
        match (from, to) {
            (Type::Borrow(inner), Type::Borrow(expected_inner))
            | (Type::MutBorrow(inner), Type::MutBorrow(expected_inner)) => {
                if let Type::DynTrait { trait_name, .. } = expected_inner.as_ref() {
                    return self.trait_registry.type_implements_trait(inner, trait_name);
                }
                false
            }
            _ => false,
        }
    }

    /// ADR 0038. Check that `actual` coerces to a `&dyn Trait` /
    /// `&mut dyn Trait` target `expected`. No-op when `expected` is
    /// anything else, so callers can invoke it unconditionally.
    ///
    /// Produces `E0012` rather than the generic `E0002` so the
    /// diagnostic can name the trait and the concrete type that
    /// failed to implement it. `what` is a short noun phrase for the
    /// binding site, e.g. `"Variable 's'"` or `"Argument 'x'"`.
    pub(super) fn check_dyn_trait_target(
        &self,
        actual: &Type,
        expected: &Type,
        span: Span,
        what: &str,
    ) -> Result<()> {
        if !Self::is_dyn_trait_ref(expected) {
            return Ok(());
        }
        if self.can_coerce_with_traits(actual, expected) {
            return Ok(());
        }

        let expected_inner = match expected {
            Type::Borrow(inner) | Type::MutBorrow(inner) => inner.as_ref(),
            _ => unreachable!("is_dyn_trait_ref gate above"),
        };
        let Type::DynTrait { trait_name, .. } = expected_inner else {
            unreachable!("is_dyn_trait_ref gate above")
        };

        // If the value is itself a borrow, name the concrete type
        // in the message; otherwise the mismatch is "not a borrow".
        let (concrete_desc, suffix) = match actual {
            Type::Borrow(inner) | Type::MutBorrow(inner) => (
                inner.to_string(),
                format!("`{}` does not implement trait `{}`", inner, trait_name),
            ),
            other => (
                other.to_string(),
                format!("expected a borrow, found `{}`", other),
            ),
        };

        let mut err = CompileError::at(
            span,
            &format!("{} expects `{}`, but {}", what, expected, suffix),
            ErrorCode::E0012,
        );
        if matches!(actual, Type::Borrow(_) | Type::MutBorrow(_)) {
            err = err.with_suggestion(&format!(
                "Add `impl {} for {}`, or pass a value whose type implements `{}`",
                trait_name, concrete_desc, trait_name
            ));
        }
        Err(err)
    }

    fn resolve_trait_method(&self, type_: &Type, method_name: &str) -> Option<FunctionDecl> {
        self.trait_registry
            .resolve_method(type_, method_name)
            .cloned()
    }
}
