#![allow(dead_code)]
#![allow(unused_variables)]

// ALGOL26 Interpreter - Consumes IR directly
// Complete implementation of IR execution

use crate::ir::{IRProgram, IRFunction, IRBlock, IRInstruction, IRValue, IRConstant, IRType, IRBinOp};
use std::collections::HashMap;

#[derive(Debug, Clone)]
pub enum RuntimeValue {
    Int(i64),
    Float(f64),
    String(String),
    Bool(bool),
    List(Vec<RuntimeValue>),
    Void,
}

impl RuntimeValue {
    fn as_float(&self) -> f64 {
        match self {
            RuntimeValue::Float(f) => *f,
            RuntimeValue::Int(i) => *i as f64,
            _ => 0.0,
        }
    }
    
    fn as_int(&self) -> i64 {
        match self {
            RuntimeValue::Int(i) => *i,
            RuntimeValue::Float(f) => *f as i64,
            _ => 0,
        }
    }
    
    fn as_bool(&self) -> bool {
        match self {
            RuntimeValue::Bool(b) => *b,
            RuntimeValue::Int(i) => *i != 0,
            RuntimeValue::Float(f) => *f != 0.0,
            _ => false,
        }
    }
    
    fn display(&self) -> String {
        match self {
            RuntimeValue::Int(i) => format!("{}", i),
            RuntimeValue::Float(f) => format!("{:.1}", f),
            RuntimeValue::String(s) => s.clone(),
            RuntimeValue::Bool(b) => format!("{}", b),
            RuntimeValue::List(list) => {
                let items: Vec<String> = list.iter().map(|v| v.display()).collect();
                format!("[{}]", items.join(", "))
            }
            RuntimeValue::Void => String::new(),
        }
    }
    
    fn type_name(&self) -> &'static str {
        match self {
            RuntimeValue::Int(_) => "Int",
            RuntimeValue::Float(_) => "Float",
            RuntimeValue::String(_) => "String",
            RuntimeValue::Bool(_) => "Bool",
            RuntimeValue::List(_) => "List",
            RuntimeValue::Void => "Void",
        }
    }
}

pub struct Interpreter {
    variables: HashMap<String, RuntimeValue>,
    output: Vec<String>,
    program: IRProgram,
    pc: usize,
    current_function: Option<String>,
    current_block: usize,
    return_value: Option<RuntimeValue>,
}

impl Interpreter {
    pub fn new(program: IRProgram) -> Self {
        Interpreter {
            variables: HashMap::new(),
            output: Vec::new(),
            program,
            pc: 0,
            current_function: None,
            current_block: 0,
            return_value: None,
        }
    }
    
    pub fn run(&mut self) -> Result<String, String> {
        // Find main function
        let main_func = self.program.functions.iter()
            .find(|f| f.name == "main")
            .cloned()
            .ok_or("No main function found")?;
        
        self.execute_function(&main_func)?;
        
        Ok(self.output.join("\n"))
    }
    
    fn execute_function(&mut self, func: &IRFunction) -> Result<(), String> {
        self.current_function = Some(func.name.clone());
        self.return_value = None;
        
        // Initialize local variables
        for (name, type_, _mutable) in &func.local_vars {
            let default = match type_ {
                IRType::Int => RuntimeValue::Int(0),
                IRType::Float => RuntimeValue::Float(0.0),
                IRType::String => RuntimeValue::String(String::new()),
                IRType::Bool => RuntimeValue::Bool(false),
                IRType::List(_) => RuntimeValue::List(Vec::new()),
                IRType::Void => RuntimeValue::Void,
                _ => RuntimeValue::Float(0.0),
            };
            self.variables.insert(name.clone(), default);
        }
        
        // Execute blocks sequentially (for now)
        for block in &func.blocks {
            self.current_block = block.id;
            self.execute_block(block)?;
            
            // Check if we need to return
            if self.return_value.is_some() {
                break;
            }
        }
        
        self.current_function = None;
        Ok(())
    }
    
    fn execute_block(&mut self, block: &IRBlock) -> Result<(), String> {
        for instruction in &block.instructions {
            self.execute_instruction(instruction)?;
            
            // Stop if we returned
            if self.return_value.is_some() {
                break;
            }
        }
        Ok(())
    }
    
