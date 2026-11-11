#!/usr/bin/env node
//
// runtime/wasm/host.js
//
// Minimal Node host for ALGOL26 WebAssembly modules.
//
// The module is linked with `wasm-ld --allow-undefined`, so the C
// library functions it references (printf, exit, malloc, free,
// sqrt, ...) arrive as imports from the `env` module. This script
// implements them well enough to run ALGOL26 programs.
//
// Usage:
//   node runtime/wasm/host.js path/to/module.wasm

'use strict';

const fs = require('fs');

const wasmPath = process.argv[2];
if (!wasmPath) {
    console.error('usage: node runtime/wasm/host.js <module.wasm>');
    process.exit(1);
}

const bytes = fs.readFileSync(wasmPath);
const wasmModule = new WebAssembly.Module(bytes);

// The module's linear memory is only reachable after instantiation,
// but `printf` needs it during execution. Mutable closure variable,
// set before main() runs.
let linearMemory = null;

// Bump allocator state. We start past the module's static data and
// stack; 1 MB is conservative.
const HEAP_START = 1 << 20;
let heapTop = 0;

function readCString(ptr) {
    if (!linearMemory) return '';
    const mem = new Uint8Array(linearMemory.buffer);
    let end = ptr;
    while (end < mem.length && mem[end] !== 0) end++;
    return Buffer.from(mem.buffer, ptr, end - ptr).toString('utf8');
}

