# 26 — Remote AWH (REM-001)

> Standalone implementation contract for the current rust branch. This prompt describes the implementation work; it does not claim that the feature is already complete.

## Prompt identity
**Prompt:** 26 / REM-001  
**Feature:** Remote AWH  
**Primary roadmap area:** Remote/local-to-remote workspace control  
**Target branch:** rust  
**Canonical folder:** docs/remote/

### Mission
Implement, converge, and verify Remote AWH inside AWH's documented product boundary.

This prompt is standalone:
- inspect current rust before changing code;
- reuse existing contracts;
- do not require another prompt or PR to be merged first;
- do not create duplicate stores, services, authorization, policy, filesystem, edit, snapshot, rollback, audit, Git, MCP, or identity systems;
- keep external-agent reasoning/model selection outside AWH.

# 1. Scope

## In scope
- Define remote AWH using the existing Control API and remote TUI/backend concepts.
- Secure endpoint configuration, authentication, workspace/session scope, bounded request/response and reconnect semantics.
- Preserve local-first behavior; remote mode is explicit.
- Preserve server-side policy, identity and audit.

## Out of scope
- No cloud multi-tenant platform.
- No second RPC protocol.
- No implicit public bind/tunnel.
- No distributed scheduler.

## Product boundary
Remote AWH is a deployment/transport adapter around Control API/application services. Server-side AWH remains authoritative.

# 2. Required repository forensics

Read at minimum:
Cargo.toml, README.md, AGENTS.md, docs/PROJECT_CONTEXT.md, docs/FEATURES.md, docs/architecture.md, docs/security.md, docs/threat-model.md, docs/CLI.md, docs/roadmap/PROJECT_ROADMAP.md, docs/roadmap/GROWTH_STRATEGY.md, docs/roadmap/ROADMAP_AGENT_PROFILES_POLICY_MCP.md, and implementation prompts 01–17.

Inspect:
src/skills/remote.rs, src/tui/remote.rs, src/tui/screens/remote.rs, src/api/control.rs, HTTP client/config/resource-limit code.

Search before adding abstractions:
rg -n "Remote|remote backend|RemoteBackend|base_url|Control API|api/v1|remote"
rg -n "TODO|FIXME|unimplemented!|todo!|panic!|unwrap\(|expect\("

Record which existing contracts are reused, extended, adapted, or replaced and why. Missing prompt coverage is not evidence of missing Rust implementation.

# 3. Architecture

## Canonical responsibility
Remote client → authenticated transport → Control API → server authorization → canonical services. URL paths never prove authorization.

### Architectural invariants
1. One canonical domain/service owner.
2. One canonical model and persistence authority where persistence is required.
3. Existing workspace, agent, session, worktree and project identity remain authoritative.
4. PolicyEngine/capability authorization remains authoritative.
5. Existing FilesService/EditService/Snapshot/Rollback/Audit/Git/MCP boundaries are reused.
6. Interface adapters do not create alternate business semantics.
7. Structured errors distinguish validation, authorization, state, transport and execution failures.
8. Existing locking/concurrency primitives are reused.
9. Restart and corruption behavior are explicit.
10. Source existence is not treated as verification evidence by itself.

## State and lifecycle
Configured → Connecting → Authenticated → Active → Reconnecting/Disconnected → Closed. Reconnect reauthenticates and revalidates scope.

# 4. Interfaces

## Rust/service interface
Define connection/session/config/error types distinguishing transport, authentication, remote authorization and domain errors.

## CLI interface
If CLI remote config/connection exists, make it explicit; never print credentials.

## Control API interface
Remote mode consumes the versioned Control API; define HTTPS policy, timeouts, response limits, retries and idempotency.

## MCP interface
If exposed through MCP, reuse existing MCP security or adapt through Control API; never create an unauthenticated remote protocol.

## TUI interface
Existing remote TUI backend uses the same API and refreshes scope after reconnect.

All applicable interfaces must call the same canonical service/domain implementation.

# 5. Security

Security requirements are implementation requirements.

## Authorization
Require explicit credentials and server-side authorization. Local config is not a remote grant.

## Input/resource safety
Validate URL scheme, redirects, request/response limits and timeouts; prevent credential leakage on redirects.

## Isolation
Remote session/workspace identity is explicit; local IDs are not authorization proof.

## Secrets and sensitive data
Use existing secret/config authority; never log tokens/cookies/raw secret responses.

## Required threat cases
Test unauthorized callers, forged/mismatched identity, cross-scope access, malformed input/state, resource exhaustion, replay, concurrent mutation, partial failure, crash/restart, and secret leakage.

# 6. Persistence and recovery

Persist only endpoint/config metadata; sessions/caches are disposable unless versioned.

For durable state prove:
write/publication → process termination → new process → reload → verification.

