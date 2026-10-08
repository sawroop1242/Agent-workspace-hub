# 25 — Control API (API-001)

> Standalone implementation contract for the current rust branch. This prompt describes the implementation work; it does not claim that the feature is already complete.

## Prompt identity
**Prompt:** 25 / API-001  
**Feature:** Control API  
**Primary roadmap area:** Stable local-first HTTP control plane  
**Target branch:** rust  
**Canonical folder:** docs/control-api/

### Mission
Implement, converge, and verify Control API inside AWH's documented product boundary.

This prompt is standalone:
- inspect current rust before changing code;
- reuse existing contracts;
- do not require another prompt or PR to be merged first;
- do not create duplicate stores, services, authorization, policy, filesystem, edit, snapshot, rollback, audit, Git, MCP, or identity systems;
- keep external-agent reasoning/model selection outside AWH.

# 1. Scope

## In scope
- Make src/api/control.rs the canonical versioned Control API boundary.
- Complete awh api serve|status|tokens|logs and documented /api/v1 operations.
- Keep handlers thin adapters over application services.
- Define auth/token handling, scoped authorization, request limits, error schema, idempotency and lifecycle.
- Preserve local-first defaults and explicit remote exposure.

## Out of scope
- No second application service layer.
- No unauthenticated admin API.
- No replacement MCP protocol.
- No cloud control plane.

## Product boundary
AWH owns a stable HTTP control interface over local services. HTTP transport is not a second runtime/persistence authority.

# 2. Required repository forensics

Read at minimum:
Cargo.toml, README.md, AGENTS.md, docs/PROJECT_CONTEXT.md, docs/FEATURES.md, docs/architecture.md, docs/security.md, docs/threat-model.md, docs/CLI.md, docs/roadmap/PROJECT_ROADMAP.md, docs/roadmap/GROWTH_STRATEGY.md, docs/roadmap/ROADMAP_AGENT_PROFILES_POLICY_MCP.md, and implementation prompts 01–17.

Inspect:
src/api/control.rs, startup/routing, auth/token/capability, audit/rate limit, API tests and docs.

Search before adding abstractions:
rg -n "/api/v1|ControlApi|control.rs|api serve|api status|api tokens|api logs|Authorization|bearer"
rg -n "TODO|FIXME|unimplemented!|todo!|panic!|unwrap\(|expect\("

Record which existing contracts are reused, extended, adapted, or replaced and why. Missing prompt coverage is not evidence of missing Rust implementation.

# 3. Architecture

## Canonical responsibility
HTTP request → parse/limits/rate limit → authentication → scope → service authorization → canonical service → structured response/audit.

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
Server configured → safe bind → accepting → graceful shutdown. Token/config changes are explicit; malformed requests never reach mutation.

# 4. Interfaces

## Rust/service interface
Define stable DTOs/errors/versioning without leaking persistence internals. Use correlation IDs and typed HTTP errors.

## CLI interface
awh api serve|status|tokens|logs; safe local bind defaults, no token printing, actual listener status.

## Control API interface
Document every /api/v1 route: method, schema, auth, scope, idempotency/concurrency and error mapping. Bound body/query/pagination.

## MCP interface
API is not MCP. Operations shared with MCP delegate to the same service.

## TUI interface
Local TUI should prefer local services; remote TUI may use Control API explicitly.

All applicable interfaces must call the same canonical service/domain implementation.

# 5. Security

Security requirements are implementation requirements.

## Authorization
Authentication and scope must be established before consequential service calls. URL selectors are not authorization.

## Input/resource safety
Bound body/header/query sizes, JSON/domain validation, rate limits, path validation and pagination.

## Isolation
Tokens/sessions/request scopes cannot cross workspaces/agents without explicit authorization.

## Secrets and sensitive data
Never log bearer tokens, Authorization headers or secret fields. Token secret is shown only at intended creation point.

## Required threat cases
Test unauthorized callers, forged/mismatched identity, cross-scope access, malformed input/state, resource exhaustion, replay, concurrent mutation, partial failure, crash/restart, and secret leakage.

# 6. Persistence and recovery

Persist token metadata using existing config/secret authority; no parallel credential DB.

For durable state prove:
write/publication → process termination → new process → reload → verification.

Corrupt, truncated, incompatible, or partially published state must fail closed or follow an explicitly documented recovery rule. If another subsystem owns persistence, use that owner.

# 7. Integration boundaries

| Boundary | Existing authority | Control API responsibility |
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
Test route parsing, DTO validation, auth middleware, errors, pagination and redaction.

## Integration tests
Real HTTP through production router/middleware for allowed/denied/read/write paths; restart server and verify durable config/token state.

## Security/adversarial tests
Invalid/expired token, scope mismatch, traversal, oversized body, rate-limit exhaustion, header leakage, method confusion.

## Failure/recovery tests
Malformed JSON, downstream timeout, auth failure, token persistence failure, graceful shutdown/restart.

## Compatibility/real-interface tests
Existing /api/v1 routes, CLI api commands, service semantics and clients.

## Verification gates
cargo fmt --all -- --check
cargo check --all-targets
cargo test --all-targets
cargo clippy --all-targets --all-features -- -D warnings
git diff --check

If an external service, platform, terminal emulator, remote target, or release environment is unavailable, record missing evidence instead of claiming verification.

# 9. Rollout

## Compatibility
Preserve existing endpoint shapes or version changes explicitly. Do not alter domain semantics under the same API version.

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
Unsafe bind/security configuration prevents startup; downstream failure cannot be reported as success.

## Observability
Use existing structured logging and persistent audit. Include correlation identifiers where supported. Never log tokens, credentials, secret values, or unnecessarily raw sensitive payloads.

# 10. Acceptance criteria

## Functional
- [ ] Thin versioned API adapter.
- [ ] Auth/scoped authorization before mutation.
- [ ] Stable bounded schemas/errors.
- [ ] No secret leakage.
- [ ] Real HTTP tests.

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

- No cloud control plane.
- No duplicate services.
- No public unauthenticated admin API.

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
