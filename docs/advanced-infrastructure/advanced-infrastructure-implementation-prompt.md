# 29 — Advanced Infrastructure (INF-001)

> Standalone implementation contract for the current rust branch. This prompt describes the implementation work; it does not claim that the feature is already complete.

## Prompt identity
**Prompt:** 29 / INF-001  
**Feature:** Advanced Infrastructure  
**Primary roadmap area:** Optional sandboxing, quotas, network policy, secrets, remote execution, storage optimization and RBAC  
**Target branch:** rust  
**Canonical folder:** docs/advanced-infrastructure/

### Mission
Implement, converge, and verify Advanced Infrastructure inside AWH's documented product boundary.

This prompt is standalone:
- inspect current rust before changing code;
- reuse existing contracts;
- do not require another prompt or PR to be merged first;
- do not create duplicate stores, services, authorization, policy, filesystem, edit, snapshot, rollback, audit, Git, MCP, or identity systems;
- keep external-agent reasoning/model selection outside AWH.

# 1. Scope

## In scope
- Establish modular adapter boundaries for optional high-risk infrastructure: process/OS/container/WASM sandboxing, quotas, network policy, secret-provider adapters, remote execution, snapshot deduplication and enterprise RBAC where demanded.
- Reuse current security/service contracts.
- Provide capability discovery and fail-closed behavior for mandatory controls.
- Keep core AWH usable without optional infrastructure.

## Out of scope
- No mandatory cloud vendor.
- No replacement of current sandbox/auth/policy/snapshot/secret primitives.
- No broad enterprise platform rewrite.
- No implicit remote execution.

## Product boundary
Core AWH defines stable security/resource interfaces; optional adapters implement them. PolicyEngine/authorization remain authoritative.

# 2. Required repository forensics

Read at minimum:
Cargo.toml, README.md, AGENTS.md, docs/PROJECT_CONTEXT.md, docs/FEATURES.md, docs/architecture.md, docs/security.md, docs/threat-model.md, docs/CLI.md, docs/roadmap/PROJECT_ROADMAP.md, docs/roadmap/GROWTH_STRATEGY.md, docs/roadmap/ROADMAP_AGENT_PROFILES_POLICY_MCP.md, and implementation prompts 01–17.

Inspect:
src/mcp/sandbox.rs and platform modules; rate/resource limits; authorization/policy; secret permissions/config; terminal; snapshot; any existing RBAC/remote execution code.

Search before adding abstractions:
rg -n "sandbox|quota|resource limit|network policy|secret|RBAC|remote execution|dedup|container|WASM|capability"
rg -n "TODO|FIXME|unimplemented!|todo!|panic!|unwrap\(|expect\("

Record which existing contracts are reused, extended, adapted, or replaced and why. Missing prompt coverage is not evidence of missing Rust implementation.

# 3. Architecture

## Canonical responsibility
Request → capability/policy → selected adapter → bounded resource → execution/storage → verification → audit. A weaker adapter cannot silently replace a mandatory stronger control.

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
Available → Configured → Validated → Active → Failed/Disabled. Adapter changes must not invalidate active safety assumptions.

# 4. Interfaces

## Rust/service interface
Use small adapter traits only where current code lacks a suitable contract. Return typed capability/availability/errors; avoid a giant universal infrastructure trait.

## CLI interface
Expose infrastructure inspection through doctor/status/config where appropriate; high-risk changes require explicit authorization.

## Control API interface
Control API may expose health/effective configuration but never raw secrets or unrestricted adapter control.

## MCP interface
MCP operations see only PolicyEngine-granted capabilities; adapter IDs are not permissions.

## TUI interface
TUI/doctor shows effective adapter status and degraded states via canonical config/policy services.

All applicable interfaces must call the same canonical service/domain implementation.

# 5. Security

Security requirements are implementation requirements.

## Authorization
Optional capability is denied unless explicitly allowed. RBAC composes with existing capability/policy rather than replacing it. Remote execution requires explicit identity/scope/network/process controls.

## Input/resource safety
Validate adapter IDs, quota values, network destinations, container/WASM config, secret refs, remote endpoints and dedup parameters; reject unsafe defaults.

## Isolation
Bind adapters to workspace/agent/session/worktree. Stronger isolation may be selected; weaker isolation cannot be silently used when policy requires stronger.

## Secrets and sensitive data
Secret adapters return values only to authorized execution boundaries and never log/persist them. RBAC data contains no credentials.

## Required threat cases
Test unauthorized callers, forged/mismatched identity, cross-scope access, malformed input/state, resource exhaustion, replay, concurrent mutation, partial failure, crash/restart, and secret leakage.

# 6. Persistence and recovery

Adapter config uses existing config authority. Snapshot dedup must preserve exact-byte recovery and integrity. Remote state is explicit/reconciled.

For durable state prove:
write/publication → process termination → new process → reload → verification.

Corrupt, truncated, incompatible, or partially published state must fail closed or follow an explicitly documented recovery rule. If another subsystem owns persistence, use that owner.

# 7. Integration boundaries

| Boundary | Existing authority | Advanced Infrastructure responsibility |
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
Test adapter selection, capability negotiation, quota arithmetic, network rule matching, secret refs, RBAC composition and dedup integrity.

## Integration tests
Exercise a real sandbox/resource adapter where available; verify policy-denied operations cannot select permissive fallback; dedup round-trip/corruption; remote only with actual target.

## Security/adversarial tests
Fallback/downgrade, quota bypass, network bypass, secret leakage, remote scope confusion, adapter impersonation, corrupted dedup metadata, RBAC escalation.

## Failure/recovery tests
Adapter unavailable, partial sandbox setup, quota exhaustion, network-policy error, secret outage, remote disconnect, dedup corruption, restart.

## Compatibility/real-interface tests
Existing sandbox/resource limits, authorization/policy, terminal/MCP/snapshot/audit and platform CI.

## Verification gates
cargo fmt --all -- --check
cargo check --all-targets
cargo test --all-targets
cargo clippy --all-targets --all-features -- -D warnings
git diff --check

If an external service, platform, terminal emulator, remote target, or release environment is unavailable, record missing evidence instead of claiming verification.

# 9. Rollout

## Compatibility
Optional adapters are additive; safe defaults unchanged. Never silently switch to weaker security.

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
If a mandatory control cannot be established, deny. Optional optimization may fall back only to an equivalent-security implementation.

## Observability
Use existing structured logging and persistent audit. Include correlation identifiers where supported. Never log tokens, credentials, secret values, or unnecessarily raw sensitive payloads.

# 10. Acceptance criteria

## Functional
- [ ] Explicit adapter boundaries.
- [ ] PolicyEngine remains authoritative.
- [ ] Mandatory controls fail closed.
- [ ] No secret leakage.
- [ ] Resource/network/sandbox controls are actually tested.
- [ ] Dedup preserves recovery.
- [ ] Remote execution has identity/scope/reconciliation.
- [ ] Core remains stable.

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

- No mandatory cloud dependency.
- No enterprise platform rewrite.
- No silent security downgrade.
- No unrestricted remote execution.

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