const env = {
    // `sprintf` — like printf but writes to a memory buffer instead
    // of stdout. Used by Int.to_string and other number→string
    // builtins the compiler now emits. Same format-specifier support
    // and wasm32 variadic ABI as `printf` above.
    sprintf: function (dstPtr, fmtPtr) {
        const ptrs = Array.prototype.slice.call(arguments, 2);
        const view = new Uint8Array(linearMemory.buffer);
        const dv = new DataView(linearMemory.buffer);

        const readStrAt = (p) => {
            let e = p;
            while (e < view.length && view[e] !== 0) e++;
            return Buffer.from(view.buffer, p, e - p).toString('utf8');
        };

        let e1 = fmtPtr;
        while (e1 < view.length && view[e1] !== 0) e1++;
        const fmt = Buffer.from(view.buffer, fmtPtr, e1 - fmtPtr).toString('utf8');

        let out = '';
        let argIdx = 0;
        let i = 0;
        while (i < fmt.length) {
            const ch = fmt[i];
            if (ch !== '%') { out += ch; i++; continue; }
            if (fmt[i + 1] === '%') { out += '%'; i += 2; continue; }

            let j = i + 1;
            while (j < fmt.length && '+- #0123456789.*'.includes(fmt[j])) j++;
            let lenMod = '';
            while (j < fmt.length && (fmt[j] === 'l' || fmt[j] === 'h')) {
                lenMod += fmt[j];
                j++;
            }
            if (j >= fmt.length) { out += fmt.slice(i); break; }
            const conv = fmt[j];
            const slot = ptrs[argIdx++];

            switch (conv) {
                case 'd':
                case 'i':
                    if (lenMod === 'll' || lenMod === 'l') {
                        out += dv.getBigInt64(slot, true).toString();
                    } else {
                        out += dv.getInt32(slot, true).toString();
                    }
                    break;
                case 'u':
                    if (lenMod === 'll' || lenMod === 'l') {
                        out += dv.getBigUint64(slot, true).toString();
                    } else {
                        out += dv.getUint32(slot, true).toString();
                    }
                    break;
                case 's':
                    out += readStrAt(dv.getUint32(slot, true));
                    break;
                case 'f': {
                    // Read the precision from the format specifier,
                    // defaulting to 6 (C standard for `%f`). The
                    // substring between `i + 1` and `j` holds
                    // flags/width/precision, possibly followed by
                    // length modifiers.
                    let prec = 6;
                    const spec = fmt.slice(i + 1, j).replace(/[lh]+$/, '');
                    const dot = spec.lastIndexOf('.');
                    if (dot !== -1) {
                        const digits = spec.slice(dot + 1);
                        if (/^\d+$/.test(digits)) {
                            prec = parseInt(digits, 10);
                        }
                    }
                    out += dv.getFloat64(slot, true).toFixed(prec);
                    break;
                }
                default:
                    out += '%' + conv;
            }
            i = j + 1;
        }

        for (let n = 0; n < out.length; n++) {
            view[dstPtr + n] = out.charCodeAt(n);
        }
        view[dstPtr + out.length] = 0;
        return out.length;
    },
    // ─── stdio ───
    printf: function (fmtPtr) {
        // wasm32 C ABI: variadic args are passed BY REFERENCE — each
        // argument is a pointer to a memory slot holding the value.
        // The format string itself is a fixed arg and is passed
        // directly. We dereference each vararg according to the type
        // the format specifier asks for.
        //
        // LIMITATION: only the formats the ALGOL26 compiler currently
        // emits are handled — `%lld\n` (Int), `%.1f\n` (Float), `%s\n`
        // (String). In particular, `%f` without a precision always
        // prints one decimal place, which happens to match `%.1f`
        // but not C's default of 6. Extend when a new lowering needs it.
        const ptrs = Array.prototype.slice.call(arguments, 1);
        const view = new Uint8Array(linearMemory.buffer);
        const dv = new DataView(linearMemory.buffer);

        const readStrAt = (p) => {
            let e = p;
            while (e < view.length && view[e] !== 0) e++;
            return Buffer.from(view.buffer, p, e - p).toString('utf8');
        };

        let e1 = fmtPtr;
        while (e1 < view.length && view[e1] !== 0) e1++;
        const fmt = Buffer.from(view.buffer, fmtPtr, e1 - fmtPtr).toString('utf8');

        let out = '';
        let argIdx = 0;
        let i = 0;
        while (i < fmt.length) {
            const ch = fmt[i];
            if (ch !== '%') { out += ch; i++; continue; }
            if (fmt[i + 1] === '%') { out += '%'; i += 2; continue; }

            // Scan flags / width / precision
            let j = i + 1;
            while (j < fmt.length && '+- #0123456789.*'.includes(fmt[j])) j++;
            // Scan length modifiers (`l`, `ll`, `h`, `hh`)
            let lenMod = '';
            while (j < fmt.length && (fmt[j] === 'l' || fmt[j] === 'h')) {
                lenMod += fmt[j];
                j++;
            }
            if (j >= fmt.length) { out += fmt.slice(i); break; }
            const conv = fmt[j];

            if (argIdx >= ptrs.length) {
                out += '?';
                i = j + 1;
                continue;
            }
            const slot = ptrs[argIdx++];

            switch (conv) {
                case 's': {
                    // slot holds a 4-byte char* in wasm32
                    const strPtr = dv.getUint32(slot, true);
                    out += readStrAt(strPtr);
                    break;
                }
                case 'd':
                case 'i': {
                    if (lenMod === 'll' || lenMod === 'l') {
                        out += dv.getBigInt64(slot, true).toString();
                    } else {
                        out += dv.getInt32(slot, true).toString();
                    }
                    break;
                }
                case 'u': {
                    if (lenMod === 'll' || lenMod === 'l') {
                        out += dv.getBigUint64(slot, true).toString();
                    } else {
                        out += dv.getUint32(slot, true).toString();
                    }
                    break;
                }
                case 'x':
                case 'X': {
                    if (lenMod === 'll' || lenMod === 'l') {
                        out += dv.getBigUint64(slot, true).toString(16);
                    } else {
                        out += dv.getUint32(slot, true).toString(16);
                    }
                    break;
                }
                case 'f':
                case 'F':
                case 'e':
                case 'E':
                case 'g':
                case 'G': {
                    out += dv.getFloat64(slot, true).toFixed(1);
                    break;
                }
                case 'c': {
                    out += String.fromCharCode(dv.getInt32(slot, true));
                    break;
                }
                default:
                    out += '%' + conv;
            }
            i = j + 1;
        }
        process.stdout.write(out);
        return out.length;
    },
    putchar: function (c) {
        process.stdout.write(String.fromCharCode(c));
        return c;
    },
    exit: function (code) {
        process.exit(code >>> 0);
    },
    abort: function () {
        process.exit(134);
    },

    // ─── memory ───
    malloc: function (size) {
        // wasm32 i64 ABI: `size` arrives as BigInt. Convert to Number
        // for arithmetic; the module's heap is small enough that
        // Number precision is not a concern.
        const sz = typeof size === 'bigint' ? Number(size) : size;
        if (heapTop === 0) heapTop = HEAP_START;
        const ptr = heapTop;
        heapTop += (sz + 15) & ~15;
        const neededPages = Math.ceil(heapTop / 65536);
        const currentPages = linearMemory.buffer.byteLength / 65536;
        if (neededPages > currentPages) {
            const prevPages = linearMemory.grow(neededPages - currentPages);
            if (prevPages === -1) {
                // Out of memory. C's malloc returns 0 on failure;
                // match that rather than returning a pointer into
                // pages that were never allocated.
                return 0;
            }
        }
        return ptr;
    },
    free: function () {
        // Bump allocator — `free` is a no-op. Memory is reclaimed
        // only when the module exits. This is a deliberate
        // simplification for the test host; a real WASM runtime
        // would provide a full allocator.
    },

    // ─── math (LLVM lowerings use the C names) ───
    sqrt: Math.sqrt,
    sin: Math.sin,
    cos: Math.cos,
    tan: Math.tan,
    exp: Math.exp,
    log: Math.log,
    floor: Math.floor,
    ceil: Math.ceil,
    fabs: Math.abs,
    pow: Math.pow,

    // ─── string ───
    // `strlen` returns the number of UTF-8 bytes before the null
    // terminator, matching C semantics. Note: the ALGOL26
    // `String.length` builtin does NOT call this — it calls
    // `algol26_strlen_utf8`, which counts codepoints. This
    // implementation is only reachable from a future lowering that
    // explicitly asks for byte length.
    strlen: function (ptr) {
        const mem = new Uint8Array(linearMemory.buffer);
        let end = ptr;
        while (end < mem.length && mem[end] !== 0) end++;
        // wasm32 i64 return: must be BigInt.
        return BigInt(end - ptr);
    },
    // `algol26_strlen_utf8` counts Unicode codepoints: every byte
    // whose top two bits are not `10` (i.e. not a UTF-8
    // continuation byte) increments the count. Matches the emitted
    // LLVM helper and the interpreter's `str::chars().count()`.
    algol26_strlen_utf8: function (ptr) {
        const mem = new Uint8Array(linearMemory.buffer);
        let count = 0;
        let i = ptr;
        while (i < mem.length && mem[i] !== 0) {
            if ((mem[i] & 0xc0) !== 0x80) count++;
            i++;
        }
        // wasm32 i64 return: must be BigInt.
        return BigInt(count);
    },
    // algol26_string_split(s, sep, out_count) — matches Rust's
    // `str::split(sep)`. For non-empty `sep`, JS's String.split
    // matches Rust exactly (leading/trailing/double-sep all
    // preserved). For empty `sep`, both produce one element per
    // codepoint wrapped in leading and trailing empties.
    // Returns char** (array of u32 pointers); each element is
    // malloc'd. Writes element count through out_count.
    algol26_string_split: function (s_ptr, sep_ptr, out_count_ptr) {
        const s = readCString(s_ptr);
        const sep = readCString(sep_ptr);
        let parts;
        if (sep === '') {
            // Empty separator: Rust yields ["", c1, c2, ..., ""].
            parts = [''].concat(Array.from(s), ['']);
        } else {
            parts = s.split(sep);
        }
        const arrPtr = env.malloc(parts.length * 4);
        const mem = new Uint8Array(linearMemory.buffer);
        const u32 = new Uint32Array(linearMemory.buffer);
        for (let i = 0; i < parts.length; i++) {
            const str = parts[i];
            const len = Buffer.byteLength(str, 'utf8');
            const strPtr = env.malloc(len + 1);
            const buf = Buffer.from(str, 'utf8');
            mem.set(buf, strPtr);
            mem[strPtr + len] = 0;
            u32[(arrPtr >> 2) + i] = strPtr;
        }
        // Write int64_t count as two little-endian u32 words.
        // Counts here fit in u32; high word stays 0.
        u32[out_count_ptr >> 2] = parts.length;
        u32[(out_count_ptr >> 2) + 1] = 0;
        return arrPtr;
    },

    // algol26_string_join(items, count, sep) — matches Rust's
    // `slice::join(sep)`. `items` is char** (array of u32 ptrs),
    // `count` is int64_t (BigInt), `sep` is char*.
    algol26_string_join: function (items_ptr, count_bigint, sep_ptr) {
        const count = Number(count_bigint);
        const sep = readCString(sep_ptr);
        if (count <= 0) {
            // Empty list → empty string.
            const empty = env.malloc(1);
            new Uint8Array(linearMemory.buffer)[empty] = 0;
            return empty;
        }
        const u32 = new Uint32Array(linearMemory.buffer);
        const parts = [];
        for (let i = 0; i < count; i++) {
            const strPtr = u32[(items_ptr >> 2) + i];
            parts.push(strPtr ? readCString(strPtr) : '');
        }
        const joined = parts.join(sep);
        const len = Buffer.byteLength(joined, 'utf8');
        const outPtr = env.malloc(len + 1);
        const buf = Buffer.from(joined, 'utf8');
        const mem = new Uint8Array(linearMemory.buffer);
        mem.set(buf, outPtr);
        mem[outPtr + len] = 0;
        return outPtr;
    },

    // ── StringOps runtime ports ──
    //
    // These mirror the C implementations in runtime/string.c. They
    // read/write ALGOL26 strings through linear memory, use env.malloc
    // for allocation, and match the interpreter's semantics exactly
    // (codepoint-indexed, Unicode-aware).

    // Decode a single UTF-8 codepoint at a byte offset. Mirrors
    // runtime/string.c's `decode_utf8`. Returns {cp, adv} where
    // `adv` is the byte length (1..4). On malformed input, falls
    // through to a 1-byte advance with the raw byte as the cp.
    _decode_utf8: function (mem, ptr) {
        const b0 = mem[ptr];
        if (b0 < 0x80) return { cp: b0, adv: 1 };
        if ((b0 & 0xE0) === 0xC0 && mem[ptr + 1] !== 0) {
            return {
                cp: ((b0 & 0x1F) << 6) | (mem[ptr + 1] & 0x3F),
                adv: 2,
            };
        }
        if ((b0 & 0xF0) === 0xE0 && mem[ptr + 1] !== 0 && mem[ptr + 2] !== 0) {
            return {
                cp: ((b0 & 0x0F) << 12)
                    | ((mem[ptr + 1] & 0x3F) << 6)
                    | (mem[ptr + 2] & 0x3F),
                adv: 3,
            };
        }
        if ((b0 & 0xF8) === 0xF0
            && mem[ptr + 1] !== 0 && mem[ptr + 2] !== 0 && mem[ptr + 3] !== 0) {
            return {
                cp: ((b0 & 0x07) << 18)
                    | ((mem[ptr + 1] & 0x3F) << 12)
                    | ((mem[ptr + 2] & 0x3F) << 6)
                    | (mem[ptr + 3] & 0x3F),
                adv: 4,
            };
        }
        // Malformed — consume one byte, expose it raw.
        return { cp: b0, adv: 1 };
    },

    // Write a JS string to linear memory at `ptr`, UTF-8 encoded,
    // with a NUL terminator. Returns the number of bytes written
    // (excluding the terminator).
    _write_cstr: function (mem, ptr, str) {
        const buf = Buffer.from(str, 'utf8');
        mem.set(buf, ptr);
        mem[ptr + buf.length] = 0;
        return buf.length;
    },

    // Match runtime/string.c's is_unicode_whitespace.
    _is_unicode_whitespace: function (cp) {
        return (cp >= 0x09 && cp <= 0x0D)
            || cp === 0x20
            || cp === 0x85
            || cp === 0xA0
            || cp === 0x1680
            || (cp >= 0x2000 && cp <= 0x200A)
            || cp === 0x2028
            || cp === 0x2029
            || cp === 0x202F
            || cp === 0x205F
            || cp === 0x3000;
    },

    // Encode one codepoint as UTF-8. Mirrors algol26_char_to_string.
    _encode_utf8: function (cp) {
        if (cp > 0x10FFFF || (cp >= 0xD800 && cp <= 0xDFFF)) {
            // Replacement character.
            return Buffer.from([0xEF, 0xBF, 0xBD]);
        }
        if (cp < 0x80) return Buffer.from([cp]);
        if (cp < 0x800) {
            return Buffer.from([0xC0 | (cp >> 6), 0x80 | (cp & 0x3F)]);
        }
        if (cp < 0x10000) {
            return Buffer.from([
                0xE0 | (cp >> 12),
                0x80 | ((cp >> 6) & 0x3F),
                0x80 | (cp & 0x3F),
            ]);
        }
        return Buffer.from([
            0xF0 | (cp >> 18),
            0x80 | ((cp >> 12) & 0x3F),
            0x80 | ((cp >> 6) & 0x3F),
            0x80 | (cp & 0x3F),
        ]);
    },

    // algol26_string_substring(s, start, length) → malloc'd char*.
    // Codepoint-indexed. Clamps out-of-range to "". start/length
    // arrive as BigInt (i64).
    algol26_string_substring: function (s_ptr, start_bigint, length_bigint) {
        const s = readCString(s_ptr);
        let start = Number(start_bigint);
        let length = Number(length_bigint);
        if (start < 0) start = 0;
        if (length < 0) length = 0;
        const cps = Array.from(s);
        const slice = cps.slice(start, start + length).join('');
        const len = Buffer.byteLength(slice, 'utf8');
        const outPtr = env.malloc(len + 1);
        const mem = new Uint8Array(linearMemory.buffer);
        env._write_cstr(mem, outPtr, slice);
        return outPtr;
    },

    // algol26_string_char_at(s, i) → malloc'd 1-cp string | NULL.
    // i arrives as BigInt.
    algol26_string_char_at: function (s_ptr, i_bigint) {
        const s = readCString(s_ptr);
        const i = Number(i_bigint);
        if (i < 0) return 0;
        const cps = Array.from(s);
        if (i >= cps.length) return 0;
        const ch = cps[i];
        const len = Buffer.byteLength(ch, 'utf8');
        const outPtr = env.malloc(len + 1);
        const mem = new Uint8Array(linearMemory.buffer);
        env._write_cstr(mem, outPtr, ch);
        return outPtr;
    },

    // algol26_string_trim(s) → malloc'd char* (Unicode-aware).
    algol26_string_trim: function (s_ptr) {
        const s = readCString(s_ptr);
        // JS's String.prototype.trim handles the Unicode White_Space
        // set (includes U+FEFF, which Rust does not — but Rust's
        // char::is_whitespace does not include U+FEFF either, so
        // we handle it explicitly).
        const trimmed = Array.from(s)
            .filter(c => true) // no-op, keeps codepoint array
            .join('');
        // Manual trim to match runtime/string.c exactly:
        const cps = Array.from(trimmed);
        let start = 0;
        while (start < cps.length
               && env._is_unicode_whitespace(cps[start].codePointAt(0))) {
            start++;
        }
        if (start === cps.length) {
            const empty = env.malloc(1);
            new Uint8Array(linearMemory.buffer)[empty] = 0;
            return empty;
        }
        let end = start;
        for (let j = start; j < cps.length; j++) {
            if (!env._is_unicode_whitespace(cps[j].codePointAt(0))) end = j + 1;
        }
        const result = cps.slice(start, end).join('');
        const len = Buffer.byteLength(result, 'utf8');
        const outPtr = env.malloc(len + 1);
        const mem = new Uint8Array(linearMemory.buffer);
        env._write_cstr(mem, outPtr, result);
        return outPtr;
    },

    // algol26_char_to_string(cp) → malloc'd 1-cp UTF-8 string.
    // cp arrives as i32 (Number).
    algol26_char_to_string: function (cp) {
        const buf = env._encode_utf8(cp);
        const outPtr = env.malloc(buf.length + 1);
        const mem = new Uint8Array(linearMemory.buffer);
        mem.set(buf, outPtr);
        mem[outPtr + buf.length] = 0;
        return outPtr;
    },

    // algol26_string_char_at_cp(s, i, out_u32_ptr) → i32 (0 | 1).
    // i arrives as BigInt. Writes the codepoint through out_ptr.
    algol26_string_char_at_cp: function (s_ptr, i_bigint, out_ptr) {
        const s = readCString(s_ptr);
        const i = Number(i_bigint);
        if (i < 0) return 0;
        const cps = Array.from(s);
        if (i >= cps.length) return 0;
        const u32 = new Uint32Array(linearMemory.buffer);
        u32[out_ptr >> 2] = cps[i].codePointAt(0);
        return 1;
    },

    // algol26_string_to_chars(s, out_count_i64_ptr) → malloc'd i32 array | NULL.
    // Empty string → NULL + out_count = 0.
    algol26_string_to_chars: function (s_ptr, out_count_ptr) {
        const s = readCString(s_ptr);
        const cps = Array.from(s);
        const u32 = new Uint32Array(linearMemory.buffer);
        if (cps.length === 0) {
            // out_count = 0 (two u32 words, both zero).
            u32[out_count_ptr >> 2] = 0;
            u32[(out_count_ptr >> 2) + 1] = 0;
            return 0;
        }
        const bufPtr = env.malloc(cps.length * 4);
        for (let i = 0; i < cps.length; i++) {
            u32[(bufPtr >> 2) + i] = cps[i].codePointAt(0);
        }
        // Write int64_t count (two u32 words, low + high).
        u32[out_count_ptr >> 2] = cps.length;
        u32[(out_count_ptr >> 2) + 1] = 0;
        return bufPtr;
    },

    // ── Command-line args ──
    //
    // process.argv = [node, host.js, module.wasm, userArg1, ...]
    // so slice(3) is what the program sees, matching the
    // interpreter's std::env::args().skip(1) plus the host's own
    // two argv entries (node executable, host.js script).
    //
    // algol26_args_init is a no-op under WASM: the host already
    // has the argument vector in process.argv, and unlike the C
    // runtime the module can't be re-invoked with different args.
    // The generated main wrapper still calls this with (argc, argv)
    // from the WASM main signature — we ignore those. _count and
    // _get read process.argv directly.
    algol26_args_init: function (argc, argv) {
        // No-op; see comment above.
    },

    algol26_args_count: function () {
        const userArgs = process.argv.slice(3);
        // wasm32 i64 return: must be BigInt.
        return BigInt(userArgs.length);
    },

    algol26_args_get: function (i) {
        const userArgs = process.argv.slice(3);
        const idx = Number(i);
        if (idx < 0 || idx >= userArgs.length) return 0;   // NULL
        const arg = userArgs[idx];
        const len = Buffer.byteLength(arg, 'utf8');
        const ptr = env.malloc(len + 1);
        const mem = new Uint8Array(linearMemory.buffer);
        mem.set(Buffer.from(arg, 'utf8'), ptr);
        mem[ptr + len] = 0;
        return ptr;
    },

    strcmp: function (a, b) {
        const sa = readCString(a);
        const sb = readCString(b);
        return sa < sb ? -1 : sa > sb ? 1 : 0;
    },
    toupper: function (c) {
        // ASCII-only. Matches the LLVM codegen's dependency on libc
        // toupper, which the wasm32 build will resolve to this shim.
        return (c >= 97 && c <= 122) ? c - 32 : c;
    },
    tolower: function (c) {
        return (c >= 65 && c <= 90) ? c + 32 : c;
    },
    memcpy: function (dst, src, n) {
        const mem = new Uint8Array(linearMemory.buffer);
        for (let i = 0; i < n; i++) {
            mem[dst + i] = mem[src + i];
        }
        return dst;
    },
    strcpy: function (dst, src) {
        const mem = new Uint8Array(linearMemory.buffer);
        let i = 0;
        while (mem[src + i] !== 0) {
            mem[dst + i] = mem[src + i];
            i++;
        }
        mem[dst + i] = 0;
        return dst;
    },
    strcat: function (dst, src) {
        const s = readCString(src);
        const mem = new Uint8Array(linearMemory.buffer);
        let d = dst;
        while (mem[d] !== 0) d++;
        for (let i = 0; i < s.length; i++) mem[d + i] = s.charCodeAt(i);
        mem[d + s.length] = 0;
        return dst;
    },
};

async function run() {
    const imports = WebAssembly.Module.imports(wasmModule);
    const envNames = new Set(
        imports.filter(i => i.module === 'env').map(i => i.name)
    );

    const provided = new Set(Object.keys(env));
    const missing = [...envNames].filter(n => !provided.has(n));
    if (missing.length > 0) {
        console.error(
            `[wasm host] unresolved imports: ${missing.join(', ')}`
        );
        console.error(
            `[wasm host] add implementations to runtime/wasm/host.js`
        );
        process.exit(1);
    }

    const instance = await WebAssembly.instantiate(wasmModule, { env });

    if (!instance.exports.memory) {
        console.error('[wasm host] module does not export memory');
        process.exit(1);
    }
    linearMemory = instance.exports.memory;

    if (typeof instance.exports.main !== 'function') {
        console.error('[wasm host] module does not export main');
        process.exit(1);
    }

    const result = instance.exports.main();
    // If `main` declares a non-Void return type (e.g.
    // `function main() -> Int`), propagate a non-zero value to the
    // shell exit code. `procedure main` returns undefined, which is
    // treated as success.
    if (typeof result === 'number' && result !== 0) {
        process.exit(result);
    }
}

run().catch(e => {
    console.error('[wasm host]', e);
    process.exit(1);
});