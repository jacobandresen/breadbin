#!/usr/bin/env bash
# install.sh — build Breadbin as the current user, then install system-wide.
#
# Usage (two-step so cargo runs as you, not root):
#   packaging/install.sh build    # cargo build --release
#   sudo packaging/install.sh     # install to /usr (requires root)
#
# Or in one shot without sudo (installs to ~/.local instead):
#   PREFIX=~/.local packaging/install.sh
set -euo pipefail

REPO="$(cd "$(dirname "$0")/.." && pwd)"
PREFIX="${PREFIX:-/usr}"

# ── Build (must run as the user who owns the Rust toolchain) ─────────────────
if [ "${1:-}" = "build" ]; then
    echo "Building breadbin (release)..."
    cd "$REPO"
    cargo build --release
    echo "Build complete. Now run:  sudo packaging/install.sh"
    exit 0
fi

# ── Install (writes to $PREFIX — needs root for /usr) ────────────────────────
if [ ! -f "$REPO/target/release/breadbin" ]; then
    echo "Binary not found. Build first:  packaging/install.sh build" >&2
    exit 1
fi

echo "Installing to $PREFIX..."

install -Dm755 "$REPO/target/release/breadbin" \
    "$PREFIX/bin/breadbin"

install -Dm644 "$REPO/crates/breadbin-gui/data/io.github.jacobandresen.Breadbin.gschema.xml" \
    "$PREFIX/share/glib-2.0/schemas/io.github.jacobandresen.Breadbin.gschema.xml"

install -Dm644 "$REPO/packaging/breadbin.desktop" \
    "$PREFIX/share/applications/breadbin.desktop"

install -Dm644 "$REPO/packaging/breadbin.svg" \
    "$PREFIX/share/icons/hicolor/scalable/apps/breadbin.svg"

echo "Running post-install hooks..."
glib-compile-schemas "$PREFIX/share/glib-2.0/schemas"
gtk-update-icon-cache -q -t -f "$PREFIX/share/icons/hicolor" 2>/dev/null || true
update-desktop-database -q "$PREFIX/share/applications" 2>/dev/null || true

echo "Done. Launch with:  breadbin"
