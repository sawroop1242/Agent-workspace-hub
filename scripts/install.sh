#!/usr/bin/env bash
# Agent Workspace Hub — One-line installer
# Usage: curl -fsSL https://github.com/YOUR_USERNAME/agent-workspace-hub/releases/latest/download/install.sh | bash

set -e

REPO="YOUR_USERNAME/agent-workspace-hub"
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
    echo "Please install Python 3.11 or higher and try again."
    exit 1
fi

echo "Found Python: $PYTHON_CMD ($($PYTHON_CMD --version))"

# Fetch latest release
echo "Fetching latest release..."
RELEASE_JSON=$(curl -fsSL "$API_URL")
WHEEL_URL=$(echo "$RELEASE_JSON" | grep -o '"browser_download_url": "[^"]*\.whl"' | head -1 | cut -d'"' -f4)
VERSION=$(echo "$RELEASE_JSON" | grep -o '"tag_name": "[^"]*"' | head -1 | cut -d'"' -f4)

if [ -z "$WHEEL_URL" ]; then
    echo "Error: Could not find wheel in latest release."
    echo "Falling back to pip install from source..."
    $PYTHON_CMD -m pip install --user "git+https://github.com/${REPO}.git#egg=agent-workspace-hub[composio]"
    echo ""
    echo "Installed from source. Launch with: awh"
    exit 0
fi

echo "Latest version: $VERSION"
echo "Downloading..."

# Create temp dir
TMPDIR=$(mktemp -d)
trap "rm -rf $TMPDIR" EXIT

# Download wheel
curl -fsSL -o "$TMPDIR/awh.whl" "$WHEEL_URL"

echo "Installing..."
$PYTHON_CMD -m pip install --user "$TMPDIR/awh.whl"

echo ""
echo "=========================================="
echo "  Installation Complete!"
echo "=========================================="
echo ""
echo "Launch with: awh"
echo ""
echo "First time setup:"
echo "  1. Run: awh"
echo "  2. Go to Settings"
echo "  3. Add your Composio API key"
echo "  4. Start the MCP server from Home screen"
echo ""
