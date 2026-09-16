#!/usr/bin/env python3
"""
Fix the printf shim to handle wasm32 C varargs semantics.

The wasm32 ABI passes variadic args by reference: each arg is a
pointer to a memory slot holding the actual value. The shim must
dereference per the format specifier's type:

  %s         char*     -- slot holds a 4-byte string pointer
  %lld / %ld int64     -- slot holds an 8-byte signed integer
  %.1f       double    -- slot holds an 8-byte float
  (etc.)

The fixed-arg format string, by contrast, is passed directly (not
through a slot), which is why fmtPtr was already correct.
"""

import re
import sys
from pathlib import Path

NEW_PRINTF = '''    printf: function (fmtPtr) {
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
'''


def main():
    repo = Path.cwd()
    host = repo / "runtime" / "wasm" / "host.js"
    if not host.exists():
        print(f"ERROR: {host} not found", file=sys.stderr)
        return 1

    text = host.read_text()

    if "variadic args are passed BY REFERENCE" in text:
        print("  skipped: printf already fixed")
        return 0

    # Match from the current printf block through the next method.
    pattern = re.compile(
        r"    printf: function[\s\S]*?(?=\n    putchar: function)",
        re.MULTILINE,
    )
    if not pattern.search(text):
        print(
            "ERROR: printf block not found. The host.js may have been\n"
            "  edited. Paste the whole file and I'll adjust.",
            file=sys.stderr,
        )
        return 1

    new_text = pattern.sub(NEW_PRINTF.rstrip("\n"), text, count=1)
    if new_text == text:
        print("ERROR: replacement made no change", file=sys.stderr)
        return 1

    host.write_text(new_text)
    print("  patched: runtime/wasm/host.js (printf now dereferences varargs)")
    return 0


if __name__ == "__main__":
    sys.exit(main())