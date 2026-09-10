# MCP Integration

`awh mcp serve` exposes Agent Workspace Hub as a standards-compliant MCP
server. This document describes the protocol surface, both transports, the
tool catalog (52 core tools, plus 12 optional `github.*` tools for a 64-tool
catalog when `GITHUB_TOKEN` is set), and the interoperability evidence.

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

## Tool catalog (52 core tools; 64 with `github.*`)

The core catalog below is always advertised. The 12 `github.*` tools in the
last row appear in `tools/list` **only when `GITHUB_TOKEN` is set** (a classic
PAT or fine-grained token); when unset, they are neither advertised nor
callable, and callers get a clear error if they try.

| Tool | Purpose |
| --- | --- |
| `skills.list` / `skills.read` / `skills.add` / `skills.remove` / `skills.search` | Skill discovery and management |
| `workspace.context` / `workspace.list_files` / `workspace.read_file` / `workspace.write_file` / `workspace.delete_file` | Workspace inspection and bounded file editing (2 MiB read / 5 MiB write caps, path-traversal checked) |
| `memory.store` / `memory.search` / `memory.get` / `memory.update` / `memory.delete` | Project-scoped memory |
| `tasks.create` / `tasks.get` / `tasks.list` / `tasks.update` / `tasks.delete` | Task management |
| `connectors.get` / `connectors.list` / `connectors.add` / `connectors.enable` / `connectors.disable` / `connectors.remove` | External connector management |
| `connector.providers` / `connector.tools` / `connector.invoke` | External connector invocation |
| `connector.composio_link` / `connector.composio_accounts` / `connector.composio_register` / `connector.composio_remove` | Composio connected-account management |
| `git.status` / `git.log` / `git.diff` / `git.stage` / `git.unstage` / `git.commit` / `git.branch` | Repository operations |
| `terminal.run` | Sandboxed command execution |
| `context.status` / `context.insert` / `context.get` / `context.remove` / `context.search` / `context.optimize` / `context.assemble` / `context.protect` / `context.unprotect` / `context.offload` / `context.restore` | Context engine: token budget, offload, and item protection |
| `github.pr_list` / `github.pr_get` / `github.pr_create` / `github.pr_merge` / `github.pr_review` / `github.issue_list` / `github.issue_get` / `github.issue_create` / `github.issue_comment` / `github.checks_status` / `github.workflow_dispatch` / `github.release_create` | GitHub-native pull request, issue, CI, and release operations (require `GITHUB_TOKEN`) |

### `github.*` targeting and configuration

- **`GITHUB_TOKEN`** enables the tools. The token is read at dispatcher
  construction and sent only as a `Authorization: Bearer` header to
  `api.github.com` (or `GITHUB_API_URL` when set, e.g. for GitHub Enterprise
  Server). It is never written to logs, audit entries, or tool results.
- **`owner`/`repo` resolution** for every `github.*` tool, in order: explicit
  `owner`+`repo` arguments, then the project's `origin` git remote, then
  `GITHUB_DEFAULT_OWNER`/`GITHUB_DEFAULT_REPO`. Sources are never mixed — a
  call that specifies `owner` but not `repo` resolves the *pair* from the
  same fallback or fails closed with an error, never an explicit owner paired
  with an environment repo.
- Requests go to the GitHub REST API v3 directly (`/repos/{owner}/{repo}/...`);
  no Composio or other intermediary is involved.

Every tool advertises a JSON `inputSchema`; malformed arguments are rejected
at dispatch with a JSON-RPC error rather than a panic.

## Security model

### Session lifecycle (MCP §8)

A session (one stdio process or one SSE connection) must complete the
`initialize` exchange before anything else. Every request sent before that —
except `initialize` and `ping` — is rejected with JSON-RPC error `-32002`
(server not initialized), and the denial is audited. A malformed
`initialize` (bad params) returns `-32602` and leaves the session
uninitialized so the client can retry; a duplicate `initialize` on a live
session is a deterministic `-32600`. Notifications are accepted silently at
every stage, matching the MCP reference servers.

### Custom MCP servers: trust is enforced, not stored

Custom (per-project, `.agent/mcps.json`) MCP servers follow this lifecycle:

```
configuration → enabled? → trust decision → permission validation
             → spawn → initialize → health (circuit breaker) → registration
```

* **Enabled is not trusted.** An enabled server is only spawned if it has an
  explicit, matching approval in the persistent trust store
  (`~/.agent-workspace-hub`). Missing, blocked, wrong-version, or
  over-broad (config requests permissions the approval does not grant)
  fail closed: the server is skipped entirely — it never registers a
  provider, so its tools are invisible. Untrusted servers are unavailable
  by construction, not by policy alone.
