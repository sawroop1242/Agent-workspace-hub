#!/usr/bin/env bash
# Agent Workspace Hub — One-line installer
# Usage: curl -fsSL https://github.com/sawroop1242/Agent-workspace-hub/releases/latest/download/install.sh | bash

set -e

REPO="sawroop1242/Agent-workspace-hub"
API_URL="https://api.github.com/repos/${REPO}/releases/latest"

echo "=========================================="
echo "  Agent Workspace Hub Installer"
echo "=========================================="

# Detect Python
PYTHON_CMD=""
for cmd in python3.13 python3.12 python3.11 python3; do
    if command -v "$cmd" &> /dev/null; then
        VERSION=$($cmd --version 2>&1 | awk '{print $2}' | cut -d. -f1,2)
        MAJOR=$(echo "$VERSION" | cut -d. -f1)
        MINOR=$(echo "$VERSION" | cut -d. -f2)
        if [ "$MAJOR" -gt 3 ] || ([ "$MAJOR" -eq 3 ] && [ "$MINOR" -ge 11 ]); then
            PYTHON_CMD=$cmd
            break
        fi
    fi
done

if [ -z "$PYTHON_CMD" ]; then
    echo "Error: Python 3.11+ is required but not found."
    exit 1
fi

echo "Found Python: $PYTHON_CMD ($($PYTHON_CMD --version))"

# Fetch latest release
echo "Fetching latest release..."
RELEASE_JSON=$(curl -fsSL "$API_URL")
WHEEL_URL=$(echo "$RELEASE_JSON" | grep -o '"browser_download_url": "[^"]*\.whl"' | head -1 | cut -d'"' -f4)
VERSION=$(echo "$RELEASE_JSON" | grep -o '"tag_name": "[^"]*"' | head -1 | cut -d'"' -f4)

if [ -z "$WHEEL_URL" ]; then
    echo "Error: Could not find wheel. Installing from source..."
    $PYTHON_CMD -m pip install --user "git+https://github.com/${REPO}.git#egg=agent-workspace-hub[composio]"
    echo "Done. Launch with: awh"
    exit 0
fi

echo "Latest version: $VERSION"
echo "Downloading..."

# Use safe temp dir (avoid TMPDIR env var conflict)
INSTALL_TMP="${HOME}/.awh-install-tmp"
mkdir -p "$INSTALL_TMP"
trap "rm -rf $INSTALL_TMP" EXIT

WHEEL_FILE="$INSTALL_TMP/agent_workspace_hub.whl"
curl -fsSL -o "$WHEEL_FILE" "$WHEEL_URL"

echo "Installing..."
$PYTHON_CMD -m pip install --user "$WHEEL_FILE"

echo ""
echo "=========================================="
echo "  Installation Complete!"
echo "=========================================="
echo ""
echo "Launch with: awh"
echo ""
echo "If 'awh' not found, run:"
echo "  export PATH=\"\$HOME/.local/bin:\$PATH\""
echo ""
echo "First time setup:"
echo "  1. Run: awh"
echo "  2. Go to Settings, add Composio API key"
echo "  3. Start MCP server from Home screen"
echo ""
