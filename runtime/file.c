// runtime/file.c — File.* runtime for the ALGOL26 LLVM backend.
//
// Matches the interpreter's success semantics (src/backends/interpreter/eval.rs):
//
//   File.read(path)              -> String  (contents; invalid UTF-8 is
//                                            NOT validated in v1 — see
//                                            "Known Gaps" in docs/STATUS.md)
//   File.write(path, content)    -> Int     (byte count written)
//   File.append(path, content)   -> Int     (byte count appended)
//
// On error, the C layer returns NULL / 0 without printing. The
// codegen arm (src/backends/llvm_codegen/builtins.rs) emits the
// error branch: fprintf(stderr, ...) + exit(1), matching the
// interpreter's `Err(EvalError::Runtime(...))` abort-on-error
// discipline. Keeping the C layer policy-free means a future
// backend can choose different error handling without duplicating
// the IO primitives.
//
// Memory: algol26_file_read returns a malloc'd, NUL-terminated
// buffer that the caller owns. The LLVM codegen stores it as a
// String value; freeing is deferred (compilers run for seconds,
// one leak per file read is acceptable — same trade-off as
// Int.to_string's sprintf buffer).
//
// File.write truncates (std::fs::write semantics). File.append
// creates the file if absent (OpenOptions::create(true).append(true)
// semantics; C's "ab" mode matches).

#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <errno.h>

// Read the entire file at `path` into a malloc'd NUL-terminated
// buffer. Returns NULL on any error (open failure, seek failure,
// allocation failure). The caller owns the returned buffer.
//
// No UTF-8 validation. The interpreter uses
// `std::fs::read_to_string`, which rejects invalid UTF-8; LLVM's
// File.read is byte-transparent in v1. See docs/STATUS.md
// "Known Gaps".
char *algol26_file_read(const char *path) {
    if (path == NULL) return NULL;

    FILE *f = fopen(path, "rb");
    if (f == NULL) return NULL;

    // Seek to end to size the buffer. Not all streams are seekable,
    // but a regular file opened for read always is; if ftell fails,
    // fall back to a grow-by-chunks read. For v1 we only handle
    // seekable files (the common case for source code and data).
    if (fseek(f, 0, SEEK_END) != 0) {
        fclose(f);
        return NULL;
    }
    long size = ftell(f);
    if (size < 0) {
        fclose(f);
        return NULL;
    }
    if (fseek(f, 0, SEEK_SET) != 0) {
        fclose(f);
        return NULL;
    }

    // size + 1 for the NUL terminator. Guard against overflow:
    // if size == LONG_MAX, size+1 overflows size_t on 32-bit.
    // Real files won't be that large; reject rather than corrupt.
    if ((unsigned long)size > (SIZE_MAX - 1)) {
        fclose(f);
        return NULL;
    }

    char *buf = (char *)malloc((size_t)size + 1);
    if (buf == NULL) {
        fclose(f);
        return NULL;
    }

    size_t read = fread(buf, 1, (size_t)size, f);
    fclose(f);

    // A short read means the file changed size under us or IO
    // failed. Treat as error rather than returning a truncated
    // buffer silently.
    if (read != (size_t)size) {
        free(buf);
        return NULL;
    }

    buf[read] = '\0';
    return buf;
}

// Write `content` to `path`, truncating. Returns the number of
// bytes written on success, 0 on error (0 is ambiguous with an
// empty write — but the codegen arm distinguishes success from
// failure by inspecting the underlying fwrite result; see below).
//
// To avoid the ambiguity: return (int64_t)-1 on error, and the
// byte count on success. Callers check < 0.
int64_t algol26_file_write(const char *path, const char *content) {
    if (path == NULL || content == NULL) return -1;

    FILE *f = fopen(path, "wb");
    if (f == NULL) return -1;

    size_t len = strlen(content);
    size_t written = fwrite(content, 1, len, f);
    int close_ok = (fclose(f) == 0);

    if (written != len || !close_ok) return -1;
    return (int64_t)len;
}

// Append `content` to `path`, creating the file if it does not
// exist. Returns the byte count appended on success, -1 on error.
int64_t algol26_file_append(const char *path, const char *content) {
    if (path == NULL || content == NULL) return -1;

    FILE *f = fopen(path, "ab");
    if (f == NULL) return -1;

    size_t len = strlen(content);
    size_t written = fwrite(content, 1, len, f);
    int close_ok = (fclose(f) == 0);

    if (written != len || !close_ok) return -1;
    return (int64_t)len;
}
