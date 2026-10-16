# ADR 0057: Statement Boundaries

Status: Accepted (implementation complete; tests pass)

> **Note (commit `123de0b`):** The "end-of-line operator
> continuation" case (`val total := a +` followed by an indented
> `b`) was originally out of scope for this ADR. It has since been
> fixed in the lexer — see "End-of-line operator continuations"
> under Decision. The fixture `pass_op_at_end_of_line.gol` flipped
> from `EXPECT: REJECT` to `EXPECT: COMPILE`.

## Context

ADR 0001 established significant indentation: the lexer emits
`Indent` when a block opens and `Dedent` when it closes. It emits
these tokens only at *changes* in indentation depth. Between two
statements at the same depth, the lexer emits no separator.

The parser has therefore always inferred statement boundaries from
the first token of each statement: if that token cannot continue
the previous statement's expression, it must begin a new
statement. This inference has worked for every statement form the
language had before ADR 0056, because those forms begin with
keywords (`var`, `print`, `return`, ...) or identifiers
(`foo := ...`, `foo(args)`), none of which can continue an
expression.

ADR 0056 introduced `*r := v` (deref-assign), whose first token is
`*`. `*` is also the multiplication operator, and it appears at the
start of a line where the previous statement's expression parser
will happily consume it as a continuation:

    proc main()
        var x := 10
        var r := &mut x
        *r := *r + 1        # parsed as: var r := (&mut x) * r := ...
        print(x)

The parser reads line 4 as a continuation of line 3's expression
and errors at the second `*`. This is confirmed by the token dump
of the minimal case, in both the indentation form and the brace
form. The brace form's `{`/`}` delimit the block, not the
statements inside it, so the ambiguity is present there too.

The problem is not specific to `*`. Any token that can both begin
a statement and continue an expression is ambiguous at the start
of a line at the current statement's indentation. The current
enumeration identifies five:

| Token | Statement form it can start | Expression continuation it can be |
|-------|----------------------------|----------------------------------|
| `*`   | deref-assign (ADR 0056)    | binary multiplication, prefix deref |
| `{`   | block (expression position)| record-literal continuation      |
| `[`   | list literal (expr-stmt)   | array-access continuation        |
| `(`   | parenthesized expr (stmt)  | call continuation                |
| `-`   | unary negation (expr-stmt) | binary subtraction               |

`*`, `{`, and `-` are confirmed by direct test. `[` and `(` are
inferred from the parser's structure without a direct reproduction;
they belong to the same class.

Line-wrapped binary operators inside parentheses already work:

    print(a
        - b)

prints `7`. This is a legitimate continuation and any rule we
adopt must preserve it.

## Decision

**A token at the start of a line, at the same column as the current
statement's first token, ends the current statement.** It begins a
new statement.

**Exception:** inside a parenthesized `( )`, bracketed `[ ]`, or
braced `{ }` sub-expression, a line break is a continuation. The
rule does not apply; line-wrapped expressions inside delimiters
remain valid.

### Rationale

The rule is *positional*, not token-specific. It resolves the
whole class of ambiguities with one rule, rather than patching each
token case individually. Any future statement form that begins
with an operator-like token inherits the same disambiguation.

The exception is mandatory, not optional. Without it, the rule
would break `print(a\n    - b)`, which is a real pattern the
current grammar accepts and which no ADR has proposed removing.

### What this forbids

    val total := a
        + b

The `+` at the start of line 2 is at the same column as the
statement's first token (`val`), so by the rule it ends the
statement. To continue an expression across a line break, one of:

  * wrap the expression in parentheses:

        val total := (a
            + b)

The parenthesized form works today. The "operator at end of
previous line" form also works as of commit `123de0b`:

    val total := a +
        b

The original implementation did *not* accept this form: the
lexer emitted an `Indent` before the continuation line, and
`parse_unary` rejected the `Indent` token. That was tracked as
`pass_op_at_end_of_line.gol` under
`tests/adr_examples/0057_boundaries/`, marked `EXPECT: REJECT`.

The gap is now closed by extending the lexer's continuation
suppression to operator-ending lines. See "End-of-line operator
continuations" below. The fixture has been flipped to
`EXPECT: COMPILE`.

### End-of-line operator continuations

