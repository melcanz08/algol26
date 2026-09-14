# ALGOL26 Memory Model Hierarchy

**Effective**: v0.8.0

## Three Safety Levels

```
                    MEMORY MODEL
                         │
        ┌────────────────┼────────────────┐
        ▼                ▼                ▼
      SAFE           CONTROLLED          RAW
        │                │                │
   bounds checks     regions          unsafe
   ownership         arenas           pointers
   borrowing         lifetimes        alloc/free
   types             scoped mem       FFI
```

---

## Level 1: SAFE (Default)

**What it provides**:
- Bounds checking on arrays
- Ownership tracking (move/copy semantics)
- Borrow checking (immutable/mutable)
- Type safety
- Automatic memory management

**When to use**: Always, unless you have a specific reason not to.

**Example**:
```golang
val x := 5           // Int is Copy
val s := "hello"     // String is Move
val arr := [1, 2, 3] // List with bounds checking
```

---

## Level 2: CONTROLLED (Explicit Opt-In)

**What it provides**:
- Region-based memory management
- Explicit lifetimes
- Arena allocation
- Scoped memory

**When to use**: When you need predictable memory behavior without full unsafe.

**Example**:
```golang
region temp
    val x := 5
    // x is freed when region exits
```

**Current status**: `region.rs` and `region_memory.rs` exist but are not fully integrated.

---

## Level 3: RAW (Explicit Unsafe)

**What it provides**:
- Raw pointers
- Manual alloc/free
- FFI calls
- Direct memory manipulation

**When to use**: Only when absolutely necessary (FFI, low-level operations).

**Example**:
```golang
unsafe
    val ptr := alloc(100)
    // Manual memory management
    free(ptr)
```

**Safety requirement**: Unsafe code must NEVER leak into safe semantics.

---

## Boundary Rules

1. **Safe → Controlled**: Allowed (safe code can use regions)
2. **Safe → Raw**: NOT allowed (must go through `unsafe` block)
3. **Controlled → Safe**: Allowed (regions are compatible with safe code)
4. **Controlled → Raw**: NOT allowed (must go through `unsafe` block)
5. **Raw → Safe**: NOT allowed (must validate manually)
6. **Raw → Controlled**: NOT allowed (must validate manually)

---

## Compiler Enforcement

| Boundary | Enforced By | Status |
|----------|-------------|--------|
| Safe default | All code outside unsafe blocks | ✅ |
| Unsafe explicit | `unsafe` keyword required | ✅ |
| Region explicit | `region` keyword required | ✅ Parsed |
| Raw pointers explicit | `Ptr` type | ✅ |
| FFI explicit | `extern` keyword | ✅ |

## Current Gaps

- Region memory not fully integrated into code generation
- No runtime enforcement of region boundaries
- Unsafe blocks don't verify that unsafe operations stay within them