// src/backends/interpreter/mod.rs
//
// Tree-walking interpreter over SemanticProgram.
//
// Supported:
//   - the full instruction set except FFI and channel operations
//   - Option, Result, and pattern matching with payload bindings
//   - user-defined function calls (via eval_call) with frame
//     save/restore, including nested calls in expressions
//
// Not supported:
//   - parallel execution. Spawn and Fork run sequentially; the
//     interpreter does not create OS threads.
//   - foreign function calls.
//   - channel declarations, sends, and receives. The capability
//     check refuses these before execution; the instruction
//     arms are defensive guards.
//   - reference operations (`&x`, `&mut x`, `*r`). Also refused
//     by the capability check before execution.
//
// Errors: `execute_function` returns `Result<(), EvalError>`.
// `run()` is the public boundary and converts to `String` for
// backward compatibility. See `runtime::EvalError` for the
// three error categories (TypeMismatch, Runtime, Unsupported).
use crate::ir::semantic_ir::{
    Instruction, SemanticFunction, SemanticProgram, Terminator, TypedIRValue,
};
use std::collections::HashMap;

mod eval;
mod pattern;
mod runtime;
pub use runtime::{EvalError, RuntimeValue};

/// A single active `region NAME` block. Allocations made inside
/// the block are recorded here by `Instruction::Allocate` and
/// freed when the region exits (either via `RegionExit` or via
/// an early `return` from the enclosing function).
#[derive(Debug)]
pub(super) struct RegionFrame {
    pub name: String,
    /// Handles into `Interpreter::heap` for allocations made
    /// while this frame was the innermost active region.
    pub allocations: Vec<usize>,
}

pub struct Interpreter {
    pub(super) variables: HashMap<String, RuntimeValue>,
    pub(super) output: Vec<String>,
    pub(super) program: SemanticProgram,
    pub(super) return_value: Option<RuntimeValue>,
    /// Simulated heap for `alloc` / `free`. The key is an opaque
    /// pointer handle (exposed to the program as
    /// `RuntimeValue::Int`); the value is the allocated byte
    /// buffer. Real pointers are not meaningful in a tree-walker,
    /// so the handle indirection gives the runtime the same
    /// observable behavior without FFI.
    pub(super) heap: HashMap<usize, Vec<u8>>,
    pub(super) next_ptr: usize,
    /// Stack of active `region` blocks in the current function.
    /// Cleared on function exit; saved and restored across
    /// user-function calls so a callee cannot accidentally free
    /// its caller's region allocations.
    pub(super) region_stack: Vec<RegionFrame>,
    /// Command-line arguments passed to the process, excluding the
    /// program name. Exposed to the language via the `args()`
    /// builtin. Set at construction from `std::env::args()` in
    /// production; injectable via `with_args` for tests.
    pub(super) command_line_args: Vec<String>,
}

impl Interpreter {
    pub fn new(program: SemanticProgram) -> Self {
        Self::with_args(program, std::env::args().skip(1).collect())
    }

    /// Construct an interpreter with an explicit argument list.
    /// `new` calls this with `std::env::args().skip(1)`. Tests use
    /// this directly to inject known arguments without spawning a
    /// subprocess.
    pub fn with_args(program: SemanticProgram, args: Vec<String>) -> Self {
        Self {
            variables: HashMap::new(),
            output: Vec::new(),
            program,
            return_value: None,
            heap: HashMap::new(),
            next_ptr: 1,
            region_stack: Vec::new(),
            command_line_args: args,
        }
    }

    /// Public boundary. Converts `EvalError` into the `String`
    /// diagnostics the CLI expects. This is the *only* place in
    /// the interpreter that should convert.
    pub fn run(&mut self) -> Result<String, String> {
        let main_func = self
            .program
            .functions
            .iter()
            .find(|f| f.name == "main")
            .cloned()
            .ok_or_else(|| "No main function found".to_string())?;

        self.execute_function(&main_func)
            .map_err(|e| e.to_string())?;
        Ok(self.output.join("\n"))
    }

