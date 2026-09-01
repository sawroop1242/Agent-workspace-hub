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
| Remote transport auth | Mandatory bearer-token auth for the HTTP/SSE transport; constant-time comparison; fail closed when the API key is unset | `mcp/auth` |
| Remote transport TLS | TLS 1.2+ with certificate/key material loaded from files; half-configured TLS rejected | `mcp/tls` |
| Remote request limits | Bounded HTTP body, connection, session, and timeout limits; per-session isolation | `mcp/http`, `mcp/sse` |

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
| `AWH_API_KEY` | Bearer token required for remote (HTTP/SSE) MCP access |
| `AWH_HOST` | Remote transport bind address (default `0.0.0.0`) |
| `AWH_PORT` | Remote transport port (default `8443`) |
| `AWH_TLS_CERT` | Path to PEM certificate chain (enables HTTPS) |
| `AWH_TLS_KEY` | Path to PEM private key (enables HTTPS) |
| `AWH_ALLOWED_ORIGINS` | CORS allow-list (empty disables CORS) |

## Remote transport security

The remote (HTTP/SSE) transport is disabled by default; stdio is the only
transport enabled unless `awh mcp serve --transport sse` is invoked. Remote MCP
enforces the following, all of which fail closed:

- **Authentication is mandatory.** The expected API key is read from `AWH_API_KEY`
  (or the variable named by `--api-key-env`). If it is unset or empty, the server
  refuses to start. Comparisons use a constant-time routine (`subtle`).
- **TLS private keys are never logged.** Error paths reference only the file path,
  never the key contents. API keys and `Authorization` headers are likewise never
  written to logs or errors.
- **No secret leakage in errors.** HTTP error bodies are generic (`401 Unauthorized`,
  `404 unknown session`, `400 malformed ...`). Filesystem paths, environment
  variables, and stack traces are not exposed.
- **Per-session isolation.** Each SSE connection is a distinct session with its own
  broadcast channel; one client cannot observe another's responses.
- **Limits.** Request body size, session count, request timeout, and SSE idle/keep-alive
  timeouts are bounded to resist resource exhaustion.
- **CORS is restrictive by default.** No `Access-Control-Allow-Origin` is emitted
  unless `AWH_ALLOWED_ORIGINS` is configured with an explicit allow-list.

### Never log these

- TLS private key bytes.
- API keys (`AWH_API_KEY` or the configured variable's value).
- `Authorization` header values.
- MCP secret values resolved from `${secret:NAME}` references.

## Audit logging

All denied security decisions and circuit-breaker trips emit structured `tracing`
events with a stable `event` field (`mcp_security_denied`, `mcp_secret_denied`,
`mcp_circuit_open`). Secret values are never included; only secret *names* and
stable reason slugs are logged.

Allow-side events are audited as well (`mcp_audit`), covering successful HTTP
authentication, SSE session creation and destruction, every `tools/call`
invocation (tool name only, never arguments), and external connector
invocation (provider and tool name only).

## Branch protection (manual configuration required)

The production branch is **`rust`**. It must be protected so nothing reaches it
without passing the required CI checks. The automation token available to the
hardening workflow cannot call the branch-protection API (the
`Administration: write` permission is not granted), so an administrator must
configure the following **manually** under
*Settings → Branches → Branch protection rules → Add rule → Branch name pattern:
`rust`*:

| Setting | Required value | Reason |
| --- | --- | --- |
| Require a pull request before merging | Enabled | No direct pushes to production |
| Require approvals | 1 (or 2 for stricter policy) | Human review gate |
| Dismiss stale pull request approvals | Enabled | Approvals do not survive new pushes |
| Require review from code owners | Enabled, if `CODEOWNERS` present | Named accountable reviewers |
| Require status checks to pass | Enabled | Blocks merges that fail CI |
| Required checks | `fmt`, `build-test (ubuntu-latest)`, `build-test (macos-latest)`, `build-test (windows-latest)`, `clippy`, `audit` | The four jobs defined in `.github/workflows/rust.yml` |
| Require branches to be up to date | Enabled | Merges must include the latest production code |
| Require conversation resolution | Enabled | No silently dropped review threads |
| Require signed commits | Recommended | Provenance of production code |
| Require linear history | Recommended | Clean, revertible history |
| Include administrators | Enabled | The rule applies to everyone |
| Restrict force pushes / deletions | Both denied | Production history is immutable |

The `audit` check fails the merge on any RUSTSEC vulnerability found in
`Cargo.lock`; warning-level advisories (unmaintained/yanked transitive crates)
are surfaced but do not block.

Verification: after applying the rule, `GET /repos/{owner}/{repo}/branches/rust/protection`
should return the settings above, and a direct push to `rust` should be rejected
with *"Protected branch update failed"*.