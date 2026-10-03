#!/usr/bin/env python3
"""Finish items.rs: replace both recursive calls."""

from pathlib import Path

PATH = Path("src/semantics/analyzer/items.rs")
src = PATH.read_text()

old = ".map(|a| Self::resolve_syntax_with_records(a, records, nominals, enums))"
new = ".map(|a| Self::resolve_syntax_with_records(a, records, nominals, enums, subranges))"

n = src.count(old)
if n != 2:
    print(f"FAIL: expected 2 occurrences, found {n}")
    raise SystemExit(1)

src = src.replace(old, new)
PATH.write_text(src)
print(f"OK: replaced {n} recursive call(s)")
