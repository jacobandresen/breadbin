#!/usr/bin/env bash
set -euo pipefail

REPO="$(cd "$(dirname "$0")" && pwd)"
SCHEMA_DIR="$REPO/crates/breadbin-gui/data"

# Compile the GSettings schema if missing or stale.
if [ ! -f "$SCHEMA_DIR/gschemas.compiled" ] || \
   [ "$SCHEMA_DIR/io.github.jacobandresen.Breadbin.gschema.xml" -nt "$SCHEMA_DIR/gschemas.compiled" ]; then
    glib-compile-schemas "$SCHEMA_DIR"
fi

# Prefer a release build; fall back to debug.
BINARY="$REPO/target/release/breadbin"
if [ ! -f "$BINARY" ]; then
    BINARY="$REPO/target/debug/breadbin"
fi

if [ ! -f "$BINARY" ]; then
    echo "No binary found. Run 'cargo build' or 'cargo build --release' first." >&2
    exit 1
fi

exec env GSETTINGS_SCHEMA_DIR="$SCHEMA_DIR" "$BINARY" "$@"
