# Algol26 Generated Showcase

8 programs, each runs identically on interpreter, LLVM, and WASM.

Run them with:

```
for f in examples/showcase/*.gol; do
    echo "=== $f ==="
    target/debug/algol26 run --interpreter "$f"
done
```

## Programs

1. `defer_order.gol` — see `tests/fixtures/defer_order.gol`
2. `enum_match.gol` — see `tests/fixtures/enum_match.gol`
3. `int_to_string.gol` — see `tests/fixtures/int_to_string.gol`
4. `list_of_records.gol` — see `tests/fixtures/list_of_records.gol`
5. `match_enum_variant.gol` — see `tests/fixtures/match_enum_variant.gol`
6. `option_some.gol` — see `tests/fixtures/option_some.gol`
7. `records_basic.gol` — see `tests/fixtures/records_basic.gol`
8. `string_ops_basic.gol` — see `tests/fixtures/string_ops_basic.gol`

## Not in the showcase

These fixtures are interpreter-only. Each has a `// supported:` header explaining which backend refuses it and why.

- `generic_record.gol`
- `match_with_bindings.gol`
- `method_by_value_receiver.gol`
- `string_split.gol`
