// algol26/src/codegen.rs

use std::collections::HashMap;
use inkwell::context::Context;
use inkwell::builder::Builder;
use inkwell::module::Module;
use inkwell::values::{PointerValue, BasicValue, BasicValueEnum, FunctionValue, IntValue, FloatValue};
use inkwell::FloatPredicate;
use inkwell::IntPredicate;
use inkwell::AddressSpace;
use inkwell::types::{BasicType, BasicTypeEnum, BasicMetadataTypeEnum};
use crate::ast::{Expr, FunctionDecl, Stmt, BinOp};

#[derive(Debug, Clone, Copy)]
struct LoopContext<'ctx> {
    break_block: inkwell::basic_block::BasicBlock<'ctx>,
    continue_block: inkwell::basic_block::BasicBlock<'ctx>,
}
#[allow(unused_imports)]
use crate::ast::{MatchCase, Pattern};
use crate::diagnostics::{CompileError, ErrorCode, Result};

#[allow(dead_code)]
#[derive(Clone, Debug, PartialEq)]
enum VarType {
    Float,
    Int,
    String,
    Bool,
    List,
}

#[allow(dead_code)]
#[derive(Clone, Debug, PartialEq)]
enum Ownership {
    Owned,
    Borrowed,
    Moved,
}

pub struct CodeGen<'ctx> {
    pub context: &'ctx Context,
    pub module: Module<'ctx>,
    pub builder: Builder<'ctx>,
    loop_stack: Vec<LoopContext<'ctx>>,
    variables: HashMap<String, (PointerValue<'ctx>, VarType, Ownership)>,
    lists: HashMap<String, Vec<Expr>>,
    functions: HashMap<String, FunctionValue<'ctx>>,
    pub current_function: Option<FunctionValue<'ctx>>,
    scope_stack: Vec<HashMap<String, (PointerValue<'ctx>, VarType, Ownership)>>,
    defer_stack: Vec<Vec<Stmt>>,
}

impl<'ctx> CodeGen<'ctx> {
    pub fn new(context: &'ctx Context, module_name: &str) -> Self {
        let module = context.create_module(module_name);
        let builder = context.create_builder();
        CodeGen {
            context,
            module,
            builder,
            variables: HashMap::new(),
            lists: HashMap::new(),
            functions: HashMap::new(),
            current_function: None,
            loop_stack: Vec::new(),
            scope_stack: vec![HashMap::new()],
            defer_stack: vec![Vec::new()],
    }
    }

    fn push_scope(&mut self) {
        self.scope_stack.push(HashMap::new());
        self.defer_stack.push(Vec::new());
    }

    fn pop_scope(&mut self) {
        self.defer_stack.pop();
        self.scope_stack.pop();
    }

    fn mark_moved(&mut self, name: &str) {
        if let Some(scope) = self.scope_stack.last_mut() {
            if let Some((_, _, ownership)) = scope.get_mut(name) {
                *ownership = Ownership::Moved;
            }
        }
        if let Some((_, _, ownership)) = self.variables.get_mut(name) {
            *ownership = Ownership::Moved;
        }
    }

    fn declare_variable(&mut self, name: &str, ptr: PointerValue<'ctx>, var_type: VarType) {
        if let Some(scope) = self.scope_stack.last_mut() {
            scope.insert(name.to_string(), (ptr, var_type.clone(), Ownership::Owned));
        }
        self.variables.insert(name.to_string(), (ptr, var_type, Ownership::Owned));
    }

    fn lookup_variable(&self, name: &str) -> Option<(PointerValue<'ctx>, VarType)> {
        for scope in self.scope_stack.iter().rev() {
            if let Some((ptr, var_type, ownership)) = scope.get(name) {
                // Don't return moved variables
                if *ownership == Ownership::Moved {
                    return None;
                }
                return Some((*ptr, var_type.clone()));
            }
        }
        self.variables.get(name).map(|(ptr, var_type, ownership)| {
            if *ownership == Ownership::Moved {
                None
            } else {
                Some((*ptr, var_type.clone()))
            }
        }).flatten()
    }

    #[allow(dead_code)]
    #[allow(dead_code)]
    pub fn compile_ir(&mut self, ir_program: &crate::ir::IRProgram) -> Result<()> {
        self.register_stdlib();
        
        // Declare all IR functions in LLVM
        for ir_func in &ir_program.functions {
            let _return_type = match ir_func.return_type {
                crate::ir::IRType::Int => self.context.i32_type().as_basic_type_enum(),
                crate::ir::IRType::Float => self.context.f64_type().as_basic_type_enum(),
                crate::ir::IRType::String => self.context.ptr_type(AddressSpace::default()).as_basic_type_enum(),
                crate::ir::IRType::Bool => self.context.bool_type().as_basic_type_enum(),
                _ => self.context.f64_type().as_basic_type_enum(),
            };
            
            let param_types: Vec<BasicMetadataTypeEnum> = ir_func.params.iter().map(|(_, t)| {
                match t {
                    crate::ir::IRType::Int => self.context.i64_type().as_basic_type_enum().into(),
                    crate::ir::IRType::Float => self.context.f64_type().as_basic_type_enum().into(),
                    crate::ir::IRType::String => self.context.ptr_type(AddressSpace::default()).as_basic_type_enum().into(),
                    crate::ir::IRType::Bool => self.context.bool_type().as_basic_type_enum().into(),
                    _ => self.context.f64_type().as_basic_type_enum().into(),
                }
            }).collect();
            
            let fn_type = match ir_func.return_type {
                crate::ir::IRType::Void => self.context.void_type().fn_type(&param_types, false),
                crate::ir::IRType::Int => self.context.i32_type().fn_type(&param_types, false),
                _ => self.context.f64_type().fn_type(&param_types, false),
            };
            
            let function = self.module.add_function(&ir_func.name, fn_type, None);
            self.functions.insert(ir_func.name.clone(), function);
        }
        
        Ok(())
    }

    pub fn compile_program(&mut self, functions: Vec<FunctionDecl>) -> Result<()> {
        self.register_stdlib();

        for func in &functions {
            if func.is_extern {
                // Declare external function without a body
                let return_type_str = func.return_type.as_deref().unwrap_or("void");
                let param_types: Vec<BasicMetadataTypeEnum> = func.params.iter().map(|(_, t)| {
                    self.get_type_from_string(t).into()
                }).collect();
                
                let fn_type = if return_type_str == "void" {
                    self.context.void_type().fn_type(&param_types, false)
                } else {
                    self.get_type_from_string(return_type_str).fn_type(&param_types, false)
                };
                
                let clean_name = func.name.trim_end_matches("()").to_string();
                let function = self.module.add_function(&clean_name, fn_type, None);
                self.functions.insert(clean_name, function);
            }
            
            if func.name == "main" {
                let i32_type = self.context.i32_type();
                let fn_type = i32_type.fn_type(&[], false);
                let clean_name = func.name.trim_end_matches("()").to_string();
                let function = self.module.add_function(&clean_name, fn_type, None);
                self.functions.insert(clean_name, function);
            } else {
                let return_type_str = func.return_type.as_deref().unwrap_or("void");
                
                let param_types: Vec<BasicMetadataTypeEnum> = func.params.iter().map(|(_, t)| {
                    self.get_type_from_string(t).into()
                }).collect();
                
                let fn_type = if return_type_str == "void" {
                    self.context.void_type().fn_type(&param_types, false)
                } else {
                    self.get_type_from_string(return_type_str).fn_type(&param_types, false)
                };
                
                let clean_name = func.name.trim_end_matches("()").to_string();
                let function = self.module.add_function(&clean_name, fn_type, None);
                self.functions.insert(clean_name, function);
            }
        }

        for func in &functions {
            if !func.is_extern {
                self.compile_function(&func)?;
            }
        }

        Ok(())
    }

    fn get_type_from_string(&self, type_str: &str) -> BasicTypeEnum<'ctx> {
        match type_str {
            "int" => self.context.i64_type().as_basic_type_enum(),
            "float" => self.context.f64_type().as_basic_type_enum(),
            "string" => self.context.ptr_type(AddressSpace::default()).as_basic_type_enum(),
            "bool" => self.context.bool_type().as_basic_type_enum(),
            _ => self.context.f64_type().as_basic_type_enum(),
        }
    }

    fn register_stdlib(&mut self) {
        // Register Math functions with their C names
        let f64_type = self.context.f64_type();
        
        // Register Math functions (use C library names)
        // Only add each function once to avoid duplicate symbols
        let sqrt_fn = self.module.add_function(
            "sqrt",
            f64_type.fn_type(&[f64_type.into()], false),
            None
        );
        self.functions.insert("Math.sqrt".to_string(), sqrt_fn);
        
        let pow_fn = self.module.add_function(
            "pow",
            f64_type.fn_type(&[f64_type.into(), f64_type.into()], false),
            None
        );
        self.functions.insert("Math.pow".to_string(), pow_fn);
        
        let sin_fn = self.module.add_function(
            "sin",
            f64_type.fn_type(&[f64_type.into()], false),
            None
        );
        self.functions.insert("Math.sin".to_string(), sin_fn);
        
        let cos_fn = self.module.add_function(
            "cos",
            f64_type.fn_type(&[f64_type.into()], false),
            None
        );
        self.functions.insert("Math.cos".to_string(), cos_fn);
        
        let abs_fn = self.module.add_function(
            "fabs",
            f64_type.fn_type(&[f64_type.into()], false),
            None
        );
        self.functions.insert("Math.abs".to_string(), abs_fn);
        
        let floor_fn = self.module.add_function(
            "floor",
            f64_type.fn_type(&[f64_type.into()], false),
            None
        );
        self.functions.insert("Math.floor".to_string(), floor_fn);
        
        let ceil_fn = self.module.add_function(
            "ceil",
            f64_type.fn_type(&[f64_type.into()], false),
            None
        );
        self.functions.insert("Math.ceil".to_string(), ceil_fn);
        
        let exp_fn = self.module.add_function(
            "exp",
            f64_type.fn_type(&[f64_type.into()], false),
            None
        );
        self.functions.insert("Math.exp".to_string(), exp_fn);
        
        let log_fn = self.module.add_function(
            "log",
            f64_type.fn_type(&[f64_type.into()], false),
            None
        );
        self.functions.insert("Math.log".to_string(), log_fn);
        
        let tan_fn = self.module.add_function(
            "tan",
            f64_type.fn_type(&[f64_type.into()], false),
            None
        );
        self.functions.insert("Math.tan".to_string(), tan_fn);
        
        // Register File I/O functions
        let ptr_type = self.context.ptr_type(AddressSpace::default());
        let i32_type = self.context.i32_type();
        let i64_type = self.context.i64_type();
        
        // fopen(filename, mode) -> ptr
        let fopen_fn = self.module.add_function(
            "fopen",
            ptr_type.fn_type(&[ptr_type.into(), ptr_type.into()], false),
            None
        );
        self.functions.insert("File.open".to_string(), fopen_fn);
        
        // fprintf(file, format, ...) -> int
        let fprintf_fn = self.module.add_function(
            "fprintf",
            i32_type.fn_type(&[ptr_type.into(), ptr_type.into()], true),
            None
        );
        self.functions.insert("File.write".to_string(), fprintf_fn);
        
        // fgets(str, n, stream) -> ptr
        let fgets_fn = self.module.add_function(
            "fgets",
            ptr_type.fn_type(&[ptr_type.into(), i32_type.into(), ptr_type.into()], false),
            None
        );
        self.functions.insert("File.read".to_string(), fgets_fn);

        
        // malloc for raw memory
        let malloc_raw = self.module.add_function(
            "malloc",
            ptr_type.fn_type(&[i64_type.into()], false),
            None
        );
        self.functions.insert("alloc".to_string(), malloc_raw);
        
        let free_raw = self.module.add_function(
            "free",
            self.context.void_type().fn_type(&[ptr_type.into()], false),
            None
        );
        self.functions.insert("free".to_string(), free_raw);
        
        // fputs(str, stream) -> int (for File.append)
        let fputs_fn = self.module.add_function(
            "fputs",
            i32_type.fn_type(&[ptr_type.into(), ptr_type.into()], false),
            None
        );
        self.functions.insert("File.append".to_string(), fputs_fn);
        
        // Register List functions (implemented as compiler builtins)
        // These will be handled specially in compile_expr
        self.functions.insert("List.length".to_string(), self.module.add_function(
            "list_length",
            i32_type.fn_type(&[ptr_type.into()], false),
            None
        ));
        self.functions.insert("List.sum".to_string(), self.module.add_function(
            "list_sum",
            ptr_type.fn_type(&[ptr_type.into()], false),
            None
        ));
        self.functions.insert("List.max".to_string(), self.module.add_function(
            "list_max",
            ptr_type.fn_type(&[ptr_type.into()], false),
            None
        ));
        self.functions.insert("List.min".to_string(), self.module.add_function(
            "list_min",
            ptr_type.fn_type(&[ptr_type.into()], false),
            None
        ));


        let i32_type = self.context.i32_type();
        let i8_ptr_type = self.context.ptr_type(AddressSpace::default());
        let printf_type = i32_type.fn_type(&[i8_ptr_type.into()], true);
        let printf = self.module.add_function("printf", printf_type, None);
        self.functions.insert("printf".to_string(), printf);


    }

    fn compile_function(&mut self, ast: &FunctionDecl) -> Result<()> {
        if ast.is_extern {
            // External functions are already declared, skip body compilation
            return Ok(());
        }

        let clean_name = ast.name.trim_end_matches("()").to_string();
        let function = self.functions.get(&clean_name).unwrap().clone();
        self.current_function = Some(function);
        
        let basic_block = self.context.append_basic_block(function, "entry");
        self.builder.position_at_end(basic_block);

        self.push_scope();

        for (i, (param_name, param_type)) in ast.params.iter().enumerate() {
            let param = function.get_nth_param(i as u32).unwrap();
            let alloca = self.create_entry_block_alloca(param_name, param_type);
            self.builder.build_store(alloca, param).unwrap();
            let var_type = match param_type.as_str() {
                "int" => VarType::Int,
                "float" => VarType::Float,
                "string" => VarType::String,
                "bool" => VarType::Bool,
                "list" => VarType::List,
                _ => VarType::Float,
            };
            self.declare_variable(param_name, alloca, var_type);
        }

        for stmt in &ast.body {
            self.compile_stmt(stmt)?;
        }

        if ast.name == "main" {
            let zero = self.context.i32_type().const_int(0, false);
            self.builder.build_return(Some(&zero)).unwrap();
        } else if ast.return_type.is_none() || ast.return_type.as_deref() == Some("void") {
            self.builder.build_return(None).unwrap();
        } else {
            let return_type_str = ast.return_type.as_deref().unwrap_or("float");
            let default_val = match return_type_str {
                "int" => self.context.i64_type().const_int(0, false).as_basic_value_enum(),
                "string" => self.context.ptr_type(AddressSpace::default()).const_null().as_basic_value_enum(),
                "bool" => self.context.bool_type().const_int(0, false).as_basic_value_enum(),
                _ => self.context.f64_type().const_float(0.0).as_basic_value_enum(),
            };
            self.builder.build_return(Some(&default_val)).unwrap();
        }

        self.pop_scope();
        self.current_function = None;
        Ok(())
    }

    fn compile_stmt(&mut self, stmt: &Stmt) -> Result<()> {
        match stmt {
            Stmt::VarDecl { name, value, type_annotation, mutable: _ } => {
                match value {
                    Expr::List(elements) => {
                        self.lists.insert(name.clone(), elements.clone());
                    }
                    Expr::String(s) => {
                        let global = self.builder.build_global_string_ptr(s, "str").unwrap();
                        let alloca = self.create_entry_block_alloca(name, "string");
                        self.builder.build_store(alloca, global).unwrap();
                        self.declare_variable(name, alloca, VarType::String);
                    }
                    Expr::FunctionCall { name: func_name, args: _ } if func_name.starts_with("String.") => {
                        // String function call returns a pointer
                        let value = self.compile_expr(value)?;
                        let alloca = self.create_entry_block_alloca(name, "string");
                        self.builder.build_store(alloca, value).unwrap();
                        self.declare_variable(name, alloca, VarType::String);
                    }
                    _ => {
                        let val = self.compile_expr(value)?;
                        let var_type = if val.is_int_value() {
                            if val.into_int_value().get_type().get_bit_width() == 1 {
                                VarType::Bool
                            } else {
                                VarType::Int
                            }
                        } else {
                            VarType::Float
                        };
                        let type_str = type_annotation.as_deref().unwrap_or_else(|| {
                            match var_type {
                                VarType::Bool => "bool",
                                VarType::Int => "int",
                                _ => "float",
                            }
                        });
                        let alloca = self.create_entry_block_alloca(name, type_str);
                        self.builder.build_store(alloca, val).unwrap();
                        self.declare_variable(name, alloca, var_type);
                    }
                }
            }
            Stmt::Assign { name, value } => {
                // Check if we're moving a variable
                if let Expr::Var(source_name) = value {
                    // This is a move operation
                    let val = self.compile_expr(value)?;
                    if let Some((ptr, _)) = self.lookup_variable(name) {
                        self.builder.build_store(ptr, val).unwrap();
                        // Mark source as moved
                        self.mark_moved(source_name);
                    } else {
                        return Err(CompileError::new(
                            &format!("Undefined variable '{}'", name),
                                0, 0, "",
                                ErrorCode::E0001
                            ));
                    }
                } else {
                    // Regular assignment
                    let val = self.compile_expr(value)?;
                    if let Some((ptr, _)) = self.lookup_variable(name) {
                        self.builder.build_store(ptr, val).unwrap();
                    } else {
                        return Err(CompileError::new(
                            &format!("Undefined variable '{}'", name),
                                0, 0, "",
                                ErrorCode::E0001
                            ));
                    }
                }
            }
            Stmt::Print { expr } => {
                match expr {
                    Expr::String(s) => {
                        let printf_func = self.functions.get("printf").unwrap().clone();
                        let format = self.builder.build_global_string_ptr("%s\n", "fmt_str").unwrap();
                        let str_val = self.builder.build_global_string_ptr(s, "str").unwrap();
                        self.builder.build_direct_call(
                            printf_func,
                            &[format.as_pointer_value().into(), str_val.as_pointer_value().into()],
                            "printf_call",
                        ).unwrap();
                    }
                    Expr::Var(name) => {
                        // Check if it's a string variable
                        if let Some((ptr, var_type)) = self.lookup_variable(name) {
                            let printf_func = self.functions.get("printf").unwrap().clone();
                            
                            match var_type {
                                VarType::String => {
                                    let format = self.builder.build_global_string_ptr("%s\n", "fmt_str").unwrap();
                                    let loaded = self.builder.build_load(
                                        self.context.ptr_type(AddressSpace::default()),
                                        ptr,
                                        name
                                    ).unwrap();
                                    self.builder.build_direct_call(
                                        printf_func,
                                        &[format.as_pointer_value().into(), loaded.into()],
                                        "printf_call",
                                    ).unwrap();
                                }
                                VarType::Int => {
                                    let format = self.builder.build_global_string_ptr("%lld\n", "fmt_int").unwrap();
                                    let loaded = self.builder.build_load(
                                        self.context.i64_type(),
                                        ptr,
                                        name
                                    ).unwrap();
                                    self.builder.build_direct_call(
                                        printf_func,
                                        &[format.as_pointer_value().into(), loaded.into()],
                                        "printf_call",
                                    ).unwrap();
                                }
                                _ => {
                                    let format = self.builder.build_global_string_ptr("%.1f\n", "fmt_float").unwrap();
                                    let loaded = self.builder.build_load(
                                        self.context.f64_type(),
                                        ptr,
                                        name
                                    ).unwrap();
                                    self.builder.build_direct_call(
                                        printf_func,
                                        &[format.as_pointer_value().into(), loaded.into()],
                                        "printf_call",
                                    ).unwrap();
                                }
                            }
                        }
                    }
                    _ => {
                        let val = self.compile_expr(expr)?;
                        let printf_func = self.functions.get("printf").unwrap().clone();
                        
                        if val.is_float_value() {
                            let format = self.builder.build_global_string_ptr("%.1f\n", "fmt_float").unwrap();
                            self.builder.build_direct_call(
                                printf_func,
                                &[format.as_pointer_value().into(), val.into()],
                                "printf_call",
                            ).unwrap();
                        } else if val.is_int_value() {
                            let format = self.builder.build_global_string_ptr("%lld\n", "fmt_int").unwrap();
                            self.builder.build_direct_call(
                                printf_func,
                                &[format.as_pointer_value().into(), val.into()],
                                "printf_call",
                            ).unwrap();
                        } else {
                            let format = self.builder.build_global_string_ptr("%s\n", "fmt_str").unwrap();
                            self.builder.build_direct_call(
                                printf_func,
                                &[format.as_pointer_value().into(), val.into()],
                                "printf_call",
                            ).unwrap();
                        }
                    }
                }
            }
            Stmt::If { condition, then_body, else_body } => {
                let cond_val = self.compile_expr(condition)?;
                // If it's a float, convert to i1; if already i1, use directly
                let cond_val = if cond_val.is_float_value() {
                    let zero = self.context.f64_type().const_float(0.0);
                    let cmp = self.builder.build_float_compare(
                        FloatPredicate::ONE,
                        cond_val.into_float_value(),
                        zero,
                        "ifcond"
                    ).unwrap();
                    cmp.as_basic_value_enum()
                } else {
                    cond_val
                };
                
                let parent_fn = self.builder.get_insert_block().unwrap().get_parent().unwrap();
                let then_bb = self.context.append_basic_block(parent_fn, "then");
                let else_bb = else_body.as_ref().map(|_| self.context.append_basic_block(parent_fn, "else"));
                let merge_bb = self.context.append_basic_block(parent_fn, "ifcont");

                if let Some(else_bb) = else_bb {
                    self.builder.build_conditional_branch(cond_val.into_int_value(), then_bb, else_bb).unwrap();
                } else {
                    self.builder.build_conditional_branch(cond_val.into_int_value(), then_bb, merge_bb).unwrap();
                }

                self.builder.position_at_end(then_bb);
                self.push_scope();
                for s in then_body {
                    self.compile_stmt(s)?;
                }
                self.pop_scope();
                self.builder.build_unconditional_branch(merge_bb).unwrap();

                if let (Some(else_bb), Some(else_body)) = (else_bb, else_body) {
                    self.builder.position_at_end(else_bb);
                    self.push_scope();
                    for s in else_body {
                        self.compile_stmt(s)?;
                    }
                    self.pop_scope();
                    self.builder.build_unconditional_branch(merge_bb).unwrap();
                }

                self.builder.position_at_end(merge_bb);
            }
            Stmt::For { var, iterable, body } => {
                let elements = match iterable {
                    Expr::List(elems) => elems.clone(),
                    Expr::Var(name) => {
                        if let Some(elems) = self.lists.get(name) {
                            elems.clone()
                        } else {
                            return Err(CompileError::new(
                                &format!("Undefined list variable '{}'", name),
                                    0, 0, "",
                                    ErrorCode::E0001
                                ));
                        }
                    }
                    _ => return Err(CompileError::new("For loop iterable must be a list or list variable", 0, 0, ""
                    , ErrorCode::E0001)),
                };

                let alloca = match self.lookup_variable(var) {
                    Some((ptr, _)) => ptr,
                    None => {
                        let p = self.create_entry_block_alloca(var, "float");
                        self.declare_variable(var, p, VarType::Float);
                        p
                    }
                };

                for elem in elements {
                    let val = self.compile_expr(&elem)?;
                    self.builder.build_store(alloca, val).unwrap();
                    self.push_scope();
                    for s in body {
                        self.compile_stmt(s)?;
                    }
                    self.pop_scope();
                }
            }
            Stmt::While { condition, body } => {
                let parent_fn = self.builder.get_insert_block().unwrap().get_parent().unwrap();
                let cond_bb = self.context.append_basic_block(parent_fn, "whilecond");
                let body_bb = self.context.append_basic_block(parent_fn, "whilebody");
                let merge_bb = self.context.append_basic_block(parent_fn, "whilecont");
                
                // Push loop context for break/continue
                self.loop_stack.push(LoopContext {
                    break_block: merge_bb,
                    continue_block: cond_bb,
                });

                self.builder.build_unconditional_branch(cond_bb).unwrap();
                self.builder.position_at_end(cond_bb);
                
                let cond_val = self.compile_expr(condition)?;
                // If it's a float, convert to i1; if already i1, use directly
                let cond_val = if cond_val.is_float_value() {
                    let zero = self.context.f64_type().const_float(0.0);
                    let cmp = self.builder.build_float_compare(
                        FloatPredicate::ONE,
                        cond_val.into_float_value(),
                        zero,
                        "whilecond"
                    ).unwrap();
                    cmp.as_basic_value_enum()
                } else {
                    cond_val
                };
                self.builder.build_conditional_branch(cond_val.into_int_value(), body_bb, merge_bb).unwrap();

                self.builder.position_at_end(body_bb);
                self.push_scope();
                for s in body {
                    self.compile_stmt(s)?;
                }
                self.pop_scope();
                self.builder.build_unconditional_branch(cond_bb).unwrap();

                self.builder.position_at_end(merge_bb);
                self.loop_stack.pop();
            }
            Stmt::Return { value } => {
                match value {
                    Some(expr) => {
                        let val = self.compile_expr(expr)?;
                        self.builder.build_return(Some(&val)).unwrap();
                    }
                    None => {
                        self.builder.build_return(None).unwrap();
                    }
                }
            }
            Stmt::Spawn { body } => {
                // Compile spawn body inline (sequential for now)
                // Future: emit thread creation code
                self.push_scope();
                for s in body {
                    self.compile_stmt(s)?;
                }
                self.pop_scope();
            }
            Stmt::Parallel { blocks } => {
                // Compile blocks sequentially (future: parallel)
                for block in blocks {
                    self.push_scope();
                    for s in block {
                        self.compile_stmt(s)?;
                    }
                    self.pop_scope();
                }
            }
            Stmt::ChannelDecl { name } => {
                // Create a simple channel variable (placeholder)
                let alloca = self.create_entry_block_alloca(name, "float");
                self.declare_variable(name, alloca, VarType::Float);
            }
            Stmt::Send { channel, value } => {
                // Send value to channel (store in channel variable)
                let val = self.compile_expr(value)?;
                if let Some((ptr, _)) = self.lookup_variable(channel) {
                    self.builder.build_store(ptr, val).unwrap();
                }
            }
            Stmt::Receive { channel, target } => {
                // Receive value from channel (load from channel variable)
                if let Some((ptr, _)) = self.lookup_variable(channel) {
                    let loaded = self.builder.build_load(self.context.f64_type(), ptr, channel).unwrap();
                    if !target.is_empty() {
                        if let Some((target_ptr, _)) = self.lookup_variable(target) {
                            self.builder.build_store(target_ptr, loaded).unwrap();
                        }
                    }
                }
            }
            Stmt::Match { value, cases } => {
                let _ = value;
                // For now, compile the first case body and bind variables
                if let Some(first_case) = cases.first() {
                    self.push_scope();
                    
                    // Bind pattern variables
                    match &first_case.pattern {
                        Pattern::Some(var) => {
                            let alloca = self.create_entry_block_alloca(var, "float");
                            self.declare_variable(var, alloca, VarType::Float);
                        }
                        Pattern::None => {}
                        Pattern::Ok(var) => {
                            let alloca = self.create_entry_block_alloca(var, "float");
                            self.declare_variable(var, alloca, VarType::Float);
                        }
                        Pattern::Error(var) => {
                            let alloca = self.create_entry_block_alloca(var, "string");
                            self.declare_variable(var, alloca, VarType::String);
                        }
                        Pattern::Wildcard => {}
                        Pattern::Literal(_) => {}
                    }
                    
                    for s in &first_case.body {
                        self.compile_stmt(s)?;
                    }
                    self.pop_scope();
                }
            }
            Stmt::Break => {
                if let Some(loop_ctx) = self.loop_stack.last().copied() {
                    self.builder.build_unconditional_branch(loop_ctx.break_block).unwrap();
                }
            }
            Stmt::Continue => {
                if let Some(loop_ctx) = self.loop_stack.last().copied() {
                    self.builder.build_unconditional_branch(loop_ctx.continue_block).unwrap();
                }
            }
            Stmt::Defer { stmt } => {
                // Store the deferred statement
                if let Some(defer_list) = self.defer_stack.last_mut() {
                    defer_list.push((**stmt).clone());
                }
            }
            Stmt::UnsafeBlock { body } => {
                // Unsafe block: compile body without safety checks
                for stmt in body {
                    self.compile_stmt(stmt)?;
                }
            }
            Stmt::RegionBlock { name: _, body } => {
                // Region block: compile body normally
                for stmt in body {
                    self.compile_stmt(stmt)?;
                }
            }
            Stmt::Import { path } => {
                // Import statements are handled by the module loader
                let _ = path;
            }
            Stmt::TryCatch { try_body, catch_var, catch_body, finally_body: _ } => {
                // If we have a catch variable, store a default error message
                if let Some(var) = catch_var {
                    if let Some((ptr, _)) = self.lookup_variable(var) {
                        let error_msg = self.builder.build_global_string_ptr("Operation failed", "err_msg").unwrap();
                        self.builder.build_store(ptr, error_msg).unwrap();
                    }
                }
                // Compile try body
                for stmt in try_body {
                    self.compile_stmt(stmt)?;
                }
                // Compile catch body (fallback if try fails)
                if !catch_body.is_empty() {
                    for stmt in catch_body {
                        self.compile_stmt(stmt)?;
                    }
                }
            }
            Stmt::ArrayAssign { array: _, index, value } => {
                // For now, just compile the value and ignore the assignment
                // In future, this will properly update the array
                let _val = self.compile_expr(value)?;
                let _idx = self.compile_expr(index)?;
            }
            Stmt::FunctionCall { name, args } => {
                // Check for string functions
                if name.starts_with("String.") {
                    let compiled_args: Vec<BasicValueEnum> = args.iter()
                        .map(|arg| self.compile_expr(arg))
                        .collect::<Result<Vec<_>>>()?;
                    let _ = self.call_string_function(name, &compiled_args)?;
                    return Ok(());
                }
                
                // Check for file functions
                if name.starts_with("File.") {
                    let compiled_args: Vec<BasicValueEnum> = args.iter()
                        .map(|arg| self.compile_expr(arg))
                        .collect::<Result<Vec<_>>>()?;
                    let _ = self.call_file_function(name, &compiled_args)?;
                    return Ok(());
                }
                
                let clean_name = name.trim_end_matches("()");
                let func = self.functions.get(clean_name).cloned().ok_or_else(|| CompileError::new(
                    &format!("Undefined function '{}'", name),
                        0, 0, "",
                        ErrorCode::E0001
                    ))?;
                
                let mut arg_vals = Vec::new();
                for arg in args {
                    arg_vals.push(self.compile_expr(arg)?.into());
                }
                
                let _ = self.builder.build_direct_call(func, &arg_vals, "calltmp").unwrap();
            }
        }
        Ok(())
    }


    fn evaluate_list_operation(&self, name: &str, elements: &[Expr]) -> Result<BasicValueEnum<'ctx>> {
        let mut values: Vec<f64> = Vec::new();
        for elem in elements {
            match elem {
                Expr::Number(n) => values.push(*n),
                Expr::Int(i) => values.push(*i as f64),
                _ => {}
            }
        }
        
        match name {
            "List.length" => {
                // Return as Float to match variable type system
                let len = values.len() as f64;
                Ok(self.context.f64_type().const_float(len).as_basic_value_enum())
            }
            "List.sum" => {
                let sum: f64 = values.iter().sum();
                Ok(self.context.f64_type().const_float(sum).as_basic_value_enum())
            }
            "List.max" => {
                let max = values.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
                Ok(self.context.f64_type().const_float(max).as_basic_value_enum())
            }
            "List.min" => {
                let min = values.iter().cloned().fold(f64::INFINITY, f64::min);
                Ok(self.context.f64_type().const_float(min).as_basic_value_enum())
            }
            _ => Ok(self.context.f64_type().const_float(0.0).as_basic_value_enum())
        }
    }
    
    fn promote_to_float(&self, val: BasicValueEnum<'ctx>) -> Result<FloatValue<'ctx>> {
        if val.is_float_value() {
            Ok(val.into_float_value())
        } else if val.is_int_value() {
            let int_val = val.into_int_value();
            let float_type = self.context.f64_type();
            let float_val = self.builder.build_signed_int_to_float(
                int_val,
                float_type,
                "int_to_float"
            ).map_err(|e| CompileError::new(
                &format!("Failed to convert int to float: {}", e),
                0, 0, "",
                ErrorCode::E0001
            ))?;
            Ok(float_val)
        } else {
            Err(CompileError::new(
                "Cannot convert non-numeric type to float",
                0, 0, "",
                ErrorCode::E0001
            ))
        }
    }

    fn compile_expr(&mut self, expr: &Expr) -> Result<BasicValueEnum<'ctx>> {
        match expr {
            Expr::Borrow { expr } => {
                // For now, just compile the inner expression
                self.compile_expr(expr)
            }
            Expr::MutBorrow { expr } => {
                // For now, just compile the inner expression
                self.compile_expr(expr)
            }
            Expr::Deref { expr } => {
                self.compile_expr(expr)
            }
            Expr::AddrOf { expr } => {
                self.compile_expr(expr)
            }
            Expr::Number(n) => {
                Ok(self.context.f64_type().const_float(*n).as_basic_value_enum())
            }
            Expr::Int(n) => {
                Ok(self.context.i64_type().const_int(*n as u64, true).as_basic_value_enum())
            }
            Expr::String(s) => {
                let global = self.builder.build_global_string_ptr(s, "str").unwrap();
                Ok(global.as_pointer_value().as_basic_value_enum())
            }
            Expr::Bool(b) => {
                Ok(self.context.bool_type().const_int(*b as u64, false).as_basic_value_enum())
            }
            Expr::Var(name) => {
                if let Some((ptr, var_type)) = self.lookup_variable(name) {
                    match var_type {
                        VarType::String => {
                            let loaded = self.builder.build_load(
                                self.context.ptr_type(AddressSpace::default()),
                                ptr,
                                name
                            ).unwrap();
                            Ok(loaded.as_basic_value_enum())
                        }
                        VarType::Int => {
                            let loaded = self.builder.build_load(
                                self.context.i64_type(),
                                ptr,
                                name
                            ).unwrap();
                            Ok(loaded.as_basic_value_enum())
                        }
                        VarType::Bool => {
                            let loaded = self.builder.build_load(
                                self.context.bool_type(),
                                ptr,
                                name
                            ).unwrap();
                            Ok(loaded.as_basic_value_enum())
                        }
                        _ => {
                            let loaded = self.builder.build_load(
                                self.context.f64_type(),
                                ptr,
                                name
                            ).unwrap();
                            Ok(loaded.as_basic_value_enum())
                        }
                    }
                } else {
                    Err(CompileError::new(
                        &format!("Undefined variable '{}'", name),
                            0, 0, "",
                            ErrorCode::E0001
                        ))
                }
            }
            Expr::List(_) => {
                Err(CompileError::new("Lists are tracked at compile time", 0, 0, ""
                , ErrorCode::E0001))
            }
            Expr::ArrayAccess { array, index } => {
                if let Expr::Var(array_name) = array.as_ref() {
                    let elements_opt = self.lists.get(array_name).cloned();
                    
                    if let Some(elements) = elements_opt {
                        // Try compile-time constant folding first
                        let idx_opt = if let Expr::Int(i) = index.as_ref() {
                            Some(*i as usize)
                        } else if let Expr::Number(n) = index.as_ref() {
                            Some(*n as usize)
                        } else {
                            None
                        };
                        
                        if let Some(idx) = idx_opt {
                            // Compile-time bounds check
                            if idx >= elements.len() {
                                return Err(CompileError::new(
                                    &format!(
                                        "Array index {} out of bounds (array has {} elements)",
                                        idx, elements.len()
                                    ),
                                    0, 0, "",
                                    ErrorCode::E0006
                                ));
                            }
                            
                            let element = &elements[idx];
                            let result = self.compile_expr(element)?;
                            return Ok(result);
                        }
                        
                        // Dynamic index - runtime bounds check
                        let idx_val = self.compile_expr(index)?;
                        let idx_int = if idx_val.is_float_value() {
                            self.builder.build_float_to_signed_int(
                                idx_val.into_float_value(),
                                self.context.i64_type(),
                                "ftoi"
                            ).unwrap()
                        } else {
                            idx_val.into_int_value()
                        };
                        
                        self.emit_runtime_bounds_check(idx_int, elements.len(), array_name)?;
                        
                        // Return first element (simplified for now)
                        let element = &elements[0];
                        let result = self.compile_expr(element)?;
                        Ok(result)
                    } else {
                        Err(CompileError::new(
                            &format!("Undefined list '{}'", array_name),
                            0, 0, "",
                            ErrorCode::E0003
                        ))
                    }
                } else {
                    Err(CompileError::new(
                        "Array access requires variable name",
                        0, 0, "",
                        ErrorCode::E0001
                    ))
                }
            }
            Expr::Binary { left, op, right } => {
                let l_val = self.compile_expr(left)?;
                let r_val = self.compile_expr(right)?;

                match op {
                    BinOp::Add => {
                        if l_val.is_float_value() && r_val.is_float_value() {
                            let result = self.builder.build_float_add(
                                l_val.into_float_value(), 
                                r_val.into_float_value(), 
                                "addtmp"
                            ).unwrap();
                            Ok(result.as_basic_value_enum())
                        } else if l_val.is_int_value() && r_val.is_int_value() {
                            let result = self.builder.build_int_add(
                                l_val.into_int_value(), 
                                r_val.into_int_value(), 
                                "addtmp"
                            ).unwrap();
                            Ok(result.as_basic_value_enum())
                        } else if l_val.is_int_value() && r_val.is_float_value() {
                            // Convert int to float, then add
                            let l_float = self.promote_to_float(l_val)?;
                            let result = self.builder.build_float_add(
                                l_float,
                                r_val.into_float_value(),
                                "addtmp"
                            ).unwrap();
                            Ok(result.as_basic_value_enum())
                        } else if l_val.is_float_value() && r_val.is_int_value() {
                            // Convert int to float, then add
                            let r_float = self.promote_to_float(r_val)?;
                            let result = self.builder.build_float_add(
                                l_val.into_float_value(),
                                r_float,
                                "addtmp"
                            ).unwrap();
                            Ok(result.as_basic_value_enum())
                        } else {
                            Err(CompileError::new("Type mismatch in addition", 0, 0, "", ErrorCode::E0001))
                        }
                    }
                    BinOp::Subtract => {
                        if l_val.is_float_value() && r_val.is_float_value() {
                            let result = self.builder.build_float_sub(
                                l_val.into_float_value(), 
                                r_val.into_float_value(), 
                                "subtmp"
                            ).unwrap();
                            Ok(result.as_basic_value_enum())
                        } else if l_val.is_int_value() && r_val.is_int_value() {
                            let result = self.builder.build_int_sub(
                                l_val.into_int_value(), 
                                r_val.into_int_value(), 
                                "subtmp"
                            ).unwrap();
                            Ok(result.as_basic_value_enum())
                        } else if l_val.is_int_value() && r_val.is_float_value() {
                            let l_float = self.promote_to_float(l_val)?;
                            let result = self.builder.build_float_sub(
                                l_float,
                                r_val.into_float_value(),
                                "subtmp"
                            ).unwrap();
                            Ok(result.as_basic_value_enum())
                        } else if l_val.is_float_value() && r_val.is_int_value() {
                            let r_float = self.promote_to_float(r_val)?;
                            let result = self.builder.build_float_sub(
                                l_val.into_float_value(),
                                r_float,
                                "subtmp"
                            ).unwrap();
                            Ok(result.as_basic_value_enum())
                        } else {
                            Err(CompileError::new("Type mismatch in subtraction", 0, 0, "", ErrorCode::E0001))
                        }
                    }
                    BinOp::Multiply => {
                        if l_val.is_float_value() && r_val.is_float_value() {
                            let result = self.builder.build_float_mul(
                                l_val.into_float_value(), 
                                r_val.into_float_value(), 
                                "multmp"
                            ).unwrap();
                            Ok(result.as_basic_value_enum())
                        } else if l_val.is_int_value() && r_val.is_int_value() {
                            let result = self.builder.build_int_mul(
                                l_val.into_int_value(), 
                                r_val.into_int_value(), 
                                "multmp"
                            ).unwrap();
                            Ok(result.as_basic_value_enum())
                        } else if l_val.is_int_value() && r_val.is_float_value() {
                            let l_float = self.promote_to_float(l_val)?;
                            let result = self.builder.build_float_mul(
                                l_float,
                                r_val.into_float_value(),
                                "multmp"
                            ).unwrap();
                            Ok(result.as_basic_value_enum())
                        } else if l_val.is_float_value() && r_val.is_int_value() {
                            let r_float = self.promote_to_float(r_val)?;
                            let result = self.builder.build_float_mul(
                                l_val.into_float_value(),
                                r_float,
                                "multmp"
                            ).unwrap();
                            Ok(result.as_basic_value_enum())
                        } else {
                            Err(CompileError::new("Type mismatch in multiplication", 0, 0, "", ErrorCode::E0001))
                        }
                    }
                    BinOp::Divide => {
                        if l_val.is_float_value() && r_val.is_float_value() {
                            let result = self.builder.build_float_div(
                                l_val.into_float_value(), 
                                r_val.into_float_value(), 
                                "divtmp"
                            ).unwrap();
                            Ok(result.as_basic_value_enum())
                        } else if l_val.is_int_value() && r_val.is_int_value() {
                            let result = self.builder.build_int_signed_div(
                                l_val.into_int_value(), 
                                r_val.into_int_value(), 
                                "divtmp"
                            ).unwrap();
                            Ok(result.as_basic_value_enum())
                        } else if l_val.is_int_value() && r_val.is_float_value() {
                            let l_float = self.promote_to_float(l_val)?;
                            let result = self.builder.build_float_div(
                                l_float,
                                r_val.into_float_value(),
                                "divtmp"
                            ).unwrap();
                            Ok(result.as_basic_value_enum())
                        } else if l_val.is_float_value() && r_val.is_int_value() {
                            let r_float = self.promote_to_float(r_val)?;
                            let result = self.builder.build_float_div(
                                l_val.into_float_value(),
                                r_float,
                                "divtmp"
                            ).unwrap();
                            Ok(result.as_basic_value_enum())
                        } else {
                            Err(CompileError::new("Type mismatch in division", 0, 0, "", ErrorCode::E0001))
                        }
                    }
                    BinOp::Greater | BinOp::Less | BinOp::GreaterEqual | BinOp::LessEqual | BinOp::Equal | BinOp::NotEqual => {
                        // Handle mixed Int/Float comparison
                        if l_val.is_float_value() && r_val.is_float_value() {
                            let pred = match op {
                                BinOp::Greater => FloatPredicate::OGT,
                                BinOp::Less => FloatPredicate::OLT,
                                BinOp::GreaterEqual => FloatPredicate::OGE,
                                BinOp::LessEqual => FloatPredicate::OLE,
                                BinOp::Equal => FloatPredicate::OEQ,
                                BinOp::NotEqual => FloatPredicate::ONE,
                                _ => unreachable!(),
                            };
                            let cmp = self.builder.build_float_compare(
                                pred,
                                l_val.into_float_value(), 
                                r_val.into_float_value(), 
                                "cmptmp"
                            ).unwrap();
                            Ok(cmp.as_basic_value_enum())
                        } else if l_val.is_int_value() && r_val.is_int_value() {
                            let pred = match op {
                                BinOp::Greater => IntPredicate::SGT,
                                BinOp::Less => IntPredicate::SLT,
                                BinOp::GreaterEqual => IntPredicate::SGE,
                                BinOp::LessEqual => IntPredicate::SLE,
                                BinOp::Equal => IntPredicate::EQ,
                                BinOp::NotEqual => IntPredicate::NE,
                                _ => unreachable!(),
                            };
                            let cmp = self.builder.build_int_compare(
                                pred,
                                l_val.into_int_value(), 
                                r_val.into_int_value(), 
                                "cmptmp"
                            ).unwrap();
                            Ok(cmp.as_basic_value_enum())
                        } else {
                            // Mixed Int/Float - promote to float
                            let l_float = self.promote_to_float(l_val)?;
                            let r_float = self.promote_to_float(r_val)?;
                            let pred = match op {
                                BinOp::Greater => FloatPredicate::OGT,
                                BinOp::Less => FloatPredicate::OLT,
                                BinOp::GreaterEqual => FloatPredicate::OGE,
                                BinOp::LessEqual => FloatPredicate::OLE,
                                BinOp::Equal => FloatPredicate::OEQ,
                                BinOp::NotEqual => FloatPredicate::ONE,
                                _ => unreachable!(),
                            };
                            let cmp = self.builder.build_float_compare(
                                pred,
                                l_float, 
                                r_float, 
                                "cmptmp"
                            ).unwrap();
                            Ok(cmp.as_basic_value_enum())
                        }
                    }
                    BinOp::And | BinOp::Or => {
                        let l_bool = if l_val.is_float_value() {
                            let zero = self.context.f64_type().const_float(0.0);
                            self.builder.build_float_compare(
                                FloatPredicate::ONE,
                                l_val.into_float_value(),
                                zero,
                                "lbool"
                            ).unwrap()
                        } else {
                            l_val.into_int_value()
                        };
                        let r_bool = if r_val.is_float_value() {
                            let zero = self.context.f64_type().const_float(0.0);
                            self.builder.build_float_compare(
                                FloatPredicate::ONE,
                                r_val.into_float_value(),
                                zero,
                                "rbool"
                            ).unwrap()
                        } else {
                            r_val.into_int_value()
                        };
                        let result = if matches!(op, BinOp::And) {
                            self.builder.build_and(l_bool, r_bool, "andtmp").unwrap()
                        } else {
                            self.builder.build_or(l_bool, r_bool, "ortmp").unwrap()
                        };
                        Ok(result.as_basic_value_enum())
                    }
                }
            }
            Expr::Some { value } => {
                // For now, just unwrap the value
                let inner = self.compile_expr(value)?;
                Ok(inner)
            }
            Expr::None => {
                // None has no value, return 0.0 for now
                Ok(self.context.f64_type().const_float(0.0).as_basic_value_enum())
            }
            Expr::Ok { value } => {
                let inner = self.compile_expr(value)?;
                Ok(inner)
            }
            Expr::Error { value } => {
                let inner = self.compile_expr(value)?;
                Ok(inner)
            }
            Expr::FunctionCall { name, args } => {
                // Check for string functions
                if name.starts_with("String.") {
                    let compiled_args: Vec<BasicValueEnum> = args.iter()
                        .map(|arg| self.compile_expr(arg))
                        .collect::<Result<Vec<_>>>()?;
                    return self.call_string_function(name, &compiled_args);
                }
                
                // Check for file functions
                if name.starts_with("File.") {
                    let compiled_args: Vec<BasicValueEnum> = args.iter()
                        .map(|arg| self.compile_expr(arg))
                        .collect::<Result<Vec<_>>>()?;
                    return self.call_file_function(name, &compiled_args);
                }
                
                // Check for List functions - evaluate at compile time
                if name.starts_with("List.") {
                    // Check if arg is a list literal
                    if let Some(Expr::List(elements)) = args.first() {
                        return self.evaluate_list_operation(name, elements);
                    }
                    // Check if arg is a variable that holds a list
                    if let Some(Expr::Var(var_name)) = args.first() {
                        if let Some(elements) = self.lists.get(var_name) {
                            return self.evaluate_list_operation(name, elements);
                        }
                    }
                    return Ok(self.context.f64_type().const_float(0.0).as_basic_value_enum());
                }
                
                // Functions are registered with their ALGOL26 names (e.g., "Math.sqrt")
                let clean_name = name.trim_end_matches("()");
                let func = self.functions.get(clean_name).cloned().ok_or_else(|| CompileError::new(
                    &format!("Undefined function '{}'", name),
                        0, 0, "",
                        ErrorCode::E0001
                    ))?;
                
                let mut arg_vals = Vec::new();
                for arg in args {
                    arg_vals.push(self.compile_expr(arg)?.into());
                }
                
                let call = self.builder.build_direct_call(func, &arg_vals, "calltmp").unwrap();
                
                // Correct chain: try_as_basic_value() -> ValueKind, then .basic() -> Option<BasicValueEnum>
                if let Some(value) = call.try_as_basic_value().basic() {
                    Ok(value)
                } else {
                    // Void function - return 0.0 as placeholder
                    Ok(self.context.f64_type().const_float(0.0).as_basic_value_enum())
                }
            }
        }
    }

    #[allow(dead_code)]
    fn emit_bounds_check(&self, index: IntValue, len: u64, array_name: &str) -> Result<()> {
        let len_val = self.context.i64_type().const_int(len, false);
        let zero = self.context.i64_type().const_int(0, false);
        
        // Check index >= 0
        let ge_zero = self.builder.build_int_compare(
            inkwell::IntPredicate::SGE,
            index,
            zero,
            "ge_zero"
        ).unwrap();
        
        // Check index < len
        let lt_len = self.builder.build_int_compare(
            inkwell::IntPredicate::SLT,
            index,
            len_val,
            "lt_len"
        ).unwrap();
        
        // Combine checks
        let in_bounds = self.builder.build_and(ge_zero, lt_len, "in_bounds").unwrap();
        
        // Create basic blocks
        let parent_fn = self.builder.get_insert_block().unwrap().get_parent().unwrap();
        let continue_bb = self.context.append_basic_block(parent_fn, "bounds_ok");
        let error_bb = self.context.append_basic_block(parent_fn, "bounds_error");
        
        // Branch based on check
        self.builder.build_conditional_branch(in_bounds, continue_bb, error_bb).unwrap();
        
        // Error path
        self.builder.position_at_end(error_bb);
        let printf_func = self.functions.get("printf").unwrap().clone();
        let msg = format!("Runtime error: Array '{}' index out of bounds\n", array_name);
        let error_msg = self.builder.build_global_string_ptr(&msg, "bounds_msg").unwrap();
        let format = self.builder.build_global_string_ptr("%s", "fmt").unwrap();
        self.builder.build_direct_call(
            printf_func,
            &[format.as_pointer_value().into(), error_msg.as_pointer_value().into()],
            "printf_error"
        ).unwrap();
        
        // Exit with code 1
        let exit_func = self.module.add_function(
            "exit",
            self.context.void_type().fn_type(&[self.context.i32_type().into()], false),
            None
        );
        let one = self.context.i32_type().const_int(1, false);
        self.builder.build_direct_call(exit_func, &[one.into()], "exit").unwrap();
        self.builder.build_unreachable().unwrap();
        
        // Continue path
        self.builder.position_at_end(continue_bb);
        
        Ok(())
    }

    fn emit_runtime_bounds_check(&self, index: IntValue, len: usize, array_name: &str) -> Result<()> {
        let len_val = self.context.i64_type().const_int(len as u64, false);
        let zero = self.context.i64_type().const_int(0, false);
        
        let ge_zero = self.builder.build_int_compare(
            inkwell::IntPredicate::SGE,
            index,
            zero,
            "ge_zero"
        ).unwrap();
        
        let lt_len = self.builder.build_int_compare(
            inkwell::IntPredicate::SLT,
            index,
            len_val,
            "lt_len"
        ).unwrap();
        
        let in_bounds = self.builder.build_and(ge_zero, lt_len, "in_bounds").unwrap();
        
        let parent_fn = self.builder.get_insert_block().unwrap().get_parent().unwrap();
        let continue_bb = self.context.append_basic_block(parent_fn, "bounds_ok");
        let error_bb = self.context.append_basic_block(parent_fn, "bounds_error");
        
        self.builder.build_conditional_branch(in_bounds, continue_bb, error_bb).unwrap();
        
        // Error path
        self.builder.position_at_end(error_bb);
        let printf_func = self.functions.get("printf").unwrap().clone();
        let _ = array_name;
        let msg = format!("Runtime error: Array index out of bounds\n");
        let error_msg = self.builder.build_global_string_ptr(&msg, "bounds_msg").unwrap();
        let format = self.builder.build_global_string_ptr("%s", "fmt_err").unwrap();
        self.builder.build_direct_call(
            printf_func,
            &[format.as_pointer_value().into(), error_msg.as_pointer_value().into()],
            "printf_error"
        ).unwrap();
        
        let exit_func = self.module.add_function(
            "exit",
            self.context.void_type().fn_type(&[self.context.i32_type().into()], false),
            None
        );
        let one = self.context.i32_type().const_int(1, false);
        self.builder.build_direct_call(exit_func, &[one.into()], "exit").unwrap();
        self.builder.build_unreachable().unwrap();
        
        // Continue path
        self.builder.position_at_end(continue_bb);
        
        Ok(())
    }

    fn create_entry_block_alloca(&self, name: &str, type_str: &str) -> PointerValue<'ctx> {
        let builder = self.context.create_builder();
        let entry = self.builder.get_insert_block().unwrap().get_parent().unwrap().get_first_basic_block().unwrap();
        
        match entry.get_first_instruction() {
            Some(first_instr) => builder.position_before(&first_instr),
            None => builder.position_at_end(entry),
        }
        
        let alloca_type = self.get_type_from_string(type_str);
        builder.build_alloca(alloca_type, name).unwrap()
    }
}
