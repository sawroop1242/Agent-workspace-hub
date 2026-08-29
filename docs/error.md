# AWH Error Guide

**Project:** Agent Workspace Hub (AWH)  
**Component:** Rust CLI / MCP Server  
**Environment:** Android → Termux → `proot-distro` → Ubuntu  
**OpenCode:** MCP client  
**Version:** AWH `0.1.0`

## Purpose

This document records known errors and troubleshooting procedures for the Rust implementation of Agent Workspace Hub (AWH), with emphasis on ARM64 Linux, Android/Termux, Ubuntu proot, the AWH MCP server, and OpenCode MCP integration.

## Environment

Typical tested environment:

```text
Android
  └── Termux
       └── proot-distro
            └── Ubuntu
                 └── AWH Rust binary
```

Check architecture:

```bash
uname -m
```

Expected:

```text
aarch64
```

Typical binary location:

```text
/root/.local/bin/awh
```

## Error: `Not: command not found`

### Symptom

After downloading the binary, AWH reports:

```text
/root/.local/bin/awh: line 1: Not: command not found
```

### Cause

The downloaded file was an HTTP error response such as `Not Found`, rather than the executable. This can happen when using a `/releases/latest/download/...` URL while the desired release is a prerelease.

### Fix

Use the actual release tag and fail on HTTP errors:

```bash
curl -fL \
  https://github.com/sawroop1242/Agent-workspace-hub/releases/download/v0.1.0/awh-linux-aarch64 \
  -o ~/.local/bin/awh
chmod +x ~/.local/bin/awh
```

Always prefer `curl -fL` for binary downloads so HTTP errors are not silently saved as executables.

## Error: `file: command not found`

### Symptom

```text
bash: file: command not found
```

### Fix

Install the package:

```bash
apt update
apt install -y file
```

Then:

```bash
file ~/.local/bin/awh
```

Expected output identifies an ARM64 ELF executable.

## AWH MCP server starts with no visible output

Running:

```bash
awh mcp serve
```

may appear to hang. This is expected for a stdio MCP server: it waits for MCP JSON-RPC messages on stdin and writes responses to stdout.

Do not expect a message such as `Listening on http://127.0.0.1:3000` unless an HTTP transport is explicitly implemented.

## `awh mcp status` requires an ID

### Symptom

```text
error: the following required arguments were not provided:
  <ID>

Usage: awh mcp status <ID>
```

### Cause

`status` under `awh mcp` expects the ID of a registered MCP server. It is not the global AWH process-status command.

Use:

```bash
awh mcp status <ID>
```

## `awh mcp list` shows nothing

An empty result from:

```bash
awh mcp list
```

means that no external MCP servers are currently registered in AWH's MCP registry. It does not mean that:

```bash
awh mcp serve
```

is broken.

These are separate concepts:

```text
awh mcp serve  → starts AWH's MCP server
awh mcp list   → lists MCP servers registered in AWH
```

## OpenCode: AWH MCP timeout

### Symptom

OpenCode may report:

```text
✗ awh failed
    Operation timed out after 30000ms
    awh mcp serve
```

### Diagnosis

First test the AWH MCP server directly. A successful initialization test is:

```bash
printf '%s\n' \
'{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-06-18","capabilities":{},"clientInfo":{"name":"test","version":"1.0"}}}' |
awh mcp serve
```

A successful response has the following characteristics:

```text
server name: agent-workspace-hub
version: 0.1.0
protocol: 2025-06-18
```

If this direct test succeeds but OpenCode times out, the binary and basic MCP handshake are working; investigate OpenCode's MCP configuration, process environment, and transport settings.

## OpenCode: `agent-workspace-hub` SSE error

### Symptom

```text
✗ agent-workspace-hub failed
    SSE error: Was there a typo in the url or port?
    http://127.0.0
```

### Cause

This is an invalid or incomplete HTTP/SSE MCP configuration. It is separate from the local AWH command:

```bash
awh mcp serve
```

For a local stdio AWH server, configure OpenCode to launch the executable rather than using an SSE URL.

## OpenCode: `user-skills` SSE 406 error

### Symptom

```text
✗ user-skills failed
    SSE error: Non-200 status code (406)
```

### Cause

The remote endpoint is returning HTTP `406 Not Acceptable`. This is an external HTTP/SSE MCP configuration issue and is unrelated to the local AWH binary.

## OpenCode: Composio connection closed

### Symptom

```text
✗ composio failed
    MCP error -32000: Connection closed
```

### Possible causes

- invalid or expired API key
- remote endpoint configuration
- authentication failure
- transport incompatibility
- remote server closing the connection

This is independent of the AWH local MCP server.

### Security

Never commit MCP API keys to source code, documentation, shell scripts, or public repositories. Rotate/revoke a credential if it has been exposed.

## MCP stdio architecture

The intended local architecture is:

```text
┌──────────────────────────┐
│        OpenCode          │
│        MCP Client        │
└────────────┬─────────────┘
             │
             │ stdin/stdout
             │ JSON-RPC
             ▼
┌──────────────────────────┐
│     awh mcp serve        │
│     AWH Rust MCP         │
└──────────────────────────┘
```

