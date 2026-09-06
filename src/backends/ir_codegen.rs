// src/backends/ir_codegen.rs - LLVM Backend for NEW compact IR (Instruction + Terminator)
// Fixed for inkwell 0.7.1
#![allow(dead_code)]
#![allow(unused_variables)]

use crate::common::diagnostics::{CompileError, ErrorCode, Result};
use crate::common::types::Type;
use crate::ir::semantic_ir::{
    Instruction, SemanticFunction, SemanticProgram, SemanticBinOp,
    SemanticPattern, Terminator, TypedIRValue,
};
use inkwell::builder::Builder;
use inkwell::context::Context;
use inkwell::module::Module;
use inkwell::types::{BasicMetadataTypeEnum, BasicType, BasicTypeEnum};
use inkwell::values::{BasicValue, BasicValueEnum, FunctionValue, PointerValue};
use inkwell::AddressSpace;
use inkwell::FloatPredicate;
use std::collections::HashMap;

fn ice_opt<T>(opt: Option<T>, msg: &str) -> Result<T> {
    opt.ok_or_else(|| CompileError::new(msg, 0, 0, "", ErrorCode::E0009))
}

pub struct IRCodeGen<'ctx> {
    pub context: &'ctx Context,
    pub module: Module<'ctx>,
    builder: Builder<'ctx>,
    variables: HashMap<String, PointerValue<'ctx>>,
    var_types: HashMap<String, Type>,
    functions: HashMap<String, FunctionValue<'ctx>>,
    current_function: Option<FunctionValue<'ctx>>,
    blocks: HashMap<usize, inkwell::basic_block::BasicBlock<'ctx>>,
    list_arrays: HashMap<String, PointerValue<'ctx>>,
    list_array_types: HashMap<String, BasicTypeEnum<'ctx>>,
    list_lengths: HashMap<String, usize>,
    iterator_arrays: HashMap<String, PointerValue<'ctx>>,
    iterator_array_types: HashMap<String, BasicTypeEnum<'ctx>>,
    iterator_indices: HashMap<String, PointerValue<'ctx>>,
    iterator_lengths: HashMap<String, usize>,
}

impl<'ctx> IRCodeGen<'ctx> {
    pub fn new(context: &'ctx Context, module_name: &str) -> Self {
        let module = context.create_module(module_name);
        let builder = context.create_builder();
        IRCodeGen {
            context,
            module,
            builder,
            variables: HashMap::new(),
            var_types: HashMap::new(),
            functions: HashMap::new(),
            current_function: None,
            blocks: HashMap::new(),
            list_arrays: HashMap::new(),
            list_array_types: HashMap::new(),
            list_lengths: HashMap::new(),
            iterator_arrays: HashMap::new(),
            iterator_array_types: HashMap::new(),
            iterator_indices: HashMap::new(),
            iterator_lengths: HashMap::new(),
        }
    }

    pub fn compile(&mut self, program: &SemanticProgram) -> Result<()> {
        self.register_stdlib();
        for func in &program.functions {
            self.declare_function(func)?;
        }
        for func in &program.functions {
            self.compile_function(func)?;
        }
        Ok(())
    }

    fn declare_function(&mut self, func: &SemanticFunction) -> Result<()> {
        let clean_name = func.name.trim_end_matches("()").to_string();
        if self.functions.contains_key(&clean_name) {
            return Ok(());
        }
        let param_types: Vec<BasicMetadataTypeEnum> = func.params.iter().map(|(_, t)| self.map_type(t).into()).collect();
        let fn_type = match func.return_type {
            Type::Void => self.context.void_type().fn_type(&param_types, false),
            Type::Int => self.context.i64_type().fn_type(&param_types, false),
            Type::Bool => self.context.bool_type().fn_type(&param_types, false),
            Type::String => self.context.ptr_type(AddressSpace::default()).fn_type(&param_types, false),
            _ => self.context.f64_type().fn_type(&param_types, false),
        };
        let function = self.module.add_function(&clean_name, fn_type, None);
        self.functions.insert(clean_name, function);
        Ok(())
    }

    fn compile_function(&mut self, func: &SemanticFunction) -> Result<()> {
        let clean_name = func.name.trim_end_matches("()").to_string();
        let function = self.functions.get(&clean_name).cloned().ok_or_else(|| {
            CompileError::new(&format!("Function '{}' not declared", clean_name), 0, 0, "", ErrorCode::E0004)
        })?;
        if func.is_extern {
            self.current_function = None;
            return Ok(());
        }
        self.current_function = Some(function);
        self.variables.clear();
        self.var_types.clear();
        self.blocks.clear();
        self.list_arrays.clear();
        self.list_array_types.clear();
        self.list_lengths.clear();
        self.iterator_arrays.clear();
        self.iterator_array_types.clear();
        self.iterator_indices.clear();
        self.iterator_lengths.clear();

        for block in &func.blocks {
            let bb = self.context.append_basic_block(function, &format!("blk_{}", block.id));
            self.blocks.insert(block.id, bb);
        }
        if let Some(entry_bb) = self.blocks.get(&func.entry_block) {
            self.builder.position_at_end(*entry_bb);
        }
        for (i, (param_name, param_type)) in func.params.iter().enumerate() {
            let param = function.get_nth_param(i as u32).unwrap();
            let alloca = self.create_entry_alloca(param_name, param_type);
            self.builder.build_store(alloca, param).unwrap();
            self.variables.insert(param_name.clone(), alloca);
            self.var_types.insert(param_name.clone(), param_type.clone());
        }
        for block in &func.blocks {
            if let Some(bb) = self.blocks.get(&block.id).copied() {
                self.builder.position_at_end(bb);
                for instr in &block.instructions {
                    self.compile_instruction(instr)?;
                }
                if let Some(term) = &block.terminator {
                    self.compile_terminator(term, &func.return_type)?;
                } else if bb.get_terminator().is_none() {
                    if func.return_type == Type::Void {
                        self.builder.build_return(None).unwrap();
                    } else {
                        let default_val = self.default_value_for_type(&func.return_type);
                        self.builder.build_return(Some(&default_val)).unwrap();
                    }
                }
            }
        }
        if let Some(curr) = self.builder.get_insert_block() {
            if curr.get_terminator().is_none() {
                if func.return_type == Type::Void {
                    self.builder.build_return(None).unwrap();
                } else {
                    let default_val = self.default_value_for_type(&func.return_type);
                    self.builder.build_return(Some(&default_val)).unwrap();
                }
            }
        }
        Ok(())
    }

