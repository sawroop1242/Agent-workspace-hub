# MCP Integration

`awh mcp serve` exposes Agent Workspace Hub as a standards-compliant MCP
server. This document describes the protocol surface, both transports, the
tool catalog (53 core tools, plus 12 optional `github.*` tools for a 65-tool
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

## Tool catalog (53 core tools; 65 with `github.*`)

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
| `mcp.status` | Read-only server health snapshot: protocol versions, tool/provider counts, per-tool call metrics, uptime. No secrets. |
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
session is a deterministic `-32600` — including under concurrency: the
New→Ready transition is a single atomic compare-and-swap, so of two racing
`initialize` requests exactly one wins and the loser receives `-32600`
(pinned by test). Notifications are accepted silently at
every stage, matching the MCP reference servers.

### Protocol version negotiation

The server advertises `SUPPORTED_PROTOCOL_VERSIONS` (currently
`2024-11-05`, `2025-03-26`, `2025-06-18`). On `initialize` it echoes the
client's requested version when it is supported and falls back to the
latest supported version otherwise — clients that need a specific version
can detect the downgrade. Unparseable requests get `-32700` with `id: null`;
JSON-RPC batches (arrays) are not part of MCP and are rejected as parse
errors, pinned by test.

### Request ids: absent means notification, `null` means invalid

The `id` member's PRESENCE — not its value — distinguishes a request from
a notification (JSON-RPC 2.0). A message with no `id` member is a
notification: the method is observed, and no response is ever emitted —
including for `tools/call` and for notifications that arrive while the
session is closed or uninitialized. A message with a PRESENT `id` is a
request, even when the value is `null`; MCP requires request ids to be
strings or numbers, so `"id": null` is rejected with `-32600` (invalid
request) rather than silently swallowed as a notification. Both semantics
are pinned by named tests.

### Structural envelope validation (-32600)

Every request is structurally validated at parse time before any
lifecycle, auth, or method dispatch: the `jsonrpc` member must be the
string `"2.0"` (missing, `null`, non-string, or a wrong version string →
`-32600`), the `method` member must be a non-empty string (missing, `null`,
non-string, or empty → `-32600`, NOT `-32601`: a structurally invalid
method is not a "method not found" case), and the `id`, when present, must
be a string or a number (booleans, arrays, objects → `-32600`). Valid ids
— including `0`, negative numbers, large integers, floats, and even the
empty string `""` — are echoed back verbatim. A well-formed but unknown
method remains `-32601`. Every rejection message names the offending
member, so clients can fix envelopes deterministically; each class is
pinned by test.

### Argument validation (schema-first, -32602)

Tool arguments are validated against the tool's declared `inputSchema`
**before** the handler runs: missing required fields, wrong types,
non-string enum values, and structurally malformed payloads (arrays where
objects are required, nulls where values are required) all return a single
standardized `-32602` (invalid params) error whose message names the
failing field — for example
`MCP argument validation failed at arguments.status: value is not allowed`.
Handler code never has to re-check types it declared in its schema, and
deeply nested payloads are capped by a depth guard. Note that schemas
deliberately do NOT set `additionalProperties: false`: unknown but
well-typed extra fields are ignored (standard JSON-Schema permissiveness),
so real clients may pass optional metadata without breaking — structural
violations still fail closed before the handler runs.

### Resource URIs are validated at the MCP boundary

`resources/read` accepts only `awh://<kind>[/single-segment-id]` where the
id may not contain path separators, traversal patterns (literal or
percent-encoded `..`), percent signs, NUL, or control characters, and the
total uri is length-capped. `awh://context` is the only segment-less form.
Malformed uris are rejected with `-32602` at the protocol boundary —
never passed downstream for the filesystem to reject — so traversal is a
protocol error, not an fs-layer concern (defense in depth: stores still
validate identifiers).

### Tool metadata and observability

Every static tool's `tools/list` entry carries the metadata from the
**canonical Tool Registry** (`src/mcp/tool_registry.rs`): `category`,
`version` (the tool's own evolution version), `schemaVersion` (the format
version of the `inputSchema` language — currently `1`, bumped only when
the schema language itself changes), `provider` (`awh`), `risk`
(`low`/`medium`/`high`), and `requiredPermissions` (labels from the same
`Permission` vocabulary the execution gate uses: `network`, `filesystem`,
`environment`, `process`, `secrets`). The registry is addressed by EXACT
tool name — there is no prefix inference anywhere: a tool that is not in
the registry is `uncategorized` rather than silently borrowing a category
from its name prefix, and an unregistered `workspace.something_new` never
masquerades as a workspace tool. A test pins that every tool in the
static catalog has a registry entry, so a tool cannot ship without
explicit metadata. All of this metadata is descriptive — for discovery and
observability; it is never a capability, permission, or security decision
(`risk` in particular is documentation for operators, and authorization
is unchanged by it).

Dynamic tools (from connector/custom MCP providers, addressed as
`provider.tool`) carry `category: "connector"`, the central AWH tool API
version as their `version` (until a provider declares its own),
`schemaVersion`, and the actual provider id as `provider`. Risk is
deliberately NOT emitted for dynamic tools: providers do not declare it
and AWH will not guess.

The dispatcher keeps bounded per-tool metrics (call counts, failure
counts, average duration — a fixed-size map, never a per-tool-name
allocation from untrusted input; all counters are CAS-loop saturating
atomics, so concurrent increments are never lost and overflow pins at
`u64::MAX` instead of wrapping to 0) exposed via the read-only
`mcp.status` tool alongside protocol versions, tool/provider counts, and
uptime. `mcp.status` reports the SERVER PROCESS state (`running`); it is
not a subsystem-health verdict — in-process subsystems (filesystem,
skills, memory) fail per-call and surface as tool errors.

### Dynamic provider tools: exposure and invocation gates

Connector/custom-MCP tools are validated at TWO independent layers, and
AWH never relies solely on the provider:

* **Exposure gate.** `tools/list` drops any dynamic tool whose advertised
  `inputSchema` is structurally malformed (a schema that is not an
  object, `required` that is not an array of strings, an uncompilable
  `pattern`, a nested subschema that is not an object, or keywords this
  validator does not support such as `$ref`). The rejection is per-tool —
  one bad advertisement never hides a provider's healthy siblings — and
  each drop is audited (`dynamic_tool_rejected`) and logged.
* **Invocation gate.** `tools/call` on a `provider.tool` name — and the
  equivalent generic `connector.invoke` path, which cannot be used to
  bypass the direct path — looks up the advertised schema and validates
  the arguments AWH-side BEFORE the provider is invoked. Mismatches fail
  closed with `-32602` and an audit deny (`tool_validation`); a schema
  that could not be advertised is also not invocable directly. Provider-
  side validation (e.g. the stdio client's own schema check) remains as
  defense-in-depth.

Lifecycle events (tool calls, resource reads, prompt requests,
notifications) are also delivered to registered **observer-only hooks**
(`McpEvent`). Hooks cannot veto, mutate, or reorder dispatch — they exist
for local observability, and a panicking hook is contained: the panic is
recorded, the hook is skipped, and the remaining hooks and the dispatch
itself proceed unaffected.

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
PASS tools/list (53 tools)
PASS every tool has an inputSchema
PASS tools/call workspace.context
PASS tools/call skills.list
PASS tools/call memory.store -> memory.search round-trip
PASS unknown tool -> JSON-RPC error (code -32603)
PASS clean disconnect (client.close)
STDIO INTEROP: ALL CHECKS PASSED

$ node examples/mcp-interop/sse-client.mjs
PASS SSE connect + initialize (server: agent-workspace-hub 0.1.0)   [HTTPS + bearer]
PASS SSE tools/list (53 tools)
PASS SSE tools/call workspace.context
PASS unknown sessionId rejected with 404
PASS wrong bearer token rejected with 401
PASS missing Authorization rejected with 401
PASS SSE client disconnect
PASS server exits on SIGTERM
SSE INTEROP: ALL CHECKS PASSED
```

> The recorded runs above were taken **without** `GITHUB_TOKEN`, so the 12
> `github.*` tools are hidden and 53 tools are advertised. With a token the
> same harnesses advertise 65 tools and pass identically — the count is
> expected to vary with the environment, which is why the harness prints it
> dynamically.

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
