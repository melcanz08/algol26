#!/usr/bin/env python3
"""Replace the ExprKind::Var arm's line/column locals with *span."""

from pathlib import Path

OLD = '''            ExprKind::Var(name, span) => {
                let line = span.start_line;
                let column = span.start_column;
                if self.is_moved(name) {
                    return Err(CompileError::simple(
                        &format!("Use of moved variable '{}'", name),
                        line,
                        column,
                        "",
                        ErrorCode::E0007,
                    )
                    .with_suggestion(
                        "Variable ownership was transferred and cannot be used in this scope",
                    ));
                }
                if self.is_mutably_borrowed(name) && !self.in_mut_borrow {
                    return Err(CompileError::simple(
                        &format!("Cannot read '{}' while it is mutably borrowed", name),
                        line,
                        column,
                        "",
                        ErrorCode::E0007,
                    )
                    .with_suggestion("Wait for the mutable borrow to end before reading"));
                }
                self.lookup_variable(name).map(|(t, _)| t).ok_or_else(|| {
                    CompileError::simple(
                        &format!("Undefined variable '{}'", name),
                        line,
                        column,
                        "",
                        ErrorCode::E0003,
                    )
                    .with_suggestion(&format!(
                        "Declare '{}' with 'var {} := ...' or 'val {} := ...' in this scope",
                        name, name, name
                    ))
                })
            }'''

NEW = '''            ExprKind::Var(name, span) => {
                if self.is_moved(name) {
                    return Err(CompileError::at(
                        *span,
                        &format!("Use of moved variable '{}'", name),
                        ErrorCode::E0007,
                    )
                    .with_suggestion(
                        "Variable ownership was transferred and cannot be used in this scope",
                    ));
                }
                if self.is_mutably_borrowed(name) && !self.in_mut_borrow {
                    return Err(CompileError::at(
                        *span,
                        &format!("Cannot read '{}' while it is mutably borrowed", name),
                        ErrorCode::E0007,
                    )
                    .with_suggestion("Wait for the mutable borrow to end before reading"));
                }
                self.lookup_variable(name).map(|(t, _)| t).ok_or_else(|| {
                    CompileError::at(
                        *span,
                        &format!("Undefined variable '{}'", name),
                        ErrorCode::E0003,
                    )
                    .with_suggestion(&format!(
                        "Declare '{}' with 'var {} := ...' or 'val {} := ...' in this scope",
                        name, name, name
                    ))
                })
            }'''

path = Path("src/semantics/analyzer/expr.rs")
src = path.read_text()

if OLD not in src:
    print("ERROR: expected ExprKind::Var arm not found verbatim.")
    print("The file may have been reformatted. Check by hand.")
    raise SystemExit(1)

src = src.replace(OLD, NEW, 1)
path.write_text(src)
print(f"OK: rewrote ExprKind::Var arm in {path}")
print(f"     remaining CompileError::simple calls: {src.count('CompileError::simple')}")