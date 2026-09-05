// src/ir_codegen.rs - LLVM Backend that consumes SemanticProgram (New IR)
// Replaces AST-based codegen.rs - FIXED for List and Iterator

#![allow(dead_code)]

use crate::common::diagnostics::{CompileError, ErrorCode, Result};
use crate::common::types::Type;
use crate::ir::semantic_ir::{
    SemanticBinOp, SemanticFunction, SemanticInstruction, SemanticProgram, TypedIRValue,
};
use inkwell::builder::Builder;
use inkwell::context::Context;
use inkwell::module::Module;
use inkwell::types::{BasicMetadataTypeEnum, BasicType, BasicTypeEnum};
use inkwell::values::{BasicValue, BasicValueEnum, FunctionValue, IntValue, PointerValue};
use inkwell::AddressSpace;
use inkwell::FloatPredicate;
use std::collections::HashMap;
fn ice_opt<T>(opt: Option<T>, msg: &str) -> Result<T> {
    opt.ok_or_else(|| CompileError::new(msg, 0, 0, "", ErrorCode::E0009))
}
fn ice_res<T, E: std::fmt::Debug>(res: std::result::Result<T, E>, ctx: &str) -> Result<T> {
    res.map_err(|e| CompileError::new(&format!("{}: {:?}", ctx, e), 0, 0, "", ErrorCode::E0009))
}
fn ice_entry<'ctx>(c: &IRCodeGen<'ctx>) -> Result<inkwell::basic_block::BasicBlock<'ctx>> {
    let bb = ice_opt(c.builder.get_insert_block(), "no insert block")?;
    let parent = ice_opt(bb.get_parent(), "no parent")?;
    ice_opt(parent.get_first_basic_block(), "no first bb")
}

