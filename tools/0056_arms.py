#!/usr/bin/env python3
"""ADR 0056 step 2: arms for Stmt::DerefAssign.

Three exhaustive-match sites flagged by rustc:
  1. src/compiler/passes/type_table_complete.rs - real arm
  2. src/semantics/analyzer/stmt.rs             - todo!()
  3. src/semantics/analyzer/ownership.rs        - real arm

Refuses to run twice on any file.
"""
import pathlib


def update(path, marker_absent, edits, post_checks):
    p = pathlib.Path(path)
    s = p.read_text()
    for m in marker_absent:
        assert m not in s, f"{path}: {m!r} already present - refusing to run twice"
    for old, new, what in edits:
        n = s.count(old)
        assert n == 1, f"{path}: anchor for {what!r} has {n} occurrences, expected 1"
        s = s.replace(old, new, 1)
    for check in post_checks:
        assert check in s, f"{path}: post-check failed: {check!r} not found"
    p.write_text(s)
    print(f"  [{path}] ok")


# --- 1. type_table_complete.rs: real arm ---
update(
    'src/compiler/passes/type_table_complete.rs',
    marker_absent=['Stmt::DerefAssign'],
    edits=[
        (
            '            Stmt::Assign { value, .. } => self.visit_expr(value),',
            '''            Stmt::Assign { value, .. } => self.visit_expr(value),
            Stmt::DerefAssign { target, value, .. } => {
                self.visit_expr(target);
                self.visit_expr(value);
            }''',
            'DerefAssign arm in visit_stmt',
        ),
    ],
    post_checks=[
        'Stmt::DerefAssign { target, value, .. } => {',
    ],
)

# --- 2. analyzer/ownership.rs: real arm for collect_deferred_captures ---
update(
    'src/semantics/analyzer/ownership.rs',
    marker_absent=['Stmt::DerefAssign'],
    edits=[
        (
            '''            Stmt::Expression(expr) => {
                self.collect_expr_captures(expr, captured);
            }
        }
    }
    pub(super) fn collect_expr_captures''',
            '''            Stmt::Expression(expr) => {
                self.collect_expr_captures(expr, captured);
            }
            Stmt::DerefAssign { target, value, .. } => {
                self.collect_expr_captures(target, captured);
                self.collect_expr_captures(value, captured);
            }
        }
    }
    pub(super) fn collect_expr_captures''',
            'DerefAssign arm in collect_deferred_captures',
        ),
    ],
    post_checks=[
        'Stmt::DerefAssign { target, value, .. } => {',
    ],
)

# --- 3. analyzer/stmt.rs: todo!() placeholder ---
update(
    'src/semantics/analyzer/stmt.rs',
    marker_absent=['Stmt::DerefAssign'],
    edits=[
        (
            '''                self.state
                    .borrows
                    .retain(|_, b| !b.place.overlaps(&target_place));
            }
        }
        Ok(())''',
            '''                self.state
                    .borrows
                    .retain(|_, b| !b.place.overlaps(&target_place));
            }
            Stmt::DerefAssign { .. } => {
                todo!("ADR 0056: type-check *r := v (analyzer/stmt.rs)")
            }
        }
        Ok(())''',
            'DerefAssign todo arm in analyze_stmt',
        ),
    ],
    post_checks=[
        'Stmt::DerefAssign { .. } => {',
    ],
)

print("done: 3 files updated")
