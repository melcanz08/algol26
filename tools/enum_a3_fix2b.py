#!/usr/bin/env python3
"""
A3 fixes part 2b: exclude tools/ from the file list.

The previous pass added 6th args to scripts in tools/ that merely
contain the call names as strings. This pass runs only on src/ and
tests/.
"""

import subprocess
from pathlib import Path

result = subprocess.run(
    ["git", "grep", "-l", "SemanticIRBuilder::build\\|analyze_with_spans"],
    capture_output=True, text=True, check=True,
)
candidates = [
    Path(l)
    for l in result.stdout.strip().split("\n")
    if l and not l.startswith("tools/")
]


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
    depth = 0
    in_str = False
    in_char = False
    count = 1
    any_content = False
    i = 0
    while i < len(args_str):
        c = args_str[i]
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
                any_content = True
            elif c == "'":
                in_char = True
                any_content = True
            elif c in "([{":
                depth += 1
            elif c in ")]}":
                depth -= 1
            elif c == "," and depth == 0:
                count += 1
            elif not c.isspace():
                any_content = True
        i += 1
    return count if any_content else 0


def append_arg(src, call_name, new_arg):
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
        if n >= 6:
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


total = 0
for f in candidates:
    src = f.read_text()
    orig = src
    src, n1 = append_arg(src, "SemanticIRBuilder::build", "std::collections::HashMap::new()")
    src, n2 = append_arg(src, "analyze_with_spans", "&[]")
    if src != orig:
        f.write_text(src)
        print(f"OK: {f} (build={n1}, spans={n2})")
        total += n1 + n2

print()
print(f"total: {total} call site(s)")
