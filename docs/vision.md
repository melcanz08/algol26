# ALGOL26 Vision

## Core Identity
ALGOL26 = ALGOL 58 + significant indentation + modern compiler technology + lessons from 68 years of programming language design

## The North Star
ALGOL 58 reimagined for 2026.

## Design Philosophy
ALGOL26 begins with the ideas of ALGOL 58: clear algorithms, structured programming, mathematical expressions, procedures, and a language designed to express computation directly.

It replaces ALGOL's explicit block delimiters with significant indentation, drawing inspiration from Python, ISWIM, and Haskell.

It then asks: What would ALGOL look like if we applied everything the programming-language community has learned since 1958?

## The Safety Philosophy

### Control Without Unsafe Defaults

ALGOL26 aims to provide:

```
C/C++ POWER              Rust SAFETY
    │                        │
    │                        │
direct memory          compile-time
deterministic          guarantees
native performance     ownership
low-level control      borrowing
predictable layout     exhaustive checks
    │                        │
    └───────────┬────────────┘
                ▼
         ALGOL26 MODEL
                │
   "CONTROL WITHOUT UNSAFE DEFAULTS"
```

### Three Levels of Memory Abstraction

```
             ALGOL26 MEMORY
                   │
      ┌────────────┼────────────┐
      ▼            ▼            ▼
   SAFE         CONTROLLED     RAW
  MEMORY         MEMORY       MEMORY
      │            │            │
   default       explicit      unsafe
      │            │            │
   simple       systems       hardware
   programs     programming   programming
```

### Memory Philosophy

ALGOL26 should not say:
- "Don't worry about memory" (too abstract)

Instead, it should say:
- "You can control memory, but the compiler won't let you accidentally violate its rules."

### Safety Goals

1. **No undefined behavior** - All operations are defined
2. **No null by default** - Option type for optional values
3. **Bounds checking** - Array access is always checked
4. **Deterministic cleanup** - No garbage collection pauses
5. **Compile-time safety** - Errors caught before runtime
6. **Zero-cost abstractions** - Safety without performance loss

## Three Layers

### 1. ALGOL Heritage (Preserved)
- Procedures as primary abstraction
- Structured programming constructs
- Mathematical expression syntax
- Block-oriented thinking
- Algorithmic focus
- Clear control flow
- Scientific computing orientation

### 2. ALGOL26 Innovations (Deliberate Improvements)
- Significant indentation (replacing begin/end)
- Static type checking with inference
- Memory safety without garbage collection overhead
- First-class error handling
- Modern module system
- No null by default
- Algebraic data types
- Pattern matching
- Deterministic ownership

### 3. Modern Implementation (How We Build It)
- Rust for compiler implementation
- LLVM for code generation
- Native machine code output
- Fast compilation
- Excellent diagnostics
- IDE support (LSP)

## Core Principles

1. **Every modern feature must solve an identified problem**
   - No feature added 'because other languages have it'
   - Each feature documented with problem/solution rationale

2. **No feature compromises ALGOL's conceptual clarity**
   - Language must remain readable and understandable
   - Avoid unnecessary complexity

3. **Syntax simplicity over syntactic sugar**
   - Prefer explicit, clear syntax
   - Remove boilerplate where possible

4. **Explicit over implicit**
   - Type conversions must be explicit
   - Behavior should be predictable
   - Exception: type inference removes obvious boilerplate

5. **Safe by default, unsafe only when explicitly requested**
   - Memory safety guaranteed
   - No undefined behavior
   - Explicit opt-in for unsafe operations

6. **Control without unsafe defaults**
   - C/C++ power with Rust safety
   - Deterministic memory management
   - Native performance

## The Five Dimensions

```
                         ALGOL26

              ┌───────────────────────┐
              │       ALGOL 58        │
              │  algorithms / clarity │
              │  procedures / math    │
              └───────────┬───────────┘
                          +
              ┌───────────▼───────────┐
              │     INDENTATION       │
              │ Python / ISWIM /      │
              │ Haskell readability   │
              └───────────┬───────────┘
                          +
              ┌───────────▼───────────┐
              │   MODERN COMPILER     │
              │ Rust + LLVM + native  │
              │ compilation           │
              └───────────┬───────────┘
                          +
              ┌───────────▼───────────┐
              │  68 YEARS OF LESSONS  │
              │                       │
              │ type safety           │
              │ memory safety         │
              │ concurrency safety    │
              │ expressive types      │
              │ good tooling          │
              │ diagnostics           │
              └───────────┬───────────┘
                          +
              ┌───────────▼───────────┐
              │   SYSTEMS POWER       │
              │                       │
              │ C/C++ level control   │
              │ deterministic memory  │
              │ native performance    │
              │ low-level capability  │
              └───────────┬───────────┘
                          │
                          ▼
                    ┌───────────┐
                    │ ALGOL26   │
                    └───────────┘
```

## Non-Goals

These are things ALGOL26 deliberately does NOT try to do:

1. **Not a general-purpose language for everything**
   - Focus on algorithmic clarity and scientific computing

2. **Not backward compatible with ALGOL 58**
   - Historical inspiration, not historical preservation

3. **Not a functional language**
   - Structured programming core, not functional paradigm

4. **Not a systems programming language**
   - No manual memory management by default
   - Unsafe operations require explicit opt-in

5. **Not a scripting language**
   - Compiled to native code
   - Static typing

6. **Not a garbage-collected language**
   - Deterministic memory management
   - No hidden GC pauses

## Success Criteria

ALGOL26 succeeds when:

1. Programs are more readable than equivalent Python
2. Code is safer than equivalent C
3. Compilation is faster than equivalent Rust
4. Learning curve is gentler than equivalent Haskell
5. Memory management is deterministic (no GC pauses)
6. Performance is comparable to native C
7. The language feels both familiar and modern
8. Safety guarantees are enforced at compile time

## Vision Progress (v0.1.0)

| Vision Element | Status |
|---------------|--------|
| ALGOL 58 heritage | ✅ |
| Indentation-based blocks | ✅ |
| Modern compiler (Rust+LLVM) | ✅ |
| Type safety | ✅ |
| Immutability | ✅ |
| Bounds checking | ✅ |
| Module system | ✅ |
| 3 backends | ✅ |
| Borrowing/ownership | 🔴 Future |
| Controlled memory | 🔴 Future |
| Raw memory (unsafe) | 🔴 Future |
| Systems programming | 🔴 Future |
| Formal "no UB" proof | 🔴 Future |

## Summary

ALGOL26: the clarity and algorithmic spirit of ALGOL 58, the readability of indentation-based languages, the safety lessons of Rust, the control and performance of C/C++, and the compiler technology of 2026.

## Key Research Questions

1. Can we achieve Rust-like safety guarantees with a simpler model?
2. Can we provide C/C++-level control without the danger?
3. Can safety and performance coexist without garbage collection?
4. Can we make the surface language simple while the compiler does sophisticated analysis?
5. Can we find an ALGOL-native safety model that is simpler than Rust's borrow checker?

## Inspiration Sources

- **ALGOL 58/60**: Structure, procedures, algorithms
- **Python**: Indentation, readability
- **ISWIM**: Expression orientation, functional concepts
- **Haskell**: Type system ideas, purity concepts
- **Rust**: Memory safety, ownership ideas
- **C/C++**: Systems programming, performance
- **Go**: Simplicity, fast compilation