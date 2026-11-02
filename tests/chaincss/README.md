# tests/chaincss/

ChainCSS-to-ALGOL26 port tests: parity harnesses for the
differential corpus, plus regression tests for compiler bugs
discovered during the port.

## Import convention

Every `.gol` test file in this directory uses **relative imports**
to `src/chaincss/`:

    import "../../src/chaincss/json.gol"
    import "../../src/chaincss/ir_types.gol"

Bare imports (`import "json.gol"`) only work when the runner `cd`s
into `src/chaincss/` before invoking `algol26 check` / `run`. That
is a wrapper artifact, not a test design.

**Rule:** if you add a test file here, use relative imports.
`algol26 check <file>` must pass standalone, without the wrapper.

`parse_ir_parity.gol` and `config_parity.gol` originally used bare
imports and only worked because their drivers (`run_parity.sh`,
`run_config_parity.sh`) `cd` into `src/chaincss/` first. They were
fixed in the commit that added this README, so
`./target/release/algol26 check tests/chaincss/<file>.gol` now
works for every test file here.

## Harness relationships

| Test file | Driver | Exercises |
|---|---|---|
| `parse_ir_parity.gol` | `chaincss/diff-corpus/run_parity.sh` | StyleDefinition (Json) → StyleIR, against the TS `parseIR` oracle |
| `config_parity.gol` | `chaincss/diff-corpus/run_config_parity.sh` | config loading + deep-merge, against the TS `getDefaultConfig` / `deepMergeConfig` oracle |
| `ast_roundtrip_smoke.gol` | `chaincss/diff-corpus/run_parity.sh` | StyleDefinition ↔ `style_definition_to_json` ↔ `parse_ir` round-trip — proves `ast.gol` is a faithful envelope for the Json the shim produces |
| `diagnostics_format_parity.gol` | `chaincss/diff-corpus/run_diagnostics_parity.sh` | `formatReport` byte-for-byte, across four scenarios |
| `compiler_bugs/*.gol` | none — run directly | regression tests, one per G## entry in `LANGUAGE_GAPS.md` |

## Running the tests

**Standalone check** (any test file):

    cd ~/dev/algol26
    ./target/release/algol26 check tests/chaincss/<file>.gol

**Parity harness** (requires the matching driver in the
`chaincss` repo and a populated `/tmp/chaincss-parity/`):

    cd ~/dev/web/chaincss
    bash diff-corpus/run_parity.sh
    bash diff-corpus/run_config_parity.sh
    bash diff-corpus/run_diagnostics_parity.sh

**Regression test** (any `compiler_bugs/*.gol` file — no corpus
needed):

    cd ~/dev/algol26
    ./target/release/algol26 run tests/chaincss/compiler_bugs/<file>.gol --interpreter

## Rules

1. **New port file** → new parity harness. The port's exit
   criterion is byte-parity with the TS original on the corpus
   the roadmap names for that phase.

2. **New compiler bug** → new `compiler_bugs/g##_*.gol` test that
   fails before the fix and passes after. Every `G##` entry in
   `LANGUAGE_GAPS.md` should have a matching test file.

3. **Never rewrite a corpus golden file.** If a `.json` under
   `chaincss/diff-corpus/expected/` disagrees with the port, the
   port is wrong (or the TS oracle is wrong, which is a separate
   investigation). Goldens are frozen.

4. **Import convention:** relative paths, always. See top.

5. **Staged inputs live in `/tmp/chaincss-parity/`.** Tests read
   from there; drivers write there. Do not commit
   `/tmp/chaincss-parity/`. It's a working directory, not test
   state.
