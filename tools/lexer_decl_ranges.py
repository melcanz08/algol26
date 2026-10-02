#!/usr/bin/env python3
"""
Widen token spans in src/frontend/lexer/decl.rs from points to ranges.

Matches the convention the main lexer patch uses:
    char_offsets entries become (start_offset, end_offset_exclusive)

The aggregation loop in mod.rs converts these to inclusive end columns.

Every replacement is exact-match. Any failure aborts without writing.
"""

from pathlib import Path

PATH = Path("src/frontend/lexer/decl.rs")

REPLACEMENTS = [
    # ─── 1. Both function signatures (2 occurrences) ─────────────
    (
        "char_offsets: &mut Vec<usize>,",
        "char_offsets: &mut Vec<(usize, usize)>,",
    ),

    # ─── 2. Keyword push in parse_declaration ────────────────────
    (
        "        // The keyword itself starts at offset 0.\n"
        "        tokens.push(keyword);\n"
        "        char_offsets.push(0);",
        "        // The keyword itself starts at offset 0 and ends at\n"
        "        // `keyword_len` (exclusive).\n"
        "        tokens.push(keyword);\n"
        "        char_offsets.push((0, keyword_len));",
    ),

    # ─── 3. Name push in parse_declaration ───────────────────────
    (
        "        let name_len = name.chars().count();\n"
        "        tokens.push(Token::Identifier(name));\n"
        "        char_offsets.push(name_start);",
        "        let name_len = name.chars().count();\n"
        "        tokens.push(Token::Identifier(name));\n"
        "        char_offsets.push((name_start, name_start + name_len));",
    ),

    # ─── 4–10. Single-char tokens in parse_signature ─────────────
    (
        "                '(' => {\n"
        "                    tokens.push(Token::LParen);\n"
        "                    char_offsets.push(offset);\n"
        "                }",
        "                '(' => {\n"
        "                    tokens.push(Token::LParen);\n"
        "                    char_offsets.push((offset, offset + 1));\n"
        "                }",
    ),
    (
        "                ')' => {\n"
        "                    tokens.push(Token::RParen);\n"
        "                    char_offsets.push(offset);\n"
        "                }",
        "                ')' => {\n"
        "                    tokens.push(Token::RParen);\n"
        "                    char_offsets.push((offset, offset + 1));\n"
        "                }",
    ),
    (
        "                ':' => {\n"
        "                    tokens.push(Token::Colon);\n"
        "                    char_offsets.push(offset);\n"
        "                }",
        "                ':' => {\n"
        "                    tokens.push(Token::Colon);\n"
        "                    char_offsets.push((offset, offset + 1));\n"
        "                }",
    ),
    (
        "                ',' => {\n"
        "                    tokens.push(Token::Comma);\n"
        "                    char_offsets.push(offset);\n"
        "                }",
        "                ',' => {\n"
        "                    tokens.push(Token::Comma);\n"
        "                    char_offsets.push((offset, offset + 1));\n"
        "                }",
    ),
    (
        "                '<' => {\n"
        "                    tokens.push(Token::Lt);\n"
        "                    char_offsets.push(offset);\n"
        "                }",
        "                '<' => {\n"
        "                    tokens.push(Token::Lt);\n"
        "                    char_offsets.push((offset, offset + 1));\n"
        "                }",
    ),
    (
        "                '>' => {\n"
        "                    tokens.push(Token::Gt);\n"
        "                    char_offsets.push(offset);\n"
        "                }",
        "                '>' => {\n"
        "                    tokens.push(Token::Gt);\n"
        "                    char_offsets.push((offset, offset + 1));\n"
        "                }",
    ),
    (
        "                '&' => {\n"
        "                    tokens.push(Token::Ampersand);\n"
        "                    char_offsets.push(offset);\n"
        "                }",
        "                '&' => {\n"
        "                    tokens.push(Token::Ampersand);\n"
        "                    char_offsets.push((offset, offset + 1));\n"
        "                }",
    ),

    # ─── 11. Arrow `->` (two chars) ──────────────────────────────
    (
        "                '-' => {\n"
        "                    if matches!(chars.peek(), Some(&(_, '>'))) {\n"
        "                        chars.next(); // consume '>'\n"
        "                        tokens.push(Token::Arrow);\n"
        "                        char_offsets.push(offset);\n"
        "                    }\n"
        "                    // A lone '-' in a signature is skipped for now.\n"
        "                }",
        "                '-' => {\n"
        "                    if matches!(chars.peek(), Some(&(_, '>'))) {\n"
        "                        chars.next(); // consume '>'\n"
        "                        tokens.push(Token::Arrow);\n"
        "                        char_offsets.push((offset, offset + 2));\n"
        "                    }\n"
        "                    // A lone '-' in a signature is skipped for now.\n"
        "                }",
    ),

    # ─── 12. Identifier in parse_signature ───────────────────────
    (
        "                    if let Some(token) = KEYWORDS.get(ident.as_str()) {\n"
        "                        tokens.push(token.clone());\n"
        "                    } else {\n"
        "                        tokens.push(Token::Identifier(ident));\n"
        "                    }\n"
        "                    char_offsets.push(offset);",
        "                    let ident_len = ident.chars().count();\n"
        "                    if let Some(token) = KEYWORDS.get(ident.as_str()) {\n"
        "                        tokens.push(token.clone());\n"
        "                    } else {\n"
        "                        tokens.push(Token::Identifier(ident));\n"
        "                    }\n"
        "                    char_offsets.push((offset, offset + ident_len));",
    ),
]


def main():
    if not PATH.exists():
        print(f"ERROR: {PATH} not found. Run from the repo root.")
        raise SystemExit(1)

    src = PATH.read_text()
    original = src

    for i, (old, new) in enumerate(REPLACEMENTS, start=1):
        count = src.count(old)
        if count == 0:
            print(f"FAIL: site {i} did not match.")
            print("─" * 60)
            print(old)
            print("─" * 60)
            print("Nothing written. Fix the site by hand and rerun.")
            raise SystemExit(1)
        if i != 1 and count > 1:
            # Site 1 is intentionally multi-occurrence (two signatures).
            print(f"FAIL: site {i} matched {count} times; expected 1.")
            raise SystemExit(1)
        src = src.replace(old, new)

    if src == original:
        print("Nothing changed. Already migrated?")
        return

    PATH.write_text(src)
    print(f"OK: patched {PATH}")
    print()
    print("NEXT:")
    print("  1. Verify src/frontend/lexer/mod.rs was patched by lexer_ranges.py")
    print("  2. cargo fmt")
    print("  3. cargo clippy --all-targets -- -D warnings")
    print("  4. cargo test --release")


if __name__ == "__main__":
    main()