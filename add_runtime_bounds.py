with open('src/codegen.rs', 'r') as f:
    content = f.read()

# Replace the current ArrayAccess with runtime bounds checking
old_array_access = """            Expr::ArrayAccess { array, index } => {
                // Get array name
                if let Expr::Var(array_name) = array.as_ref() {
                    // Clone elements to avoid borrow issues
                    let elements_opt = self.lists.get(array_name).cloned();
                    
                    if let Some(elements) = elements_opt {
                        // Get index as integer (constant folding for compile-time)
                        let idx = if let Expr::Int(i) = index.as_ref() {
                            *i as usize
                        } else if let Expr::Number(n) = index.as_ref() {
                            *n as usize
                        } else {
                            // For dynamic indices, default to 0 for now
                            0
                        };
                        
                        // Bounds check at compile time
                        if idx >= elements.len() {
                            return Err(CompileError::new(
                                &format!(
                                    "Array index {} out of bounds (array has {} elements)",
                                    idx, elements.len()
                                ),
                                0, 0, "",
                                ErrorCode::E0001
                            ));
                        }
                        
                        // Compile the correct element
                        let element = &elements[idx];
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
            }"""

new_array_access = """            Expr::ArrayAccess { array, index } => {
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
                        
                        // Dynamic index - need runtime bounds checking
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
                        
                        // Runtime bounds check
                        self.emit_runtime_bounds_check(idx_int, elements.len(), array_name)?;
                        
                        // For now, return the first element (will be replaced with switch)
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
            }"""

content = content.replace(old_array_access, new_array_access)

# Add the emit_runtime_bounds_check method if not already present
if 'fn emit_runtime_bounds_check' not in content:
    # Add before create_entry_block_alloca
    old_create = "    fn create_entry_block_alloca("
    new_method = """    fn emit_runtime_bounds_check(&self, index: IntValue, len: usize, array_name: &str) -> Result<()> {
        let len_val = self.context.i64_type().const_int(len as u64, false);
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
        
        // Error path - print message and exit
        self.builder.position_at_end(error_bb);
        let printf_func = self.functions.get("printf").unwrap().clone();
        let msg = format!("Runtime error: Array '{}' index out of bounds\\n", array_name);
        let error_msg = self.builder.build_global_string_ptr(&msg, "bounds_msg").unwrap();
        let format = self.builder.build_global_string_ptr("%s", "fmt_err").unwrap();
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

    fn create_entry_block_alloca("
    
    content = content.replace(old_create, new_method)

with open('src/codegen.rs', 'w') as f:
    f.write(content)

print("✅ Added runtime bounds checking")
