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
                if let Some(nom) = self.nominal_types.get(name) {
                    return nom.clone();
                }
                if let Some(en) = self.enum_types.get(name) {
                    return en.clone();
                }
                if let Some(sub) = self.subrange_types.get(name) {
                    return sub.clone();
                }
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
            // ADR 0038. A `dyn Trait` field on a record requires the
            // capability matrix to have allowed dynamic dispatch, which
            // it does not. If codegen reaches here, the matrix is out
            // of sync with the IR.
            TypeSyntax::DynTrait { .. } => Type::Unknown,
            TypeSyntax::Projection { name, .. } => {
                // ADR 0041. The analyzer must have normalized a
                // projection before codegen. Reaching here is a
                // compiler bug.
                debug_assert!(
                    false,
                    "LLVM codegen: `::{}` reached resolve_field_type \
                     (ADR 0041 normalization should have rewritten it)",
                    name,
                );
                Type::Unknown
            }
            TypeSyntax::Unknown => Type::Unknown,
        }
    }
    /// Map an ALGOL26 type to an LLVM type.
    ///
    /// Fail-closed for the three variants that should not appear in
    /// verified IR: `Unknown`, `TypeVar(_)`, and `Generic { .. }`.
    /// Reaching one of those arms panics in both debug and release
    /// builds. Silently emitting wrong code (the previous behaviour)
    /// violates the compiler's "fail closed" contract; a crash with
    /// a clear message is the correct failure mode for a verifier
    /// gap. Making this method return `Result` is a Tier 2 follow-up.
    pub(super) fn map_type(&self, ty: &Type) -> BasicTypeEnum<'ctx> {
        match ty {
            Type::Int => self.context.i64_type().into(),
            Type::Float => self.context.f64_type().into(),
            // A Char is a Unicode scalar value; i32 matches
            // LLVM's representation of a codepoint and the
            // option struct's payload field ({ i1, i32 }).
            Type::Char => self.context.i32_type().into(),
            Type::Bool => self.context.bool_type().into(),
            // ADR 0060. A union value is a named LLVM struct
            // `{ i32 tag, [N x i64] payload }` where N = ceil(max
            // payload size / 8). The `[N x i64]` payload buffer
            // is 8-aligned, matching the maximum alignment of
            // any ALGOL26 type; stores into the buffer are
            // misaligned-UB-free for every payload shape.
            //
            // The struct type is cached by `UnionTypeId` so
            // same-name unions from different modules get
            // distinct LLVM types (mirroring `record_struct_types`
            // at ADR 0036).
            //
            // Nothing calls `map_type(Type::Union)` today —
            // `Feature::Unions` still refuses union programs at
            // the capability check. Session 2 and Session 3 of
            // ADR 0060 will reach this arm once the refusal is
            // lifted.
            Type::Union {
                id,
                name: _,
                variants,
            } => {
                // ADR 0060. A union value is a named LLVM struct
                // `{ i32 tag, [N x i64] payload }` where N = ceil(max
                // payload size / 8). The `[N x i64]` buffer is
                // 8-aligned, matching the maximum alignment of any
                // ALGOL26 type; stores into it are
                // misaligned-UB-free for every payload shape.
                //
                // The struct type is cached by `UnionTypeId` so
                // same-name unions from different modules get
                // distinct LLVM types (mirroring `record_struct_types`
                // at ADR 0036).
                //
                // Nothing calls `map_type(Type::Union)` today —
                // `Feature::Unions` still refuses union programs at
                // the capability check. Session 2 and Session 3 of
                // ADR 0060 will reach this arm once the refusal is
                // lifted.
                if let Some(cached) = self.union_struct_types.borrow().get(id).copied() {
                    return cached.into();
                }

                // Two local `fn`s, in dependency order:
                // `size_align` is used by `size_bytes`, so it
                // must come first. Rust requires local `fn`s to be
                // defined before their first use site, unlike
                // top-level items.
                //
                // Sizes are computed structurally from the LLVM
                // type's shape, not via `BasicTypeEnum::size_of()`
                // (whose return value's `get_zero_extended_constant`
                // can be `None` for compound constant
                // representations). The structural computation
                // handles struct padding by aligning each field to
                // its own alignment, then aligning the whole struct
                // to its max alignment.
                //
                // Pointer size is assumed to be 8 (native x86-64
                // / ARM64). WASM32 has 4-byte pointers, so WASM
                // union structs are slightly oversized but
                // functionally correct. Acceptable for v1.
                fn size_align(ty: BasicTypeEnum) -> u64 {
                    match ty {
                        BasicTypeEnum::IntType(t) => ((t.get_bit_width() as u64) + 7) / 8,
                        BasicTypeEnum::FloatType(_) => 8,
                        BasicTypeEnum::PointerType(_) => 8,
                        BasicTypeEnum::StructType(st) => {
                            let mut m = 1u64;
                            for i in 0..st.count_fields() {
                                let ft = st.get_field_type_at_index(i).unwrap();
                                m = m.max(size_align(ft));
                            }
                            m
                        }
                        BasicTypeEnum::ArrayType(at) => size_align(at.get_element_type()),
                        BasicTypeEnum::VectorType(_) => 8,
                        BasicTypeEnum::ScalableVectorType(_) => 8,
                    }
                }

                #[allow(clippy::manual_checked_ops)]
                fn size_bytes(ty: BasicTypeEnum) -> u64 {
                    match ty {
                        BasicTypeEnum::IntType(t) => ((t.get_bit_width() as u64) + 7) / 8,
                        BasicTypeEnum::FloatType(_) => 8,
                        BasicTypeEnum::PointerType(_) => 8,
                        BasicTypeEnum::StructType(st) => {
                            // Padding-aware struct size: align each
                            // field to its own alignment, sum, then
                            // round up to the struct's max
                            // alignment.
                            let mut offset: u64 = 0;
                            let mut max_align: u64 = 1;
                            for i in 0..st.count_fields() {
                                let ft = st.get_field_type_at_index(i).unwrap();
                                let falign = size_align(ft);
                                if falign != 0 {
                                    // Manual ceil-division. `div_ceil`
                                    // would be clearer but requires MSRV
                                    // 1.73, which this crate does not yet
                                    // target. The `#[allow(...)]` above
                                    // silences the resulting
                                    // `manual_checked_ops` lint.
                                    offset = (offset + falign - 1) / falign * falign;
                                }
                                offset += size_bytes(ft);
                                max_align = max_align.max(falign);
                            }
                            if max_align == 0 {
                                offset
                            } else {
                                (offset + max_align - 1) / max_align * max_align
                            }
                        }
                        BasicTypeEnum::ArrayType(at) => {
                            size_bytes(at.get_element_type()) * (at.len() as u64)
                        }
                        BasicTypeEnum::VectorType(_) => 16,
                        BasicTypeEnum::ScalableVectorType(_) => 16,
                    }
                }

                let i32_ty = self.context.i32_type();
                let i64_ty = self.context.i64_type();

                // Compute max payload size across all variants.
                // Payloadless variants (`Null`) contribute 0 and
                // do not affect the max.
                let mut max_size: u64 = 0;
                for (_variant_name, payload_types) in variants {
                    if payload_types.is_empty() {
                        continue;
                    }
                    let payload_ty = if payload_types.len() == 1 {
                        self.map_type(&payload_types[0])
                    } else {
                        let field_types: Vec<BasicTypeEnum<'ctx>> =
                            payload_types.iter().map(|t| self.map_type(t)).collect();
                        self.context.struct_type(&field_types, false).into()
                    };
                    max_size = max_size.max(size_bytes(payload_ty));
                }

                // `N = ceil(max_size / 8)`. Payloadless-only
                // unions produce N = 0 and `[0 x i64]`.
                let n_words = if max_size == 0 { 0 } else { (max_size + 7) / 8 };
                let payload_array_ty = i64_ty.array_type(n_words as u32);

                // `{ i32 tag, [N x i64] payload }`. Not packed, so
                // LLVM inserts 4 bytes of padding after the tag.
                // Payload field is at offset 8.
                let struct_ty = self
                    .context
                    .struct_type(&[i32_ty.into(), payload_array_ty.into()], false);

                self.union_struct_types.borrow_mut().insert(*id, struct_ty);
                struct_ty.into()
            }
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

            Type::List(_) => {
                // ADR 0042 phase 1b. A list's runtime value is a
                // `{ buffer: ptr, length: i64, capacity: i64 }`
                // descriptor. `capacity == 0` means `buffer` points
                // at a stack alloca owned by the enclosing frame;
                // `capacity > 0` means it points at a heap buffer
                // that the list owns and must free at scope exit.
                //
                // Phase 1a populated a `list_structs` side table with
                // one descriptor alloca per list name. This arm makes
                // `map_type` agree with that shape so a list value
                // can flow through `compile_value` as a struct.
                //
                // See docs/decisions/0042-llvm-dynamic-lists.md.
                let ptr_ty = self.context.ptr_type(AddressSpace::default());
                let i64_ty = self.context.i64_type();
                self.context
                    .struct_type(&[ptr_ty.into(), i64_ty.into(), i64_ty.into()], false)
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
                assert!(
                    args.is_empty(),
                    "LLVM codegen reached generic record type {:?}<{:?}> — \
                     the capability scan should have refused this program \
                     (ADR 0036). Reaching this arm means scan.rs failed \
                     to recognize a generic record, which is a compiler \
                     bug, not a user error.",
                    name,
                    args
                );

                if let Some(cached) = self.record_struct_types.borrow().get(name) {
                    return (*cached).into();
                }

                let rec = match self.record_decls.get(name) {
                    Some(r) => r,
                    None => unreachable!(
                        "LLVM codegen: unknown record '{}' — check_backend \
                         accepted the program but the record isn't in \
                         record_decls. This is a compiler bug.",
                        name
                    ),
                };

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
                    .map(|(_, field_ty, _)| self.map_type(&self.resolve_field_type(field_ty)))
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
                // ADR 0027. A Map's runtime value is an opaque
                // `AlgoIntMap*` (see runtime/map.c). The key and value
                // types are compile-time properties used by the
                // builtins; the LLVM representation is just `ptr`.
                //
                // Feature::Map is still NOT in
                // BackendCapabilities::llvm() — the capability check
                // continues to refuse Map<K,V> programs until the
                // partial lowering here handles all key/value
                // combinations. This arm exists so that a future
                // capability flip doesn't require regenerating
                // codegen; it is unreachable today.
                self.context.ptr_type(AddressSpace::default()).into()
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

            // ADR 0038 D4 placeholder. The capability matrix refuses
            // `&dyn Trait` / `&mut dyn Trait` on all three backends;
            // reaching this arm means the matrix is out of sync with
            // the IR. Real lowering (fat pointer `{ptr, ptr}`, vtable
            // emission, indirect calls) lands in D4.
            Type::DynTrait { .. } => unreachable!(
                "LLVM codegen reached Type::DynTrait — dynamic dispatch \
                 should have been refused by check_backend (ADR 0038)"
            ),
            // ADR 0041. Projections must be normalized before
            // codegen. Reaching here means the analyzer did not
            // rewrite `Self::Item` against the impl's binding.
            Type::Associated { .. } => unreachable!(
                "LLVM codegen reached Type::Associated — projection \
                 normalization should have rewritten it (ADR 0041)"
            ),

            // Should not appear in verified IR:
            // - `Unknown` is a type-inference placeholder.
            // - `TypeVar(_)` should be eliminated by monomorphization.
            // - `Generic { .. }` should be resolved to a concrete type
            //   before codegen.
            // If one reaches here, the IR verifier missed a case.
            Type::Unknown | Type::TypeVar(_) | Type::Generic { .. } => {
                // Fail closed. A verifier hole must be a compiler
                // crash, not silently-wrong codegen. Unlike
                // `debug_assert!`, `unreachable!` fires in release
                // builds too. See docs/design-principles.md.
                unreachable!(
                    "LLVM codegen reached unresolved type `{:?}`: \
                     the IR verifier missed this. This is a compiler bug, \
                     not a user error.",
                    ty
                );
            }
        }
    }
}

