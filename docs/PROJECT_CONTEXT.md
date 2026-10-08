# Agent Workspace Hub — Canonical Project Context

> **Purpose:** This file is the long-lived context for developers and AI agents working on Agent Workspace Hub. Read this before making architectural changes.
>
> **Active engineering track:** `rust`
>
> **Status:** Security-hardened foundation; Phase 2 Code Quality & Reliability in progress. The project is **not yet declared production-complete**.

---

## 1. What this project is

Agent Workspace Hub is a security-first workspace runtime for AI agents.

The core idea is to give agents one controlled workspace in which they can work with:

- project files
- skills/custom instructions
- memory/context
- external connectors and tools
- custom MCP servers
- Git repositories
- task/execution state
- trust and permission policies

Instead of allowing every agent or MCP server to access everything directly, the Hub acts as a policy and execution boundary.

### Main product principle

```text
Agent
  ↓
Workspace Hub
  ↓
Policy / Trust / Permissions
  ↓
Validated resource access
  ↓
Sandboxed execution
  ↓
External tools / MCP / files
```

The Hub should make agent capabilities **composable without making permissions implicit**.

---

## 2. Why the project exists

AI agents increasingly need to use files, GitHub, databases, APIs, browsers, MCP servers, and user-defined tools. A raw collection of integrations creates several problems:

1. Every agent needs its own connector implementation.
2. Context becomes fragmented between agents.
3. MCP servers can receive more privileges than they need.
4. User-defined tools are difficult to govern consistently.
5. A compromised tool can become a path to the host filesystem or secrets.
6. There is no single place to audit trust, permissions, execution, and failures.

Agent Workspace Hub is intended to solve this by providing a common workspace and security boundary.

---

## 3. Architectural vision

The long-term architecture is layered.

```text
┌──────────────────────────────────────────────────────────┐
│                    Agent / CLI / UI                      │
└──────────────────────────┬───────────────────────────────┘
                           │
┌──────────────────────────▼───────────────────────────────┐
│                 Workspace API / Runtime                  │
│  context · files · memory · skills · tasks · connectors │
└──────────────────────────┬───────────────────────────────┘
                           │
┌──────────────────────────▼───────────────────────────────┐
│                 Security / Policy Layer                  │
│ trust · permissions · approvals · secrets · validation  │
└──────────────────────────┬───────────────────────────────┘
                           │
┌──────────────────────────▼───────────────────────────────┐
│                    Execution Layer                      │
│ sandbox · resource limits · timeouts · audit · retries │
└──────────────────────────┬───────────────────────────────┘
                           │
        ┌──────────────────┼──────────────────┐
        ▼                  ▼                  ▼
      Files              MCP              Connectors
        │                  │                  │
        ▼                  ▼                  ▼
    Workspace          Custom tools       External APIs
```

### Security invariant

Every capability should pass through an explicit policy boundary before execution.

**Never silently downgrade security.** If sandboxing, validation, authorization, or another mandatory control cannot be applied, reject the operation.

---

## 4. Current implementation strategy

The implementation has deliberately been incremental.

### Stage A — Establish the Rust security/runtime foundation

- Define Rust module boundaries.
- Preserve existing project concepts while introducing the hardened Rust track.
- Add MCP-specific trust and permission primitives.
- Keep security-sensitive logic isolated into small modules.

### Stage B — Custom MCP support

Allow users to register their own MCP servers.

Supported transports currently include:

- stdio
- Streamable HTTP

The registry supports:

- add
- list
- get
- remove
- enable/disable
- persisted configuration

The MCP runtime must not assume that a server is trusted merely because the user registered it.

### Stage C — Trust and permissions

MCP execution uses trust and permission checks.

The model separates concepts such as:

- whether a server is trusted
- which capabilities it may use
- which environment variables it may receive
- which secrets it may resolve
- whether the server is revoked

Persistent approval state has round-trip coverage.

