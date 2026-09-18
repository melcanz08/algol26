// src/backends/llvm_codegen/types.rs

use super::IRCodeGen;
use crate::common::types::Type;
use inkwell::types::{BasicType, BasicTypeEnum};
use inkwell::AddressSpace;

impl<'ctx> IRCodeGen<'ctx> {
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
