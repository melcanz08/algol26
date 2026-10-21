// runtime/map_str_str.c — Map<String, String> runtime for the
// ALGOL26 LLVM backend.
//
// Same open-addressing table as runtime/map_str_int.c and
// runtime/map_int_int.c (via map.c), with one difference: values
// are C strings, and the map OWNS both keys and values.
//
// Ownership contract:
//   - insert(k, v): strdups both k and v. Overwrite frees the old
//     value before strdup-ing the new one.
//   - get(k, out): on success, strdups the stored value into *out.
//     The caller OWNS the returned string and must free it (or
//     leak — the LLVM arm does not track Option<String> lifetime,
//     matching the File.read / Int.to_string leak trade-off).
//   - free(m): frees every key, every value, and both arrays.
//   - grow-rehash: frees the old key/value pointers after copying
//     into the new arrays.
//
// The get-strdup behavior matches the interpreter's
// `entries.get(&key) → Some(v) → Box::new(v.clone())` — the
// returned value is a copy, so subsequent map mutations cannot
// invalidate it.

#include <stdint.h>
#include <stdlib.h>
#include <string.h>

typedef struct {
    char   **keys;      // NULL = empty slot; non-NULL is strdup'd
    char   **values;    // strdup'd; paired with keys[i]
    int64_t  len;
    int64_t  cap;       // power of 2
} AlgoStrStrMap;

static uint64_t hash_str(const char *s) {
    uint64_t h = 1469598103934665603ULL;
    for (const unsigned char *p = (const unsigned char *)s; *p; p++) {
        h ^= (uint64_t)*p;
        h *= 1099511628211ULL;
    }
    return h;
}

// Copy `src` into a fresh malloc'd NUL-terminated buffer.
// Returns NULL on OOM or NULL src.
static char *dup_str(const char *src) {
    if (!src) return NULL;
    size_t n = strlen(src);
    char *copy = (char *)malloc(n + 1);
    if (!copy) return NULL;
    memcpy(copy, src, n + 1);
    return copy;
}

// Forward declaration: grow() re-inserts every entry and is
// defined before insert.
void algol26_map_insert_str_str(AlgoStrStrMap *m, const char *k, const char *v);

static void grow(AlgoStrStrMap *m) {
    int64_t old_cap = m->cap;
    char **old_keys = m->keys;
    char **old_values = m->values;

    m->cap = old_cap * 2;
    m->keys = calloc((size_t)m->cap, sizeof(char *));
    m->values = calloc((size_t)m->cap, sizeof(char *));
    if (!m->keys || !m->values) {
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
            algol26_map_insert_str_str(m, old_keys[i], old_values[i]);
            free(old_keys[i]);
            free(old_values[i]);
        }
    }
    free(old_keys);
    free(old_values);
}

AlgoStrStrMap *algol26_map_new_str_str(int64_t capacity) {
    AlgoStrStrMap *m = malloc(sizeof(AlgoStrStrMap));
    if (!m) return NULL;
    int64_t cap = 16;
    while (cap < capacity) cap *= 2;
    m->cap = cap;
    m->len = 0;
    m->keys = calloc((size_t)cap, sizeof(char *));
    m->values = calloc((size_t)cap, sizeof(char *));
    if (!m->keys || !m->values) {
        free(m->keys);
        free(m->values);
        free(m);
        return NULL;
    }
    return m;
}

void algol26_map_free_str_str(AlgoStrStrMap *m) {
    if (!m) return;
    for (int64_t i = 0; i < m->cap; i++) {
        free(m->keys[i]);
        free(m->values[i]);
    }
    free(m->keys);
    free(m->values);
    free(m);
}

void algol26_map_insert_str_str(AlgoStrStrMap *m, const char *k, const char *v) {
    if (m == NULL || k == NULL || v == NULL) return;
    if (m->len >= m->cap) grow(m);
    if (m->len >= m->cap) return;  // grow failed (OOM)
    uint64_t h = hash_str(k);
    int64_t i = (int64_t)(h & (uint64_t)(m->cap - 1));
    while (m->keys[i] != NULL) {
        if (strcmp(m->keys[i], k) == 0) {
            // Overwrite: free old value, strdup new. Keep original key.
            char *newv = dup_str(v);
            if (!newv) return;
            free(m->values[i]);
            m->values[i] = newv;
            return;
        }
        i = (i + 1) & (m->cap - 1);
    }
    char *key_copy = dup_str(k);
    char *val_copy = dup_str(v);
    if (!key_copy || !val_copy) {
        free(key_copy);
        free(val_copy);
        return;
    }
    m->keys[i] = key_copy;
    m->values[i] = val_copy;
    m->len++;
}

