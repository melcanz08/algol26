// src/ir/semantic_ir/display.rs
//
// Human-readable rendering of a `SemanticProgram`. Used by
// `algol26 inspect --ir`.
//
// Deliberately not a `Display` impl: the output is multi-line,
// option-dependent, and not what a caller would want interpolated
// into a larger string. The entry point is `format_program`,
// which returns a `String`.
//
// The rendering is *source-shaped* wherever possible —
// `var x: Int := 5` for a `Declare`, `x := 5` for an `Assign`,
// `&x` for a `BorrowShared`, etc. This is intentional: the IR
// should be readable to anyone who knows ALGOL26 source, so that
// `inspect --ir` is useful for debugging the compiler without
// learning the `Debug` format of every enum.

// `write!` into a `String` cannot fail — `String`'s `Write`
// impl is infallible. Every `.unwrap()` below is safe by
// construction.
#![allow(clippy::unwrap_used)]

use super::{
    Instruction, SemanticBinOp, SemanticBlock, SemanticFunction, SemanticPattern, SemanticProgram,
    Terminator, TypedIRValue,
};
use std::fmt::Write;

/// Which view of the program to render.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FormatMode {
    /// Full instructions and terminators. This is the default —
    /// what `inspect --ir` shows.
    Linear,
    /// Block IDs and their successors, no instructions. What
    /// `inspect --cfg` shows: useful for reasoning about control
    /// flow without the noise of instruction bodies.
    Cfg,
    /// Function signatures only. Useful for a high-level overview
    /// of a large program.
    Signatures,
}

/// Options controlling `format_program_with`.
#[derive(Debug, Clone, Copy)]
pub struct FormatOptions {
    pub mode: FormatMode,
}

impl Default for FormatOptions {
    fn default() -> Self {
        FormatOptions {
            mode: FormatMode::Linear,
        }
    }
}

/// Format a program with default options.
pub fn format_program(program: &SemanticProgram) -> String {
    format_program_with(program, FormatOptions::default())
}

/// Format a program with the given options.
pub fn format_program_with(program: &SemanticProgram, opts: FormatOptions) -> String {
    let mut out = String::new();

    if program.functions.is_empty() {
        out.push_str("(no functions)\n");
    } else {
        for (i, func) in program.functions.iter().enumerate() {
            if i > 0 {
                out.push('\n');
            }
            format_function(&mut out, func, opts);
        }
    }

    if !program.ffi_symbols.is_empty() {
        out.push('\n');
        out.push_str("ffi symbols:\n");
        let mut syms: Vec<(&String, &String)> = program.ffi_symbols.iter().collect();
        syms.sort_by_key(|(k, _)| k.as_str());
        for (algol_name, c_name) in syms {
            writeln!(out, "  {} -> {}", algol_name, c_name).unwrap();
        }
    }

    if !program.ffi_libraries.is_empty() {
        writeln!(out, "ffi libraries: {}", program.ffi_libraries.join(", ")).unwrap();
    }

    out
}

fn format_function(out: &mut String, func: &SemanticFunction, opts: FormatOptions) {
    // Header: match source-language syntax so the IR view is
    // immediately recognizable.
    write!(out, "function {}(", func.name).unwrap();
    for (i, (name, ty)) in func.params.iter().enumerate() {
        if i > 0 {
            out.push_str(", ");
        }
        write!(out, "{}: {}", name, ty).unwrap();
    }
    write!(out, ") -> {}", func.return_type).unwrap();

    if func.is_extern {
        out.push_str("  (extern)");
    }
    out.push('\n');

    match opts.mode {
        FormatMode::Signatures => {}
        FormatMode::Linear => {
            for block in &func.blocks {
                format_block_linear(out, block, func.entry_block);
            }
        }
        FormatMode::Cfg => {
            for block in &func.blocks {
                format_block_cfg(out, block, func.entry_block);
            }
        }
    }
}

fn format_block_linear(out: &mut String, block: &SemanticBlock, entry_id: usize) {
    if block.id == entry_id {
        writeln!(out, "  [{}] (entry):", block.id).unwrap();
    } else {
        writeln!(out, "  [{}]:", block.id).unwrap();
    }

    for instr in &block.instructions {
        out.push_str("    ");
        format_instruction(out, instr);
        out.push('\n');
    }

    if let Some(term) = &block.terminator {
        out.push_str("    ");
        format_terminator(out, term);
        out.push('\n');
    }
}

