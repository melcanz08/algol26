// src/backends/llvm_codegen/value.rs

use super::resolve_math_name;
use super::IRCodeGen;
use crate::common::diagnostics::{CompileError, ErrorCode, Result};
use crate::common::types::Type;
use crate::ir::semantic_ir::TypedIRValue;
use inkwell::types::{BasicType, BasicTypeEnum};
use inkwell::values::BasicValueEnum;
use inkwell::AddressSpace;

impl<'ctx> IRCodeGen<'ctx> {
    /// Compile an expression as a *reference* — i.e., produce the
    /// address of its inner value rather than loading from it. Used by
    /// `Borrow` and `MutBorrow` (and `AddrOf` could delegate here too).
    ///
    /// Fail-closed: an unknown variable is an error, not a null
    /// pointer. A silent null would produce a program that segfaults
    /// at runtime instead of failing at compile time.
    pub(super) fn compile_reference(&self, expr: &TypedIRValue) -> Result<BasicValueEnum<'ctx>> {
        match expr {
            TypedIRValue::Variable(name, _) => self
                .variables
                .get(name)
                .map(|p| (*p).into())
                .ok_or_else(|| {
                    CompileError::unsupported_operation(
                        &format!("reference to undefined variable `{}`", name),
                        "llvm",
                    )
                }),
            // A reference-to-reference yields the inner reference.
            TypedIRValue::BorrowShared { expr, .. } | TypedIRValue::BorrowMutable { expr, .. } => {
                self.compile_value(expr)
            }
            // Reference-to-element: GEP to the element's address,
            // no load. The array's alloca lives in `list_arrays`
            // under the source variable's name; its LLVM type is in
            // `list_array_types`. Those two maps are the authoritative
            // source for a list's backing storage — `map_type(List)`
            // is only an opaque `ptr`, so it cannot be used here.
            TypedIRValue::ArrayAccess { array, index, .. } => {
                let array_name = match &**array {
                    TypedIRValue::Variable(n, _) => n.clone(),
                    _ => {
                        return Err(CompileError::unsupported_operation(
                            "reference to element of non-variable array \
                             (nested arrays not supported)",
                            "llvm",
                        ));
                    }
                };
                // ADR 0042 phase 4. Runtime buffer load when a
                // descriptor exists.
                let arr_ptr = self.list_buffer_value(&array_name).ok_or_else(|| {
                    CompileError::unsupported_operation(
                        &format!("reference to unknown array `{}`", array_name),
                        "llvm",
                    )
                })?;
                let arr_ty = self
                    .list_array_types
                    .get(&array_name)
                    .copied()
                    .ok_or_else(|| {
                        CompileError::unsupported_operation(
                            &format!("array `{}` has no registered LLVM type", array_name),
                            "llvm",
                        )
                    })?;

                // Evaluate the index, truncating to i32 to match the
                // static-index GEPs used when the array was built.
                let idx_val = self.compile_value(index)?;
                let idx_int = match idx_val {
                    BasicValueEnum::IntValue(v) => {
                        if v.get_type().get_bit_width() == 32 {
                            v
                        } else {
                            self.builder
                                .build_int_truncate(v, self.context.i32_type(), "idx32")
                                .unwrap()
                        }
                    }
                    _ => {
                        return Err(CompileError::unsupported_operation(
                            "array index must be an integer",
                            "llvm",
                        ));
                    }
                };

                let ptr = unsafe {
                    self.builder
                        .build_gep(
                            arr_ty,
                            arr_ptr,
                            &[self.context.i32_type().const_zero(), idx_int],
                            "elem_ref",
                        )
                        .unwrap()
                };
                Ok(ptr.into())
            }
            // Anything else falls back to producing a value; the caller
            // (or verifier) is responsible for ensuring it's used as a
            // reference only when the inner form is addressable.
            other => self.compile_value(other),
        }
    }

    pub(super) fn compile_value(&self, val: &TypedIRValue) -> Result<BasicValueEnum<'ctx>> {
        Ok(match val {
            TypedIRValue::Int(i) => self.context.i64_type().const_int(*i as u64, true).into(),
            TypedIRValue::Float(f) => self.context.f64_type().const_float(*f).into(),
            TypedIRValue::Bool(b) => self
                .context
                .bool_type()
                .const_int(if *b { 1 } else { 0 }, false)
                .into(),
            TypedIRValue::String(s) => {
                let global = self.builder.build_global_string_ptr(s, "strtmp").unwrap();
                global.as_pointer_value().into()
            }
            TypedIRValue::Void => self.context.f64_type().const_float(0.0).into(),
            TypedIRValue::NullPtr => self
                .context
                .ptr_type(AddressSpace::default())
                .const_null()
                .into(),
            // `PtrLiteral(p)` carries an absolute pointer value as an
            // integer. Lower it to a real pointer with `inttoptr` so
            // the resulting LLVM value has pointer type, matching the
            // IR type `Type::Ptr`.
            TypedIRValue::PtrLiteral(p) => {
                let i64_ty = self.context.i64_type();
                let ptr_ty = self.context.ptr_type(AddressSpace::default());
                let int_val = i64_ty.const_int(*p as u64, false);
                self.builder
                    .build_int_to_ptr(int_val, ptr_ty, "ptr_literal")
                    .unwrap()
                    .into()
            }
            // ADR 0038 D4a. A `&dyn Trait` value is a fat pointer
            // `{ data: ptr, vtable: ptr }`. `data` compiles through
            // `compile_reference` — the borrow's inner expression is
            // addressable, so this yields a pointer to the concrete
            // storage. The vtable half comes from the global emitted
            // by `IRCodeGen::emit_vtables`. The value is an alloca of
            // the fat-pointer struct; callers load it by value when
            // passing to a callee (D4b).
            TypedIRValue::DynTrait {
                data,
                vtable_id,
                target_type: _,
            } => {
                let data_val = self.compile_reference(data)?;
                let vtable_global = self.vtables.get(vtable_id).copied().ok_or_else(|| {
                    CompileError::unsupported_operation(
                        &format!(
                            "no vtable emitted for `{}` — check_backend should                              have refused this program (ADR 0038)",
                            vtable_id,
                        ),
                        "llvm",
                    )
                })?;
                let vtable_ptr = vtable_global.as_pointer_value();

                let ptr_ty = self.context.ptr_type(AddressSpace::default());
                let fat_ty = self
                    .context
                    .struct_type(&[ptr_ty.into(), ptr_ty.into()], false);
                let alloca = self.builder.build_alloca(fat_ty, "dyn_trait_fat").unwrap();
                let data_slot = self
                    .builder
                    .build_struct_gep(fat_ty, alloca, 0, "data_slot")
                    .unwrap();
                let vtable_slot = self
                    .builder
                    .build_struct_gep(fat_ty, alloca, 1, "vtable_slot")
                    .unwrap();
                self.builder.build_store(data_slot, data_val).unwrap();
                self.builder.build_store(vtable_slot, vtable_ptr).unwrap();
                alloca.into()
            }
            // ADR 0038 D4b. A virtual call can appear nested as
            // another value's operand — `print(s.area())` carries the
            // VirtualCall as Print's operand; `f(s.area())` carries
            // it as a call argument. Lower it inline: load the fat
            // pointer halves, GEP to the vtable slot, load the method
            // pointer, indirect-call it with `data` as the receiver.
            TypedIRValue::VirtualCall {
                receiver,
                method_name: _,
                slot,
                args,
                return_type,
            } => {
                let ptr_ty = self.context.ptr_type(AddressSpace::default());
                let fat_ty = self
                    .context
                    .struct_type(&[ptr_ty.into(), ptr_ty.into()], false);

                // Load the receiver's slot (a `ptr` to the fat
                // pointer struct). See the sibling arm in
                // `instruction.rs` for why this is `compile_value`,
                // not `compile_reference`.
                let fat_val = self.compile_value(receiver)?;
                let fat_ptr = fat_val.into_pointer_value();

                let data_slot = self
                    .builder
                    .build_struct_gep(fat_ty, fat_ptr, 0, "dyn_data_slot")
                    .unwrap();
                let vtable_slot = self
                    .builder
                    .build_struct_gep(fat_ty, fat_ptr, 1, "dyn_vtable_slot")
                    .unwrap();
                let data_ptr = self
                    .builder
                    .build_load(ptr_ty, data_slot, "dyn_data")
                    .unwrap()
                    .into_pointer_value();
                let vtable_ptr = self
                    .builder
                    .build_load(ptr_ty, vtable_slot, "dyn_vtable")
                    .unwrap()
                    .into_pointer_value();

                let slot_const = self.context.i32_type().const_int(*slot as u64, false);
                let method_slot_ptr = unsafe {
                    self.builder
                        .build_gep(ptr_ty, vtable_ptr, &[slot_const], "method_slot")
                        .unwrap()
                };
                let method_fn = self
                    .builder
                    .build_load(ptr_ty, method_slot_ptr, "method_fn")
                    .unwrap()
                    .into_pointer_value();

                let ret_ty = match return_type {
                    Type::Void => None,
                    other => Some(self.map_type(other)),
                };
                let mut param_types: Vec<inkwell::types::BasicMetadataTypeEnum> =
                    vec![ptr_ty.into()];
                let mut arg_vals: Vec<BasicValueEnum> = Vec::with_capacity(args.len() + 1);
                arg_vals.push(data_ptr.into());
                for a in args {
                    let v = self.compile_value(a)?;
                    param_types.push(v.get_type().into());
                    arg_vals.push(v);
                }
                let fn_ty = match ret_ty {
                    Some(rt) => rt.fn_type(&param_types, false),
                    None => self.context.void_type().fn_type(&param_types, false),
                };

                let call_args: Vec<inkwell::values::BasicMetadataValueEnum> =
                    arg_vals.iter().map(|v| (*v).into()).collect();
                let call_site = self
                    .builder
                    .build_indirect_call(fn_ty, method_fn, &call_args, "vcall")
                    .unwrap();

                match call_site.try_as_basic_value() {
                    inkwell::values::ValueKind::Basic(v) => v,
                    inkwell::values::ValueKind::Instruction(_) => {
                        // Void-returning virtual call used in value
                        // position. The analyzer should have rejected
                        // this at the source; produce a dummy so we
                        // don't panic.
                        self.context.f64_type().const_float(0.0).into()
                    }
                }
            }
            // List literals have no LLVM lowering in the current
            // backend. Before this fix, the arm returned `null` — a
            // silent wrong-code bug. Now it errors.
            TypedIRValue::List(_, _) => {
                return Err(CompileError::unsupported_operation(
                    "list literal value (use List.length or iterate instead)",
                    "llvm",
                ));
            }
            TypedIRValue::Record {
                name,
                fields,
                record_type,
            } => {
                // ADR 0036 L2. A record literal becomes an alloca of
                // the record's LLVM struct type, with each field
                // stored into its slot. The expression's value is a
                // pointer to that alloca.
                //
                // Records are still refused by the capability check,
                // so this code path is unreachable until L1e. Its
                // shape mirrors the list-literal handling in
                // `instruction.rs:22`: allocate once, store each
                // element, return the pointer.
                let struct_ty = match self.map_type(record_type) {
                    BasicTypeEnum::StructType(s) => s,
                    _ => unreachable!(
                        "map_type(Type::Record) returned a non-struct type for '{}'",
                        name
                    ),
                };
                let alloca = self
                    .builder
                    .build_alloca(struct_ty, &format!("{}_tmp", name))
                    .unwrap();
                let rec_decl = match self.record_decls.get(name) {
                    Some(r) => r,
                    None => {
                        unreachable!("record '{}' missing from record_decls — compiler bug", name)
                    }
                };
                for (i, (fname, fval)) in fields.iter().enumerate() {
                    // Field ordering must match the struct type's
                    // declaration order. `map_type` built the struct
                    // from `rec_decl.fields`, so indices line up as
                    // long as the literal lists fields in the same
                    // order — which the analyzer enforces.
                    let _ = fname;
                    let fptr = unsafe {
                        self.builder
                            .build_gep(
                                struct_ty,
                                alloca,
                                &[
                                    self.context.i32_type().const_zero(),
                                    self.context.i32_type().const_int(i as u64, false),
                                ],
                                &format!("{}_field_{}", name, i),
                            )
                            .unwrap()
                    };
                    let fv = self.compile_value(fval)?;

                    // A nested record literal compiles to a pointer to
                    // its own alloca (see this arm's tail, where
                    // `alloca.into()` is the return value). The
                    // enclosing struct's field slot is the struct type
                    // itself — `%Outer = type { %Inner }`, not
                    // `{ ptr }` — so storing the pointer would put 8
                    // bytes of address in a slot that downstream code
                    // reads as the struct's value. Load the struct
                    // through the pointer before storing, matching the
                    // fix pattern in A3 (List<Record> slot-store).
                    let field_llvm_ty = struct_ty
                        .get_field_type_at_index(i as u32)
                        .expect("record struct has fewer fields than its literal");
                    let fv = match (fv, &field_llvm_ty) {
                        (BasicValueEnum::PointerValue(p), BasicTypeEnum::StructType(st)) => self
                            .builder
                            .build_load(*st, p, &format!("{}_field_{}_load", name, i))
                            .unwrap(),
                        (v, _) => v,
                    };

                    self.builder.build_store(fptr, fv).unwrap();
                }
                let _ = rec_decl;
                alloca.into()
            }
            TypedIRValue::Map { .. } => {
                unreachable!(
                    "LLVM codegen reached TypedIRValue::Map — \
                     maps should have been refused by check_backend"
                );
            }
            TypedIRValue::Variable(name, _) => {
                // ALGOL26: UNDEFINED VARIABLE IS AN ERROR, not 0.0!
                let ptr = self.variables.get(name).ok_or_else(|| {
                    CompileError::simple(
                        &format!("Undefined variable '{}' during LLVM codegen", name),
                        0,
                        0,
                        "",
                        ErrorCode::E0003,
                    )
                })?;

                // `variables` and `var_types` are updated in
                // lockstep by every producer (`Declare`, `Assign`,
                // `Allocate`, list-init, iterator-init, parameter
                // bind). A variable in one but not the other means
                // a producer updated only half the invariant. Fail
                // closed rather than defaulting to Float — the
                // previous `unwrap_or(Type::Float)` would silently
                // load an f64 from a non-float slot.
                let ty = self.var_types.get(name).cloned().ok_or_else(|| {
                    CompileError::simple(
                        &format!(
                            "LLVM codegen: variable `{}` has an alloca but no \
                             recorded type. A producer added it to `variables` \
                             without also adding it to `var_types`.",
                            name
                        ),
                        0,
                        0,
                        "",
                        ErrorCode::E0009,
                    )
                })?;

                // ADR 0042 phase 1b. A list variable's value is
                // the {buffer, length, capacity} descriptor, not the
                // buffer pointer. The descriptor alloca is registered
                // in `list_structs` by every producer (phase 1a: list
                // literals, list moves; phase 1b: list parameters).
                // Loading it here is what makes a list value flow
                // through `compile_value` as a struct, matching the
                // ABI `declare_function` now declares.
                //
                // A reference parameter (`self: &Point`, `x: &mut T`)
                // is analogous to the old behavior: the incoming
                // pointer IS the reference. That branch is unchanged
                // below.
                if matches!(ty, Type::List(_)) {
                    let struct_alloca = self.list_structs.get(name).copied().ok_or_else(|| {
                        CompileError::simple(
                            &format!(
                                "LLVM codegen: list `{}` has no descriptor struct; \
                                 a producer registered it in `variables` without \
                                 populating `list_structs`",
                                name
                            ),
                            0,
                            0,
                            "",
                            ErrorCode::E0009,
                        )
                    })?;
                    let struct_ty = match self.map_type(&ty) {
                        BasicTypeEnum::StructType(s) => s,
                        _ => unreachable!("map_type(Type::List) returned non-struct"),
                    };
                    return Ok(self
                        .builder
                        .build_load(struct_ty, struct_alloca, name)
                        .unwrap());
                }
                // A reference *parameter* has the pointer directly in
                // `variables[name]`; its value is the reference
                // itself. A local reference binding (`val r := &x`)
                // is created by `Declare` via `create_entry_alloca`,
                // so `variables[r]` is an alloca *holding* a pointer
                // and needs a load. Both have `var_types` of
                // `Borrow(_)`, so the type alone can't distinguish
                // them; `ref_param_vars` carries the signal.
                if matches!(ty, Type::Borrow(_) | Type::MutBorrow(_))
                    && self.ref_param_vars.contains(name)
                {
                    return Ok((*ptr).into());
                }
                let llvm_ty = self.map_type(&ty);
                self.builder.build_load(llvm_ty, *ptr, name).unwrap()
            }
            TypedIRValue::BinaryOp {
                op,
                left,
                right,
                result_type,
            } => {
                let l = self.compile_value(left)?;
                let r = self.compile_value(right)?;
                self.compile_binop(op, l, r)?
            }
            TypedIRValue::Call {
                function,
                args,
                return_type,
            } => {
                // Propagate errors from argument compilation — was
                // previously `.unwrap()`, which panicked on any nested
                // codegen failure instead of surfacing it.
                let arg_vals: Vec<BasicValueEnum> = args
                    .iter()
                    .map(|a| self.compile_value(a))
                    .collect::<Result<Vec<_>>>()?;
                let callee_name = function.trim_end_matches("()").to_string();

                // ADR 0042 phase 4. `xs.append(v)` in expression
                // position (a loop body, an if-branch) routes through
                // this arm, not `compile_instruction(Call)`. Dispatch
                // to the same helper and return a Void placeholder —
                // `compile_value` needs a value, and `Void` has no
                // LLVM representation, so an f64 dummy is the least
                // surprising placeholder (matches `TypedIRValue::Void`
                // lowering elsewhere).
                if callee_name == "List.append" {
                    self.emit_list_append(args)?;
                    return Ok(self.context.f64_type().const_float(0.0).into());
                }

                // `Math.*` names are registered in the LLVM module
                // under their unmangled C names (sqrt, pow, fabs, ...).
                // Translate before lookup so the call finds them.
                let llvm_name = resolve_math_name(&callee_name)
                    .map(|s| s.to_string())
                    .unwrap_or_else(|| callee_name.clone());

                if let Some(callee) = self
                    .module
                    .get_function(&llvm_name)
                    .or_else(|| self.functions.get(&llvm_name).cloned())
                {
                    // Same coercion as the Instruction::Call form:
                    // see `coerce_arg_to_param` in mod.rs.
                    let coerced: Vec<BasicValueEnum> = arg_vals
                        .iter()
                        .enumerate()
                        .map(|(i, v)| self.coerce_arg_to_param(callee, i, *v))
                        .collect::<Result<Vec<_>>>()?;
                    let call_args: Vec<inkwell::values::BasicMetadataValueEnum> =
                        coerced.iter().map(|v| (*v).into()).collect();
                    let call_site = self
                        .builder
                        .build_call(callee, &call_args, "calltmp")
                        .unwrap();
                    match call_site.try_as_basic_value() {
                        // A record-returning call yields a struct
                        // value. Store it in a fresh alloca and use
                        // the alloca pointer as the value, matching
                        // how `TypedIRValue::Record{...}` is
                        // represented.
                        inkwell::values::ValueKind::Basic(v)
                            if matches!(return_type, Type::Record(..)) && v.is_struct_value() =>
                        {
                            let alloca = self
                                .builder
                                .build_alloca(v.get_type(), "call_rec_tmp")
                                .unwrap();
                            self.builder.build_store(alloca, v).unwrap();
                            alloca.into()
                        }
                        inkwell::values::ValueKind::Basic(v) => v,
                        // A call that returns void cannot be used as a
                        // value. The IR should have used
                        // `Instruction::Call` instead of
                        // `TypedIRValue::Call` for such calls. Fail
                        // closed rather than silently returning 0.0.
                        inkwell::values::ValueKind::Instruction(_) => {
                            return Err(CompileError::unsupported_operation(
                                &format!(
                                    "call to `{}` yields no value; use an \
                                     `Instruction::Call` form instead of \
                                     `TypedIRValue::Call`",
                                    callee_name
                                ),
                                "llvm",
                            ));
                        }
                    }
                } else {
                    // `compile_builtin_value` now returns
                    // `(BasicValueEnum, Type)`; discard the type here
                    // because the caller of `compile_value` doesn't
                    // need it.
                    self.compile_builtin_value(&callee_name, args)?.0
                }
            }
            TypedIRValue::ArrayAccess {
                array,
                index,
                element_type,
            } => {
                let arr_name = match array.as_ref() {
                    TypedIRValue::Variable(n, _) => Some(n.clone()),
                    _ => None,
                };

                if let Some(arr_name) = arr_name {
                    // ADR 0042 phase 4. Runtime buffer load when a
                    // descriptor exists — the static map is stale
                    // after any append grew the list.
                    if let Some(arr_ptr) = self.list_buffer_value(&arr_name) {
                        let arr_ty = self
                            .list_array_types
                            .get(&arr_name)
                            .cloned()
                            .unwrap_or_else(|| self.context.f64_type().array_type(0).into());
                        let idx_val = self.compile_value(index)?;

                        // ADR 0042 phase 2. Bounds checking now
                        // works for every list, including
                        // parameters: the length comes from the
                        // descriptor struct when no compile-time
                        // hint exists. A `None` from the helper
                        // means no producer registered a length --
                        // a compiler bug, fail closed rather than
                        // silently skip the check.
                        if idx_val.is_int_value() {
                            let idx_int = idx_val.into_int_value();
                            let len_val = self.list_length_value(&arr_name).ok_or_else(|| {
                                CompileError::simple(
                                    &format!(
                                        "LLVM codegen: bounds check on `{}` \
                                         has no length source (no `list_lengths` \
                                         entry and no `list_structs` descriptor)",
                                        arr_name
                                    ),
                                    0,
                                    0,
                                    "",
                                    ErrorCode::E0009,
                                )
                            })?;

                            // Check idx >= 0
                            let zero = self.context.i64_type().const_int(0, false);
                            let is_negative = self
                                .builder
                                .build_int_compare(
                                    inkwell::IntPredicate::SLT,
                                    idx_int,
                                    zero,
                                    "bounds_check_neg",
                                )
                                .unwrap();

                            // Check idx < len
                            let is_too_big = self
                                .builder
                                .build_int_compare(
                                    inkwell::IntPredicate::SGE,
                                    idx_int,
                                    len_val,
                                    "bounds_check_big",
                                )
                                .unwrap();

                            let out_of_bounds = self
                                .builder
                                .build_or(is_negative, is_too_big, "out_of_bounds")
                                .unwrap();

                            // Create error block
                            let current_bb = self.builder.get_insert_block().unwrap();
                            let error_bb = self
                                .context
                                .append_basic_block(self.current_function.unwrap(), "bounds_error");
                            let continue_bb = self
                                .context
                                .append_basic_block(self.current_function.unwrap(), "bounds_ok");

                            self.builder
                                .build_conditional_branch(out_of_bounds, error_bb, continue_bb)
                                .unwrap();

                            // Error block: print error and exit
                            self.builder.position_at_end(error_bb);
                            let error_msg = self
                                .builder
                                .build_global_string_ptr(
                                    "Error: Array index out of bounds\n",
                                    "bounds_error_msg",
                                )
                                .unwrap();
                            let printf_fn = self.module.get_function("printf").unwrap();
                            self.builder
                                .build_call(
                                    printf_fn,
                                    &[error_msg.as_pointer_value().into()],
                                    "print_error",
                                )
                                .unwrap();
                            // ADR 0042 phase 2. The error block calls
                            // libc `exit(1)` rather than emitting a
                            // `return`. Returning from inside the bounds
                            // error handed control back to the caller
                            // (e.g. a `print(a[0])` after a failed index
                            // would run on stale state) and forced a
                            // return-type match against the enclosing
                            // function's signature — a `ret i32 1`
                            // inside an `i64`-returning function is
                            // invalid IR. `exit` is `noreturn` and
                            // aborts the program, matching the
                            // interpreter's semantics.
                            let exit_fn = self.module.get_function("exit").ok_or_else(|| {
                                CompileError::simple(
                                    "LLVM codegen: exit not registered in stdlib",
                                    0,
                                    0,
                                    "",
                                    ErrorCode::E0009,
                                )
                            })?;
                            let status = self.context.i32_type().const_int(1, false);
                            self.builder
                                .build_call(exit_fn, &[status.into()], "oob_exit")
                                .unwrap();
                            self.builder.build_unreachable().unwrap();

                            // Continue block
                            self.builder.position_at_end(continue_bb);
                        }

                        let idx_i32 = if idx_val.is_int_value() {
                            let iv = idx_val.into_int_value();
                            if iv.get_type().get_bit_width() != 32 {
                                self.builder
                                    .build_int_cast(iv, self.context.i32_type(), "idx32")
                                    .unwrap()
                            } else {
                                iv
                            }
                        } else {
                            self.context.i32_type().const_zero()
                        };

                        let elem_ptr = unsafe {
                            self.builder
                                .build_gep(
                                    arr_ty,
                                    arr_ptr,
                                    &[self.context.i32_type().const_zero(), idx_i32],
                                    "arr_access",
                                )
                                .unwrap()
                        };

                        let elem_llvm_ty = self.map_type(element_type);
                        self.builder
                            .build_load(elem_llvm_ty, elem_ptr, "elem_load")
                            .unwrap()
                    } else {
                        return Err(CompileError::simple(
                            &format!(
                                "codegen: ArrayAccess on unknown list '{}' (known lists: {:?})",
                                arr_name,
                                self.list_arrays.keys().collect::<Vec<_>>()
                            ),
                            0,
                            0,
                            "",
                            ErrorCode::E0004,
                        ));
                    }
                } else {
                    return Err(CompileError::simple(
                        "codegen: ArrayAccess with non-variable array expression",
                        0,
                        0,
                        "",
                        ErrorCode::E0004,
                    ));
                }
            }
            TypedIRValue::Cast { value, target_type } => {
                let v = self.compile_value(value)?;
                // ALGOL26: Check ACTUAL LLVM type, not IR type.
                match (&v, target_type) {
                    (BasicValueEnum::IntValue(iv), Type::Float) => self
                        .builder
                        .build_signed_int_to_float(*iv, self.context.f64_type(), "i2f")
                        .unwrap()
                        .into(),
                    (BasicValueEnum::FloatValue(fv), Type::Int) => self
                        .builder
                        .build_float_to_signed_int(*fv, self.context.i64_type(), "f2i")
                        .unwrap()
                        .into(),
                    (BasicValueEnum::FloatValue(_), Type::Float) => {
                        // Already a float - no cast needed
                        v
                    }
                    (BasicValueEnum::IntValue(_), Type::Int) => {
                        // Already an int - no cast needed
                        v
                    }
                    // String is a `char*` pointer in LLVM. A cast whose
                    // source is already a pointer and whose target is
                    // `String` is a no-op — this is the shape the
                    // nominal unwrap `Distinct<String> -> String`
                    // produces (ADR 0029).
                    (BasicValueEnum::PointerValue(_), Type::String) => v,
                    // ADR 0030: enum wrap. The value's LLVM type is
                    // i64 (matching `map_type(Type::Enum)`), so the
                    // cast is a no-op. No runtime bounds check in v1;
                    // the analyzer rejects out-of-range literals.
                    (BasicValueEnum::IntValue(_), Type::Enum { .. }) => v,
                    // ADR 0031: subrange wrap/unwrap. `map_type`
                    // unwraps Subrange to its base, so the LLVM value
                    // is always an i64 for Int and enum bases. Both
                    // directions are no-ops at the LLVM level. The
                    // runtime bounds check is a separate instruction
                    // (BoundsCheck); it fires before this cast on the
                    // construct path.
                    (BasicValueEnum::IntValue(_), Type::Subrange { .. }) => v,
                    // ADR 0029: nominal wrap/unwrap. The nominal and
                    // its base share the same runtime representation,
                    // so the cast is a no-op. The inner value's LLVM
                    // type already matches the base.
                    (_, Type::Distinct { base, .. }) => match (&v, &**base) {
                        (BasicValueEnum::IntValue(_), Type::Int | Type::Bool) => v,
                        (BasicValueEnum::FloatValue(_), Type::Float) => v,
                        (BasicValueEnum::PointerValue(_), Type::String) => v,
                        _ => {
                            return Err(CompileError::unsupported_operation(
                                "cast to nominal type with mismatched LLVM representation",
                                "llvm",
                            ));
                        }
                    },
                    (source_llvm, target) => {
                        // Reaching this arm means the analyzer permitted
                        // a cast the LLVM backend cannot express. The
                        // verifier's cast-legality check should have
                        // rejected it before codegen.
                        return Err(CompileError::simple(
                            &format!(
                                "LLVM codegen: cannot lower cast from LLVM value \
                                 of kind {:?} to {:?}",
                                source_llvm, target
                            ),
                            0,
                            0,
                            "",
                            ErrorCode::E0002,
                        ));
                    }
                }
            }
            TypedIRValue::BorrowShared { expr, .. } => self.compile_reference(expr)?,

            TypedIRValue::BorrowMutable { expr, .. } => self.compile_reference(expr)?,
            TypedIRValue::ReadReference { expr, target_type } => {
                let ptr = self.compile_value(expr)?;
                if !ptr.is_pointer_value() {
                    return Err(CompileError::unsupported_operation(
                        &format!("read through non-pointer value (kind {:?})", ptr),
                        "llvm",
                    ));
                }
                let llvm_ty = self.map_type(target_type);
                self.builder
                    .build_load(llvm_ty, ptr.into_pointer_value(), "read_ref_load")
                    .unwrap()
            }
            TypedIRValue::AddrOf { expr, .. } => {
                if let TypedIRValue::Variable(name, _) = expr.as_ref() {
                    self.variables
                        .get(name)
                        .map(|p| (*p).into())
                        .ok_or_else(|| {
                            CompileError::unsupported_operation(
                                &format!("address-of undefined variable `{}`", name),
                                "llvm",
                            )
                        })?
                } else {
                    self.compile_value(expr)?
                }
            }
            // These four variants have no LLVM lowering — the
            // LLVM backend does not model Option<T> or Result<T,E>
            // as tagged unions. The capability scan refuses
            // programs that would produce them, so reaching this
            // code means the scan was bypassed or the IR builder
            // emitted something the capability system missed.
            //
            // Error out defensively instead of silently unwrapping
            // (which is what the code did before PR-13c and was the
            // source of a real wrong-code bug).
            // Option<T> lowers to `{ bool is_some, T payload }`. The
            // aggregate is built with `insertvalue` on `undef` and
            // returned as a value; the surrounding Declare stores it
            // into the variable's alloca (whose type is the same
            // struct via `map_type(Type::Option(_))`).
            TypedIRValue::Some(inner) => {
                let inner_val = self.compile_value(inner)?;
                let inner_ty = inner.type_of();
                let opt_ty = Type::option(inner_ty);
                let struct_ty = match self.map_type(&opt_ty) {
                    BasicTypeEnum::StructType(st) => st,
                    _ => {
                        return Err(CompileError::unsupported_operation(
                            "Option<T> did not map to an LLVM struct type",
                            "llvm",
                        ));
                    }
                };
                let tag = self.context.bool_type().const_int(1, false);
                let with_tag = self
                    .builder
                    .build_insert_value(struct_ty.get_undef(), tag, 0, "some_tag")
                    .unwrap()
                    .into_struct_value();
                let with_payload = self
                    .builder
                    .build_insert_value(with_tag, inner_val, 1, "some_payload")
                    .unwrap()
                    .into_struct_value();
                with_payload.into()
            }
            TypedIRValue::None { option_type } => {
                let struct_ty = match self.map_type(option_type) {
                    BasicTypeEnum::StructType(st) => st,
                    _ => {
                        return Err(CompileError::unsupported_operation(
                            "None did not map to an LLVM struct type",
                            "llvm",
                        ));
                    }
                };
                let tag = self.context.bool_type().const_zero();
                let with_tag = self
                    .builder
                    .build_insert_value(struct_ty.get_undef(), tag, 0, "none_tag")
                    .unwrap()
                    .into_struct_value();
                with_tag.into()
            }
            TypedIRValue::Ok { .. } => {
                return Err(CompileError::unsupported_operation(
                    "Ok(...) (Result<T, E> has no LLVM lowering)",
                    "llvm",
                ));
            }
            TypedIRValue::Error { .. } => {
                return Err(CompileError::unsupported_operation(
                    "Error(...) (Result<T, E> has no LLVM lowering)",
                    "llvm",
                ));
            }

            // These variants have no LLVM lowering. The capability
            // matrix refuses programs that would produce them, so
            // reaching this point means the IR builder emitted
            // something the backend cannot handle — a compiler bug,
            // not a program the user should have written.
            TypedIRValue::Array(_, _, _) => {
                return Err(CompileError::unsupported_operation(
                    "array literal value",
                    "llvm",
                ));
            }
            TypedIRValue::Range(_, _) => {
                return Err(CompileError::unsupported_operation("range value", "llvm"));
            }
            TypedIRValue::FieldAccess {
                object,
                field,
                field_type,
            } => {
                // ADR 0036 L3. `p.x` compiles to GEP to the field
                // address followed by a load.
                //
                // The record's storage IS the struct alloca created
                // by L2 (`compile_value(Record{...})`). Declare and
                // Assign store that pointer directly in `variables`,
                // so a Variable receiver resolves by looking it up
                // — not by calling `compile_value`, which would load
                // a struct value through the alloca.
                let raw_object_type = object.type_of();
                // Auto-deref through a reference. Inside a method,
                // `self: &Pair<T>` gives `self` type `Borrow<Record>`,
                // so `self.first` reaches this arm with a borrowed
                // object. The GEP works the same way: the borrow's
                // value is already a pointer to the record's alloca.
                let object_type = match &raw_object_type {
                    Type::Borrow(inner) | Type::MutBorrow(inner) => (**inner).clone(),
                    other => other.clone(),
                };
                let record_name = match &object_type {
                    Type::Record(name, _) => name.clone(),
                    _ => {
                        return Err(CompileError::simple(
                            &format!(
                                "LLVM codegen: field access on non-record type {:?}",
                                raw_object_type
                            ),
                            0,
                            0,
                            "",
                            ErrorCode::E0002,
                        ));
                    }
                };

                let rec_decl = match self.record_decls.get(&record_name) {
                    Some(r) => r.clone(),
                    None => {
                        return Err(CompileError::simple(
                            &format!("LLVM codegen: unknown record '{}'", record_name),
                            0,
                            0,
                            "",
                            ErrorCode::E0003,
                        ));
                    }
                };

                let field_idx = match rec_decl.fields.iter().position(|(n, _, _)| n == field) {
                    Some(i) => i,
                    None => {
                        return Err(CompileError::simple(
                            &format!(
                                "LLVM codegen: record '{}' has no field '{}'",
                                record_name, field
                            ),
                            0,
                            0,
                            "",
                            ErrorCode::E0004,
                        ));
                    }
                };

                let struct_ty = match self.map_type(&object_type) {
                    BasicTypeEnum::StructType(s) => s,
                    _ => unreachable!(
                        "map_type(Type::Record) returned a non-struct type for '{}'",
                        record_name
                    ),
                };

                // Resolve the record's struct alloca pointer.
                let obj_ptr = match &**object {
                    TypedIRValue::Variable(var_name, _) => {
                        match self.variables.get(var_name).copied() {
                            Some(p) => p,
                            None => {
                                return Err(CompileError::simple(
                                    &format!("LLVM codegen: unknown variable '{}'", var_name),
                                    0,
                                    0,
                                    "",
                                    ErrorCode::E0003,
                                ));
                            }
                        }
                    }
                    _ => {
                        // Most record values are already pointers to
                        // their alloca: `RecordLiteral` returns one
                        // (see the arm above) and `Variable` for a
                        // record holds one directly. One path breaks
                        // the convention — `ArrayAccess` on a
                        // `List<Record>` loads the struct by value
                        // out of the element slot. Spill a by-value
                        // struct to a temporary alloca so the GEP
                        // below has a pointer to work with. The
                        // alloca is stack-allocated and LLVM's
                        // mem2reg promotes it, so there is no
                        // runtime cost. This is the surgical fix;
                        // unifying the representation of a record
                        // value across all producers is a separate
                        // refactor (see the List<T> work in A2 for
                        // the shape of that class of problem).
                        let val = self.compile_value(object)?;
                        if val.is_pointer_value() {
                            val.into_pointer_value()
                        } else if val.is_struct_value() {
                            let st = val.into_struct_value();
                            let tmp = self
                                .builder
                                .build_alloca(st.get_type(), "field_obj_tmp")
                                .unwrap();
                            self.builder.build_store(tmp, st).unwrap();
                            tmp
                        } else {
                            return Err(CompileError::simple(
                                &format!(
                                    "LLVM codegen: record object is not a \
                                     pointer or struct value (kind: {:?})",
                                    val
                                ),
                                0,
                                0,
                                "",
                                ErrorCode::E0002,
                            ));
                        }
                    }
                };

                let field_ptr = unsafe {
                    self.builder
                        .build_gep(
                            struct_ty,
                            obj_ptr,
                            &[
                                self.context.i32_type().const_zero(),
                                self.context.i32_type().const_int(field_idx as u64, false),
                            ],
                            &format!("{}_{}_ptr", record_name, field),
                        )
                        .unwrap()
                };

                let field_llvm_ty = self.map_type(field_type);
                self.builder
                    .build_load(
                        field_llvm_ty,
                        field_ptr,
                        &format!("{}_{}", record_name, field),
                    )
                    .unwrap()
            }
            TypedIRValue::Set { bits, .. } => {
                self.context.i64_type().const_int(*bits, false).into()
            }
            TypedIRValue::SetSingleton {
                element,
                element_type,
            } => {
                let elem_llvm = self.compile_value(element)?;
                let elem_i64 = if elem_llvm.is_int_value() {
                    let iv = elem_llvm.into_int_value();
                    if iv.get_type().get_bit_width() < 64 {
                        self.builder
                            .build_int_z_extend(iv, self.context.i64_type(), "set_singleton_ext")
                            .unwrap()
                    } else {
                        iv
                    }
                } else {
                    return Err(CompileError::simple(
                        "SetSingleton element must compile to an integer",
                        0,
                        0,
                        "",
                        ErrorCode::E0009,
                    ));
                };

                let low: i64 = match element_type {
                    Type::Subrange { low, .. } => *low,
                    _ => 0,
                };
                let bit = if low != 0 {
                    self.builder
                        .build_int_sub(
                            elem_i64,
                            self.context.i64_type().const_int(low as u64, true),
                            "set_singleton_adj",
                        )
                        .unwrap()
                } else {
                    elem_i64
                };
                let one = self.context.i64_type().const_int(1, false);
                self.builder
                    .build_left_shift(one, bit, "set_singleton_bit")
                    .unwrap()
                    .into()
            }
        })
    }
}
