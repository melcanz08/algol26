// ALGOL26 Intermediate Representation (IR)
#![allow(dead_code)]
#![allow(unused_imports)]

#[derive(Debug, Clone, PartialEq)]
pub enum IRType {
    Void,
    Int,
    Float,
    String,
    Bool,
    List(usize),
    Pointer(Box<IRType>),
}

#[derive(Debug, Clone)]
pub enum IRValue {
    Constant(IRConstant),
    Variable(String),
    Temporary(usize),
}

#[derive(Debug, Clone)]
pub enum IRConstant {
    Int(i64),
    Float(f64),
    String(String),
    Bool(bool),
    List(Vec<IRConstant>),
}

#[derive(Debug, Clone)]
pub enum IRInstruction {
    Alloca { name: String, type_: IRType, mutable: bool },
    Store { target: String, value: IRValue },
    Load { result: String, source: String },
    BinaryOp { result: String, op: IRBinOp, left: IRValue, right: IRValue },
    Branch { condition: IRValue, then_block: usize, else_block: usize },
    Jump { block: usize },
    Return { value: Option<IRValue> },
    Call { result: Option<String>, function: String, args: Vec<IRValue> },
    Print { value: IRValue },
    ArrayAccess { result: String, array: String, index: IRValue },
    BoundsCheck { index: IRValue, len: usize },
    Label(usize),
    Nop,
}

#[derive(Debug, Clone)]
pub enum IRBinOp {
    Add,
    Subtract,
    Multiply,
    Divide,
    Greater,
    Less,
    GreaterEqual,
    LessEqual,
    Equal,
    NotEqual,
    And,
    Or,
}

#[derive(Debug, Clone)]
pub struct IRBlock {
    pub id: usize,
    pub instructions: Vec<IRInstruction>,
}

#[derive(Debug, Clone)]
pub struct IRFunction {
    pub name: String,
    pub params: Vec<(String, IRType)>,
    pub return_type: IRType,
    pub blocks: Vec<IRBlock>,
    pub local_vars: Vec<(String, IRType, bool)>,
}

#[derive(Debug, Default)]
pub struct IRProgram {
    pub functions: Vec<IRFunction>,
    pub next_temp: usize,
    pub next_block: usize,
}

impl IRProgram {
    pub fn new() -> Self {
        IRProgram {
            functions: Vec::new(),
            next_temp: 0,
            next_block: 0,
        }
    }
    
    pub fn new_temp(&mut self, prefix: &str) -> String {
        let temp = format!("{}_{}", prefix, self.next_temp);
        self.next_temp += 1;
        temp
    }
    
    pub fn new_block_id(&mut self) -> usize {
        let id = self.next_block;
        self.next_block += 1;
        id
    }
    
    pub fn add_function(&mut self, function: IRFunction) {
        self.functions.push(function);
    }
    
    pub fn get_function(&self, name: &str) -> Option<&IRFunction> {
        self.functions.iter().find(|f| f.name == name)
    }
    
    pub fn display(&self) -> String {
        let mut output = String::new();
        output.push_str("ALGOL26 IR Program\n");
        output.push_str("==================\n\n");
        
        for func in &self.functions {
            output.push_str(&format!("function {}(", func.name));
            for (i, (name, type_)) in func.params.iter().enumerate() {
                if i > 0 { output.push_str(", "); }
                output.push_str(&format!("{}: {:?}", name, type_));
            }
            output.push_str(&format!(") -> {:?}\n", func.return_type));
            
            for (name, type_, mutable) in &func.local_vars {
                output.push_str(&format!("  {} {}: {:?}\n", 
                    if *mutable { "var" } else { "val" },
                    name, type_
                ));
            }
            
            for block in &func.blocks {
                output.push_str(&format!("  block {}:\n", block.id));
                for instr in &block.instructions {
                    output.push_str(&format!("    {:?}\n", instr));
                }
            }
            output.push_str("\n");
        }
        
        output
    }
}

pub struct IRBuilder {
    pub program: IRProgram,
    current_function: Option<String>,
    current_block: usize,
    scopes: Vec<Vec<String>>,
}

impl IRBuilder {
    pub fn new() -> Self {
        let mut builder = IRBuilder {
            program: IRProgram::new(),
            current_function: None,
            current_block: 0,
            scopes: vec![Vec::new()],
        };
        
        builder.current_block = builder.program.new_block_id();
        builder
    }
    
    pub fn begin_function(&mut self, name: &str, return_type: IRType) {
        self.current_function = Some(name.to_string());
        let block_id = self.program.new_block_id();
        let block = IRBlock {
            id: block_id,
            instructions: Vec::new(),
        };
        
        let function = IRFunction {
            name: name.to_string(),
            params: Vec::new(),
            return_type,
            blocks: vec![block],
            local_vars: Vec::new(),
        };
        
        self.program.add_function(function);
        self.current_block = block_id;
    }
    
    pub fn end_function(&mut self) {
        self.current_function = None;
    }
    
    pub fn emit(&mut self, instruction: IRInstruction) {
        if let Some(func_name) = &self.current_function {
            if let Some(func) = self.program.functions.iter_mut().find(|f| &f.name == func_name) {
                if let Some(block) = func.blocks.iter_mut().find(|b| b.id == self.current_block) {
                    block.instructions.push(instruction);
                }
            }
        }
    }
    
    pub fn declare_variable(&mut self, name: &str, type_: IRType, mutable: bool) {
        if let Some(func_name) = &self.current_function {
            if let Some(func) = self.program.functions.iter_mut().find(|f| &f.name == func_name) {
                func.local_vars.push((name.to_string(), type_, mutable));
            }
        }
        if let Some(scope) = self.scopes.last_mut() {
            scope.push(name.to_string());
        }
    }
    
    pub fn push_scope(&mut self) {
        self.scopes.push(Vec::new());
    }
    
    pub fn pop_scope(&mut self) {
        self.scopes.pop();
    }
    
    pub fn new_block(&mut self) -> usize {
        let id = self.program.new_block_id();
        if let Some(func_name) = &self.current_function {
            if let Some(func) = self.program.functions.iter_mut().find(|f| &f.name == func_name) {
                func.blocks.push(IRBlock { id, instructions: Vec::new() });
            }
        }
        id
    }
    
    pub fn set_block(&mut self, block_id: usize) {
        self.current_block = block_id;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_ir_program() {
        let mut builder = IRBuilder::new();
        builder.begin_function("main", IRType::Int);
        builder.declare_variable("x", IRType::Float, true);
        builder.emit(IRInstruction::Print {
            value: IRValue::Variable("x".to_string()),
        });
        builder.end_function();
        
        let display = builder.program.display();
        assert!(display.contains("function main"));
        assert!(display.contains("x"));
    }
    
    #[test]
    fn test_ir_types() {
        let mut builder = IRBuilder::new();
        builder.begin_function("test", IRType::Void);
        builder.declare_variable("i", IRType::Int, false);
        builder.declare_variable("f", IRType::Float, true);
        builder.declare_variable("s", IRType::String, false);
        builder.declare_variable("b", IRType::Bool, false);
        builder.declare_variable("l", IRType::List(3), false);
        builder.declare_variable("p", IRType::Pointer(Box::new(IRType::Int)), false);
        builder.end_function();
        
        let func = builder.program.get_function("test").unwrap();
        assert_eq!(func.local_vars.len(), 6);
    }
}
