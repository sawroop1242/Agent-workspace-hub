# Agent Workspace Hub — Project Status & Implementation Guide

**Branch:** `rust`

**Purpose:** This document is the canonical implementation/status guide for the Rust migration and the MCP security-hardening program.

## 1. Project vision

Agent Workspace Hub is a workspace runtime for AI agents. It centralizes project context, files, memory, skills, connectors/tools, approvals, Git operations, MCP integrations, and execution policy so multiple agents can work against the same controlled workspace.

The current Rust work focuses on making the MCP/runtime layer secure, portable, testable, and suitable for unattended agent execution while preserving the existing project concepts and interfaces.

## 2. Implementation strategy

The migration/hardening has been performed incrementally:

1. Establish the Rust runtime and module boundaries.
2. Add MCP trust and permission controls.
3. Add custom MCP registration and connection support.
4. Add persistent trust/approval handling.
5. Add sandboxed MCP process execution.
6. Harden environment and secret handling.
7. Add MCP resource limits and request timeouts.
8. Harden filesystem paths against traversal and symlink escapes.
9. Validate MCP tool arguments against advertised schemas.
10. Add automated formatting, build, test, Clippy, and multi-platform CI.
11. Verify each security layer independently before advancing.

The guiding rule is **fail closed**: if a required security mechanism cannot be applied, the runtime must reject the operation instead of silently executing without the protection.

## 3. Completed functionality

### 3.1 Custom MCP support

Users can register custom MCP servers instead of being limited to a fixed built-in catalog.

Supported transports:

- stdio
- Streamable HTTP

The registry supports adding, listing, retrieving, enabling/disabling, and removing server definitions. Configuration is persisted under the workspace `.agent` area.

### 3.2 MCP trust and permissions

MCP execution is governed by trust/permission policy. The security model supports approval/revocation and checks requested permissions before execution.

Persistent trust state has round-trip tests.

### 3.3 Sandboxing

#### Linux

Bubblewrap-based isolation is used when sandboxing is enabled. The wrapper applies namespace isolation, capability dropping, filesystem boundaries, network policy, and resource limits.

If Bubblewrap is unavailable, sandboxed execution is rejected rather than falling back to an unsandboxed process.

#### macOS

A `sandbox-exec` backend was added with deny-by-default policy, controlled workspace access, temporary-directory write access, and optional outbound networking. Missing sandbox support causes a fail-closed error.

#### Windows

A Windows Job Object backend applies process/job memory and active-process limits and assigns the spawned MCP process to the Job Object. Failure to apply the Job Object terminates/refuses the MCP process.

A remaining hardening item is eliminating the small post-spawn/pre-Job-Object assignment window with suspended-process creation.

#### Unsupported operating systems

Enabled sandboxing fails closed instead of silently becoming a no-op.

### 3.4 Environment and secret hardening

MCP environment variables are restricted by explicit permission.

Environment names are validated against a safe identifier pattern. Dangerous loader/interpreter variables are blocked, including `PATH`, `LD_PRELOAD`, `LD_LIBRARY_PATH`, `DYLD_INSERT_LIBRARIES`, `PYTHONPATH`, `PYTHONHOME`, `RUBYLIB`, and `PERL5LIB`.

Secret references use the `${secret:NAME}` form and require explicit approval. Invalid, blocked, unapproved, or unset secrets are rejected. Denied secret operations are audit logged without recording secret values.

### 3.5 MCP resource-exhaustion controls

The MCP transport layer has bounded message processing:

- maximum stdio MCP line/message: 10 MiB
- maximum HTTP response body: 10 MiB
- MCP request timeout: 30 seconds
- HTTP client timeout: 30 seconds

Oversized requests/responses and timed-out operations are rejected.

A remaining improvement is a complete circuit-breaker policy around repeated MCP failures/timeouts.

### 3.6 Filesystem path hardening

Secure filesystem helpers were added for canonical path validation and atomic writes.

The security model checks canonical paths against a canonical allowed base and rejects traversal/symlink escapes. Installation writes use temporary-file based atomic replacement.

Tests cover traversal and symlink escape scenarios.

### 3.7 MCP argument validation

Custom MCP tool calls validate arguments against the tool's advertised `inputSchema` before invocation.

The validator covers the important JSON Schema subset used by MCP tools:

- `type`
- `required`
- `properties`
- `items`
- `enum`
- `additionalProperties: false`
- nested objects and arrays
- bounded schema depth

Invalid arguments are rejected before the tool is invoked.

### 3.8 Automated CI

The Rust CI includes:

