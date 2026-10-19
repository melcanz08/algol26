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

// ── algol26_string_char_at(s, i) ──
//
// Returns a 1-codepoint string for the i-th codepoint of s, or NULL
// if i is out of range (negative, or >= the codepoint count). The
// LLVM codegen arm wraps the result in Option<String>.
//
// Codepoint-indexed, like algol26_string_substring and
// algol26_strlen_utf8. Matches the interpreter's
// `s.chars().nth(i as usize)`.
//
// Distinct from substring(s, i, 1): substring CLAMPS — an out-of-
// range start returns "" (non-NULL). charAt must return NULL so
// the caller's Option is None, not Some(""). Hence the explicit
// walker here rather than a delegation.
char *algol26_string_char_at(const char *s, int64_t i) {
    if (s == NULL || i < 0) return NULL;

    // Walk to the i-th codepoint.
    const char *p = s;
    int64_t cp = 0;
    while (*p != '\0' && cp < i) {
        p++;
        while ((*p & 0xC0) == 0x80) p++;
        cp++;
    }

    // If we ran out of string before reaching i, i is past the end.
    if (*p == '\0') return NULL;

    // The codepoint at p is one lead byte plus any continuation bytes.
    const char *q = p + 1;
    while ((*q & 0xC0) == 0x80) q++;
    size_t nbytes = (size_t)(q - p);

    char *buf = (char *)malloc(nbytes + 1);
    if (buf == NULL) return NULL;
    memcpy(buf, p, nbytes);
    buf[nbytes] = '\0';
    return buf;
}

// ── Unicode whitespace classification ──
//
// Matches Rust's `char::is_whitespace`, which is what the
// interpreter's `String.trim` uses (via `str::trim()`). The
// Unicode White_Space property covers these codepoints:
//
//   0x0009..=0x000D  (tab, LF, VT, FF, CR)
//   0x0020           (space)
//   0x0085           (NEL)
//   0x00A0           (NBSP)
//   0x1680           (Ogham space mark)
//   0x2000..=0x200A  (en quad .. hair space)
//   0x2028, 0x2029   (line separator, paragraph separator)
//   0x202F           (narrow NBSP)
//   0x205F           (medium mathematical space)
//   0x3000           (ideographic space)
//
// Using ctype's `isspace` would be simpler but diverges from the
// interpreter on non-ASCII whitespace (and is unsafe on UTF-8
// lead bytes, whose argument must be representable as
// `unsigned char`). Walking codepoints is the correct scope.
static int is_unicode_whitespace(uint32_t cp) {
    return (cp >= 0x09 && cp <= 0x0D)
        || cp == 0x20
        || cp == 0x85
        || cp == 0xA0
        || cp == 0x1680
        || (cp >= 0x2000 && cp <= 0x200A)
        || cp == 0x2028
        || cp == 0x2029
        || cp == 0x202F
        || cp == 0x205F
        || cp == 0x3000;
}

// Decode the UTF-8 codepoint starting at `p`. `*adv` receives the
// byte length of the sequence (1..4). Invalid UTF-8: returns the
// first byte as a codepoint and adv = 1 — same tolerant walk the
// rest of runtime/string.c uses.
static uint32_t decode_utf8(const char *p, size_t *adv) {
    unsigned char b0 = (unsigned char)p[0];
    if (b0 < 0x80) { *adv = 1; return b0; }
    if ((b0 & 0xE0) == 0xC0 && p[1] != '\0') {
        *adv = 2;
        return ((uint32_t)(b0 & 0x1F) << 6)
             | ((uint32_t)((unsigned char)p[1] & 0x3F));
    }
    if ((b0 & 0xF0) == 0xE0 && p[1] != '\0' && p[2] != '\0') {
        *adv = 3;
        return ((uint32_t)(b0 & 0x0F) << 12)
             | ((uint32_t)((unsigned char)p[1] & 0x3F) << 6)
             | ((uint32_t)((unsigned char)p[2] & 0x3F));
    }
    if ((b0 & 0xF8) == 0xF0 && p[1] != '\0' && p[2] != '\0' && p[3] != '\0') {
        *adv = 4;
        return ((uint32_t)(b0 & 0x07) << 18)
             | ((uint32_t)((unsigned char)p[1] & 0x3F) << 12)
             | ((uint32_t)((unsigned char)p[2] & 0x3F) << 6)
             | ((uint32_t)((unsigned char)p[3] & 0x3F));
    }
    *adv = 1;
    return b0;
}

// ── algol26_string_trim(s) ──
//
// Returns a malloc'd copy of `s` with leading and trailing
// Unicode-whitespace codepoints removed. Matches the interpreter's
// `str::trim()`. The caller owns the returned buffer.
//
// If `s` is all whitespace, returns a fresh empty string (non-NULL,
// zero-length). If `s` is NULL, returns NULL.
char *algol26_string_trim(const char *s) {
    if (s == NULL) return NULL;

    // Walk forward, skipping whitespace.
    const char *start = s;
    while (*start != '\0') {
        size_t adv = 0;
        uint32_t cp = decode_utf8(start, &adv);
        if (!is_unicode_whitespace(cp)) break;
        start += adv;
    }

    // If start is at the end (all whitespace), return "".
    if (*start == '\0') {
        char *empty = (char *)malloc(1);
        if (empty) empty[0] = '\0';
        return empty;
    }

    // Walk backward, skipping whitespace. We can't walk backward
    // over UTF-8 easily (continuation bytes), so scan forward and
    // remember the last non-whitespace byte-end.
    const char *end = start;
    const char *p = start;
    while (*p != '\0') {
        size_t adv = 0;
        uint32_t cp = decode_utf8(p, &adv);
        if (!is_unicode_whitespace(cp)) end = p + adv;
        p += adv;
    }

    size_t nbytes = (size_t)(end - start);
    char *buf = (char *)malloc(nbytes + 1);
    if (buf == NULL) return NULL;
    if (nbytes > 0) memcpy(buf, start, nbytes);
    buf[nbytes] = '\0';
    return buf;
}