/// CFG view: block ID, then the list of successor block IDs (or
/// `return` / `(no terminator)` for terminal blocks). No
/// instructions, no operand detail.
fn format_block_cfg(out: &mut String, block: &SemanticBlock, entry_id: usize) {
    if block.id == entry_id {
        write!(out, "  [{}] (entry)", block.id).unwrap();
    } else {
        write!(out, "  [{}]", block.id).unwrap();
    }

    match &block.terminator {
        None => out.push_str(" -> (no terminator)"),
        Some(Terminator::Return { .. }) => out.push_str(" -> return"),
        Some(term) => {
            let succs = term.successors();
            if succs.is_empty() {
                out.push_str(" -> (no successors)");
            } else {
                out.push_str(" -> ");
                for (i, s) in succs.iter().enumerate() {
                    if i > 0 {
                        out.push_str(", ");
                    }
                    write!(out, "[{}]", s).unwrap();
                }
            }
        }
    }
    out.push('\n');
}

fn format_instruction(out: &mut String, instr: &Instruction) {
    match instr {
        Instruction::Nop => out.push_str("nop"),
        Instruction::Declare {
            name,
            mutable,
            type_,
            value,
        } => {
            let kw = if *mutable { "var" } else { "val" };
            write!(out, "{} {}: {} := ", kw, name, type_).unwrap();
            format_value(out, value);
        }
        Instruction::Assign { target, value } => {
            write!(out, "{} := ", target).unwrap();
            format_value(out, value);
        }
        Instruction::ArrayAssign {
            array,
            index,
            value,
        } => {
            format_value(out, array);
            out.push('[');
            format_value(out, index);
            out.push_str("] := ");
            format_value(out, value);
        }
        Instruction::Print { value } => {
            out.push_str("print(");
            format_value(out, value);
            out.push(')');
        }
        Instruction::Call { func, args, result } => {
            if let Some(r) = result {
                write!(out, "{} := ", r).unwrap();
            }
            write!(out, "{}(", func).unwrap();
            for (i, arg) in args.iter().enumerate() {
                if i > 0 {
                    out.push_str(", ");
                }
                format_value(out, arg);
            }
            out.push(')');
        }
        Instruction::IteratorInit { iterator, iterable } => {
            write!(out, "{} := iter_init(", iterator).unwrap();
            format_value(out, iterable);
            out.push(')');
        }
        Instruction::ChannelDecl { name, type_ } => {
            write!(out, "channel {}: {}", name, type_).unwrap();
        }
        Instruction::SendChannel { channel, value } => {
            write!(out, "send({}, ", channel).unwrap();
            format_value(out, value);
            out.push(')');
        }
        Instruction::ReceiveChannel { channel, target } => {
            write!(out, "recv({}) -> {}", channel, target).unwrap();
        }
        Instruction::Allocate {
            target,
            size,
            type_,
        } => {
            write!(out, "{}: {} := alloc(", target, type_).unwrap();
            format_value(out, size);
            out.push(')');
        }
        Instruction::Free { ptr } => {
            out.push_str("free(");
            format_value(out, ptr);
            out.push(')');
        }
        Instruction::RegionEnter { name } => {
            write!(out, "region {} {{", name).unwrap();
        }
        Instruction::RegionExit { name } => {
            write!(out, "}} // end region {}", name).unwrap();
        }
    }
}

