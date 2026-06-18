#!/usr/bin/env bash
#
# setup-dependencies.sh
#
# Installs the project's dependencies:
#   - Rust     (to build the `breadbin` binary; rustup.rs gives the newest toolchain
#               if your distro's packaged Rust is older than the crate needs)
#   - VICE     (Commodore emulator)
#   - WezTerm  (terminal emulator)
#
# Supported platforms:
#   - macOS        (Homebrew)
#   - Arch Linux   (pacman)
#   - Ubuntu/Debian (apt)
#
set -euo pipefail

log()  { printf '\033[1;32m==>\033[0m %s\n' "$*"; }
warn() { printf '\033[1;33m==>\033[0m %s\n' "$*" >&2; }
die()  { printf '\033[1;31mError:\033[0m %s\n' "$*" >&2; exit 1; }

have() { command -v "$1" >/dev/null 2>&1; }

# Download Hack Nerd Font into the user's font dir (Linux). Nerd Fonts are not
# packaged by apt, and the symbol glyphs breadbin's TUI uses (⏎ ↵ ⬇ ♪ ★ ⚠ ▶)
# are missing from stock DejaVu/Noto -- a Nerd Font supplies them all.
install_nerd_font() {
    if fc-list 2>/dev/null | grep -qi "Hack Nerd Font"; then
        log "Hack Nerd Font already installed; skipping."
        return
    fi
    local dir url tmp
    dir="${HOME}/.local/share/fonts"
    url="https://github.com/ryanoasis/nerd-fonts/releases/latest/download/Hack.zip"
    mkdir -p "$dir"
    tmp="$(mktemp -d)"
    log "Downloading Hack Nerd Font..."
    if curl -fL "$url" -o "$tmp/Hack.zip"; then
        unzip -oq "$tmp/Hack.zip" -d "$dir"
    else
        warn "Could not download Hack Nerd Font from $url"
        warn "Install a Nerd Font manually or the TUI's symbol glyphs may not render."
    fi
    rm -rf "$tmp"
}

install_macos() {
    have brew || die "Homebrew is required. Install it from https://brew.sh and re-run."

    log "Updating Homebrew..."
    brew update

    log "Installing Rust..."
    brew install rust

    log "Installing VICE..."
    brew install --cask vice

    log "Installing WezTerm..."
    brew install --cask wezterm
}

install_arch() {
    have pacman || die "pacman not found; this is not an Arch-based system."

    log "Refreshing package databases and installing Rust, VICE and WezTerm..."
    sudo pacman -Syu --needed --noconfirm rust vice wezterm

    log "Installing fonts for the TUI's box-drawing and symbol glyphs..."
    # breadbin's TUI draws arrows, triangles, blocks, ★/♪/⚠/⏎/↵ and U+2B07 (⬇).
    # ttf-nerd-fonts-symbols-mono (official repo) supplies the symbol glyphs that
    # DejaVu/Noto lack (⏎ U+23CE, ⬇ U+2B07, ↵ U+21B5, ♪ U+266A) in monospace cells.
    sudo pacman -S --needed --noconfirm ttf-dejavu noto-fonts ttf-nerd-fonts-symbols-mono
}

install_ubuntu() {
    have apt-get || die "apt-get not found; this is not a Debian/Ubuntu system."

    log "Updating package lists..."
    sudo apt-get update

    log "Installing Rust (cargo)..."
    # Debian/Ubuntu's packaged cargo can lag the crate's minimum; if `cargo build`
    # later complains it's too old, install the current toolchain from https://rustup.rs
    sudo apt-get install -y cargo

    log "Installing VICE..."
    sudo apt-get install -y vice

    log "Installing fonts for the TUI's box-drawing and symbol glyphs..."
    # breadbin's TUI draws arrows, triangles, blocks, ★/♪/⚠/⏎/↵ and U+2B07 (⬇).
    # Several of these (⏎ U+23CE, ⬇ U+2B07, ↵ U+21B5, ♪ U+266A) are absent from
    # DejaVu/Noto, so a plain monospace font renders them as tofu. The reliable
    # fix is a Nerd Font, which packs every one of these into proper monospace
    # cells -- and it's what the recommended WezTerm config falls back to. Apt
    # has no Nerd Font, so fetch Hack Nerd Font from the nerd-fonts release.
    sudo apt-get install -y fonts-dejavu-core fonts-noto-core fonts-noto-mono curl unzip
    install_nerd_font
    fc-cache -f || true

    log "Installing WezTerm..."
    # WezTerm is not in the default Ubuntu repositories, so use the
    # official flatpak-free APT repository published by the WezTerm project.
    if ! have wezterm; then
        sudo apt-get install -y curl gnupg
        curl -fsSL https://apt.fury.io/wez/gpg.key \
            | sudo gpg --yes --dearmor -o /usr/share/keyrings/wezterm-fury.gpg
        echo 'deb [signed-by=/usr/share/keyrings/wezterm-fury.gpg] https://apt.fury.io/wez/ * *' \
            | sudo tee /etc/apt/sources.list.d/wezterm.list >/dev/null
        sudo apt-get update
        sudo apt-get install -y wezterm
    else
        log "WezTerm already installed; skipping."
    fi
}

detect_and_install() {
    local os
    os="$(uname -s)"

    case "$os" in
        Darwin)
            log "Detected macOS."
            install_macos
            ;;
        Linux)
            if [ -r /etc/os-release ]; then
                # shellcheck disable=SC1091
                . /etc/os-release
            fi
            case "${ID:-}${ID_LIKE:-}" in
                *arch*)
                    log "Detected Arch Linux."
                    install_arch
                    ;;
                *ubuntu*|*debian*)
                    log "Detected Ubuntu/Debian."
                    install_ubuntu
                    ;;
                *)
                    # Fall back to whichever package manager is present.
                    if have pacman; then
                        log "Detected pacman-based system."
                        install_arch
                    elif have apt-get; then
                        log "Detected apt-based system."
                        install_ubuntu
                    else
                        die "Unsupported Linux distribution: ${ID:-unknown}"
                    fi
                    ;;
            esac
            ;;
        *)
            die "Unsupported platform: $os"
            ;;
    esac
}

detect_and_install
log "Done. Rust, VICE, WezTerm and TUI fonts are installed."
