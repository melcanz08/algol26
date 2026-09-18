#!/usr/bin/env python3
"""
Apply rustc's suggested fixes for pattern-position errors from the JSON
diagnostics stream. Handles E0027 (missing field in pattern) and E0532
(expected unit variant, got tuple variant).

Constructor errors (E0063, E0061) are intentionally *not* handled here —
those need real span values and are fixed by hand.
"""
import json
import subprocess
import sys
from collections import defaultdict

TARGET_CODES = {"E0027", "E0532"}

def get_diagnostics():
    result = subprocess.run(
        ["cargo", "build", "--message-format=json", "--all-features", "--lib"],
        capture_output=True, text=True,
    )
    out = []
    for line in result.stdout.splitlines():
        try:
            msg = json.loads(line)
        except json.JSONDecodeError:
            continue
        if msg.get("reason") != "compiler-message":
            continue
        diag = msg.get("message", {})
        code = diag.get("code") or {}
        if code.get("code") not in TARGET_CODES:
            continue
        out.append(diag)
    return out

def collect_fixes(diagnostics):
    """
    For each diagnostic, look at its children (suggestions). Take the first
    child whose spans are all machine-applicable. Return a flat list of
    (file, byte_start, byte_end, replacement) tuples.
    """
    fixes = []
    for diag in diagnostics:
        for child in diag.get("children", []):
            spans = child.get("spans", [])
            if not spans:
                continue
            ok = True
            for s in spans:
                if s.get("suggested_replacement") is None:
                    ok = False
                    break
                if s.get("suggestion_applicability") not in (
                    "MachineApplicable", "MaybeIncorrect",
                ):
                    ok = False
                    break
                if not s.get("file_name"):
                    ok = False
                    break
            if not ok:
                continue
            for s in spans:
                fixes.append({
                    "file": s["file_name"],
                    "start": s["byte_start"],
                    "end": s["byte_end"],
                    "replacement": s["suggested_replacement"],
                })
            break  # only the first fully-applicable suggestion
    return fixes

def apply_fixes(fixes, dry_run=False):
    by_file = defaultdict(list)
    for fix in fixes:
        by_file[fix["file"]].append(fix)

    total = 0
    for path, file_fixes in sorted(by_file.items()):
        with open(path, "r") as f:
            content = f.read()
        # Apply in reverse byte-offset order so earlier offsets remain valid.
        for fix in sorted(file_fixes, key=lambda x: -x["start"]):
            content = content[:fix["start"]] + fix["replacement"] + content[fix["end"]:]
        if not dry_run:
            with open(path, "w") as f:
                f.write(content)
        print(f"  {path}: {len(file_fixes)} fix(es)")
        total += len(file_fixes)
    print(f"Total: {total} fixes across {len(by_file)} file(s).")

def main():
    dry_run = "--dry-run" in sys.argv
    print("Collecting diagnostics...")
    diags = get_diagnostics()
    print(f"  {len(diags)} target diagnostic(s)")
    fixes = collect_fixes(diags)
    print(f"  {len(fixes)} suggested fix(es)")
    if not fixes:
        print("Nothing to apply.")
        return
    apply_fixes(fixes, dry_run=dry_run)

if __name__ == "__main__":
    main()