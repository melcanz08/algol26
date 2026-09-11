# ALGOL26 Documentation

This directory is the entry point for documentation about the
ALGOL26 compiler and language.

## What to read first

- **[language/reference.md](language/reference.md)** — the current
  language reference. Every claim in this file is verified against
  the compiler; if the file says something works, it compiles.
- **[architecture/overview.md](architecture/overview.md)** — the
  compiler pipeline and its major components.
- **[decisions/](decisions/)** — Architecture Decision Records.
  These are historical; the language model is defined by the
  decisions listed here.

## Directory structure

| Directory | Contents | Stability |
|---|---|---|
| `language/` | Current language reference and versioning policy | Current |
| `architecture/` | Compiler architecture and design principles | Current |
| `compiler/` | Pipeline contracts, test organization, policies | Current |
| `decisions/` | Architecture Decision Records (ADRs) | Historical, immutable |
| `releases/` | Release notes | Historical, immutable |
| `memory/` | Memory model | Current |
| `archive/` | Superseded status, roadmap, and backlog documents | Historical |

## What is not in this directory

- **Working examples**: see `examples/` in the project root.
- **Adversarial test cases**: see `tests/adversarial/`.
- **Historical source comments**: superseded doc comments are
  updated in place, not archived. The archive is for whole
  documents.

## Trust policy

A file under `docs/` should be **either**:

- A **current description** of what the code does today. It must
  match the source. If it doesn't, the file is buggy.
- A **historical record** under `decisions/`, `releases/`, or
  `archive/`. It is frozen at the moment it was written.

Do not write documents that describe both "what is true today" and
"what we plan to be true." Split them: current description in the
appropriate current directory, plan in `language/roadmap.md` or a
similarly-named forward-looking file.