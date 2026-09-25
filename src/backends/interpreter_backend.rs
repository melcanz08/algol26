// src/backends/interpreter_backend.rs
//
// `Backend` implementation that runs a VerifiedIR program through the
// tree-walking interpreter and captures its output.
use crate::backends::backend::{Backend, BackendOutput};
use crate::backends::interpreter::Interpreter;
use crate::common::diagnostics::{CompileError, ErrorCode, Result};
use crate::ir::verified_ir::VerifiedIR;
use std::sync::Mutex;

pub struct InterpreterBackend {
    output_buffer: Mutex<Vec<u8>>,
    /// Program arguments exposed to the interpreted program via
    /// `args()`. Empty by default; the CLI populates this from the
    /// post-`--` arguments on the command line. ADR 0023.
    program_args: Vec<String>,
}

impl InterpreterBackend {
    pub fn new() -> Self {
        Self::with_args(Vec::new())
    }

    /// Construct with an explicit program-argument list. `args()`
    /// inside the interpreted program returns this list.
    pub fn with_args(program_args: Vec<String>) -> Self {
        Self {
            output_buffer: Mutex::new(Vec::new()),
            program_args,
        }
    }

    pub fn get_output(&self) -> String {
        let buffer = self
            .output_buffer
            .lock()
            .expect("interpreter output lock poisoned");
        String::from_utf8_lossy(&buffer).to_string()
    }

    pub fn clear_output(&self) {
        let mut buffer = self
            .output_buffer
            .lock()
            .expect("interpreter output lock poisoned");
        buffer.clear();
    }
}

impl Default for InterpreterBackend {
    fn default() -> Self {
        Self::new()
    }
}

impl Backend for InterpreterBackend {
    fn compile(&self, ir: &VerifiedIR, _output_name: &str) -> Result<BackendOutput> {
        let mut interpreter =
            Interpreter::with_args(ir.program().clone(), self.program_args.clone());

        // Capture output from interpreter
        let output = interpreter.run().map_err(|e| {
            CompileError::simple(&format!("Runtime error: {}", e), 0, 0, "", ErrorCode::E0002)
        })?;

        let stdout = if output.is_empty() {
            String::new()
        } else {
            format!("{}\n", output)
        };

        // Keep the legacy buffer in sync so existing callers
        // using `get_output()` continue to work; the returned
        // enum now also carries the same string for new callers.
        let mut buffer = self
            .output_buffer
            .lock()
            .expect("interpreter output lock poisoned");
        buffer.clear();
        buffer.extend_from_slice(stdout.as_bytes());

        Ok(BackendOutput::InterpreterOutput { stdout })
    }

