#!/usr/bin/env python3
"""
Drop the `=` separator from nominal type declarations.

Before:  type UserId = distinct Int
After:   type UserId distinct Int

The lexer rejects bare `=` (with a good error message pointing at
`:=` and `==`), so no separator is the smallest change. Uses only
existing tokens.
"""

from pathlib import Path

ITEMS = Path("src/frontend/parser/items.rs")
TESTS = Path("src/frontend/parser/tests.rs")

# 1. Remove the expect_token for `=`
EDIT_ITEMS = (
    """        let name = self.expect_identifier("type name")?;
        self.expect_token(Token::Assign, "'='")?;

        // `distinct` is required. `type X = Int` is a type alias,
        // not implemented in this ADR.""",
    """        let name = self.expect_identifier("type name")?;

        // `distinct` is required. `type X = Int` (a type alias) is
        // not implemented in this ADR. There is no `=` separator:
        // the lexer reserves bare `=` for future use and rejects it
        // with a message pointing at `:=` and `==`.""",
)

# 2. Update the tests to the new syntax
EDIT_TESTS_1 = (
    """    let src = "type UserId = distinct Int\\n";""",
    """    let src = "type UserId distinct Int\\n";""",
)

EDIT_TESTS_2 = (
    """    let src = "type UserId = Int\\n";""",
    """    let src = "type UserId Int\\n";""",
)


def patch(path, edits):
    src = path.read_text()
    for i, (old, new) in enumerate(edits, start=1):
        count = src.count(old)
        if count != 1:
            print(f"FAIL: {path} — edit {i} matched {count} times; expected 1.")
            print("-" * 60)
            print(old)
            print("-" * 60)
            raise SystemExit(1)
        src = src.replace(old, new, 1)
    path.write_text(src)
    print(f"OK: {path}")


def main():
    if not ITEMS.exists() or not TESTS.exists():
        print("ERROR: run from the repo root.")
        raise SystemExit(1)

    patch(ITEMS, [EDIT_ITEMS])
    patch(TESTS, [EDIT_TESTS_1, EDIT_TESTS_2])

    print()
    print("NEXT: cargo test --release --lib frontend::parser::tests")


if __name__ == "__main__":
    main()
