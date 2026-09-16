#!/usr/bin/env python3
"""
Intercept `p := alloc(n)` in Stmt::Assign so it emits
Instruction::Allocate — matching what Stmt::VarDecl does for
`var p := alloc(n)`.

Without this, `p := alloc(16)` falls through the generic Assign
path, produces a Call to "alloc" (no LLVM function), and fails at
codegen with "unhandled builtin 'alloc'".
"""

import sys
from pathlib import Path

ANCHOR = """                if !var_info.mutable {
                    self.diagnostics
                        .push(format!("Cannot assign to immutable variable '{}'", name));
                }
"""

REPLACEMENT = """                if !var_info.mutable {
                    self.diagnostics
                        .push(format!("Cannot assign to immutable variable '{}'", name));
                }

                // `p := alloc(n)` is a memory operation, not a generic
                // assignment. Emit `Instruction::Allocate` (which
                // stores the fresh pointer into `p`'s alloca) so the
                // codegen goes through the malloc lowering and the
                // region tracker sees the pointer. This mirrors what
                // `Stmt::VarDecl` does for `var p := alloc(n)`.
                // Without this interception, `alloc` on an Assign RHS
                // reaches LLVM codegen as a generic Call and fails
                // with "unhandled builtin 'alloc'".
                if let Expr::FunctionCall { name: fn_name, args, .. } = value {
                    if fn_name == "alloc" && args.len() == 1 {
                        let size = self.translate_expr(
                            program,
                            func,
                            current_block,
                            &args[0],
                        );
                        let ptr_ty = Type::pointer(Type::Unknown);
                        self.safe_push_instruction(
                            func,
                            current_block,
                            SemanticInstruction::Allocate {
                                target: name.clone(),
                                size,
                                type_: ptr_ty,
                            },
                        );
                        if let Some(merge) = self.pending_merge.take() {
                            return FlowResult::Reachable(merge);
                        }
                        return FlowResult::Reachable(current_block);
                    }
                }
"""


def main():
    repo = Path.cwd()
    if not (repo / "Cargo.toml").exists():
        print("ERROR: run from repo root", file=sys.stderr)
        return 1

    path = repo / "src" / "semantics" / "builder" / "expr.rs"
    if not path.exists():
        print(f"ERROR: {path} not found", file=sys.stderr)
        return 1

    text = path.read_text()
    if ANCHOR not in text:
        print(
            "ERROR: anchor not found. Paste the current Stmt::Assign arm\n"
            "  and I'll adjust the script.",
            file=sys.stderr,
        )
        return 1

    if "is a memory operation, not a generic" in text:
        print("  skipped: interception already present")
        return 0

    new_text = text.replace(ANCHOR, REPLACEMENT, 1)
    if new_text == text:
        print("ERROR: replacement made no change", file=sys.stderr)
        return 1

    path.write_text(new_text)
    print("  patched: src/semantics/builder/expr.rs")
    return 0


if __name__ == "__main__":
    sys.exit(main())