    pub(super) fn execute_function(&mut self, func: &SemanticFunction) -> Result<(), EvalError> {
        let mut current = func.entry_block;
        let mut iterations = 0;
        // Pending branches after a Fork. Each entry is
        // (remaining_branch_blocks, join_block). When a Jump targets
        // the top entry's join_block and there are more branches
        // queued, we run the next branch instead of the join.
        //
        // This models sequential execution of `parallel` blocks: the
        // interpreter does not spawn OS threads (see module doc).
        let mut pending_forks: Vec<(Vec<usize>, usize)> = Vec::new();

        loop {
            if iterations > 100_000_000 {
                return Err(EvalError::Runtime("infinite loop detected".into()));
            }
            iterations += 1;

            let block = func
                .blocks
                .iter()
                .find(|b| b.id == current)
                .ok_or_else(|| EvalError::Runtime(format!("block {} not found", current)))?
                .clone();

            for instr in &block.instructions {
                self.execute_instruction(instr)?;
            }

            match &block.terminator {
                Some(Terminator::Return { value, .. }) => {
                    if let Some(v) = value {
                        self.return_value = Some(self.eval_value(v)?);
                    }
                    // Any `region` blocks still open at return
                    // (from early-return paths) get cleaned up
                    // here. Region frames are function-local, so
                    // the caller's frame stack is untouched.
                    while let Some(frame) = self.region_stack.pop() {
                        for handle in frame.allocations {
                            self.heap.remove(&handle);
                        }
                    }
                    return Ok(());
                }
                Some(Terminator::Jump { block: target }) => {
                    let mut jumped_to_next_branch = false;
                    if let Some((remaining, fork_join)) = pending_forks.last_mut() {
                        if target == fork_join {
                            if !remaining.is_empty() {
                                let next = remaining.remove(0);
                                current = next;
                                jumped_to_next_branch = true;
                            } else {
                                pending_forks.pop();
                            }
                        }
                    }
                    if !jumped_to_next_branch {
                        current = *target;
                    }
                }
                Some(Terminator::Branch {
                    condition,
                    then_block,
                    else_block,
                }) => {
                    // NOTE: `as_bool` still coerces non-Bool silently.
                    // TypeCheckPass should guarantee Bool here; see
                    // PR-B for making `as_bool` fallible.
                    let cond = self.eval_value(condition)?.as_bool();
                    current = if cond { *then_block } else { *else_block };
                }
                Some(Terminator::IteratorNext {
                    iterator,
                    target,
                    body_block,
                    exit_block,
                }) => {
                    let idx_key = format!("{}_idx", iterator);
                    let current_idx = match self.variables.get(&idx_key) {
                        Some(RuntimeValue::Int(i)) => *i as usize,
                        Some(other) => {
                            return Err(EvalError::Runtime(format!(
                                "iterator index `{}` has wrong type at runtime: {}",
                                idx_key,
                                runtime::runtime_kind(other)
                            )));
                        }
                        None => {
                            return Err(EvalError::Runtime(format!(
                                "iterator index `{}` not found — `IteratorInit` \
                                 should have created it",
                                idx_key
                            )));
                        }
                    };

                    let iterable = self.variables.get(iterator).cloned().ok_or_else(|| {
                        EvalError::Runtime(format!(
                            "iterator `{}` not found — `IteratorInit` should \
                             have bound it",
                            iterator
                        ))
                    })?;

                    if let RuntimeValue::List(list) = iterable {
                        if current_idx < list.len() {
                            let value = list[current_idx].clone();
                            self.variables.insert(target.clone(), value);
                            self.variables
                                .insert(idx_key, RuntimeValue::Int((current_idx + 1) as i64));
                            current = *body_block;
                        } else {
                            current = *exit_block;
                        }
                    } else {
                        current = *exit_block;
                    }
                }
                Some(Terminator::Spawn { entry_block }) => {
                    current = *entry_block;
                }
                Some(Terminator::Fork { blocks, join_block }) => {
                    if let Some((first, rest)) = blocks.split_first() {
                        if !rest.is_empty() {
                            pending_forks.push((rest.to_vec(), *join_block));
                        }
                        current = *first;
                    } else {
                        current = *join_block;
                    }
                }
                Some(Terminator::Switch {
                    value,
                    cases,
                    default_block,
                }) => {
                    let val = self.eval_value(value)?;
                    let mut matched = false;

                    for (pattern, target) in cases {
                        if let Some(bindings) = self.try_pattern_match(pattern, &val)? {
                            for (name, bound_val) in bindings {
                                self.variables.insert(name, bound_val);
                            }
                            current = *target;
                            matched = true;
                            break;
                        }
                    }

                    if !matched {
                        if let Some(default) = default_block {
                            current = *default;
                        } else {
                            return Err(EvalError::Runtime(
                                "switch has no matching case and no default".into(),
                            ));
                        }
                    }
                }
                None => return Ok(()),
            }
        }
    }

