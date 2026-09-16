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
    // ─── stdio ───
    printf: function (fmtPtr) {
        // wasm32 C ABI: variadic args are passed BY REFERENCE — each
        // argument is a pointer to a memory slot holding the value.
        // The format string itself is a fixed arg and is passed
        // directly. We dereference each vararg according to the type
        // the format specifier asks for.
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
        if (heapTop === 0) heapTop = HEAP_START;
        const ptr = heapTop;
        heapTop += (size + 15) & ~15;
        const neededPages = Math.ceil(heapTop / 65536);
        const currentPages = linearMemory.buffer.byteLength / 65536;
        if (neededPages > currentPages) {
            linearMemory.grow(neededPages - currentPages);
        }
        return ptr;
    },
    free: function () {
        // Bump allocator — no-op.
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
    strlen: function (ptr) {
        return readCString(ptr).length;
    },
    strcmp: function (a, b) {
        const sa = readCString(a);
        const sb = readCString(b);
        return sa < sb ? -1 : sa > sb ? 1 : 0;
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

    instance.exports.main();
}

run().catch(e => {
    console.error('[wasm host]', e);
    process.exit(1);
});
