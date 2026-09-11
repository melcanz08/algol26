# ALGOL26 adversarial safety tests

These are deliberately hostile compiler inputs for the safety audit.

The important distinction is:

- `PASS-CORRECT`: rejected for the intended safety reason
- `FAIL-ACCEPTED`: compiler accepted a program that should be rejected
- `FAIL-WRONG`: compiler rejected it, but for an unrelated reason
- `REVIEW-*`: behavior requires backend/manual inspection

Some tests are positive controls (`EXPECT: ACCEPT`) and one is a defer semantic probe.
The exact syntax may change as ALGOL26 evolves; preserve failing probes because they are useful
for regression testing.

The key goal is not merely "reject bad code". It is "reject it for the correct invariant."