int algol26_map_get_str_str(AlgoStrStrMap *m, const char *k, char **out) {
    if (m == NULL || k == NULL || out == NULL) return 0;
    uint64_t h = hash_str(k);
    int64_t i = (int64_t)(h & (uint64_t)(m->cap - 1));
    while (m->keys[i] != NULL) {
        if (strcmp(m->keys[i], k) == 0) {
            // Return an owned COPY so later mutations cannot
            // invalidate it. Caller must free.
            char *copy = dup_str(m->values[i]);
            if (!copy) return 0;
            *out = copy;
            return 1;
        }
        i = (i + 1) & (m->cap - 1);
    }
    return 0;
}

int algol26_map_contains_str_str(AlgoStrStrMap *m, const char *k) {
    if (m == NULL || k == NULL) return 0;
    uint64_t h = hash_str(k);
    int64_t i = (int64_t)(h & (uint64_t)(m->cap - 1));
    while (m->keys[i] != NULL) {
        if (strcmp(m->keys[i], k) == 0) return 1;
        i = (i + 1) & (m->cap - 1);
    }
    return 0;
}

int64_t algol26_map_length_str_str(AlgoStrStrMap *m) {
    return m ? m->len : 0;
}

// ── qsort comparators ──
static int cmp_str_kv(const void *a, const void *b) {
    const char *x = *(const char *const *)a;
    const char *y = *(const char *const *)b;
    return strcmp(x, y);
}

typedef struct { char *k; char *v; } StrStrKvPair;

static int cmp_str_str_pair_by_key(const void *a, const void *b) {
    return strcmp(((const StrStrKvPair *)a)->k, ((const StrStrKvPair *)b)->k);
}

static char *kv_dup(const char *src) {
    if (!src) return NULL;
    size_t n = strlen(src);
    char *copy = (char *)malloc(n + 1);
    if (!copy) return NULL;
    memcpy(copy, src, n + 1);
    return copy;
}

// ── algol26_map_keys_str_str(m, out_count) ──
char **algol26_map_keys_str_str(AlgoStrStrMap *m, int64_t *out_count) {
    if (out_count) *out_count = 0;
    if (!m || m->len == 0) return NULL;

    char **keys = (char **)malloc(sizeof(char *) * (size_t)m->len);
    if (!keys) return NULL;
    int64_t n = 0;
    for (int64_t i = 0; i < m->cap; i++) {
        if (m->keys[i] != NULL) {
            char *copy = kv_dup(m->keys[i]);
            if (!copy) {
                for (int64_t j = 0; j < n; j++) free(keys[j]);
                free(keys);
                return NULL;
            }
            keys[n++] = copy;
        }
    }
    qsort(keys, (size_t)n, sizeof(char *), cmp_str_kv);
    if (out_count) *out_count = n;
    return keys;
}

// ── algol26_map_values_str_str(m, out_count) ──
char **algol26_map_values_str_str(AlgoStrStrMap *m, int64_t *out_count) {
    if (out_count) *out_count = 0;
    if (!m || m->len == 0) return NULL;

    StrStrKvPair *pairs = (StrStrKvPair *)malloc(sizeof(StrStrKvPair) * (size_t)m->len);
    if (!pairs) return NULL;
    int64_t n = 0;
    for (int64_t i = 0; i < m->cap; i++) {
        if (m->keys[i] != NULL) {
            pairs[n].k = m->keys[i];
            pairs[n].v = m->values[i];
            n++;
        }
    }
    qsort(pairs, (size_t)n, sizeof(StrStrKvPair), cmp_str_str_pair_by_key);

    char **values = (char **)malloc(sizeof(char *) * (size_t)n);
    if (!values) { free(pairs); return NULL; }
    for (int64_t i = 0; i < n; i++) {
        char *copy = kv_dup(pairs[i].v);
        if (!copy) {
            for (int64_t j = 0; j < i; j++) free(values[j]);
            free(values);
            free(pairs);
            return NULL;
        }
        values[i] = copy;
    }
    free(pairs);

    if (out_count) *out_count = n;
    return values;
}

