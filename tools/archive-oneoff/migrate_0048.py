#!/usr/bin/env python3
"""ADR 0048 phase 3 -- migrate `function` declarations to `fn`/`proc`.

Conservative classification (matches tools/audit_functions.py):
  - `function NAME(...) -> T` with body exactly `return E` on one line
      -> `fn NAME(...) -> T` + indented `E`
  - `function NAME(...) -> T` with any other body
      -> `proc NAME(...) -> T` (keyword-only change)
  - `function NAME` (no arrow)
      -> `proc NAME`
  - Trait signature (`function NAME(...) -> T`, no body)
      -> `fn NAME(...) -> T`
  - Trait signature without arrow
      -> `proc NAME`

`extern function` is not migrated. The regex matches only a
declaration that begins with `function` (optionally after `pub`),
so extern declarations are skipped and must be handled separately.

Usage:
  python3 tools/migrate_0048.py --dry-run     # summary, no writes
  python3 tools/migrate_0048.py --dry-run -v  # summary + per-decl
  python3 tools/migrate_0048.py               # apply
  python3 tools/migrate_0048.py tests/         # narrow
"""
import argparse
import re
import sys
from pathlib import Path

DECL_RE = re.compile(
    r'^(?P<indent>[ \t]*)(?P<pub>pub\s+)?(?P<ext>extern\s+(?:"[^"]*"\s+)?)?function\s+(?P<name>\w+)(?P<rest>.*)$'
)
COMMENT_RE = re.compile(r'//.*$')
DEFAULT_IGNORES = {'.git', 'target', 'archive-oneoff', 'node_modules'}


def strip_comment(line):
    return COMMENT_RE.sub('', line)


def indent_of(line):
    return len(line) - len(line.lstrip(' \t'))


def bracket_delta(line):
    code = strip_comment(line)
    return (code.count('(') - code.count(')')
            + code.count('[') - code.count(']'))


def find_decl_end(lines, start):
    """Return (header_end, body_end) for a decl beginning at lines[start].
    header_end is the first body line; body_end is one past the last."""
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


def classify(lines, start, header_end, body_end, has_arrow):
    """Return ('fn', expr_or_None) | ('proc', None)."""
    body_indices = [i for i in range(header_end, body_end)
                    if strip_comment(lines[i]).strip() != '']
    if not body_indices:
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
        if not DECL_RE.match(strip_comment(lines[i])):
            out.append(lines[i])
            i += 1
            continue

        header_end, body_end = find_decl_end(lines, i)
        header_text = ' '.join(lines[j] for j in range(i, header_end))
        has_arrow = '->' in header_text
        kind, expr = classify(lines, i, header_end, body_end, has_arrow)

        # Rewrite the header: `function` -> `fn` or `proc`.
        new_kw = 'fn' if kind == 'fn' else 'proc'
        out.append(lines[i].replace('function', new_kw, 1))
        for j in range(i + 1, header_end):
            out.append(lines[j])

        if kind == 'fn' and expr is not None:
            # Collapse `return E` into a single indented expression.
            body_indices = [j for j in range(header_end, body_end)
                            if strip_comment(lines[j]).strip() != '']
            orig = lines[body_indices[0]]
            body_indent = orig[:len(orig) - len(orig.lstrip(' \t'))]
            out.append(f"{body_indent}{expr}")
        else:
            # Copy the body unchanged (proc body, or a trait signature).
            for j in range(header_end, body_end):
                out.append(lines[j])

        name = DECL_RE.match(strip_comment(lines[i])).group('name')
        changes.append((path, i + 1, kind, name))
        i = body_end

    if changes and not dry_run:
        path.write_text('\n'.join(out) + '\n')
    return changes


def main():
    ap = argparse.ArgumentParser(description=__doc__,
                                 formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument('root', nargs='?', default='.')
    ap.add_argument('--dry-run', action='store_true',
                    help='print summary, do not write')
    ap.add_argument('-v', '--verbose', action='store_true',
                    help='list every declaration migrated')
    ap.add_argument('--ignore', action='append', default=[])
    args = ap.parse_args()

    root = Path(args.root).resolve()
    if not root.is_dir():
        print(f"error: {root} is not a directory", file=sys.stderr)
        return 2
    ignores = DEFAULT_IGNORES | set(args.ignore)

    all_changes = []
    for f in sorted(root.rglob('*.gol')):
        if any(p in ignores for p in f.relative_to(root).parts[:-1]):
            continue
        all_changes.extend(rewrite_file(f, dry_run=args.dry_run))

    fn_count = sum(1 for c in all_changes if c[2] == 'fn')
    proc_count = sum(1 for c in all_changes if c[2] == 'proc')
    verb = 'would migrate' if args.dry_run else 'migrated'
    print(f"{verb}: {len(all_changes)} declarations "
          f"({fn_count} fn, {proc_count} proc)")
    if args.verbose:
        for path, ln, kind, name in all_changes:
            rel = path.relative_to(root)
            print(f"  {rel}:{ln}  {name}  -> {kind}")
    return 0


if __name__ == '__main__':
    sys.exit(main())
