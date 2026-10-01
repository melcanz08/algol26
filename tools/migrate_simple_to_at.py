#!/usr/bin/env python3
"""Migrate CompileError::simple(msg, line, col, "", code) to
CompileError::at(span, msg, code).

Handles:
  self.current_span.start_line, self.current_span.start_column  ->  self.current_span
  span.start_line, span.start_column                            ->  *span
  a.current_span.start_line, a.current_span.start_column        ->  a.current_span

Skips (and reports) any call whose last four arguments don't match
one of those shapes — e.g. pre-extracted `line, column` locals.

Usage:
    python3 tools/migrate_simple_to_at.py --dry-run src/semantics/analyzer/expr.rs
    python3 tools/migrate_simple_to_at.py src/semantics/analyzer/expr.rs
    cargo fmt
"""

import re
import sys
from pathlib import Path


NEEDLE = "CompileError::simple("

# Trailing tail: comma-after-message, then the four args we collapse.
# Two variants, tried in order.
_PATTERNS = [
    # self.current_span / a.current_span / any_ident.current_span
    (
        re.compile(
            r",\s*"
            r"(\w+)\.current_span\.start_line,\s*"
            r"\1\.current_span\.start_column,\s*"
            r'"",\s*'
            r"ErrorCode::(\w+),?\s*\Z",
            re.DOTALL,
        ),
        False,  # no deref
    ),
    # bare `span` (a &Span) — needs *span
    (
        re.compile(
            r",\s*"
            r"(\w+)\.start_line,\s*"
            r"\1\.start_column,\s*"
            r'"",\s*'
            r"ErrorCode::(\w+),?\s*\Z",
            re.DOTALL,
        ),
        True,  # deref
    ),
]


def find_call_end(source: str, open_paren: int) -> int:
    """Index just past the ) that closes the ( at open_paren. -1 if unbalanced."""
    depth = 0
    i = open_paren
    in_str = False
    while i < len(source):
        c = source[i]
        if in_str:
            if c == "\\":
                i += 2
                continue
            if c == '"':
                in_str = False
        else:
            if c == '"':
                in_str = True
            elif c == "(":
                depth += 1
            elif c == ")":
                depth -= 1
                if depth == 0:
                    return i + 1
        i += 1
    return -1


def migrate(source: str):
    out = []
    pos = 0
    migrated = 0
    skipped = []

    while True:
        idx = source.find(NEEDLE, pos)
        if idx == -1:
            out.append(source[pos:])
            break

        open_paren = idx + len(NEEDLE) - 1
        close = find_call_end(source, open_paren)
        if close == -1:
            out.append(source[pos:])
            break

        args = source[open_paren + 1 : close - 1]

        matched = False
        for pattern, needs_deref in _PATTERNS:
            m = pattern.search(args)
            if not m:
                continue
            ident = m.group(1)
            code = m.group(2)
            msg = args[: m.start()].strip()
            span_expr = f"*{ident}" if needs_deref else f"{ident}.current_span"
            replacement = f"CompileError::at({span_expr}, {msg}, ErrorCode::{code})"
            out.append(source[pos:idx])
            out.append(replacement)
            pos = close
            migrated += 1
            matched = True
            break

        if not matched:
            skipped.append(source[idx:close])
            out.append(source[pos:close])
            pos = close

    return "".join(out), migrated, skipped


def main():
    if len(sys.argv) < 2:
        print(__doc__)
        sys.exit(1)

    dry = "--dry-run" in sys.argv
    paths = [a for a in sys.argv[1:] if not a.startswith("--")]
    if not paths:
        print(__doc__)
        sys.exit(1)

    for p in paths:
        path = Path(p)
        source = path.read_text()
        new_source, n, skipped = migrate(source)

        print(f"{path}: migrated {n} call(s)")

        if skipped:
            print(f"  skipped {len(skipped)} call(s) — review manually:")
            for s in skipped:
                first = s.split("\n", 1)[0].strip()
                print(f"    {first[:100]}")

        if not dry and n > 0:
            path.write_text(new_source)
            print(f"  wrote {path}")


if __name__ == "__main__":
    main()