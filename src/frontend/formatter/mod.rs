// src/frontend/formatter/mod.rs
//
// ADR 0058. The `algol26 fmt` formatter.
//
// The formatter reads a source file, parses it to AST, decides
// the file's block form (braces or indentation) from the first
// block-bearing construct, and emits canonical source. Its output
// is idempotent: `fmt(fmt(x)) == fmt(x)`.
//
// Session 2a: the emitter. Comments are not yet interleaved
// (session 2b). Until then, the formatter drops comments — not
// acceptable for shipping, but sufficient to test the emitter's
// structural correctness.

mod emit;

pub use emit::{BlockForm, Formatter};
