// runtime/map.c — Map<Int, Int> runtime for ALGOL26 LLVM backend.
//
// Open-addressing hash table with linear probing, growing by
// doubling when the map is full. Matches runtime/map_str_int.c's
// growth strategy so both shapes behave the same past the initial
// capacity — and match the interpreter's HashMap, which grows
// unboundedly.
//
// (Prior to this, insert past capacity looped forever: the
// linear-probe loop had no empty slot to terminate on. A 20-entry
// program hung on LLVM while working on the interpreter. See
// corpus_53_map_int_int_growth.gol for the regression pin.)

#include <stdlib.h>
#include <stdint.h>
#include <string.h>

#define EMPTY_KEY INT64_MIN  // sentinel; users cannot store this key

typedef struct {
    int64_t *keys;     // EMPTY_KEY = empty slot
    int64_t *values;
    int64_t  len;
    int64_t  cap;      // power of 2
} AlgoIntMap;

static uint64_t hash_i64(int64_t k) {
    // FNV-1a over the 8 bytes of the key.
    uint64_t h = 1469598103934665603ULL;
    for (int i = 0; i < 8; i++) {
        h ^= (uint64_t)(k >> (i * 8)) & 0xFF;
        h *= 1099511628211ULL;
    }
    return h;
}

AlgoIntMap *algol26_map_new_int(int64_t capacity) {
    AlgoIntMap *m = malloc(sizeof(AlgoIntMap));
    if (!m) return NULL;
    // Round up to power of 2.
    int64_t cap = 16;
    while (cap < capacity) cap *= 2;
    m->cap = cap;
    m->len = 0;
    m->keys = malloc(sizeof(int64_t) * cap);
    m->values = malloc(sizeof(int64_t) * cap);
    if (!m->keys || !m->values) { free(m->keys); free(m->values); free(m); return NULL; }
    for (int64_t i = 0; i < cap; i++) m->keys[i] = EMPTY_KEY;
    return m;
}

void algol26_map_free_int(AlgoIntMap *m) {
    if (!m) return;
    free(m->keys);
    free(m->values);
    free(m);
}

// Forward declaration: grow() re-inserts every entry, and grow is
// defined before insert to keep the file's top-down structure
// (new/free/grow/insert/get/contains/length).
void algol26_map_insert_int(AlgoIntMap *m, int64_t k, int64_t v);

// Double the capacity and rehash. Called from insert when the map
// is full, so the linear-probe loop always has at least one empty
// slot to land on.
static void grow(AlgoIntMap *m) {
    int64_t old_cap = m->cap;
    int64_t *old_keys = m->keys;
    int64_t *old_values = m->values;

    m->cap = old_cap * 2;
    m->keys = malloc(sizeof(int64_t) * (size_t)m->cap);
    m->values = malloc(sizeof(int64_t) * (size_t)m->cap);
    if (!m->keys || !m->values) {
        // OOM: restore old arrays, drop new. Insert becomes a no-op.
        free(m->keys);
        free(m->values);
        m->keys = old_keys;
        m->values = old_values;
        m->cap = old_cap;
        return;
    }
    for (int64_t i = 0; i < m->cap; i++) m->keys[i] = EMPTY_KEY;
    m->len = 0;
    for (int64_t i = 0; i < old_cap; i++) {
        if (old_keys[i] != EMPTY_KEY) {
            algol26_map_insert_int(m, old_keys[i], old_values[i]);
        }
    }
    free(old_keys);
    free(old_values);
}

void algol26_map_insert_int(AlgoIntMap *m, int64_t k, int64_t v) {
    if (k == EMPTY_KEY) return;
    if (m->len >= m->cap) grow(m);
    if (m->len >= m->cap) return;  // grow failed (OOM); drop the insert
    uint64_t h = hash_i64(k);
    int64_t i = (int64_t)(h & (m->cap - 1));
    while (m->keys[i] != EMPTY_KEY) {
        if (m->keys[i] == k) { m->values[i] = v; return; }  // update
        i = (i + 1) & (m->cap - 1);
    }
    m->keys[i] = k;
    m->values[i] = v;
    m->len++;
}

