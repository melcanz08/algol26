#!/usr/bin/env python3
"""
Audit `function` declarations in an ALGOL26 (.gol) corpus.

Implements the migration rule from ADR 0048 as a classification pass, so
the numbers you get are exactly the buckets the migration script would
produce.

Buckets:
  FN_CANDIDATE    body is one `return E`, E is a plain expression
                  -> becomes `fn`
  FN_NEEDS_EXPR   body is one `return E`, E contains expression-position
                  if/match -> becomes `fn` *only if* expr-position control
                  flow already exists in the parser
  PROC            body is multi-statement (or contains `defer`)
                  -> becomes `proc`
  VOID_PROC       `function foo` with no `-> T`
                  -> becomes `proc`
  EMPTY_BODY      no body at all (malformed or extern)
  UNKNOWN         couldn't classify

Usage:
  python3 tools/audit_functions.py                # table
  python3 tools/audit_functions.py --verbose      # per-declaration
  python3 tools/audit_functions.py --json         # machine-readable
  python3 tools/audit_functions.py path/to/root   # alternate root
"""

import argparse
import json
import re
import sys
from collections import Counter, defaultdict
from pathlib import Path

DECL_RE = re.compile(
    r'^(?P<indent>[ \t]*)(?:pub\s+)?function\s+(?P<name>\w+)(?P<rest>.*)$'
)

# Directories we never want to scan. target/ is build output, .git/ is
# obvious, tools/archive-oneoff/ is the fossil record of old migrations.
DEFAULT_IGNORES = {'.git', 'target', 'archive-oneoff', 'node_modules'}

COMMENT_RE = re.compile(r'//.*$')


def strip_comment(line: str) -> str:
    return COMMENT_RE.sub('', line)


def indent_of(line: str) -> int:
    return len(line) - len(line.lstrip(' \t'))


def bracket_delta(line: str) -> int:
    """Net open parens/brackets, ignoring comments. Used to detect
    multi-line declaration headers."""
    code = strip_comment(line)
    return (code.count('(') - code.count(')')
            + code.count('[') - code.count(']'))


def find_function_decls(lines):
    """Yield (decl_line_idx, decl_indent, name, has_arrow, body) for each
    `function` declaration in the file."""
    i = 0
    n = len(lines)
    while i < n:
        code = strip_comment(lines[i])
        m = DECL_RE.match(code)
        if not m:
            i += 1
            continue

        decl_indent = len(m.group('indent'))
        name = m.group('name')

        # Header may span multiple lines if brackets are unbalanced.
        header_parts = [code]
        depth = bracket_delta(code)
        j = i + 1
        while depth > 0 and j < n:
            header_parts.append(strip_comment(lines[j]))
            depth += bracket_delta(lines[j])
            j += 1
        header = ' '.join(header_parts)
        has_arrow = '->' in header

        # Body: subsequent lines with indent > decl_indent, plus blanks
        # internal to the body. Stops at the first non-blank line at or
        # below the declaration indent.
        body = []
        while j < n:
            raw = lines[j].rstrip('\n')
            if raw.strip() == '':
                body.append((j, raw))
                j += 1
                continue
            if indent_of(raw) > decl_indent:
                body.append((j, raw))
                j += 1
            else:
                break

        yield (i, decl_indent, name, has_arrow, body)
        i = j


def classify(body):
    """Return a tuple describing the body shape.

    ('EMPTY',)
    ('SINGLE_RETURN', expr, needs_expr_control)
    ('SINGLE_OTHER', stmt_text)
    ('MULTI', has_defer)
    """
    nonblank = [(ln, l) for ln, l in body if strip_comment(l).strip() != '']
    if not nonblank:
        return ('EMPTY',)

    body_indent = min(indent_of(l) for _, l in nonblank)
    top = [(ln, l) for ln, l in nonblank if indent_of(l) == body_indent]

    if len(top) == 1:
        _, l = top[0]
        stripped = strip_comment(l).strip()
        if stripped == 'return':
            return ('SINGLE_RETURN', '', False)
        if stripped.startswith('return '):
            expr = stripped[len('return '):].strip()
            # Heuristic: does the expression itself contain control flow
            # that must be expression-position?  `return if a then b else c`
            # and `return match x ...` both need it.
            needs = bool(re.search(r'\b(?:if|match)\b', expr))
            return ('SINGLE_RETURN', expr, needs)
        return ('SINGLE_OTHER', stripped)

    has_defer = any(re.search(r'\bdefer\b', strip_comment(l))
                    for _, l in nonblank)
    return ('MULTI', has_defer)


def should_skip(path: Path, root: Path, ignores):
    for part in path.relative_to(root).parts[:-1]:
        if part in ignores:
            return True
    return False


