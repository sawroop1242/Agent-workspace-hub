# MCP Integration

`awh mcp serve` exposes Agent Workspace Hub as a standards-compliant MCP
server. This document describes the protocol surface, both transports, the
43-tool catalog, and the interoperability evidence.

## Transports

### stdio (default)

```bash
awh mcp serve            # JSON-RPC over stdin/stdout
```

Used by local MCP clients (OpenCode, Codex CLI, MCP Inspector). One client
per process. Line length is capped at `AWH_MAX_MCP_LINE_BYTES`
(default 10 MiB).

### HTTP + SSE (remote)

```bash
export AWH_API_KEY=...   # required: bearer token for remote access
awh mcp serve --transport sse --host 0.0.0.0 --port 8443 \
  --tls-cert cert.pem --tls-key key.pem
```

Endpoints:

| Endpoint | Purpose |
| --- | --- |
| `GET /health` | Liveness probe (no authentication) |
| `GET /sse` | Server-Sent Events stream; opens an isolated session |
| `POST /mcp?sessionId=…` | Submit a JSON-RPC message to that session |

Authentication is **mandatory** for `/sse` and `/mcp`: a `Bearer` token
matching `AWH_API_KEY` (constant-time comparison). Sessions are capped at
100 concurrent; each has its own dispatcher state; idle keep-alive pings
every 15 s. TLS is strongly recommended; without `--tls-cert/--tls-key` the
server runs plain HTTP, which is only acceptable on a private network.

## Tool catalog (43 tools)

| Tool | Purpose |
| --- | --- |
| `skills.list` / `skills.read` / `skills.add` / `skills.remove` / `skills.search` | Skill discovery and management |
| `workspace.context` / `workspace.list_files` / `workspace.read_file` | Workspace inspection |
| `memory.store` / `memory.search` / `memory.get` / `memory.delete` | Project-scoped memory |
| `tasks.create` / `tasks.list` / `tasks.update` / `tasks.delete` | Task management |
| `connectors.list` / `connectors.add` / `connectors.enable` / `connectors.disable` / `connectors.remove` | External connector management |
| `connector.providers` / `connector.tools` / `connector.invoke` | External connector invocation |

Every tool advertises a JSON `inputSchema`; malformed arguments are rejected
at dispatch with a JSON-RPC error rather than a panic.

## Interoperability evidence

AWH is verified against two independent, standards-compliant MCP clients:

1. **Official `@modelcontextprotocol/sdk` reference client** (TypeScript) —
   the protocol stack used by OpenCode, Codex, and most MCP clients.
2. **Official MCP Inspector** — the reference testing client maintained by
   the MCP project.

Recorded results (full harness output in `examples/mcp-interop/`):

```text
$ node examples/mcp-interop/stdio-client.mjs
PASS connect + initialize                      (server: agent-workspace-hub 0.1.0)
PASS tools/list (43 tools)
PASS every tool has an inputSchema
PASS tools/call workspace.context
PASS tools/call skills.list
PASS tools/call memory.store -> memory.search round-trip
PASS unknown tool -> JSON-RPC error (code -32603)
PASS clean disconnect (client.close)
STDIO INTEROP: ALL CHECKS PASSED

$ node examples/mcp-interop/sse-client.mjs
PASS SSE connect + initialize (server: agent-workspace-hub 0.1.0)   [HTTPS + bearer]
PASS SSE tools/list (43 tools)
PASS SSE tools/call workspace.context
PASS unknown sessionId rejected with 404
PASS wrong bearer token rejected with 401
PASS missing Authorization rejected with 401
PASS SSE client disconnect
PASS server exits on SIGTERM
SSE INTEROP: ALL CHECKS PASSED

$ npx @modelcontextprotocol/inspector --cli awh mcp serve --method tools/call \
    --tool-name skills.list
{"content":[{"type":"text","text":"[]"}]}
```

**OpenCode and Codex themselves: NOT VERIFIED.** They require interactive
provider accounts that CI cannot exercise. Because both build on the same
reference SDK/protocol AWH passes above, and because the MCP Inspector (the
project's own conformance client) connects successfully, protocol
compatibility is demonstrated; vendor-client UX checks remain manual for an
operator with those accounts.

## End-to-end workflow

```
OpenCode / Codex / any MCP client
        │ stdio or HTTPS+SSE
        ▼
awh mcp serve
        │  initialize → tools/list
        ▼
workspace.context            (project discovery)
        │
        ▼
memory.store / tasks.create  (project operations)
        │
        ▼
results returned to the agent
```

## Connecting a client

### OpenCode

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

### Remote (any SSE-capable client)

```
URL: https://host:8443/sse
Authorization: Bearer <AWH_API_KEY>
```

### Testing manually

```bash
printf '%s\n' '{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-03-26","capabilities":{},"clientInfo":{"name":"t","version":"1"}}}' | awh mcp serve
```

Or drive it with the official Inspector:

```bash
npx @modelcontextprotocol/inspector --cli awh mcp serve --method tools/call --tool-name skills.list
```

See `examples/mcp-interop/README.md` to run both interop harnesses yourself
(`npm install` once inside that directory, then `npm run stdio` /
`npm run sse`).
