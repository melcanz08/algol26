#!/usr/bin/env python3
"""Run tests/codegen_probes/*.gol through both backends.

Reports a PASS/FAIL matrix and groups LLVM failures by class.
The class grouping is a substring match against the error
message — approximate by design, useful for triage, not for
assertions.
"""
import subprocess
import sys
from collections import Counter, defaultdict
import pathlib
from pathlib import Path

BIN = 'target/debug/algol26'
PROBE_DIR = Path('tests/codegen_probes')

# Failure classes, most specific first. First matching substring wins.
CLASSES = [
    ('missing_compile_value',  ['list literal value', 'has no LLVM lowering',
                                 'no LLVM array lowering']),
    ('missing_declare_abi',    ['which has no LLVM lowering',
                                 'did not map to a struct']),
    ('value_route',            ['yields no value', 'use an `Instruction::Call`']),
    ('side_table_missing',     ['was never registered', 'has no descriptor',
                                 'no length source', 'not a tracked list']),
    ('non_variable_receiver',  ['requires a variable argument',
                                 'neither a variable nor a list literal',
                                 'non-variable array']),
    ('unsupported_shape',      ['does not support operation']),
    ('invalid_ir',             ['Generated LLVM IR is invalid']),
    ('ice',                    ['unreachable', 'internal error', 'compiler bug']),
]

def classify(err):
    if not err:
        return '-'
    low = err.lower()
    for name, needles in CLASSES:
        for n in needles:
            if n.lower() in low:
                return name
    return 'other'

def run(path, flags):
    # Run from a temp cwd so the compiler's output executable lands
    # in /tmp, not next to the .gol source. Without this, every
    # probe run drops an extensionless binary into
    # tests/codegen_probes/ — which `.gitignore` now catches, but
    # which is cleaner to avoid in the first place.
    import tempfile
    try:
        with tempfile.TemporaryDirectory(prefix='algol26-probe-') as tmpdir:
            # Absolute path to the source so the compiler can find
            # it from the temp cwd.
            src = str(path.resolve())
            p = subprocess.run([str(pathlib.Path(BIN).resolve()), 'run', *flags, src],
                               capture_output=True, text=True, timeout=20,
                               cwd=tmpdir)
    except subprocess.TimeoutExpired:
        return ('TIMEOUT', '', 'timeout')
    out = (p.stdout or '').strip()
    err = (p.stderr or '').strip()
    if p.returncode != 0:
        return ('FAIL', out, err)
    return ('PASS', out, err)

def main():
    probes = sorted(PROBE_DIR.glob('*.gol'))
    if not probes:
        print(f"no probes in {PROBE_DIR}")
        return 1

    rows = []
    for probe in probes:
        tag = probe.stem
        i_status, i_out, i_err = run(probe, ['--interpreter'])
        l_status, l_out, l_err = run(probe, [])
        # Divergence: both ran, but outputs differ.
        if i_status == 'PASS' and l_status == 'PASS' and i_out != l_out:
            verdict = 'DIVERGE'
        elif i_status == 'PASS' and l_status == 'PASS':
            verdict = 'OK'
        elif i_status == 'PASS' and l_status != 'PASS':
            verdict = 'LLVM-GAP'
        elif i_status != 'PASS' and l_status == 'PASS':
            verdict = 'INTERP-GAP'
        else:
            verdict = 'BOTH-FAIL'
        rows.append((tag, verdict, classify(l_err)))

    # The compiler writes .ll files (and, for successful runs, an
    # extensionless executable) next to the source regardless of
    # cwd. Clean them up so the probe directory contains only .gol
    # sources after every run.
    for probe in probes:
        for sibling in probe.parent.glob(f"{probe.stem}.*"):
            if sibling.suffix != ".gol":
                sibling.unlink(missing_ok=True)
        binary = probe.parent / probe.stem
        if binary.exists() and binary.is_file():
            binary.unlink()

    width = max(len(r[0]) for r in rows)
    print(f"\n{'probe':<{width}}  {'verdict':<11}  class")
    print('-' * (width + 26))
    for tag, verdict, cls in rows:
        print(f"{tag:<{width}}  {verdict:<11}  {cls}")

    print()
    verdict_counts = Counter(v for _, v, _ in rows)
    print(f"total {len(rows)} probes")
    for v in ['OK', 'DIVERGE', 'LLVM-GAP', 'INTERP-GAP', 'BOTH-FAIL']:
        if verdict_counts[v]:
            print(f"  {v:<11} {verdict_counts[v]}")

    gap_classes = Counter(c for _, v, c in rows if v == 'LLVM-GAP' and c != '-')
    if gap_classes:
        print("\nLLVM gaps by class:")
        for cls, n in gap_classes.most_common():
            print(f"  {cls:<24} {n}")
    return 0

if __name__ == '__main__':
    sys.exit(main())
