#![allow(dead_code)]

// src/ir_codegen.rs

#![allow(unused_variables)]
#![allow(unused_imports)]

// ALGOL26 LLVM Backend that consumes IR directly
// This replaces the AST-based codegen

use std::collections::HashMap;
use inkwell::context::Context;
use inkwell::builder::Builder;
use inkwell::module::Module;
use inkwell::values::{PointerValue, BasicValue, BasicValueEnum, FunctionValue, IntValue};
use inkwell::FloatPredicate;
use inkwell::AddressSpace;
use inkwell::types::{BasicType, BasicTypeEnum, BasicMetadataTypeEnum};
use crate::ir::{IRProgram, IRFunction, IRBlock, IRInstruction, IRValue, IRConstant, IRType, IRBinOp};
use crate::diagnostics::{CompileError, ErrorCode, Result};

pub struct IRCodeGen<'ctx> {
    pub context: &'ctx Context,
    pub module: Module<'ctx>,
    builder: Builder<'ctx>,
    variables: HashMap<String, PointerValue<'ctx>>,
    functions: HashMap<String, FunctionValue<'ctx>>,
    current_function: Option<FunctionValue<'ctx>>,
    blocks: HashMap<usize, inkwell::basic_block::BasicBlock<'ctx>>,
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
        }
    }
    
    pub fn compile(&mut self, ir_program: &IRProgram) -> Result<()> {
        self.register_stdlib();
        
        // First pass: declare all functions
        for func in &ir_program.functions {
            let return_type = self.map_ir_type(&func.return_type);
            let param_types: Vec<BasicMetadataTypeEnum> = func.params.iter()
                .map(|(_, t)| self.map_ir_type(t).into())
                .collect();
            
            let fn_type = match func.return_type {
                IRType::Void => self.context.void_type().fn_type(&param_types, false),
                IRType::Int => self.context.i32_type().fn_type(&param_types, false),
                _ => self.context.f64_type().fn_type(&param_types, false),
            };
            
            let function = self.module.add_function(&func.name, fn_type, None);
            self.functions.insert(func.name.clone(), function);
        }
        
        // Second pass: compile function bodies
        for func in &ir_program.functions {
            self.compile_function(func)?;
        }
        
        Ok(())
    }
    
    fn map_ir_type(&self, ir_type: &IRType) -> BasicTypeEnum<'ctx> {
        match ir_type {
            IRType::Int => self.context.i64_type().as_basic_type_enum(),
            IRType::Float => self.context.f64_type().as_basic_type_enum(),
            IRType::String => self.context.ptr_type(AddressSpace::default()).as_basic_type_enum(),
            IRType::Bool => self.context.bool_type().as_basic_type_enum(),
            IRType::Void => self.context.f64_type().as_basic_type_enum(),
            _ => self.context.f64_type().as_basic_type_enum(),
        }
    }
    
    fn register_stdlib(&mut self) {
        let i32_type = self.context.i32_type();
        let i8_ptr_type = self.context.ptr_type(AddressSpace::default());
        let printf_type = i32_type.fn_type(&[i8_ptr_type.into()], true);
        let printf = self.module.add_function("printf", printf_type, None);
        self.functions.insert("printf".to_string(), printf);
        
        // Math functions
        let f64_type = self.context.f64_type();
        
        let sqrt_fn = self.module.add_function("sqrt", f64_type.fn_type(&[f64_type.into()], false), None);
        self.functions.insert("Math.sqrt".to_string(), sqrt_fn);
        
        let pow_fn = self.module.add_function("pow", f64_type.fn_type(&[f64_type.into(), f64_type.into()], false), None);
        self.functions.insert("Math.pow".to_string(), pow_fn);
        
        let sin_fn = self.module.add_function("sin", f64_type.fn_type(&[f64_type.into()], false), None);
        self.functions.insert("Math.sin".to_string(), sin_fn);
        
        let cos_fn = self.module.add_function("cos", f64_type.fn_type(&[f64_type.into()], false), None);
        self.functions.insert("Math.cos".to_string(), cos_fn);
        
        let abs_fn = self.module.add_function("fabs", f64_type.fn_type(&[f64_type.into()], false), None);
        self.functions.insert("Math.abs".to_string(), abs_fn);
    }
    
    fn compile_function(&mut self, func: &IRFunction) -> Result<()> {
        let function = self.functions.get(&func.name).unwrap().clone();
        self.current_function = Some(function);
        self.variables.clear();
        self.blocks.clear();
        
        // Create blocks
        for block in &func.blocks {
            let bb = self.context.append_basic_block(function, &format!("block_{}", block.id));
            self.blocks.insert(block.id, bb);
        }
        
        // Set entry block
        if let Some(first_block) = func.blocks.first() {
            if let Some(entry_bb) = self.blocks.get(&first_block.id) {
                self.builder.position_at_end(*entry_bb);
            }
        }
        
        // Allocate local variables
        for (name, type_, _mutable) in &func.local_vars {
            let alloca = self.create_alloca(name, type_);
            self.variables.insert(name.clone(), alloca);
        }
        
        // Compile each block's instructions
        for block in &func.blocks {
            if let Some(bb) = self.blocks.get(&block.id) {
                self.builder.position_at_end(*bb);
                for instr in &block.instructions {
                    self.compile_instruction(instr)?;
                }
            }
        }
        
        // Add return at end if needed
        if func.return_type == IRType::Void {
            self.builder.build_return(None).unwrap();
        } else {
            let zero = self.context.f64_type().const_float(0.0);
            self.builder.build_return(Some(&zero)).unwrap();
        }
        
        self.current_function = None;
        Ok(())
    }
    
    fn create_alloca(&self, name: &str, type_: &IRType) -> PointerValue<'ctx> {
        let builder = self.context.create_builder();
        let entry = self.builder.get_insert_block().unwrap().get_parent().unwrap().get_first_basic_block().unwrap();
        match entry.get_first_instruction() {
            Some(first) => builder.position_before(&first),
            None => builder.position_at_end(entry),
        }
        let alloca_type = self.map_ir_type(type_);
        builder.build_alloca(alloca_type, name).unwrap()
    }
    
    fn compile_instruction(&mut self, instr: &IRInstruction) -> Result<()> {
        match instr {
            IRInstruction::Alloca { name, type_, mutable } => {
                let alloca = self.create_alloca(name, type_);
                self.variables.insert(name.clone(), alloca);
                let _ = mutable;
            }
            IRInstruction::Print { value } => {
                let val = self.compile_value(value)?;
                let printf_func = self.functions.get("printf").unwrap().clone();
                
                if val.is_float_value() {
                    let format = self.builder.build_global_string_ptr("%.1f\n", "fmt_float").unwrap();
                    self.builder.build_direct_call(printf_func, &[format.as_pointer_value().into(), val.into()], "printf_call").unwrap();
                } else if val.is_int_value() {
                    let format = self.builder.build_global_string_ptr("%lld\n", "fmt_int").unwrap();
                    self.builder.build_direct_call(printf_func, &[format.as_pointer_value().into(), val.into()], "printf_call").unwrap();
                } else {
                    let format = self.builder.build_global_string_ptr("%s\n", "fmt_str").unwrap();
                    self.builder.build_direct_call(printf_func, &[format.as_pointer_value().into(), val.into()], "printf_call").unwrap();
                }
            }
            IRInstruction::Store { target, value } => {
                let val = self.compile_value(value)?;
                if let Some(ptr) = self.variables.get(target) {
                    self.builder.build_store(*ptr, val).unwrap();
                }
            }
            IRInstruction::BinaryOp { result, op, left, right } => {
                let l = self.compile_value(left)?;
                let r = self.compile_value(right)?;
                let val = self.compile_binary_op(op, &l, &r)?;
                
                // Store result in a temporary variable
                let alloca = self.create_alloca(result, &IRType::Float);
                self.builder.build_store(alloca, val).unwrap();
                self.variables.insert(result.clone(), alloca);
            }
            IRInstruction::Return { value } => {
                match value {
                    Some(v) => {
                        let val = self.compile_value(v)?;
                        self.builder.build_return(Some(&val)).unwrap();
                    }
                    None => {
                        self.builder.build_return(None).unwrap();
                    }
                }
            }
            IRInstruction::Branch { condition, then_block, else_block } => {
                let cond = self.compile_value(condition)?;
                let cond_int = if cond.is_float_value() {
                    let zero = self.context.f64_type().const_float(0.0);
                    self.builder.build_float_compare(FloatPredicate::ONE, cond.into_float_value(), zero, "cond").unwrap()
                } else {
                    cond.into_int_value()
                };
                
                let then_bb = *self.blocks.get(then_block).unwrap();
                let else_bb = *self.blocks.get(else_block).unwrap();
                self.builder.build_conditional_branch(cond_int, then_bb, else_bb).unwrap();
            }
            IRInstruction::Jump { block } => {
                let target = *self.blocks.get(block).unwrap();
                self.builder.build_unconditional_branch(target).unwrap();
            }
            IRInstruction::Call { result, function, args } => {
                let func = self.functions.get(function).cloned().ok_or_else(|| {
                    CompileError::new(&format!("Undefined function '{}'", function), 0, 0, "", ErrorCode::E0004)
                })?;
                
                let arg_vals: Vec<BasicValueEnum> = args.iter()
                    .map(|a| self.compile_value(a))
                    .collect::<Result<Vec<_>>>()?;
                
                let arg_metadata: Vec<inkwell::values::BasicMetadataValueEnum> = arg_vals
                    .iter()
                    .map(|v| (*v).into())
                    .collect();
                
                let call = self.builder.build_direct_call(func, &arg_metadata, "calltmp").unwrap();
                
                if let Some(result_name) = result {
                    if let Some(value) = call.try_as_basic_value().basic() {
                        let alloca = self.create_alloca(result_name, &IRType::Float);
                        self.builder.build_store(alloca, value).unwrap();
                        self.variables.insert(result_name.clone(), alloca);
                    }
                }
            }
            IRInstruction::ArrayAccess { result, array, index } => {
                // Simplified: just return 0.0 for now
                let alloca = self.create_alloca(result, &IRType::Float);
                let zero = self.context.f64_type().const_float(0.0);
                self.builder.build_store(alloca, zero).unwrap();
                self.variables.insert(result.clone(), alloca);
                let _ = (array, index);
            }
            IRInstruction::Nop => {}
            IRInstruction::BoundsCheck { .. } => {}
            IRInstruction::Label(_) => {}
            IRInstruction::Load { result, source } => {
                if let Some(ptr) = self.variables.get(source) {
                    let loaded = self.builder.build_load(self.context.f64_type(), *ptr, "load").unwrap();
                    let alloca = self.create_alloca(result, &IRType::Float);
                    self.builder.build_store(alloca, loaded).unwrap();
                    self.variables.insert(result.clone(), alloca);
                }
            }
        }
        Ok(())
    }
    
    fn compile_value(&self, value: &IRValue) -> Result<BasicValueEnum<'ctx>> {
        match value {
            IRValue::Constant(c) => Ok(self.compile_constant(c)),
            IRValue::Variable(name) => {
                if let Some(ptr) = self.variables.get(name) {
                    let loaded = self.builder.build_load(self.context.f64_type(), *ptr, name).unwrap();
                    Ok(loaded.as_basic_value_enum())
                } else {
                    Ok(self.context.f64_type().const_float(0.0).as_basic_value_enum())
                }
            }
            IRValue::Temporary(_) => Ok(self.context.f64_type().const_float(0.0).as_basic_value_enum()),
        }
    }
    
    fn compile_constant(&self, constant: &IRConstant) -> BasicValueEnum<'ctx> {
        match constant {
            IRConstant::Int(i) => self.context.i64_type().const_int(*i as u64, true).as_basic_value_enum(),
            IRConstant::Float(f) => self.context.f64_type().const_float(*f).as_basic_value_enum(),
            IRConstant::String(s) => {
                let global = self.builder.build_global_string_ptr(s, "str").unwrap();
                global.as_pointer_value().as_basic_value_enum()
            }
            IRConstant::Bool(b) => self.context.bool_type().const_int(*b as u64, false).as_basic_value_enum(),
            IRConstant::List(_) => self.context.f64_type().const_float(0.0).as_basic_value_enum(),
        }
    }
    
    fn compile_binary_op(&self, op: &IRBinOp, left: &BasicValueEnum<'ctx>, right: &BasicValueEnum<'ctx>) -> Result<BasicValueEnum<'ctx>> {
        match op {
            IRBinOp::Add => {
                let result = self.builder.build_float_add(left.into_float_value(), right.into_float_value(), "addtmp").unwrap();
                Ok(result.as_basic_value_enum())
            }
            IRBinOp::Subtract => {
                let result = self.builder.build_float_sub(left.into_float_value(), right.into_float_value(), "subtmp").unwrap();
                Ok(result.as_basic_value_enum())
            }
            IRBinOp::Multiply => {
                let result = self.builder.build_float_mul(left.into_float_value(), right.into_float_value(), "multmp").unwrap();
                Ok(result.as_basic_value_enum())
            }
            IRBinOp::Divide => {
                let result = self.builder.build_float_div(left.into_float_value(), right.into_float_value(), "divtmp").unwrap();
                Ok(result.as_basic_value_enum())
            }
            IRBinOp::Greater => {
                let cmp = self.builder.build_float_compare(FloatPredicate::OGT, left.into_float_value(), right.into_float_value(), "cmptmp").unwrap();
                Ok(cmp.as_basic_value_enum())
            }
            IRBinOp::Less => {
                let cmp = self.builder.build_float_compare(FloatPredicate::OLT, left.into_float_value(), right.into_float_value(), "cmptmp").unwrap();
                Ok(cmp.as_basic_value_enum())
            }
            IRBinOp::GreaterEqual => {
                let cmp = self.builder.build_float_compare(FloatPredicate::OGE, left.into_float_value(), right.into_float_value(), "cmptmp").unwrap();
                Ok(cmp.as_basic_value_enum())
            }
            IRBinOp::LessEqual => {
                let cmp = self.builder.build_float_compare(FloatPredicate::OLE, left.into_float_value(), right.into_float_value(), "cmptmp").unwrap();
                Ok(cmp.as_basic_value_enum())
            }
            IRBinOp::Equal => {
                let cmp = self.builder.build_float_compare(FloatPredicate::OEQ, left.into_float_value(), right.into_float_value(), "cmptmp").unwrap();
                Ok(cmp.as_basic_value_enum())
            }
            IRBinOp::NotEqual => {
                let cmp = self.builder.build_float_compare(FloatPredicate::ONE, left.into_float_value(), right.into_float_value(), "cmptmp").unwrap();
                Ok(cmp.as_basic_value_enum())
            }
            IRBinOp::And => {
                let l = left.into_int_value();
                let r = right.into_int_value();
                let result = self.builder.build_and(l, r, "andtmp").unwrap();
                Ok(result.as_basic_value_enum())
            }
            IRBinOp::Or => {
                let l = left.into_int_value();
                let r = right.into_int_value();
                let result = self.builder.build_or(l, r, "ortmp").unwrap();
                Ok(result.as_basic_value_enum())
            }
        }
    }
}
