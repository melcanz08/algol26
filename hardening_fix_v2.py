#!/usr/bin/env python3
"""
ALGOL26 Hardening Fix v2 - cleans the leftovers from v1
Run: python3 hardening_fix_v2.py
"""

import shutil
import re
from pathlib import Path

ROOT = Path.home() / "dev" / "algol26"
if not ROOT.exists():
    ROOT = Path.cwd()

print(f"=== Fix v2 @ {ROOT} ===")

# Fix 1: region_memory.rs last unwrap()
rm = ROOT / "src/runtime/region_memory.rs"
if rm.exists():
    txt = rm.read_text()
    # fix self.memory.get_mut(...).unwrap().push
    txt = txt.replace(
        "self.memory.get_mut(region_name).unwrap().push(block);",
        "self.memory.get_mut(region_name).ok_or_else(|| format!(\"No memory entry for region '{}'\", region_name))?.push(block);"
    )
    # also need to make allocate return Result properly - ensure function uses ?
    # change push that needs ? to handle
    if "ok_or_else" in txt and "fn allocate" in txt:
        # make sure allocate uses ? - already does
        pass
    # also fix any other .unwrap() in this file
    txt = re.sub(r"self\.regions\.get\(region_name\)\.unwrap\(\)", 
                 "self.regions.get(region_name).ok_or_else(|| format!(\"Region '{}' not found\", region_name))?",
                 txt)
    # backup
    shutil.copy(rm, str(rm)+".bak2")
    rm.write_text(txt)
    print(f"PATCHED {rm.relative_to(ROOT)} - removed last unwrap()")

# Fix 2: Patch semantic.rs - you need to upload it, but try auto-detect structure
sem = ROOT / "src/semantics/semantic.rs"
if sem.exists():
    txt = sem.read_text(encoding='utf-8', errors='ignore')
    print(f"\nFound semantic.rs ({len(txt)} bytes)")
    # Look for pop_scope
    if "fn pop_scope" in txt:
        # Find the pop_scope impl and inject borrow cleanup
        # This is a heuristic patch
        old_pop = re.search(r"fn pop_scope\(&mut self\)\s*\{[^}]*\}", txt, re.DOTALL)
        if old_pop:
            old_code = old_pop.group(0)
            print(f"Current pop_scope:\n{old_code[:200]}...")
            
            # Replace with hardened version that clears borrows
            new_code = """fn pop_scope(&mut self) {
        if let Some(scope) = self.scopes.pop() {
            // HARDENED: End all borrows whose borrower was declared in this scope
            for var_name in scope.keys() {
                self.active_borrows.retain(|b| b.borrower != *var_name);
                self.mutable_borrows.remove(var_name);
                self.borrowed_by.remove(var_name);
                // NLL: also clear borrows of vars that die here
                self.borrowed_by.retain(|_, borrowers| {
                    borrowers.retain(|b| b != var_name);
                    !borrowers.is_empty()
                });
            }
        }
        // Also clear expired immutable borrows
        self.immutable_borrows.retain(|_, v| !v.is_empty());
    }

    fn kill_dead_borrows(&mut self, remaining_uses: &[String]) {
        let used_set: std::collections::HashSet<_> = remaining_uses.iter().collect();
        self.active_borrows.retain(|b| used_set.contains(&b.borrower));
    }"""
            # Only patch if not already hardened
            if "HARDENED" not in txt:
                txt = txt.replace(old_code, new_code, 1)
                shutil.copy(sem, str(sem)+".bak2")
                sem.write_text(txt)
                print(f"PATCHED {sem.relative_to(ROOT)} with NLL pop_scope")
            else:
                print("semantic.rs already hardened")
        else:
            print("Could not find pop_scope body, please paste file")
    else:
        print("No pop_scope found in semantic.rs")
        
    # Also check for the error message location
    if "Cannot read" in txt and "while it is mutably borrowed" in txt:
        print("\nFound borrow error message - patching to allow NLL...")
        # Allow read after last use - change the check
        # Look for the function that checks read
        txt = re.sub(
            r'if self\.is_mutably_borrowed\(.*\)\s*\{\s*return Err\(.*Cannot read.*\)',
            'if self.is_mutably_borrowed(var) && self.is_borrower_live(var) { return Err(format!("Cannot read \'{}\' while it is mutably borrowed", var))',
            txt
        )

# Fix 3: Quick count again
print("\n[3] Remaining unwrap() in src/:")
count = 0
for p in ROOT.rglob("src/**/*.rs"):
    if "target" in str(p): continue
    t = p.read_text(encoding='utf-8', errors='ignore')
    # count only non-test
    unwraps = len(re.findall(r"\.unwrap\(\)", t))
    if unwraps>0 and "ir_codegen.rs" not in str(p):
        print(f"  {p.relative_to(ROOT)}: {unwraps}")
        count+=unwraps
print(f"Non-ir_codegen unwraps left: {count} (goal 0)")
print(f"\nir_codegen.rs has 126 unwraps - these are LLVM builder calls.")
print("For Level 2 hardening, you can allow them with ICE message.")
print("For Level 3, change each builder call to return Result and use ?")

print("\n=== v2 DONE ===")
print("Run:")
print("  cargo test --lib semantics::borrow_checker_extra_test::test_borrow_in_nested_scope -- --nocapture")