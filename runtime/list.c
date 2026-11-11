// runtime/list.c
//
// Aggregate operations over ALGOL26 list descriptors.
// Descriptor layout: { T* buf, i64 len, i64 cap }.
// max/min return a deterministic value (0) for empty lists; the
// LLVM codegen wraps the result in Option<T> with tag = (len > 0),
// matching the interpreter's None-on-empty behaviour.

#include <stdint.h>
#include <stddef.h>

int64_t algol26_list_sum_int(const int64_t *buf, int64_t len) {
    int64_t acc = 0;
    for (int64_t i = 0; i < len; i++) acc += buf[i];
    return acc;
}

double algol26_list_sum_float(const double *buf, int64_t len) {
    double acc = 0.0;
    for (int64_t i = 0; i < len; i++) acc += buf[i];
    return acc;
}

int64_t algol26_list_max_int(const int64_t *buf, int64_t len) {
    if (len == 0) return 0;
    int64_t m = buf[0];
    for (int64_t i = 1; i < len; i++) if (buf[i] > m) m = buf[i];
    return m;
}

double algol26_list_max_float(const double *buf, int64_t len) {
    if (len == 0) return 0.0;
    double m = buf[0];
    for (int64_t i = 1; i < len; i++) if (buf[i] > m) m = buf[i];
    return m;
}

int64_t algol26_list_min_int(const int64_t *buf, int64_t len) {
    if (len == 0) return 0;
    int64_t m = buf[0];
    for (int64_t i = 1; i < len; i++) if (buf[i] < m) m = buf[i];
    return m;
}

double algol26_list_min_float(const double *buf, int64_t len) {
    if (len == 0) return 0.0;
    double m = buf[0];
    for (int64_t i = 1; i < len; i++) if (buf[i] < m) m = buf[i];
    return m;
}


// ── print(list) ─────────────────────────────────────────────────────
// Matches the interpreter's [a, b, c] formatting.
#include <stdio.h>

void algol26_print_list_int(const int64_t *buf, int64_t len) {
    printf("[");
    for (int64_t i = 0; i < len; i++) {
        if (i) printf(", ");
        printf("%lld", (long long)buf[i]);
    }
    printf("]");
}

void algol26_print_list_float(const double *buf, int64_t len) {
    printf("[");
    for (int64_t i = 0; i < len; i++) {
        if (i) printf(", ");
        printf("%.6f", buf[i]);
    }
    printf("]");
}

void algol26_print_list_str(const char *const *buf, int64_t len) {
    printf("[");
    for (int64_t i = 0; i < len; i++) {
        if (i) printf(", ");
        printf("%s", buf[i]);
    }
    printf("]");
}
