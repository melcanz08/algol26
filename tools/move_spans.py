#!/usr/bin/env python3
"""
Wire move spans through ownership.rs, stmt.rs, and expr.rs.

Prerequisite: src/semantics/state/mod.rs has already been patched by
tools/state_moved_at.py (move_out takes a Span).

Changes:
  ownership.rs  mark_moved(name) -> mark_moved(name, span)
  ownership.rs  add moved_at(name) accessor
  stmt.rs       pass value.span() to mark_moved
  expr.rs       attach secondary span to "use of moved variable"
"""

from pathlib import Path

OWNERSHIP = Path("src/semantics/analyzer/ownership.rs")
STMT = Path("src/semantics/analyzer/stmt.rs")
EXPR = Path("src/semantics/analyzer/expr.rs")

EDIT_OWNERSHIP_1 = (
    """    pub(super) fn mark_moved(&mut self, name: &str) {
        self.state.move_out(name);
    }""",
    """    pub(super) fn mark_moved(&mut self, name: &str, span: Span) {
        self.state.move_out(name, span);
    }

    /// The source location of the move that consumed `name`, if any.
    /// Returns `None` when the variable was never moved, or was
    /// moved by a path that did not record a span (e.g. a join
    /// where neither branch preserved one).
    pub(super) fn moved_at(&self, name: &str) -> Option<Span> {
        self.state.vars.get(name).and_then(|s| s.moved_at)
    }""",
)

EDIT_STMT = (
    """                    if source != name && !self.is_moved(source) && !self.is_type_copy(&value_type) {
                        self.mark_moved(source);
                    }""",
    """                    if source != name && !self.is_moved(source) && !self.is_type_copy(&value_type) {
                        self.mark_moved(source, value.span());
                    }""",
)

EDIT_EXPR = (
    """            ExprKind::Var(name, span) => {
                if self.is_moved(name) {
                    return Err(CompileError::at(
                        *span,
                        &format!("Use of moved variable '{}'", name),
                        ErrorCode::E0007,
                    )
                    .with_suggestion(
                        "Variable ownership was transferred and cannot be used in this scope",
                    ));
                }""",
    """            ExprKind::Var(name, span) => {
                if self.is_moved(name) {
                    let mut err = CompileError::at(
                        *span,
                        &format!("Use of moved variable '{}'", name),
                        ErrorCode::E0007,
                    )
                    .with_suggestion(
                        "Variable ownership was transferred and cannot be used in this scope",
                    );
                    if let Some(moved_span) = self.moved_at(name) {
                        err = err.with_secondary(moved_span, format!("`{}` moved here", name));
                    }
                    return Err(err);
                }""",
)


def patch(path, edits):
    src = path.read_text()
    for old, new in edits:
        if src.count(old) != 1:
            print(f"FAIL: {path} — pattern matched {src.count(old)} times; expected 1")
            print("─" * 60)
            print(old[:200])
            print("─" * 60)
            raise SystemExit(1)
        src = src.replace(old, new, 1)
    path.write_text(src)
    print(f"OK: {path}")


def main():
    for path in (OWNERSHIP, STMT, EXPR):
        if not path.exists():
            print(f"ERROR: {path} not found. Run from the repo root.")
            raise SystemExit(1)

    patch(OWNERSHIP, [EDIT_OWNERSHIP_1])
    patch(STMT, [EDIT_STMT])
    patch(EXPR, [EDIT_EXPR])

    print()
    print("NEXT: cargo fmt && cargo clippy --all-targets -- -D warnings && cargo test --release")


if __name__ == "__main__":
    main()