#!/usr/bin/env python3
"""
A3 fixes part 3: count arguments correctly.

The previous attempt counted top-level commas and added 1. But most
call sites have a trailing comma (Rust convention), which inflated
the count by 1 — making 5-arg calls look like 6-arg calls, so they
were skipped.

This version splits by top-level commas and counts *non-empty*
pieces, ignoring trailing commas.
"""

from pathlib import Path


def find_call_end(src, open_paren):
    depth = 0
    i = open_paren
    in_str = False
    in_char = False
    while i < len(src):
        c = src[i]
        if in_str:
            if c == "\\":
                i += 2
                continue
            if c == '"':
                in_str = False
        elif in_char:
            if c == "\\":
                i += 2
                continue
            if c == "'":
                in_char = False
        else:
            if c == '"':
                in_str = True
            elif c == "'":
                in_char = True
            elif c == "(":
                depth += 1
            elif c == ")":
                depth -= 1
                if depth == 0:
                    return i + 1
        i += 1
    return -1


def count_top_level_args(args_str):
    """Count non-empty pieces separated by top-level commas."""
    depth = 0
    in_str = False
    in_char = False
    pieces = []
    current = []
    i = 0
    while i < len(args_str):
        c = args_str[i]
        if in_str:
            current.append(c)
            if c == "\\" and i + 1 < len(args_str):
                current.append(args_str[i + 1])
                i += 2
                continue
            if c == '"':
                in_str = False
        elif in_char:
            current.append(c)
            if c == "\\" and i + 1 < len(args_str):
                current.append(args_str[i + 1])
                i += 2
                continue
            if c == "'":
                in_char = False
        else:
            if c == '"':
                in_str = True
                current.append(c)
            elif c == "'":
                in_char = True
                current.append(c)
            elif c in "([{":
                depth += 1
                current.append(c)
            elif c in ")]}":
                depth -= 1
                current.append(c)
            elif c == "," and depth == 0:
                s = "".join(current).strip()
                if s:
                    pieces.append(s)
                current = []
            else:
                current.append(c)
        i += 1
    s = "".join(current).strip()
    if s:
        pieces.append(s)
    return len(pieces)


def append_arg(src, call_name, target_arg_count, new_arg):
    changes = 0
    pos = 0
    while True:
        idx = src.find(call_name, pos)
        if idx == -1:
            break
        j = idx + len(call_name)
        while j < len(src) and src[j] in " \t\n":
            j += 1
        if j >= len(src) or src[j] != "(":
            pos = idx + len(call_name)
            continue

        line_start = src.rfind("\n", 0, idx) + 1
        prefix = src[line_start:idx]
        if prefix.rstrip().endswith("fn"):
            pos = j
            continue

        close = find_call_end(src, j)
        if close == -1:
            pos = j
            continue

        args_str = src[j + 1 : close - 1]
        n = count_top_level_args(args_str)
        if n >= target_arg_count:
            pos = close
            continue

        indent = "        "
        for line in reversed(args_str.split("\n")):
            if line.strip():
                indent = line[: len(line) - len(line.lstrip())]
                break

        trimmed = args_str.rstrip()
        if not trimmed.endswith(","):
            trimmed += ","
        new_args = f"{trimmed}\n{indent}{new_arg},\n        "

        src = src[: j + 1] + new_args + src[close - 1 :]
        changes += 1
        pos = j + 1 + len(new_args) + 1

    return src, changes


targets = [
    ("SemanticIRBuilder::build", 6, "std::collections::HashMap::new()"),
    ("analyze_with_spans", 6, "&[]"),
    ("type_check_program", 6, "&parsed.enums"),
]

total = 0
for root in [Path("src"), Path("tests")]:
    for f in sorted(root.rglob("*.rs")):
        src = f.read_text()
        orig = src
        counts = []
        for name, n, arg in targets:
            src, c = append_arg(src, name, n, arg)
            if c:
                counts.append(f"{name}={c}")
        if src != orig:
            f.write_text(src)
            print(f"OK: {f} ({', '.join(counts)})")
            total += sum(int(c.split("=")[1]) for c in counts)

print()
print(f"total: {total} call site(s)")
