#!/bin/bash

# ALGOL26 Compiler Wrapper
ALGOL26_DIR="$HOME/dev/algol26"

# If invoked inside the repo, use the repo itself.
if [ -f "Cargo.toml" ] && [ -f "src/main.rs" ]; then
    ALGOL26_DIR="$(pwd)"
fi

RELEASE="$ALGOL26_DIR/target/release/algol26"
DEBUG="$ALGOL26_DIR/target/debug/algol26"

# Pick the freshest built binary. `-nt` means "newer than".
if [ -f "$RELEASE" ] && [ -f "$DEBUG" ]; then
    if [ "$DEBUG" -nt "$RELEASE" ]; then
        COMPILER="$DEBUG"
    else
        COMPILER="$RELEASE"
    fi
elif [ -f "$RELEASE" ]; then
    COMPILER="$RELEASE"
elif [ -f "$DEBUG" ]; then
    COMPILER="$DEBUG"
else
    echo "Building ALGOL26 compiler (first run)..."
    (cd "$ALGOL26_DIR" && cargo build --release)
    if [ $? -ne 0 ]; then
        echo "Build failed."
        exit 1
    fi
    COMPILER="$RELEASE"
fi

# Forward everything to the real binary. No hardcoded help, no
# filename check — the binary knows its own subcommands and
# produces better error messages than the wrapper could.
exec "$COMPILER" "$@"