// ── algol26_string_split(s, sep, out_count) ──
//
// Matches Rust's `str::split(sep)` as used by the interpreter:
//
//   "a,b,c"  / ","  -> ["a", "b", "c"]
//   "a,,b"   / ","  -> ["a", "", "b"]
//   "a,b,"   / ","  -> ["a", "b", ""]
//   ""       / ","  -> [""]                       (one empty)
//   "abc"    / ""   -> ["", "a", "b", "c", ""]     (leading + trailing
//                                                    empty, chars between;
//                                                    codepoints, not bytes)
//   "héllo"  / ""   -> ["", "h", "é", "l", "l", "o", ""]
//   "a"      / "a"  -> ["", ""]
//
// out_count receives the element count. The returned array is
// heap-allocated; every element is `strdup`'d and owned by the
// caller. Returns NULL on allocation failure.
char **algol26_string_split(const char *s, const char *sep, int64_t *out_count) {
    if (s == NULL || sep == NULL || out_count == NULL) {
        if (out_count) *out_count = 0;
        return NULL;
    }

    size_t seplen = strlen(sep);

    // ── Empty separator: split by codepoint ──
    // Result is [""] + each codepoint + [""].
    if (seplen == 0) {
        // Count codepoints.
        int64_t cp_count = 0;
        const char *p = s;
        while (*p != '\0') {
            size_t adv = 0;
            (void)decode_utf8(p, &adv);
            p += adv;
            cp_count++;
        }
        int64_t total = cp_count + 2;
        char **result = (char **)malloc(sizeof(char *) * (size_t)total);
        if (!result) { *out_count = 0; return NULL; }

        // [0] = ""
        result[0] = (char *)malloc(1);
        if (!result[0]) { free(result); *out_count = 0; return NULL; }
        result[0][0] = '\0';

        // [1..cp_count+1] = each codepoint as its own string
        p = s;
        for (int64_t i = 0; i < cp_count; i++) {
            size_t adv = 0;
            (void)decode_utf8(p, &adv);
            char *cp = (char *)malloc(adv + 1);
            if (!cp) {
                for (int64_t k = 0; k < i + 1; k++) free(result[k]);
                free(result);
                *out_count = 0;
                return NULL;
            }
            memcpy(cp, p, adv);
            cp[adv] = '\0';
            result[i + 1] = cp;
            p += adv;
        }

        // [cp_count+1] = ""
        result[cp_count + 1] = (char *)malloc(1);
        if (!result[cp_count + 1]) {
            for (int64_t k = 0; k < cp_count + 1; k++) free(result[k]);
            free(result);
            *out_count = 0;
            return NULL;
        }
        result[cp_count + 1][0] = '\0';

        *out_count = total;
        return result;
    }

    // ── Non-empty separator: byte-substring split (Rust's str::split) ──
    // Count occurrences to pre-allocate.
    int64_t count = 1;  // at least one element
    const char *scan = s;
    while ((scan = strstr(scan, sep)) != NULL) {
        count++;
        scan += seplen;
    }

    char **result = (char **)malloc(sizeof(char *) * (size_t)count);
    if (!result) { *out_count = 0; return NULL; }

    // Fill in each slice.
    int64_t idx = 0;
    const char *start = s;
    const char *next;
    while ((next = strstr(start, sep)) != NULL) {
        size_t len = (size_t)(next - start);
        char *slice = (char *)malloc(len + 1);
        if (!slice) {
            for (int64_t k = 0; k < idx; k++) free(result[k]);
            free(result);
            *out_count = 0;
            return NULL;
        }
        memcpy(slice, start, len);
        slice[len] = '\0';
        result[idx++] = slice;
        start = next + seplen;
    }
    // Final slice after the last separator.
    {
        size_t len = strlen(start);
        char *slice = (char *)malloc(len + 1);
        if (!slice) {
            for (int64_t k = 0; k < idx; k++) free(result[k]);
            free(result);
            *out_count = 0;
            return NULL;
        }
        memcpy(slice, start, len + 1);
        result[idx++] = slice;
    }

    *out_count = count;
    return result;
}

// ── algol26_string_join(items, count, sep) ──
//
// Matches Rust's `slice::join(sep)` on a `Vec<&str>`:
//
//   ["a","b","c"] / ","  -> "a,b,c"
//   []            / ","  -> ""
//   ["a"]         / ","  -> "a"
//   ["","", ""]   / ","  -> ",,"
//
// Returns a heap-allocated concatenation. Caller owns it.
char *algol26_string_join(char **items, int64_t count, const char *sep) {
    if (sep == NULL) return NULL;
    if (count < 0) count = 0;

    size_t seplen = strlen(sep);

    // Total length: sum of all item lengths + seplen * (count - 1)
    //               + 1 for NUL.
    size_t total = 1;
    for (int64_t i = 0; i < count; i++) {
        const char *item = (items && items[i]) ? items[i] : "";
        total += strlen(item);
    }
    if (count > 1) {
        total += seplen * (size_t)(count - 1);
    }

    char *buf = (char *)malloc(total);
    if (!buf) return NULL;

    size_t pos = 0;
    for (int64_t i = 0; i < count; i++) {
        if (i > 0) {
            memcpy(buf + pos, sep, seplen);
            pos += seplen;
        }
        const char *item = (items && items[i]) ? items[i] : "";
        size_t ilen = strlen(item);
        memcpy(buf + pos, item, ilen);
        pos += ilen;
    }
    buf[pos] = '\0';
    return buf;
}