### Stage D — OS sandboxing

The MCP process is isolated according to the host OS.

#### Linux

Use Bubblewrap where configured to provide namespace/filesystem/network/capability isolation and resource limits.

#### macOS

Use a sandbox backend with deny-by-default policy and explicitly controlled workspace/temp/network access.

#### Windows

Use Windows Job Objects for process/resource isolation.

### Fail-closed rule

If sandboxing is mandatory and the required backend cannot be applied, the process must not continue unsandboxed.

Known remaining Windows hardening: eliminate the small post-spawn/pre-Job-Object assignment window using suspended-process creation.

---

## 5. Security controls already implemented

### 5.1 Environment and secret hardening

Environment variables are permission-gated and validated.

Environment names follow a safe identifier pattern:

```text
[A-Za-z_][A-Za-z0-9_]*
```

Dangerous loader/interpreter variables are blocked, including:

```text
PATH
LD_PRELOAD
LD_LIBRARY_PATH
DYLD_INSERT_LIBRARIES
PYTHONPATH
PYTHONHOME
RUBYLIB
PERL5LIB
```

Secret references use an explicit form such as:

```text
${secret:API_TOKEN}
```

A secret must be explicitly approved. Invalid, blocked, unapproved, or unset secrets fail rather than silently expanding to an empty value.

Audit events must never contain the actual secret value.

### 5.2 MCP resource limits

Current transport limits include:

```text
Maximum stdio MCP message: 10 MiB
Maximum HTTP response body: 10 MiB
MCP request timeout: 30 seconds
HTTP client timeout: 30 seconds
```

Oversized messages and timeouts are rejected.

A full circuit-breaker policy is still planned.

### 5.3 Filesystem security

Security helpers canonicalize and validate paths against an allowed base.

They are intended to prevent:

- `../` traversal
- absolute-path escapes
- symlink escapes
- installation outside the permitted workspace

Installation writes use temporary-file based atomic replacement.

Further OS-specific safe-open/race-resistant primitives are planned for high-risk paths.

### 5.4 MCP argument validation

Before `tools/call`, the Hub obtains the tool's advertised `inputSchema` and validates arguments.

The current validator covers the practical subset needed by the implementation:

- type
- required
- properties
- items
- enum
- `additionalProperties: false`
- nested objects/arrays
- bounded schema depth

The validator rejects malformed or unauthorized arguments before tool execution.

### 5.5 CI quality/security gates

The Rust CI is intended to enforce:

```text
cargo fmt --all -- --check
cargo check --all-targets
cargo test --all-targets
cargo clippy --all-targets --all-features -- -D warnings
```

The project also uses cross-platform CI and has a manual workflow trigger.

---

## 6. Current Rust module responsibilities

The important MCP/security modules are conceptually:

```text
src/mcp/
├── mod.rs
│   └── MCP module exports
│
├── custom_mcp.rs
│   ├── custom MCP registry
│   ├── stdio transport
│   ├── Streamable HTTP transport
│   ├── bounded requests/responses
│   └── tool-call validation integration
│
├── permissions.rs
│   ├── MCP permissions
│   ├── environment policy
│   ├── secret approval
│   └── dangerous-variable blocking
│
├── sandbox.rs
│   ├── platform sandbox selection
│   ├── resource limits
│   ├── Linux isolation
│   ├── macOS isolation
│   └── Windows Job Object handling
│
├── schema.rs
│   └── MCP JSON argument validation
│
└── path security helpers
    ├── canonical path validation
    └── atomic writes
```

Exact module filenames should be checked against the current branch before adding new abstractions. Do not create duplicate security utilities when an existing primitive can be extended safely.

---

## 7. Project phases

## Phase 1 — Security Fixes

### Completed implementation

