// Updated IRTranslator - preserves type information
// Option/Result types are NOT collapsed

#![allow(dead_code)]

use crate::ast::{Expr, FunctionDecl, Stmt, BinOp};
use crate::ir::{IRBuilder, IRProgram, IRType, IRValue, IRConstant, IRInstruction, IRBinOp};

pub struct IRTranslator;

impl IRTranslator {
    pub fn translate(functions: &[FunctionDecl]) -> IRProgram {
        let mut builder = IRBuilder::new();
        
        for func in functions {
            let return_type = Self::map_type(func.return_type.as_deref().unwrap_or("void"));
            builder.begin_function(&func.name, return_type);
            
            for (name, type_str) in &func.params {
                let param_type = Self::map_type(type_str);
                builder.declare_variable(name, param_type, false);
            }
            
            for stmt in &func.body {
                Self::translate_stmt(&mut builder, stmt);
            }
            
            builder.end_function();
        }
        
        builder.program
    }
    
    fn map_type(type_str: &str) -> IRType {
        match type_str {
            "int" => IRType::Int,
            "float" => IRType::Float,
            "string" => IRType::String,
            "bool" => IRType::Bool,
            "void" => IRType::Void,
            _ => IRType::Float,
        }
    }
    
    fn translate_stmt(builder: &mut IRBuilder, stmt: &Stmt) {
        match stmt {
            Stmt::VarDecl { name, value, mutable, .. } => {
                let ir_value = Self::translate_expr(builder, value);
                let var_type = Self::infer_type(value);
                builder.declare_variable(name, var_type, *mutable);
                builder.emit(IRInstruction::Store {
                    target: name.clone(),
                    value: ir_value,
                });
            }
            Stmt::Assign { name, value } => {
                let ir_value = Self::translate_expr(builder, value);
                builder.emit(IRInstruction::Store {
                    target: name.clone(),
                    value: ir_value,
                });
            }
            Stmt::Print { expr } => {
                let ir_value = Self::translate_expr(builder, expr);
                builder.emit(IRInstruction::Print { value: ir_value });
            }
            Stmt::Return { value } => {
                let ir_value = value.as_ref().map(|v| Self::translate_expr(builder, v));
                builder.emit(IRInstruction::Return { value: ir_value });
            }
            Stmt::If { condition, then_body, else_body } => {
                let cond = Self::translate_expr(builder, condition);
                let then_block = builder.new_block();
                let else_block = builder.new_block();
                let merge_block = builder.new_block();
                
                builder.emit(IRInstruction::Branch {
                    condition: cond,
                    then_block,
                    else_block,
                });
                
                builder.set_block(then_block);
                for s in then_body {
                    Self::translate_stmt(builder, s);
                }
                builder.emit(IRInstruction::Jump { block: merge_block });
                
                builder.set_block(else_block);
                if let Some(else_stmts) = else_body {
                    for s in else_stmts {
                        Self::translate_stmt(builder, s);
                    }
                }
                builder.emit(IRInstruction::Jump { block: merge_block });
                
                builder.set_block(merge_block);
            }
            Stmt::While { condition, body } => {
                let cond_block = builder.new_block();
                let body_block = builder.new_block();
                let merge_block = builder.new_block();
                
                builder.emit(IRInstruction::Jump { block: cond_block });
                
                builder.set_block(cond_block);
                let cond = Self::translate_expr(builder, condition);
                builder.emit(IRInstruction::Branch {
                    condition: cond,
                    then_block: body_block,
                    else_block: merge_block,
                });
                
                builder.set_block(body_block);
                for s in body {
                    Self::translate_stmt(builder, s);
                }
                builder.emit(IRInstruction::Jump { block: cond_block });
                
                builder.set_block(merge_block);
            }
            Stmt::For { var, iterable, body } => {
                if let Expr::List(elements) = iterable {
                    for elem in elements {
                        let ir_value = Self::translate_expr(builder, elem);
                        builder.emit(IRInstruction::Store {
                            target: var.clone(),
                            value: ir_value,
                        });
                        for s in body {
                            Self::translate_stmt(builder, s);
                        }
                    }
                }
            }
            Stmt::FunctionCall { name, args } => {
                let ir_args: Vec<IRValue> = args.iter()
                    .map(|a| Self::translate_expr(builder, a))
                    .collect();
                builder.emit(IRInstruction::Call {
                    result: None,
                    function: name.clone(),
                    args: ir_args,
                });
            }
            Stmt::Spawn { body } => {
                for s in body {
                    Self::translate_stmt(builder, s);
                }
            }
            Stmt::Parallel { blocks } => {
                for block in blocks {
                    for s in block {
                        Self::translate_stmt(builder, s);
                    }
                }
            }
            Stmt::Defer { stmt } => {
                let _ = stmt;
            }
            Stmt::Match { value, cases } => {
                let _ = value;
                if let Some(first_case) = cases.first() {
                    for s in &first_case.body {
                        Self::translate_stmt(builder, s);
                    }
                }
            }
            Stmt::UnsafeBlock { body } => {
                for s in body {
                    Self::translate_stmt(builder, s);
                }
            }
            Stmt::RegionBlock { name: _, body } => {
                for s in body {
                    Self::translate_stmt(builder, s);
                }
            }
            Stmt::Import { path } => {
                let _ = path;
            }
            Stmt::TryCatch { try_body, catch_var: _, catch_body, finally_body: _ } => {
                for s in try_body {
                    Self::translate_stmt(builder, s);
                }
                for s in catch_body {
                    Self::translate_stmt(builder, s);
                }
            }
            _ => {}
        }
    }
    
