#!/usr/bin/env bash
# Agent Workspace Hub — one-step Rust binary installer
#
# Install the latest published release without Python, pip, pipx, or Cargo:
#   curl -fsSL https://raw.githubusercontent.com/sawroop1242/Agent-workspace-hub/main/scripts/install.sh | bash
#
# Install a specific release:
#   curl -fsSL https://raw.githubusercontent.com/sawroop1242/Agent-workspace-hub/main/scripts/install.sh | bash -s -- --version v0.1.0
#
# The installer detects Linux/macOS/Windows and CPU architecture, downloads the
# matching GitHub Release asset, verifies its SHA-256 when sha256sums.txt exists,
# installs it to ~/.local/bin by default, and exposes it as `awh`.

set -Eeuo pipefail

REPO="${AWH_REPO:-sawroop1242/Agent-workspace-hub}"
VERSION="${AWH_VERSION:-latest}"
PREFIX="${AWH_PREFIX:-$HOME/.local/bin}"

usage() {
    cat <<EOF
Agent Workspace Hub installer (Rust binary)

Usage:
  curl -fsSL https://raw.githubusercontent.com/${REPO}/main/scripts/install.sh | bash
  curl -fsSL https://raw.githubusercontent.com/${REPO}/main/scripts/install.sh | bash -s -- [options]

Options:
  --version TAG    Install a specific release (default: latest)
  --prefix DIR     Install directory (default: ~/.local/bin)
  --repo OWNER/REPO
                   GitHub repository (default: ${REPO})
  -h, --help       Show this help

Environment:
  AWH_REPO, AWH_VERSION, AWH_PREFIX
EOF
}

while [ "$#" -gt 0 ]; do
    case "$1" in
        --version)
            [ "$#" -ge 2 ] || { echo "Error: --version requires a value." >&2; exit 2; }
            VERSION="$2"
            shift 2
            ;;
        --prefix)
            [ "$#" -ge 2 ] || { echo "Error: --prefix requires a value." >&2; exit 2; }
            PREFIX="$2"
            shift 2
            ;;
        --repo)
            [ "$#" -ge 2 ] || { echo "Error: --repo requires a value." >&2; exit 2; }
            REPO="$2"
            shift 2
            ;;
        -h|--help)
            usage
            exit 0
            ;;
        *)
            echo "Error: unknown option: $1" >&2
            usage >&2
            exit 2
            ;;
    esac
done

log() {
    printf '%s\n' "$*"
}

fail() {
    printf 'Error: %s\n' "$*" >&2
    exit 1
}

require_cmd() {
    command -v "$1" >/dev/null 2>&1 || fail "$1 is required but was not found."
}

detect_os() {
    case "$(uname -s)" in
        Linux*) echo "linux" ;;
        Darwin*) echo "macos" ;;
        MINGW*|MSYS*|CYGWIN*) echo "windows" ;;
        *) fail "Unsupported operating system: $(uname -s)" ;;
    esac
}

detect_arch() {
    case "$(uname -m)" in
        x86_64|amd64) echo "x86_64" ;;
        aarch64|arm64) echo "aarch64" ;;
        *) fail "Unsupported architecture: $(uname -m)" ;;
    esac
}

asset_name() {
    local os="$1" arch="$2"
    case "$os" in
        linux) echo "awh-linux-${arch}" ;;
        macos) echo "awh-macos-${arch}" ;;
        windows)
            [ "$arch" = "x86_64" ] || fail "Windows prebuilt binaries currently support x86_64 only."
            echo "awh-windows-x86_64.exe"
            ;;
    esac
}

resolve_version() {
    if [ "$VERSION" != "latest" ]; then
        printf '%s' "$VERSION"
        return
    fi

    curl -fsSL \
        -H 'Accept: application/vnd.github+json' \
        "https://api.github.com/repos/${REPO}/releases/latest" \
        | sed -n 's/.*"tag_name"[[:space:]]*:[[:space:]]*"\([^"]*\)".*/\1/p' \
        | head -n 1
}

sha256_file() {
    local file="$1"
    if command -v sha256sum >/dev/null 2>&1; then
        sha256sum "$file" | awk '{print $1}'
    elif command -v shasum >/dev/null 2>&1; then
        shasum -a 256 "$file" | awk '{print $1}'
    else
        return 1
    fi
}

verify_checksum() {
    local file="$1" expected="$2" actual

    [ -n "$expected" ] || return 0

    if ! actual="$(sha256_file "$file")"; then
        log "Warning: sha256sum/shasum is unavailable; checksum verification skipped."
        return 0
    fi

    if [ "${actual,,}" != "${expected,,}" ]; then
        fail "SHA-256 verification failed for $(basename "$file")."
    fi

    log "SHA-256 verified."
}

install_binary() {
    local os arch asset tag base_url url tmpdir downloaded checksum_url sums expected final

    os="$(detect_os)"
    arch="$(detect_arch)"
    asset="$(asset_name "$os" "$arch")"
    tag="$(resolve_version)"
    [ -n "$tag" ] || fail "Could not determine the latest release tag."

    base_url="https://github.com/${REPO}/releases/download/${tag}"
    url="${base_url}/${asset}"

    log "AWH installer"
    log "Platform: ${os}/${arch}"
    log "Release: ${tag}"
    log "Asset: ${asset}"

    mkdir -p "$PREFIX"
    tmpdir="$(mktemp -d 2>/dev/null || mktemp -d -t awh-install)"
    trap 'rm -rf "$tmpdir"' EXIT

    downloaded="${tmpdir}/${asset}"
    log "Downloading..."
    curl -fL --retry 3 --retry-delay 1 --connect-timeout 10 -o "$downloaded" "$url" \
        || fail "Could not download ${url}"
    [ -s "$downloaded" ] || fail "Downloaded AWH binary is empty."

    checksum_url="${base_url}/sha256sums.txt"
    sums="${tmpdir}/sha256sums.txt"
    expected=""
    if curl -fL --retry 2 --connect-timeout 10 -sS -o "$sums" "$checksum_url"; then
        expected="$(awk -v file="$asset" '$NF == file {print $1; exit}' "$sums")"
        if [ -n "$expected" ]; then
            verify_checksum "$downloaded" "$expected"
        else
            log "Warning: no checksum entry found for ${asset}; continuing."
        fi
    else
        log "Warning: sha256sums.txt unavailable; continuing without checksum verification."
    fi

    chmod 755 "$downloaded"

    if [ "$os" = "windows" ]; then
        final="${PREFIX}/awh.exe"
        mv -f "$downloaded" "$final"
    else
        final="${PREFIX}/awh"
        mv -f "$downloaded" "$final"
    fi

    chmod 755 "$final"
    log "Installed: ${final}"
}

print_success() {
    cat <<EOF

Installation complete.

Run:
  awh

Installed binary:
  ${PREFIX}/awh

If 'awh' is not found, add this directory to PATH:
  export PATH="${PREFIX}:\$PATH"
EOF
}

require_cmd curl
install_binary
print_success
