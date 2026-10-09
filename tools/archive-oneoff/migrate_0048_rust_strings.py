#!/usr/bin/env python3
"""ADR 0048 phase 4b -- migrate `function` inside single-line Rust strings.

Rust unit tests embed .gol source in string literals with `\n` escapes.
`tools/migrate_0048_rust_tests.py` walks physical lines, so it only saw
multi-line raw strings. This script finds each `"..."` literal, decodes
its escape sequences, applies the same conservative classifier, and
re-encodes. Raw strings (`r#"..."#`) are already handled by the previous
script and are left alone.

Usage:
  python3 tools/migrate_0048_rust_strings.py --dry-run
  python3 tools/migrate_0048_rust_strings.py --dry-run -v
  python3 tools/migrate_0048_rust_strings.py
  python3 tools/migrate_0048_rust_strings.py src/frontend/parser/tests.rs
"""
import argparse
import pathlib
import re
import sys

DECL_RE = re.compile(
    r'^(?P<indent>[ \t]*)(?P<pub>pub\s+)?(?P<ext>extern\s+(?:"[^"]*"\s+)?)?function\s+(?P<name>\w+)(?P<rest>.*)$'
)
COMMENT_RE = re.compile(r'//.*$')
# Matches a double-quoted Rust string literal, honoring `\"`.
STRING_RE = re.compile(r'"(?:[^"\\]|\\.)*"')


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


def migrate_source(src):
    """Apply classifier to a multi-line .gol source. Returns (new, count)."""
    lines = src.split('\n')
    out = []
    i = 0
    n = len(lines)
    count = 0
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
        out.append(re.sub(r'\bfunction\b', new_kw, lines[i], count=1))
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
        count += 1
        i = body_end
    return '\n'.join(out), count


def unescape(s):
    out = []
    i = 0
    while i < len(s):
        c = s[i]
        if c == '\\' and i + 1 < len(s):
            nxt = s[i + 1]
            out.append({'n': '\n', 't': '\t', 'r': '\r',
                        '"': '"', '\\': '\\'}.get(nxt, '\\' + nxt))
            i += 2
        else:
            out.append(c)
            i += 1
    return ''.join(out)


def escape(s):
    out = []
    for c in s:
        if c == '\n':
            out.append('\\n')
        elif c == '\t':
            out.append('\\t')
        elif c == '\r':
            out.append('\\r')
        elif c == '"':
            out.append('\\"')
        elif c == '\\':
            out.append('\\\\')
        else:
            out.append(c)
    return ''.join(out)


def rewrite_file(path, dry_run, verbose):
    text = path.read_text()
    parts = []
    last_end = 0
    total = 0
    for m in STRING_RE.finditer(text):
        content = m.group(0)[1:-1]
        unescaped = unescape(content)
        if 'function ' not in unescaped:
            continue
        migrated, n = migrate_source(unescaped)
        if n == 0:
            continue
        parts.append(text[last_end:m.start()])
        parts.append('"' + escape(migrated) + '"')
        last_end = m.end()
        total += n
        if verbose:
            print(f"  {path}: {n} decl(s) in string at offset {m.start()}")
    if total == 0:
        return 0
    parts.append(text[last_end:])
    if not dry_run:
        path.write_text(''.join(parts))
    return total


def main():
    ap = argparse.ArgumentParser(description=__doc__,
                                 formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument('paths', nargs='*', default=['src'])
    ap.add_argument('--dry-run', action='store_true')
    ap.add_argument('-v', '--verbose', action='store_true')
    args = ap.parse_args()

    targets = []
    for p in args.paths:
        pp = pathlib.Path(p)
        if pp.is_file():
            targets.append(pp)
        elif pp.is_dir():
            targets.extend(sorted(pp.rglob('*.rs')))
        else:
            print(f"warning: {p} not found", file=sys.stderr)

    total = 0
    for f in targets:
        total += rewrite_file(f, dry_run=args.dry_run, verbose=args.verbose)

    verb = 'would migrate' if args.dry_run else 'migrated'
    print(f"{verb}: {total} declarations in Rust string literals")
    return 0


if __name__ == '__main__':
    sys.exit(main())