def audit(root: Path, ignores):
    results = []
    for f in sorted(root.rglob('*.gol')):
        if should_skip(f, root, ignores):
            continue
        try:
            text = f.read_text(encoding='utf-8')
        except Exception as e:
            print(f"warning: cannot read {f}: {e}", file=sys.stderr)
            continue
        lines = text.splitlines()
        for (ln, decl_indent, name, has_arrow, body) in find_function_decls(lines):
            kind = classify(body)
            if not has_arrow:
                bucket = 'VOID_PROC'
            elif kind[0] == 'EMPTY':
                bucket = 'EMPTY_BODY'
            elif kind[0] == 'SINGLE_RETURN':
                _, expr, needs = kind
                bucket = 'FN_NEEDS_EXPR' if needs else 'FN_CANDIDATE'
            elif kind[0] in ('SINGLE_OTHER', 'MULTI'):
                bucket = 'PROC'
            else:
                bucket = 'UNKNOWN'
            results.append({
                'file': str(f.relative_to(root)),
                'line': ln + 1,
                'name': name,
                'has_arrow': has_arrow,
                'bucket': bucket,
                'detail': list(kind),
            })
    return results


def print_table(results, root):
    total = len(results)
    counts = Counter(r['bucket'] for r in results)
    order = ['FN_CANDIDATE', 'FN_NEEDS_EXPR', 'PROC',
             'VOID_PROC', 'EMPTY_BODY', 'UNKNOWN']

    print()
    print(f"root: {root}")
    print(f"scanned: {total} `function` declarations")
    print()
    print(f"{'bucket':<16}{'count':>8}{'pct':>9}")
    print('-' * 33)
    for b in order:
        c = counts.get(b, 0)
        if c == 0:
            continue
        print(f"{b:<16}{c:>8}{100*c/total:>8.1f}%")
    print('-' * 33)
    print(f"{'total':<16}{total:>8}{100:>8.1f}%")
    print()

    if total == 0:
        print("Verdict: no `function` declarations found. Nothing to migrate.")
        return

    fn = counts.get('FN_CANDIDATE', 0) + counts.get('FN_NEEDS_EXPR', 0)
    proc = counts.get('PROC', 0) + counts.get('VOID_PROC', 0)

    if fn >= 0.60 * total:
        print(f"Verdict: SPLIT IS REAL.")
        print(f"  {fn}/{total} declarations want `fn`. The keyword earns its keep.")
    elif proc >= 0.60 * total:
        print(f"Verdict: SPLIT IS DECORATIVE.")
        print(f"  {proc}/{total} declarations want `proc`. `fn` is a niche form.")
        print(f"  Consider ADR alternative B (single keyword) before committing.")
    else:
        print(f"Verdict: REAL SPLIT, REAL MIGRATION.")
        print(f"  fn-shaped: {fn}   proc-shaped: {proc}   "
              f"({100*fn/total:.0f}% / {100*proc/total:.0f}%)")
        print(f"  Budget phase 3 of the ADR as a multi-hour review, not a script-and-go.")


def print_verbose(results):
    by_bucket = defaultdict(list)
    for r in results:
        by_bucket[r['bucket']].append(r)
    for b in ['FN_CANDIDATE', 'FN_NEEDS_EXPR', 'PROC',
              'VOID_PROC', 'EMPTY_BODY', 'UNKNOWN']:
        if b not in by_bucket:
            continue
        print(f"\n=== {b} ({len(by_bucket[b])}) ===")
        for r in by_bucket[b]:
            arrow = ' -> T' if r['has_arrow'] else ''
            print(f"  {r['file']}:{r['line']}  {r['name']}{arrow}")
            if b == 'FN_NEEDS_EXPR':
                print(f"      expr: {r['detail'][1]}")


def main():
    ap = argparse.ArgumentParser(description=__doc__,
                                 formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument('root', nargs='?', default='.',
                    help='repo root to scan (default: .)')
    ap.add_argument('--verbose', action='store_true',
                    help='list every declaration by bucket')
    ap.add_argument('--json', action='store_true',
                    help='emit machine-readable JSON')
    ap.add_argument('--ignore', action='append', default=[],
                    help='additional directory names to skip')
    args = ap.parse_args()

    root = Path(args.root).resolve()
    if not root.is_dir():
        print(f"error: {root} is not a directory", file=sys.stderr)
        return 2

    ignores = set(DEFAULT_IGNORES) | set(args.ignore)
    results = audit(root, ignores)

    if args.json:
        json.dump(results, sys.stdout, indent=2)
        print()
        return 0

    print_table(results, root)
    if args.verbose:
        print_verbose(results)
    return 0


if __name__ == '__main__':
    sys.exit(main())