    fn translate_expr(builder: &mut IRBuilder, expr: &Expr) -> IRValue {
        match expr {
            Expr::Borrow { expr } => Self::translate_expr(builder, expr),
            Expr::MutBorrow { expr } => Self::translate_expr(builder, expr),
            Expr::Deref { expr } => Self::translate_expr(builder, expr),
            Expr::AddrOf { expr } => Self::translate_expr(builder, expr),
            Expr::Number(n) => IRValue::Constant(IRConstant::Float(*n)),
            Expr::Int(i) => IRValue::Constant(IRConstant::Int(*i)),
            Expr::String(s) => IRValue::Constant(IRConstant::String(s.clone())),
            Expr::Bool(b) => IRValue::Constant(IRConstant::Bool(*b)),
            Expr::Var(name) => IRValue::Variable(name.clone()),
            Expr::List(elements) => {
                let constants: Vec<IRConstant> = elements.iter()
                    .map(|e| match e {
                        Expr::Number(n) => IRConstant::Float(*n),
                        Expr::Int(i) => IRConstant::Int(*i),
                        Expr::String(s) => IRConstant::String(s.clone()),
                        Expr::Bool(b) => IRConstant::Bool(*b),
                        _ => IRConstant::Float(0.0),
                    })
                    .collect();
                IRValue::Constant(IRConstant::List(constants))
            }
            Expr::Binary { left, op, right } => {
                let l = Self::translate_expr(builder, left);
                let r = Self::translate_expr(builder, right);
                let temp_name = builder.program.new_temp("binop");
                let ir_op = Self::map_binop(op);
                builder.emit(IRInstruction::BinaryOp {
                    result: temp_name.clone(),
                    op: ir_op,
                    left: l,
                    right: r,
                });
                IRValue::Variable(temp_name)
            }
            Expr::FunctionCall { name, args } => {
                let ir_args: Vec<IRValue> = args.iter()
                    .map(|a| Self::translate_expr(builder, a))
                    .collect();
                let temp_name = builder.program.new_temp("call");
                builder.emit(IRInstruction::Call {
                    result: Some(temp_name.clone()),
                    function: name.clone(),
                    args: ir_args,
                });
                IRValue::Variable(temp_name)
            }
            Expr::ArrayAccess { array, index } => {
                if let Expr::Var(array_name) = array.as_ref() {
                    let idx = Self::translate_expr(builder, index);
                    let temp_name = builder.program.new_temp("arr");
                    builder.emit(IRInstruction::ArrayAccess {
                        result: temp_name.clone(),
                        array: array_name.clone(),
                        index: idx,
                    });
                    IRValue::Variable(temp_name)
                } else {
                    IRValue::Constant(IRConstant::Float(0.0))
                }
            }
            Expr::Some { value } => {
                // PRESERVE Option semantics - don't collapse!
                let inner = Self::translate_expr(builder, value);
                let inner_str = match &inner {
                    IRValue::Constant(IRConstant::Float(f)) => format!("{}", f),
                    IRValue::Constant(IRConstant::Int(i)) => format!("{}", i),
                    IRValue::Variable(name) => name.clone(),
                    _ => "?".to_string(),
                };
                IRValue::Variable(format!("Some({})", inner_str))
            }
            Expr::None => {
                IRValue::Variable("None".to_string())
            }
            Expr::Ok { value } => {
                let inner = Self::translate_expr(builder, value);
                let inner_str = match &inner {
                    IRValue::Constant(IRConstant::Float(f)) => format!("{}", f),
                    IRValue::Constant(IRConstant::Int(i)) => format!("{}", i),
                    IRValue::Variable(name) => name.clone(),
                    _ => "?".to_string(),
                };
                IRValue::Variable(format!("Ok({})", inner_str))
            }
            Expr::Error { value } => {
                let inner = Self::translate_expr(builder, value);
                IRValue::Variable(format!("Error({})", match inner {
                    IRValue::Constant(IRConstant::String(s)) => format!("\"{}\"", s),
                    IRValue::Variable(name) => name,
                    _ => "?".to_string(),
                }))
            }
        }
    }
    
    fn map_binop(op: &BinOp) -> IRBinOp {
        match op {
            BinOp::Add => IRBinOp::Add,
            BinOp::Subtract => IRBinOp::Subtract,
            BinOp::Multiply => IRBinOp::Multiply,
            BinOp::Divide => IRBinOp::Divide,
            BinOp::Greater => IRBinOp::Greater,
            BinOp::Less => IRBinOp::Less,
            BinOp::GreaterEqual => IRBinOp::GreaterEqual,
            BinOp::LessEqual => IRBinOp::LessEqual,
            BinOp::Equal => IRBinOp::Equal,
            BinOp::NotEqual => IRBinOp::NotEqual,
            BinOp::And => IRBinOp::And,
            BinOp::Or => IRBinOp::Or,
        }
    }
    
    fn infer_type(expr: &Expr) -> IRType {
        match expr {
            Expr::Number(_) => IRType::Float,
            Expr::Int(_) => IRType::Int,
            Expr::String(_) => IRType::String,
            Expr::Bool(_) => IRType::Bool,
            Expr::List(_) => IRType::List(0),
            Expr::Some { .. } => IRType::Pointer(Box::new(IRType::Float)),
            Expr::None => IRType::Pointer(Box::new(IRType::Void)),
            Expr::Ok { .. } => IRType::Pointer(Box::new(IRType::Float)),
            Expr::Error { .. } => IRType::Pointer(Box::new(IRType::String)),
            _ => IRType::Float,
        }
    }
}
