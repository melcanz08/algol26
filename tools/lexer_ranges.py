#!/usr/bin/env python3
"""
Widen lexer token spans from points to ranges.

Reads src/frontend/lexer/mod.rs and rewrites:
  - token_positions: Vec<(usize, usize)>  -> Vec<(usize, usize, usize)>
  - char_positions:  Vec<usize>           -> Vec<(usize, usize)>  # (start, end-exclusive)
  - all positions.push(x) sites in tokenize_expression
  - the final zip that constructs SpannedToken

Every replacement is exact-match. Any failure aborts without writing.

IMPORTANT: src/frontend/lexer/decl.rs shares the positions signature
and needs the SAME transformation by hand (this script cannot see it).
"""

from pathlib import Path

PATH = Path("src/frontend/lexer/mod.rs")

REPLACEMENTS = [
    # ─── 1. token_positions type ─────────────────────────────────
    (
        "let mut token_positions: Vec<(usize, usize)> = Vec::new();",
        "let mut token_positions: Vec<(usize, usize, usize)> = Vec::new();",
    ),

    # ─── 2. char_positions type ──────────────────────────────────
    (
        "let mut char_positions: Vec<usize> = Vec::new();",
        "let mut char_positions: Vec<(usize, usize)> = Vec::new();",
    ),

    # ─── 3. dummy point-position pushes (4 sites, all become triples) ─
    (
        "token_positions.push((current_line, 1));",
        "token_positions.push((current_line, 1, 1));",
    ),
    (
        "token_positions.push((line_number, 1));",
        "token_positions.push((line_number, 1, 1));",
    ),
    (
        "token_positions.push((current_line, 0));",
        "token_positions.push((current_line, 0, 0));",
    ),

    # ─── 4. tokenize_line signature ──────────────────────────────
    (
        "fn tokenize_line(\n"
        "        trimmed: &str,\n"
        "        line_number: usize,\n"
        "        line: &str,\n"
        "        tokens: &mut Vec<Token>,\n"
        "        positions: &mut Vec<usize>,\n"
        "    ) -> Result<()> {",
        "fn tokenize_line(\n"
        "        trimmed: &str,\n"
        "        line_number: usize,\n"
        "        line: &str,\n"
        "        tokens: &mut Vec<Token>,\n"
        "        positions: &mut Vec<(usize, usize)>,\n"
        "    ) -> Result<()> {",
    ),

    # ─── 5. tokenize_expression signature ────────────────────────
    (
        "fn tokenize_expression(\n"
        "        expr: &str,\n"
        "        line_number: usize,\n"
        "        line: &str,\n"
        "        tokens: &mut Vec<Token>,\n"
        "        positions: &mut Vec<usize>,\n"
        "    ) -> Result<()> {",
        "fn tokenize_expression(\n"
        "        expr: &str,\n"
        "        line_number: usize,\n"
        "        line: &str,\n"
        "        tokens: &mut Vec<Token>,\n"
        "        positions: &mut Vec<(usize, usize)>,\n"
        "    ) -> Result<()> {",
    ),

    # ─── 6. string branch ────────────────────────────────────────
    (
        "            } else if c == '\"' {\n"
        "                positions.push(position);\n"
        "                chars.next();\n"
        "                position += 1;\n"
        "                let string_content =\n"
        "                    Lexer::read_string(&mut chars, &mut position, line_number, line)?;\n"
        "                tokens.push(Token::StringLit(string_content));",
        "            } else if c == '\"' {\n"
        "                let start = position;\n"
        "                chars.next();\n"
        "                position += 1;\n"
        "                let string_content =\n"
        "                    Lexer::read_string(&mut chars, &mut position, line_number, line)?;\n"
        "                tokens.push(Token::StringLit(string_content));\n"
        "                positions.push((start, position));",
    ),

    # ─── 7. identifier branch ────────────────────────────────────
    (
        "            } else if c.is_alphabetic() || c == '_' {\n"
        "                positions.push(position);\n"
        "                let ident = Lexer::read_identifier(&mut chars);\n"
        "                position += ident.chars().count();\n"
        "                Lexer::classify_identifier(ident, tokens);",
        "            } else if c.is_alphabetic() || c == '_' {\n"
        "                let start = position;\n"
        "                let ident = Lexer::read_identifier(&mut chars);\n"
        "                position += ident.chars().count();\n"
        "                Lexer::classify_identifier(ident, tokens);\n"
        "                positions.push((start, position));",
    ),

    # ─── 8. number branch ────────────────────────────────────────
    (
        "            } else if c.is_numeric() {\n"
        "                positions.push(position);\n"
        "                let (token, len) = Lexer::read_number(&mut chars)?;\n"
        "                tokens.push(token);\n"
        "                position += len;",
        "            } else if c.is_numeric() {\n"
        "                let start = position;\n"
        "                let (token, len) = Lexer::read_number(&mut chars)?;\n"
        "                tokens.push(token);\n"
        "                position += len;\n"
        "                positions.push((start, position));",
    ),

    # ─── 9. operator branch ──────────────────────────────────────
    (
        "            } else {\n"
        "                positions.push(position);\n"
        "                Lexer::handle_operator(&mut chars, &mut position, line_number, line, tokens)?;\n"
        "            }",
        "            } else {\n"
        "                let start = position;\n"
        "                Lexer::handle_operator(&mut chars, &mut position, line_number, line, tokens)?;\n"
        "                positions.push((start, position));\n"
        "            }",
    ),

    # ─── 10. aggregation loop ────────────────────────────────────
    (
        "            for col in &char_positions {\n"
        "                token_positions.push((current_line, base_column + col));\n"
        "            }",
        "            for (start, end) in &char_positions {\n"
        "                token_positions.push((\n"
        "                    current_line,\n"
        "                    base_column + start,\n"
        "                    base_column + end.saturating_sub(1),\n"
        "                ));\n"
        "            }",
    ),

    # ─── 11. final zip -> SpannedToken ───────────────────────────
    (
        "        let spanned: Vec<SpannedToken> = tokens\n"
        "            .into_iter()\n"
        "            .zip(token_positions)\n"
        "            .map(|(token, (line, column))| SpannedToken {\n"
        "                token,\n"
        "                span: Span::point(line, column),\n"
        "            })\n"
        "            .collect();",
        "        let spanned: Vec<SpannedToken> = tokens\n"
        "            .into_iter()\n"
        "            .zip(token_positions)\n"
        "            .map(|(token, (line, start, end))| SpannedToken {\n"
        "                token,\n"
        "                span: Span::new(line, start, line, end),\n"
        "            })\n"
        "            .collect();",
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
            print("File may have been reformatted by cargo fmt.")
            print("Nothing written. Fix the site by hand and rerun.")
            raise SystemExit(1)
        if count > 1 and i != 3 and i != 4:
            # Sites 3 and 4 are intentionally multi-occurrence.
            print(f"FAIL: site {i} matched {count} times; expected 1.")
            raise SystemExit(1)
        src = src.replace(old, new)

    if src == original:
        print("Nothing changed. Already migrated?")
        return

    PATH.write_text(src)
    print(f"OK: patched {PATH}")
    print()
    print("NEXT: src/frontend/lexer/decl.rs needs the SAME change by hand.")
    print("      - signature:  positions: &mut Vec<usize>  ->  &mut Vec<(usize, usize)>")
    print("      - for each `positions.push(position);` before a token:")
    print("            let start = position;")
    print("            ... advance position ...")
    print("            positions.push((start, position));")
    print("      Same for any positions.push sites in decl.rs.")


if __name__ == "__main__":
    main()