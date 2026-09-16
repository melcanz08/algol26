// tests/corpus_diff.rs
//
// Per-program corpus tests, generated at build time from
// tests/corpus/*.gol. The harness lives in `corpus_support`; the
// test functions themselves are in $OUT_DIR/corpus_generated.rs.

mod corpus_support;

include!(concat!(env!("OUT_DIR"), "/corpus_generated.rs"));
