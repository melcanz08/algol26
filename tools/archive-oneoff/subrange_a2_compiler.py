#!/usr/bin/env python3
"""
A2 continuation: thread SubrangeDecl through compiler.rs.

Line-based insertions anchored on exact content. Handles all the
sites that A2 for enums touched, plus one additional pattern
(the `.enums.clone()` field initializers).
"""

import re
from pathlib import Path

PATH = Path("src/compiler.rs")
lines = PATH.read_text().split("\n")

# ─── 1. `.enums.clone()` sites — insert subranges after each ──────
clone_re = re.compile(r"^(\s*)enums: (\w+(?:\.\w+)*)\.enums\.clone\(\),$")
insertions = []
for i, line in enumerate(lines):
    m = clone_re.match(line)
    if m:
        insertions.append((i, f"{m.group(1)}subranges: {m.group(2)}.subranges.clone(),"))
for i, new_line in reversed(insertions):
    lines.insert(i + 1, new_line)
print(f"OK: {len(insertions)} .enums.clone() site(s)")

# ─── 2. Non-clone sites ──────────────────────────────────────────
def insert_after_anchor(anchor, insert, label):
    for i, line in enumerate(lines):
        if line == anchor:
            indent = line[: len(line) - len(line.lstrip())]
            lines.insert(i + 1, f"{indent}{insert}")
            print(f"OK: {label}")
            return
    print(f"FAIL: {label} (anchor: {anchor!r})")
    raise SystemExit(1)

insert_after_anchor(
    "            enums: parsed.enums,",
    "subranges: parsed.subranges,",
    "prepare_frontend ParsedProgram",
)
insert_after_anchor(
    "            enums: program.enum_decls,",
    "subranges: program.subrange_decls,",
    "parse return",
)
insert_after_anchor(
    "            enums: all_enums,",
    "subranges: all_subranges,",
    "process_imports return",
)
insert_after_anchor(
    "        let mut all_enums = parsed.enums.clone();",
    "let mut all_subranges = parsed.subranges.clone();",
    "all_subranges local",
)
insert_after_anchor(
    "        all_enums: &mut Vec<crate::frontend::ast::EnumDecl>,",
    "all_subranges: &mut Vec<crate::frontend::ast::SubrangeDecl>,",
    "load_import_recursive signature",
)

# ─── 3. ParsedProgram struct field ───────────────────────────────
for i, line in enumerate(lines):
    if line == "    pub enums: Vec<crate::frontend::ast::EnumDecl>,":
        lines.insert(
            i + 1,
            "    pub subranges: Vec<crate::frontend::ast::SubrangeDecl>,",
        )
        print(f"OK: ParsedProgram struct field (line {i + 2})")
        break
else:
    print("FAIL: ParsedProgram struct field not found")
    raise SystemExit(1)

# ─── 4. Call-argument insertions ─────────────────────────────────
def insert_arg(anchor, next_anchor, new_arg, label):
    for i, line in enumerate(lines):
        if line.strip() == anchor and i + 1 < len(lines) and lines[i + 1].strip() == next_anchor:
            indent = line[: len(line) - len(line.lstrip())]
            lines.insert(i + 1, f"{indent}{new_arg}")
            print(f"OK: {label}")
            return
    print(f"FAIL: {label}")
    raise SystemExit(1)

insert_arg(
    "&mut all_enums,",
    "&mut visited,",
    "&mut all_subranges,",
    "process_imports load_import_recursive call arg",
)
insert_arg(
    "all_enums,",
    "visited,",
    "all_subranges,",
    "load_import_recursive recursive call arg",
)

# ─── 5. Merge loop after the enums merge ────────────────────────
for i, line in enumerate(lines):
    if line.strip() == "all_enums.push(e);":
        for j in range(i, -1, -1):
            if "for e in imported.enum_decls" in lines[j]:
                for_start = j
                break
        else:
            print("FAIL: could not find `for e in imported.enum_decls`")
            raise SystemExit(1)
        depth = 0
        for_end = -1
        for k in range(for_start, len(lines)):
            depth += lines[k].count("{") - lines[k].count("}")
            if depth == 0 and k > for_start:
                for_end = k
                break
        indent = lines[for_start][: len(lines[for_start]) - len(lines[for_start].lstrip())]
        block = [
            f"{indent}for s in imported.subrange_decls {{",
            f"{indent}    if !all_subranges.iter().any(|x| x.name == s.name) {{",
            f"{indent}        all_subranges.push(s);",
            f"{indent}    }}",
            f"{indent}}}",
        ]
        for k, nl in enumerate(block):
            lines.insert(for_end + 1 + k, nl)
        print(f"OK: merge subranges block (after line {for_end + 1})")
        break
else:
    print("FAIL: `all_enums.push(e);` not found")
    raise SystemExit(1)

PATH.write_text("\n".join(lines))
print(f"OK: compiler.rs now {len(lines)} lines")