* **A corrupted or unreadable trust store means no approvals** — deny all,
  never fall back to "probably fine".
* **Permissions are validated at registration**: `network`, `process`,
  `filesystem` (absolute, existing paths only), `environment` (safe
  identifier names; dangerous variables like `LD_PRELOAD` and `PATH` are
  blocked), and `secrets` (every secret must also be an allowed
  environment key — a conflict is rejected). A malformed permission set
  cannot even be stored.
* **Permissions are enforced at spawn**: the child receives only the
  allow-listed environment keys; secret values are injected only when the
  secret name appears in the server's own configuration, and only when the
  matching secret permission was granted.
* No automatic trust escalation: nothing a server does at runtime can
  upgrade its approval.

This is the *current* trust boundary — a per-server execution gate. It is
not the future AWH-wide Tool Broker / capability system.

### Sandbox (platform-specific, honestly stated)

Custom stdio MCP servers are spawned through a sandbox when enabled. What
that means depends on the platform — no false equivalence claimed:

| Platform | Mechanism | Enforced |
| --- | --- | --- |
| Linux | bubblewrap (`bwrap`) | mount namespace (root + permission paths bound; `/usr`, `/proc`, `/dev`, `/tmp` only), user/pid/ipc/uts namespaces, all capabilities dropped, network **only** when the `network` permission is granted, and rlimits: address space 2 GiB, CPU 300 s, 128 processes, 1024 fds (defaults; `SandboxLimits`). `--die-with-parent` and `--new-session` prevent orphans. Missing `bwrap` (override with `AWH_BWRAP`) ⇒ **execution fails closed**, never runs unsandboxed. |
| macOS | `sandbox-exec` profile | `(deny default)` with read allowed for system paths + the project root, writes restricted to the project root and `/tmp`; network only when granted. Note: `sandbox-exec` is deprecated by Apple but remains enforced here. Resource limits (CPU/memory/fd/process counts) are **not** applied on macOS. |
| Windows | Job Object | memory + job memory + active-process limits applied to the child; handle released on drop. This bounds resources only — it is **not** a filesystem or network sandbox. |
| Other | none | Sandboxed execution is **refused** when enabled; the server is not started unsandboxed. |

Sandbox-disabled is a per-server configuration choice; the trust gate,
permission validation, environment filtering, and circuit breaker still apply.

### Circuit breaker and bounded resources

* Every custom MCP server is wrapped in a **circuit breaker** (closed →
  open after 5 consecutive failures → half-open after a 30 s cooldown).
  In half-open exactly **one** probe call is admitted; concurrent callers
  are rejected, and a lost probe self-heals after 60 s rather than wedging
  the breaker. A failing server degrades to fast errors instead of
  cascading delays. Thresholds/cooldown are tunable via
  `AWH_CIRCUIT_FAILURE_THRESHOLD` / `AWH_CIRCUIT_COOLDOWN_SECS`.
* **Bounded everything**: stdio lines and HTTP bodies are capped at
  10 MiB (`AWH_MAX_MCP_LINE_BYTES`), HTTP client requests time out at
  30 s, per-request MCP dispatch times out, and SSE sessions are capped
  at 100 concurrent with per-session dispatcher state. Children are
  `kill_on_drop`, so a dropped client cannot leave an orphaned server
  process.
* **No panics on the request path**: construction of shared clients
  returns errors (fail-closed) instead of unwinding, HTTP handlers
  degrade gracefully (e.g. a rate-limited response still ships without
  the header if header rendering ever failed), and all denials are
  audited (`api_rate_limit`, `session_preinit_rejected`, …).

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
PASS tools/list (64 tools)
PASS every tool has an inputSchema
PASS tools/call workspace.context
PASS tools/call skills.list
PASS tools/call memory.store -> memory.search round-trip
PASS unknown tool -> JSON-RPC error (code -32603)
PASS clean disconnect (client.close)
STDIO INTEROP: ALL CHECKS PASSED

$ node examples/mcp-interop/sse-client.mjs
PASS SSE connect + initialize (server: agent-workspace-hub 0.1.0)   [HTTPS + bearer]
PASS SSE tools/list (64 tools)
PASS SSE tools/call workspace.context
PASS unknown sessionId rejected with 404
PASS wrong bearer token rejected with 401
PASS missing Authorization rejected with 401
PASS SSE client disconnect
PASS server exits on SIGTERM
SSE INTEROP: ALL CHECKS PASSED
```

> The recorded runs above were taken with `GITHUB_TOKEN` set, so all 12
> `github.*` tools are advertised (64 total). Without it the same harnesses
> advertise 52 tools and pass identically — the count is expected to vary
> with the environment, which is why the harness prints it dynamically.

The MCP Inspector (the reference testing client) also round-trips a call:

```text
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
