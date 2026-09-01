# Architecture

Agent Workspace Hub (AWH) is a single Rust binary (`awh`) that gives AI
coding agents a secure, persistent workspace through the Model Context
Protocol (MCP).

## Why the Rust implementation exists

The repository contains a legacy Python implementation on `main`. The
`rust` branch is the **canonical production implementation**: type-safe,
deny-by-default, single-binary, and cross-platform. The Python code is
retained as historical reference only; it is not built, tested, or shipped.

```
main ────── legacy Python (reference only, unmaintained)
   └──── rust  ── canonical production implementation
```

Development happens on feature branches merged into `rust` via pull request.
`rust` is the release branch and must be branch-protected (see
[`security.md`](security.md) § Branch protection).

## Component map

```
src/
├── main.rs / cli          CLI entry point (clap): status, mcp, skill, registry
├── core/                  Storage engines (workspace-local state)
│   ├── workspace.rs       Project root resolution & context assembly
│   ├── project.rs         Per-project metadata
│   ├── memory.rs          Project memory store
│   ├── tasks.rs           Task store
│   ├── files.rs           Sandboxed file reads
│   └── context.rs         AGENTS.md-style context loading
├── models/                Shared typed domain models
├── skills/                Skill discovery / loading / management
└── mcp/                   The security-relevant surface
    ├── server.rs          StdioMcpServer: JSON-RPC over stdin/stdout
    ├── http.rs            HTTPS server: /health, /sse, /mcp (axum)
    ├── sse.rs             Session registry (SSE session lifecycle)
    ├── dispatcher.rs      McpDispatcher: shared tool dispatch (both transports)
    ├── auth.rs            Bearer-token authentication
    ├── permissions.rs     Centralized permission model (deny by default)
    ├── execution_gate.rs  Centralized gate every tool call passes through
    ├── sandbox.rs         bwrap sandbox command construction & limits
    ├── trust.rs / trust_store.rs / cli_trust.rs
    │                      Persistent trust records for external MCP servers
    ├── security.rs        Secret redaction & response sanitization
    ├── circuit_breaker.rs Failure isolation for external calls
    ├── audit.rs           Structured security audit events (allow + deny)
    ├── connectors.rs / composio*.rs / providers.rs
    │                      External connector surface (Composio et al.)
    ├── community_registry.rs
    │                      Community MCP registry client
    ├── custom_mcp.rs / global_mcp.rs
    │                      Custom & global external MCP server records
    ├── tls.rs             TLS acceptor (typed half-configured error)
    ├── config.rs          ResourceLimits + injectable env overrides
    └── error.rs           Deterministic JSON-RPC error codes
```

## Request flow

Every MCP request — from stdio or from the HTTPS/SSE transport — follows the
same path through the same dispatcher, so the two transports cannot drift
apart in behavior:

```
MCP client (OpenCode, Codex, Inspector, custom)
    │  (stdio: JSON-RPC lines)     (HTTP: POST /mcp?sessionId=…)
    ▼                                    ▼
StdioMcpServer ─────────────┬──── HTTP server (axum)
                            ▼
                     McpDispatcher
                            │
                     ExecutionGate  ── centralized: no tool dispatch
                            │          bypasses permission checks
              ┌─────────────┼─────────────┐
              ▼             ▼             ▼
         Permissions      Trust        Approval / limits
              └─────────────┴─────────────┘
                            ▼
                        Sandbox (bwrap) for subprocess execution
                            ▼
              Workspace / Skills / Tasks / Memory / Connectors
                            ▼
                        Audit (allow + deny events)
```

### Execution order for a tool call

1. **Transport framing** — line-length (stdio, default 10 MiB) or
   body-size limits (HTTP, default 10 MiB) reject oversized payloads before
   parsing.
2. **Authentication** (remote transport only) — constant-time bearer token
   comparison; `401` on missing/malformed/wrong token.
3. **Session resolution** (remote) — unknown `sessionId` → `404`.
4. **Dispatch** — `McpDispatcher` resolves the tool name; unknown tools and
   methods return deterministic JSON-RPC error codes, never panics.
5. **Execution gate** — permission, trust, approval, and resource-limit
   checks run centrally. A tool implementation cannot opt out.
6. **Sandbox** — subprocess execution runs under `bwrap` with
   read-only bind mounts of the project root and resource caps
   (CPU, memory, wall clock, output size).
7. **Sanitization** — responses pass through secret redaction before
   reaching the client.
8. **Audit** — every invoke (allowed) and every denial is emitted as a
   structured `tracing` event.

## Security layers (summary)

| Layer | Enforced by | Fails |
| --- | --- | --- |
| Authentication | `mcp/auth.rs` (remote) | closed |
| Authorization | `mcp/permissions.rs` | closed |
| Trust | `mcp/trust.rs`, `trust_store.rs` | closed |
| Approval/limits | `mcp/execution_gate.rs`, `config.rs` | closed |
| Sandbox | `mcp/sandbox.rs` | closed |
| Secret redaction | `mcp/security.rs` | closed |
| Audit | `mcp/audit.rs` | open telemetry |

Details: [`security.md`](security.md) and [`threat-model.md`](threat-model.md).

## Non-goals

- AWH does not implement an LLM agent loop; it is the workspace/tool layer
  that agent runtimes connect to.
- AWH does not expose arbitrary shell execution to MCP clients. Process
  execution exists only inside the sandbox and only for trusted, registered
  tools.