The rule in "What this forbids" answers the case where an operator
*starts* a continuation line. The mirror case — an operator *ends*
the previous line, and the next line is the continuation:

    val total := a +
        b

— is handled by a separate mechanism in the lexer, added in commit
`123de0b` after the initial ADR 0057 implementation.

The lexer already suppresses `Indent`/`Dedent` emission for two
kinds of continuation lines: bracket-depth continuations (open `[`
or `(` across lines) and chain continuations (a line whose first
non-whitespace character is `.`). This commit adds a third:

- **Operator continuations.** If the last token of the previous
  non-blank line is a token that cannot legally end a statement,
  the current line is a continuation. Suppress its `Indent` and
  `Dedent`.

The token set is `Lexer::token_requires_continuation`. It
deliberately excludes tokens with a dual role:

  - `Lt` and `Gt` are generic type brackets as well as comparison
    operators. A function signature `-> Option<T>` ends in `Gt`
    and would otherwise suppress the `Indent` for its own body.
  - `Ampersand` and `Star` can start an expression (`&x`, `*x`).
  - `And` and `Or` are logical operators, but `And` also separates
    `parallel` blocks. After `parallel do ... and`, the next line's
    `Indent` must be emitted for the second block.

Included tokens: `Plus`, `Minus`, `Slash`, `Equal`, `NotEqual`,
`LessEqual`, `GreaterEqual`, `In`, `Comma`.

This is orthogonal to the *statement-start* rule. That rule handles
"operator at the same column as the statement's first token"; this
one handles "operator at the end of the previous line, followed by
an indented continuation." Both can coexist:

    val a := b + c        # statement
    val d := e +          # statement, ends with operator
        f                 # continuation of the same statement
    val g := h
        + i               # new statement (statement-start rule)

The two rules disagree only when the previous line ends with an
operator *and* the continuation line is at the same column as the
statement start. That case does not arise in practice: an
end-of-line operator always implies the continuation is indented
or bracketed. If it ever does, the statement-start rule wins,
because the parser's boundary check fires before the lexer's
continuation suppression can matter.

### What this enables

Every statement form whose first token is also an expression
continuation becomes unambiguous:

    *r := *r + 1
    { ... }               # block, not record-literal continuation
    [1, 2, 3]             # list literal, not array-access continuation
    (a + b)               # parenthesized expr, not call continuation
    -x                    # unary negation, if a bare-expression statement

## Alternatives considered

### A. Emit a `Newline` token at every same-indentation statement boundary

The lexer tracks paren/bracket/brace depth; when depth is zero and
a line ends at non-whitespace with the next non-blank line at the
same or shallower indentation, emit `Token::Newline`. The parser
consumes `Newline` as a hard statement separator.

