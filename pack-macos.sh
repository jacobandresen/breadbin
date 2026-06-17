#!/usr/bin/env bash
#
# pack-macos.sh
#
# Build breadbin and package it as a double-clickable macOS application
# (breadbin.app) plus a drag-to-Applications disk image (breadbin.dmg).
#
# The .app bundles the compiled `breadbin` binary and a tiny launcher: double
# -clicking it opens the cover-art kiosk in WezTerm (which renders the inline box
# art), falling back to Terminal.app + the text menu when WezTerm isn't installed.
#
# Usage:
#   ./pack-macos.sh            # build release binary, make dist/breadbin.app + .dmg
#   ./pack-macos.sh --no-dmg   # just the .app
#
set -euo pipefail

cd "$(dirname "$0")"
REPO="$PWD"
RUST_DIR="$REPO/rust"
DIST="$REPO/dist"

APP_NAME="breadbin"
BUNDLE_ID="com.jacobandresen.breadbin"
APP="$DIST/$APP_NAME.app"
MAKE_DMG=1
[ "${1:-}" = "--no-dmg" ] && MAKE_DMG=0

log() { printf '\033[1;32m==>\033[0m %s\n' "$*"; }
die() { printf '\033[1;31mError:\033[0m %s\n' "$*" >&2; exit 1; }

[ "$(uname -s)" = "Darwin" ] || die "this packager only runs on macOS"
command -v cargo >/dev/null 2>&1 || die "cargo not found — install Rust (https://rustup.rs)"

# Version from Cargo.toml ([package] version = "x.y.z")
VERSION="$(awk -F'"' '/^version[[:space:]]*=/{print $2; exit}' "$RUST_DIR/Cargo.toml")"
VERSION="${VERSION:-0.0.0}"

log "Building release binary (v$VERSION) ..."
( cd "$RUST_DIR" && cargo build --release )
BIN="$RUST_DIR/target/release/$APP_NAME"
[ -x "$BIN" ] || die "expected binary at $BIN"

log "Assembling $APP_NAME.app ..."
rm -rf "$APP"
mkdir -p "$APP/Contents/MacOS" "$APP/Contents/Resources"

# The real CLI lives in Resources; the bundle's executable is the launcher below.
cp "$BIN" "$APP/Contents/Resources/$APP_NAME"
chmod +x "$APP/Contents/Resources/$APP_NAME"

cat > "$APP/Contents/Info.plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>CFBundleName</key>            <string>$APP_NAME</string>
    <key>CFBundleDisplayName</key>     <string>breadbin</string>
    <key>CFBundleIdentifier</key>      <string>$BUNDLE_ID</string>
    <key>CFBundleVersion</key>         <string>$VERSION</string>
    <key>CFBundleShortVersionString</key><string>$VERSION</string>
    <key>CFBundlePackageType</key>     <string>APPL</string>
    <key>CFBundleExecutable</key>      <string>$APP_NAME-launch</string>
    <key>CFBundleIconFile</key>        <string>breadbin.icns</string>
    <key>LSMinimumSystemVersion</key>  <string>11.0</string>
    <key>NSHighResolutionCapable</key> <true/>
</dict>
</plist>
PLIST

# Launcher: open the kiosk in WezTerm (inline cover art); else Terminal + menu.
cat > "$APP/Contents/MacOS/$APP_NAME-launch" <<'LAUNCH'
#!/bin/bash
HERE="$(cd "$(dirname "$0")" && pwd)"
BIN="$HERE/../Resources/breadbin"

WT="/Applications/WezTerm.app/Contents/MacOS/wezterm"
[ -x "$WT" ] || WT="$(command -v wezterm 2>/dev/null || true)"

if [ -n "$WT" ] && [ -x "$WT" ]; then
    exec "$WT" start --always-new-process -- "$BIN" kiosk
fi

# No WezTerm: the text menu works in any terminal (covers just won't render).
exec /usr/bin/osascript \
    -e "tell application \"Terminal\" to do script \"clear; '$BIN' menu; exit\"" \
    -e 'tell application "Terminal" to activate'
LAUNCH
chmod +x "$APP/Contents/MacOS/$APP_NAME-launch"

# Optional icon: drop a breadbin.icns next to this script to brand the app.
if [ -f "$REPO/breadbin.icns" ]; then
    cp "$REPO/breadbin.icns" "$APP/Contents/Resources/breadbin.icns"
fi

# Ad-hoc sign so Gatekeeper doesn't flag the unsigned bundle as "damaged"
# (especially on Apple Silicon). Replace "-" with a Developer ID to distribute.
log "Ad-hoc signing ..."
codesign --force --deep --sign - "$APP" >/dev/null 2>&1 || \
    log "  (codesign unavailable; the app is unsigned)"

log "Built $APP"

if [ "$MAKE_DMG" = 1 ]; then
    command -v hdiutil >/dev/null 2>&1 || die "hdiutil not found (needed for the .dmg)"
    log "Building $APP_NAME.dmg ..."
    STAGE="$(mktemp -d)"
    cp -R "$APP" "$STAGE/"
    ln -s /Applications "$STAGE/Applications"   # drag-to-install target
    DMG="$DIST/$APP_NAME.dmg"
    rm -f "$DMG"
    hdiutil create -volname "$APP_NAME" -srcfolder "$STAGE" -ov -format UDZO "$DMG" >/dev/null
    rm -rf "$STAGE"
    log "Built $DMG"
fi

log "Done. Install by dragging $APP_NAME.app into /Applications (or open the .dmg)."
