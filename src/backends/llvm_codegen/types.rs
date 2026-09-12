// src/backends/llvm_codegen/types.rs

use super::IRCodeGen;
use crate::common::types::Type;
use inkwell::types::{BasicType, BasicTypeEnum};
use inkwell::values::BasicValueEnum;
use inkwell::AddressSpace;

impl<'ctx> IRCodeGen<'ctx> {

    pub(super) fn map_type(&self, ty: &Type) -> BasicTypeEnum<'ctx> {
        match ty {
            Type::Int => self.context.i64_type().into(),
            Type::Float => self.context.f64_type().into(),
            Type::Bool => self.context.bool_type().into(),
            Type::String => self.context.ptr_type(AddressSpace::default()).into(),
            Type::Void => self.context.ptr_type(AddressSpace::default()).into(), // FIX: Void as ptr for now
            Type::Ptr => self.context.ptr_type(AddressSpace::default()).into(),
            Type::Unknown => self.context.f64_type().into(), // Default for unknown
            Type::Never => self.context.ptr_type(AddressSpace::default()).into(),

            // For composite types, use pointer to first-class aggregate
            Type::List(inner) => {
                // Create a struct { ptr, length } for runtime bounds checking
                let elem_ty = self.map_type(inner);
                let len_ty = self.context.i64_type();
                self.context
                    .struct_type(&[elem_ty.into(), len_ty.into()], false)
                    .into()
            }
            Type::Array(inner, size) => {
                let elem_ty = self.map_type(inner);
                elem_ty.array_type(*size as u32).into()
            }
            Type::Option(inner) => {
                // Option = { bool is_some, T value }
                let inner_ty = self.map_type(inner);
                let bool_ty = self.context.bool_type();
                self.context
                    .struct_type(&[bool_ty.into(), inner_ty.into()], false)
                    .into()
            }
            Type::Result { ok, error } => {
                // Result = { bool is_ok, Ok value, Error value }
                let ok_ty = self.map_type(ok);
                let err_ty = self.map_type(error);
                let bool_ty = self.context.bool_type();
                self.context
                    .struct_type(&[bool_ty.into(), ok_ty.into(), err_ty.into()], false)
                    .into()
            }
            Type::Pointer(inner) => {
                let inner_ty = self.map_type(inner);
                self.context.ptr_type(AddressSpace::default()).into()
            }
            Type::Borrow(inner) | Type::MutBorrow(inner) => {
                let inner_ty = self.map_type(inner);
                self.context.ptr_type(AddressSpace::default()).into()
            }
            Type::Channel(inner) => {
                let inner_ty = self.map_type(inner);
                self.context.ptr_type(AddressSpace::default()).into()
            }
            Type::Function { .. } => self.context.ptr_type(AddressSpace::default()).into(),
            Type::TypeVar(_) | Type::Generic { .. } => self.context.f64_type().into(),
            Type::Tuple(_) => self.context.ptr_type(AddressSpace::default()).into(),
        }
    }

    pub(super) fn default_value_for_type(&self, ty: &Type) -> BasicValueEnum<'ctx> {
        match ty {
            Type::Int => self.context.i64_type().const_int(0, false).into(),
            Type::Bool => self.context.bool_type().const_int(0, false).into(),
            Type::String => self
                .context
                .ptr_type(AddressSpace::default())
                .const_null()
                .into(),
            Type::Float => self.context.f64_type().const_float(0.0).into(),
            _ => self.context.f64_type().const_float(0.0).into(),
        }
    }

}