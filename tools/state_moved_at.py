#!/usr/bin/env python3
"""
Add `moved_at: Option<Span>` to VarState in src/semantics/state/mod.rs.

- Adds the field with default None.
- Adds `VarState::moved_from(span)` constructor.
- Updates `moved()` to set moved_at: None.
- Updates `join` to merge moved_at: self.moved_at.or(other.moved_at).
- Changes `move_out(&str)` to `move_out(&str, Span)`.
- Fixes the one call site in the test module.
"""

from pathlib import Path

PATH = Path("src/semantics/state/mod.rs")

REPLACEMENTS = [
    # 1. import
    (
        "use std::collections::{HashMap, HashSet};",
        "use crate::common::span::Span;\nuse std::collections::{HashMap, HashSet};",
    ),

    # 2. field on VarState
    (
        """#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VarState {
    pub init: InitState,
    pub ownership: OwnershipState,
}""",
        """#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VarState {
    pub init: InitState,
    pub ownership: OwnershipState,
    /// Where the value was moved out, if it has been. `None` while
    /// the binding still owns its value; `Some(span)` once a move
    /// has occurred. Preserved across branch joins so a use-after-
    /// move diagnostic can point at the originating move site.
    pub moved_at: Option<Span>,
}""",
    ),

    # 3. available()
    (
        """    pub fn available() -> Self {
        VarState {
            init: InitState::Initialized,
            ownership: OwnershipState::Owned,
        }
    }""",
        """    pub fn available() -> Self {
        VarState {
            init: InitState::Initialized,
            ownership: OwnershipState::Owned,
            moved_at: None,
        }
    }""",
    ),

    # 4. uninitialized()
    (
        """    pub fn uninitialized() -> Self {
        VarState {
            init: InitState::Uninitialized,
            ownership: OwnershipState::Owned,
        }
    }""",
        """    pub fn uninitialized() -> Self {
        VarState {
            init: InitState::Uninitialized,
            ownership: OwnershipState::Owned,
            moved_at: None,
        }
    }""",
    ),

    # 5. moved() + new moved_from()
    (
        """    /// A variable that has been moved out.
    pub fn moved() -> Self {
        VarState {
            init: InitState::Initialized,
            ownership: OwnershipState::Moved,
        }
    }""",
        """    /// A variable that has been moved out, with no known site.
    /// Prefer `moved_from(span)` at real move sites so diagnostics
    /// can point at the originating expression.
    pub fn moved() -> Self {
        VarState {
            init: InitState::Initialized,
            ownership: OwnershipState::Moved,
            moved_at: None,
        }
    }

    /// A variable moved out at a known source location.
    pub fn moved_from(span: Span) -> Self {
        VarState {
            init: InitState::Initialized,
            ownership: OwnershipState::Moved,
            moved_at: Some(span),
        }
    }""",
    ),

    # 6. join
    (
        """    pub fn join(self, other: Self) -> Self {
        VarState {
            init: self.init.join(other.init),
            ownership: self.ownership.join(other.ownership),
        }
    }""",
        """    pub fn join(self, other: Self) -> Self {
        VarState {
            init: self.init.join(other.init),
            ownership: self.ownership.join(other.ownership),
            // Prefer whichever branch has a move span. If both do,
            // take self's; deterministic given join order.
            moved_at: self.moved_at.or(other.moved_at),
        }
    }""",
    ),

    # 7. move_out signature and body
    (
        """    pub fn move_out(&mut self, name: &str) {
        self.vars.insert(name.to_string(), VarState::moved());
        self.borrows.retain(|_, b| b.place != name);
    }""",
        """    pub fn move_out(&mut self, name: &str, span: Span) {
        self.vars.insert(name.to_string(), VarState::moved_from(span));
        self.borrows.retain(|_, b| b.place != name);
    }""",
    ),

    # 8. move_ends_borrow test
    (
        """        assert!(s.is_borrowed("x"));
        s.move_out("x");
        assert!(!s.is_borrowed("x"));""",
        """        assert!(s.is_borrowed("x"));
        s.move_out("x", Span::point(1, 1));
        assert!(!s.is_borrowed("x"));""",
    ),
]


def main():
    if not PATH.exists():
        print(f"ERROR: {PATH} not found. Run from the repo root.")
        raise SystemExit(1)

    src = PATH.read_text()
    for i, (old, new) in enumerate(REPLACEMENTS, start=1):
        if src.count(old) != 1:
            print(f"FAIL: site {i} matched {src.count(old)} times; expected 1.")
            print("─" * 60)
            print(old[:200])
            print("─" * 60)
            raise SystemExit(1)
        src = src.replace(old, new, 1)

    PATH.write_text(src)
    print(f"OK: patched {PATH}")


if __name__ == "__main__":
    main()