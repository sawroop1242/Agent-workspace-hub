# MCP Interoperability Harness

Real-client interop tests for `awh mcp serve`, using the **official
`@modelcontextprotocol/sdk` TypeScript reference client** — the same protocol
stack that OpenCode, Codex, and other standards-compliant MCP clients use.

These scripts are evidence tools, not unit tests: they spawn the real `awh`
binary and drive it over a real transport, then print `PASS`/`FAIL` per check
and exit nonzero on any failure.

## What is verified

| Transport | Checks |
| --- | --- |
| `stdio-client.mjs` | connect + initialize, server name/version, `tools/list` (all tools carry `inputSchema`), `tools/call` workspace.context, skills.list, memory.store → memory.search round-trip, unknown tool → clean JSON-RPC error, clean disconnect |
| `sse-client.mjs` | HTTPS server start (TLS), SSE connect + initialize with bearer token, `tools/list`, `tools/call`, unknown `sessionId` → 404, wrong bearer token → 401, missing `Authorization` → 401, client disconnect, server exit on SIGTERM |

## Prerequisites

```bash
# Rust binary under test
cargo build --release

# Reference client (official npm registry)
cd examples/mcp-interop
npm install            # uses the committed package-lock.json
```

The SSE harness needs a TLS certificate. For local testing, generate a
self-signed pair:

```bash
mkdir -p /tmp/awh-tls
openssl req -x509 -newkey rsa:2048 -keyout /tmp/awh-tls/key.pem \
  -out /tmp/awh-tls/cert.pem -days 30 -nodes -subj "/CN=localhost"
```

> The harness trusts **only this certificate** (via `NODE_EXTRA_CA_CERTS`)
> and still verifies the chain. Never disable certificate verification
> (`NODE_TLS_REJECT_UNAUTHORIZED=0`) against a production server.

## Running

```bash
cd examples/mcp-interop
npm run stdio
npm run sse
```

Both scripts:

- use an isolated `HOME` (`/tmp/awh-interop-home`) so they never touch your
  real `~/.agent-workspace-hub` memory, trust store, or skills;
- use an ephemeral port (SSE), so concurrent runs do not collide;
- clean up spawned processes and temp files on exit.

## Expected output

```text
PASS connect + initialize
PASS tools/list (24 tools)
...
STDIO INTEROP: ALL CHECKS PASSED
```

```text
PASS SSE connect + initialize (server: agent-workspace-hub 0.1.0)
...
SSE INTEROP: ALL CHECKS PASSED
```

## Verified results

Both harnesses were executed against `awh 0.1.0` built from the `rust` branch
and passed every check (see `docs/mcp.md` § Interoperability evidence for the
recorded output). Manual verification against the OpenCode and Codex clients
requires accounts and client installs outside this repository's CI scope and
is documented as **NOT VERIFIED** there — the reference-SDK harness above is
the reproducible interop evidence.

## Connecting OpenCode manually

OpenCode speaks MCP over stdio:

```json
{
  "mcp": {
    "awh": {
      "type": "local",
      "command": ["awh", "mcp", "serve"],
      "enabled": true
    }
  }
}
```

See `docs/mcp.md` for the full end-to-end workflow diagram.