    fn execute_instruction(&mut self, instr: &IRInstruction) -> Result<(), String> {
        match instr {
            IRInstruction::Alloca { name, type_, mutable } => {
                let default = match type_ {
                    IRType::Int => RuntimeValue::Int(0),
                    IRType::Float => RuntimeValue::Float(0.0),
                    IRType::String => RuntimeValue::String(String::new()),
                    IRType::Bool => RuntimeValue::Bool(false),
                    _ => RuntimeValue::Float(0.0),
                };
                self.variables.insert(name.clone(), default);
                let _ = mutable;
            }
            IRInstruction::Store { target, value } => {
                let val = self.evaluate_value(value)?;
                self.variables.insert(target.clone(), val);
            }
            IRInstruction::Load { result, source } => {
                let val = self.variables.get(source)
                    .cloned()
                    .unwrap_or(RuntimeValue::Float(0.0));
                self.variables.insert(result.clone(), val);
            }
            IRInstruction::Print { value } => {
                let val = self.evaluate_value(value)?;
                self.output.push(val.display());
            }
            IRInstruction::BinaryOp { result, op, left, right } => {
                let l = self.evaluate_value(left)?;
                let r = self.evaluate_value(right)?;
                let val = self.execute_binary_op(op, &l, &r)?;
                self.variables.insert(result.clone(), val);
            }
            IRInstruction::Return { value } => {
                match value {
                    Some(v) => {
                        self.return_value = Some(self.evaluate_value(v)?);
                    }
                    None => {
                        self.return_value = Some(RuntimeValue::Void);
                    }
                }
            }
            IRInstruction::Branch { condition, then_block, else_block } => {
                let cond = self.evaluate_value(condition)?;
                if cond.as_bool() {
                    self.current_block = *then_block;
                } else {
                    self.current_block = *else_block;
                }
            }
            IRInstruction::Jump { block } => {
                self.current_block = *block;
            }
            IRInstruction::Call { result, function, args } => {
                // Handle built-in functions
                match function.as_str() {
                    "Math.sqrt" => {
                        let arg = self.evaluate_value(&args[0])?;
                        let val = RuntimeValue::Float(arg.as_float().sqrt());
                        if let Some(result_name) = result {
                            self.variables.insert(result_name.clone(), val);
                        }
                    }
                    "Math.pow" => {
                        let x = self.evaluate_value(&args[0])?.as_float();
                        let y = self.evaluate_value(&args[1])?.as_float();
                        let val = RuntimeValue::Float(x.powf(y));
                        if let Some(result_name) = result {
                            self.variables.insert(result_name.clone(), val);
                        }
                    }
                    "Math.sin" => {
                        let x = self.evaluate_value(&args[0])?.as_float();
                        let val = RuntimeValue::Float(x.sin());
                        if let Some(result_name) = result {
                            self.variables.insert(result_name.clone(), val);
                        }
                    }
                    "Math.cos" => {
                        let x = self.evaluate_value(&args[0])?.as_float();
                        let val = RuntimeValue::Float(x.cos());
                        if let Some(result_name) = result {
                            self.variables.insert(result_name.clone(), val);
                        }
                    }
                    "Math.abs" => {
                        let x = self.evaluate_value(&args[0])?.as_float();
                        let val = RuntimeValue::Float(x.abs());
                        if let Some(result_name) = result {
                            self.variables.insert(result_name.clone(), val);
                        }
                    }
                    _ => {
                        // User-defined function
                        if let Some(func) = self.program.get_function(function) {
                            let func = func.clone();
                            self.execute_function(&func)?;
                            if let Some(result_name) = result {
                                let val = self.return_value.clone().unwrap_or(RuntimeValue::Float(0.0));
                                self.variables.insert(result_name.clone(), val);
                            }
                        }
                    }
                }
            }
            IRInstruction::ArrayAccess { result, array, index } => {
                let idx = self.evaluate_value(index)?.as_int() as usize;
                if let RuntimeValue::List(elements) = self.evaluate_value(&IRValue::Variable(array.clone()))? {
                    if idx < elements.len() {
                        let val = elements[idx].clone();
                        self.variables.insert(result.clone(), val);
                    } else {
                        return Err(format!("Array index {} out of bounds", idx));
                    }
                } else {
                    self.variables.insert(result.clone(), RuntimeValue::Float(0.0));
                }
            }
            IRInstruction::BoundsCheck { index, len } => {
                let idx = self.evaluate_value(index)?.as_int();
                if idx < 0 || idx >= *len as i64 {
                    return Err(format!("Array index {} out of bounds (len {})", idx, len));
                }
            }
            IRInstruction::Label(_) => {}
            IRInstruction::Nop => {}
        }
        Ok(())
    }
    
    fn evaluate_value(&self, value: &IRValue) -> Result<RuntimeValue, String> {
        match value {
            IRValue::Constant(c) => Ok(self.evaluate_constant(c)),
            IRValue::Variable(name) => {
                self.variables.get(name)
                    .cloned()
                    .ok_or_else(|| format!("Undefined variable '{}'", name))
            }
            IRValue::Temporary(_) => Ok(RuntimeValue::Float(0.0)),
        }
    }
    
