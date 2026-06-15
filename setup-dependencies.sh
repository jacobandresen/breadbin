#!/usr/bin/env bash
#
# setup-dependencies.sh
#
# Installs the project's runtime dependencies:
#   - Python 3 (the scripts are single-file Python 3, stdlib only)
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

install_macos() {
    have brew || die "Homebrew is required. Install it from https://brew.sh and re-run."

    log "Updating Homebrew..."
    brew update

    log "Installing Python 3..."
    brew install python

    log "Installing VICE..."
    brew install --cask vice

    log "Installing WezTerm..."
    brew install --cask wezterm
}

install_arch() {
    have pacman || die "pacman not found; this is not an Arch-based system."

    log "Refreshing package databases and installing Python 3, VICE and WezTerm..."
    sudo pacman -Syu --needed --noconfirm python vice wezterm
}

install_ubuntu() {
    have apt-get || die "apt-get not found; this is not a Debian/Ubuntu system."

    log "Updating package lists..."
    sudo apt-get update

    log "Installing Python 3..."
    sudo apt-get install -y python3

    log "Installing VICE..."
    sudo apt-get install -y vice

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
log "Done. Python 3, VICE and WezTerm are installed."