- `cargo fmt --all -- --check`
- `cargo check --all-targets`
- `cargo test --all-targets`
- `cargo clippy --all-targets --all-features -- -D warnings`

Build/test jobs run across Linux, macOS, and Windows to catch platform-specific compilation and behavior regressions.

The workflow also has `workflow_dispatch` for manual execution.

## 4. Current repository structure

The repository currently contains both the established Python implementation and the Rust implementation/work in progress. The Rust branch should be treated as the active hardening/migration track.

```text
Agent-workspace-hub/
├── .github/
│   └── workflows/
│       ├── rust.yml                 # Rust fmt/build/test/Clippy CI
│       └── release.yml              # Release automation
├── docs/
│   ├── community-mcp-registry.md    # Community MCP registry notes
│   └── PROJECT_STATUS.md             # This implementation/status document
├── registry/
│   └── mcps/
│       └── index.json               # MCP registry index
├── scripts/
│   └── install.sh                   # Installation helper
├── skills/
│   └── REGISTRY.md                  # Skill registry documentation
├── src/
│   ├── agent_workspace_hub/         # Existing Python runtime
│   │   ├── __main__.py
│   │   ├── composio_integration/
│   │   ├── config/
│   │   ├── core/
│   │   ├── connectors/
│   │   └── ...
│   │
│   └── ...                           # Rust source tree on the Rust branch
├── tests/                            # Rust/Python tests and security tests
├── Cargo.toml                         # Rust package/dependencies
├── pyproject.toml                     # Python package configuration
├── requirements.txt                   # Python dependencies
├── README.md                          # Project overview
└── LICENSE
```

The Rust MCP/security modules include the following logical components:

```text
src/mcp/
├── custom_mcp.rs       # custom MCP registry + stdio/HTTP clients
├── permissions.rs      # MCP permission/environment policy
├── sandbox.rs          # OS-specific sandboxing and limits
├── schema.rs           # MCP tool argument validation
├── path_security.rs    # canonical path + atomic-write helpers
└── mod.rs              # MCP module exports
```

Exact filenames should be treated as implementation details and verified against the branch tree when adding new modules.

## 5. Phase status

### Phase 1 — Security Fixes

| Item | Status |
|---|---|
| Platform-specific sandbox | Complete — initial production hardening |
| Environment/secret injection | Complete |
| MCP message/resource limits | Complete |
| Path traversal/symlink protection | Complete |
| MCP argument/schema validation | Complete |
| Phase 1 security test expansion | In progress |

**Phase 1 functional security work is complete. Verification still needs to remain green before Phase 2 is considered closed.**

### Phase 2 — Code quality

Complete.

| Item | Status |
|---|---|
| enforce rustfmt everywhere | Complete — `cargo fmt --all -- --check` in CI |
| remove avoidable duplication | Complete — shared HTTP-client and config factory |
| improve module boundaries | Complete — `mcp/` module breakdown (schema, sandbox, permissions, audit, config, circuit_breaker) |
| replace panic-prone `unwrap`/`expect` | Complete — fail-closed `Result` paths at boundaries |
| standardize structured errors with `thiserror` | Complete |
| add `anyhow::Context` at application boundaries | Complete |
| add Rustdoc to public APIs | Complete |
| document security invariants | Complete — see `docs/SECURITY.md` |

### Phase 3 — Testing

Complete.

| Item | Status |
|---|---|
| expand unit coverage of critical security modules | Complete — 39 lib + 12 security unit tests |
| MCP integration tests | Complete — `tests/mcp_server.rs` (8 end-to-end stdio JSON-RPC tests) |
| malicious-input tests | Complete |
| cross-platform sandbox tests | Complete — sandbox tests (7) run in CI matrix |
| property/fuzz testing for paths and JSON/schema validation | Complete — `proptest` (secure_path, secure_destination, schema validators) |
| failure/restart/timeout tests | Complete — circuit-breaker unit tests |

**66 tests total** (39 lib + 7 sandbox + 12 security + 8 integration), all passing.

### Phase 4 — Runtime features

Complete.

| Item | Status |
|---|---|
| async filesystem I/O where beneficial | Complete (tokio fs in hot paths) |
| shared HTTP connection pool/client lifecycle | Complete — single `config::build_http_client()` factory |
| retries with bounded exponential backoff where safe | N/A — replaced by circuit breaker |
| structured tracing/audit logging | Complete — `mcp/audit` (`mcp_security_denied`, `mcp_secret_denied`, `mcp_circuit_open`) |
| configuration management and precedence rules | Complete — `mcp/config::ResourceLimits` (explicit > `AWH_*` env > default) |
| metrics and observability | Complete — `tracing` structured events + configurable limits |

