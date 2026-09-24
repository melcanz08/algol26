// src/semantics/builder/build.rs

use super::*;

impl SemanticIRBuilder {
    pub(super) fn build_impl(&mut self, functions: &[FunctionDecl]) -> SemanticProgram {
        let mut program = SemanticProgram::new();

        // Register Math functions
        self.function_types.insert(
            "Math.sqrt".to_string(),
            FunctionSignature {
                params: vec![("x".to_string(), Type::Float)],
                return_type: Type::Float,
            },
        );
        self.function_types.insert(
            "Math.pow".to_string(),
            FunctionSignature {
                params: vec![
                    ("x".to_string(), Type::Float),
                    ("y".to_string(), Type::Float),
                ],
                return_type: Type::Float,
            },
        );
        self.function_types.insert(
            "Math.sin".to_string(),
            FunctionSignature {
                params: vec![("x".to_string(), Type::Float)],
                return_type: Type::Float,
            },
        );
        self.function_types.insert(
            "Math.cos".to_string(),
            FunctionSignature {
                params: vec![("x".to_string(), Type::Float)],
                return_type: Type::Float,
            },
        );
        self.function_types.insert(
            "Math.abs".to_string(),
            FunctionSignature {
                params: vec![("x".to_string(), Type::Float)],
                return_type: Type::Float,
            },
        );
        self.function_types.insert(
            "Math.floor".to_string(),
            FunctionSignature {
                params: vec![("x".to_string(), Type::Float)],
                return_type: Type::Float,
            },
        );
        self.function_types.insert(
            "Math.ceil".to_string(),
            FunctionSignature {
                params: vec![("x".to_string(), Type::Float)],
                return_type: Type::Float,
            },
        );
        self.function_types.insert(
            "Math.exp".to_string(),
            FunctionSignature {
                params: vec![("x".to_string(), Type::Float)],
                return_type: Type::Float,
            },
        );
        self.function_types.insert(
            "Math.log".to_string(),
            FunctionSignature {
                params: vec![("x".to_string(), Type::Float)],
                return_type: Type::Float,
            },
        );
        self.function_types.insert(
            "Math.tan".to_string(),
            FunctionSignature {
                params: vec![("x".to_string(), Type::Float)],
                return_type: Type::Float,
            },
        );

        // Register String functions
        self.function_types.insert(
            "String.length".to_string(),
            FunctionSignature {
                params: vec![("s".to_string(), Type::String)],
                return_type: Type::Int,
            },
        );
        self.function_types.insert(
            "String.concat".to_string(),
            FunctionSignature {
                params: vec![
                    ("s1".to_string(), Type::String),
                    ("s2".to_string(), Type::String),
                ],
                return_type: Type::String,
            },
        );
        self.function_types.insert(
            "String.substring".to_string(),
            FunctionSignature {
                params: vec![
                    ("s".to_string(), Type::String),
                    ("start".to_string(), Type::Int),
                    ("length".to_string(), Type::Int),
                ],
                return_type: Type::String,
            },
        );
        self.function_types.insert(
            "String.to_upper".to_string(),
            FunctionSignature {
                params: vec![("s".to_string(), Type::String)],
                return_type: Type::String,
            },
        );
        self.function_types.insert(
            "String.to_lower".to_string(),
            FunctionSignature {
                params: vec![("s".to_string(), Type::String)],
                return_type: Type::String,
            },
        );

        // Register File functions
        self.function_types.insert(
            "File.read".to_string(),
            FunctionSignature {
                params: vec![("path".to_string(), Type::String)],
                return_type: Type::String,
            },
        );

        // Register Raw memory functions
        self.function_types.insert(
            "alloc".to_string(),
            FunctionSignature {
                params: vec![("size".to_string(), Type::Int)],
                return_type: Type::Pointer(Box::new(Type::Unknown)),
            },
        );
        self.function_types.insert(
            "free".to_string(),
            FunctionSignature {
                params: vec![("ptr".to_string(), Type::Pointer(Box::new(Type::Unknown)))],
                return_type: Type::Void,
            },
        );

        // Register List functions
        self.function_types.insert(
            "List.length".to_string(),
            FunctionSignature {
                params: vec![("arr".to_string(), Type::List(Box::new(Type::Float)))],
                return_type: Type::Int,
            },
        );
        self.function_types.insert(
            "List.sum".to_string(),
            FunctionSignature {
                params: vec![("arr".to_string(), Type::List(Box::new(Type::Float)))],
                return_type: Type::Float,
            },
        );
        self.function_types.insert(
            "List.max".to_string(),
            FunctionSignature {
                params: vec![("arr".to_string(), Type::List(Box::new(Type::Float)))],
                return_type: Type::Float,
            },
        );
        self.function_types.insert(
            "List.min".to_string(),
            FunctionSignature {
                params: vec![("arr".to_string(), Type::List(Box::new(Type::Float)))],
                return_type: Type::Float,
            },
        );
        self.function_types.insert(
            "File.write".to_string(),
            FunctionSignature {
                params: vec![
                    ("path".to_string(), Type::String),
                    ("content".to_string(), Type::String),
                ],
                return_type: Type::Int,
            },
        );
        self.function_types.insert(
            "File.append".to_string(),
            FunctionSignature {
                params: vec![
                    ("path".to_string(), Type::String),
                    ("content".to_string(), Type::String),
                ],
                return_type: Type::Int,
            },
        );

        // Register user-defined functions (templates, including extern).
        for func in functions {
            let return_type = func
                .return_type
                .as_ref()
                .map(|t| t.to_type())
                .unwrap_or(Type::Void);
            let params = func
                .params
                .iter()
                .map(|(n, t)| {
                    let type_ = match t {
                        Some(s) => s.to_type(),
                        None => Type::Unknown,
                    };
                    (n.clone(), type_)
                })
                .collect();

            self.function_types.insert(
                func.name.clone(),
                FunctionSignature {
                    params,
                    return_type,
                },
            );
        }

        // Register specialization signatures up front. Call sites
        // inside any function — including non-generic callers like
        // `main` — look these up for argument coercion. Doing this
        // before the emit loop means a specialization's signature is
        // available regardless of declaration order.
        self.register_specialization_signatures(functions);

        // Emit functions.
        for func in functions {
            if func.type_params.is_empty() {
                self.emit_one_function(&mut program, func, func.name.clone(), HashMap::new());
            } else {
                // The generic template is analyzer input, not
                // executable output. Skip it; emit one
                // SemanticFunction per concrete specialization the
                // plan recorded. Stage 3.2e adds transitive closure
                // over symbolic call-site entries.
                let specs: Vec<Specialization> = self
                    .plan
                    .specializations
                    .values()
                    .filter(|s| s.function == func.name)
                    .cloned()
                    .collect();

                for spec in specs {
                    self.emit_one_function(
                        &mut program,
                        func,
                        spec.mangled_name.clone(),
                        spec.bindings(),
                    );
                }
            }
        }

        program
    }

