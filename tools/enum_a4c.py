#!/usr/bin/env python3
"""
A4c: display arms for variant patterns.

- ast_display.rs:         prints the bare variant name
- semantic_ir/display.rs: prints `VariantName (ordinal)`
"""

from pathlib import Path

AST_DISPLAY = Path("src/frontend/ast_display.rs")
IR_DISPLAY = Path("src/ir/semantic_ir/display.rs")


def patch(path, edits):
    src = path.read_text()
    for i, (old, new) in enumerate(edits, start=1):
        count = src.count(old)
        if count != 1:
            print(f"FAIL: {path} — edit {i} matched {count} times; expected 1.")
            print("-" * 60)
            print(old[:250])
            print("-" * 60)
            raise SystemExit(1)
        src = src.replace(old, new, 1)
    path.write_text(src)
    print(f"OK: {path}")


patch(
    AST_DISPLAY,
    [
        (
            """        Pattern::Range { start, end } => {
            if let Some(s) = start {
                format_expr(out, 0, s);
            }
            out.push_str("..");
            if let Some(e) = end {
                format_expr(out, 0, e);
            }
        }
    }
}""",
            """        Pattern::Range { start, end } => {
            if let Some(s) = start {
                format_expr(out, 0, s);
            }
            out.push_str("..");
            if let Some(e) = end {
                format_expr(out, 0, e);
            }
        }
        // ADR 0030: enum variant pattern. Rendered as the bare
        // variant name, matching source syntax.
        Pattern::Variant(name) => out.push_str(name),
    }
}""",
        ),
    ],
)

patch(
    IR_DISPLAY,
    [
        (
            """        SemanticPattern::Record { name, bindings } => {
            write!(out, "{} {{ {} }}", name, bindings.join(", ")).unwrap();
        }
    }
}""",
            """        SemanticPattern::Record { name, bindings } => {
            write!(out, "{} {{ {} }}", name, bindings.join(", ")).unwrap();
        }
        // ADR 0030: enum variant pattern. The ordinal is what the
        // runtime compares against; showing it alongside the name
        // makes the IR's actual behavior legible.
        SemanticPattern::Variant { name, ordinal } => {
            write!(out, "{} ({})", name, ordinal).unwrap();
        }
    }
}""",
        ),
    ],
)