#[cfg(test)]
mod union_type_tests {
    use super::*;
    use crate::common::types::{Type, UnionTypeId};
    use inkwell::context::Context;

    fn make_codegen<'ctx>(ctx: &'ctx Context) -> IRCodeGen<'ctx> {
        IRCodeGen::new(ctx, "test_module")
    }

    #[test]
    fn union_payloadless_only_is_zero_sized_payload() {
        let ctx = Context::create();
        let cg = make_codegen(&ctx);
        let ty = Type::Union {
            id: UnionTypeId(1),
            name: "Empty".to_string(),
            variants: vec![("A".to_string(), vec![]), ("B".to_string(), vec![])],
        };
        let llvm_ty = cg.map_type(&ty);
        let st = match llvm_ty {
            BasicTypeEnum::StructType(st) => st,
            other => panic!("expected struct, got {:?}", other),
        };
        assert_eq!(st.count_fields(), 2, "tag + payload");
        let payload_arr = match st.get_field_type_at_index(1).unwrap() {
            BasicTypeEnum::ArrayType(a) => a,
            other => panic!("expected array payload, got {:?}", other),
        };
        assert_eq!(payload_arr.len(), 0, "N = 0 for payloadless-only union");
        assert_eq!(
            payload_arr.get_element_type(),
            BasicTypeEnum::IntType(cg.context.i64_type()),
            "payload element type is i64"
        );
    }

    #[test]
    fn union_with_int_payload_is_one_i64_word() {
        let ctx = Context::create();
        let cg = make_codegen(&ctx);
        let ty = Type::Union {
            id: UnionTypeId(2),
            name: "MaybeInt".to_string(),
            variants: vec![
                ("None".to_string(), vec![]),
                ("Some".to_string(), vec![Type::Int]),
            ],
        };
        let llvm_ty = cg.map_type(&ty);
        let st = match llvm_ty {
            BasicTypeEnum::StructType(st) => st,
            other => panic!("expected struct, got {:?}", other),
        };
        let payload_arr = match st.get_field_type_at_index(1).unwrap() {
            BasicTypeEnum::ArrayType(a) => a,
            other => panic!("expected array payload, got {:?}", other),
        };
        assert_eq!(payload_arr.len(), 1);
    }

    #[test]
    fn union_with_string_and_int_payload_is_one_i64_word() {
        let ctx = Context::create();
        let cg = make_codegen(&ctx);
        let ty = Type::Union {
            id: UnionTypeId(3),
            name: "Json".to_string(),
            variants: vec![
                ("Null".to_string(), vec![]),
                ("Num".to_string(), vec![Type::Int]),
                ("Str".to_string(), vec![Type::String]),
            ],
        };
        let llvm_ty = cg.map_type(&ty);
        let st = match llvm_ty {
            BasicTypeEnum::StructType(st) => st,
            other => panic!("expected struct, got {:?}", other),
        };
        let payload_arr = match st.get_field_type_at_index(1).unwrap() {
            BasicTypeEnum::ArrayType(a) => a,
            other => panic!("expected array payload, got {:?}", other),
        };
        assert_eq!(payload_arr.len(), 1);
    }

    #[test]
    fn union_with_list_descriptor_payload_is_three_i64_words() {
        let ctx = Context::create();
        let cg = make_codegen(&ctx);
        let ty = Type::Union {
            id: UnionTypeId(4),
            name: "JsonArr".to_string(),
            variants: vec![
                ("Null".to_string(), vec![]),
                ("Arr".to_string(), vec![Type::list(Type::Int)]),
            ],
        };
        let llvm_ty = cg.map_type(&ty);
        let st = match llvm_ty {
            BasicTypeEnum::StructType(st) => st,
            other => panic!("expected struct, got {:?}", other),
        };
        let payload_arr = match st.get_field_type_at_index(1).unwrap() {
            BasicTypeEnum::ArrayType(a) => a,
            other => panic!("expected array payload, got {:?}", other),
        };
        assert_eq!(payload_arr.len(), 3, "24 bytes / 8 = 3 i64 words");
    }

    #[test]
    fn union_type_is_cached() {
        let ctx = Context::create();
        let cg = make_codegen(&ctx);
        let ty = Type::Union {
            id: UnionTypeId(5),
            name: "Cached".to_string(),
            variants: vec![("X".to_string(), vec![Type::Int])],
        };
        let first = cg.map_type(&ty);
        let second = cg.map_type(&ty);
        assert_eq!(first, second);
    }
}
