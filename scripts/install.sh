#!/usr/bin/env bash
# Agent Workspace Hub — one-line installer (Rust binary)
#
# Usage:
#   curl -fsSL https://raw.githubusercontent.com/sawroop1242/Agent-workspace-hub/main/scripts/install.sh | bash
#
# Options:
#   curl ... | bash -s -- --source source     # build from source with cargo
#   curl ... | bash -s -- --version v0.1.0    # install a specific release tag
#   curl ... | bash -s -- --prefix ~/.bin     # install to a custom directory
#
# The installer downloads a prebuilt binary from the latest GitHub release for
# the detected OS/architecture. Termux on Android is detected automatically
# and uses the native Android ARM64 asset. It falls back to a cargo build when
# no matching asset exists (or when --source source is requested).

set -Eeuo pipefail

REPO="${AWH_REPO:-sawroop1242/Agent-workspace-hub}"
REF="${AWH_REF:-rust}"
RAW_URL="https://raw.githubusercontent.com/${REPO}/${REF}/scripts/install.sh"

INSTALL_SOURCE="${AWH_SOURCE:-release}"
VERSION="${AWH_VERSION:-latest}"
PREFIX="${AWH_PREFIX:-$HOME/.local/bin}"

usage() {
    cat <<EOF
Agent Workspace Hub installer (Rust binary)

Usage:
  curl -fsSL ${RAW_URL} | bash
  curl -fsSL ${RAW_URL} | bash -s -- [options]

Options:
  --source release|source  Install a prebuilt binary, or build from source with cargo. Default: release
  --version tag            Install a specific release tag (default: latest)
  --repo owner/name        GitHub repository to install from. Default: ${REPO}
  --ref git-ref            Git ref for source installs and raw installer URL. Default: ${REF}
  --prefix dir             Install directory (default: ${PREFIX})
  -h, --help               Show this help

Environment overrides:
  AWH_REPO, AWH_REF, AWH_SOURCE, AWH_VERSION, AWH_PREFIX
EOF
}

while [ "$#" -gt 0 ]; do
    case "$1" in
        --source)
            INSTALL_SOURCE="${2:-}"
            shift 2
            ;;
        --version)
            VERSION="${2:-}"
            shift 2
            ;;
        --repo)
            REPO="${2:-}"
            shift 2
            ;;
        --ref)
            REF="${2:-}"
            shift 2
            ;;
        --prefix)
            PREFIX="${2:-}"
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

if [ "$INSTALL_SOURCE" != "release" ] && [ "$INSTALL_SOURCE" != "source" ]; then
    echo "Error: --source must be either 'release' or 'source'." >&2
    exit 2
fi

log()    { printf '%s\n' "$*"; }
fail()   { printf 'Error: %s\n' "$*" >&2; exit 1; }

require_cmd() {
    command -v "$1" >/dev/null 2>&1 || fail "$1 is required but was not found."
}

detect_os() {
    # Termux exposes PREFIX under /data/data/com.termux/files/usr and may set
    # TERMUX_VERSION. Check this before generic Linux detection.
    if [ -n "${TERMUX_VERSION:-}" ] ||
       [ "${PREFIX:-}" = "/data/data/com.termux/files/usr" ] ||
       [ -n "${TERMUX_APK_RELEASE:-}" ]; then
        echo "android"
        return
    fi

    case "$(uname -s)" in
        Linux*)  echo "linux" ;;
        Darwin*) echo "macos" ;;
        *MINGW*|*MSYS*|*CYGWIN*) echo "windows" ;;
        *) fail "Unsupported operating system: $(uname -s)" ;;
    esac
}

detect_arch() {
    case "$(uname -m)" in
        x86_64|amd64)  echo "x86_64" ;;
        aarch64|arm64) echo "aarch64" ;;
        *) fail "Unsupported architecture: $(uname -m)" ;;
    esac
}

asset_name() {
    local os="$1" arch="$2"
    case "$os" in
        linux)   echo "awh-linux-${arch}" ;;
        android) echo "awh-android-${arch}" ;;
        macos)   echo "awh-macos-${arch}" ;;
        windows) echo "awh-windows-${arch}.exe" ;;
    esac
}

resolve_tag() {
    if [ "$VERSION" != "latest" ]; then
        printf '%s' "$VERSION"
        return
    fi
    curl -fsSL "https://api.github.com/repos/${REPO}/releases/latest" |
        sed -n 's/.*"tag_name": "\([^"]*\)".*/\1/p' | head -n 1
}

download_binary() {
    local os arch asset tag release_api url dest final

    os="$(detect_os)"
    arch="$(detect_arch)"
    asset="$(asset_name "$os" "$arch")"

    # The Android release currently supports ARM64 only.
    if [ "$os" = "android" ] && [ "$arch" != "aarch64" ]; then
        fail "Android prebuilt binaries currently support aarch64/ARM64 only."
    fi

    tag="$(resolve_tag)"
    if [ -z "$tag" ]; then
        return 1
    fi

    release_api="https://api.github.com/repos/${REPO}/releases/tags/${tag}"
    url="$(curl -fsSL "$release_api" |
        sed -n "s|.*\"browser_download_url\": \"\([^\"]*${asset}[^\"]*\)\".*|\1|p" | head -n 1)"

    if [ -z "$url" ]; then
        url="https://github.com/${REPO}/releases/download/${tag}/${asset}"
    fi

    log "Detected platform: ${os}/${arch}"
    log "Downloading ${asset} (${tag})..."
    dest="${PREFIX}/${asset}"
    curl -fsSL -o "$dest" "$url"

    [ -s "$dest" ] || fail "Downloaded asset is empty: $url"

    if [ "$os" != "windows" ]; then
        chmod +x "$dest"
    fi

    if [ "$os" = "windows" ]; then
        cp "$dest" "${PREFIX}/awh.exe"
        final="${PREFIX}/awh.exe"
    else
        ln -sf "$(basename "$dest")" "${PREFIX}/awh"
        final="${PREFIX}/awh"
    fi

    log "Installed: ${final}"
}

build_from_source() {
    require_cmd cargo
    local tmpdir
    tmpdir="$(mktemp -d 2>/dev/null || mktemp -d -t awh-build)"
    trap 'rm -rf "$tmpdir"' EXIT

    log "Building from source: https://github.com/${REPO}.git (${REF})"
    git clone --depth 1 --branch "$REF" "https://github.com/${REPO}.git" "$tmpdir/repo"
    ( cd "$tmpdir/repo" && cargo build --release )
    install -m 755 "$tmpdir/repo/target/release/awh" "${PREFIX}/awh"
    log "Installed: ${PREFIX}/awh"
}

print_success() {
    cat <<'EOF'

==========================================
  Installation Complete!
==========================================

Launch with:
  awh

If 'awh' is not found on PATH, add the install directory:
  export PATH="$HOME/.local/bin:$PATH"

Tune runtime limits via AWH_* environment variables (see docs/SECURITY.md).
EOF
}

log "=========================================="
log "  Agent Workspace Hub Installer (Rust)"
log "=========================================="

require_cmd curl

mkdir -p "$PREFIX"

if [ "$INSTALL_SOURCE" = "source" ]; then
    build_from_source
else
    if download_binary; then
        :
    else
        log "Prebuilt binary unavailable; falling back to building from source."
        build_from_source
    fi
fi

print_success
