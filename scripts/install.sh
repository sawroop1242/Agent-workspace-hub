#!/usr/bin/env bash
# Agent Workspace Hub — one-line installer
# Usage:
#   curl -fsSL https://raw.githubusercontent.com/sawroop1242/Agent-workspace-hub/main/scripts/install.sh | bash
# Optional:
#   curl -fsSL https://raw.githubusercontent.com/sawroop1242/Agent-workspace-hub/main/scripts/install.sh | bash -s -- --source source --with-composio

set -Eeuo pipefail

REPO="${AWH_REPO:-sawroop1242/Agent-workspace-hub}"
REF="${AWH_REF:-main}"
API_URL="https://api.github.com/repos/${REPO}/releases/latest"
SOURCE_URL="git+https://github.com/${REPO}.git@${REF}#egg=agent-workspace-hub"
RAW_URL="https://raw.githubusercontent.com/${REPO}/${REF}/scripts/install.sh"
INSTALL_SOURCE="release"
WITH_COMPOSIO="0"
PYTHON_CMD="${PYTHON:-}"

usage() {
    cat <<EOF
Agent Workspace Hub installer

Usage:
  curl -fsSL ${RAW_URL} | bash
  curl -fsSL ${RAW_URL} | bash -s -- [options]

Options:
  --source release|source  Install latest release wheel first, or install from Git source. Default: release
  --with-composio          Install optional Composio dependencies too
  --repo owner/name        GitHub repository to install from. Default: ${REPO}
  --ref git-ref            Git ref for source installs and raw installer URL. Default: ${REF}
  --python path            Python 3.11+ executable to use
  -h, --help               Show this help

Environment overrides:
  AWH_REPO, AWH_REF, PYTHON
EOF
}

while [ "$#" -gt 0 ]; do
    case "$1" in
        --source)
            INSTALL_SOURCE="${2:-}"
            shift 2
            ;;
        --with-composio)
            WITH_COMPOSIO="1"
            shift
            ;;
        --repo)
            REPO="${2:-}"
            shift 2
            ;;
        --ref)
            REF="${2:-}"
            shift 2
            ;;
        --python)
            PYTHON_CMD="${2:-}"
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

API_URL="https://api.github.com/repos/${REPO}/releases/latest"
SOURCE_URL="git+https://github.com/${REPO}.git@${REF}#egg=agent-workspace-hub"

if [ "$INSTALL_SOURCE" != "release" ] && [ "$INSTALL_SOURCE" != "source" ]; then
    echo "Error: --source must be either 'release' or 'source'." >&2
    exit 2
fi

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

python_version_ok() {
    "$1" - <<'PY' >/dev/null 2>&1
import sys
raise SystemExit(0 if sys.version_info >= (3, 11) else 1)
PY
}

find_python() {
    if [ -n "$PYTHON_CMD" ]; then
        command -v "$PYTHON_CMD" >/dev/null 2>&1 || fail "Python executable not found: $PYTHON_CMD"
        python_version_ok "$PYTHON_CMD" || fail "Python 3.11+ is required: $($PYTHON_CMD --version 2>&1)"
        return
    fi

    for cmd in python3.13 python3.12 python3.11 python3 python; do
        if command -v "$cmd" >/dev/null 2>&1 && python_version_ok "$cmd"; then
            PYTHON_CMD="$cmd"
            return
        fi
    done

    fail "Python 3.11+ is required but was not found."
}

pip_install_user() {
    local spec="$1"
    log "Installing with pip --user..."
    "$PYTHON_CMD" -m pip install --user --upgrade pip >/dev/null
    "$PYTHON_CMD" -m pip install --user --upgrade "$spec"
}

pipx_install() {
    local spec="$1"
    if command -v pipx >/dev/null 2>&1; then
        log "Installing isolated CLI with pipx..."
        pipx install --force "$spec"
        return 0
    fi
    return 1
}

install_spec() {
    local spec="$1"
    if ! pipx_install "$spec"; then
        pip_install_user "$spec"
    fi
}

build_package_spec() {
    local base="$1"
    if [ "$WITH_COMPOSIO" = "1" ]; then
        printf '%s[composio]' "$base"
    else
        printf '%s' "$base"
    fi
}

install_from_source() {
    local spec
    spec="$(build_package_spec "$SOURCE_URL")"
    log "Installing from source: https://github.com/${REPO}.git (${REF})"
    install_spec "$spec"
}

install_from_release() {
    local release_json wheel_url version install_tmp wheel_file spec

    require_cmd curl
    log "Fetching latest release metadata..."
    if ! release_json="$(curl -fsSL "$API_URL")"; then
        log "Could not fetch the latest release; falling back to source install."
        install_from_source
        return
    fi

    wheel_url="$(printf '%s' "$release_json" | sed -n 's/.*"browser_download_url": "\([^"]*\.whl\)".*/\1/p' | head -n 1)"
    version="$(printf '%s' "$release_json" | sed -n 's/.*"tag_name": "\([^"]*\)".*/\1/p' | head -n 1)"

    if [ -z "$wheel_url" ]; then
        log "No wheel asset found in the latest release; falling back to source install."
        install_from_source
        return
    fi

    log "Latest release: ${version:-unknown}"
    install_tmp="$(mktemp -d 2>/dev/null || mktemp -d -t awh-install)"
    trap 'rm -rf "$install_tmp"' EXIT

    wheel_file="${install_tmp}/agent_workspace_hub.whl"
    log "Downloading wheel..."
    curl -fsSL -o "$wheel_file" "$wheel_url"

    spec="$(build_package_spec "$wheel_file")"
    install_spec "$spec"
}

print_success() {
    cat <<'EOF'

==========================================
  Installation Complete!
==========================================

Launch with:
  awh

If 'awh' is not found, add the user scripts directory to PATH:
  export PATH="$HOME/.local/bin:$PATH"

First time setup:
  1. Run: awh
  2. Go to Settings and add your Composio API key if you use connectors
  3. Start the MCP server from the Home screen
EOF
}

log "=========================================="
log "  Agent Workspace Hub Installer"
log "=========================================="

require_cmd curl
find_python
log "Found Python: $PYTHON_CMD ($($PYTHON_CMD --version 2>&1))"

if [ "$INSTALL_SOURCE" = "source" ]; then
    install_from_source
else
    install_from_release
fi

print_success
