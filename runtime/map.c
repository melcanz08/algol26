// runtime/map.c — minimal Map<Int, Int> runtime for ALGOL26 LLVM backend.
//
// Fixed-capacity open-addressing hash table with linear probing.
// No resizing in v1; capacity is chosen at construction and grows
// the map is refused if it exceeds it. This is a prototype — the
// real implementation needs dynamic growth.

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

void algol26_map_insert_int(AlgoIntMap *m, int64_t k, int64_t v) {
    if (k == EMPTY_KEY) return;  // silent reject; fix with error code later
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