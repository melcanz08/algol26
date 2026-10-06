# ADR 0037 — WASM Records and References

## Status

Revised. Superseded in spirit by ADR 0036.

> **Revision note (2026-10-06, post-investigation).** The original
> draft of this ADR assumed the WASM backend was a separate codegen
> with its own value model and memory allocation decisions. Code
> inspection showed otherwise: `src/backends/wasm_backend.rs` emits
> LLVM IR and invokes `wasm-ld` on the resulting object files. WASM
> is LLVM compiled to wasm32, not a distinct backend.
>
> The consequence: records and references land on WASM the moment
> they land on LLVM. There is no separate layout, no separate memory
> model, no separate allocation question. The original seven open
> questions (Q1–Q7) collapse to a single testing question: does
> `wasm-ld` accept the LLVM IR that record and reference codegen
> produces?
>
> The original document is preserved below for the audit trail.
> Its design sections are superseded; its framing is wrong.

## What actually remains for WASM

**Records.** Follow ADR 0036. Once LLVM record codegen works
end-to-end, WASM inherits it. The only WASM-specific change is
adding `Feature::Records` to the WASM capability matrix in
`src/backends/capabilities/mod.rs` and verifying the toolchain
accepts the module.

**References.** Refused by the WASM capability matrix today. The
refusal predates the discovery that WASM reuses LLVM codegen — LLVM
has complete reference lowering, and `wasm-ld` accepts it. Flipping
`Feature::References` for WASM is likely a one-line change plus
verification. Investigate by building a reference-using program
targeting WASM and observing whether the toolchain accepts it. If
it does, add the capability and a differential test. If not, the
specific rejection is the design problem.

**Both changes are follow-ups to ADR 0036, not a separate ADR.**
This file is retained as a record of the investigation and the
original framing, not as an independent design project.

## Original draft (superseded)

The sections below were the original ADR 0037. They describe a
WASM-specific design problem that turned out not to exist. Preserved
for the audit trail.