Corrupt, truncated, incompatible, or partially published state must fail closed or follow an explicitly documented recovery rule. If another subsystem owns persistence, use that owner.

# 7. Integration boundaries

| Boundary | Existing authority | Remote AWH responsibility |
|---|---|---|
| Workspace/project | init/workspace services | resolve/enforce scope |
| Agent/session | AgentRuntimeService | propagate identity |
| Capability/policy | PolicyEngine/authorization | enforce consequential actions |
| Filesystem | FilesService | reuse path/resource safety |
| Edit/recovery | EditService/Snapshot/Rollback | reuse canonical mutation/recovery |
| Audit | persistent AuditLog | emit correlated redacted events |
| Git/worktree | Git/worktree services | reuse where applicable |
| MCP | MCP dispatcher/session | adapter only |

## No-duplication rule
Do not introduce parallel identity, auth, policy, filesystem safety, edit, snapshot, rollback, audit, Git, MCP session, or persistence systems. Use adapters at boundaries.

# 8. Testing

## Unit tests
Test URL parsing, reconnect state, timeout and error mapping.

## Integration tests
Real authenticated HTTP against a local AWH server; disconnect/reconnect, token expiry, scope mismatch and mutation.

## Security/adversarial tests
Downgrade HTTP, redirect leakage, token leakage, scope confusion, oversized response/request, unsafe public bind.

## Failure/recovery tests
Network reset, server restart, unknown completion of non-idempotent operation, token expiry.

## Compatibility/real-interface tests
Control API, remote TUI/backend and existing HTTP limits.

## Verification gates
cargo fmt --all -- --check
cargo check --all-targets
cargo test --all-targets
cargo clippy --all-targets --all-features -- -D warnings
git diff --check

If an external service, platform, terminal emulator, remote target, or release environment is unavailable, record missing evidence instead of claiming verification.

# 9. Rollout

## Compatibility
Remote mode must not change local behavior. Version API compatibility and never auto-downgrade HTTPS to HTTP.

## Rollout order
1. Inspect current contract and implementation.
2. Establish/extend canonical service and model.
3. Establish/migrate persistence only where required.
4. Integrate existing identity and security boundaries.
5. Add interface adapters.
6. Add unit/integration/security/recovery tests.
7. Run verification gates.
8. Update feature-specific docs/status only for behavior actually proven.

No step depends on another prompt or PR being merged first.

## Failure behavior
Do not retry unknown non-idempotent completion without idempotency. Auth/TLS failures fail closed.

## Observability
Use existing structured logging and persistent audit. Include correlation identifiers where supported. Never log tokens, credentials, secret values, or unnecessarily raw sensitive payloads.

# 10. Acceptance criteria

## Functional
- [ ] Remote is an adapter over Control API.
- [ ] Auth and server-side authorization are mandatory.
- [ ] Reconnect revalidates identity/scope.
- [ ] Limits/timeouts are enforced.
- [ ] Real remote tests cover read/deny/mutation.

## Architecture
- [ ] One canonical implementation/domain owner exists.
- [ ] Existing contracts are reused or explicitly extended.
- [ ] No duplicate business-logic path exists.
- [ ] Interface adapters preserve identical semantics.
- [ ] Lifecycle and restart behavior are explicit.

## Security
- [ ] Authorization is enforced at the execution boundary.
- [ ] Scope/identity mismatches are rejected.
- [ ] Inputs and resources are bounded.
- [ ] Secrets are redacted.
- [ ] Consequential failures fail closed where required.

## Persistence/recovery
- [ ] One authoritative persistence owner exists.
- [ ] Required state survives restart.
- [ ] Corruption/partial publication is detected.
- [ ] Concurrency behavior is deterministic.
- [ ] Recovery semantics are tested.

## Testing
- [ ] Unit tests pass.
- [ ] Integration tests pass.
- [ ] Adversarial tests pass.
- [ ] Failure/recovery tests pass.
- [ ] Applicable real-interface tests pass.
- [ ] Repository verification gates pass.

## Documentation
- [ ] Feature contract is documented.
- [ ] Runtime behavior is not overstated.
- [ ] Known limitations/evidence gaps are reported.

# 11. Explicit non-goals

- No cloud platform.
- No second RPC.
- No automatic public exposure.

Do not expand into model routing, autonomous orchestration, generic DAG scheduling, or unrelated infrastructure.

# 12. Final implementation report

Report:
Feature:
Canonical implementation:
Existing contracts reused:
Files changed:
Interfaces added/changed:
Persistence/migrations:
Security controls:
Audit/observability:
Tests added:
Real-interface evidence:
Verification results:
Known limitations:

## Standalone execution rule
A developer must be able to read this prompt, inspect current rust, identify existing implementations, implement only this feature's missing/incomplete contract, test it, and verify acceptance without another prompt, PR, or undocumented assumption.