**Rejected as a first step.** Correct in principle, and the shape
Rust uses internally (via the lexer's `TokenKind::Newline`), but
the implementation requires the lexer to track delimiter depth —
a state it does not currently have. This is a larger change than
the problem warrants, and the parser-side rule below achieves the
same result without a new token.

### B. Continuation lines must be indented deeper (Python's rule)

A line at the same indentation as the previous is a new statement;
a line indented deeper is a continuation of the previous
expression.

**Rejected.** It reinterprets existing indentation semantics. Under
this rule, `val total := a` followed by an indented line means one
thing in expression position (a continuation) and another in
statement position (a block body). The parser would have to know
which context it is in to interpret the `Indent`, and the same
source text would mean different things depending on its
surroundings. That is more ambiguity than the language has today,
not less. Also, `if cond\n    body` would parse as a continuation
of the condition expression, which is not what anyone writes.

### C. Semicolons

Add `;` as a statement terminator.

**Rejected.** ADR 0001 committed to significant indentation.
Semicolons contradict that choice and would require migrating every
`.gol` file.

### D. Disambiguate by token case

Give each ambiguous token a distinct spelling when it means the
statement form. For example: `*` for multiplication, a keyword
`deref` for deref-assign; `{` for block, a keyword `record` for
record literal.

**Rejected.** This trades a parser complexity for a syntax
complexity. The language's surface remains cleaner with the
positional rule. Also, the space of ambiguous tokens is not
bounded — a future ADR could add a statement form starting with
any operator — so a token-by-token fix would require a new
spelling every time.

### E. Restrict the language so no statement form starts with an
ambiguous token

Rewrite ADR 0056's deref-assign as `deref r := v` or similar so the
first token is a keyword.

**Rejected.** Same problem as D, and it retroactively invalidates
ADR 0056's syntax choice.

## Consequences

### Positive

- `*r := v` parses correctly in both block styles. ADR 0056's
  interpreter same-function test becomes writable.
- Every future statement form that starts with an operator-like
  token is unblocked by the same rule, with no further parser
  work.
- The rule is a single positional check, not per-token branching.

### Negative

- `val total := a\n    + b` (operator-leading continuation lines)
  becomes a syntax error. Authors must move the operator to the
  end of the previous line or wrap in parentheses. The error
  message should name the fix.
- Any existing `.gol` file that uses the newly-forbidden style
  needs rewriting. Unknown how many such files exist in
  `examples/`, `tests/conformance/`, and `benchmarks/`. The
  conformance suite's fixed tests are the visible subset; the
  ADR's implementation step includes a survey.
- The parser needs to track the current statement's start column
  across recursive descent, saving and restoring it at delimiter
  boundaries. This is new parser state.

### Neutral

- No lexer change. No new tokens. The two block styles (ADR 0054)
  are both covered by the same rule.
- The rule is enforced only at statement boundaries; expression
  sub-parsing is unaffected except at the specific points where an
  operator at the statement's start column would have been
  consumed as a continuation.

## Implementation sketch

1. **Add `statement_start_column: Option<usize>` to `Parser`.**
   `None` means "not currently at a statement boundary" — the rule
   is inactive (e.g. inside `( )`, `[ ]`, `{ }`).

2. **Set it in `parse_body_statements`.** Before each call to
   `parse_stmt`, set `statement_start_column` to the column of the
   current token (the statement's first token). After the call,
   leave it set for the next iteration to overwrite.

3. **Check it in the binary-operator loops.** In
   `parse_multiplicative`, `parse_additive`, `parse_comparison`,
   `parse_logical_and`, `parse_logical_or`: before consuming an
   operator, check whether `peek_column() == statement_start_column`.
   If yes, break the loop and return the left side. The operator
   stays in the token stream for the next `parse_stmt`.

4. **Clear and restore at delimiter boundaries.** In `parse_primary`'s
   `LParen` branch, `parse_identifier_expr`'s `LParen`/`LBracket`
   branches, `parse_postfix`'s `LParen`/`LBracket` branches, and
   `parse_block_expr`: save `statement_start_column`, set it to
   `None`, run the inner parse, restore. Inside these, the rule is
   inactive and line-wrapped operators work.

5. **Targeted error at a rejected line-start operator.** When
   `parse_stmt`'s `_other` fall-through hits an operator token that
   is at the statement's start column, produce a specific message:
   "`+` at the start of a line begins a new statement; to continue
   the previous expression, move the operator to the end of the
   previous line." Otherwise the generic "Unexpected expression"
   message is adequate.

   The mirror case — the operator at the *end* of the previous
   line — is handled by the lexer, not by this rule. See
   "End-of-line operator continuations" under Decision. The
   error message above is still the right guidance: if the author
   sees it, they should either move the operator up (which now
   works) or wrap the expression in parentheses.

6. **Tests.**

   - `tests/adr_examples/0057_boundaries/pass_op_at_end_of_line.gol`
   - `tests/adr_examples/0057_boundaries/pass_wrapped_in_parens.gol`
   - `tests/adr_examples/0057_boundaries/fail_op_at_line_start.gol`
   - `tests/adr_examples/0057_boundaries/pass_deref_assign_after_stmt.gol`
   - `tests/adr_examples/0057_boundaries/pass_record_literal_same_line.gol`
   - `tests/adr_examples/0057_boundaries/fail_record_literal_on_next_line.gol`

## See also

- ADR 0001 (significant indentation)
- ADR 0054 (alternate brace block syntax)
- ADR 0056 (reference writes) — the feature that exposed this gap
- `src/frontend/lexer/operator.rs` — token definitions
- `src/frontend/parser/stmt.rs` — `parse_stmt`, `parse_body_statements`
- `src/frontend/parser/expr.rs` — the binary-operator loops