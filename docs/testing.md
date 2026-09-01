# Testing

123 automated tests (unit + integration) plus two external interop harnesses
guard the `rust` branch. CI runs them on Ubuntu, macOS, and Windows.

## Running

```bash
cargo test --all-targets      # everything
cargo test --test mcp_http    # one suite
```

## Test map

The security suites deliberately mirror the phases of the production
hardening work. The suggested per-file split in the hardening plan is
implemented with consolidated files; the mapping is:

| Hardening-plan suite | Actual file | Coverage |
| --- | --- | --- |
| `mcp_protocol` | `tests/mcp_server.rs` | initialize handshake, tools/list, JSON-RPC error codes (-32600/-32601/-32603), malformed JSON, unsupported version, unknown method, unknown tool |
| `mcp_auth` | `tests/mcp_http.rs` | missing/invalid/wrong bearer token, malformed Bearer header, valid token acceptance, `/health` without auth |
| `mcp_sessions` | `tests/mcp_http.rs` | session limit (100), unknown session ID → 404, isolated per-session state |
| `mcp_paths` / `mcp_sandbox` | `tests/mcp_sandbox.rs` | relative project root rejected, absolute-path requirement, relative filesystem path rejected, fail-closed when `bwrap` missing, limit bounds validation |
| `mcp_permissions` / `mcp_approval` / `trust` | `tests/mcp_security.rs` | unknown MCP denied, blocked MCP denied, wrong version denied, extra permission denied, approved-then-allowed, revoked-then-denied, corrupted trust store fails closed, secret/environment permission checks |
| `mcp_limits` | `tests/mcp_http.rs` + `src/mcp/config.rs` unit tests | oversized body rejected, session cap, line-length cap, per-request timeout, injectable env overrides |
| `mcp_tls` | inline tests in `src/mcp/tls.rs` | typed error on half-configured TLS (cert-without-key and key-without-cert), valid acceptor |
| `mcp_memory` / `mcp_tasks` / `mcp_skills` | `tests/mcp_server.rs` | memory store→get round-trip + persistence, task create/list/update, skills catalog |
| `mcp_connectors` | `tests/mcp_server.rs` + `src/mcp/connectors.rs` | unknown qualified provider tool fails closed, connector store size limits |
| `mcp_audit` | inline tests in `src/mcp/audit.rs` | subscriber-capture test proving allow-side events reach the tracing subscriber |

## What each suite proves

- **`mcp_server.rs` (11 tests)** — protocol correctness: every malformed or
  unknown input yields a deterministic JSON-RPC error code, never a panic.
- **`mcp_http.rs` (10 tests)** — remote attack surface: authentication is
  mandatory and constant-time-wrong tokens behave identically to missing
  ones; oversized bodies and session floods are rejected.
- **`mcp_security.rs` (14 tests)** — authorization fails closed: an unknown,
  blocked, wrong-version, or over-permissioned MCP server is denied even
  before any tool runs; a *corrupted* trust store denies everything.
- **`mcp_sandbox.rs` (7 tests)** — sandbox construction fails closed on
  missing `bwrap`, non-absolute or nonexistent project roots, and invalid
  resource limits.
- **Inline unit tests (81)** — module-level invariants: permission model,
  execution gate, circuit breaker, secret redaction, config precedence,
  TLS state validation, connector limits, audit events.

## Interop harnesses (external, Node)

`examples/mcp-interop/` drives the real binary with the official MCP SDK and
Inspector — see [`mcp.md`](mcp.md) § Interoperability evidence. These run
outside `cargo test` because they need npm dependencies and a network-free
TLS setup; they are reproducible locally and documented step by step.

## Regression policy

Every security bug discovered during hardening became a test before the fix
was merged (for example: SSE session lifecycle bug → session-limit and
unknown-session tests; half-configured TLS panic → typed-error tests;
sandbox env mutation → injectable `wrap_command_with()`).

## Coverage philosophy

Tests focus on security behavior (fail-closed paths, limits, redaction) over
line coverage percentages: a line that no test exercises but that cannot
cause a security failure is lower priority than a fail-open path that has no
test.