- [x] Platform-specific sandboxing
- [x] Fail-closed sandbox behavior
- [x] Environment-variable validation
- [x] Dangerous environment-variable blocking
- [x] Explicit secret approval
- [x] MCP message-size limits
- [x] MCP request timeouts
- [x] HTTP response-size limits
- [x] Path traversal protection
- [x] Symlink escape protection
- [x] Atomic filesystem installation writes
- [x] MCP tool argument validation

### Verification still required

Implementation completion does not mean the final release is security-certified. Continue regression, integration, adversarial, cross-platform, and fuzz/property testing.

---

## Phase 2 — Code Quality & Reliability

**Current phase.**

### Completed so far

- [x] Readability/rustfmt cleanup started
- [x] MCP schema validator formatted
- [x] Custom MCP client formatted
- [x] Initial `unwrap`/`expect`/panic audit found no obvious production matches in the searched Rust code

### Next work

1. Introduce structured domain errors where callers need to distinguish error classes.
2. Use `thiserror` for stable library/domain error types.
3. Use `anyhow::Context` at application/runtime boundaries.
4. Remove duplicated validation/error formatting.
5. Establish clean module ownership and APIs.
6. Add Rustdoc to public interfaces.
7. Document security invariants next to security-critical code.
8. Run the complete CI gate after each coherent refactor.

Do not replace every `anyhow::Error` mechanically. Use typed errors where program logic needs to match on an error category; use `anyhow` where the error is primarily propagated to an application boundary.

---

## Phase 3 — Comprehensive Testing

Planned:

- critical-module unit tests
- MCP integration tests
- malicious-input tests
- sandbox behavior tests per OS
- path traversal property tests
- schema fuzz/property tests
- timeout and process-crash tests
- persistent trust tests
- connector failure tests
- coverage measurement
- regression gates

Target: **>80% meaningful test coverage**, with security-critical paths receiving stronger targeted coverage rather than relying only on aggregate percentage.

---

## Phase 4 — Runtime Features

Planned:

### Async filesystem I/O

Review synchronous filesystem operations and migrate appropriate runtime paths to `tokio::fs`.

Do not migrate security-sensitive code blindly: filesystem atomicity and platform-specific guarantees must be preserved.

### HTTP connection pooling

Use shared `reqwest::Client` instances where lifecycle and isolation allow it.

Add bounded retries with exponential backoff only for operations that are safe to retry.

### Structured logging

Use `tracing` consistently.

Log:

- request IDs
- MCP server IDs
- operation names
- authorization decisions
- timeout/failure events
- security denials

Never log:

- secrets
- authentication tokens
- private file contents
- raw sensitive tool arguments unless explicitly redacted

### Configuration management

Introduce a validated configuration model with clear precedence, for example:

```text
CLI > environment > configuration file > defaults
```

The exact precedence must be documented before implementation.

### Metrics

Add metrics for:

- MCP calls
- validation failures
- authorization denials
- timeouts
- process failures
- latency
- resource-limit events

---

## Phase 5 — Release Polish

Planned:

- zero Clippy warnings as a release gate
- complete Rustdoc
- architecture documentation
- installation guide
- configuration guide
- MCP developer guide
- security policy
- threat model
- release/upgrade guide
- reproducible verification
- benchmark suite
- memory/resource benchmarks

Performance targets such as `<50ms p99` latency and `<20MB` memory must be measured against a documented workload. They must not be claimed merely because the implementation compiles.

---

## 8. Threat model

The most important threat is an agent-controlled or externally supplied MCP server being more powerful than intended.

### Threats addressed

```text
MCP process escapes workspace
    → sandbox + path validation

MCP reads dangerous host environment
    → environment allowlist + blocked variables

MCP steals approved secrets indirectly
    → explicit secret approval + audit logging

MCP sends huge request/response
    → 10 MiB bounds

MCP hangs forever
    → 30s timeout

MCP receives malicious tool arguments
    → inputSchema validation

MCP writes through ../ or symlink
    → canonical path validation + atomic write

MCP remains trusted after revocation
    → trust-store authorization checks
```

