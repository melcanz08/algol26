#![allow(dead_code)]
#![allow(unused_variables)]

// src/backends/llvm_codegen/mod.rs

use crate::common::diagnostics::{CompileError, ErrorCode, Result};
use crate::common::types::Type;
use crate::ir::semantic_ir::{SemanticFunction, SemanticProgram};
use inkwell::builder::Builder;
use inkwell::context::Context;
use inkwell::module::Module;
use inkwell::types::{BasicMetadataTypeEnum, BasicTypeEnum};
use inkwell::values::{FunctionValue, PointerValue};
use inkwell::AddressSpace;
use std::collections::HashMap;

mod types;
mod effects;
mod builtins;
mod value;
mod binop;
mod instruction;
mod terminator;

fn ice_opt<T>(opt: Option<T>, msg: &str) -> Result<T> {
    opt.ok_or_else(|| CompileError::simple(msg, 0, 0, "", ErrorCode::E0009))
}

pub struct IRCodeGen<'ctx> {
    pub context: &'ctx Context,
    pub module: Module<'ctx>,
    pub(super) builder: Builder<'ctx>,
    pub(super) variables: HashMap<String, PointerValue<'ctx>>,
    pub(super) var_types: HashMap<String, Type>,
    pub(super) functions: HashMap<String, FunctionValue<'ctx>>,
    pub(super) current_function: Option<FunctionValue<'ctx>>,
    pub(super) blocks: HashMap<usize, inkwell::basic_block::BasicBlock<'ctx>>,
    pub(super) list_arrays: HashMap<String, PointerValue<'ctx>>,
    pub(super) list_array_types: HashMap<String, BasicTypeEnum<'ctx>>,
    pub(super) list_lengths: HashMap<String, usize>,
    pub(super) iterator_arrays: HashMap<String, PointerValue<'ctx>>,
    pub(super) iterator_array_types: HashMap<String, BasicTypeEnum<'ctx>>,
    pub(super) iterator_indices: HashMap<String, PointerValue<'ctx>>,
    pub(super) iterator_lengths: HashMap<String, usize>,
}

/// Map an IR-level `Math.*` function name to the corresponding name
/// registered in the LLVM module. Returns `None` for non-Math names.
///
/// The IR uses `Math.sqrt`, `Math.pow`, etc. The LLVM module registers
/// the C library names (`sqrt`, `pow`, `fabs`, ...). This bridge lets
/// the codegen find them.
fn resolve_math_name(ir_name: &str) -> Option<&'static str> {
    match ir_name {
        "Math.sqrt" => Some("sqrt"),
        "Math.pow" => Some("pow"),
        "Math.sin" => Some("sin"),
        "Math.cos" => Some("cos"),
        "Math.tan" => Some("tan"),
        "Math.exp" => Some("exp"),
        "Math.log" => Some("log"),
        "Math.floor" => Some("floor"),
        "Math.ceil" => Some("ceil"),
        "Math.abs" => Some("fabs"),
        _ => None,
    }
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
            CompileError::simple(
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
            let bb = self
                .context
                .append_basic_block(function, &format!("blk_{}", block.id));
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
            self.var_types
                .insert(param_name.clone(), param_type.clone());
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

    pub(super) fn create_entry_alloca(&self, name: &str, ty: &Type) -> PointerValue<'ctx> {
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
}