    /// Pre-register each concrete specialization's mangled name with
    /// its substituted parameter and return types, so call-site
    /// coercion can resolve the specialized signature before the
    /// function body is even emitted.
    fn register_specialization_signatures(&mut self, functions: &[FunctionDecl]) {
        let specs: Vec<Specialization> = self.plan.all_specializations().cloned().collect();
        for spec in specs {
            let Some(func) = functions.iter().find(|f| f.name == spec.function) else {
                continue;
            };
            let subst = spec.bindings();
            let params: Vec<(String, Type)> = func
                .params
                .iter()
                .map(|(n, t)| {
                    let raw = match t {
                        Some(s) => s.to_type(),
                        None => Type::Unknown,
                    };
                    (n.clone(), raw.substitute(&subst))
                })
                .collect();
            let return_type = func
                .return_type
                .as_ref()
                .map(|t| t.to_type())
                .unwrap_or(Type::Void)
                .substitute(&subst);
            self.function_types.insert(
                spec.mangled_name.clone(),
                FunctionSignature {
                    params,
                    return_type,
                },
            );
        }
    }

    /// Emit one `SemanticFunction` from `func`'s body, under the
    /// substitution `subst`, naming the output `emitted_name`.
    ///
    /// For a non-generic function, `subst` is empty and
    /// `emitted_name == func.name`, so the emitted function is
    /// identical to what the pre-3.2d builder produced.
    fn emit_one_function(
        &mut self,
        program: &mut SemanticProgram,
        func: &FunctionDecl,
        emitted_name: String,
        subst: HashMap<String, Type>,
    ) {
        let saved_subst = std::mem::replace(&mut self.current_subst, subst);

        self.push_scope();
        for (name, type_str) in &func.params {
            let raw = match type_str {
                Some(s) => s.to_type(),
                None => Type::Unknown,
            };
            let param_type = raw.substitute(&self.current_subst);
            if self.scopes.last().is_some_and(|s| s.contains_key(name)) {
                self.diagnostics.push(format!(
                    "Duplicate parameter '{}' in function '{}'",
                    name, emitted_name
                ));
            } else {
                self.declare_var(name, param_type, true);
            }
        }

        let entry_id = program.new_block_id();

        let emitted_params: Vec<(String, Type)> = func
            .params
            .iter()
            .map(|(n, t)| {
                let raw = match t {
                    Some(s) => s.to_type(),
                    None => Type::Unknown,
                };
                (n.clone(), raw.substitute(&self.current_subst))
            })
            .collect();
        let emitted_return = func
            .return_type
            .as_ref()
            .map(|t| t.to_type())
            .unwrap_or(Type::Void)
            .substitute(&self.current_subst);

        let mut semantic_func = SemanticFunction {
            name: emitted_name.clone(),
            params: emitted_params,
            return_type: emitted_return,
            blocks: vec![SemanticBlock {
                id: entry_id,
                instructions: Vec::new(),
                terminator: None,
            }],
            entry_block: entry_id,
            is_extern: func.is_extern,
        };

        // Fresh defer scope for this function. The previous function's
        // cleanups must not leak into this one.
        self.defer_stack.clear();

        let flow = self.translate_block(program, &mut semantic_func, entry_id, &func.body);

        match flow {
            FlowResult::Reachable(final_id) => {
                // Fall-off-the-end of a function is an implicit `return`. If
                // any defers were registered during this function's body, chain
                // their cleanup blocks LIFO before the implicit return — same
                // shape as the chain in Stmt::Return.
                let cleanups: Vec<usize> = self
                    .defer_stack
                    .last()
                    .map(|ctx| ctx.cleanup_blocks.iter().rev().copied().collect())
                    .unwrap_or_default();

                if cleanups.is_empty() {
                    if let Some(b) = semantic_func.blocks.iter_mut().find(|b| b.id == final_id) {
                        if b.terminator.is_none() {
                            b.terminator = Some(Terminator::Return {
                                value: None,
                                type_: Type::Void,
                            });
                        }
                    }
                } else {
                    // final block → first cleanup
                    if let Some(b) = semantic_func.blocks.iter_mut().find(|b| b.id == final_id) {
                        if b.terminator.is_none() {
                            b.terminator = Some(Terminator::Jump { block: cleanups[0] });
                        }
                    }
                    // each cleanup → next cleanup
                    for i in 0..cleanups.len() - 1 {
                        if let Some(cb) = semantic_func
                            .blocks
                            .iter_mut()
                            .find(|b| b.id == cleanups[i])
                        {
                            cb.terminator = Some(Terminator::Jump {
                                block: cleanups[i + 1],
                            });
                        }
                    }
                    // last cleanup emits the real return
                    if let Some(cb) = semantic_func
                        .blocks
                        .iter_mut()
                        .find(|b| cleanups.last().is_some_and(|last| b.id == *last))
                    {
                        cb.terminator = Some(Terminator::Return {
                            value: None,
                            type_: Type::Void,
                        });
                    }
                }
            }
            FlowResult::Unreachable => {}
        }

        for block in &mut semantic_func.blocks {
            if block.terminator.is_none() && semantic_func.return_type == Type::Void {
                block.terminator = Some(Terminator::Return {
                    value: None,
                    type_: Type::Void,
                });
            }
        }

        let is_impl_method = func
            .params
            .first()
            .map(|(name, _)| name == "self")
            .unwrap_or(false);

        if !func.is_extern
            && !is_impl_method
            && semantic_func.return_type != Type::Void
            && flow.is_reachable()
        {
            self.diagnostics.push(format!(
                "Function '{}' may reach end without returning a value",
                emitted_name
            ));
        }

        self.pop_scope();

        // Extern declarations carry link metadata that the LLVM
        // codegen and linker need. Record it on the program before
        // the function is moved into the functions vec. Externs are
        // never generic, so the metadata is keyed by the declaration
        // name.
        if func.is_extern {
            if let Some(ffi) = &func.ffi_info {
                if let Some(sym) = &ffi.symbol_name {
                    program.ffi_symbols.insert(func.name.clone(), sym.clone());
                }
                if let Some(lib) = &ffi.library {
                    if !program.ffi_libraries.contains(lib) {
                        program.ffi_libraries.push(lib.clone());
                    }
                }
                if ffi.variadic {
                    program.variadic_functions.insert(func.name.clone());
                }
            }
        }

        program.functions.push(semantic_func);

        self.current_subst = saved_subst;
    }
}
