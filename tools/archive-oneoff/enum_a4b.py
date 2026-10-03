#!/usr/bin/env python3
"""
A4b: exhaustive-match arms for SemanticPattern::Variant and
Pattern::Variant.

- interpreter/pattern.rs: Variant matches when the runtime Int
  equals the ordinal.
- type_table_complete.rs: Variant binds nothing and contains no
  sub-expressions, so the walker is a no-op.
"""

from pathlib import Path

INTERP = Path("src/backends/interpreter/pattern.rs")
WALKER = Path("src/compiler/passes/type_table_complete.rs")


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
    INTERP,
    [
        (
            """            SemanticPattern::Record { name, bindings } => match value {
                RuntimeValue::Record { name: rn, fields } if rn == name => {
                    let mut out = Vec::with_capacity(bindings.len());
                    for b in bindings {
                        match fields.iter().find(|(n, _)| n == b) {
                            Some((_, v)) => out.push((b.clone(), v.clone())),
                            None => return Ok(None),
                        }
                    }
                    Ok(Some(out))
                }
                _ => Ok(None),
            },
        }
    }
}""",
            """            SemanticPattern::Record { name, bindings } => match value {
                RuntimeValue::Record { name: rn, fields } if rn == name => {
                    let mut out = Vec::with_capacity(bindings.len());
                    for b in bindings {
                        match fields.iter().find(|(n, _)| n == b) {
                            Some((_, v)) => out.push((b.clone(), v.clone())),
                            None => return Ok(None),
                        }
                    }
                    Ok(Some(out))
                }
                _ => Ok(None),
            },

            // ADR 0030: enum variant match. An enum value at runtime
            // is its ordinal (an Int). The IR builder resolved the
            // ordinal from the matched type's `Type::Enum` at
            // translation time, so this is a plain integer compare.
            SemanticPattern::Variant { ordinal, .. } => match value {
                RuntimeValue::Int(n) if *n == *ordinal => Ok(Some(Vec::new())),
                _ => Ok(None),
            },
        }
    }
}""",
        ),
    ],
)

patch(
    WALKER,
    [
        (
            """            Pattern::Some(_)
            | Pattern::None
            | Pattern::Ok(_)
            | Pattern::Error(_)
            | Pattern::Wildcard
            | Pattern::Binding(_) => {}
        }
    }""",
            """            Pattern::Some(_)
            | Pattern::None
            | Pattern::Ok(_)
            | Pattern::Error(_)
            | Pattern::Wildcard
            | Pattern::Binding(_) => {}
            // ADR 0030: variant patterns bind nothing and contain no
            // sub-expressions. The analyzer has already validated the
            // name against the enum's variants.
            Pattern::Variant(_) => {}
        }
    }""",
        ),
    ],
)