### Threats still requiring hardening

- Windows spawn-to-Job-Object race
- robust streaming HTTP bounds
- full circuit breaker
- OS-level race-resistant filesystem opening
- secret-provider architecture
- broader adversarial/fuzz testing

---

## 9. Design rules for future contributors and AI agents

These rules are mandatory unless an architectural decision explicitly changes them.

### Rule 1 — Security before convenience

Never disable a security mechanism simply to make a test or integration work.

### Rule 2 — Fail closed

If a required sandbox, permission, validation, or policy check cannot execute, reject the operation.

### Rule 3 — Least privilege

MCP servers should receive only the permissions they need.

### Rule 4 — Validate before execution

Validate configuration, permissions, paths, environment variables, and tool arguments before invoking external code.

### Rule 5 — Never leak secrets

Do not put secrets in errors, logs, debug output, test snapshots, or documentation.

### Rule 6 — Preserve security invariants during refactoring

A refactor is not successful if it merely makes code cleaner while weakening a security boundary.

### Rule 7 — Avoid duplicate primitives

Search the repository before adding a new permission check, path validator, error type, logging helper, or MCP transport abstraction.

### Rule 8 — Keep dependencies purposeful

Add a dependency only when it provides a clear benefit and does not unnecessarily enlarge the trusted computing base.

### Rule 9 — Test the failure path

Security code needs negative tests: denied permission, malformed input, missing sandbox, oversized messages, traversal, symlink escape, timeout, revoked trust, and process failure.

### Rule 10 — CI before phase advancement

Do not move to the next major phase merely because code was written. Run the relevant tests and CI gates first.

---

## 10. Recommended implementation workflow

For every security/runtime change:

```text
1. Read this context file.
2. Inspect the existing implementation.
3. Identify the security invariant affected.
4. Make the smallest coherent change.
5. Add/modify tests for both success and failure.
6. Run formatting.
7. Run check/build.
8. Run tests.
9. Run Clippy.
10. Inspect the diff.
11. Update project documentation/status.
12. Commit with a focused message.
13. Verify CI.
14. Only then advance to the next task.
```

---

## 11. Definition of complete project

Agent Workspace Hub should not be called complete until:

- [ ] all security tests pass on Linux, macOS, and Windows
- [ ] no known critical/high security issue remains
- [ ] sandbox behavior is verified on supported platforms
- [ ] MCP integration tests are comprehensive
- [ ] adversarial/property/fuzz tests cover critical boundaries
- [ ] Clippy is warning-free
- [ ] public APIs are documented
- [ ] error architecture is consistent
- [ ] configuration behavior is documented
- [ ] observability is implemented
- [ ] performance targets are measured
- [ ] memory/resource targets are measured
- [ ] installation and upgrade documentation is complete
- [ ] security policy and threat model are published
- [ ] backward compatibility is verified
- [ ] release CI is green

---

## 12. Current decision

The project should continue with **Phase 2: structured error handling and reliability cleanup**.

The immediate architectural goal is:

```text
Low-level security/domain layer
        │
        ▼
Typed, matchable errors (`thiserror`)
        │
        ▼
Runtime/application boundary
        │
        ▼
Context-rich propagation (`anyhow::Context`)
        │
        ▼
CLI / API / agent-visible error
```

This gives the project precise programmatic error handling without forcing every internal function into an unnecessarily large custom error hierarchy.

---

## 13. Relationship to other documentation

`docs/PROJECT_STATUS.md` records the current implementation/status snapshot.

This file, `docs/PROJECT_CONTEXT.md`, is the **architectural memory and continuation guide**. It explains what the system is, why the security controls exist, how the implementation is intended to evolve, and how future developers/AI agents should continue the work.

When implementation and documentation disagree, verify the current `rust` branch and update both documents rather than assuming either is authoritative.
