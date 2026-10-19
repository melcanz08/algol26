// runtime/args.c — command-line arguments for the ALGOL26 LLVM backend.
//
// Matches the interpreter's `args()`:
//   - Returns List<String>, one entry per *user* argument.
//   - argv[0] (the program name) is skipped, matching the
//     interpreter's std::env::args().skip(1).
//   - Zero user arguments yields an empty list, not NULL and not an
//     error.
//
// The init function is called once from the generated `main`
// wrapper, before `algol26_user_main`, with the `(argc, argv)`
// the C runtime supplies. Globals are overwritten, not appended:
// a second init (e.g. a REPL calling main repeatedly) replaces
// the previous argument vector.
//
// The getter returns a pointer into argv, which the C runtime
// owns for the process lifetime. The LLVM codegen does not free
// or copy it — the buffer the returned { ptr, i64, i64 } points
// at is a heap array of these borrowed pointers.

#include <stdint.h>
#include <stddef.h>

static int64_t g_count = 0;
static char **g_args = NULL;

void algol26_args_init(int argc, char **argv) {
    // Skip the program name, like the interpreter.
    if (argc > 1) {
        g_count = (int64_t)(argc - 1);
        g_args = argv + 1;
    } else {
        g_count = 0;
        g_args = NULL;
    }
}

int64_t algol26_args_count(void) {
    return g_count;
}

const char *algol26_args_get(int64_t i) {
    if (i < 0 || i >= g_count) return NULL;
    return g_args[i];
}
