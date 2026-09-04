# ALGOL26 IR Pass Contracts

**Effective**: v0.8.0

## What Is a Pass Contract?

Every IR transformation pass has a contract defining:
- **Input invariant**: What must be true before the pass runs
- **Transformation**: What the pass does
- **Output invariant**: What is guaranteed after the pass
- **Preserved semantics**: What must NOT change

---

## Pass 1: Loop Desugaring

**File**: `src/ir/loop_desugar.rs`

| Property | Description |
|----------|-------------|
| Input | AST containing `for`/`while` loops |
| Output | AST without loops (unrolled or converted) |
| Preserves | Program semantics (same observable behavior) |
| May change | Control flow structure (loops become blocks) |
| Must NOT | Change variable types or ownership |

---

## Pass 2: Defer Lowering

**File**: `src/ir/defer_lowering.rs`

| Property | Description |
|----------|-------------|
| Input | IR containing `Defer` instructions |
| Output | IR without `Defer` instructions |
| Preserves | Every defer executes before scope exit |
| Preserves | Return semantics (early returns still run defers) |
| Preserves | Error semantics |
| May change | Control flow structure |

---

## Pass 3: Monomorphization

**File**: `src/ir/monomorphize.rs`

| Property | Description |
|----------|-------------|
| Input | Generic AST with type parameters |
| Output | Concrete AST without type parameters |
| Preserves | Program semantics for each instantiation |
| Guarantees | No unresolved generic calls remain |
| Guarantees | All type parameters substituted |
| May change | Function names (specialized names) |

---

## Pass 4: Constant Folding

**File**: `src/ir/optimizer.rs`

| Property | Description |
|----------|-------------|
| Input | Valid semantic IR |
| Output | Valid semantic IR with constants folded |
| Preserves | Program semantics (same output) |
| Preserves | Types (no type changes) |
| Preserves | Ownership (no ownership changes) |
| May change | Expression structure (constant replaces expression) |

---

## Pass 5: Dead Code Elimination

**File**: `src/ir/optimizer.rs`

| Property | Description |
|----------|-------------|
| Input | Valid semantic IR |
| Output | Valid semantic IR without unreachable code |
| Preserves | Program semantics |
| Preserves | All observable behavior |
| May change | Number of blocks/instructions |
| Must NOT | Remove code with side effects |

---

## Pass 6: IR Verification

**File**: `src/ir/semantic_ir.rs` (verify method)

| Property | Description |
|----------|-------------|
| Input | Any SemanticProgram |
| Output | Result<(), String> |
| Checks | Block IDs are unique |
| Checks | Jump targets exist |
| Checks | Entry block exists |
| Guarantees | If Ok, IR is structurally valid |
| Does NOT | Check semantic correctness (already done) |

---

## Enforcement

These contracts are currently documented but NOT enforced in code.
Future work: Add runtime verification after each pass to enforce these contracts.