#![allow(dead_code)]
#![allow(unused_variables)]

// ALGOL26 Interpreter - Consumes SemanticProgram (New IR)

use crate::ir::semantic_ir::{
    SemanticProgram, SemanticFunction, SemanticBlock, SemanticInstruction,
    TypedIRValue, SemanticBinOp,
};
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
}

pub struct Interpreter {
    variables: HashMap<String, RuntimeValue>,
    output: Vec<String>,
    program: SemanticProgram,
    return_value: Option<RuntimeValue>,
}

impl Interpreter {
    pub fn new(program: SemanticProgram) -> Self {
        Interpreter {
            variables: HashMap::new(),
            output: Vec::new(),
            program,
            return_value: None,
        }
    }
    
    pub fn run(&mut self) -> crate::common::diagnostics::Result<String> {
        let main_func = self.program.functions.iter()
            .find(|f| f.name == "main")
            .cloned()
            .ok_or("No main function found")?;
        
        self.execute_function(&main_func)?;
        Ok(self.output.join("\n"))
    }
    
    fn execute_function(&mut self, func: &SemanticFunction) -> Result<(), String> {
        self.return_value = None;
        
        for block in &func.blocks {
            self.execute_block(block)?;
            if self.return_value.is_some() {
                break;
            }
        }
        
        Ok(())
    }
    
    fn execute_block(&mut self, block: &SemanticBlock) -> Result<(), String> {
        for instruction in &block.instructions {
            self.execute_instruction(instruction)?;
            if self.return_value.is_some() {
                break;
            }
        }
        Ok(())
    }
    
    fn execute_instruction(&mut self, instr: &SemanticInstruction) -> Result<(), String> {
        match instr {
            SemanticInstruction::Nop => {}
            
            SemanticInstruction::Declare { name, mutable: _, type_: _, value } => {
                let val = self.evaluate_value(value)?;
                self.variables.insert(name.clone(), val);
            }
            
            SemanticInstruction::Assign { target, value } => {
                let val = self.evaluate_value(value)?;
                self.variables.insert(target.clone(), val);
            }
            
            SemanticInstruction::Print { value } => {
                let val = self.evaluate_value(value)?;
                self.output.push(val.display());
            }
            
            SemanticInstruction::Return { value, type_: _ } => {
                match value {
                    Some(v) => self.return_value = Some(self.evaluate_value(v)?),
                    None => self.return_value = Some(RuntimeValue::Void),
                }
            }
            
            SemanticInstruction::Branch { condition, then_block: _, else_block: _ } => {
                let _ = self.evaluate_value(condition)?;
            }
            
            SemanticInstruction::Jump { block: _ } => {}
            
            SemanticInstruction::Switch { value, cases, default_block: _ } => {
                let _ = self.evaluate_value(value)?;
                let _ = cases;
            }
            
            SemanticInstruction::Call { result, function, args, return_type: _ } => {
                let arg_vals: Vec<RuntimeValue> = args.iter()
                    .map(|a| self.evaluate_value(a))
                    .collect::<Result<Vec<_>, String>>()?;
                
                let call_result = self.execute_call(function, &arg_vals)?;
                
                if let Some(result_name) = result {
                    self.variables.insert(result_name.clone(), call_result);
                }
            }
            
            SemanticInstruction::MethodCall { result, receiver, receiver_type, method_name, args, return_type } => {
                let receiver_val = self.evaluate_value(receiver)?;
                let mut all_args = vec![receiver_val];
                for arg in args {
                    all_args.push(self.evaluate_value(arg)?);
                }
                
                let function_name = format!("{}_{}", receiver_type, method_name);
                let call_result = self.execute_call(&function_name, &all_args)?;
                
                if let Some(result_name) = result {
                    self.variables.insert(result_name.clone(), call_result);
                }
            }
            
            SemanticInstruction::IteratorInit { iterator, iterable } => {
                let val = self.evaluate_value(iterable)?;
                self.variables.insert(iterator.clone(), val);
            }
            
            SemanticInstruction::IteratorNext { iterator: _, target: _, body_block: _, exit_block: _ } => {}
            
            SemanticInstruction::Spawn { entry_block: _ } => {}
            
            SemanticInstruction::Fork { blocks: _, join_block: _ } => {}
            
            SemanticInstruction::Defer { cleanup_block: _ } => {}
            
            SemanticInstruction::ChannelDecl { name, type_: _ } => {
                self.variables.entry(name.clone()).or_insert(RuntimeValue::Void);
            }
            
            SemanticInstruction::Send { channel, value } => {
                let val = self.evaluate_value(value)?;
                self.variables.insert(channel.clone(), val);
            }
            
            SemanticInstruction::Receive { channel, target } => {
                let val = self.variables.get(channel)
                    .cloned()
                    .unwrap_or(RuntimeValue::Void);
                self.variables.insert(target.clone(), val);
            }
            
            SemanticInstruction::ArrayAssign { array, index, value } => {
                let _ = self.evaluate_value(array)?;
                let _ = self.evaluate_value(index)?;
                let _ = self.evaluate_value(value)?;
            }
        }
        Ok(())
    }
    