fn format_value(out: &mut String, value: &TypedIRValue) {
    match value {
        TypedIRValue::Int(i) => write!(out, "{}", i).unwrap(),
        TypedIRValue::Float(f) => write!(out, "{}", f).unwrap(),
        TypedIRValue::String(s) => write!(out, "{:?}", s).unwrap(),
        TypedIRValue::Bool(b) => write!(out, "{}", b).unwrap(),
        TypedIRValue::Void => out.push_str("void"),
        TypedIRValue::NullPtr => out.push_str("null"),
        TypedIRValue::PtrLiteral(p) => write!(out, "0x{:x}", p).unwrap(),
        TypedIRValue::Variable(name, _) => out.push_str(name),
        TypedIRValue::Cast { value, target_type } => {
            out.push('(');
            format_value(out, value);
            write!(out, " as {})", target_type).unwrap();
        }
        TypedIRValue::BinaryOp {
            op, left, right, ..
        } => {
            out.push('(');
            format_value(out, left);
            write!(out, " {} ", binop_str(op)).unwrap();
            format_value(out, right);
            out.push(')');
        }
        TypedIRValue::Call { function, args, .. } => {
            write!(out, "{}(", function).unwrap();
            for (i, a) in args.iter().enumerate() {
                if i > 0 {
                    out.push_str(", ");
                }
                format_value(out, a);
            }
            out.push(')');
        }
        TypedIRValue::List(items, _) | TypedIRValue::Array(items, _, _) => {
            out.push('[');
            for (i, item) in items.iter().enumerate() {
                if i > 0 {
                    out.push_str(", ");
                }
                format_value(out, item);
            }
            out.push(']');
        }
        TypedIRValue::Some(inner) => {
            out.push_str("Some(");
            format_value(out, inner);
            out.push(')');
        }
        TypedIRValue::None { .. } => out.push_str("None"),
        TypedIRValue::Ok { value, .. } => {
            out.push_str("Ok(");
            format_value(out, value);
            out.push(')');
        }
        TypedIRValue::Error { value, .. } => {
            out.push_str("Error(");
            format_value(out, value);
            out.push(')');
        }
        TypedIRValue::ArrayAccess { array, index, .. } => {
            format_value(out, array);
            out.push('[');
            format_value(out, index);
            out.push(']');
        }
        TypedIRValue::BorrowShared { expr, .. } => {
            out.push('&');
            format_value(out, expr);
        }
        TypedIRValue::BorrowMutable { expr, .. } => {
            out.push_str("&mut ");
            format_value(out, expr);
        }
        TypedIRValue::ReadReference { expr, .. } => {
            out.push('*');
            format_value(out, expr);
        }
        TypedIRValue::AddrOf { expr, .. } => {
            out.push_str("addr_of(");
            format_value(out, expr);
            out.push(')');
        }
        TypedIRValue::Range(start, end) => {
            format_value(out, start);
            out.push_str("..");
            format_value(out, end);
        }
        TypedIRValue::FieldAccess { object, field, .. } => {
            format_value(out, object);
            out.push('.');
            out.push_str(field);
        }
    }
}

fn binop_str(op: &SemanticBinOp) -> &'static str {
    match op {
        SemanticBinOp::Add => "+",
        SemanticBinOp::Subtract => "-",
        SemanticBinOp::Multiply => "*",
        SemanticBinOp::Divide => "/",
        SemanticBinOp::Greater => ">",
        SemanticBinOp::Less => "<",
        SemanticBinOp::GreaterEqual => ">=",
        SemanticBinOp::LessEqual => "<=",
        SemanticBinOp::Equal => "==",
        SemanticBinOp::NotEqual => "!=",
    }
}

fn format_terminator(out: &mut String, term: &Terminator) {
    match term {
        Terminator::Return { value, .. } => match value {
            Some(v) => {
                out.push_str("return ");
                format_value(out, v);
            }
            None => out.push_str("return"),
        },
        Terminator::Jump { block } => {
            write!(out, "jump -> [{}]", block).unwrap();
        }
        Terminator::Branch {
            condition,
            then_block,
            else_block,
        } => {
            out.push_str("branch ");
            format_value(out, condition);
            write!(out, " ? [{}] : [{}]", then_block, else_block).unwrap();
        }
        Terminator::Switch {
            value,
            cases,
            default_block,
        } => {
            out.push_str("switch ");
            format_value(out, value);
            out.push_str(" {");
            for (pat, target) in cases {
                out.push_str(" case ");
                format_pattern(out, pat);
                write!(out, " -> [{}],", target).unwrap();
            }
            if let Some(d) = default_block {
                write!(out, " default -> [{}]", d).unwrap();
            }
            out.push_str(" }");
        }
        Terminator::IteratorNext {
            iterator,
            target,
            body_block,
            exit_block,
        } => {
            write!(
                out,
                "iter_next({}) -> body [{}] ({} bound), exit [{}]",
                iterator, body_block, target, exit_block
            )
            .unwrap();
        }
        Terminator::Spawn { entry_block } => {
            write!(out, "spawn -> [{}]", entry_block).unwrap();
        }
        Terminator::Fork { blocks, join_block } => {
            out.push_str("fork [");
            for (i, b) in blocks.iter().enumerate() {
                if i > 0 {
                    out.push_str(", ");
                }
                write!(out, "{}", b).unwrap();
            }
            write!(out, "] join [{}]", join_block).unwrap();
        }
    }
}

