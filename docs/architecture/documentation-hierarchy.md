# ALGOL26 Documentation Hierarchy

**Effective**: v0.8.0

## Document Roles (Authority Order)

When documents disagree, the **higher-ranked document wins**.

| Priority | Document | Role |
|----------|----------|------|
| 1 | `formal-specification.md` | **Mathematical/semantic authority** — Defines what programs MEAN |
| 2 | `language-specification.md` | **Normative language rules** — Complete syntax and semantics |
| 3 | `algol26-contract.md` | **Compiler/project invariants** — Architecture guarantees |
| 4 | `architecture.md` | **Implementation architecture** — How the compiler is built |
| 5 | `architecture-inventory.md` | **Code inventory** — File-by-file audit |
| 6 | `architecture-maturity.md` | **Maturity scores** — Honest assessment |
| 7 | `ir-pass-contracts.md` | **Pass contracts** — Input/output invariants for each pass |
| 8 | `memory-model.md` | **Memory hierarchy** — SAFE/CONTROLLED/RAW levels |
| 9 | `language-reference.md` | **User-facing reference** — How to write ALGOL26 |
| 10 | `language-freeze.md` | **Freeze declaration** — What cannot change |
| 11 | `versioning.md` | **Version policy** — How versions are managed |
| 12 | `test-organization.md` | **Test architecture** — Test suite to layer mapping |
| 13 | `no-panic-policy.md` | **No-panic rule** — What must not panic |
| 14 | `current-status.md` | **Current state** — Latest progress snapshot |
| 15 | `vision.md` | **Long-term philosophy** — Where ALGOL26 is going |

---

## Resolution Rules

| Question Type | Authoritative Document |
|---------------|----------------------|
| Semantic question (what does this MEAN?) | `formal-specification.md` |
| Syntax question (is this VALID?) | `language-specification.md` |
| Architecture question (where does this BELONG?) | `algol26-contract.md` |
| Implementation question (HOW is this built?) | `architecture.md` |
| Usage question (how do I WRITE this?) | `language-reference.md` |
| Pass contract question (what does this pass DO?) | `ir-pass-contracts.md` |
| Memory question (is this SAFE?) | `memory-model.md` |

---

## Update Rules

- **Language changes** → Update `formal-specification.md` and `language-specification.md` FIRST
- **Compiler changes** → Update `architecture.md` and `architecture-inventory.md`
- **Pass changes** → Update `ir-pass-contracts.md`
- **Memory model changes** → Update `memory-model.md`
- **Test changes** → Update `test-organization.md` and `current-status.md`
- **Version changes** → Update `versioning.md`

---

## Conflict Example

If `language-reference.md` says "`+` works on strings" but `formal-specification.md` doesn't mention string concatenation:

- **`formal-specification.md` wins**
- Either fix the reference doc or update the formal spec
- Never silently accept the contradiction

---

## Current Documentation Inventory

| Document | Exists? | Status |
|----------|---------|--------|
| `formal-specification.md` | ✅ | Authority |
| `language-specification.md` | ✅ | Normative |
| `algol26-contract.md` | ✅ | Invariants |
| `architecture.md` | ✅ | Implementation |
| `architecture-inventory.md` | ✅ | Audit |
| `architecture-maturity.md` | ✅ | Scores |
| `ir-pass-contracts.md` | ✅ | Pass contracts |
| `memory-model.md` | ✅ | Memory hierarchy |
| `language-reference.md` | ✅ | User reference |
| `language-freeze.md` | ✅ | Freeze declaration |
| `versioning.md` | ✅ | Version policy |
| `test-organization.md` | ✅ | Test architecture |
| `no-panic-policy.md` | ✅ | No-panic rule |
| `current-status.md` | ✅ | Status snapshot |
| `vision.md` | ✅ | Philosophy |