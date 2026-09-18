#!/usr/bin/env python3
"""
WASM host shim + linking.

- wasm_backend.rs: after writing the LLVM relocatable object to
  `<out>.wasm.o`, invoke wasm-ld to link it into a runnable
  `<out>.wasm` with `main` and `memory` exported.
- Creates runtime/wasm/host.js — a Node shim providing the C library
  symbols the module imports (printf, exit, malloc, free, math, strlen,
  strcmp, strcat).
- Creates runtime/wasm/run.sh — compile + run wrapper.
"""

import os
import stat
import sys
from pathlib import Path


OLD_WASM = """        let wasm_path = format!("{}.wasm", output_name);
        machine
            .write_to_file(
                &codegen.module,
                FileType::Object,
                std::path::Path::new(&wasm_path),
            )
            .map_err(|e| {
                CompileError::simple(
                    &format!("Failed to write WASM to {}: {}", wasm_path, e),
                    0,
                    0,
                    "",
                    ErrorCode::E0001,
                )
            })?;

        println!("[Generated WASM: {}]", wasm_path);

        Ok(BackendOutput::WasmModule {
            path: std::path::PathBuf::from(wasm_path),
        })"""

NEW_WASM = '''        // wasm-ld expects a relocatable object, so write LLVM's
        // output to a `.wasm.o` intermediate first, then link it
        // into a runnable module.
        let obj_path = format!("{}.wasm.o", output_name);
        machine
            .write_to_file(
                &codegen.module,
                FileType::Object,
                std::path::Path::new(&obj_path),
            )
            .map_err(|e| {
                CompileError::simple(
                    &format!("Failed to write WASM object to {}: {}", obj_path, e),
                    0,
                    0,
                    "",
                    ErrorCode::E0001,
                )
            })?;

        // Link the object into a runnable module. Undefined
        // C-library symbols (printf, exit, malloc, free, sqrt,
        // strlen, ...) become imports from `env`; the Node host
        // in `runtime/wasm/host.js` provides them.
        //   --no-entry        : no CLI `_start` entry; host calls main
        //   --allow-undefined : unresolved symbols become imports
        //   --export=main     : expose main to the host
        //   --export=memory   : expose linear memory to the host
        let wasm_path = format!("{}.wasm", output_name);
        let link = std::process::Command::new("wasm-ld")
            .arg("--no-entry")
            .arg("--allow-undefined")
            .arg("--export=main")
            .arg("--export=memory")
            .arg("-o")
            .arg(&wasm_path)
            .arg(&obj_path)
            .output()
            .map_err(|e| {
                CompileError::simple(
                    &format!(
                        "Failed to run wasm-ld: {} \\
                         (install with `sudo apt install lld`)",
                        e
                    ),
                    0,
                    0,
                    "",
                    ErrorCode::E0001,
                )
            })?;

        if !link.status.success() {
            return Err(CompileError::simple(
                &format!(
                    "wasm-ld linking failed:\\n{}",
                    String::from_utf8_lossy(&link.stderr)
                ),
                0,
                0,
                "",
                ErrorCode::E0001,
            ));
        }

        let _ = std::fs::remove_file(&obj_path);

        println!("[Generated WASM: {}]", wasm_path);

        Ok(BackendOutput::WasmModule {
            path: std::path::PathBuf::from(wasm_path),
        })'''

HOST_JS = r'''#!/usr/bin/env node
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

    const { instance } = await WebAssembly.instantiate(wasmModule, { env });

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
'''

RUN_SH = r'''#!/usr/bin/env bash
#
# runtime/wasm/run.sh — compile an ALGOL26 program to WASM and run
# it under the Node host shim.
#
# Usage:
#   runtime/wasm/run.sh path/to/program.gol

set -euo pipefail

if [ $# -lt 1 ]; then
    echo "usage: $0 <program.gol>" >&2
    exit 1
fi

SRC="$1"
OUT="${SRC%.gol}"
ROOT="$(cd "$(dirname "$0")/../.." && pwd)"

"$ROOT/target/debug/algol26" wasm "$SRC"
exec node "$ROOT/runtime/wasm/host.js" "${OUT}.wasm"
'''


def main():
    repo = Path.cwd()
    if not (repo / "Cargo.toml").exists():
        print("ERROR: run from repo root", file=sys.stderr)
        return 1

    # 1. Patch wasm_backend.rs
    wasm_path = repo / "src" / "backends" / "wasm_backend.rs"
    if not wasm_path.exists():
        print(f"ERROR: {wasm_path} not found", file=sys.stderr)
        return 1
    text = wasm_path.read_text()
    if "wasm-ld" in text:
        print("  skipped: wasm_backend.rs already links")
    elif OLD_WASM not in text:
        print(
            "ERROR: wasm_backend.rs anchor not found. The file may have\n"
            "  drifted since the last paste. Paste the tail of\n"
            "  `fn compile` and I'll adjust the script.",
            file=sys.stderr,
        )
        return 1
    else:
        text = text.replace(OLD_WASM, NEW_WASM, 1)
        wasm_path.write_text(text)
        print("  patched: src/backends/wasm_backend.rs")

    # 2. Write runtime/wasm/host.js
    wasm_dir = repo / "runtime" / "wasm"
    wasm_dir.mkdir(parents=True, exist_ok=True)
    host = wasm_dir / "host.js"
    host.write_text(HOST_JS)
    print("  wrote: runtime/wasm/host.js")

    # 3. Write runtime/wasm/run.sh + chmod +x
    run = wasm_dir / "run.sh"
    run.write_text(RUN_SH)
    os.chmod(run, os.stat(run).st_mode | stat.S_IXUSR | stat.S_IXGRP | stat.S_IXOTH)
    print("  wrote: runtime/wasm/run.sh (executable)")

    return 0


if __name__ == "__main__":
    sys.exit(main())