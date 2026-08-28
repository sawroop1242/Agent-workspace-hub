# Security Policy and Threat Model

This document describes the security posture of Agent Workspace Hub (the Rust
`awh` binary), the threats its MCP execution surface is intended to resist, and
how to report vulnerabilities.

## Reporting a vulnerability

Do **not** open a public issue for a suspected security vulnerability. Report it
privately so a fix can be prepared and released before public disclosure.

- Preferred: GitHub Security Advisories on this repository (Private vulnerability
  reporting).
- Alternatively: email the maintainers with affected version, a reproduction or
  logs, and impact.

We will acknowledge within 3 business days and aim to publish a fix and advisory
within 30 days of confirmation.

## Supported versions

Only the latest release on the default branch (`rust`) is supported. No backport
branches are currently maintained.

## Threat model

Agent Workspace Hub launches and drives third-party MCP servers (stdlib and
streamable HTTP) on behalf of an agent. It is therefore treated as a security
boundary between:

1. An untrusted (or merely misbehaving) MCP server, and
2. The host filesystem, process environment, and secrets.

The core assumption: **an MCP server is untrusted input.** Every boundary below
fails closed.

### Assets

| Asset | Risk |
|---|---|
| Workspace files outside the sandbox root | Unauthorized read/write |
| Host environment variables / secrets | Exfiltration, credential theft |
| Agent identity / authorization state | Privilege escalation |
| Availability of the hub runtime | DoS via runaway MCP server |

### Trust boundaries and mitigations

Each row maps a boundary to the implemented control and its source module.

| Boundary | Control | Module |
|---|---|---|
| Server registration | Per-server enable flag; trust registry | `mcp/trust`, `mcp/trust_store` |
| Environment injection | Allow-list of permitted env vars; blocked-name deny-list; secret references require explicit approval | `mcp/permissions` |
| Secret resolution | `${secret:NAME}` references are validated and only expanded for explicitly-approved names; secret *values* are never logged | `mcp/custom_mcp`, `mcp/audit` |
| Path access | Secure path resolution; symlink/traversal rejection; safe destination checks | `src/secure_path` (security module) |
| OS sandbox | Per-platform sandbox: Windows Job Objects, sandbox-exec on macOS | `mcp/sandbox` |
| Message size | Bounded MCP line/JSON and HTTP body size (default 10 MiB) | `mcp/config`, `mcp/custom_mcp` |
| Request duration | Per-request and HTTP client timeouts (default 30 s) | `mcp/config` |
| Argument validation | JSON Schema validation of tool arguments with depth/size limits | `mcp/schema` |
| Tool authorization | Execution gate approves/denies tool calls | `mcp/execution_gate` |
| Provider resilience | Circuit breaker trips after configured consecutive failures | `mcp/circuit_breaker` |
| Auditability | Structured audit events for every denied decision and circuit trip | `mcp/audit` |

### Out of scope / residual risks

The following are accepted as out of scope for the current release and are noted
for future hardening:

1. **Windows sandbox race** — the child process may briefly execute before the Job
   Object is assigned. Mitigation planned: suspended process creation.
2. **macOS sandbox** — legacy `sandbox-exec` is used; a modern macOS sandbox
   strategy is a follow-up.
3. **Secret storage** — ordinary environment variables are used as an MCP-level
   secret source, not a long-term secret provider. A dedicated secret provider is a
   future enhancement.
4. **Streaming HTTP** — SSE/event-stream bodies must continue to be incrementally
   bounded rather than accumulated (see `mcp/custom_mcp`).

## Secure defaults

- Fail closed: any denied decision is a hard error, never a silent passthrough.
- Default message/body limit: **10 MiB**.
- Default request/HTTP timeout: **30 seconds**.
- Default circuit-breaker threshold: **5** consecutive failures; cooldown **30 s**.
- These are overridable via `AWH_*` environment variables with documented
  precedence (explicit config > env var > default). See `mcp/config`.

## Configuration precedence

1. Explicit in-code/config value.
2. `AWH_*` environment variable override.
3. Built-in conservative default.

Tunable variables:

| Variable | Purpose |
|---|---|
| `AWH_MAX_MCP_LINE_BYTES` | Stdio MCP message size limit |
| `AWH_MAX_HTTP_BODY_BYTES` | HTTP response body limit |
| `AWH_MCP_REQUEST_TIMEOUT_SECS` | Per-request MCP timeout |
| `AWH_HTTP_CLIENT_TIMEOUT_SECS` | HTTP client timeout |
| `AWH_CIRCUIT_FAILURE_THRESHOLD` | Consecutive failures before trip |
| `AWH_CIRCUIT_COOLDOWN_SECS` | Open-circuit cooldown |

## Audit logging

All denied security decisions and circuit-breaker trips emit structured `tracing`
events with a stable `event` field (`mcp_security_denied`, `mcp_secret_denied`,
`mcp_circuit_open`). Secret values are never included; only secret *names* and
stable reason slugs are logged.