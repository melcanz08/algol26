// src/semantics/semantic_builder/build.rs

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

        // Register user-defined functions (including extern)
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

        for func in functions {
            self.push_scope();
            for (name, type_str) in &func.params {
                let param_type = match type_str {
                    Some(s) => s.to_type(),
                    None => Type::Unknown,
                };
                if self.scopes.last().is_some_and(|s| s.contains_key(name)) {
                    self.diagnostics.push(format!(
                        "Duplicate parameter '{}' in function '{}'",
                        name, func.name
                    ));
                } else {
                    self.declare_var(name, param_type, false);
                }
            }

            let entry_id = program.new_block_id();
            let mut semantic_func = SemanticFunction {
                name: func.name.clone(),
                params: func
                    .params
                    .iter()
                    .map(|(n, t)| {
                        let type_ = match t {
                            Some(s) => s.to_type(),
                            None => Type::Unknown,
                        };
                        (n.clone(), type_)
                    })
                    .collect(),
                return_type: func
                    .return_type
                    .as_ref()
                    .map(|t| t.to_type())
                    .unwrap_or(Type::Void),
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

            let flow = self.translate_block(&mut program, &mut semantic_func, entry_id, &func.body);

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
                                cb.terminator = Some(Terminator::Jump { block: cleanups[i + 1] });
                            }
                        }
                        // last cleanup emits the real return
                        if let Some(cb) = semantic_func
                            .blocks
                            .iter_mut()
                            .find(|b| b.id == *cleanups.last().unwrap())
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
                    func.name
                ));
            }

            self.pop_scope();
            program.functions.push(semantic_func);
        }

        program
    }
}