When OpenCode manages the server, do not normally start a detached copy with `nohup`. The MCP client needs to own the stdio process and its stdin/stdout streams.

## Testing `tools/list`

After verifying initialization, test whether AWH exposes MCP tools:

```bash
printf '%s\n' \
'{"jsonrpc":"2.0","id":2,"method":"tools/list","params":{}}' |
awh mcp serve
```

Possible outcomes:

- A `tools` array containing entries: AWH exposes MCP tools.
- An empty `tools` array: the server is valid but currently exposes no tools.
- A method-not-found error: the current MCP implementation does not implement `tools/list`.

The initialization response observed during testing advertised an empty tools capability object:

```json
"capabilities": {
  "tools": {}
}
```

Therefore, tool discovery should be verified independently before assuming that OpenCode can call AWH functionality as MCP tools.

## OpenCode configuration

For a local stdio server, the configuration should be equivalent to:

```json
{
  "$schema": "https://opencode.ai/config.json",
  "mcp": {
    "awh": {
      "type": "local",
      "command": [
        "/root/.local/bin/awh",
        "mcp",
        "serve"
      ]
    }
  }
}
```

Using the absolute path is recommended in Ubuntu/proot because OpenCode may have a different `PATH` from the interactive shell.

The OpenCode CLI form is:

```bash
opencode mcp add awh -- /root/.local/bin/awh mcp serve
```

Then inspect:

```bash
opencode mcp list
```

Do not configure the local AWH process as SSE unless AWH explicitly provides an HTTP/SSE transport.

## Termux vs Ubuntu proot

### Direct Termux

For native Android/Termux execution, the Android ARM64 target is:

```text
aarch64-linux-android
```

and the release asset is:

```text
awh-android-aarch64
```

### Ubuntu proot

For the Ubuntu userspace under `proot-distro`, the preferred target is:

```text
aarch64-unknown-linux-gnu
```

with release asset:

```text
awh-linux-aarch64
```

The tested Ubuntu proot environment successfully executed the Linux ARM64 binary and the AWH MCP server.

## Binary installation

```bash
mkdir -p ~/.local/bin

curl -fL \
  https://github.com/sawroop1242/Agent-workspace-hub/releases/download/v0.1.0/awh-linux-aarch64 \
  -o ~/.local/bin/awh

chmod +x ~/.local/bin/awh
```

Add the directory to PATH if necessary:

```bash
echo 'export PATH="$HOME/.local/bin:$PATH"' >> ~/.bashrc
source ~/.bashrc
```

Verify:

```bash
which awh
awh --version
awh --help
```

## Troubleshooting checklist

Run these checks in order:

### 1. Architecture

```bash
uname -m
```

Expected:

```text
aarch64
```

### 2. Binary exists

```bash
ls -lh ~/.local/bin/awh
```

### 3. Executable permission

```bash
ls -l ~/.local/bin/awh
```

### 4. PATH

```bash
which awh
```

### 5. CLI

```bash
awh --help
```

### 6. Bootstrap status

```bash
awh status
```

Expected tested output includes:

```text
Agent Workspace Hub — Rust
status: bootstrap complete
```

### 7. MCP command

```bash
awh mcp --help
```

### 8. MCP server

```bash
awh mcp serve
```

### 9. MCP initialization

Use the JSON-RPC initialization test described above.

### 10. OpenCode

```bash
opencode mcp list
```

## Useful commands

### AWH

```bash
awh --help
awh --version
awh status
awh mcp --help
awh mcp serve
awh mcp list
awh mcp add <server>
awh mcp status <ID>
```

### Process management

Find a manually started AWH MCP server:

```bash
ps aux | grep '[a]wh mcp serve'
```

Stop one if necessary:

```bash
pkill -f 'awh mcp serve'
```

### Binary diagnostics

```bash
uname -m
which awh
ls -lh ~/.local/bin/awh
```

If installed:

```bash
file ~/.local/bin/awh
```

### OpenCode

```bash
opencode mcp list
```

Add local AWH:

```bash
opencode mcp add awh -- /root/.local/bin/awh mcp serve
```

## Recommended debugging flow

Use this order for future failures:

```text
1. Verify architecture
        ↓
2. Verify binary
        ↓
3. Run awh --help
        ↓
4. Run awh status
        ↓
5. Run awh mcp serve
        ↓
6. Test MCP initialize
        ↓
7. Test tools/list
        ↓
8. Configure OpenCode
        ↓
9. Run opencode mcp list
        ↓
10. Test AWH from OpenCode
```

This separates failures between:

```text
Binary
  ↓
AWH CLI
  ↓
MCP implementation
  ↓
OpenCode configuration
  ↓
External MCP server
```

## Summary

The tested AWH `0.1.0` ARM64 Linux binary runs successfully in Ubuntu under `proot-distro`.

The command:

```bash
awh mcp serve
```

starts the local MCP server and waits for stdio JSON-RPC traffic.

A direct MCP `initialize` request successfully returned a valid response identifying:

```text
server: agent-workspace-hub
version: 0.1.0
protocol: 2025-06-18
```

Therefore, if OpenCode reports a timeout while the direct MCP handshake succeeds, focus on the OpenCode MCP configuration and process environment rather than the ARM64 executable itself.
