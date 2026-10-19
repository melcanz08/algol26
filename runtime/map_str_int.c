// runtime/map_str_int.c — Map<String, Int> runtime for the
// ALGOL26 LLVM backend.
//
// Mirrors runtime/map.c (Map<Int, Int>) with two differences:
//   1. Keys are C strings (`const char*`), not `int64_t`.
//   2. The map owns the keys: `strdup` on insert, `free` on
//      `algol26_map_free_str_int`. Callers pass borrowed pointers;
//      the map copies.
//
// Sentinel: `keys[i] == NULL` means empty slot. String keys are
// always non-NULL after `strdup`, so NULL is unambiguous — no
// separate "used" bitmap needed.
//
// Capacity: rounded up to a power of 2, min 16, and doubles via
// grow() when the map is full. This is a deliberate divergence
// from runtime/map.c: Map<Int, Int> still has the fixed-capacity
// limitation (a follow-up), but Map<String, Int> grows correctly
// so its behavior matches the interpreter's HashMap. Without
// this, inserting a 17th entry would wrap the linear-probe loop
// and corrupt the map.
//
// Overwrite semantics match Rust's HashMap::insert (which the
// interpreter uses): an existing key keeps its original string,
// only the value is updated. So re-inserting "a" twice keeps the
// first "a" allocation alive and frees nothing until map free.

#include <stdint.h>
#include <stdlib.h>
#include <string.h>

typedef struct {
    char   **keys;     // NULL = empty slot; non-NULL is strdup'd
    int64_t *values;
    int64_t  len;
    int64_t  cap;      // power of 2
} AlgoStrIntMap;

static uint64_t hash_str(const char *s) {
    // FNV-1a over the string's bytes, same constants as
    // hash_i64 in map.c.
    uint64_t h = 1469598103934665603ULL;
    for (const unsigned char *p = (const unsigned char *)s; *p; p++) {
        h ^= (uint64_t)*p;
        h *= 1099511628211ULL;
    }
    return h;
}

AlgoStrIntMap *algol26_map_new_str_int(int64_t capacity) {
    AlgoStrIntMap *m = malloc(sizeof(AlgoStrIntMap));
    if (!m) return NULL;
    int64_t cap = 16;
    while (cap < capacity) cap *= 2;
    m->cap = cap;
    m->len = 0;
    m->keys = calloc((size_t)cap, sizeof(char *));
    m->values = malloc(sizeof(int64_t) * (size_t)cap);
    if (!m->keys || !m->values) {
        free(m->keys);
        free(m->values);
        free(m);
        return NULL;
    }
    return m;
}

void algol26_map_free_str_int(AlgoStrIntMap *m) {
    if (!m) return;
    for (int64_t i = 0; i < m->cap; i++) {
        free(m->keys[i]);
    }
    free(m->keys);
    free(m->values);
    free(m);
}

// Double the capacity and rehash. Called from insert when the map
// is full, so the linear-probe loop always has at least one empty
// slot to land on. This is a deliberate divergence from
// runtime/map.c: Map<Int, Int> still has the fixed-capacity
// limitation (a follow-up), but Map<String, Int> grows correctly
// so its behavior matches the interpreter's HashMap.
// Forward declaration: grow() re-inserts every entry, and grow is
// defined before insert to keep the file's top-down structure
// (new/free/grow/insert/get/contains/length). The forward
// declaration keeps the call well-typed.
void algol26_map_insert_str_int(AlgoStrIntMap *m, const char *k, int64_t v);

static void grow(AlgoStrIntMap *m) {
    int64_t old_cap = m->cap;
    char **old_keys = m->keys;
    int64_t *old_values = m->values;

    m->cap = old_cap * 2;
    m->keys = calloc((size_t)m->cap, sizeof(char *));
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
    m->len = 0;
    for (int64_t i = 0; i < old_cap; i++) {
        if (old_keys[i] != NULL) {
            algol26_map_insert_str_int(m, old_keys[i], old_values[i]);
            free(old_keys[i]);
        }
    }
    free(old_keys);
    free(old_values);
}

void algol26_map_insert_str_int(AlgoStrIntMap *m, const char *k, int64_t v) {
    if (m == NULL || k == NULL) return;
    if (m->len >= m->cap) grow(m);
    if (m->len >= m->cap) return;  // grow failed (OOM); drop the insert
    uint64_t h = hash_str(k);
    int64_t i = (int64_t)(h & (uint64_t)(m->cap - 1));
    while (m->keys[i] != NULL) {
        if (strcmp(m->keys[i], k) == 0) {
            m->values[i] = v;   // update; keep original key allocation
            return;
        }
        i = (i + 1) & (m->cap - 1);
    }
    // Empty slot: strdup and store.
    // Explicit malloc+copy instead of strdup (POSIX, not C11).
    // The implicit-declaration warning for strdup under -std=c11
    // truncates the returned pointer to 32 bits, causing a segfault.
    size_t klen = strlen(k);
    char *copy = (char *)malloc(klen + 1);
    if (!copy) return;  // OOM; silent reject
    memcpy(copy, k, klen + 1);
    m->keys[i] = copy;
    m->values[i] = v;
    m->len++;
}

int algol26_map_get_str_int(AlgoStrIntMap *m, const char *k, int64_t *out) {
    if (m == NULL || k == NULL) return 0;
    uint64_t h = hash_str(k);
    int64_t i = (int64_t)(h & (uint64_t)(m->cap - 1));
    while (m->keys[i] != NULL) {
        if (strcmp(m->keys[i], k) == 0) {
            *out = m->values[i];
            return 1;
        }
        i = (i + 1) & (m->cap - 1);
    }
    return 0;
}

int algol26_map_contains_str_int(AlgoStrIntMap *m, const char *k) {
    int64_t dummy;
    return algol26_map_get_str_int(m, k, &dummy);
}

int64_t algol26_map_length_str_int(AlgoStrIntMap *m) {
    return m ? m->len : 0;
}
