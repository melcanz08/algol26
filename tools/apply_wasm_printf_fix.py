#!/usr/bin/env python3
"""
Fix the printf shim in runtime/wasm/host.js: parse the format string
and consume the varargs that WASM passes as additional JS args.
"""

import sys
from pathlib import Path

OLD = '''    printf: function (fmtPtr) {
        const s = readCString(fmtPtr);
        // We cannot resolve C varargs in WASM — the ABI does not
        // carry them across the import boundary. The compiler
        // currently emits `printf("%lld\\n", v)` style calls for
        // non-string prints, which we degrade here by replacing
        // format specifiers with `?`. String prints work exactly.
        // Documented in IMPLEMENTATION_STATUS.md.
        if (s.includes('%')) {
            process.stdout.write(s.replace(/%[a-zA-Z]+/g, '?'));
        } else {
            process.stdout.write(s);
        }
        return s.length;
    },'''

NEW = '''    printf: function (fmtPtr, ...args) {
        const fmt = readCString(fmtPtr);
        let out = '';
        let argIdx = 0;
        let i = 0;
        while (i < fmt.length) {
            const ch = fmt[i];
            if (ch !== '%') {
                out += ch;
                i++;
                continue;
            }
            // Literal `%%` -> `%`
            if (fmt[i + 1] === '%') {
                out += '%';
                i += 2;
                continue;
            }
            // Scan flags / width / precision / length modifiers
            // (including `l` and `ll` for `%lld`).
            let j = i + 1;
            while (j < fmt.length && /[-+ 0#0-9.*lh]/.test(fmt[j])) j++;
            if (j >= fmt.length) {
                out += fmt.slice(i);
                break;
            }
            const conv = fmt[j];
            if (argIdx >= args.length) {
                out += '?';
                i = j + 1;
                continue;
            }
            const v = args[argIdx++];
            switch (conv) {
                case 's':
                    out += readCString(Number(v));
                    break;
                case 'd':
                case 'i':
                    out += (typeof v === 'bigint'
                        ? v
                        : BigInt(v | 0)).toString();
                    break;
                case 'u':
                    out += (typeof v === 'bigint'
                        ? v
                        : BigInt(v >>> 0)).toString();
                    break;
                case 'x':
                case 'X':
                    out += (typeof v === 'bigint'
                        ? v
                        : BigInt(v >>> 0)).toString(16);
                    break;
                case 'f':
                case 'F':
                case 'e':
                case 'E':
                case 'g':
                case 'G':
                    out += Number(v).toFixed(1);
                    break;
                case 'c':
                    out += String.fromCharCode(Number(v));
                    break;
                case 'p':
                    out += '0x' + Number(v).toString(16);
                    break;
                default:
                    out += '%' + conv;
            }
            i = j + 1;
        }
        process.stdout.write(out);
        return out.length;
    },'''


def main():
    repo = Path.cwd()
    host = repo / "runtime" / "wasm" / "host.js"
    if not host.exists():
        print(f"ERROR: {host} not found", file=sys.stderr)
        return 1

    text = host.read_text()
    if "Scan flags / width / precision" in text:
        print("  skipped: printf already upgraded")
        return 0

    if OLD not in text:
        print(
            "ERROR: printf anchor not found in host.js. Paste the\n"
            "  current printf block and I'll adjust.",
            file=sys.stderr,
        )
        return 1

    host.write_text(text.replace(OLD, NEW, 1))
    print("  patched: runtime/wasm/host.js")
    return 0


if __name__ == "__main__":
    sys.exit(main())