pub struct IRCodeGen<'ctx> {
    pub context: &'ctx Context,
    pub module: Module<'ctx>,
    builder: Builder<'ctx>,
    variables: HashMap<String, PointerValue<'ctx>>,
    functions: HashMap<String, FunctionValue<'ctx>>,
    current_function: Option<FunctionValue<'ctx>>,
    blocks: HashMap<usize, inkwell::basic_block::BasicBlock<'ctx>>,
    loop_stack: Vec<(
        inkwell::basic_block::BasicBlock<'ctx>,
        inkwell::basic_block::BasicBlock<'ctx>,
    )>,
    // NEW: track list arrays and lengths
    list_arrays: HashMap<String, PointerValue<'ctx>>,
    list_lengths: HashMap<String, usize>,
    iterator_arrays: HashMap<String, PointerValue<'ctx>>,
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
            functions: HashMap::new(),
            current_function: None,
            blocks: HashMap::new(),
            loop_stack: Vec::new(),
            list_arrays: HashMap::new(),
            list_lengths: HashMap::new(),
            iterator_arrays: HashMap::new(),
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

        // Skip if already declared (e.g., stdlib functions like sqrt, pow, printf)
        if self.functions.contains_key(&clean_name) {
            return Ok(());
        }

        let _return_type = self.map_type(&func.return_type);
        let param_types: Vec<BasicMetadataTypeEnum> = func
            .params
            .iter()
            .map(|(_, t)| self.map_type(t).into())
            .collect();
        let fn_type = match func.return_type {
            Type::Void => self.context.void_type().fn_type(&param_types, false),
            Type::Int => self.context.i64_type().fn_type(&param_types, false),
            Type::Bool => self.context.bool_type().fn_type(&param_types, false),
            Type::String => self
                .context
                .ptr_type(AddressSpace::default())
                .fn_type(&param_types, false),
            _ => self.context.f64_type().fn_type(&param_types, false),
        };
        let function = self.module.add_function(&clean_name, fn_type, None);
        self.functions.insert(clean_name, function);
        Ok(())
    }

    fn compile_function(&mut self, func: &SemanticFunction) -> Result<()> {
        let clean_name = func.name.trim_end_matches("()").to_string();
        let function = self.functions.get(&clean_name).cloned().ok_or_else(|| {
            CompileError::new(
                &format!("Function '{}' not declared", clean_name),
                0,
                0,
                "",
                ErrorCode::E0004,
            )
        })?;
        if func.is_extern {
            self.current_function = None;
            return Ok(());
        }
        self.current_function = Some(function);
        self.variables.clear();
        self.blocks.clear();
        self.loop_stack.clear();
        self.list_arrays.clear();
        self.list_lengths.clear();
        self.iterator_arrays.clear();
        self.iterator_indices.clear();
        self.iterator_lengths.clear();

        for block in &func.blocks {
            let bb = self
                .context
                .append_basic_block(function, &format!("blk_{}", block.id));
            self.blocks.insert(block.id, bb);
        }
        if let Some(entry_bb) = self.blocks.get(&func.entry_block) {
            self.builder.position_at_end(*entry_bb);
        }
        for (i, (param_name, param_type)) in func.params.iter().enumerate() {
            let param = ice_opt(function.get_nth_param(i as u32), "missing")?;
            let alloca = self.create_entry_alloca(param_name, param_type)?;
            self.builder.build_store(alloca, param).map_err(|e| {
                CompileError::new(&format!("LLVM ICE: {:?}", e), 0, 0, "", ErrorCode::E0009)
            })?;
            self.variables.insert(param_name.clone(), alloca);
        }
        for block in &func.blocks {
            let bb_opt = self.blocks.get(&block.id).copied();
            if let Some(bb) = bb_opt {
                self.builder.position_at_end(bb);
                for instr in &block.instructions {
                    self.compile_instruction(instr, &func.return_type)?;
                }
                if bb.get_terminator().is_none() {
                    if func.return_type == Type::Void {
                        self.builder.build_return(None).map_err(|e| {
                            CompileError::new(
                                &format!("LLVM ICE: {:?}", e),
                                0,
                                0,
                                "",
                                ErrorCode::E0009,
                            )
                        })?;
                    } else {
                        let default_val = self.default_value_for_type(&func.return_type);
                        self.builder.build_return(Some(&default_val)).map_err(|e| {
                            CompileError::new(
                                &format!("LLVM ICE: {:?}", e),
                                0,
                                0,
                                "",
                                ErrorCode::E0009,
                            )
                        })?;
                    }
                }
            }
        }
        let current_bb = ice_opt(self.builder.get_insert_block(), "missing")?;
        if current_bb.get_terminator().is_none() {
            if func.return_type == Type::Void {
                self.builder.build_return(None).map_err(|e| {
                    CompileError::new(&format!("LLVM ICE: {:?}", e), 0, 0, "", ErrorCode::E0009)
                })?;
            } else {
                let default_val = self.default_value_for_type(&func.return_type);
                self.builder.build_return(Some(&default_val)).map_err(|e| {
                    CompileError::new(&format!("LLVM ICE: {:?}", e), 0, 0, "", ErrorCode::E0009)
                })?;
            }
        }
        if let Some(entry_bb) = self.blocks.get(&func.entry_block) {
            self.builder.position_at_end(*entry_bb);
            let has_terminator = entry_bb.get_terminator().is_some();
            if !has_terminator {
                if func.return_type == Type::Void {
                    self.builder.build_return(None).map_err(|e| {
                        CompileError::new(&format!("LLVM ICE: {:?}", e), 0, 0, "", ErrorCode::E0009)
                    })?;
                } else {
                    let default_val = self.default_value_for_type(&func.return_type);
                    self.builder.build_return(Some(&default_val)).map_err(|e| {
                        CompileError::new(&format!("LLVM ICE: {:?}", e), 0, 0, "", ErrorCode::E0009)
                    })?;
                }
            }
        }
        self.current_function = None;
        Ok(())
    }

    fn compile_instruction(
        &mut self,
        instr: &SemanticInstruction,
        _return_type: &Type,
    ) -> Result<()> {
        match instr {
            SemanticInstruction::Nop => {}

            SemanticInstruction::Declare {
                name,
                mutable: _,
                type_,
                value,
            } => {
                // Handle List specially
                if let TypedIRValue::List(elements, _) = value {
                    let len = elements.len();
                    let f64_type = self.context.f64_type();
                    let array_type = f64_type.array_type(len as u32);
                    let builder = self.context.create_builder();
                    let entry = self
                        .builder
                        .get_insert_block()
                        .ok_or_else(|| {
                            CompileError::new(
                                "LLVM ICE: no insert block",
                                0,
                                0,
                                "",
                                ErrorCode::E0009,
                            )
                        })?
                        .get_parent()
                        .ok_or_else(|| {
                            CompileError::new("LLVM ICE: no parent", 0, 0, "", ErrorCode::E0009)
                        })?
                        .get_first_basic_block()
                        .ok_or_else(|| {
                            CompileError::new(
                                "LLVM ICE: get_first_basic_block None",
                                0,
                                0,
                                "",
                                ErrorCode::E0009,
                            )
                        })?;
                    match entry.get_first_instruction() {
                        Some(first) => builder.position_before(&first),
                        None => builder.position_at_end(entry),
                    }
                    let alloca = builder.build_alloca(array_type, name).map_err(|e| {
                        CompileError::new(&format!("LLVM ICE: {:?}", e), 0, 0, "", ErrorCode::E0009)
                    })?;
                    // Store each element
                    for (i, elem) in elements.iter().enumerate() {
                        let val = self.compile_value(elem)?;
                        let idx = self.context.i64_type().const_int(i as u64, false);
                        let ptr = unsafe {
                            builder
                                .build_gep(
                                    array_type,
                                    alloca,
                                    &[self.context.i64_type().const_int(0, false), idx],
                                    &format!("{}_ptr_{}", name, i),
                                )
                                .map_err(|e| {
                                    CompileError::new(
                                        &format!("LLVM ICE: {:?}", e),
                                        0,
                                        0,
                                        "",
                                        ErrorCode::E0009,
                                    )
                                })?
                        };
                        // Use main builder to store (or entry builder)
                        self.builder.build_store(ptr, val).map_err(|e| {
                            CompileError::new(
                                &format!("LLVM ICE: {:?}", e),
                                0,
                                0,
                                "",
                                ErrorCode::E0009,
                            )
                        })?;
                    }
                    self.variables.insert(name.clone(), alloca);
                    self.list_arrays.insert(name.clone(), alloca);
                    self.list_lengths.insert(name.clone(), len);
                    return Ok(());
                }
                let val = self.compile_value(value)?;
                let alloca = self.create_entry_alloca(name, type_)?;
                self.builder.build_store(alloca, val).map_err(|e| {
                    CompileError::new(&format!("LLVM ICE: {:?}", e), 0, 0, "", ErrorCode::E0009)
                })?;
                self.variables.insert(name.clone(), alloca);
            }

            SemanticInstruction::Assign { target, value } => {
                let val = self.compile_value(value)?;
                if let Some(ptr) = self.variables.get(target) {
                    self.builder.build_store(*ptr, val).map_err(|e| {
                        CompileError::new(&format!("LLVM ICE: {:?}", e), 0, 0, "", ErrorCode::E0009)
                    })?;
                }
            }

            SemanticInstruction::ArrayAssign {
                array,
                index,
                value,
            } => {
                let _ = self.compile_value(array)?;
                let _ = self.compile_value(index)?;
                let _ = self.compile_value(value)?;
            }

            SemanticInstruction::Print { value } => {
                // Check if printing a list literal directly
                if let TypedIRValue::List(elements, _) = value {
                    self.emit_print_list_literal(elements)?;
                } else if let TypedIRValue::Variable(var_name, _) = value {
                    if self.list_arrays.contains_key(var_name) {
                        self.emit_print_list_var(var_name)?;
                    } else {
                        let val = self.compile_value(value)?;
                        self.emit_print(&val)?;
                    }
                } else {
                    let val = self.compile_value(value)?;
                    self.emit_print(&val)?;
                }
            }

            SemanticInstruction::Return { value, type_ } => match value {
                Some(v) => {
                    let val = self.compile_value(v)?;
                    let coerced = self.coerce_to_type(val, type_)?;
                    self.builder.build_return(Some(&coerced)).map_err(|e| {
                        CompileError::new(&format!("LLVM ICE: {:?}", e), 0, 0, "", ErrorCode::E0009)
                    })?;
                }
                None => {
                    self.builder.build_return(None).map_err(|e| {
                        CompileError::new(&format!("LLVM ICE: {:?}", e), 0, 0, "", ErrorCode::E0009)
                    })?;
                }
            },

            SemanticInstruction::Branch {
                condition,
                then_block,
                else_block,
            } => {
                let cond = self.compile_value(condition)?;
                let cond_bool = self.to_bool(cond)?;
                let then_bb = *self.blocks.get(then_block).ok_or_else(|| {
                    CompileError::new(
                        "LLVM ICE: missing entry block 0",
                        0,
                        0,
                        "",
                        ErrorCode::E0009,
                    )
                })?;
                let else_bb = *self.blocks.get(else_block).ok_or_else(|| {
                    CompileError::new(
                        "LLVM ICE: missing entry block 0",
                        0,
                        0,
                        "",
                        ErrorCode::E0009,
                    )
                })?;
                self.builder
                    .build_conditional_branch(cond_bool, then_bb, else_bb)
                    .map_err(|e| {
                        CompileError::new(&format!("LLVM ICE: {:?}", e), 0, 0, "", ErrorCode::E0009)
                    })?;
            }

            SemanticInstruction::Jump { block } => {
                if let Some(target) = self.blocks.get(block) {
                    self.builder
                        .build_unconditional_branch(*target)
                        .map_err(|e| {
                            CompileError::new(
                                &format!("LLVM ICE: {:?}", e),
                                0,
                                0,
                                "",
                                ErrorCode::E0009,
                            )
                        })?;
                }
            }

            SemanticInstruction::Switch {
                value,
                cases,
                default_block,
            } => {
                let _ = self.compile_value(value)?;
                if let Some((_, first_target)) = cases.first() {
                    let target = ice_opt(self.blocks.get(first_target).copied(), "missing")?;
                    self.builder
                        .build_unconditional_branch(target)
                        .map_err(|e| {
                            CompileError::new(
                                &format!("LLVM ICE: {:?}", e),
                                0,
                                0,
                                "",
                                ErrorCode::E0009,
                            )
                        })?;
                } else if let Some(default) = default_block {
                    let target = ice_opt(self.blocks.get(default).copied(), "missing")?;
                    self.builder
                        .build_unconditional_branch(target)
                        .map_err(|e| {
                            CompileError::new(
                                &format!("LLVM ICE: {:?}", e),
                                0,
                                0,
                                "",
                                ErrorCode::E0009,
                            )
                        })?;
                }
            }

            SemanticInstruction::Call {
                result,
                function,
                args,
                return_type,
            } => {
                if function.starts_with("File.") {
                    let compiled_args: Vec<BasicValueEnum> = args
                        .iter()
                        .map(|a| self.compile_value(a))
                        .collect::<Result<Vec<_>>>()?;
                    let val = self.call_file_function(function, &compiled_args)?;
                    if let Some(result_name) = result {
                        let alloca = self.create_entry_alloca(result_name, return_type)?;
                        self.builder.build_store(alloca, val).map_err(|e| {
                            CompileError::new(
                                &format!("LLVM ICE: {:?}", e),
                                0,
                                0,
                                "",
                                ErrorCode::E0009,
                            )
                        })?;
                        self.variables.insert(result_name.clone(), alloca);
                    }
                    return Ok(());
                }
                if function.starts_with("List.") {
                    // Check for direct List or Cast-wrapped List
                    let list_elements = match args.first() {
                        Some(TypedIRValue::List(elements, _)) => Some(elements),
                        Some(TypedIRValue::Cast { value, .. }) => {
                            if let TypedIRValue::List(elements, _) = value.as_ref() {
                                Some(elements)
                            } else {
                                None
                            }
                        }
                        _ => None,
                    };

                    if let Some(elements) = list_elements {
                        let val = self.evaluate_list_operation(function, elements)?;
                        if let Some(result_name) = result {
                            let alloca = self.create_entry_alloca(result_name, return_type)?;
                            self.builder.build_store(alloca, val).map_err(|e| {
                                CompileError::new(
                                    &format!("LLVM ICE: {:?}", e),
                                    0,
                                    0,
                                    "",
                                    ErrorCode::E0009,
                                )
                            })?;
                            self.variables.insert(result_name.clone(), alloca);
                        }
                    }
                    return Ok(());
                }
                if function.starts_with("String.") {
                    let compiled_args: Vec<BasicValueEnum> = args
                        .iter()
                        .map(|a| self.compile_value(a))
                        .collect::<Result<Vec<_>>>()?;
                    let val = self.call_string_function(function, &compiled_args)?;
                    if let Some(result_name) = result {
                        let alloca = self.create_entry_alloca(result_name, return_type)?;
                        self.builder.build_store(alloca, val).map_err(|e| {
                            CompileError::new(
                                &format!("LLVM ICE: {:?}", e),
                                0,
                                0,
                                "",
                                ErrorCode::E0009,
                            )
                        })?;
                        self.variables.insert(result_name.clone(), alloca);
                    }
                    return Ok(());
                }
                let func = self.functions.get(function).cloned().ok_or_else(|| {
                    CompileError::new(
                        &format!("Undefined function '{}'", function),
                        0,
                        0,
                        "",
                        ErrorCode::E0004,
                    )
                })?;
                let arg_vals: Vec<BasicValueEnum> = args
                    .iter()
                    .map(|a| self.compile_value(a))
                    .collect::<Result<Vec<_>>>()?;
                let arg_metadata: Vec<inkwell::values::BasicMetadataValueEnum> =
                    arg_vals.iter().map(|v| (*v).into()).collect();
                let call = self
                    .builder
                    .build_direct_call(func, &arg_metadata, "calltmp")
                    .map_err(|e| {
                        CompileError::new(&format!("LLVM ICE: {:?}", e), 0, 0, "", ErrorCode::E0009)
                    })?;
                if let Some(result_name) = result {
                    if let Some(val) = call.try_as_basic_value().basic() {
                        let alloca = self.create_entry_alloca(result_name, return_type)?;
                        self.builder.build_store(alloca, val).map_err(|e| {
                            CompileError::new(
                                &format!("LLVM ICE: {:?}", e),
                                0,
                                0,
                                "",
                                ErrorCode::E0009,
                            )
                        })?;
                        self.variables.insert(result_name.clone(), alloca);
                    }
                }
            }

            SemanticInstruction::IteratorInit { iterator, iterable } => {
                // iterable can be List literal or Variable that is List
                let (array_ptr, len) = match iterable {
                    TypedIRValue::List(elements, _) => {
                        // Create array for this list literal directly in iterator
                        let len = elements.len();
                        let f64_type = self.context.f64_type();
                        let array_type = f64_type.array_type(len as u32);
                        let builder = self.context.create_builder();
                        let entry = self
                            .builder
                            .get_insert_block()
                            .ok_or_else(|| {
                                CompileError::new(
                                    "LLVM ICE: no insert block",
                                    0,
                                    0,
                                    "",
                                    ErrorCode::E0009,
                                )
                            })?
                            .get_parent()
                            .ok_or_else(|| {
                                CompileError::new("LLVM ICE: no parent", 0, 0, "", ErrorCode::E0009)
                            })?
                            .get_first_basic_block()
                            .ok_or_else(|| {
                                CompileError::new(
                                    "LLVM ICE: get_first_basic_block None",
                                    0,
                                    0,
                                    "",
                                    ErrorCode::E0009,
                                )
                            })?;
                        match entry.get_first_instruction() {
                            Some(first) => builder.position_before(&first),
                            None => builder.position_at_end(entry),
                        }
                        let alloca = builder
                            .build_alloca(array_type, &format!("{}_arr", iterator))
                            .map_err(|e| {
                                CompileError::new(
                                    &format!("LLVM ICE: {:?}", e),
                                    0,
                                    0,
                                    "",
                                    ErrorCode::E0009,
                                )
                            })?;
                        for (i, elem) in elements.iter().enumerate() {
                            let val = self.compile_value(elem)?;
                            let idx = self.context.i64_type().const_int(i as u64, false);
                            let ptr = unsafe {
                                self.builder
                                    .build_gep(
                                        array_type,
                                        alloca,
                                        &[self.context.i64_type().const_int(0, false), idx],
                                        &format!("iter_ptr_{}", i),
                                    )
                                    .map_err(|e| {
                                        CompileError::new(
                                            &format!("LLVM ICE: {:?}", e),
                                            0,
                                            0,
                                            "",
                                            ErrorCode::E0009,
                                        )
                                    })?
                            };
                            self.builder.build_store(ptr, val).map_err(|e| {
                                CompileError::new(
                                    &format!("LLVM ICE: {:?}", e),
                                    0,
                                    0,
                                    "",
                                    ErrorCode::E0009,
                                )
                            })?;
                        }
                        (alloca, len)
                    }
                    TypedIRValue::Variable(var_name, _) => {
                        // Look up list array for var
                        if let Some(arr_ptr) = self.list_arrays.get(var_name) {
                            let len = self.list_lengths.get(var_name).copied().unwrap_or(0);
                            (*arr_ptr, len)
                        } else {
                            // Fallback: try to compile variable as list (should not happen)
                            // Create empty array
                            let f64_type = self.context.f64_type();
                            let array_type = f64_type.array_type(0);
                            let builder = self.context.create_builder();
                            let entry = self
                                .builder
                                .get_insert_block()
                                .ok_or_else(|| {
                                    CompileError::new(
                                        "LLVM ICE: no insert block",
                                        0,
                                        0,
                                        "",
                                        ErrorCode::E0009,
                                    )
                                })?
                                .get_parent()
                                .ok_or_else(|| {
                                    CompileError::new(
                                        "LLVM ICE: no parent",
                                        0,
                                        0,
                                        "",
                                        ErrorCode::E0009,
                                    )
                                })?
                                .get_first_basic_block()
                                .ok_or_else(|| {
                                    CompileError::new(
                                        "LLVM ICE: get_first_basic_block None",
                                        0,
                                        0,
                                        "",
                                        ErrorCode::E0009,
                                    )
                                })?;
                            match entry.get_first_instruction() {
                                Some(first) => builder.position_before(&first),
                                None => builder.position_at_end(entry),
                            }
                            let alloca = builder
                                .build_alloca(array_type, &format!("{}_empty", iterator))
                                .map_err(|e| {
                                    CompileError::new(
                                        &format!("LLVM ICE: {:?}", e),
                                        0,
                                        0,
                                        "",
                                        ErrorCode::E0009,
                                    )
                                })?;
                            (alloca, 0)
                        }
                    }
                    _ => {
                        let f64_type = self.context.f64_type();
                        let array_type = f64_type.array_type(0);
                        let builder = self.context.create_builder();
                        let entry = self
                            .builder
                            .get_insert_block()
                            .ok_or_else(|| {
                                CompileError::new(
                                    "LLVM ICE: no insert block",
                                    0,
                                    0,
                                    "",
                                    ErrorCode::E0009,
                                )
                            })?
                            .get_parent()
                            .ok_or_else(|| {
                                CompileError::new("LLVM ICE: no parent", 0, 0, "", ErrorCode::E0009)
                            })?
                            .get_first_basic_block()
                            .ok_or_else(|| {
                                CompileError::new(
                                    "LLVM ICE: get_first_basic_block None",
                                    0,
                                    0,
                                    "",
                                    ErrorCode::E0009,
                                )
                            })?;
                        match entry.get_first_instruction() {
                            Some(first) => builder.position_before(&first),
                            None => builder.position_at_end(entry),
                        }
                        let alloca = builder
                            .build_alloca(array_type, &format!("{}_empty", iterator))
                            .map_err(|e| {
                                CompileError::new(
                                    &format!("LLVM ICE: {:?}", e),
                                    0,
                                    0,
                                    "",
                                    ErrorCode::E0009,
                                )
                            })?;
                        (alloca, 0)
                    }
                };

                // Create index alloca
                let i64_type = self.context.i64_type();
                let builder = self.context.create_builder();
                let entry = self
                    .builder
                    .get_insert_block()
                    .ok_or_else(|| {
                        CompileError::new("LLVM ICE: no insert block", 0, 0, "", ErrorCode::E0009)
                    })?
                    .get_parent()
                    .ok_or_else(|| {
                        CompileError::new("LLVM ICE: no parent", 0, 0, "", ErrorCode::E0009)
                    })?
                    .get_first_basic_block()
                    .ok_or_else(|| {
                        CompileError::new(
                            "LLVM ICE: get_first_basic_block None",
                            0,
                            0,
                            "",
                            ErrorCode::E0009,
                        )
                    })?;
                match entry.get_first_instruction() {
                    Some(first) => builder.position_before(&first),
                    None => builder.position_at_end(entry),
                }
                let idx_alloca = builder
                    .build_alloca(i64_type, &format!("{}_idx", iterator))
                    .map_err(|e| {
                        CompileError::new(&format!("LLVM ICE: {:?}", e), 0, 0, "", ErrorCode::E0009)
                    })?;
                self.builder
                    .build_store(idx_alloca, i64_type.const_int(0, false))
                    .map_err(|e| {
                        CompileError::new(&format!("LLVM ICE: {:?}", e), 0, 0, "", ErrorCode::E0009)
                    })?;

                self.iterator_arrays.insert(iterator.clone(), array_ptr);
                self.iterator_indices.insert(iterator.clone(), idx_alloca);
                self.iterator_lengths.insert(iterator.clone(), len);
            }

            SemanticInstruction::IteratorNext {
                iterator,
                target,
                body_block,
                exit_block,
            } => {
                let idx_ptr = ice_opt(self.iterator_indices.get(iterator).copied(), "missing")?;
                let arr_ptr = ice_opt(self.iterator_arrays.get(iterator).copied(), "missing")?;
                let len = self.iterator_lengths.get(iterator).copied().unwrap_or(0);

                let i64_type = self.context.i64_type();
                let f64_type = self.context.f64_type();

                // Load index
                let idx_val = self
                    .builder
                    .build_load(i64_type, idx_ptr, "iter_idx")
                    .map_err(|e| {
                        CompileError::new(&format!("LLVM ICE: {:?}", e), 0, 0, "", ErrorCode::E0009)
                    })?
                    .into_int_value();
                let len_val = i64_type.const_int(len as u64, false);
                let cond = self
                    .builder
                    .build_int_compare(inkwell::IntPredicate::SLT, idx_val, len_val, "iter_cond")
                    .map_err(|e| {
                        CompileError::new(&format!("LLVM ICE: {:?}", e), 0, 0, "", ErrorCode::E0009)
                    })?;

                let body_bb = ice_opt(self.blocks.get(body_block).copied(), "missing")?;
                let exit_bb = ice_opt(self.blocks.get(exit_block).copied(), "missing")?;

                // Create blocks for then/else of iterator
                let current_fn = ice_opt(self.current_function, "missing")?;
                let load_bb = self.context.append_basic_block(current_fn, "iter_load");

                self.builder
                    .build_conditional_branch(cond, load_bb, exit_bb)
                    .map_err(|e| {
                        CompileError::new(&format!("LLVM ICE: {:?}", e), 0, 0, "", ErrorCode::E0009)
                    })?;

                // Load block: load element and store to target, increment index, jump to body
                self.builder.position_at_end(load_bb);
                // For simplicity assume array type is [len x double]
                let array_type = f64_type.array_type(len as u32);
                let elem_ptr = unsafe {
                    self.builder
                        .build_gep(
                            array_type,
                            arr_ptr,
                            &[i64_type.const_int(0, false), idx_val],
                            "elem_ptr",
                        )
                        .map_err(|e| {
                            CompileError::new(
                                &format!("LLVM ICE: {:?}", e),
                                0,
                                0,
                                "",
                                ErrorCode::E0009,
                            )
                        })?
                };
                let elem_val = self
                    .builder
                    .build_load(f64_type, elem_ptr, "elem")
                    .map_err(|e| {
                        CompileError::new(&format!("LLVM ICE: {:?}", e), 0, 0, "", ErrorCode::E0009)
                    })?;

                // Store to target variable
                if let Some(target_ptr) = self.variables.get(target) {
                    self.builder
                        .build_store(*target_ptr, elem_val)
                        .map_err(|e| {
                            CompileError::new(
                                &format!("LLVM ICE: {:?}", e),
                                0,
                                0,
                                "",
                                ErrorCode::E0009,
                            )
                        })?;
                } else {
                    // Create alloca for target if not exists
                    let alloca = self.create_entry_alloca(target, &Type::Float)?;
                    self.builder.build_store(alloca, elem_val).map_err(|e| {
                        CompileError::new(&format!("LLVM ICE: {:?}", e), 0, 0, "", ErrorCode::E0009)
                    })?;
                    self.variables.insert(target.clone(), alloca);
                }

                // Increment index
                let one = i64_type.const_int(1, false);
                let next_idx = self
                    .builder
                    .build_int_add(idx_val, one, "next_idx")
                    .map_err(|e| {
                        CompileError::new(&format!("LLVM ICE: {:?}", e), 0, 0, "", ErrorCode::E0009)
                    })?;
                self.builder.build_store(idx_ptr, next_idx).map_err(|e| {
                    CompileError::new(&format!("LLVM ICE: {:?}", e), 0, 0, "", ErrorCode::E0009)
                })?;

                self.builder
                    .build_unconditional_branch(body_bb)
                    .map_err(|e| {
                        CompileError::new(&format!("LLVM ICE: {:?}", e), 0, 0, "", ErrorCode::E0009)
                    })?;
            }

            SemanticInstruction::Spawn { entry_block } => {
                if let Some(target) = self.blocks.get(entry_block) {
                    self.builder
                        .build_unconditional_branch(*target)
                        .map_err(|e| {
                            CompileError::new(
                                &format!("LLVM ICE: {:?}", e),
                                0,
                                0,
                                "",
                                ErrorCode::E0009,
                            )
                        })?;
                }
            }

            SemanticInstruction::Fork { blocks, join_block } => {
                if let Some(first) = blocks.first() {
                    if let Some(target) = self.blocks.get(first) {
                        self.builder
                            .build_unconditional_branch(*target)
                            .map_err(|e| {
                                CompileError::new(
                                    &format!("LLVM ICE: {:?}", e),
                                    0,
                                    0,
                                    "",
                                    ErrorCode::E0009,
                                )
                            })?;
                    }
                } else if let Some(join) = self.blocks.get(join_block) {
                    self.builder
                        .build_unconditional_branch(*join)
                        .map_err(|e| {
                            CompileError::new(
                                &format!("LLVM ICE: {:?}", e),
                                0,
                                0,
                                "",
                                ErrorCode::E0009,
                            )
                        })?;
                }
            }

            SemanticInstruction::Defer { cleanup_block } => {
                if let Some(target) = self.blocks.get(cleanup_block) {
                    self.builder
                        .build_unconditional_branch(*target)
                        .map_err(|e| {
                            CompileError::new(
                                &format!("LLVM ICE: {:?}", e),
                                0,
                                0,
                                "",
                                ErrorCode::E0009,
                            )
                        })?;
                }
            }

            SemanticInstruction::ChannelDecl { name, type_ } => {
                let alloca = self.create_entry_alloca(name, type_)?;
                self.variables.insert(name.clone(), alloca);
            }

            SemanticInstruction::Send { channel, value } => {
                let val = self.compile_value(value)?;
                if let Some(ptr) = self.variables.get(channel) {
                    self.builder.build_store(*ptr, val).map_err(|e| {
                        CompileError::new(&format!("LLVM ICE: {:?}", e), 0, 0, "", ErrorCode::E0009)
                    })?;
                }
            }

            SemanticInstruction::Receive { channel, target } => {
                if let Some(ptr) = self.variables.get(channel) {
                    let loaded = self
                        .builder
                        .build_load(self.context.f64_type(), *ptr, channel)
                        .map_err(|e| {
                            CompileError::new(
                                &format!("LLVM ICE: {:?}", e),
                                0,
                                0,
                                "",
                                ErrorCode::E0009,
                            )
                        })?;
                    if !target.is_empty() {
                        if let Some(target_ptr) = self.variables.get(target) {
                            self.builder.build_store(*target_ptr, loaded).map_err(|e| {
                                CompileError::new(
                                    &format!("LLVM ICE: {:?}", e),
                                    0,
                                    0,
                                    "",
                                    ErrorCode::E0009,
                                )
                            })?;
                        }
                    }
                }
            }
            SemanticInstruction::MethodCall {
                result,
                receiver,
                receiver_type: _,
                method_name,
                args,
                return_type,
            } => {
                let receiver_val = self.compile_value(receiver)?;
                let mut all_args = vec![receiver_val];
                for arg in args {
                    all_args.push(self.compile_value(arg)?);
                }

                let function_name = format!("Int_{}", method_name); // Phase 3: use receiver type
                if let Some(func) = self.functions.get(&function_name).cloned() {
                    let arg_metadata: Vec<inkwell::values::BasicMetadataValueEnum> =
                        all_args.iter().map(|v| (*v).into()).collect();
                    let call = self
                        .builder
                        .build_direct_call(func, &arg_metadata, "method_call")
                        .map_err(|e| {
                            CompileError::new(
                                &format!("LLVM ICE: {:?}", e),
                                0,
                                0,
                                "",
                                ErrorCode::E0009,
                            )
                        })?;
                    if let Some(result_name) = result {
                        if let Some(val) = call.try_as_basic_value().basic() {
                            let alloca = self.create_entry_alloca(result_name, return_type)?;
                            self.builder.build_store(alloca, val).map_err(|e| {
                                CompileError::new(
                                    &format!("LLVM ICE: {:?}", e),
                                    0,
                                    0,
                                    "",
                                    ErrorCode::E0009,
                                )
                            })?;
                            self.variables.insert(result_name.clone(), alloca);
                        }
                    }
                }
            }
        }
        Ok(())
    }

    fn compile_value(&self, value: &TypedIRValue) -> Result<BasicValueEnum<'ctx>> {
        match value {
            TypedIRValue::Int(i) => Ok(self
                .context
                .i64_type()
                .const_int(*i as u64, true)
                .as_basic_value_enum()),
            TypedIRValue::Float(f) => Ok(self
                .context
                .f64_type()
                .const_float(*f)
                .as_basic_value_enum()),
            TypedIRValue::String(s) => {
                let global = self
                    .builder
                    .build_global_string_ptr(s, "str")
                    .map_err(|e| {
                        CompileError::new(&format!("LLVM ICE: {:?}", e), 0, 0, "", ErrorCode::E0009)
                    })?;
                Ok(global.as_pointer_value().as_basic_value_enum())
            }
            TypedIRValue::Bool(b) => Ok(self
                .context
                .bool_type()
                .const_int(*b as u64, false)
                .as_basic_value_enum()),
            TypedIRValue::Void => Ok(self
                .context
                .f64_type()
                .const_float(0.0)
                .as_basic_value_enum()),
            TypedIRValue::PtrLiteral(val) => Ok(self
                .context
                .i64_type()
                .const_int(*val as u64, false)
                .as_basic_value_enum()),
            TypedIRValue::NullPtr => Ok(self
                .context
                .i64_type()
                .const_int(0, false)
                .as_basic_value_enum()),
            TypedIRValue::List(values, _) => {
                // For list literal used directly (not in Declare), return first element for backward compat
                // But proper handling is in Declare and IteratorInit
                if let Some(first) = values.first() {
                    self.compile_value(first)
                } else {
                    Ok(self
                        .context
                        .f64_type()
                        .const_float(0.0)
                        .as_basic_value_enum())
                }
            }
            TypedIRValue::Some(v) => self.compile_value(v),
            TypedIRValue::None { .. } => Ok(self
                .context
                .f64_type()
                .const_float(0.0)
                .as_basic_value_enum()),
            TypedIRValue::Ok { value: v, .. } => self.compile_value(v),
            TypedIRValue::Error { value: v, .. } => self.compile_value(v),
            TypedIRValue::Variable(name, type_) => {
                if let Some(ptr) = self.variables.get(name) {
                    match type_ {
                        Type::Int => {
                            let loaded = self
                                .builder
                                .build_load(self.context.i64_type(), *ptr, name)
                                .map_err(|e| {
                                    CompileError::new(
                                        &format!("LLVM ICE: {:?}", e),
                                        0,
                                        0,
                                        "",
                                        ErrorCode::E0009,
                                    )
                                })?;
                            Ok(loaded.as_basic_value_enum())
                        }
                        Type::Bool => {
                            let loaded = self
                                .builder
                                .build_load(self.context.bool_type(), *ptr, name)
                                .map_err(|e| {
                                    CompileError::new(
                                        &format!("LLVM ICE: {:?}", e),
                                        0,
                                        0,
                                        "",
                                        ErrorCode::E0009,
                                    )
                                })?;
                            Ok(loaded.as_basic_value_enum())
                        }
                        Type::String => {
                            let loaded = self
                                .builder
                                .build_load(
                                    self.context.ptr_type(AddressSpace::default()),
                                    *ptr,
                                    name,
                                )
                                .map_err(|e| {
                                    CompileError::new(
                                        &format!("LLVM ICE: {:?}", e),
                                        0,
                                        0,
                                        "",
                                        ErrorCode::E0009,
                                    )
                                })?;
                            Ok(loaded.as_basic_value_enum())
                        }
                        _ => {
                            let loaded = self
                                .builder
                                .build_load(self.context.f64_type(), *ptr, name)
                                .map_err(|e| {
                                    CompileError::new(
                                        &format!("LLVM ICE: {:?}", e),
                                        0,
                                        0,
                                        "",
                                        ErrorCode::E0009,
                                    )
                                })?;
                            Ok(loaded.as_basic_value_enum())
                        }
                    }
                } else {
                    Ok(self
                        .context
                        .f64_type()
                        .const_float(0.0)
                        .as_basic_value_enum())
                }
            }
            TypedIRValue::Cast { value, target_type } => {
                let val = self.compile_value(value)?;
                self.coerce_to_type(val, target_type)
            }
            TypedIRValue::BinaryOp {
                op,
                left,
                right,
                result_type,
            } => {
                let l = self.compile_value(left)?;
                let r = self.compile_value(right)?;
                let result = self.compile_binary_op(op, &l, &r)?;
                self.coerce_to_type(result, result_type)
            }
            TypedIRValue::Call {
                function,
                args,
                return_type,
            } => {
                if function.starts_with("File.") {
                    let compiled_args: Vec<BasicValueEnum> = args
                        .iter()
                        .map(|a| self.compile_value(a))
                        .collect::<Result<Vec<_>>>()?;
                    return self.call_file_function(function, &compiled_args);
                }
                if function.starts_with("List.") {
                    // Check for direct List or Cast-wrapped List
                    let list_elements = match args.first() {
                        Some(TypedIRValue::List(elements, _)) => Some(elements),
                        Some(TypedIRValue::Cast { value, .. }) => {
                            if let TypedIRValue::List(elements, _) = value.as_ref() {
                                Some(elements)
                            } else {
                                None
                            }
                        }
                        _ => None,
                    };

                    if let Some(elements) = list_elements {
                        return self.evaluate_list_operation(function, elements);
                    }
                    return Ok(self
                        .context
                        .f64_type()
                        .const_float(0.0)
                        .as_basic_value_enum());
                }
                if function.starts_with("String.") {
                    let compiled_args: Vec<BasicValueEnum> = args
                        .iter()
                        .map(|a| self.compile_value(a))
                        .collect::<Result<Vec<_>>>()?;
                    return self.call_string_function(function, &compiled_args);
                }
                let func = self.functions.get(function).cloned().ok_or_else(|| {
                    CompileError::new(
                        &format!("Undefined function '{}'", function),
                        0,
                        0,
                        "",
                        ErrorCode::E0004,
                    )
                })?;
                let arg_vals: Vec<BasicValueEnum> = args
                    .iter()
                    .map(|a| self.compile_value(a))
                    .collect::<Result<Vec<_>>>()?;
                let arg_metadata: Vec<inkwell::values::BasicMetadataValueEnum> =
                    arg_vals.iter().map(|v| (*v).into()).collect();
                let call = self
                    .builder
                    .build_direct_call(func, &arg_metadata, "calltmp")
                    .map_err(|e| {
                        CompileError::new(&format!("LLVM ICE: {:?}", e), 0, 0, "", ErrorCode::E0009)
                    })?;
                if let Some(val) = call.try_as_basic_value().basic() {
                    self.coerce_to_type(val, return_type)
                } else {
                    Ok(self
                        .context
                        .f64_type()
                        .const_float(0.0)
                        .as_basic_value_enum())
                }
            }
            TypedIRValue::ArrayAccess {
                array,
                index,
                element_type: _,
            } => {
                // array can be Variable or List literal
                let idx_val = self.compile_value(index)?;
                let idx_int = match idx_val {
                    BasicValueEnum::IntValue(iv) => iv,
                    BasicValueEnum::FloatValue(fv) => {
                        // convert float index to int
                        self.builder.build_float_to_signed_int(fv, self.context.i64_type(), "idx_f2i").map_err(|e| {
                            CompileError::new(&format!("LLVM ICE: {:?}", e), 0, 0, "", ErrorCode::E0009)
                        })?
                    }
                    _ => self.context.i64_type().const_int(0, false),
                };
                // If array is a variable that is a list
                if let TypedIRValue::Variable(var_name, _) = array.as_ref() {
                    if let Some(arr_ptr) = self.list_arrays.get(var_name).cloned() {
                        let f64_type = self.context.f64_type();
                        let ptr = unsafe {
                            self.builder.build_gep(
                                f64_type.array_type(self.list_lengths.get(var_name).copied().unwrap_or(0) as u32),
                                arr_ptr,
                                &[self.context.i64_type().const_int(0, false), idx_int],
                                "arr_idx_ptr",
                            ).map_err(|e| {
                                CompileError::new(&format!("LLVM ICE: {:?}", e), 0, 0, "", ErrorCode::E0009)
                            })?
                        };
                        let loaded = self.builder.build_load(f64_type, ptr, "arr_elem").map_err(|e| {
                            CompileError::new(&format!("LLVM ICE: {:?}", e), 0, 0, "", ErrorCode::E0009)
                        })?;
                        Ok(loaded.as_basic_value_enum())
                    } else if let Some(var_ptr) = self.variables.get(var_name).cloned() {
                        // Fallback: if variable holds f64 (single element case)
                        let loaded = self.builder.build_load(self.context.f64_type(), var_ptr, "var_load").map_err(|e| {
                            CompileError::new(&format!("LLVM ICE: {:?}", e), 0, 0, "", ErrorCode::E0009)
                        })?;
                        Ok(loaded.as_basic_value_enum())
                    } else {
                        Ok(self.context.f64_type().const_float(0.0).as_basic_value_enum())
                    }
                } else if let TypedIRValue::List(elements, _) = array.as_ref() {
                    // Direct list literal access with constant index
                    if let Some(elem) = elements.get(idx_int.get_sign_extended_constant().unwrap_or(0) as usize) {
                        self.compile_value(elem)
                    } else {
                        Ok(self.context.f64_type().const_float(0.0).as_basic_value_enum())
                    }
                } else {
                    Ok(self.context.f64_type().const_float(0.0).as_basic_value_enum())
                }
            }
            TypedIRValue::Borrow { expr, .. } => {
                // Borrow creates a reference - for now, just compile the inner value
                // TODO: Implement proper reference semantics
                self.compile_value(expr)
            }
            TypedIRValue::MutBorrow { expr, .. } => {
                // Mutable borrow - for now, just compile the inner value
                // TODO: Implement proper mutable reference semantics
                self.compile_value(expr)
            }
            TypedIRValue::Deref { expr, .. } => {
                // Dereference - load value from pointer
                // TODO: Implement proper load instruction
                self.compile_value(expr)
            }
            TypedIRValue::AddrOf { expr, .. } => {
                // Address of - get pointer to value
                // TODO: Implement proper address-of instruction
                self.compile_value(expr)
            }
            TypedIRValue::MethodCall {
                receiver,
                receiver_type,
                method_name,
                args,
                return_type,
            } => {
                // Build function name like "Int_compare"
                let function_name = format!("{}_{}", receiver_type, method_name);

                // Compile receiver
                let receiver_val = self.compile_value(receiver)?;

                // Compile all arguments
                let mut all_args = vec![receiver_val];
                for arg in args {
                    all_args.push(self.compile_value(arg)?);
                }

                // Look up the method function
                if let Some(func) = self.functions.get(&function_name).cloned() {
                    let arg_metadata: Vec<inkwell::values::BasicMetadataValueEnum> =
                        all_args.iter().map(|v| (*v).into()).collect();
                    let call = self
                        .builder
                        .build_direct_call(func, &arg_metadata, "method_call")
                        .map_err(|e| {
                            CompileError::new(
                                &format!("LLVM ICE: {:?}", e),
                                0,
                                0,
                                "",
                                ErrorCode::E0009,
                            )
                        })?;
                    if let Some(val) = call.try_as_basic_value().basic() {
                        self.coerce_to_type(val, return_type)
                    } else {
                        Ok(self
                            .context
                            .f64_type()
                            .const_float(0.0)
                            .as_basic_value_enum())
                    }
                } else {
                    // Method function not registered yet (Phase 3 will fix)
                    // For now, return default value based on return type
                    match return_type {
                        Type::Int => Ok(self
                            .context
                            .i64_type()
                            .const_int(0, false)
                            .as_basic_value_enum()),
                        Type::Float => Ok(self
                            .context
                            .f64_type()
                            .const_float(0.0)
                            .as_basic_value_enum()),
                        Type::Bool => Ok(self
                            .context
                            .bool_type()
                            .const_int(0, false)
                            .as_basic_value_enum()),
                        Type::String => Ok(self
                            .context
                            .ptr_type(AddressSpace::default())
                            .const_null()
                            .as_basic_value_enum()),
                        _ => Ok(self
                            .context
                            .f64_type()
                            .const_float(0.0)
                            .as_basic_value_enum()),
                    }
                }
            }
        }
    }

    fn compile_binary_op(
        &self,
        op: &SemanticBinOp,
        left: &BasicValueEnum<'ctx>,
        right: &BasicValueEnum<'ctx>,
    ) -> Result<BasicValueEnum<'ctx>> {
        match op {
            SemanticBinOp::Add => {
                if left.is_float_value() && right.is_float_value() {
                    let r = self
                        .builder
                        .build_float_add(
                            left.into_float_value(),
                            right.into_float_value(),
                            "addtmp",
                        )
                        .map_err(|e| {
                            CompileError::new(
                                &format!("LLVM ICE: {:?}", e),
                                0,
                                0,
                                "",
                                ErrorCode::E0009,
                            )
                        })?;
                    Ok(r.as_basic_value_enum())
                } else if left.is_int_value() && right.is_int_value() {
                    let r = self
                        .builder
                        .build_int_add(left.into_int_value(), right.into_int_value(), "addtmp")
                        .map_err(|e| {
                            CompileError::new(
                                &format!("LLVM ICE: {:?}", e),
                                0,
                                0,
                                "",
                                ErrorCode::E0009,
                            )
                        })?;
                    Ok(r.as_basic_value_enum())
                } else {
                    let l = self.promote_to_float(*left)?;
                    let r = self.promote_to_float(*right)?;
                    let res = self.builder.build_float_add(l, r, "addtmp").map_err(|e| {
                        CompileError::new(&format!("LLVM ICE: {:?}", e), 0, 0, "", ErrorCode::E0009)
                    })?;
                    Ok(res.as_basic_value_enum())
                }
            }
            SemanticBinOp::Subtract => {
                if left.is_float_value() && right.is_float_value() {
                    let r = self
                        .builder
                        .build_float_sub(
                            left.into_float_value(),
                            right.into_float_value(),
                            "subtmp",
                        )
                        .map_err(|e| {
                            CompileError::new(
                                &format!("LLVM ICE: {:?}", e),
                                0,
                                0,
                                "",
                                ErrorCode::E0009,
                            )
                        })?;
                    Ok(r.as_basic_value_enum())
                } else if left.is_int_value() && right.is_int_value() {
                    let r = self
                        .builder
                        .build_int_sub(left.into_int_value(), right.into_int_value(), "subtmp")
                        .map_err(|e| {
                            CompileError::new(
                                &format!("LLVM ICE: {:?}", e),
                                0,
                                0,
                                "",
                                ErrorCode::E0009,
                            )
                        })?;
                    Ok(r.as_basic_value_enum())
                } else {
                    let l = self.promote_to_float(*left)?;
                    let r = self.promote_to_float(*right)?;
                    let res = self.builder.build_float_sub(l, r, "subtmp").map_err(|e| {
                        CompileError::new(&format!("LLVM ICE: {:?}", e), 0, 0, "", ErrorCode::E0009)
                    })?;
                    Ok(res.as_basic_value_enum())
                }
            }
            SemanticBinOp::Multiply => {
                if left.is_float_value() && right.is_float_value() {
                    let r = self
                        .builder
                        .build_float_mul(
                            left.into_float_value(),
                            right.into_float_value(),
                            "multmp",
                        )
                        .map_err(|e| {
                            CompileError::new(
                                &format!("LLVM ICE: {:?}", e),
                                0,
                                0,
                                "",
                                ErrorCode::E0009,
                            )
                        })?;
                    Ok(r.as_basic_value_enum())
                } else if left.is_int_value() && right.is_int_value() {
                    let r = self
                        .builder
                        .build_int_mul(left.into_int_value(), right.into_int_value(), "multmp")
                        .map_err(|e| {
                            CompileError::new(
                                &format!("LLVM ICE: {:?}", e),
                                0,
                                0,
                                "",
                                ErrorCode::E0009,
                            )
                        })?;
                    Ok(r.as_basic_value_enum())
                } else {
                    let l = self.promote_to_float(*left)?;
                    let r = self.promote_to_float(*right)?;
                    let res = self.builder.build_float_mul(l, r, "multmp").map_err(|e| {
                        CompileError::new(&format!("LLVM ICE: {:?}", e), 0, 0, "", ErrorCode::E0009)
                    })?;
                    Ok(res.as_basic_value_enum())
                }
            }
            SemanticBinOp::Divide => {
                let l = self.promote_to_float(*left)?;
                let r = self.promote_to_float(*right)?;
                let res = self.builder.build_float_div(l, r, "divtmp").map_err(|e| {
                    CompileError::new(&format!("LLVM ICE: {:?}", e), 0, 0, "", ErrorCode::E0009)
                })?;
                Ok(res.as_basic_value_enum())
            }
            SemanticBinOp::Greater => {
                let cmp = self
                    .builder
                    .build_float_compare(
                        FloatPredicate::OGT,
                        self.promote_to_float(*left)?,
                        self.promote_to_float(*right)?,
                        "cmptmp",
                    )
                    .map_err(|e| {
                        CompileError::new(&format!("LLVM ICE: {:?}", e), 0, 0, "", ErrorCode::E0009)
                    })?;
                Ok(cmp.as_basic_value_enum())
            }
            SemanticBinOp::Less => {
                let cmp = self
                    .builder
                    .build_float_compare(
                        FloatPredicate::OLT,
                        self.promote_to_float(*left)?,
                        self.promote_to_float(*right)?,
                        "cmptmp",
                    )
                    .map_err(|e| {
                        CompileError::new(&format!("LLVM ICE: {:?}", e), 0, 0, "", ErrorCode::E0009)
                    })?;
                Ok(cmp.as_basic_value_enum())
            }
            SemanticBinOp::GreaterEqual => {
                let cmp = self
                    .builder
                    .build_float_compare(
                        FloatPredicate::OGE,
                        self.promote_to_float(*left)?,
                        self.promote_to_float(*right)?,
                        "cmptmp",
                    )
                    .map_err(|e| {
                        CompileError::new(&format!("LLVM ICE: {:?}", e), 0, 0, "", ErrorCode::E0009)
                    })?;
                Ok(cmp.as_basic_value_enum())
            }
            SemanticBinOp::LessEqual => {
                let cmp = self
                    .builder
                    .build_float_compare(
                        FloatPredicate::OLE,
                        self.promote_to_float(*left)?,
                        self.promote_to_float(*right)?,
                        "cmptmp",
                    )
                    .map_err(|e| {
                        CompileError::new(&format!("LLVM ICE: {:?}", e), 0, 0, "", ErrorCode::E0009)
                    })?;
                Ok(cmp.as_basic_value_enum())
            }
            SemanticBinOp::Equal => {
                let cmp = self
                    .builder
                    .build_float_compare(
                        FloatPredicate::OEQ,
                        self.promote_to_float(*left)?,
                        self.promote_to_float(*right)?,
                        "cmptmp",
                    )
                    .map_err(|e| {
                        CompileError::new(&format!("LLVM ICE: {:?}", e), 0, 0, "", ErrorCode::E0009)
                    })?;
                Ok(cmp.as_basic_value_enum())
            }
            SemanticBinOp::NotEqual => {
                let cmp = self
                    .builder
                    .build_float_compare(
                        FloatPredicate::ONE,
                        self.promote_to_float(*left)?,
                        self.promote_to_float(*right)?,
                        "cmptmp",
                    )
                    .map_err(|e| {
                        CompileError::new(&format!("LLVM ICE: {:?}", e), 0, 0, "", ErrorCode::E0009)
                    })?;
                Ok(cmp.as_basic_value_enum())
            }
            SemanticBinOp::And => {
                let l = self.to_bool(*left)?;
                let r = self.to_bool(*right)?;
                let res = self.builder.build_and(l, r, "andtmp").map_err(|e| {
                    CompileError::new(&format!("LLVM ICE: {:?}", e), 0, 0, "", ErrorCode::E0009)
                })?;
                Ok(res.as_basic_value_enum())
            }
            SemanticBinOp::Or => {
                let l = self.to_bool(*left)?;
                let r = self.to_bool(*right)?;
                let res = self.builder.build_or(l, r, "ortmp").map_err(|e| {
                    CompileError::new(&format!("LLVM ICE: {:?}", e), 0, 0, "", ErrorCode::E0009)
                })?;
                Ok(res.as_basic_value_enum())
            }
        }
    }

    fn promote_to_float(
        &self,
        val: BasicValueEnum<'ctx>,
    ) -> Result<inkwell::values::FloatValue<'ctx>> {
        if val.is_float_value() {
            Ok(val.into_float_value())
        } else if val.is_int_value() {
            let int_val = val.into_int_value();
            let float_type = self.context.f64_type();
            self.builder
                .build_signed_int_to_float(int_val, float_type, "int_to_float")
                .map_err(|e| {
                    CompileError::new(
                        &format!("Failed to convert: {}", e),
                        0,
                        0,
                        "",
                        ErrorCode::E0001,
                    )
                })
        } else {
            Err(CompileError::new(
                "Cannot convert non-numeric to float",
                0,
                0,
                "",
                ErrorCode::E0001,
            ))
        }
    }

    fn to_bool(&self, val: BasicValueEnum<'ctx>) -> Result<IntValue<'ctx>> {
        if val.is_float_value() {
            let zero = self.context.f64_type().const_float(0.0);
            ice_res(
                self.builder.build_float_compare(
                    FloatPredicate::ONE,
                    val.into_float_value(),
                    zero,
                    "tobool",
                ),
                "tobool",
            )
        } else {
            Ok(val.into_int_value())
        }
    }
    fn coerce_to_type(
        &self,
        val: BasicValueEnum<'ctx>,
        target: &Type,
    ) -> Result<BasicValueEnum<'ctx>> {
        match target {
            Type::Float => {
                if val.is_float_value() {
                    Ok(val)
                } else if val.is_int_value() {
                    Ok(self.promote_to_float(val)?.as_basic_value_enum())
                } else {
                    Ok(val)
                }
            }
            Type::Int => {
                if val.is_int_value() {
                    Ok(val)
                } else if val.is_float_value() {
                    let int = self
                        .builder
                        .build_float_to_signed_int(
                            val.into_float_value(),
                            self.context.i64_type(),
                            "ftoi",
                        )
                        .map_err(|e| {
                            CompileError::new(
                                &format!("LLVM ICE: {:?}", e),
                                0,
                                0,
                                "",
                                ErrorCode::E0009,
                            )
                        })?;
                    Ok(int.as_basic_value_enum())
                } else {
                    Ok(val)
                }
            }
            _ => Ok(val),
        }
    }

    fn default_value_for_type(&self, type_: &Type) -> BasicValueEnum<'ctx> {
        match type_ {
            Type::Int => self
                .context
                .i64_type()
                .const_int(0, false)
                .as_basic_value_enum(),
            Type::Float => self
                .context
                .f64_type()
                .const_float(0.0)
                .as_basic_value_enum(),
            Type::String => self
                .context
                .ptr_type(AddressSpace::default())
                .const_null()
                .as_basic_value_enum(),
            Type::Bool => self
                .context
                .bool_type()
                .const_int(0, false)
                .as_basic_value_enum(),
            _ => self
                .context
                .f64_type()
                .const_float(0.0)
                .as_basic_value_enum(),
        }
    }

    fn map_type(&self, type_: &Type) -> BasicTypeEnum<'ctx> {
        match type_ {
            Type::Int => self.context.i64_type().as_basic_type_enum(),
            Type::Float => self.context.f64_type().as_basic_type_enum(),
            Type::String => self
                .context
                .ptr_type(AddressSpace::default())
                .as_basic_type_enum(),
            Type::Bool => self.context.bool_type().as_basic_type_enum(),
            Type::Void => self.context.f64_type().as_basic_type_enum(),
            _ => self.context.f64_type().as_basic_type_enum(),
        }
    }

    fn create_entry_alloca(&self, name: &str, type_: &Type) -> Result<PointerValue<'ctx>> {
        let builder = self.context.create_builder();
        let entry = ice_entry(self)?;
        match entry.get_first_instruction() {
            Some(first) => builder.position_before(&first),
            None => builder.position_at_end(entry),
        }
        let alloca_type = self.map_type(type_);
        ice_res(builder.build_alloca(alloca_type, name), "build_alloca")
    }
    fn emit_print(&self, val: &BasicValueEnum<'ctx>) -> Result<()> {
        let printf_func = ice_opt(self.functions.get("printf").copied(), "missing printf")?;
        if val.is_float_value() {
            let format = ice_res(
                self.builder.build_global_string_ptr("%.1f\n", "fmt_float"),
                "build_global_string_ptr float",
            )?;
            ice_res(
                self.builder.build_direct_call(
                    printf_func,
                    &[format.as_pointer_value().into(), (*val).into()],
                    "printf_call",
                ),
                "build_direct_call float",
            )?;
        } else if val.is_int_value() {
            let format = ice_res(
                self.builder.build_global_string_ptr("%lld\n", "fmt_int"),
                "build_global_string_ptr int",
            )?;
            ice_res(
                self.builder.build_direct_call(
                    printf_func,
                    &[format.as_pointer_value().into(), (*val).into()],
                    "printf_call",
                ),
                "build_direct_call int",
            )?;
        } else {
            let format = ice_res(
                self.builder.build_global_string_ptr("%s\n", "fmt_str"),
                "build_global_string_ptr str",
            )?;
            ice_res(
                self.builder.build_direct_call(
                    printf_func,
                    &[format.as_pointer_value().into(), (*val).into()],
                    "printf_call",
                ),
                "build_direct_call str",
            )?;
        }
        Ok(())
    }

    fn emit_print_string_raw(&self, literal: &str) -> Result<()> {
        let printf_func = ice_opt(self.functions.get("printf").copied(), "missing printf")?;
        let fmt = ice_res(
            self.builder.build_global_string_ptr(literal, "fmt_raw"),
            "build_global_string_ptr raw",
        )?;
        ice_res(
            self.builder.build_direct_call(
                printf_func,
                &[fmt.as_pointer_value().into()],
                "printf_raw",
            ),
            "build_direct_call raw",
        )?;
        Ok(())
    }

    fn emit_print_float_raw(&self, val: &BasicValueEnum<'ctx>) -> Result<()> {
        let printf_func = ice_opt(self.functions.get("printf").copied(), "missing printf")?;
        let format = ice_res(
            self.builder.build_global_string_ptr("%.1f", "fmt_float_raw"),
            "build_global_string_ptr float raw",
        )?;
        ice_res(
            self.builder.build_direct_call(
                printf_func,
                &[format.as_pointer_value().into(), (*val).into()],
                "printf_float_raw",
            ),
            "build_direct_call float raw",
        )?;
        Ok(())
    }

    fn emit_print_list_literal(&self, elements: &[TypedIRValue]) -> Result<()> {
        self.emit_print_string_raw("[")?;
        for (i, elem) in elements.iter().enumerate() {
            if i > 0 {
                self.emit_print_string_raw(", ")?;
            }
            let val = self.compile_value(elem)?;
            if val.is_float_value() {
                self.emit_print_float_raw(&val)?;
            } else if val.is_int_value() {
                // print int as float style? keep int
                let printf_func = ice_opt(self.functions.get("printf").copied(), "missing printf")?;
                let format = ice_res(
                    self.builder.build_global_string_ptr("%lld", "fmt_int_raw"),
                    "build_global_string_ptr int raw",
                )?;
                ice_res(
                    self.builder.build_direct_call(
                        printf_func,
                        &[format.as_pointer_value().into(), val.into()],
                        "printf_int_raw",
                    ),
                    "build_direct_call int raw",
                )?;
            } else {
                self.emit_print(&val)?;
            }
        }
        self.emit_print_string_raw("]\n")?;
        Ok(())
    }

    fn emit_print_list_var(&self, var_name: &str) -> Result<()> {
        let printf_func = ice_opt(self.functions.get("printf").copied(), "missing printf")?;
        let arr_ptr = self.list_arrays.get(var_name).copied().ok_or_else(|| {
            CompileError::new(&format!("Unknown list array {}", var_name), 0, 0, "", ErrorCode::E0009)
        })?;
        let len = self.list_lengths.get(var_name).copied().unwrap_or(0);
        let f64_type = self.context.f64_type();

        self.emit_print_string_raw("[")?;

        for i in 0..len {
            if i > 0 {
                self.emit_print_string_raw(", ")?;
            }
            let idx = self.context.i64_type().const_int(i as u64, false);
            let array_type = f64_type.array_type(len as u32);
            let elem_ptr = unsafe {
                self.builder.build_gep(
                    array_type,
                    arr_ptr,
                    &[self.context.i64_type().const_int(0, false), idx],
                    &format!("list_print_ptr_{}_{}", var_name, i),
                ).map_err(|e| {
                    CompileError::new(&format!("LLVM ICE: {:?}", e), 0, 0, "", ErrorCode::E0009)
                })?
            };
            let loaded = self.builder.build_load(f64_type, elem_ptr, &format!("list_print_val_{}_{}", var_name, i)).map_err(|e| {
                CompileError::new(&format!("LLVM ICE: {:?}", e), 0, 0, "", ErrorCode::E0009)
            })?;
            // print loaded float without newline
            let format = ice_res(
                self.builder.build_global_string_ptr("%.1f", "fmt_float_raw"),
                "build_global_string_ptr float raw",
            )?;
            ice_res(
                self.builder.build_direct_call(
                    printf_func,
                    &[format.as_pointer_value().into(), loaded.as_basic_value_enum().into()],
                    "printf_list_elem",
                ),
                "build_direct_call list elem",
            )?;
        }

        self.emit_print_string_raw("]\n")?;
        Ok(())
    }

    fn register_stdlib(&mut self) {
        let i32_type = self.context.i32_type();
        let i8_ptr_type = self.context.ptr_type(AddressSpace::default());
        let printf_type = i32_type.fn_type(&[i8_ptr_type.into()], true);
        let printf = self.module.add_function("printf", printf_type, None);
        self.functions.insert("printf".to_string(), printf);
        let f64_type = self.context.f64_type();
        let math_fns = [
            ("Math.sqrt", "sqrt"),
            ("Math.pow", "pow"),
            ("Math.sin", "sin"),
            ("Math.cos", "cos"),
            ("Math.abs", "fabs"),
            ("Math.floor", "floor"),
            ("Math.ceil", "ceil"),
            ("Math.exp", "exp"),
            ("Math.log", "log"),
            ("Math.tan", "tan"),
        ];
        for (algol_name, c_name) in math_fns {
            let fn_val =
                self.module
                    .add_function(c_name, f64_type.fn_type(&[f64_type.into()], false), None);
            self.functions.insert(algol_name.to_string(), fn_val);
            self.functions.insert(c_name.to_string(), fn_val); // ADD THIS
        }
        let ptr_type = self.context.ptr_type(AddressSpace::default());
        let strlen_fn = self.module.add_function(
            "strlen",
            self.context.i64_type().fn_type(&[ptr_type.into()], false),
            None,
        );
        self.functions
            .insert("String.length".to_string(), strlen_fn);
        let fopen_fn = self.module.add_function(
            "fopen",
            ptr_type.fn_type(&[ptr_type.into(), ptr_type.into()], false),
            None,
        );
        self.functions.insert("File.open".to_string(), fopen_fn);
        let fclose_fn =
            self.module
                .add_function("fclose", i32_type.fn_type(&[ptr_type.into()], false), None);
        self.functions.insert("File.close".to_string(), fclose_fn);
        let fprintf_fn = self.module.add_function(
            "fprintf",
            i32_type.fn_type(&[ptr_type.into(), ptr_type.into()], true),
            None,
        );
        self.functions.insert("File.write".to_string(), fprintf_fn);
        let fgets_fn = self.module.add_function(
            "fgets",
            ptr_type.fn_type(&[ptr_type.into(), i32_type.into(), ptr_type.into()], false),
            None,
        );
        self.functions.insert("File.read".to_string(), fgets_fn);
        let fclose_fn =
            self.module
                .add_function("fclose", i32_type.fn_type(&[ptr_type.into()], false), None);
        self.functions.insert("File.close".to_string(), fclose_fn);
        let i64_type = self.context.i64_type();
        let pthread_create_fn = self.module.add_function(
            "pthread_create",
            i32_type.fn_type(
                &[
                    ptr_type.into(),
                    ptr_type.into(),
                    ptr_type.into(),
                    ptr_type.into(),
                ],
                false,
            ),
            None,
        );
        self.functions
            .insert("pthread_create".to_string(), pthread_create_fn);
        let pthread_join_fn = self.module.add_function(
            "pthread_join",
            i32_type.fn_type(&[i64_type.into(), ptr_type.into()], false),
            None,
        );
        self.functions
            .insert("pthread_join".to_string(), pthread_join_fn);
        let strcat_type = ptr_type.fn_type(&[ptr_type.into(), ptr_type.into()], false);
        self.module.add_function("strcat", strcat_type, None);
        let strcpy_type = ptr_type.fn_type(&[ptr_type.into(), ptr_type.into()], false);
        self.module.add_function("strcpy", strcpy_type, None);
        let toupper_type = i32_type.fn_type(&[i32_type.into()], false);
        self.module.add_function("toupper", toupper_type, None);
        let tolower_type = i32_type.fn_type(&[i32_type.into()], false);
        self.module.add_function("tolower", tolower_type, None);
        let malloc_type = ptr_type.fn_type(&[i64_type.into()], false);
        self.module.add_function("malloc", malloc_type, None);
    }

    pub fn evaluate_list_operation(
        &self,
        name: &str,
        list_values: &[TypedIRValue],
    ) -> Result<BasicValueEnum<'ctx>> {
        let mut values: Vec<f64> = Vec::new();
        for v in list_values {
            match v {
                TypedIRValue::Float(f) => values.push(*f),
                TypedIRValue::Int(i) => values.push(*i as f64),
                _ => {}
            }
        }
        match name {
            "List.length" => Ok(self
                .context
                .f64_type()
                .const_float(values.len() as f64)
                .as_basic_value_enum()),
            "List.sum" => {
                let sum: f64 = values.iter().sum();
                Ok(self
                    .context
                    .f64_type()
                    .const_float(sum)
                    .as_basic_value_enum())
            }
            "List.max" => {
                let max = values.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
                Ok(self
                    .context
                    .f64_type()
                    .const_float(max)
                    .as_basic_value_enum())
            }
            "List.min" => {
                let min = values.iter().cloned().fold(f64::INFINITY, f64::min);
                Ok(self
                    .context
                    .f64_type()
                    .const_float(min)
                    .as_basic_value_enum())
            }
            _ => Ok(self
                .context
                .f64_type()
                .const_float(0.0)
                .as_basic_value_enum()),
        }
    }

    pub fn call_file_function(
        &self,
        name: &str,
        args: &[BasicValueEnum<'ctx>],
    ) -> Result<BasicValueEnum<'ctx>> {
        let ptr_type = self.context.ptr_type(AddressSpace::default());
        let i32_type = self.context.i32_type();
        let i64_type = self.context.i64_type();
        let fopen_fn = ice_opt(self.module.get_function("fopen"), "missing")?;
        let fclose_fn = ice_opt(self.module.get_function("fclose"), "missing")?;
        let fprintf_fn = ice_opt(self.module.get_function("fprintf"), "missing")?;
        let fgets_fn = ice_opt(self.module.get_function("fgets"), "missing")?;
        match name {
            "File.write" | "File.append" => {
                if args.len() < 2 {
                    return Ok(i32_type.const_int(0, false).as_basic_value_enum());
                }
                let path = args[0];
                let content = args[1];
                let mode_str = if name == "File.write" { "w" } else { "a" };
                let mode = self
                    .builder
                    .build_global_string_ptr(mode_str, "file_mode")
                    .map_err(|e| {
                        CompileError::new(&format!("LLVM ICE: {:?}", e), 0, 0, "", ErrorCode::E0009)
                    })?;
                let fopen_call = self
                    .builder
                    .build_call(
                        fopen_fn,
                        &[path.into(), mode.as_pointer_value().into()],
                        "file_fopen",
                    )
                    .map_err(|e| {
                        CompileError::new(&format!("LLVM ICE: {:?}", e), 0, 0, "", ErrorCode::E0009)
                    })?;
                let file_ptr = fopen_call
                    .try_as_basic_value()
                    .basic()
                    .unwrap_or_else(|| ptr_type.const_null().as_basic_value_enum())
                    .into_pointer_value();
                let current_fn = ice_opt(self.current_function, "missing")?;
                let valid_bb = self.context.append_basic_block(current_fn, "file_valid");
                let null_bb = self.context.append_basic_block(current_fn, "file_null");
                let merge_bb = self.context.append_basic_block(current_fn, "file_merge");
                let is_null = self
                    .builder
                    .build_is_null(file_ptr, "file_isnull")
                    .map_err(|e| {
                        CompileError::new(&format!("LLVM ICE: {:?}", e), 0, 0, "", ErrorCode::E0009)
                    })?;
                self.builder
                    .build_conditional_branch(is_null, null_bb, valid_bb)
                    .map_err(|e| {
                        CompileError::new(&format!("LLVM ICE: {:?}", e), 0, 0, "", ErrorCode::E0009)
                    })?;
                self.builder.position_at_end(valid_bb);
                let format = self
                    .builder
                    .build_global_string_ptr("%s", "file_fmt")
                    .map_err(|e| {
                        CompileError::new(&format!("LLVM ICE: {:?}", e), 0, 0, "", ErrorCode::E0009)
                    })?;
                self.builder
                    .build_call(
                        fprintf_fn,
                        &[
                            file_ptr.into(),
                            format.as_pointer_value().into(),
                            content.into(),
                        ],
                        "file_fprintf",
                    )
                    .map_err(|e| {
                        CompileError::new(&format!("LLVM ICE: {:?}", e), 0, 0, "", ErrorCode::E0009)
                    })?;
                self.builder
                    .build_call(fclose_fn, &[file_ptr.into()], "file_fclose")
                    .map_err(|e| {
                        CompileError::new(&format!("LLVM ICE: {:?}", e), 0, 0, "", ErrorCode::E0009)
                    })?;
                let one = i32_type.const_int(1, false);
                self.builder
                    .build_unconditional_branch(merge_bb)
                    .map_err(|e| {
                        CompileError::new(&format!("LLVM ICE: {:?}", e), 0, 0, "", ErrorCode::E0009)
                    })?;
                let valid_incoming = ice_opt(self.builder.get_insert_block(), "missing")?;
                self.builder.position_at_end(null_bb);
                let zero = i32_type.const_int(0, false);
                self.builder
                    .build_unconditional_branch(merge_bb)
                    .map_err(|e| {
                        CompileError::new(&format!("LLVM ICE: {:?}", e), 0, 0, "", ErrorCode::E0009)
                    })?;
                let null_incoming = ice_opt(self.builder.get_insert_block(), "missing")?;
                self.builder.position_at_end(merge_bb);
                let phi = self
                    .builder
                    .build_phi(i32_type, "file_result")
                    .map_err(|e| {
                        CompileError::new(&format!("LLVM ICE: {:?}", e), 0, 0, "", ErrorCode::E0009)
                    })?;
                phi.add_incoming(&[(&one, valid_incoming), (&zero, null_incoming)]);
                Ok(phi.as_basic_value().as_basic_value_enum())
            }
            "File.read" => {
                if args.is_empty() {
                    return Ok(ptr_type.const_null().as_basic_value_enum());
                }
                let path = args[0];
                let mode_r = self
                    .builder
                    .build_global_string_ptr("r", "file_mode_r")
                    .map_err(|e| {
                        CompileError::new(&format!("LLVM ICE: {:?}", e), 0, 0, "", ErrorCode::E0009)
                    })?;
                let fopen_call = self
                    .builder
                    .build_call(
                        fopen_fn,
                        &[path.into(), mode_r.as_pointer_value().into()],
                        "file_fopen_r",
                    )
                    .map_err(|e| {
                        CompileError::new(&format!("LLVM ICE: {:?}", e), 0, 0, "", ErrorCode::E0009)
                    })?;
                let file_ptr = fopen_call
                    .try_as_basic_value()
                    .basic()
                    .unwrap_or_else(|| ptr_type.const_null().as_basic_value_enum())
                    .into_pointer_value();
                let malloc_fn = ice_opt(self.module.get_function("malloc"), "missing")?;
                let buf_size = i64_type.const_int(256, false);
                let malloc_call = self
                    .builder
                    .build_call(malloc_fn, &[buf_size.into()], "file_malloc")
                    .map_err(|e| {
                        CompileError::new(&format!("LLVM ICE: {:?}", e), 0, 0, "", ErrorCode::E0009)
                    })?;
                let buf = malloc_call
                    .try_as_basic_value()
                    .basic()
                    .unwrap_or_else(|| ptr_type.const_null().as_basic_value_enum())
                    .into_pointer_value();
                let current_fn = ice_opt(self.current_function, "missing")?;
                let valid_bb = self
                    .context
                    .append_basic_block(current_fn, "file_read_valid");
                let null_bb = self
                    .context
                    .append_basic_block(current_fn, "file_read_null");
                let merge_bb = self
                    .context
                    .append_basic_block(current_fn, "file_read_merge");
                let is_null = self
                    .builder
                    .build_is_null(file_ptr, "file_isnull_r")
                    .map_err(|e| {
                        CompileError::new(&format!("LLVM ICE: {:?}", e), 0, 0, "", ErrorCode::E0009)
                    })?;
                self.builder
                    .build_conditional_branch(is_null, null_bb, valid_bb)
                    .map_err(|e| {
                        CompileError::new(&format!("LLVM ICE: {:?}", e), 0, 0, "", ErrorCode::E0009)
                    })?;
                self.builder.position_at_end(valid_bb);
                let size_val = i32_type.const_int(256, false);
                self.builder
                    .build_call(
                        fgets_fn,
                        &[buf.into(), size_val.into(), file_ptr.into()],
                        "file_fgets",
                    )
                    .map_err(|e| {
                        CompileError::new(&format!("LLVM ICE: {:?}", e), 0, 0, "", ErrorCode::E0009)
                    })?;
                self.builder
                    .build_call(fclose_fn, &[file_ptr.into()], "file_fclose_r")
                    .map_err(|e| {
                        CompileError::new(&format!("LLVM ICE: {:?}", e), 0, 0, "", ErrorCode::E0009)
                    })?;
                self.builder
                    .build_unconditional_branch(merge_bb)
                    .map_err(|e| {
                        CompileError::new(&format!("LLVM ICE: {:?}", e), 0, 0, "", ErrorCode::E0009)
                    })?;
                let valid_incoming = ice_opt(self.builder.get_insert_block(), "missing")?;
                self.builder.position_at_end(null_bb);
                let empty = self
                    .builder
                    .build_global_string_ptr("", "file_empty")
                    .map_err(|e| {
                        CompileError::new(&format!("LLVM ICE: {:?}", e), 0, 0, "", ErrorCode::E0009)
                    })?;
                self.builder
                    .build_unconditional_branch(merge_bb)
                    .map_err(|e| {
                        CompileError::new(&format!("LLVM ICE: {:?}", e), 0, 0, "", ErrorCode::E0009)
                    })?;
                let null_incoming = ice_opt(self.builder.get_insert_block(), "missing")?;
                self.builder.position_at_end(merge_bb);
                let phi = self
                    .builder
                    .build_phi(ptr_type, "file_read_result")
                    .map_err(|e| {
                        CompileError::new(&format!("LLVM ICE: {:?}", e), 0, 0, "", ErrorCode::E0009)
                    })?;
                phi.add_incoming(&[
                    (&buf, valid_incoming),
                    (&empty.as_pointer_value(), null_incoming),
                ]);
                Ok(phi.as_basic_value().as_basic_value_enum())
            }
            _ => Ok(i32_type.const_int(0, false).as_basic_value_enum()),
        }
    }

    pub fn call_string_function(
        &self,
        name: &str,
        args: &[BasicValueEnum<'ctx>],
    ) -> Result<BasicValueEnum<'ctx>> {
        match name {
            "String.length" => {
                let func = ice_opt(self.module.get_function("strlen"), "missing")?;
                let metadata: Vec<inkwell::values::BasicMetadataValueEnum> =
                    args.iter().map(|a| (*a).into()).collect();
                let call = self
                    .builder
                    .build_call(func, &metadata, "strlen_call")
                    .map_err(|e| {
                        CompileError::new(&format!("LLVM ICE: {:?}", e), 0, 0, "", ErrorCode::E0009)
                    })?;
                Ok(call.try_as_basic_value().basic().unwrap_or_else(|| {
                    self.context
                        .i64_type()
                        .const_int(0, false)
                        .as_basic_value_enum()
                }))
            }
            "String.concat" => {
                if args.len() < 2 {
                    return Ok(self
                        .context
                        .ptr_type(AddressSpace::default())
                        .const_null()
                        .as_basic_value_enum());
                }
                let s1 = args[0];
                let s2 = args[1];
                let ptr_type = self.context.ptr_type(AddressSpace::default());
                let i64_type = self.context.i64_type();
                let strlen_fn = ice_opt(self.module.get_function("strlen"), "missing")?;
                let strcpy_fn = ice_opt(self.module.get_function("strcpy"), "missing")?;
                let strcat_fn = ice_opt(self.module.get_function("strcat"), "missing")?;
                let malloc_fn = ice_opt(self.module.get_function("malloc"), "missing")?;
                let len1 = self
                    .builder
                    .build_call(strlen_fn, &[s1.into()], "len1")
                    .map_err(|e| {
                        CompileError::new(&format!("LLVM ICE: {:?}", e), 0, 0, "", ErrorCode::E0009)
                    })?
                    .try_as_basic_value()
                    .basic()
                    .unwrap_or_else(|| i64_type.const_int(0, false).as_basic_value_enum())
                    .into_int_value();
                let len2 = self
                    .builder
                    .build_call(strlen_fn, &[s2.into()], "len2")
                    .map_err(|e| {
                        CompileError::new(&format!("LLVM ICE: {:?}", e), 0, 0, "", ErrorCode::E0009)
                    })?
                    .try_as_basic_value()
                    .basic()
                    .unwrap_or_else(|| i64_type.const_int(0, false).as_basic_value_enum())
                    .into_int_value();
                let one = i64_type.const_int(1, false);
                let sum = self.builder.build_int_add(len1, len2, "sum").map_err(|e| {
                    CompileError::new(&format!("LLVM ICE: {:?}", e), 0, 0, "", ErrorCode::E0009)
                })?;
                let total = self.builder.build_int_add(sum, one, "total").map_err(|e| {
                    CompileError::new(&format!("LLVM ICE: {:?}", e), 0, 0, "", ErrorCode::E0009)
                })?;
                let buf = self
                    .builder
                    .build_call(malloc_fn, &[total.into()], "malloc_buf")
                    .map_err(|e| {
                        CompileError::new(&format!("LLVM ICE: {:?}", e), 0, 0, "", ErrorCode::E0009)
                    })?
                    .try_as_basic_value()
                    .basic()
                    .unwrap_or_else(|| ptr_type.const_null().as_basic_value_enum())
                    .into_pointer_value();
                self.builder
                    .build_call(strcpy_fn, &[buf.into(), s1.into()], "strcpy_s1")
                    .map_err(|e| {
                        CompileError::new(&format!("LLVM ICE: {:?}", e), 0, 0, "", ErrorCode::E0009)
                    })?;
                let result = self
                    .builder
                    .build_call(strcat_fn, &[buf.into(), s2.into()], "strcat_s2")
                    .map_err(|e| {
                        CompileError::new(&format!("LLVM ICE: {:?}", e), 0, 0, "", ErrorCode::E0009)
                    })?;
                Ok(result
                    .try_as_basic_value()
                    .basic()
                    .unwrap_or_else(|| buf.as_basic_value_enum()))
            }
            "String.to_upper" => self.impl_string_case(args, true),
            "String.to_lower" => self.impl_string_case(args, false),
            "String.substring" => self.impl_string_substring(args),
            _ => {
                if !args.is_empty() {
                    Ok(args[0])
                } else {
                    Ok(self
                        .context
                        .ptr_type(AddressSpace::default())
                        .const_null()
                        .as_basic_value_enum())
                }
            }
        }
    }

    fn impl_string_case(
        &self,
        args: &[BasicValueEnum<'ctx>],
        to_upper: bool,
    ) -> Result<BasicValueEnum<'ctx>> {
        if args.is_empty() {
            return Ok(self
                .context
                .ptr_type(AddressSpace::default())
                .const_null()
                .as_basic_value_enum());
        }
        let s = args[0];
        let i64_type = self.context.i64_type();
        let i32_type = self.context.i32_type();
        let i8_type = self.context.i8_type();
        let ptr_type = self.context.ptr_type(AddressSpace::default());
        let strlen_fn = ice_opt(self.module.get_function("strlen"), "missing")?;
        let malloc_fn = ice_opt(self.module.get_function("malloc"), "missing")?;
        let ctype_fn = if to_upper {
            ice_opt(self.module.get_function("toupper"), "missing")?
        } else {
            ice_opt(self.module.get_function("tolower"), "missing")?
        };
        let len_call = self
            .builder
            .build_call(strlen_fn, &[s.into()], "case_len")
            .map_err(|e| {
                CompileError::new(&format!("LLVM ICE: {:?}", e), 0, 0, "", ErrorCode::E0009)
            })?;
        let len = len_call
            .try_as_basic_value()
            .basic()
            .unwrap_or_else(|| i64_type.const_int(0, false).as_basic_value_enum())
            .into_int_value();
        let one = i64_type.const_int(1, false);
        let total = self
            .builder
            .build_int_add(len, one, "case_total")
            .map_err(|e| {
                CompileError::new(&format!("LLVM ICE: {:?}", e), 0, 0, "", ErrorCode::E0009)
            })?;
        let malloc_call = self
            .builder
            .build_call(malloc_fn, &[total.into()], "case_malloc")
            .map_err(|e| {
                CompileError::new(&format!("LLVM ICE: {:?}", e), 0, 0, "", ErrorCode::E0009)
            })?;
        let buf = malloc_call
            .try_as_basic_value()
            .basic()
            .unwrap_or_else(|| ptr_type.const_null().as_basic_value_enum())
            .into_pointer_value();
        let current_fn = ice_opt(self.current_function, "missing")?;
        let loop_bb = self.context.append_basic_block(current_fn, "case_loop");
        let body_bb = self.context.append_basic_block(current_fn, "case_body");
        let done_bb = self.context.append_basic_block(current_fn, "case_done");
        let i_ptr = self.builder.build_alloca(i64_type, "case_i").map_err(|e| {
            CompileError::new(&format!("LLVM ICE: {:?}", e), 0, 0, "", ErrorCode::E0009)
        })?;
        self.builder
            .build_store(i_ptr, i64_type.const_int(0, false))
            .map_err(|e| {
                CompileError::new(&format!("LLVM ICE: {:?}", e), 0, 0, "", ErrorCode::E0009)
            })?;
        self.builder
            .build_unconditional_branch(loop_bb)
            .map_err(|e| {
                CompileError::new(&format!("LLVM ICE: {:?}", e), 0, 0, "", ErrorCode::E0009)
            })?;
        self.builder.position_at_end(loop_bb);
        let i_val = self
            .builder
            .build_load(i64_type, i_ptr, "case_i_load")
            .map_err(|e| {
                CompileError::new(&format!("LLVM ICE: {:?}", e), 0, 0, "", ErrorCode::E0009)
            })?
            .into_int_value();
        let cond = self
            .builder
            .build_int_compare(inkwell::IntPredicate::SLT, i_val, len, "case_cond")
            .map_err(|e| {
                CompileError::new(&format!("LLVM ICE: {:?}", e), 0, 0, "", ErrorCode::E0009)
            })?;
        self.builder
            .build_conditional_branch(cond, body_bb, done_bb)
            .map_err(|e| {
                CompileError::new(&format!("LLVM ICE: {:?}", e), 0, 0, "", ErrorCode::E0009)
            })?;
        self.builder.position_at_end(body_bb);
        let src_ptr = unsafe {
            self.builder
                .build_gep(
                    self.context.i8_type(),
                    s.into_pointer_value(),
                    &[i_val],
                    "case_src",
                )
                .map_err(|e| {
                    CompileError::new(&format!("LLVM ICE: {:?}", e), 0, 0, "", ErrorCode::E0009)
                })?
        };
        let src_byte = self
            .builder
            .build_load(i8_type, src_ptr, "case_src_byte")
            .map_err(|e| {
                CompileError::new(&format!("LLVM ICE: {:?}", e), 0, 0, "", ErrorCode::E0009)
            })?;
        let src_char = self
            .builder
            .build_int_z_extend(src_byte.into_int_value(), i32_type, "case_src_char")
            .map_err(|e| {
                CompileError::new(&format!("LLVM ICE: {:?}", e), 0, 0, "", ErrorCode::E0009)
            })?;
        let ctype_call = self
            .builder
            .build_call(ctype_fn, &[src_char.into()], "case_ctype")
            .map_err(|e| {
                CompileError::new(&format!("LLVM ICE: {:?}", e), 0, 0, "", ErrorCode::E0009)
            })?;
        let converted = ctype_call
            .try_as_basic_value()
            .basic()
            .unwrap_or_else(|| i32_type.const_int(0, false).as_basic_value_enum())
            .into_int_value();
        let dst_ptr = unsafe {
            self.builder
                .build_gep(self.context.i8_type(), buf, &[i_val], "case_dst")
                .map_err(|e| {
                    CompileError::new(&format!("LLVM ICE: {:?}", e), 0, 0, "", ErrorCode::E0009)
                })?
        };
        let converted_byte = self
            .builder
            .build_int_truncate(converted, i8_type, "case_byte")
            .map_err(|e| {
                CompileError::new(&format!("LLVM ICE: {:?}", e), 0, 0, "", ErrorCode::E0009)
            })?;
        self.builder
            .build_store(dst_ptr, converted_byte)
            .map_err(|e| {
                CompileError::new(&format!("LLVM ICE: {:?}", e), 0, 0, "", ErrorCode::E0009)
            })?;
        let next_i = self
            .builder
            .build_int_add(i_val, one, "case_next")
            .map_err(|e| {
                CompileError::new(&format!("LLVM ICE: {:?}", e), 0, 0, "", ErrorCode::E0009)
            })?;
        self.builder.build_store(i_ptr, next_i).map_err(|e| {
            CompileError::new(&format!("LLVM ICE: {:?}", e), 0, 0, "", ErrorCode::E0009)
        })?;
        self.builder
            .build_unconditional_branch(loop_bb)
            .map_err(|e| {
                CompileError::new(&format!("LLVM ICE: {:?}", e), 0, 0, "", ErrorCode::E0009)
            })?;
        self.builder.position_at_end(done_bb);
        let null_pos = unsafe {
            self.builder
                .build_gep(self.context.i8_type(), buf, &[len], "case_null")
                .map_err(|e| {
                    CompileError::new(&format!("LLVM ICE: {:?}", e), 0, 0, "", ErrorCode::E0009)
                })?
        };
        self.builder
            .build_store(null_pos, i8_type.const_int(0, false))
            .map_err(|e| {
                CompileError::new(&format!("LLVM ICE: {:?}", e), 0, 0, "", ErrorCode::E0009)
            })?;
        Ok(buf.as_basic_value_enum())
    }

    fn impl_string_substring(&self, args: &[BasicValueEnum<'ctx>]) -> Result<BasicValueEnum<'ctx>> {
        if args.len() < 3 {
            return Ok(self
                .context
                .ptr_type(AddressSpace::default())
                .const_null()
                .as_basic_value_enum());
        }
        let s = args[0];
        let start = args[1].into_int_value();
        let length = args[2].into_int_value();
        let i64_type = self.context.i64_type();
        let i8_type = self.context.i8_type();
        let ptr_type = self.context.ptr_type(AddressSpace::default());
        let malloc_fn = ice_opt(self.module.get_function("malloc"), "missing")?;
        let one = i64_type.const_int(1, false);
        let total = self
            .builder
            .build_int_add(length, one, "sub_total")
            .map_err(|e| {
                CompileError::new(&format!("LLVM ICE: {:?}", e), 0, 0, "", ErrorCode::E0009)
            })?;
        let malloc_call = self
            .builder
            .build_call(malloc_fn, &[total.into()], "sub_malloc")
            .map_err(|e| {
                CompileError::new(&format!("LLVM ICE: {:?}", e), 0, 0, "", ErrorCode::E0009)
            })?;
        let buf = malloc_call
            .try_as_basic_value()
            .basic()
            .unwrap_or_else(|| ptr_type.const_null().as_basic_value_enum())
            .into_pointer_value();
        let current_fn = ice_opt(self.current_function, "missing")?;
        let loop_bb = self.context.append_basic_block(current_fn, "sub_loop");
        let body_bb = self.context.append_basic_block(current_fn, "sub_body");
        let done_bb = self.context.append_basic_block(current_fn, "sub_done");
        let i_ptr = self.builder.build_alloca(i64_type, "sub_i").map_err(|e| {
            CompileError::new(&format!("LLVM ICE: {:?}", e), 0, 0, "", ErrorCode::E0009)
        })?;
        self.builder
            .build_store(i_ptr, i64_type.const_int(0, false))
            .map_err(|e| {
                CompileError::new(&format!("LLVM ICE: {:?}", e), 0, 0, "", ErrorCode::E0009)
            })?;
        self.builder
            .build_unconditional_branch(loop_bb)
            .map_err(|e| {
                CompileError::new(&format!("LLVM ICE: {:?}", e), 0, 0, "", ErrorCode::E0009)
            })?;
        self.builder.position_at_end(loop_bb);
        let i_val = self
            .builder
            .build_load(i64_type, i_ptr, "sub_i_load")
            .map_err(|e| {
                CompileError::new(&format!("LLVM ICE: {:?}", e), 0, 0, "", ErrorCode::E0009)
            })?
            .into_int_value();
        let cond = self
            .builder
            .build_int_compare(inkwell::IntPredicate::SLT, i_val, length, "sub_cond")
            .map_err(|e| {
                CompileError::new(&format!("LLVM ICE: {:?}", e), 0, 0, "", ErrorCode::E0009)
            })?;
        self.builder
            .build_conditional_branch(cond, body_bb, done_bb)
            .map_err(|e| {
                CompileError::new(&format!("LLVM ICE: {:?}", e), 0, 0, "", ErrorCode::E0009)
            })?;
        self.builder.position_at_end(body_bb);
        let offset = self
            .builder
            .build_int_add(start, i_val, "sub_offset")
            .map_err(|e| {
                CompileError::new(&format!("LLVM ICE: {:?}", e), 0, 0, "", ErrorCode::E0009)
            })?;
        let src_ptr = unsafe {
            self.builder
                .build_gep(
                    self.context.i8_type(),
                    s.into_pointer_value(),
                    &[offset],
                    "sub_src",
                )
                .map_err(|e| {
                    CompileError::new(&format!("LLVM ICE: {:?}", e), 0, 0, "", ErrorCode::E0009)
                })?
        };
        let src_byte = self
            .builder
            .build_load(i8_type, src_ptr, "sub_src_byte")
            .map_err(|e| {
                CompileError::new(&format!("LLVM ICE: {:?}", e), 0, 0, "", ErrorCode::E0009)
            })?;
        let dst_ptr = unsafe {
            self.builder
                .build_gep(self.context.i8_type(), buf, &[i_val], "sub_dst")
                .map_err(|e| {
                    CompileError::new(&format!("LLVM ICE: {:?}", e), 0, 0, "", ErrorCode::E0009)
                })?
        };
        self.builder.build_store(dst_ptr, src_byte).map_err(|e| {
            CompileError::new(&format!("LLVM ICE: {:?}", e), 0, 0, "", ErrorCode::E0009)
        })?;
        let next_i = self
            .builder
            .build_int_add(i_val, one, "sub_next")
            .map_err(|e| {
                CompileError::new(&format!("LLVM ICE: {:?}", e), 0, 0, "", ErrorCode::E0009)
            })?;
        self.builder.build_store(i_ptr, next_i).map_err(|e| {
            CompileError::new(&format!("LLVM ICE: {:?}", e), 0, 0, "", ErrorCode::E0009)
        })?;
        self.builder
            .build_unconditional_branch(loop_bb)
            .map_err(|e| {
                CompileError::new(&format!("LLVM ICE: {:?}", e), 0, 0, "", ErrorCode::E0009)
            })?;
        self.builder.position_at_end(done_bb);
        let null_pos = unsafe {
            self.builder
                .build_gep(self.context.i8_type(), buf, &[length], "sub_null")
                .map_err(|e| {
                    CompileError::new(&format!("LLVM ICE: {:?}", e), 0, 0, "", ErrorCode::E0009)
                })?
        };
        self.builder
            .build_store(null_pos, i8_type.const_int(0, false))
            .map_err(|e| {
                CompileError::new(&format!("LLVM ICE: {:?}", e), 0, 0, "", ErrorCode::E0009)
            })?;
        Ok(buf.as_basic_value_enum())
    }
}