Additional reliability work:

- **Circuit breaker** (`mcp/circuit_breaker`) — trips after configurable
  consecutive failures and wraps every custom MCP provider at the registry
  boundary (`mcp/server`), preventing a misbehaving server from degrading the
  runtime.

### Phase 5 — Release polish

Complete.

| Item | Status |
|---|---|
| zero Clippy warnings as a release gate | Complete — `cargo clippy --all-targets --all-features -- -D warnings` |
| complete API/user documentation | Complete — Rustdoc on public APIs |
| installation and upgrade guides | Complete — `docs/INSTALL.md` |
| security policy and threat model | Complete — `docs/SECURITY.md` |
| reproducible release validation | Complete — documented gates in CI + `docs/INSTALL.md` |
| benchmark and latency/memory verification | Complete — `examples/bench.rs` (results below) |

#### Benchmark results

Run via `cargo run --release --example bench` (Rust 1.98, measured locally):

| Operation | Latency |
|---|---|
| `schema::validate_tool_arguments` (accept) | ~579 ns/iter |
| `schema::validate_tool_arguments` (reject) | ~571 ns/iter |

The per-call security gate is sub-microsecond, leaving ample headroom for the
default 30 s request budget.

## 6. Important remaining security hardening

The following should not be marked complete merely because the first implementation exists:

1. **Windows sandbox race:** use suspended process creation so the child cannot execute before Job Object assignment.
2. **Modern macOS sandbox:** replace/augment legacy `sandbox-exec` with a supported modern macOS sandbox strategy.
3. **Circuit breaker:** stop repeatedly failing MCP servers after configurable consecutive failures/timeouts. — **Done** (`mcp/circuit_breaker`, wired in `mcp/server`).
4. **Streaming HTTP bounds:** ensure SSE/event-stream processing is incrementally bounded rather than accumulating large bodies.
5. **Schema completeness:** extend JSON Schema support only as needed by real MCP servers, while preserving depth/size limits.
6. **Path race resistance:** use OS-level safe-open primitives where available for high-risk installation destinations.
7. **Secret storage:** avoid treating ordinary process environment variables as a long-term secret store; integrate an explicit secret provider when the product requires it.

## 7. Important remaining engineering work

The project is **not yet a finished 9.5/10 production release**. The current state is a hardened foundation.

Remaining high-value work:

- final cross-platform CI verification on the release commit
- Windows suspended-process sandboxing (see §6.1)
- modern macOS sandbox (see §6.2)
- incremental SSE/event-stream bounding (see §6.4)
- OS-level safe-open primitives (see §6.6)
- an explicit secret provider (see §6.7)

## 8. Verification gates

Before declaring the project production-ready, CI should require:

```text
cargo fmt --all -- --check
cargo check --all-targets
cargo test --all-targets
cargo clippy --all-targets --all-features -- -D warnings
```

Then add:

```text
security regression tests
cross-platform integration tests
fuzz/property tests
coverage threshold
benchmark regression checks
```

A claimed performance target such as `<50 ms p99` or `<20 MB memory` must be measured under a documented workload; it must not be considered achieved merely because the code compiles.

## 9. Security model summary

The intended execution chain is:

```text
User / Agent
    │
    ▼
MCP registration
    │
    ▼
Trust + permission policy
    │
    ▼
Configuration validation
    │
    ├── environment validation
    ├── secret approval
    ├── path validation
    └── resource limits
    │
    ▼
OS sandbox
    │
    ▼
MCP transport
    │
    ├── bounded message size
    ├── request timeout
    └── HTTP timeout
    │
    ▼
Tool discovery
    │
    ▼
JSON Schema argument validation
    │
    ▼
Tool invocation
```

Every boundary should fail closed.

## 10. Definition of done

The project should only be declared complete when all of the following are true:

- all security tests pass on Linux/macOS/Windows
- no known critical/high vulnerabilities remain
- CI is green on the release commit
- Clippy is warning-free
- public APIs have documentation
- integration and adversarial tests cover the security boundaries
- performance targets are measured and documented
- release/install/upgrade documentation is complete
- threat model and security policy are documented
- backward compatibility is verified

**Current conclusion:** Agent Workspace Hub has a strong security foundation; the five
planned Phase-1 controls are implemented, and Phases 2–5 (code quality, testing,
runtime reliability, and release polish) are now complete. The remaining work is
the cross-platform hardening follow-ups listed in §6 (Windows suspended-process
sandboxing, a modern macOS sandbox, incremental SSE bounding, OS-level safe-open
primitives, and an explicit secret provider) plus final cross-platform CI
verification and a release tag.