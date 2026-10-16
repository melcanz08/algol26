# ADR 0058: `algol26 fmt`

Status: Accepted. Sessions 1, 2a, 2b, 3, 4a, 4b, 5a, and 5b landed.
The formatter handles 300 of 309 parseable files in the corpus.
Two rules document the remaining limitations: block-shaped
expressions in single-line containers (multi-line container
emission pending), and body-less proc/fn declarations (parser
fix pending).

## Context

ADR 0054 introduced alternate brace syntax for block structure.
Its "Deferred: `algol26 fmt`" section commits to a formatter that
normalizes block form ("one form per file, matching the file's
first block") but explicitly defers the design:

> A formatter that normalizes block form ... is desirable but out
> of scope for this ADR. No formatter exists in the repo today;
> writing one is a project on the scale of ADR 0053, not a
> session.

No formatter exists. Users write in whichever form reads better
and have no way to normalize a file's style after the fact. The
`docs/guide/blocks.md` convention — "one form per file" — is a
guideline, not enforced by any tool.

The project's other sources of truth about formatting are:

- **ADR 0001 (significant indentation)** — 4 spaces per level,
  tabs converted to spaces, indentation is structural.
- **ADR 0054 (brace syntax)** — `then` is required after `if`
  conditions in indentation form and omitted in brace form;
  brace-form `match` arms require `then` between pattern and
  body; semicolons remain optional.

Both are decisions about the *language*, not about formatting.
Neither pins trailing commas, spacing around operators, or the
treatment of comments. This ADR does.

## Decision

Ship `algol26 fmt`, a source-preserving formatter for ALGOL26
source. Its guarantees:

1. **Parses first.** A file that does not parse is not formatted;
   the formatter exits with the parse error.
2. **Preserves comments.** No comment is dropped, moved across a
   statement boundary, or reflowed.
3. **Picks block form once per file.** From the file's first
   block-bearing construct. Braces and indentation are never
   mixed in the output.
4. **Is idempotent.** `fmt(fmt(x)) == fmt(x)` for every input
   that parses.
5. **Does not wrap lines.** A statement on one line stays on one
   line; a statement on multiple lines is joined if it fits the
   canonical form. This is the v1 policy; wrapping is a future
   ADR.
6. **Does not reorder anything.** Imports, declarations, record
   fields, and match arms keep their source order.

The formatter's output is the canonical form of ALGOL26 source.
`docs/guide/blocks.md`'s conventions become enforceable by
`algol26 fmt --check`.

## CLI

    algol26 fmt <file>            write formatted source back to <file>
    algol26 fmt --check <file>    exit 0 if formatted, exit 1 if it would change
    algol26 fmt --stdout <file>   write to stdout, do not modify <file>
    algol26 fmt --check <dir>     recursive; exit 1 if any file would change

`--check` prints the filenames that would change to stderr, then
exits with status 1. It never modifies files.

Flags `--check` and `--stdout` are mutually exclusive.

`<file>` may be a directory with `--check`; without `--check`,
directories are rejected.

## Formatting rules

The canonical form is defined below. Where a rule is silent,
source formatting is preserved.

### Indentation

- 4 spaces per level. Tabs never appear in output.
- Brace form indents the block body one level from the opener.
  The `{` sits at the end of the header line; the `}` sits at
  the header's indentation.
- Indentation form indents the block body one level from the
  header; the block ends at the first line at or below the
  header's indentation.

### Whitespace

- One space around binary operators: `a + b`, `x == y`.
- No space around `.`, `::`, or `:` in field access and type
  annotations: `p.x`, `T::Item`, `x: Int`.
- One space after `,` and `;`.
- No space inside `( )`, `[ ]`, or `{ }` when on a single line:
  `f(a, b)`, `[1, 2, 3]`, `Point { x: 1, y: 2 }`.
- Space before `{` when it opens a block: `if x > 0 {`.
- No space before `(` in a function call or declaration:
  `f(x)`, `proc g(y: Int)`.
- One space around `:=` and `->`.

### Blank lines

- At most two consecutive blank lines are preserved.
- Exactly one blank line between top-level declarations
  (`proc`, `fn`, `rec`, `enum`, `trait`, `impl`, `import`,
  `type`).
- No blank line at the start of a block or before `}` / dedent.
- One trailing newline at end of file.

### `then` and semicolons

- Indentation form: `if cond then` — `then` present; `else`
  begins a new line at the header's indentation.
- Brace form: `if cond {` — `then` absent; `} else {` on one
  line.
- Brace-form `match` arms: `case Pattern then {`. Indentation
  form arms: `case Pattern` on its own line, body indented.
- Semicolons are removed when a newline already separates two
  statements. They are preserved when two statements share a
  line.

### Comments

- Line comments (`//`) are preserved at their source column.
- Comments at end of line stay at end of line, separated from
  the preceding token by at least one space.
- Comments on their own line are preserved on their own line.
- Blank lines between comments and code are preserved per the
  blank-line rules.
- Comment text is not reflowed or re-indented.

### Trailing commas

- Preserved. A trailing comma after the last element of a list,
  record, or argument list is retained on output.

## Block form policy

The formatter reads the file's **first block-bearing construct**
— the first `proc`, `fn`, `if`, `for`, `while`, `match`, `impl`,
`rec`, `enum`, `trait`, `region`, `unsafe`, or `defer` — and
inspects the token immediately after the header. If it is `{`,
the file's form is braces. Otherwise indentation.

The determination is made once, from the source, before
formatting. The AST does not record which form was used
(ADR 0054: "Both forms parse to the same AST"), so the formatter
must consult the source directly. This is a single token of
lookahead at the top of the file.

If a file has no block-bearing construct (e.g. only top-level
imports and constants), the formatter defaults to indentation.

The formatter's output uses the chosen form uniformly. A file
written with mixed forms — a `proc` in braces containing an `if`
in indentation — is normalized to a single form. This is the
"one form per file" convention from `docs/guide/blocks.md`,
enforced.

## Idempotence

The formatter must satisfy `fmt(fmt(x)) == fmt(x)` for every
input that parses. This is the definitional property of a
formatter; a formatter that changes its own output is broken.

The test suite asserts idempotence on:

- Every `.gol` file under `examples/`.
- Every `.gol` file under `tests/` (including fixtures and the
  paired-form brace examples).
- Every `.gol` file under `benchmarks/`.
- A set of targeted inputs exercising each rule above.
- A set of malformed inputs asserting that the formatter exits
  with a parse error and does not modify the source.

The idempotence check runs in two passes:

    let once = fmt(source);
    let twice = fmt(&once);
    assert_eq!(once, twice);

`--check` on the whole corpus runs as part of `cargo test`,
ensuring that the examples in the repo are already formatted.

## Implementation approach

The formatter is a **source-preserving** pass, not an
AST-pretty-printer. It:

1. Lexes the source, retaining comment tokens and their
   positions.
2. Parses the source to AST. Fails on parse error.
3. Reads the block form from the first block-bearing construct.
4. Walks the AST, emitting canonical text.
5. Interleaves comments by their source position — a comment
   whose source span is between two AST nodes is emitted between
   them.

This is the gofmt model: the AST provides structure, the source
provides trivia. The alternative — an AST-pretty-printer that
loses comments — is rejected (see Alternatives).

The formatter lives in a new module: `src/frontend/formatter/`.
Submodules for the emitter, the comment interleaver, and the
line-wrapping policy (empty in v1, present as a stub for future
extension).

## Out of scope

- **Line wrapping.** Long expressions and argument lists stay on
  one line. A future ADR adds wrapping with a configurable
  maximum.
- **Import sorting.** Imports keep their source order.
- **Comment reflow.** Comment text is emitted verbatim.
- **Per-block form.** A future ADR may allow mixed forms with
  a per-block marker; v1 enforces one form per file.
- **Migration between forms.** Converting a file from
  indentation to braces is a *mode* of the formatter, not a
  default. `algol26 fmt --braces <file>` and
  `algol26 fmt --indent <file>` would force a form; these are
  deferred.
- **Alignment.** Columns of `:` in record fields or of `:=` in
  adjacent `val` declarations are not aligned. Each line is
  formatted independently.
- **Configuration files.** No `.algol26fmt.toml`. The canonical
  form is the only form. Users who want a different style do
  not use the formatter.

## Alternatives considered

### A. AST-pretty-printer

Walk the AST and emit canonical text. Simpler than the
source-preserving approach: no comment interleaving, no source
position tracking. But it loses comments, which is unacceptable
for a formatter users run on their own code. Rejected.

### B. Whitespace-only, no block-form normalization

The formatter does not touch block form; it only normalizes
spaces and blank lines. Simpler: no need to read the first
block's form. But ADR 0054's deferral note commits to form
normalization as the formatter's primary job. Rejected.

### C. Per-block form preservation

The formatter preserves each block's original form. Requires
the AST to record which form was used at every block. This is
a significant change to the AST (every `Block`, `proc`, `if`,
`for`, etc. gains a form field), and it inverts the design
philosophy of ADR 0054 ("Both forms parse to the same AST").
Rejected. The "one form per file" convention is what ADR 0054
recommended, and this ADR enforces it.

### D. Reuse rustfmt's architecture

Rustfmt has a well-tested formatter architecture. But it is
specific to Rust's grammar (macros, attributes, `where`
clauses, lifetimes) and does not port cleanly. ALGOL26's
grammar is small enough that a purpose-built formatter is
simpler than adapting an existing one. Rejected.