    fn execute_call(&mut self, function: &str, args: &[RuntimeValue]) -> Result<RuntimeValue, String> {
        match function {
            // Math functions
            "Math.sqrt" => Ok(RuntimeValue::Float(args.first().map(|a| a.as_float()).unwrap_or(0.0).sqrt())),
            "Math.pow" => {
                let x = args.first().map(|a| a.as_float()).unwrap_or(0.0);
                let y = args.get(1).map(|a| a.as_float()).unwrap_or(0.0);
                Ok(RuntimeValue::Float(x.powf(y)))
            }
            "Math.sin" => Ok(RuntimeValue::Float(args.first().map(|a| a.as_float()).unwrap_or(0.0).sin())),
            "Math.cos" => Ok(RuntimeValue::Float(args.first().map(|a| a.as_float()).unwrap_or(0.0).cos())),
            "Math.abs" => Ok(RuntimeValue::Float(args.first().map(|a| a.as_float()).unwrap_or(0.0).abs())),
            "Math.floor" => Ok(RuntimeValue::Float(args.first().map(|a| a.as_float()).unwrap_or(0.0).floor())),
            "Math.ceil" => Ok(RuntimeValue::Float(args.first().map(|a| a.as_float()).unwrap_or(0.0).ceil())),
            "Math.exp" => Ok(RuntimeValue::Float(args.first().map(|a| a.as_float()).unwrap_or(0.0).exp())),
            "Math.log" => Ok(RuntimeValue::Float(args.first().map(|a| a.as_float()).unwrap_or(0.0).ln())),
            "Math.tan" => Ok(RuntimeValue::Float(args.first().map(|a| a.as_float()).unwrap_or(0.0).tan())),
            
            // String functions
            "String.length" => {
                let s = args.first().map(|a| a.display()).unwrap_or_default();
                Ok(RuntimeValue::Int(s.len() as i64))
            }
            "String.concat" => {
                let s1 = args.first().map(|a| a.display()).unwrap_or_default();
                let s2 = args.get(1).map(|a| a.display()).unwrap_or_default();
                Ok(RuntimeValue::String(format!("{}{}", s1, s2)))
            }
            "String.to_upper" => {
                let s = args.first().map(|a| a.display()).unwrap_or_default();
                Ok(RuntimeValue::String(s.to_uppercase()))
            }
            "String.to_lower" => {
                let s = args.first().map(|a| a.display()).unwrap_or_default();
                Ok(RuntimeValue::String(s.to_lowercase()))
            }
            "String.substring" => {
                let s = args.first().map(|a| a.display()).unwrap_or_default();
                let start = args.get(1).map(|a| a.as_int()).unwrap_or(0) as usize;
                let len = args.get(2).map(|a| a.as_int()).unwrap_or(0) as usize;
                if start < s.len() {
                    let end = (start + len).min(s.len());
                    Ok(RuntimeValue::String(s[start..end].to_string()))
                } else {
                    Ok(RuntimeValue::String(String::new()))
                }
            }
            
            // List functions
            "List.length" => {
                if let Some(RuntimeValue::List(elements)) = args.first() {
                    Ok(RuntimeValue::Float(elements.len() as f64))
                } else {
                    Ok(RuntimeValue::Float(0.0))
                }
            }
            "List.sum" => {
                if let Some(RuntimeValue::List(elements)) = args.first() {
                    Ok(RuntimeValue::Float(elements.iter().map(|e| e.as_float()).sum()))
                } else {
                    Ok(RuntimeValue::Float(0.0))
                }
            }
            "List.max" => {
                if let Some(RuntimeValue::List(elements)) = args.first() {
                    let max = elements.iter().map(|e| e.as_float()).fold(f64::NEG_INFINITY, f64::max);
                    Ok(RuntimeValue::Float(max))
                } else {
                    Ok(RuntimeValue::Float(0.0))
                }
            }
            "List.min" => {
                if let Some(RuntimeValue::List(elements)) = args.first() {
                    let min = elements.iter().map(|e| e.as_float()).fold(f64::INFINITY, f64::min);
                    Ok(RuntimeValue::Float(min))
                } else {
                    Ok(RuntimeValue::Float(0.0))
                }
            }
            
            // User-defined function
            _ => {
                if let Some(func) = self.program.functions.iter().find(|f| f.name == function).cloned() {
                    let saved_vars = self.variables.clone();
                    
                    for (i, (param_name, _)) in func.params.iter().enumerate() {
                        let val = args.get(i).cloned().unwrap_or(RuntimeValue::Float(0.0));
                        self.variables.insert(param_name.clone(), val);
                    }
                    
                    self.execute_function(&func)?;
                    
                    let result = self.return_value.clone().unwrap_or(RuntimeValue::Void);
                    self.variables = saved_vars;
                    
                    Ok(result)
                } else {
                    Err(format!("Undefined function '{}'", function))
                }
            }
        }
    }
    