int algol26_map_get_int(AlgoIntMap *m, int64_t k, int64_t *out) {
    uint64_t h = hash_i64(k);
    int64_t i = (int64_t)(h & (m->cap - 1));
    while (m->keys[i] != EMPTY_KEY) {
        if (m->keys[i] == k) { *out = m->values[i]; return 1; }
        i = (i + 1) & (m->cap - 1);
    }
    return 0;  // not found
}

int algol26_map_contains_int(AlgoIntMap *m, int64_t k) {
    int64_t dummy;
    return algol26_map_get_int(m, k, &dummy);
}

int64_t algol26_map_length_int(AlgoIntMap *m) { return m->len; }

// ── qsort comparators ──
// Ascending int64_t. Matches MapKey::Int's derived Ord.
static int cmp_i64(const void *a, const void *b) {
    int64_t x = *(const int64_t *)a;
    int64_t y = *(const int64_t *)b;
    return (x > y) - (x < y);
}

// Pair struct used by values_int to sort by key then project
// the value. Kept local to the function file; not exported.
typedef struct { int64_t k; int64_t v; } IntKvPair;

// Ascending by key. Matches the interpreter's
// sorted.sort_by(|a, b| a.0.cmp(b.0)).
static int cmp_i64_pair_by_key(const void *a, const void *b) {
    int64_t x = ((const IntKvPair *)a)->k;
    int64_t y = ((const IntKvPair *)b)->k;
    return (x > y) - (x < y);
}

// ── algol26_map_keys_int(m, out_count) ──
//
// Returns a malloc'd array of the map's keys, sorted ascending.
// Caller owns the array. On empty map: *out_count = 0, returns
// NULL (caller treats count=0 as empty and never dereferences).
// Matches the interpreter's `keys().sort()`.
int64_t *algol26_map_keys_int(AlgoIntMap *m, int64_t *out_count) {
    if (out_count) *out_count = 0;
    if (!m || m->len == 0) return NULL;

    int64_t *keys = (int64_t *)malloc(sizeof(int64_t) * (size_t)m->len);
    if (!keys) return NULL;

    int64_t n = 0;
    for (int64_t i = 0; i < m->cap; i++) {
        if (m->keys[i] != EMPTY_KEY) {
            keys[n++] = m->keys[i];
        }
    }
    qsort(keys, (size_t)n, sizeof(int64_t), cmp_i64);
    if (out_count) *out_count = n;
    return keys;
}

// ── algol26_map_values_int(m, out_count) ──
//
// Returns a malloc'd array of the map's values, sorted by their
// corresponding keys (ascending). Caller owns the array. On
// empty map: *out_count = 0, returns NULL.
//
// Matches the interpreter's
// `entries.iter().collect(); sorted.sort_by(|a, b| a.0.cmp(b.0));
//  sorted.into_iter().map(|(_, v)| v.clone()).collect()`.
int64_t *algol26_map_values_int(AlgoIntMap *m, int64_t *out_count) {
    if (out_count) *out_count = 0;
    if (!m || m->len == 0) return NULL;

    IntKvPair *pairs = (IntKvPair *)malloc(sizeof(IntKvPair) * (size_t)m->len);
    if (!pairs) return NULL;
    int64_t n = 0;
    for (int64_t i = 0; i < m->cap; i++) {
        if (m->keys[i] != EMPTY_KEY) {
            pairs[n].k = m->keys[i];
            pairs[n].v = m->values[i];
            n++;
        }
    }
    qsort(pairs, (size_t)n, sizeof(IntKvPair), cmp_i64_pair_by_key);

    int64_t *values = (int64_t *)malloc(sizeof(int64_t) * (size_t)n);
    if (!values) { free(pairs); return NULL; }
    for (int64_t i = 0; i < n; i++) values[i] = pairs[i].v;
    free(pairs);

    if (out_count) *out_count = n;
    return values;
}