### E. No formatter, document the style guide only

The current state: conventions exist in prose, enforcement is
by discipline. Rejected because every nontrivial language
benefits from an automated formatter, and the project already
committed to one in ADR 0054.

## Consequences

**Positive.**

- Users can normalize a file's style with a single command.
- The "one form per file" convention becomes enforceable by
  `--check` in CI.
- Editor integration (format-on-save) becomes possible.
- The formatter's existence nudges the codebase toward uniform
  style, which makes reading diffs easier.
- A canonical form reduces bikeshedding.

**Negative.**

- A new tool to maintain. Formatter bugs (dropping a comment,
  changing semantics, non-idempotent output) are high-visibility
  and high-cost.
- The comment-interleaving logic is subtle. Comments that appear
  between syntactically adjacent tokens (e.g. between `then` and
  the first statement of a block) require careful positioning.
- A file formatted by `fmt` and then edited by a user in an
  IDE with different formatting conventions will show a large
  diff on the next `--check`. This is the usual cost of adopting
  a formatter.
- The formatter's decisions are permanent in the sense that
  changing them later invalidates every file's formatted state.
  This ADR's choices (4-space indent, no wrapping, preserved
  trailing commas) should be considered durable.

**Neutral.**

- The formatter is a new top-level command. It does not touch
  any compiler stage. The lexer may gain a mode that retains
  comments; the parser is unchanged.

## Implementation plan

Three sessions.

**Session 1 — lexer comment retention.** The lexer currently
discards comments. Add a mode (or a wrapper) that retains them
with their source positions. The analyzer and IR builder
continue to consume the comment-free lexer output; only the
formatter uses the comment-retaining path.

**Session 2 — AST walk and emitter.** The formatter's core: walk
the AST, emit canonical text per the rules above, and interleave
comments by source position. The block-form policy is read from
the source. Idempotence tests run against a small subset of
`examples/`.

**Session 3 — CLI, corpus coverage, CI.** Wire `algol26 fmt`,
`--check`, and `--stdout` into `src/main.rs`. Extend the
idempotence test to the full corpus. Add `--check` on the whole
repo to CI.

## See also

- ADR 0001 — significant indentation
- ADR 0054 — alternate brace block syntax (defers the formatter
  to this ADR)
- `docs/guide/blocks.md` — the user-facing guide to block forms
- gofmt (Go) and rustfmt (Rust) — the reference designs for
  source-preserving formatters