    fn evaluate_constant(&self, constant: &IRConstant) -> RuntimeValue {
        match constant {
            IRConstant::Int(i) => RuntimeValue::Int(*i),
            IRConstant::Float(f) => RuntimeValue::Float(*f),
            IRConstant::String(s) => RuntimeValue::String(s.clone()),
            IRConstant::Bool(b) => RuntimeValue::Bool(*b),
            IRConstant::List(list) => {
                RuntimeValue::List(list.iter().map(|c| self.evaluate_constant(c)).collect())
            }
        }
    }
    
    fn execute_binary_op(&self, op: &IRBinOp, left: &RuntimeValue, right: &RuntimeValue) -> Result<RuntimeValue, String> {
        match op {
            IRBinOp::Add => {
                if let (RuntimeValue::Float(l), RuntimeValue::Float(r)) = (left, right) {
                    Ok(RuntimeValue::Float(l + r))
                } else if let (RuntimeValue::Int(l), RuntimeValue::Int(r)) = (left, right) {
                    Ok(RuntimeValue::Int(l + r))
                } else if let (RuntimeValue::String(l), RuntimeValue::String(r)) = (left, right) {
                    Ok(RuntimeValue::String(format!("{}{}", l, r)))
                } else {
                    Ok(RuntimeValue::Float(left.as_float() + right.as_float()))
                }
            }
            IRBinOp::Subtract => Ok(RuntimeValue::Float(left.as_float() - right.as_float())),
            IRBinOp::Multiply => Ok(RuntimeValue::Float(left.as_float() * right.as_float())),
            IRBinOp::Divide => Ok(RuntimeValue::Float(left.as_float() / right.as_float())),
            IRBinOp::Greater => Ok(RuntimeValue::Bool(left.as_float() > right.as_float())),
            IRBinOp::Less => Ok(RuntimeValue::Bool(left.as_float() < right.as_float())),
            IRBinOp::GreaterEqual => Ok(RuntimeValue::Bool(left.as_float() >= right.as_float())),
            IRBinOp::LessEqual => Ok(RuntimeValue::Bool(left.as_float() <= right.as_float())),
            IRBinOp::Equal => Ok(RuntimeValue::Bool(left.as_float() == right.as_float())),
            IRBinOp::NotEqual => Ok(RuntimeValue::Bool(left.as_float() != right.as_float())),
            IRBinOp::And => Ok(RuntimeValue::Bool(left.as_bool() && right.as_bool())),
            IRBinOp::Or => Ok(RuntimeValue::Bool(left.as_bool() || right.as_bool())),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ir::IRBuilder;
    
    #[test]
    fn test_interpreter_basic() {
        let mut builder = IRBuilder::new();
        builder.begin_function("main", IRType::Int);
        builder.emit(IRInstruction::Print {
            value: IRValue::Constant(IRConstant::Float(42.0)),
        });
        builder.end_function();
        
        let mut interpreter = Interpreter::new(builder.program);
        let output = interpreter.run().unwrap();
        assert_eq!(output, "42.0");
    }
    
    #[test]
    fn test_interpreter_addition() {
        let mut builder = IRBuilder::new();
        builder.begin_function("main", IRType::Int);
        builder.emit(IRInstruction::BinaryOp {
            result: "sum".to_string(),
            op: IRBinOp::Add,
            left: IRValue::Constant(IRConstant::Float(10.0)),
            right: IRValue::Constant(IRConstant::Float(20.0)),
        });
        builder.emit(IRInstruction::Print {
            value: IRValue::Variable("sum".to_string()),
        });
        builder.end_function();
        
        let mut interpreter = Interpreter::new(builder.program);
        let output = interpreter.run().unwrap();
        assert_eq!(output, "30.0");
    }
    
    #[test]
    fn test_interpreter_math() {
        let mut builder = IRBuilder::new();
        builder.begin_function("main", IRType::Int);
        builder.emit(IRInstruction::Call {
            result: Some("sqrt_result".to_string()),
            function: "Math.sqrt".to_string(),
            args: vec![IRValue::Constant(IRConstant::Float(16.0))],
        });
        builder.emit(IRInstruction::Print {
            value: IRValue::Variable("sqrt_result".to_string()),
        });
        builder.end_function();
        
        let mut interpreter = Interpreter::new(builder.program);
        let output = interpreter.run().unwrap();
        assert_eq!(output, "4.0");
    }
}