    fn evaluate_value(&mut self, value: &TypedIRValue) -> Result<RuntimeValue, String> {
        match value {
            TypedIRValue::Int(i) => Ok(RuntimeValue::Int(*i)),
            TypedIRValue::Float(f) => Ok(RuntimeValue::Float(*f)),
            TypedIRValue::String(s) => Ok(RuntimeValue::String(s.clone())),
            TypedIRValue::Bool(b) => Ok(RuntimeValue::Bool(*b)),
            TypedIRValue::List(values) => {
                let mut list = Vec::new();
                for v in values {
                    list.push(self.evaluate_value(v)?);
                }
                Ok(RuntimeValue::List(list))
            }
            TypedIRValue::Some(v) => self.evaluate_value(v),
            TypedIRValue::None => Ok(RuntimeValue::Void),
            TypedIRValue::Ok(v) => self.evaluate_value(v),
            TypedIRValue::Error(v) => self.evaluate_value(v),
            TypedIRValue::Variable(name, _) => {
                self.variables.get(name)
                    .cloned()
                    .ok_or_else(|| format!("Undefined variable '{}'", name))
            }
            TypedIRValue::Cast { value, .. } => self.evaluate_value(value),
            TypedIRValue::BinaryOp { op, left, right, .. } => {
                let l = self.evaluate_value(left)?;
                let r = self.evaluate_value(right)?;
                self.execute_binary_op(op, &l, &r)
            }
            TypedIRValue::Call { function, args, .. } => {
                let arg_vals: Vec<RuntimeValue> = args.iter()
                    .map(|a| self.evaluate_value(a))
                    .collect::<Result<Vec<_>, String>>()?;
                self.execute_call(function, &arg_vals)
            }
            TypedIRValue::ArrayAccess { array, index, .. } => {
                let arr = self.evaluate_value(array)?;
                let idx = self.evaluate_value(index)?.as_int() as usize;
                if let RuntimeValue::List(elements) = arr {
                    if idx < elements.len() {
                        Ok(elements[idx].clone())
                    } else {
                        Err(format!("Array index {} out of bounds", idx))
                    }
                } else {
                    Ok(RuntimeValue::Float(0.0))
                }
            }
            TypedIRValue::MethodCall { receiver, receiver_type, method_name, args, return_type: _ } => {
                let receiver_val = self.evaluate_value(receiver)?;
                let mut all_args = vec![receiver_val];
                for arg in args {
                    all_args.push(self.evaluate_value(arg)?);
                }
                
                let function_name = format!("{}_{}", receiver_type, method_name);
                self.execute_call(&function_name, &all_args)
            }
        }
    }
    
    fn execute_binary_op(&self, op: &SemanticBinOp, left: &RuntimeValue, right: &RuntimeValue) -> Result<RuntimeValue, String> {
        match op {
            SemanticBinOp::Add => {
                if let (RuntimeValue::String(l), RuntimeValue::String(r)) = (left, right) {
                    Ok(RuntimeValue::String(format!("{}{}", l, r)))
                } else if let (RuntimeValue::Int(l), RuntimeValue::Int(r)) = (left, right) {
                    Ok(RuntimeValue::Int(l + r))
                } else {
                    Ok(RuntimeValue::Float(left.as_float() + right.as_float()))
                }
            }
            SemanticBinOp::Subtract => Ok(RuntimeValue::Float(left.as_float() - right.as_float())),
            SemanticBinOp::Multiply => Ok(RuntimeValue::Float(left.as_float() * right.as_float())),
            SemanticBinOp::Divide => Ok(RuntimeValue::Float(left.as_float() / right.as_float())),
            SemanticBinOp::Greater => Ok(RuntimeValue::Bool(left.as_float() > right.as_float())),
            SemanticBinOp::Less => Ok(RuntimeValue::Bool(left.as_float() < right.as_float())),
            SemanticBinOp::GreaterEqual => Ok(RuntimeValue::Bool(left.as_float() >= right.as_float())),
            SemanticBinOp::LessEqual => Ok(RuntimeValue::Bool(left.as_float() <= right.as_float())),
            SemanticBinOp::Equal => Ok(RuntimeValue::Bool(left.as_float() == right.as_float())),
            SemanticBinOp::NotEqual => Ok(RuntimeValue::Bool(left.as_float() != right.as_float())),
            SemanticBinOp::And => Ok(RuntimeValue::Bool(left.as_bool() && right.as_bool())),
            SemanticBinOp::Or => Ok(RuntimeValue::Bool(left.as_bool() || right.as_bool())),
        }
    }
}