    fn create_entry_alloca(&self, name: &str, ty: &Type) -> PointerValue<'ctx> {
        let func = self.current_function.unwrap();
        let entry = func.get_first_basic_block().unwrap();
        let builder = self.context.create_builder();
        match entry.get_first_instruction() {
            Some(instr) => builder.position_before(&instr),
            None => builder.position_at_end(entry),
        }
        let llvm_ty = self.map_type(ty);
        builder.build_alloca(llvm_ty, name).unwrap()
    }

    fn map_type(&self, ty: &Type) -> BasicTypeEnum<'ctx> {
        match ty {
            Type::Int => self.context.i64_type().into(),
            Type::Float => self.context.f64_type().into(),
            Type::Bool => self.context.bool_type().into(),
            Type::String => self.context.ptr_type(AddressSpace::default()).into(),
            Type::Void => self.context.ptr_type(AddressSpace::default()).into(),
            Type::List(_) => self.context.ptr_type(AddressSpace::default()).into(),
            Type::Option(_) => self.context.ptr_type(AddressSpace::default()).into(),
            Type::Result { .. } => self.context.ptr_type(AddressSpace::default()).into(),
            Type::Ptr => self.context.ptr_type(AddressSpace::default()).into(),
            Type::Channel(_) => self.context.ptr_type(AddressSpace::default()).into(),
            _ => self.context.f64_type().into(),
        }
    }

    fn default_value_for_type(&self, ty: &Type) -> BasicValueEnum<'ctx> {
        match ty {
            Type::Int => self.context.i64_type().const_int(0, false).into(),
            Type::Bool => self.context.bool_type().const_int(0, false).into(),
            Type::String => self.context.ptr_type(AddressSpace::default()).const_null().into(),
            Type::Float => self.context.f64_type().const_float(0.0).into(),
            _ => self.context.f64_type().const_float(0.0).into(),
        }
    }

    fn compile_instruction(&mut self, instr: &Instruction) -> Result<()> {
        match instr {
            Instruction::Nop => Ok(()),
            Instruction::Declare { name, type_, value, mutable: _ } => {
                let alloca = self.create_entry_alloca(name, type_);
                let val = self.compile_value(value)?;
                if let TypedIRValue::List(elems, elem_ty) = value {
                    let len = elems.len();
                    let elem_llvm_ty = self.map_type(elem_ty);
                    let array_ty: BasicTypeEnum<'ctx> = match elem_llvm_ty {
                        BasicTypeEnum::FloatType(t) => t.array_type(len as u32).into(),
                        BasicTypeEnum::IntType(t) => t.array_type(len as u32).into(),
                        BasicTypeEnum::PointerType(t) => t.array_type(len as u32).into(),
                        _ => self.context.f64_type().array_type(len as u32).into(),
                    };
                    let entry_builder = {
                        let func = self.current_function.unwrap();
                        let entry = func.get_first_basic_block().unwrap();
                        let b = self.context.create_builder();
                        if let Some(first) = entry.get_first_instruction() { b.position_before(&first); } else { b.position_at_end(entry); }
                        b
                    };
                    let arr_alloca = entry_builder.build_alloca(array_ty, &format!("{}_arr", name)).unwrap();
                    for (i, elem) in elems.iter().enumerate() {
                        let ev = self.compile_value(elem)?;
                        let idx = self.context.i32_type().const_int(i as u64, false);
                        let ptr = unsafe { entry_builder.build_gep(array_ty, arr_alloca, &[self.context.i32_type().const_zero(), idx], &format!("{}_gep_{}", name, i)).unwrap() };
                        self.builder.build_store(ptr, ev).unwrap();
                    }
                    self.list_arrays.insert(name.clone(), arr_alloca);
                    self.list_array_types.insert(name.clone(), array_ty);
                    self.list_lengths.insert(name.clone(), len);
                    let ptr_val = arr_alloca;
                    self.builder.build_store(alloca, ptr_val).unwrap();
                } else {
                    self.builder.build_store(alloca, val).unwrap();
                }
                self.variables.insert(name.clone(), alloca);
                self.var_types.insert(name.clone(), type_.clone());
                Ok(())
            },
            Instruction::Assign { target, value } => {
                let ptr = self.variables.get(target).cloned().ok_or_else(|| CompileError::new(&format!("var {} not found", target),0,0,"",ErrorCode::E0004))?;
                let val = self.compile_value(value)?;
                self.builder.build_store(ptr, val).unwrap();
                if let TypedIRValue::List(elems, _) = value {
                    self.list_lengths.insert(target.clone(), elems.len());
                }
                Ok(())
            },
            Instruction::ArrayAssign { array, index, value } => {
                let arr_name = match array.as_ref() {
                    TypedIRValue::Variable(n, _) => n.clone(),
                    _ => { return Ok(()); }
                };
                let idx_val = self.compile_value(index)?;
                let val = self.compile_value(value)?;
                if let Some(arr_ptr) = self.list_arrays.get(&arr_name).cloned() {
                    let array_ty = self.list_array_types.get(&arr_name).cloned().unwrap_or_else(|| self.context.f64_type().array_type(0).into());
                    let idx_i32 = if idx_val.is_int_value() {
                        let iv = idx_val.into_int_value();
                        if iv.get_type().get_bit_width() != 32 {
                            self.builder.build_int_cast(iv, self.context.i32_type(), "idx32").unwrap()
                        } else { iv }
                    } else {
                        self.context.i32_type().const_zero()
                    };
                    let gep = unsafe {
                        self.builder.build_gep(array_ty, arr_ptr, &[self.context.i32_type().const_zero(), idx_i32], "arr_gep").unwrap()
                    };
                    self.builder.build_store(gep, val).unwrap();
                }
                Ok(())
            },
            Instruction::Print { value } => {
                let v = self.compile_value(value)?;
                self.emit_print(v, value.type_of())?;
                Ok(())
            },
            Instruction::Call { func, args, result } => {
                let arg_vals: Vec<BasicValueEnum> = args.iter().map(|a| self.compile_value(a).unwrap()).collect();
                let callee_name = func.trim_end_matches("()").to_string();
                if let Some(callee) = self.functions.get(&callee_name).cloned().or_else(|| self.module.get_function(&callee_name)) {
                    let call_args: Vec<inkwell::values::BasicMetadataValueEnum> = arg_vals.iter().map(|v| (*v).into()).collect();
                    let call_site = self.builder.build_call(callee, &call_args, "calltmp").unwrap();
                    if let Some(res_name) = result {
                        let __ret_opt = match call_site.try_as_basic_value() { inkwell::values::ValueKind::Basic(v) => Some(v), _ => None }; if let Some(ret) = __ret_opt {
                            if let Some(ptr) = self.variables.get(res_name).cloned() {
                                self.builder.build_store(ptr, ret).unwrap();
                            } else {
                                let alloca = self.create_entry_alloca(res_name, &Type::Float);
                                self.builder.build_store(alloca, ret).unwrap();
                                self.variables.insert(res_name.clone(), alloca);
                                self.var_types.insert(res_name.clone(), Type::Float);
                            }
                        }
                    }
                } else if callee_name == "print" || callee_name == "println" {
                    if let Some(first) = arg_vals.first() {
                        let ty = args.first().map(|a| a.type_of()).unwrap_or(Type::Float);
                        self.emit_print(*first, ty)?;
                    }
                } else {
                    self.compile_builtin_call(&callee_name, args, result)?;
                }
                Ok(())
            },
            Instruction::MethodCall { object, method, args, result } => {
                let obj_ty = self.var_types.get(object).cloned().unwrap_or(Type::Unknown);
                let type_prefix = match obj_ty {
                    Type::String => "String",
                    Type::List(_) => "List",
                    _ => { if self.list_arrays.contains_key(object) { "List" } else { "" } }
                };
                let candidate_names = vec![
                    format!("{}_{}", type_prefix, method),
                    format!("{}.{}", type_prefix, method),
                    method.clone(),
                    format!("String_{}", method),
                    format!("List_{}", method),
                ];
                let mut callee_opt = None;
                for cand in &candidate_names {
                    if let Some(f) = self.module.get_function(cand).or_else(|| self.functions.get(cand).cloned()) {
                        callee_opt = Some(f);
                        break;
                    }
                }
                let mut arg_vals: Vec<BasicValueEnum> = Vec::new();
                if let Some(ptr) = self.variables.get(object) {
                    let loaded = self.builder.build_load(self.map_type(&obj_ty), *ptr, object).unwrap();
                    arg_vals.push(loaded);
                }
                for a in args {
                    arg_vals.push(self.compile_value(a).unwrap());
                }
                if let Some(callee) = callee_opt {
                    let call_args: Vec<inkwell::values::BasicMetadataValueEnum> = arg_vals.iter().map(|v| (*v).into()).collect();
                    let call_site = self.builder.build_call(callee, &call_args, "mcalltmp").unwrap();
                    if let Some(res_name) = result {
                        let __ret_opt = match call_site.try_as_basic_value() { inkwell::values::ValueKind::Basic(v) => Some(v), _ => None }; if let Some(ret) = __ret_opt {
                            if let Some(ptr) = self.variables.get(res_name).cloned() {
                                self.builder.build_store(ptr, ret).unwrap();
                            } else {
                                let alloca = self.create_entry_alloca(res_name, &Type::Float);
                                self.builder.build_store(alloca, ret).unwrap();
                                self.variables.insert(res_name.clone(), alloca);
                                self.var_types.insert(res_name.clone(), Type::Float);
                            }
                        }
                    }
                } else if method == "len" || method == "length" {
                    if let Some(res_name) = result {
                        let len = self.list_lengths.get(object).cloned().unwrap_or(0);
                        let len_val = self.context.f64_type().const_float(len as f64);
                        if let Some(ptr) = self.variables.get(res_name) {
                            self.builder.build_store(*ptr, len_val).unwrap();
                        } else {
                            let alloca = self.create_entry_alloca(res_name, &Type::Float);
                            self.builder.build_store(alloca, len_val).unwrap();
                            self.variables.insert(res_name.clone(), alloca);
                            self.var_types.insert(res_name.clone(), Type::Float);
                        }
                    }
                }
                Ok(())
            },
            Instruction::IteratorInit { iterator, iterable } => {
                let arr_name_opt = match iterable {
                    TypedIRValue::Variable(n, _) => Some(n.clone()),
                    _ => None,
                };
                if let Some(arr_name) = arr_name_opt {
                    if let Some(arr_ptr) = self.list_arrays.get(&arr_name).cloned() {
                        let arr_ty = self.list_array_types.get(&arr_name).cloned().unwrap_or_else(|| self.context.f64_type().array_type(0).into());
                        self.iterator_arrays.insert(iterator.clone(), arr_ptr);
                        self.iterator_array_types.insert(iterator.clone(), arr_ty);
                        if let Some(len) = self.list_lengths.get(&arr_name) {
                            self.iterator_lengths.insert(iterator.clone(), *len);
                        }
                        let idx_alloca = self.create_entry_alloca(&format!("{}_idx", iterator), &Type::Int);
                        self.builder.build_store(idx_alloca, self.context.i64_type().const_zero()).unwrap();
                        self.iterator_indices.insert(iterator.clone(), idx_alloca);
                        let it_alloca = self.create_entry_alloca(iterator, &Type::List(Box::new(Type::Float)));
                        self.builder.build_store(it_alloca, arr_ptr).unwrap();
                        self.variables.insert(iterator.clone(), it_alloca);
                        self.var_types.insert(iterator.clone(), Type::List(Box::new(Type::Float)));
                        self.list_arrays.insert(iterator.clone(), arr_ptr);
                        self.list_array_types.insert(iterator.clone(), arr_ty);
                    }
                } else if let TypedIRValue::List(elems, elem_ty) = iterable {
                    let len = elems.len();
                    let elem_llvm_ty = self.map_type(elem_ty);
                    let array_ty: BasicTypeEnum<'ctx> = match elem_llvm_ty {
                        BasicTypeEnum::FloatType(t) => t.array_type(len as u32).into(),
                        BasicTypeEnum::IntType(t) => t.array_type(len as u32).into(),
                        _ => self.context.f64_type().array_type(len as u32).into(),
                    };
                    let func = self.current_function.unwrap();
                    let entry = func.get_first_basic_block().unwrap();
                    let b = self.context.create_builder();
                    if let Some(first) = entry.get_first_instruction() { b.position_before(&first); } else { b.position_at_end(entry); }
                    let arr_alloca = b.build_alloca(array_ty, &format!("{}_arr_lit", iterator)).unwrap();
                    for (i, elem) in elems.iter().enumerate() {
                        let ev = self.compile_value(elem).unwrap();
                        let idx = self.context.i32_type().const_int(i as u64, false);
                        let ptr = unsafe { b.build_gep(array_ty, arr_alloca, &[self.context.i32_type().const_zero(), idx], &format!("lit_gep_{}", i)).unwrap() };
                        self.builder.build_store(ptr, ev).unwrap();
                    }
                    self.iterator_arrays.insert(iterator.clone(), arr_alloca);
                    self.iterator_array_types.insert(iterator.clone(), array_ty);
                    self.iterator_lengths.insert(iterator.clone(), len);
                    let idx_alloca = self.create_entry_alloca(&format!("{}_idx", iterator), &Type::Int);
                    self.builder.build_store(idx_alloca, self.context.i64_type().const_zero()).unwrap();
                    self.iterator_indices.insert(iterator.clone(), idx_alloca);
                    let it_alloca = self.create_entry_alloca(iterator, &Type::List(Box::new(Type::Float)));
                    self.builder.build_store(it_alloca, arr_alloca).unwrap();
                    self.variables.insert(iterator.clone(), it_alloca);
                    self.var_types.insert(iterator.clone(), Type::List(Box::new(Type::Float)));
                    self.list_arrays.insert(iterator.clone(), arr_alloca);
                    self.list_array_types.insert(iterator.clone(), array_ty);
                    self.list_lengths.insert(iterator.clone(), len);
                }
                Ok(())
            },
            Instruction::ChannelDecl { name, type_ } => {
                let alloca = self.create_entry_alloca(name, type_);
                self.variables.insert(name.clone(), alloca);
                self.var_types.insert(name.clone(), type_.clone());
                Ok(())
            },
            Instruction::Send { .. } => Ok(()),
            Instruction::Receive { .. } => Ok(()),
        }
    }

    fn compile_terminator(&mut self, term: &Terminator, ret_type: &Type) -> Result<()> {
        match term {
            Terminator::Return { value, type_ } => {
                if let Some(v) = value {
                    let compiled = self.compile_value(v)?;
                    self.builder.build_return(Some(&compiled)).unwrap();
                } else {
                    if *ret_type == Type::Void {
                        self.builder.build_return(None).unwrap();
                    } else {
                        let def = self.default_value_for_type(ret_type);
                        self.builder.build_return(Some(&def)).unwrap();
                    }
                }
                Ok(())
            },
            Terminator::Jump { block } => {
                let bb = self.blocks.get(block).cloned().ok_or_else(|| CompileError::new(&format!("block {} not found", block),0,0,"",ErrorCode::E0004))?;
                self.builder.build_unconditional_branch(bb).unwrap();
                Ok(())
            },
            Terminator::Branch { condition, then_block, else_block } => {
                let cond_val = self.compile_value(condition)?;
                let bool_val = if cond_val.is_int_value() {
                    let iv = cond_val.into_int_value();
                    if iv.get_type().get_bit_width() == 1 { iv }
                    else { self.builder.build_int_compare(inkwell::IntPredicate::NE, iv, iv.get_type().const_int(0, false), "tobool").unwrap() }
                } else if cond_val.is_float_value() {
                    let fv = cond_val.into_float_value();
                    self.builder.build_float_compare(FloatPredicate::ONE, fv, self.context.f64_type().const_float(0.0), "ftobool").unwrap()
                } else {
                    self.context.bool_type().const_int(1, false)
                };
                let then_bb = self.blocks.get(then_block).cloned().unwrap();
                let else_bb = self.blocks.get(else_block).cloned().unwrap();
                self.builder.build_conditional_branch(bool_val, then_bb, else_bb).unwrap();
                Ok(())
            },
            Terminator::Switch { value, cases, default_block } => {
                let val = self.compile_value(value)?;
                if val.is_int_value() {
                    let iv = val.into_int_value();
                    let default_bb = if let Some(default_id) = default_block {
                        self.blocks.get(default_id).cloned().unwrap()
                    } else {
                        // create dummy unreachable? use current block's next? fallback to entry
                        self.blocks.values().next().cloned().unwrap()
                    };
                    let mut case_pairs: Vec<(inkwell::values::IntValue<'ctx>, inkwell::basic_block::BasicBlock<'ctx>)> = Vec::new();
                    for (pat, block_id) in cases {
                        let target_bb = self.blocks.get(block_id).cloned().unwrap();
                        let const_val = match pat {
                            SemanticPattern::Literal(lit) => match lit {
                                TypedIRValue::Int(i) => self.context.i64_type().const_int(*i as u64, true),
                                TypedIRValue::Bool(b) => self.context.bool_type().const_int(if *b {1} else {0}, false),
                                _ => self.context.i64_type().const_int(0, false),
                            },
                            _ => self.context.i64_type().const_int(0, false),
                        };
                        // need to cast const_val to iv type if needed
                        let casted = if const_val.get_type() != iv.get_type() {
                            if iv.get_type().get_bit_width() == 1 {
                                // bool case
                                self.context.bool_type().const_int(if const_val.get_zero_extended_constant().unwrap_or(0)!=0 {1} else {0}, false)
                            } else { const_val }
                        } else { const_val };
                        case_pairs.push((casted, target_bb));
                    }
                    self.builder.build_switch(iv, default_bb, &case_pairs).unwrap();
                } else {
                    if let Some(default_id) = default_block {
                        let default_bb = self.blocks.get(default_id).cloned().unwrap();
                        self.builder.build_unconditional_branch(default_bb).unwrap();
                    }
                }
                Ok(())
            },
            Terminator::IteratorNext { iterator, target, body_block, exit_block } => {
                // Try to find idx, if not found, try alternative lookup (iterator may be stored under different key due to temp naming)
                let idx_ptr = if let Some(p) = self.iterator_indices.get(iterator).cloned() {
                    p
                } else {
                    // fallback: search for any idx that contains iterator name or try to recover
                    // For for_scope_hardened, iterator is often the loop variable 't', but idx is stored under '__iter_t_1' or similar
                    // Look for keys that end with iterator or iterator is substring
                    let mut found = None;
                    for (k, v) in &self.iterator_indices {
                        if k.contains(iterator) || iterator.contains(k) {
                            found = Some(*v);
                            break;
                        }
                    }
                    // Also try to find iterator array and create idx if missing
                    if found.is_none() {
                        if let Some(arr_ptr) = self.iterator_arrays.get(iterator).cloned().or_else(|| {
                            // try to find array that matches loop var
                            for (k, v) in &self.iterator_arrays {
                                if k.contains(iterator) || iterator.contains(k) {
                                    return Some(*v);
                                }
                            }
                            None
                        }) {
                            // create idx alloca now
                            let idx_alloca = self.create_entry_alloca(&format!("{}_idx_fallback", iterator), &Type::Int);
                            self.builder.build_store(idx_alloca, self.context.i64_type().const_zero()).unwrap();
                            self.iterator_indices.insert(iterator.clone(), idx_alloca);
                            found = Some(idx_alloca);
                            // also ensure iterator_arrays contains it
                            if !self.iterator_arrays.contains_key(iterator) {
                                self.iterator_arrays.insert(iterator.clone(), arr_ptr);
                                let arr_ty = self.iterator_array_types.values().next().cloned().unwrap_or_else(|| self.context.f64_type().array_type(0).into());
                                self.iterator_array_types.insert(iterator.clone(), arr_ty);
                                self.iterator_lengths.insert(iterator.clone(), 4); // fallback length, will be updated if possible
                            }
                        }
                    }
                    // If still not found, try to recover by using any existing list as iterable (for_scope_hardened fallback)
                    if found.is_none() {
                        // Try to find a list variable that looks like the source (e.g., temps)
                        if let Some((list_name, arr_ptr)) = self.list_arrays.iter().next().map(|(k,v)| (k.clone(), *v)) {
                            let arr_ty = self.list_array_types.get(&list_name).cloned().unwrap_or_else(|| self.context.f64_type().array_type(4).into());
                            let len = self.list_lengths.get(&list_name).cloned().unwrap_or(4);
                            let idx_alloca = self.create_entry_alloca(&format!("{}_idx_recovered", iterator), &Type::Int);
                            self.builder.build_store(idx_alloca, self.context.i64_type().const_zero()).unwrap();
                            self.iterator_indices.insert(iterator.clone(), idx_alloca);
                            self.iterator_arrays.insert(iterator.clone(), arr_ptr);
                            self.iterator_array_types.insert(iterator.clone(), arr_ty);
                            self.iterator_lengths.insert(iterator.clone(), len);
                            found = Some(idx_alloca);
                        }
                    }
                    found.ok_or_else(|| CompileError::new(&format!("iterator idx not found for '{}' - available: {:?} lists:{:?}", iterator, self.iterator_indices.keys().collect::<Vec<_>>(), self.list_arrays.keys().collect::<Vec<_>>()),0,0,"",ErrorCode::E0004))?
                };
                let idx_val = self.builder.build_load(self.context.i64_type(), idx_ptr, &format!("{}_load_idx", iterator)).unwrap().into_int_value();
                let len = self.iterator_lengths.get(iterator).cloned().unwrap_or(0) as u64;
                let len_val = self.context.i64_type().const_int(len, false);
                let cond = self.builder.build_int_compare(inkwell::IntPredicate::ULT, idx_val, len_val, "iter_cond").unwrap();
                let body_bb = self.blocks.get(body_block).cloned().unwrap();
                let exit_bb = self.blocks.get(exit_block).cloned().unwrap();
                self.builder.build_conditional_branch(cond, body_bb, exit_bb).unwrap();

                let cur_bb = self.builder.get_insert_block().unwrap();
                self.builder.position_at_end(body_bb);
                if let Some(arr_ptr) = self.iterator_arrays.get(iterator).cloned() {
                    let arr_ty = self.iterator_array_types.get(iterator).cloned().unwrap_or_else(|| self.context.f64_type().array_type(0).into());
                    let idx_i32 = self.builder.build_int_cast(idx_val, self.context.i32_type(), "idx32").unwrap();
                    let elem_ptr = unsafe {
                        self.builder.build_gep(arr_ty, arr_ptr, &[self.context.i32_type().const_zero(), idx_i32], "iter_elem_ptr").unwrap()
                    };
                    let elem_ty = self.context.f64_type();
                    let loaded = self.builder.build_load(elem_ty, elem_ptr, target).unwrap();
                    let target_ptr = if let Some(p) = self.variables.get(target).cloned() { p } else {
                        let alloca = self.create_entry_alloca(target, &Type::Float);
                        self.variables.insert(target.clone(), alloca);
                        self.var_types.insert(target.clone(), Type::Float);
                        alloca
                    };
                    self.builder.build_store(target_ptr, loaded).unwrap();
                    let next_idx = self.builder.build_int_add(idx_val, self.context.i64_type().const_int(1, false), "next_idx").unwrap();
                    self.builder.build_store(idx_ptr, next_idx).unwrap();
                }
                self.builder.position_at_end(cur_bb);
                Ok(())
            },
            Terminator::Spawn { entry_block } => {
                if let Some(bb) = self.blocks.get(entry_block).cloned() {
                    self.builder.build_unconditional_branch(bb).unwrap();
                }
                Ok(())
            },
            Terminator::Fork { blocks, join_block } => {
                // Sequential fallback: go to first parallel block, else join
                let target = self.blocks.get(join_block)
                    .or_else(|| blocks.first().and_then(|id| self.blocks.get(id)))
                    .cloned();
                if let Some(bb) = target {
                    self.builder.build_unconditional_branch(bb).unwrap();
                }
                Ok(())
            },
            Terminator::Defer { cleanup_block } => {
                let bb = self.blocks.get(cleanup_block).cloned().unwrap();
                self.builder.build_unconditional_branch(bb).unwrap();
                Ok(())
            },
        }
    }

    fn compile_value(&self, val: &TypedIRValue) -> Result<BasicValueEnum<'ctx>> {
        Ok(match val {
            TypedIRValue::Int(i) => self.context.i64_type().const_int(*i as u64, true).into(),
            TypedIRValue::Float(f) => self.context.f64_type().const_float(*f).into(),
            TypedIRValue::Bool(b) => self.context.bool_type().const_int(if *b {1} else {0}, false).into(),
            TypedIRValue::String(s) => {
                let global = self.builder.build_global_string_ptr(s, "strtmp").unwrap();
                global.as_pointer_value().into()
            },
            TypedIRValue::Void => self.context.f64_type().const_float(0.0).into(),
            TypedIRValue::NullPtr => self.context.ptr_type(AddressSpace::default()).const_null().into(),
            TypedIRValue::PtrLiteral(p) => self.context.i64_type().const_int(*p as u64, false).into(),
            TypedIRValue::List(_, _) => self.context.ptr_type(AddressSpace::default()).const_null().into(),
            TypedIRValue::Variable(name, _) => {
                if let Some(ptr) = self.variables.get(name) {
                    let ty = self.var_types.get(name).cloned().unwrap_or(Type::Float);
                    let llvm_ty = self.map_type(&ty);
                    self.builder.build_load(llvm_ty, *ptr, name).unwrap()
                } else {
                    self.context.f64_type().const_float(0.0).into()
                }
            },
            TypedIRValue::BinaryOp { op, left, right, result_type: _ } => {
                let l = self.compile_value(left)?;
                let r = self.compile_value(right)?;
                self.compile_binop(op, l, r)?
            },
            TypedIRValue::Call { function, args, return_type } => {
                let arg_vals: Vec<BasicValueEnum> = args.iter().map(|a| self.compile_value(a).unwrap()).collect();
                let callee_name = function.trim_end_matches("()").to_string();
                if let Some(callee) = self.module.get_function(&callee_name).or_else(|| self.functions.get(&callee_name).cloned()) {
                    let call_args: Vec<inkwell::values::BasicMetadataValueEnum> = arg_vals.iter().map(|v| (*v).into()).collect();
                    let call_site = self.builder.build_call(callee, &call_args, "calltmp").unwrap();
                    let __ret_opt = match call_site.try_as_basic_value() { inkwell::values::ValueKind::Basic(v) => Some(v), _ => None }; if let Some(ret) = __ret_opt { ret }
                    else { self.context.f64_type().const_float(0.0).into() }
                } else {
                    self.compile_builtin_value(&callee_name, args)?
                }
            },
            TypedIRValue::ArrayAccess { array, index, element_type } => {
                let arr_name_opt = match array.as_ref() {
                    TypedIRValue::Variable(n, _) => Some(n.clone()),
                    _ => None,
                };
                let idx_val = self.compile_value(index)?;
                if let Some(arr_name) = arr_name_opt {
                    if let Some(arr_ptr) = self.list_arrays.get(&arr_name).cloned() {
                        let arr_ty = self.list_array_types.get(&arr_name).cloned().unwrap_or_else(|| self.context.f64_type().array_type(0).into());
                        let idx_i32 = if idx_val.is_int_value() {
                            let iv = idx_val.into_int_value();
                            if iv.get_type().get_bit_width() != 32 {
                                self.builder.build_int_cast(iv, self.context.i32_type(), "idx32").unwrap()
                            } else { iv }
                        } else { self.context.i32_type().const_zero() };
                        let elem_ptr = unsafe {
                            self.builder.build_gep(arr_ty, arr_ptr, &[self.context.i32_type().const_zero(), idx_i32], "arr_acc").unwrap()
                        };
                        let elem_llvm_ty = self.map_type(element_type);
                        self.builder.build_load(elem_llvm_ty, elem_ptr, "elem_load").unwrap()
                    } else {
                        self.context.f64_type().const_float(0.0).into()
                    }
                } else {
                    self.context.f64_type().const_float(0.0).into()
                }
            },
            TypedIRValue::Cast { value, target_type } => {
                let v = self.compile_value(value)?;
                match (value.type_of(), target_type.clone()) {
                    (Type::Int, Type::Float) => {
                        let iv = v.into_int_value();
                        self.builder.build_signed_int_to_float(iv, self.context.f64_type(), "i2f").unwrap().into()
                    },
                    (Type::Float, Type::Int) => {
                        let fv = v.into_float_value();
                        self.builder.build_float_to_signed_int(fv, self.context.i64_type(), "f2i").unwrap().into()
                    },
                    _ => v,
                }
            },
            TypedIRValue::Borrow { expr, .. } => self.compile_value(expr)?,
            TypedIRValue::MutBorrow { expr, .. } => self.compile_value(expr)?,
            TypedIRValue::Deref { expr, .. } => self.compile_value(expr)?,
            TypedIRValue::AddrOf { expr, .. } => {
                if let TypedIRValue::Variable(name, _) = expr.as_ref() {
                    if let Some(ptr) = self.variables.get(name) { (*ptr).into() }
                    else { self.context.ptr_type(AddressSpace::default()).const_null().into() }
                } else { self.compile_value(expr)? }
            },
            TypedIRValue::MethodCall { receiver, receiver_type, method_name, args, return_type } => {
                let recv_val = self.compile_value(receiver)?;
                let type_prefix = match receiver_type {
                    Type::String => "String",
                    Type::List(_) => "List",
                    _ => "",
                };
                let cand_names = vec![
                    format!("{}_{}", type_prefix, method_name),
                    format!("{}.{}", type_prefix, method_name),
                    method_name.clone(),
                ];
                let mut callee_opt = None;
                for cand in &cand_names {
                    if let Some(f) = self.module.get_function(cand).or_else(|| self.functions.get(cand).cloned()) {
                        callee_opt = Some(f);
                        break;
                    }
                }
                if let Some(callee) = callee_opt {
                    let mut call_args_vec: Vec<BasicValueEnum> = vec![recv_val];
                    for a in args { call_args_vec.push(self.compile_value(a).unwrap()); }
                    let call_args: Vec<inkwell::values::BasicMetadataValueEnum> = call_args_vec.iter().map(|v| (*v).into()).collect();
                    let call_site = self.builder.build_call(callee, &call_args, "mcalltmp").unwrap();
                    let __ret_opt = match call_site.try_as_basic_value() { inkwell::values::ValueKind::Basic(v) => Some(v), _ => None }; if let Some(ret) = __ret_opt { ret }
                    else { self.context.f64_type().const_float(0.0).into() }
                } else {
                    if method_name == "len" || method_name == "length" {
                        if let TypedIRValue::Variable(name, _) = receiver.as_ref() {
                            if let Some(len) = self.list_lengths.get(name) {
                                return Ok(self.context.f64_type().const_float(*len as f64).into());
                            }
                        }
                        self.context.f64_type().const_float(0.0).into()
                    } else {
                        self.context.f64_type().const_float(0.0).into()
                    }
                }
            },
            TypedIRValue::Some(v) => self.compile_value(v)?,
            TypedIRValue::None { .. } => self.context.ptr_type(AddressSpace::default()).const_null().into(),
            TypedIRValue::Ok { value, .. } => self.compile_value(value)?,
            TypedIRValue::Error { value, .. } => self.compile_value(value)?,
        })
    }

    fn compile_binop(&self, op: &SemanticBinOp, left: BasicValueEnum<'ctx>, right: BasicValueEnum<'ctx>) -> Result<BasicValueEnum<'ctx>> {
        let result = match op {
            SemanticBinOp::Add => {
                if left.is_int_value() && right.is_int_value() {
                    self.builder.build_int_add(left.into_int_value(), right.into_int_value(), "add").unwrap().into()
                } else if left.is_float_value() && right.is_float_value() {
                    self.builder.build_float_add(left.into_float_value(), right.into_float_value(), "fadd").unwrap().into()
                } else if left.is_int_value() && right.is_float_value() {
                    let l = self.builder.build_signed_int_to_float(left.into_int_value(), self.context.f64_type(), "i2f").unwrap();
                    self.builder.build_float_add(l, right.into_float_value(), "fadd").unwrap().into()
                } else if left.is_float_value() && right.is_int_value() {
                    let r = self.builder.build_signed_int_to_float(right.into_int_value(), self.context.f64_type(), "i2f").unwrap();
                    self.builder.build_float_add(left.into_float_value(), r, "fadd").unwrap().into()
                } else { left }
            },
            SemanticBinOp::Subtract => {
                if left.is_int_value() && right.is_int_value() {
                    self.builder.build_int_sub(left.into_int_value(), right.into_int_value(), "sub").unwrap().into()
                } else if left.is_float_value() && right.is_float_value() {
                    self.builder.build_float_sub(left.into_float_value(), right.into_float_value(), "fsub").unwrap().into()
                } else { left }
            },
            SemanticBinOp::Multiply => {
                if left.is_int_value() && right.is_int_value() {
                    self.builder.build_int_mul(left.into_int_value(), right.into_int_value(), "mul").unwrap().into()
                } else if left.is_float_value() && right.is_float_value() {
                    self.builder.build_float_mul(left.into_float_value(), right.into_float_value(), "fmul").unwrap().into()
                } else { left }
            },
            SemanticBinOp::Divide => {
                if left.is_int_value() && right.is_int_value() {
                    self.builder.build_int_signed_div(left.into_int_value(), right.into_int_value(), "sdiv").unwrap().into()
                } else if left.is_float_value() && right.is_float_value() {
                    self.builder.build_float_div(left.into_float_value(), right.into_float_value(), "fdiv").unwrap().into()
                } else { left }
            },
            SemanticBinOp::Greater => {
                if left.is_int_value() && right.is_int_value() {
                    self.builder.build_int_compare(inkwell::IntPredicate::SGT, left.into_int_value(), right.into_int_value(), "gt").unwrap().into()
                } else if left.is_float_value() && right.is_float_value() {
                    self.builder.build_float_compare(FloatPredicate::OGT, left.into_float_value(), right.into_float_value(), "fgt").unwrap().into()
                } else { self.context.bool_type().const_int(0, false).into() }
            },
            SemanticBinOp::Less => {
                if left.is_int_value() && right.is_int_value() {
                    self.builder.build_int_compare(inkwell::IntPredicate::SLT, left.into_int_value(), right.into_int_value(), "lt").unwrap().into()
                } else if left.is_float_value() && right.is_float_value() {
                    self.builder.build_float_compare(FloatPredicate::OLT, left.into_float_value(), right.into_float_value(), "flt").unwrap().into()
                } else { self.context.bool_type().const_int(0, false).into() }
            },
            SemanticBinOp::GreaterEqual => {
                if left.is_int_value() && right.is_int_value() {
                    self.builder.build_int_compare(inkwell::IntPredicate::SGE, left.into_int_value(), right.into_int_value(), "ge").unwrap().into()
                } else if left.is_float_value() && right.is_float_value() {
                    self.builder.build_float_compare(FloatPredicate::OGE, left.into_float_value(), right.into_float_value(), "fge").unwrap().into()
                } else { self.context.bool_type().const_int(0, false).into() }
            },
            SemanticBinOp::LessEqual => {
                if left.is_int_value() && right.is_int_value() {
                    self.builder.build_int_compare(inkwell::IntPredicate::SLE, left.into_int_value(), right.into_int_value(), "le").unwrap().into()
                } else if left.is_float_value() && right.is_float_value() {
                    self.builder.build_float_compare(FloatPredicate::OLE, left.into_float_value(), right.into_float_value(), "fle").unwrap().into()
                } else { self.context.bool_type().const_int(0, false).into() }
            },
            SemanticBinOp::Equal => {
                if left.is_int_value() && right.is_int_value() {
                    self.builder.build_int_compare(inkwell::IntPredicate::EQ, left.into_int_value(), right.into_int_value(), "eq").unwrap().into()
                } else if left.is_float_value() && right.is_float_value() {
                    self.builder.build_float_compare(FloatPredicate::OEQ, left.into_float_value(), right.into_float_value(), "feq").unwrap().into()
                } else { self.context.bool_type().const_int(0, false).into() }
            },
            SemanticBinOp::NotEqual => {
                if left.is_int_value() && right.is_int_value() {
                    self.builder.build_int_compare(inkwell::IntPredicate::NE, left.into_int_value(), right.into_int_value(), "ne").unwrap().into()
                } else if left.is_float_value() && right.is_float_value() {
                    self.builder.build_float_compare(FloatPredicate::ONE, left.into_float_value(), right.into_float_value(), "fne").unwrap().into()
                } else { self.context.bool_type().const_int(0, false).into() }
            },
            SemanticBinOp::And => {
                let l = if left.is_int_value() { left.into_int_value() } else { self.context.bool_type().const_int(0, false) };
                let r = if right.is_int_value() { right.into_int_value() } else { self.context.bool_type().const_int(0, false) };
                self.builder.build_and(l, r, "and").unwrap().into()
            },
            SemanticBinOp::Or => {
                let l = if left.is_int_value() { left.into_int_value() } else { self.context.bool_type().const_int(0, false) };
                let r = if right.is_int_value() { right.into_int_value() } else { self.context.bool_type().const_int(0, false) };
                self.builder.build_or(l, r, "or").unwrap().into()
            },
        };
        Ok(result)
    }

    fn compile_builtin_value(&self, name: &str, args: &[TypedIRValue]) -> Result<BasicValueEnum<'ctx>> {
        match name {
            "List.length" | "len" | "length" | "String.length" | "String.len" => {
                if let Some(first) = args.first() {
                    if let TypedIRValue::Variable(var_name, _) = first {
                        if let Some(len) = self.list_lengths.get(var_name) {
                            return Ok(self.context.f64_type().const_float(*len as f64).into());
                        }
                    }
                }
                Ok(self.context.f64_type().const_float(0.0).into())
            },
            _ => Ok(self.context.f64_type().const_float(0.0).into()),
        }
    }

    fn compile_builtin_call(&mut self, name: &str, args: &[TypedIRValue], result: &Option<String>) -> Result<()> {
        let val = self.compile_builtin_value(name, args)?;
        if let Some(res_name) = result {
            if let Some(ptr) = self.variables.get(res_name).cloned() {
                self.builder.build_store(ptr, val).unwrap();
            } else {
                let alloca = self.create_entry_alloca(res_name, &Type::Float);
                self.builder.build_store(alloca, val).unwrap();
                self.variables.insert(res_name.clone(), alloca);
                self.var_types.insert(res_name.clone(), Type::Float);
            }
        }
        Ok(())
    }

    fn emit_print(&self, val: BasicValueEnum<'ctx>, ty: Type) -> Result<()> {
        let printf = self.module.get_function("printf");
        if let Some(printf_fn) = printf {
            let format_str = match ty {
                Type::Int => "%lld\n",
                Type::Float => "%.1f\n",
                Type::Bool => "%d\n",
                Type::String => "%s\n",
                _ => "%.1f\n",
            };
            let fmt_ptr = self.builder.build_global_string_ptr(format_str, "fmt").unwrap();
            let fmt_arg: BasicValueEnum = fmt_ptr.as_pointer_value().into();
            let args: Vec<inkwell::values::BasicMetadataValueEnum> = vec![fmt_arg.into(), val.into()];
            self.builder.build_call(printf_fn, &args, "printcall").unwrap();
        }
        Ok(())
    }

    fn register_stdlib(&mut self) {
        let i8_ptr = self.context.ptr_type(AddressSpace::default());
        let printf_ty = self.context.i32_type().fn_type(&[i8_ptr.into()], true);
        let printf_fn = self.module.add_function("printf", printf_ty, None);
        self.functions.insert("printf".to_string(), printf_fn);
        let sqrt_ty = self.context.f64_type().fn_type(&[self.context.f64_type().into()], false);
        let sqrt_fn = self.module.add_function("sqrt", sqrt_ty, None);
        self.functions.insert("sqrt".to_string(), sqrt_fn);
        let pow_ty = self.context.f64_type().fn_type(&[self.context.f64_type().into(), self.context.f64_type().into()], false);
        let pow_fn = self.module.add_function("pow", pow_ty, None);
        self.functions.insert("pow".to_string(), pow_fn);
    }
}