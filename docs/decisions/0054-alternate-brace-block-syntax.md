# ADR 0054: Alternate brace block syntax

Status: Proposed. Implementation staged.

## Context

ALGOL26 currently has one block syntax: significant indentation. A
`proc`, `if`, `for`, `while`, `match`, `impl`, `rec`, `enum`, or
`trait` body is opened by a keyword and closed by a dedent. There
is no explicit block delimiter.

This is a deliberate design choice and it works well. It is also a
barrier for two groups of users:

1. **Programmers arriving from brace languages** (Rust, Go, Swift,
   C, Java, JavaScript, C#). Their first instinct on seeing
   `if x > 0 then` followed by an indented body is to look for the
   closing token. There isn't one. They have to retrain.

2. **Tooling and workflow constraints.** Indentation-significant
   code is fragile in copy-paste, in diff, in mixed tab/space
   environments, and in editors that reflow whitespace. Users who
   care about these things have no alternative.

Other languages have solved this by offering two block notations
that produce the same AST. Scala 3 calls this "Optional Braces"
and has shipped it since 2021. F# has had "verbose" and
"lightweight" syntaxes since its early days. OCaml treats
`begin`/`end` as equivalent to parentheses. The concept is
well-established and production-proven.

## Decision

Add a brace form as an **alternate** notation for block structure.
Indentation remains canonical; braces are supported.

**Both forms parse to the same AST.** The analyzer, IR builder,
verifier, and all three backends are unchanged. The feature is
entirely a lexer and parser concern.

### Grammar rules

- Block openers are unchanged: `proc`, `fn`, `if`, `for`,
  `while`, `match`, `case`, `impl`, `rec`, `enum`, `trait`,
  `region`, `unsafe`, `defer`.
- A block is closed by a dedent (indentation form) or by `}`
  (brace form).
- **`then` becomes optional.** In indentation form, `if cond
  then` — `then` marks where the condition ends. In brace form,
  `if cond {` — the `{` does that job. `then` is required in the
  indentation form and absent in the brace form.
- **Semicolons remain optional** in both forms. Newlines terminate
  statements unless the parser is mid-expression. A `;` may be
  used to put two statements on one line.
- **A form is chosen per block.** A `proc` body can be
  brace-delimited while a nested `if` is indentation-delimited.
  In practice, one form per file is the recommended convention,
  enforced by style guide rather than by the parser.

### Ambiguity resolution

`{` already appears in ALGOL26 as the delimiter of record
literals (`Point { x: 3, y: 4 }`). Introducing `{` as a block
delimiter creates a potential ambiguity, resolved by the token
preceding `{`:

- Preceded by a **type name** (an identifier that is not a
  keyword) → record literal.
- Preceded by a **keyword** (`if`, `for`, `proc`, `fn`, etc.)
  or appearing at statement position → block.

The rule is unambiguous and requires one token of lookahead.

## Consequences

**Positive.**

- A Rust or C user reads ALGOL26 immediately; a Python user reads
  it immediately. Neither has to retrain their eye.
- Brace-form code survives copy-paste, diff, and whitespace
  normalization without change.
- The existing indentation corpus is untouched. Nothing in
  `examples/`, `tests/`, or `tests/fixtures/` needs to change.
- All downstream compiler stages are unaffected.

**Negative.**

- Two syntaxes to test. Every parser test case has a "canonical"
  form and, ideally, a "same AST" assertion against the other
  form.
- Style guides become harder. Two valid ways to write a loop.
- The formatter must normalize one form or the other, or leave
  the choice to the author. Initial decision: leave it alone,
  let the author pick.
- `then` becomes optional, which is a small grammar change to a
  currently-unconditional token.

## Alternatives considered

### A. Keep indentation only

**Status quo.** Rejected as a growth path: it excludes users from
brace-language backgrounds and their tooling, for no benefit
beyond internal consistency.

### B. Switch to braces only

**Rejected.** Would require rewriting every `.gol` file in the
repo. Would discard the indentation-based identity that the
language was designed around. The cost/benefit ratio is wrong.

### C. Switch to Julia-style `begin ... end`

**Rejected.** Introduces a `begin` keyword and an `end` keyword
that duplicate block structure the indentation already carries.
More tokens to type, no new capability over braces.

### D. Use Scala 3's exact design

Considered and adopted in spirit. ALGOL26's version is simpler
because ALGOL26's block grammar is simpler (no `given`/`using`,
no significant-newline rules inside parenthesized expressions,
fewer contextual keywords).

## Implementation plan

Four sessions, each a self-contained commit.

**Session 1 — parser accepts brace blocks.** The lexer already
emits `LBrace`/`RBrace` for record literals. The parser is
extended so every block-accepting site (`parse_block`, the body
of `if`, `for`, `while`, `match`, `impl`, `rec`, `enum`, `trait`,
`region`, `unsafe`, `defer`, `proc`, `fn`) accepts a `{`-opened
body as an alternative. `then` becomes optional after `if`
conditions. New tests verify that brace form and indentation
form parse to identical ASTs for a small set of programs.

**Session 2 — formatter.** `algol26 fmt` normalizes blocks to
whichever form the file uses for its first block. Idempotent.

**Session 3 — user-facing documentation.** `docs/guide/blocks.md`
describing both forms, when to use which, and the "one form per
file" convention. README updated.

**Session 4 — corpus coverage.** Add `examples/braces/*.gol` —
the same programs as `examples/*.gol`, written in brace form,
with a test asserting identical compilation.

## See also

- ADR 0018 — canonical pipeline (unchanged by this ADR)
- `docs/decisions/0045-region-lifetime-unification.md` — the
  region block, which this ADR now allows in both forms
- Scala 3 "Optional Braces": https://docs.scala-lang.org/scala3/reference/other-new-features/indentation.html
- F# "Verbose Syntax": https://learn.microsoft.com/en-us/dotnet/fsharp/language-reference/verbose-syntax
