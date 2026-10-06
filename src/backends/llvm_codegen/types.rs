// src/backends/llvm_codegen/types.rs

use super::IRCodeGen;
use crate::common::types::Type;
use inkwell::types::{BasicType, BasicTypeEnum};
use inkwell::AddressSpace;

impl<'ctx> IRCodeGen<'ctx> {
    /// Resolve a field's syntactic type annotation to a `Type`,
    /// consulting the codegen's `record_decls` table so that a
    /// field whose type is another record resolves to
    /// `Type::Record(name, [])` rather than `Type::Unknown`.
    /// Used only when lowering a `RecordDecl`'s fields, which live
    /// in the AST as `TypeSyntax` rather than `Type`. ADR 0036 L1.
    fn resolve_field_type(&self, ts: &crate::frontend::ast::TypeSyntax) -> Type {
        use crate::frontend::ast::TypeSyntax;
        match ts {
            TypeSyntax::Named(name) => {
                if self.record_decls.contains_key(name) {
                    return Type::record(name, Vec::new());
                }
                ts.to_type()
            }
            TypeSyntax::Generic { name, args } => {
                let resolved_args: Vec<Type> =
                    args.iter().map(|a| self.resolve_field_type(a)).collect();
                match (name.to_lowercase().as_str(), resolved_args.as_slice()) {
                    ("list", [inner]) => Type::list(inner.clone()),
                    ("option", [inner]) => Type::option(inner.clone()),
                    ("borrow", [inner]) => Type::borrow(inner.clone()),
                    ("mutborrow", [inner]) | ("mut_borrow", [inner]) => {
                        Type::mut_borrow(inner.clone())
                    }
                    _ if self.record_decls.contains_key(name) => Type::record(name, resolved_args),
                    _ => Type::Unknown,
                }
            }
            TypeSyntax::Unknown => Type::Unknown,
        }
    }
    /// Map an ALGOL26 type to an LLVM type.
    ///
    /// Fail-closed for the three variants that should not appear in
    /// verified IR: `Unknown`, `TypeVar(_)`, and `Generic { .. }`. In
    /// debug builds, reaching one of those arms panics so a test
    /// catches the verifier gap; in release, the function falls back
    /// to `f64` so the compiler does not crash on malformed input.
    /// Making this method return `Result` is a Tier 2 follow-up.
    pub(super) fn map_type(&self, ty: &Type) -> BasicTypeEnum<'ctx> {
        match ty {
            Type::Int => self.context.i64_type().into(),
            Type::Float => self.context.f64_type().into(),
            Type::Bool => self.context.bool_type().into(),
            // Strings are `char*` — a pointer to a null-terminated
            // UTF-8 buffer.
            Type::String => self.context.ptr_type(AddressSpace::default()).into(),
            // `Void` has no runtime value. When it appears in a type
            // position (e.g. an unused variable declared as `Void`),
            // the backend maps it to a pointer so downstream
            // instructions have a first-class value to work with.
            // Void-returning functions are declared with LLVM's
            // `void_type()` directly and never route through this
            // method.
            Type::Void => self.context.ptr_type(AddressSpace::default()).into(),
            Type::Ptr => self.context.ptr_type(AddressSpace::default()).into(),
            // `Never` is the bottom type (a function that diverges).
            // No runtime value ever has this type, so any mapping is
            // adequate; pointer is the cheapest.
            Type::Never => self.context.ptr_type(AddressSpace::default()).into(),

            // A nominal type has no runtime representation of its
            // own — it lowers to its base type. The nominal identity
            // lives only in the type system. See ADR 0029.
            Type::Distinct { base, .. } => self.map_type(base),

            // An enum's runtime value is its ordinal: an i64. The
            // identity and variant names are compile-time only.
            // See ADR 0030.
            Type::Enum { .. } => self.context.i64_type().into(),

            // A subrange has no runtime representation of its own;
            // it lowers to its base (Int or an enum). The identity
            // and the bounds are compile-time properties enforced
            // by the analyzer's literal check and the runtime
            // BoundsCheck instruction. See ADR 0031.
            Type::Subrange { base, .. } => self.map_type(base),

            // A set's runtime value is a single u64 bitset: bit `i`
            // is set iff domain element `i` is a member. The element
            // type is a compile-time property, used only by the
            // analyzer and the IR builder to compute bit positions.
            // See ADR 0032.
            Type::Set(_) => self.context.i64_type().into(),

            Type::List(inner) => {
                // NOTE (Tier 2 follow-up): the current runtime
                // representation of a list in `instruction.rs` is a
                // fixed-size LLVM array `[N x elem]`, but this mapping
                // returns `{elem, i64}`. The two disagree. Callers that
                // actually allocate list storage use the array form
                // directly, so this mapping is not load-bearing for
                // correct programs; reconciling the two
                // representations is a separate fix.
                let elem_ty = self.map_type(inner);
                let len_ty = self.context.i64_type();
                self.context
                    .struct_type(&[elem_ty, len_ty.into()], false)
                    .into()
            }
            Type::Record(name, args) => {
                // ADR 0036 L1. Named LLVM struct type, cached so
                // repeated references to the same record share one
                // type. Two-pass construction (opaque first, body
                // after) is required for records that reference
                // themselves indirectly through Option/pointer
                // fields; the language currently forbids direct
                // recursion, but the pattern is the LLVM-recommended
                // default.
                //
                // Non-generic records only in v1 — generic record
                // types are excluded by ADR 0036's scope boundary.
                // The `args` vector is ignored here; if a
                // `Type::Record` with non-empty args reaches this
                // arm, the caller took a path ADR 0036 didn't cover.
                debug_assert!(
                    args.is_empty(),
                    "LLVM codegen reached generic record type {:?}<{:?}> — \
                     not supported in ADR 0036 v1",
                    name,
                    args
                );

                if let Some(cached) = self.record_struct_types.borrow().get(name) {
                    return (*cached).into();
                }

                let rec = self.record_decls.get(name).unwrap_or_else(|| {
                    panic!(
                        "LLVM codegen: unknown record '{}' — check_backend \
                         accepted the program but the record isn't in \
                         record_decls. This is a compiler bug.",
                        name
                    )
                });

                // Opaque placeholder inserted before recursing into
                // fields, so a nested reference to this record's type
                // resolves to the same StructType.
                let struct_ty = self.context.opaque_struct_type(name);
                self.record_struct_types
                    .borrow_mut()
                    .insert(name.clone(), struct_ty);

                let field_types: Vec<BasicTypeEnum<'ctx>> = rec
                    .fields
                    .iter()
                    .map(|(_, field_ty)| self.map_type(&self.resolve_field_type(field_ty)))
                    .collect();

                struct_ty.set_body(&field_types, false);

                struct_ty.into()
            }
            Type::Array(inner, size) => {
                let elem_ty = self.map_type(inner);
                elem_ty.array_type(*size as u32).into()
            }
            Type::Option(inner) => {
                // Option = { bool is_some, T value }. Only used if
                // the capability check ever allows Option on LLVM;
                // `value.rs` currently refuses Some/None at the
                // value-lowering level.
                let inner_ty = self.map_type(inner);
                let bool_ty = self.context.bool_type();
                self.context
                    .struct_type(&[bool_ty.into(), inner_ty], false)
                    .into()
            }
            Type::Result { ok, error } => {
                // Result = { bool is_ok, Ok value, Error value }.
                // Same caveat as Option.
                let ok_ty = self.map_type(ok);
                let err_ty = self.map_type(error);
                let bool_ty = self.context.bool_type();
                self.context
                    .struct_type(&[bool_ty.into(), ok_ty, err_ty], false)
                    .into()
            }
            Type::Map(..) => {
                // Maps are refused by the capability check (`Feature::Map` is
                // not in `BackendCapabilities::llvm()`), so this arm should
                // never be reached. If it is, the capability matrix got out
                // of sync with the IR.
                unreachable!(
                    "LLVM codegen reached Type::Map — \
                     maps should have been refused by check_backend"
                );
            }
            // LLVM uses opaque pointers, so every pointer-like type
            // maps to the same LLVM `ptr`. The inner type is preserved
            // in ALGOL26's type system for analysis, not in the LLVM
            // representation.
            Type::Pointer(_)
            | Type::Borrow(_)
            | Type::MutBorrow(_)
            | Type::Channel(_)
            | Type::Tuple(_)
            | Type::Function { .. } => self.context.ptr_type(AddressSpace::default()).into(),

            // Should not appear in verified IR:
            // - `Unknown` is a type-inference placeholder.
            // - `TypeVar(_)` should be eliminated by monomorphization.
            // - `Generic { .. }` should be resolved to a concrete type
            //   before codegen.
            // If one reaches here, the IR verifier missed a case.
            Type::Unknown | Type::TypeVar(_) | Type::Generic { .. } => {
                debug_assert!(
                    false,
                    "map_type called on unresolved type `{:?}` — \
                     the IR verifier should have rejected this",
                    ty
                );
                self.context.f64_type().into()
            }
        }
    }
}
