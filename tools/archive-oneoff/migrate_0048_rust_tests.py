#!/usr/bin/env python3
"""ADR 0048 phase 4 -- migrate `function` in Rust unit-test source strings.

The `.gol` migration (tools/migrate_0048.py) does not touch `.rs`
files. Rust unit tests embed `.gol` source in string literals
(`let src = "..."` / `r#"..."#`), and those still say `function`.
This script applies the same conservative classifier to those
embedded sources.

Handles the `extern` prefix, which the `.gol` migration skipped:
`extern function NAME(...) -> T` -> `extern fn NAME(...) -> T`
`extern function NAME`            -> `extern proc NAME`

Usage:
  python3 tools/migrate_0048_rust_tests.py --dry-run
  python3 tools/migrate_0048_rust_tests.py --dry-run -v
  python3 tools/migrate_0048_rust_tests.py
  python3 tools/migrate_0048_rust_tests.py src/semantics/analyzer/tests.rs
"""
import argparse
import re
import sys
from pathlib import Path

DECL_RE = re.compile(
    r'^(?P<indent>[ \t]*)(?P<pub>pub\s+)?(?P<ext>extern\s+(?:"[^"]*"\s+)?)?function\s+(?P<name>\w+)(?P<rest>.*)$'
)
COMMENT_RE = re.compile(r'//.*$')


def strip_comment(line):
    return COMMENT_RE.sub('', line)


def indent_of(line):
    return len(line) - len(line.lstrip(' \t'))


def bracket_delta(line):
    code = strip_comment(line)
    return (code.count('(') - code.count(')')
            + code.count('[') - code.count(']'))


def find_decl_end(lines, start):
    n = len(lines)
    m = DECL_RE.match(strip_comment(lines[start]))
    decl_indent = len(m.group('indent'))
    depth = bracket_delta(lines[start])
    i = start + 1
    while depth > 0 and i < n:
        depth += bracket_delta(lines[i])
        i += 1
    header_end = i
    while i < n:
        raw = lines[i]
        if raw.strip() == '':
            i += 1
            continue
        if indent_of(raw) > decl_indent:
            i += 1
        else:
            break
    return header_end, i


def classify(lines, start, header_end, body_end, has_arrow, is_extern):
    body_indices = [i for i in range(header_end, body_end)
                    if strip_comment(lines[i]).strip() != '']
    if is_extern or not body_indices:
        return ('fn' if has_arrow else 'proc', None)
    if not has_arrow:
        return ('proc', None)
    if len(body_indices) != 1:
        return ('proc', None)
    line = strip_comment(lines[body_indices[0]]).strip()
    if not line.startswith('return '):
        return ('proc', None)
    return ('fn', line[len('return '):])


def rewrite_file(path, dry_run):
    lines = path.read_text().splitlines()
    out = []
    i = 0
    n = len(lines)
    changes = []
    while i < n:
        m = DECL_RE.match(strip_comment(lines[i]))
        if not m:
            out.append(lines[i])
            i += 1
            continue

        header_end, body_end = find_decl_end(lines, i)
        header_text = ' '.join(lines[j] for j in range(i, header_end))
        has_arrow = '->' in header_text
        is_extern = m.group('ext') is not None
        kind, expr = classify(lines, i, header_end, body_end, has_arrow, is_extern)

        new_kw = 'fn' if kind == 'fn' else 'proc'
        head = lines[i]
        head = re.sub(r'\bfunction\b', new_kw, head, count=1)
        out.append(head)
        for j in range(i + 1, header_end):
            out.append(lines[j])

        if kind == 'fn' and expr is not None and not is_extern:
            body_indices = [j for j in range(header_end, body_end)
                            if strip_comment(lines[j]).strip() != '']
            orig = lines[body_indices[0]]
            body_indent = orig[:len(orig) - len(orig.lstrip(' \t'))]
            out.append(f"{body_indent}{expr}")
        else:
            for j in range(header_end, body_end):
                out.append(lines[j])

        changes.append((path, i + 1, kind, m.group('name')))
        i = body_end

    if changes and not dry_run:
        path.write_text('\n'.join(out) + '\n')
    return changes


def main():
    ap = argparse.ArgumentParser(description=__doc__,
                                 formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument('paths', nargs='*', default=['src'])
    ap.add_argument('--dry-run', action='store_true')
    ap.add_argument('-v', '--verbose', action='store_true')
    args = ap.parse_args()

    targets = []
    for p in args.paths:
        pp = Path(p)
        if pp.is_file():
            targets.append(pp)
        elif pp.is_dir():
            targets.extend(sorted(pp.rglob('*.rs')))
        else:
            print(f"warning: {p} not found", file=sys.stderr)

    all_changes = []
    for f in targets:
        all_changes.extend(rewrite_file(f, dry_run=args.dry_run))

    fn_count = sum(1 for c in all_changes if c[2] == 'fn')
    proc_count = sum(1 for c in all_changes if c[2] == 'proc')
    verb = 'would migrate' if args.dry_run else 'migrated'
    print(f"{verb}: {len(all_changes)} declarations in .rs test sources "
          f"({fn_count} fn, {proc_count} proc)")
    if args.verbose:
        for path, ln, kind, name in all_changes:
            print(f"  {path}:{ln}  {name}  -> {kind}")
    return 0


if __name__ == '__main__':
    sys.exit(main())
