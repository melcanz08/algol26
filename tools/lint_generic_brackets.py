#!/usr/bin/env python3
"""lint_generic_brackets.py — flag `Map<...]` / unbalanced brackets in .gol files.

Catches the recurring typo class:
  - `Map<...]` or `List<...]` (angle generic closed with square bracket)
  - any `]` with no matching `[` on the same line

Usage:
  tools/lint_generic_brackets.py <file.gol> [<file.gol> ...]
  tools/lint_generic_brackets.py --all   # scan src/**/*.gol + tests/**/*.gol

Exit code: 0 if no findings, 1 if any finding (CI-friendly).
"""
import sys
import pathlib
import argparse


def audit_line(line: str):
    """Yield (kind, col, msg) for each issue found on this line."""
    code = line
    if "//" in line:
        idx = line.index("//")
        prefix = line[:idx]
        if prefix.count('"') % 2 == 0:
            code = prefix

    angle = 0
    square = 0
    in_str = False
    i = 0
    n = len(code)
    while i < n:
        c = code[i]
        if c == '"':
            in_str = not in_str
            i += 1
            continue
        if in_str:
            i += 1
            continue
        if c == '<':
            angle += 1
        elif c == '>':
            if angle > 0:
                angle -= 1
        elif c == '[':
            square += 1
        elif c == ']':
            if angle > 0 and square == 0:
                yield ("angle-closed-with-square", i,
                       "`]` should be `>` (inside Map<... or List<...)")
                angle -= 1
            elif square > 0:
                square -= 1
            else:
                yield ("unmatched-close-square", i,
                       "`]` has no matching `[` on this line")
        i += 1


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("files", nargs="*", help=".gol files to lint")
    ap.add_argument("--all", action="store_true",
                    help="lint all .gol under src/ and tests/")
    args = ap.parse_args()

    if args.all:
        repo = pathlib.Path(__file__).resolve().parent.parent
        paths = sorted(repo.glob("src/**/*.gol")) + sorted(repo.glob("tests/**/*.gol"))
    else:
        paths = [pathlib.Path(f) for f in args.files]

    total = 0
    for p in paths:
        if not p.exists():
            continue
        for lineno, line in enumerate(p.read_text(encoding="utf-8").splitlines(), 1):
            for (kind, col, msg) in audit_line(line):
                print(f"{p}:{lineno}:{col+1}: {kind}: {msg}")
                print(f"    {line.rstrip()}")
                total += 1

    if total:
        print(f"\n{total} finding(s)", file=sys.stderr)
        sys.exit(1)
    sys.exit(0)


if __name__ == "__main__":
    main()
