// runtime/string.c — String.to_int runtime for the ALGOL26 LLVM backend.
//
// Matches the interpreter's `String.to_int` semantics exactly:
// Rust's `str::parse::<i64>()`, which is strict.
//
//   "42"                    -> Some(42)
//   "-7"                    -> Some(-7)
//   "+42"                   -> Some(42)
//   "007"                   -> Some(7)
//   "9223372036854775807"   -> Some(i64::MAX)
//   "-9223372036854775808"  -> Some(i64::MIN)
//
//   ""                      -> None
//   " 42", "42 ", "\t42"    -> None  (no whitespace allowed)
//   "42abc", "42.0", "0x10" -> None  (no trailing garbage)
//   "+", "-"                -> None  (sign with no digits)
//   "9223372036854775808"   -> None  (i64::MAX + 1, overflow)
//   "-9223372036854775809"  -> None  (i64::MIN - 1, underflow)
//
// Return convention matches algol26_map_get_int in runtime/map.c:
// the function returns 0 or 1 (an `int32_t`, matching C's int),
// and on success writes the parsed value through `*out`. The LLVM
// call site converts the 0/1 to an i1 and packs it with the i64
// payload into the `{ i1, i64 }` Option<Int> struct — the same
// shape as the Map.get lowering in builtins.rs.

#include <stdint.h>
#include <errno.h>
#include <limits.h>
#include <stdlib.h>
#include <string.h>

int32_t algol26_string_to_int(const char *s, int64_t *out) {
    if (s == NULL || out == NULL) return 0;

    // Pre-validate: optional sign, then one or more ASCII digits,
    // then end of string. This is what rejects leading/trailing
    // whitespace, "0x", "1.5", and trailing garbage — all of which
    // strtoll would otherwise accept.
    const char *p = s;
    if (*p == '+' || *p == '-') p++;
    if (*p < '0' || *p > '9') return 0;
    while (*p >= '0' && *p <= '9') p++;
    if (*p != '\0') return 0;

    // Range check. strtoll sets ERANGE on overflow and clamps the
    // return to LLONG_MAX/LLONG_MIN, so we must inspect errno.
    errno = 0;
    char *end = NULL;
    long long v = strtoll(s, &end, 10);
    if (errno == ERANGE) return 0;
    // Unreachable after the pre-validation, but cheap defence in
    // depth in case the two checks ever drift.
    if (end == s || *end != '\0') return 0;

    *out = (int64_t)v;
    return 1;
}

// ── algol26_string_substring(s, start, length) ──
//
// Returns the `length` codepoints starting at codepoint `start`.
// Codepoint-indexed to match the interpreter's
// `chars().skip(start).take(length).collect()`.
//
// This is the correct implementation for the interpreter contract.
// The previous version of this function lived inline in
// src/backends/llvm_codegen/builtins.rs and used strlen + memcpy,
// which is byte-indexed: on "héllo", substring(1, 2) returned the
// full 2-byte "é" instead of "él". The bug was invisible to the
// corpus because every String.substring test used ASCII.
//
// Bounds: start and length are clamped to [0, codepoint_count].
// A start past the end returns "". A length past the end is
// truncated. Negative values are treated as 0.
//
// Invalid UTF-8 in the input is not validated — the walker treats
// any byte that is not a continuation byte (top two bits != 10)
// as a codepoint start, mirroring algol26_strlen_utf8. See
// docs/STATUS.md "Known Gaps" (File.read UTF-8 validation).
char *algol26_string_substring(const char *s, int64_t start, int64_t length) {
    if (s == NULL) return NULL;
    if (start < 0) start = 0;
    if (length < 0) length = 0;

    // Walk forward `start` codepoints. A codepoint begins at any
    // byte that is not a UTF-8 continuation byte (10xxxxxx).
    const char *p = s;
    int64_t cp = 0;
    while (*p != '\0' && cp < start) {
        p++;
        while ((*p & 0xC0) == 0x80) p++;
        cp++;
    }

    // If we stopped short of `start` (start past end), return "".
    // Otherwise walk `length` more codepoints.
    const char *q = p;
    int64_t taken = 0;
    while (*q != '\0' && taken < length) {
        q++;
        while ((*q & 0xC0) == 0x80) q++;
        taken++;
    }

    size_t nbytes = (size_t)(q - p);
    char *buf = (char *)malloc(nbytes + 1);
    if (buf == NULL) return NULL;
    if (nbytes > 0) memcpy(buf, p, nbytes);
    buf[nbytes] = '\0';
    return buf;
}