fn format_pattern(out: &mut String, pat: &SemanticPattern) {
    match pat {
        SemanticPattern::Some { binding } => write!(out, "Some({})", binding).unwrap(),
        SemanticPattern::None => out.push_str("None"),
        SemanticPattern::Ok { binding } => write!(out, "Ok({})", binding).unwrap(),
        SemanticPattern::Error { binding } => write!(out, "Error({})", binding).unwrap(),
        SemanticPattern::Wildcard => out.push('_'),
        SemanticPattern::Literal(v) => format_value(out, v),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::common::types::Type;
    use crate::ir::semantic_ir::{SemanticBlock, SemanticFunction, SemanticProgram};

    #[test]
    fn empty_program() {
        let program = SemanticProgram::new();
        assert_eq!(format_program(&program), "(no functions)\n");
    }

    #[test]
    fn simple_main() {
        let mut program = SemanticProgram::new();
        let entry = program.new_block_id();
        let func = SemanticFunction {
            name: "main".to_string(),
            params: vec![],
            return_type: Type::Void,
            blocks: vec![SemanticBlock {
                id: entry,
                instructions: vec![
                    Instruction::Declare {
                        name: "x".to_string(),
                        mutable: true,
                        type_: Type::Int,
                        value: TypedIRValue::Int(5),
                    },
                    Instruction::Print {
                        value: TypedIRValue::Variable("x".to_string(), Type::Int),
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

        let out = format_program(&program);
        assert!(out.contains("function main() -> Void"), "got:\n{}", out);
        assert!(out.contains("[0] (entry):"), "got:\n{}", out);
        assert!(out.contains("var x: Int := 5"), "got:\n{}", out);
        assert!(out.contains("print(x)"), "got:\n{}", out);
        assert!(out.contains("return"), "got:\n{}", out);
    }

    #[test]
    fn branch_shows_successors() {
        let mut program = SemanticProgram::new();
        let entry = program.new_block_id();
        let then_id = program.new_block_id();
        let else_id = program.new_block_id();
        let func = SemanticFunction {
            name: "f".to_string(),
            params: vec![],
            return_type: Type::Void,
            blocks: vec![
                SemanticBlock {
                    id: entry,
                    instructions: vec![],
                    terminator: Some(Terminator::Branch {
                        condition: TypedIRValue::Bool(true),
                        then_block: then_id,
                        else_block: else_id,
                    }),
                },
                SemanticBlock {
                    id: then_id,
                    instructions: vec![],
                    terminator: Some(Terminator::Return {
                        value: None,
                        type_: Type::Void,
                    }),
                },
                SemanticBlock {
                    id: else_id,
                    instructions: vec![],
                    terminator: Some(Terminator::Return {
                        value: None,
                        type_: Type::Void,
                    }),
                },
            ],
            entry_block: entry,
            is_extern: false,
        };
        program.functions.push(func);

        let out = format_program(&program);
        assert!(out.contains("branch true ? ["), "got:\n{}", out);
    }

    #[test]
    fn borrow_and_deref_rendering() {
        let mut program = SemanticProgram::new();
        let entry = program.new_block_id();
        let func = SemanticFunction {
            name: "f".to_string(),
            params: vec![],
            return_type: Type::Void,
            blocks: vec![SemanticBlock {
                id: entry,
                instructions: vec![Instruction::Print {
                    value: TypedIRValue::ReadReference {
                        expr: Box::new(TypedIRValue::BorrowShared {
                            expr: Box::new(TypedIRValue::Variable("x".to_string(), Type::Int)),
                            target_type: Type::borrow(Type::Int),
                        }),
                        target_type: Type::Int,
                    },
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

        let out = format_program(&program);
        assert!(out.contains("print(*&x)"), "got:\n{}", out);
    }

    #[test]
    fn cfg_mode_shows_successors() {
        let mut program = SemanticProgram::new();
        let entry = program.new_block_id();
        let then_id = program.new_block_id();
        let else_id = program.new_block_id();
        let join = program.new_block_id();
        let func = SemanticFunction {
            name: "f".to_string(),
            params: vec![],
            return_type: Type::Void,
            blocks: vec![
                SemanticBlock {
                    id: entry,
                    instructions: vec![],
                    terminator: Some(Terminator::Branch {
                        condition: TypedIRValue::Bool(true),
                        then_block: then_id,
                        else_block: else_id,
                    }),
                },
                SemanticBlock {
                    id: then_id,
                    instructions: vec![],
                    terminator: Some(Terminator::Jump { block: join }),
                },
                SemanticBlock {
                    id: else_id,
                    instructions: vec![],
                    terminator: Some(Terminator::Jump { block: join }),
                },
                SemanticBlock {
                    id: join,
                    instructions: vec![],
                    terminator: Some(Terminator::Return {
                        value: None,
                        type_: Type::Void,
                    }),
                },
            ],
            entry_block: entry,
            is_extern: false,
        };
        program.functions.push(func);

        let out = format_program_with(
            &program,
            FormatOptions {
                mode: FormatMode::Cfg,
            },
        );
        assert!(out.contains("[0] (entry) -> ["), "got:\n{}", out);
        assert!(out.contains("-> return"), "got:\n{}", out);
        // No instructions in CFG mode.
        assert!(!out.contains("branch true ? ["), "got:\n{}", out);
    }
}