    fn execute_instruction(&mut self, instr: &Instruction) -> Result<(), EvalError> {
        match instr {
            Instruction::Nop => {}

            Instruction::Declare { name, value, .. } => {
                let val = self.eval_value(value)?;
                self.variables.insert(name.clone(), val);
            }
            Instruction::Assign { target, value } => {
                let val = self.eval_value(value)?;
                self.variables.insert(target.clone(), val);
            }
            Instruction::Print { value } => {
                let val = self.eval_value(value)?;
                self.output.push(val.display());
            }
            Instruction::Call { func, args, result } => {
                let val = self.eval_call(func, args)?;
                if let Some(res_name) = result {
                    self.variables.insert(res_name.clone(), val);
                }
            }
            Instruction::ArrayAssign {
                array,
                index,
                value,
            } => {
                let arr_name = match array.as_ref() {
                    TypedIRValue::Variable(name, _) => name.clone(),
                    _ => {
                        return Err(EvalError::Unsupported {
                            construct: "ArrayAssign on non-variable",
                            hint: "array must be a plain variable name",
                        });
                    }
                };

                let idx = self.eval_value(index)?;
                let val = self.eval_value(value)?;

                let idx_usize = match idx {
                    RuntimeValue::Int(i) if i < 0 => {
                        return Err(EvalError::Runtime(format!(
                            "array assignment index {} is negative",
                            i
                        )));
                    }
                    RuntimeValue::Int(i) => i as usize,
                    other => {
                        return Err(EvalError::TypeMismatch {
                            op: "ArrayAssign.index",
                            left: runtime::runtime_kind(&other),
                            right: "Int",
                        });
                    }
                };

                if let Some(RuntimeValue::List(list)) = self.variables.get(&arr_name).cloned() {
                    let mut new_list = list;
                    if idx_usize < new_list.len() {
                        new_list[idx_usize] = val;
                        self.variables
                            .insert(arr_name, RuntimeValue::List(new_list));
                    } else {
                        return Err(EvalError::Runtime(format!(
                            "array assignment index {} out of bounds (length {})",
                            idx_usize,
                            new_list.len()
                        )));
                    }
                } else {
                    return Err(EvalError::Runtime(format!(
                        "ArrayAssign target `{}` is not a list",
                        arr_name
                    )));
                }
            }
            Instruction::IteratorInit { iterator, iterable } => {
                let val = self.eval_value(iterable)?;
                self.variables.insert(iterator.clone(), val);
                self.variables
                    .insert(format!("{}_idx", iterator), RuntimeValue::Int(0));
            }
            Instruction::Allocate { target, size, .. } => {
                let requested = match self.eval_value(size)? {
                    RuntimeValue::Int(i) if i > 0 => i as usize,
                    RuntimeValue::Float(f) if f > 0.0 => f as usize,
                    other => {
                        return Err(EvalError::TypeMismatch {
                            op: "Allocate.size",
                            left: runtime::runtime_kind(&other),
                            right: "positive Int",
                        });
                    }
                };
                let handle = self.next_ptr;
                self.next_ptr += 1;
                self.heap.insert(handle, vec![0u8; requested]);
                if let Some(frame) = self.region_stack.last_mut() {
                    frame.allocations.push(handle);
                }
                self.variables
                    .insert(target.clone(), RuntimeValue::Int(handle as i64));
            }
            Instruction::Free { ptr } => {
                let handle = match self.eval_value(ptr)? {
                    RuntimeValue::Int(h) if h > 0 => Some(h as usize),
                    RuntimeValue::Int(_) => None, // freeing null is a no-op
                    other => {
                        return Err(EvalError::TypeMismatch {
                            op: "Free.ptr",
                            left: runtime::runtime_kind(&other),
                            right: "Int (pointer handle)",
                        });
                    }
                };
                if let Some(h) = handle {
                    self.heap.remove(&h);
                }
            }
            Instruction::RegionEnter { name } => {
                self.region_stack.push(RegionFrame {
                    name: name.clone(),
                    allocations: Vec::new(),
                });
            }
            Instruction::RegionExit { name } => match self.region_stack.pop() {
                Some(frame) if frame.name == *name => {
                    for handle in frame.allocations {
                        self.heap.remove(&handle);
                    }
                }
                Some(frame) => {
                    return Err(EvalError::Runtime(format!(
                        "region exit mismatch: expected '{}', found '{}'",
                        name, frame.name
                    )));
                }
                None => {
                    return Err(EvalError::Runtime(format!(
                        "region exit '{}' with no matching enter",
                        name
                    )));
                }
            },
            Instruction::WriteReference { .. } => {
                // The interpreter does not model references. Its
                // `Assign` handler would silently overwrite the
                // reference variable's slot, which is wrong; refuse
                // instead. Matches the interpreter's refusal of the
                // reference-family `TypedIRValue` variants in
                // `eval_value`.
                return Err(EvalError::Unsupported {
                    construct: "write-through-&mut",
                    hint: "use the LLVM backend (--interpreter does not model references)",
                });
            }
            // ADR 0020. Channels are not modeled by any backend.
            // The capability check refuses any program that would
            // reach these arms — but they return `Unsupported`
            // anyway, as a defensive guard, matching the shape of
            // the `WriteReference` arm above. Listed explicitly
            // (rather than `_ =>`) so a future IR variant forces
            // a decision at compile time.
            Instruction::ChannelDecl { .. } => {
                return Err(EvalError::Unsupported {
                    construct: "channel declaration",
                    hint: "channels are not yet implemented in any backend",
                });
            }
            Instruction::SendChannel { .. } => {
                return Err(EvalError::Unsupported {
                    construct: "channel send",
                    hint: "channels are not yet implemented in any backend",
                });
            }
            Instruction::ReceiveChannel { .. } => {
                return Err(EvalError::Unsupported {
                    construct: "channel receive",
                    hint: "channels are not yet implemented in any backend",
                });
            }
        }
        Ok(())
    }
}
