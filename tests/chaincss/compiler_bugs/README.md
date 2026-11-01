# Compiler-bug regression tests

Each file here is a minimal repro for a compiler bug that was found
while porting ChainCSS to ALGOL26. All are expected to **pass**
after the bug is fixed.

- `probeA_var_only.gol` — record literal in `var`, then `&Rec`+`if` (G16)
- `probeB_match_only.gol` — `match source.field`, then `&Rec`+`if` (G16)
- `probeC_return_only.gol` — record literal in `return`, then `&Rec`+`if` (G16)
- `probeD_option_ret.gol` — `-> Option[SomeRecord]` return type (G16)
- `probeE_ret_rec_param_if.gol` — `-> SomeRecord` + `&Rec`+`if` (G16)

**Resolved 2026-11-01** by the fix to `src/frontend/lexer/decl.rs`
(`parse_signature` missing `[`/`]` arms). All five probes now pass.
