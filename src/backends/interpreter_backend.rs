// src/backends/interpreter_backend.rs - HARDENED
use crate::backends::backend::{Backend, BackendOutput};
use crate::backends::interpreter::Interpreter;
use crate::common::diagnostics::{CompileError, ErrorCode, Result};
use crate::ir::verified_ir::VerifiedIR;
use std::sync::Mutex;

pub struct InterpreterBackend {
    output_buffer: Mutex<Vec<u8>>,
}

impl InterpreterBackend {
    pub fn new() -> Self {
        Self {
            output_buffer: Mutex::new(Vec::new()),
        }
    }

    pub fn get_output(&self) -> String {
        let buffer = self.output_buffer.lock().expect("interpreter output lock poisoned");
        String::from_utf8_lossy(&buffer).to_string()
    }

    pub fn clear_output(&self) {
        let mut buffer = self.output_buffer.lock().expect("interpreter output lock poisoned");
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
        let mut interpreter = Interpreter::new(ir.program().clone());

        // Capture output from interpreter
        let output = interpreter.run().map_err(|e| {
            CompileError::simple(
                &format!("Runtime error: {:?}", e),
                0,
                0,
                "",
                ErrorCode::E0002,
            )
        })?;

        // Store output
        let mut buffer = self.output_buffer.lock().expect("interpreter output lock poisoned");
        if output.is_empty() {
            buffer.clear();
        } else {
            *buffer = format!("{}\n", output).into_bytes();
        }

        Ok(BackendOutput::InterpreterOutput)
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
}