    fn name(&self) -> &str {
        "interpreter"
    }
    fn description(&self) -> &str {
        "Interprets SemanticProgram with output capture"
    }
    fn can_execute(&self) -> bool {
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::common::types::Type;
    use crate::ir::semantic_ir::{
        Instruction, SemanticBlock, SemanticFunction, SemanticProgram, Terminator, TypedIRValue,
    };
    use crate::ir::verified_ir::VerifiedIR;

    #[test]
    fn test_interpreter_allocate_and_free() {
        // Step 2 wiring: the interpreter now handles
        // `Instruction::Allocate` and `Instruction::Free`
        // against a simulated heap.
        use crate::common::types::Type;
        use crate::ir::semantic_ir::{
            Instruction, SemanticBlock, SemanticFunction, SemanticProgram, Terminator, TypedIRValue,
        };
        use crate::ir::verified_ir::VerifiedIR;

        let mut program = SemanticProgram::new();
        let entry = program.new_block_id();

        let func = SemanticFunction {
            name: "main".to_string(),
            params: vec![],
            return_type: Type::Void,
            blocks: vec![SemanticBlock {
                id: entry,
                instructions: vec![
                    Instruction::Allocate {
                        target: "p".to_string(),
                        size: TypedIRValue::Int(8),
                        type_: Type::pointer(Type::Unknown),
                    },
                    Instruction::Free {
                        ptr: TypedIRValue::Variable("p".to_string(), Type::pointer(Type::Unknown)),
                    },
                    Instruction::Print {
                        value: TypedIRValue::String("ok".to_string()),
                    },
                ],
                terminator: Some(Terminator::Return {
                    value: None,
                    type_: Type::Void,
                }),
            }],
            entry_block: entry,
            is_extern: false,
        };
        program.functions.push(func);

        let verified = VerifiedIR::new(program).expect("IR verification failed");
        let backend = InterpreterBackend::new();
        let result = backend.compile(&verified, "test");
        assert!(
            result.is_ok(),
            "interpreter should handle alloc/free: {:?}",
            result
        );
        assert_eq!(backend.get_output(), "ok\n");
    }

    #[test]
    fn test_interpreter_reads_file() {
        use crate::backends::backend::Backend;
        use crate::compiler::Compiler;

        let dir =
            std::env::temp_dir().join(format!("algol26_file_read_test_{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("create temp dir");
        let input_path = dir.join("input.txt");
        std::fs::write(&input_path, "hello from disk").expect("write fixture");

        let source = format!(
            "procedure main\n    val text := File.read(\"{}\")\n    print(text)\n",
            input_path.display()
        );

        let mut c = Compiler::new();
        let verified = c
            .run_pipeline_for(&source, "file_read.gol")
            .expect("pipeline should reach verified IR");

        let backend = crate::backends::interpreter_backend::InterpreterBackend::new();
        backend
            .compile(&verified, "")
            .expect("interpreter should run");
        let output = backend.get_output();
        assert_eq!(output.trim(), "hello from disk");

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_interpreter_writes_file() {
        use crate::backends::backend::Backend;
        use crate::compiler::Compiler;

        let dir =
            std::env::temp_dir().join(format!("algol26_file_write_test_{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("create temp dir");
        let out_path = dir.join("output.txt");

        let source = format!(
            "procedure main\n    val n := File.write(\"{}\", \"written\")\n    print(n)\n",
            out_path.display()
        );

        let mut c = Compiler::new();
        let verified = c
            .run_pipeline_for(&source, "file_write.gol")
            .expect("pipeline should reach verified IR");

        let backend = crate::backends::interpreter_backend::InterpreterBackend::new();
        backend
            .compile(&verified, "")
            .expect("interpreter should run");
        let output = backend.get_output();
        assert_eq!(output.trim(), "7"); // "written".len()

        let on_disk = std::fs::read_to_string(&out_path).expect("output file should exist");
        assert_eq!(on_disk, "written");

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_interpreter_region_frees_allocation_on_exit() {
        // Step 3 wiring: `region r` opens a frame; `alloc(n)`
        // inside records its handle; `RegionExit` frees it.
        // The program prints inside the region, then outside,
        // and both prints succeed.
        use crate::common::types::Type;
        use crate::ir::semantic_ir::{
            Instruction, SemanticBlock, SemanticFunction, SemanticProgram, Terminator, TypedIRValue,
        };
        use crate::ir::verified_ir::VerifiedIR;

        let mut program = SemanticProgram::new();
        let entry = program.new_block_id();

        let func = SemanticFunction {
            name: "main".to_string(),
            params: vec![],
            return_type: Type::Void,
            blocks: vec![SemanticBlock {
                id: entry,
                instructions: vec![
                    Instruction::RegionEnter {
                        name: "r".to_string(),
                    },
                    Instruction::Allocate {
                        target: "p".to_string(),
                        size: TypedIRValue::Int(8),
                        type_: Type::pointer(Type::Unknown),
                    },
                    Instruction::Print {
                        value: TypedIRValue::String("inside".to_string()),
                    },
                    Instruction::RegionExit {
                        name: "r".to_string(),
                    },
                    Instruction::Print {
                        value: TypedIRValue::String("outside".to_string()),
                    },
                ],
                terminator: Some(Terminator::Return {
                    value: None,
                    type_: Type::Void,
                }),
            }],
            entry_block: entry,
            is_extern: false,
        };
        program.functions.push(func);

        let verified = VerifiedIR::new(program).expect("IR verification failed");
        let backend = InterpreterBackend::new();
        let result = backend.compile(&verified, "test");
        assert!(result.is_ok(), "region test failed: {:?}", result);
        assert_eq!(backend.get_output(), "inside\noutside\n");
    }

    #[test]
    fn test_interpreter_captures_output() {
        let mut program = SemanticProgram::new();
        let entry = program.new_block_id();

        let func = SemanticFunction {
            name: "main".to_string(),
            params: vec![],
            return_type: Type::Void,
            blocks: vec![SemanticBlock {
                id: entry,
                instructions: vec![Instruction::Print {
                    value: TypedIRValue::String("Hello".to_string()),
                }],
                terminator: Some(Terminator::Return {
                    value: None,
                    type_: Type::Void,
                }),
            }],
            entry_block: entry,
            is_extern: false,
        };
        program.functions.push(func);

        let verified = VerifiedIR::new(program).expect("IR verification failed in test");
        let backend = InterpreterBackend::new();
        let result = backend.compile(&verified, "test");

        assert!(result.is_ok());
        assert_eq!(backend.get_output(), "Hello\n");
    }

    #[test]
    fn test_affirm_passing_continues() {
        use crate::backends::backend::Backend;
        use crate::compiler::Compiler;

        let source = r#"
procedure main
    affirm(1 < 2, "one less than two")
    print("ok")
"#;

        let mut c = Compiler::new();
        let verified = c
            .run_pipeline_for(source, "affirm_pass.gol")
            .expect("pipeline should reach verified IR");
        let backend = crate::backends::interpreter_backend::InterpreterBackend::new();
        backend
            .compile(&verified, "")
            .expect("interpreter should run");
        assert_eq!(backend.get_output().trim(), "ok");
    }

    #[test]
    fn test_affirm_failing_errors() {
        use crate::backends::backend::Backend;
        use crate::compiler::Compiler;

        let source = r#"
procedure main
    affirm(1 > 2, "one is not greater than two")
"#;

        let mut c = Compiler::new();
        let verified = c
            .run_pipeline_for(source, "affirm_fail.gol")
            .expect("pipeline should reach verified IR");
        let backend = crate::backends::interpreter_backend::InterpreterBackend::new();
        let err = backend
            .compile(&verified, "")
            .expect_err("failing affirm should error");
        let msg = format!("{}", err);
        assert!(
            msg.contains("assertion failed") && msg.contains("one is not greater than two"),
            "unexpected message: {}",
            msg
        );
    }
    #[test]
    fn test_interpreter_args_returns_injected_list() {
        use crate::backends::interpreter::Interpreter;
        use crate::compiler::Compiler;

        let source = r#"
procedure main
    val xs := args()
    print(List.length(xs))
    for a in xs
        print(a)
"#;

        let mut c = Compiler::new();
        let verified = c
            .run_pipeline_for(source, "args_test.gol")
            .expect("pipeline should reach verified IR");

        let program = verified.program().clone();
        let mut interp =
            Interpreter::with_args(program, vec!["hello".to_string(), "world".to_string()]);
        let output = interp.run().expect("interpreter should run");
        assert_eq!(output.trim(), "2\nhello\nworld");
    }
    #[test]
    fn test_interpreter_backend_passes_args_through() {
        use crate::backends::backend::Backend;
        use crate::compiler::Compiler;

        let source = r#"
procedure main
    val xs := args()
    print(List.length(xs))
    for a in xs
        print(a)
"#;

        let mut c = Compiler::new();
        let verified = c
            .run_pipeline_for(source, "args_backend.gol")
            .expect("pipeline should reach verified IR");

        let backend =
            InterpreterBackend::with_args(vec!["first".to_string(), "second".to_string()]);
        backend
            .compile(&verified, "")
            .expect("interpreter should run");
        assert_eq!(backend.get_output().trim(), "2\nfirst\nsecond");
    }
}
