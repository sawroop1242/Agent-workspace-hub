# 24 — Collaboration (COL-001)

> Standalone implementation contract for the current rust branch. This prompt describes the implementation work; it does not claim that the feature is already complete.

## Prompt identity
**Prompt:** 24 / COL-001  
**Feature:** Collaboration  
**Primary roadmap area:** Multi-agent coordination  
**Target branch:** rust  
**Canonical folder:** docs/collaboration/

### Mission
Implement, converge, and verify Collaboration inside AWH's documented product boundary.

This prompt is standalone:
- inspect current rust before changing code;
- reuse existing contracts;
- do not require another prompt or PR to be merged first;
- do not create duplicate stores, services, authorization, policy, filesystem, edit, snapshot, rollback, audit, Git, MCP, or identity systems;
- keep external-agent reasoning/model selection outside AWH.

# 1. Scope

## In scope
- Implement awh collaboration agents|status|handoff|assign|conflicts|events.
- Coordinate ownership, task/session/worktree references, handoffs, conflict visibility and durable events.
- Reuse AgentRuntimeService, task service, worktree/Git, AuditLog and PolicyEngine.
- Define explicit collaboration records and conflict semantics without becoming a workflow engine.

## Out of scope
- No generic DAG/workflow engine.
- No autonomous swarm scheduler.
- No distributed consensus.
- No replacement agent registry/session service.

## Product boundary
AWH coordinates shared state/ownership/isolation. Agents remain responsible for reasoning and execution strategy.

# 2. Required repository forensics

Read at minimum:
Cargo.toml, README.md, AGENTS.md, docs/PROJECT_CONTEXT.md, docs/FEATURES.md, docs/architecture.md, docs/security.md, docs/threat-model.md, docs/CLI.md, docs/roadmap/PROJECT_ROADMAP.md, docs/roadmap/GROWTH_STRATEGY.md, docs/roadmap/ROADMAP_AGENT_PROFILES_POLICY_MCP.md, and implementation prompts 01–17.

Inspect:
Agent runtime/identity, tasks, worktrees/Git, audit, MCP routing, TUI collaboration screens and existing handoff/conflict/event code.

Search before adding abstractions:
rg -n "Collaboration|handoff|conflict|agent status|assign|collaboration|event"
rg -n "TODO|FIXME|unimplemented!|todo!|panic!|unwrap\(|expect\("

Record which existing contracts are reused, extended, adapted, or replaced and why. Missing prompt coverage is not evidence of missing Rust implementation.

# 3. Architecture

## Canonical responsibility
Agent/session → collaboration request → scope/auth → ownership/task/worktree state → event/audit → adapters. Conflict detection is evidence-based; it does not promise semantic conflict prevention.

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
Available → Assigned → Active → HandoffRequested → HandedOff/Released; conflicts may be Detected → Resolved/Acknowledged. Exact states must match current code. Handoff must not leave two owners.

# 4. Interfaces

## Rust/service interface
Define CollaborationState, Assignment/Handoff/Conflict/Event records referencing canonical AgentId/SessionId/TaskId/WorktreeId. Stable IDs/correlation/timestamps.

## CLI interface
awh collaboration agents|status|handoff|assign|conflicts|events; bounded event queries and explicit source/target IDs.

## Control API interface
API, where present, delegates to collaboration service with scoped auth and idempotency.

## MCP interface
MCP collaboration tools delegate to same service; an agent cannot mutate another agent's ownership without policy.

## TUI interface
TUI collaboration views call the canonical service.

All applicable interfaces must call the same canonical service/domain implementation.

# 5. Security

Security requirements are implementation requirements.

## Authorization
Ownership changes require collaboration/task/worktree capability and same-workspace validation. Handoff cannot grant target capabilities.

## Input/resource safety
Validate IDs, cursors, conflict refs, notes and event limits.

## Isolation
Workspace is primary collaboration scope; referenced agents/sessions/tasks/worktrees must be permitted in that scope.

## Secrets and sensitive data
Collaboration notes/events may contain project-sensitive information; redact credentials/raw tool payloads.

## Required threat cases
Test unauthorized callers, forged/mismatched identity, cross-scope access, malformed input/state, resource exhaustion, replay, concurrent mutation, partial failure, crash/restart, and secret leakage.

# 6. Persistence and recovery

Persist one collaboration authority with versioned/optimistic concurrency. Recovery cannot create two owners.

For durable state prove:
write/publication → process termination → new process → reload → verification.

Corrupt, truncated, incompatible, or partially published state must fail closed or follow an explicitly documented recovery rule. If another subsystem owns persistence, use that owner.

# 7. Integration boundaries

| Boundary | Existing authority | Collaboration responsibility |
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
Test state machine, handoff preconditions, conflict classification, event ordering and scope.

## Integration tests
Real agent/session/task/worktree handoff and restart; concurrent handoff attempts.

## Security/adversarial tests
Forged IDs, cross-workspace handoff, unauthorized assignment, capability escalation, replay.

## Failure/recovery tests
Persistence failure during handoff, stale source version, crash during publication, duplicate event.

## Compatibility/real-interface tests
Agent/task/worktree/audit services, CLI/MCP/TUI/API.

## Verification gates
cargo fmt --all -- --check
cargo check --all-targets
cargo test --all-targets
cargo clippy --all-targets --all-features -- -D warnings
git diff --check

If an external service, platform, terminal emulator, remote target, or release environment is unavailable, record missing evidence instead of claiming verification.

# 9. Rollout

## Compatibility
Do not change identity/session semantics. Existing task assignment becomes adapter input, not a second authority.

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
If atomic handoff cannot be guaranteed, do not report success.

## Observability
Use existing structured logging and persistent audit. Include correlation identifiers where supported. Never log tokens, credentials, secret values, or unnecessarily raw sensitive payloads.

# 10. Acceptance criteria

## Functional
- [ ] Canonical collaboration service/state.
- [ ] Canonical identity references.
- [ ] Explicit handoff/conflict semantics.
- [ ] Scope-safe ownership.
- [ ] Durable correlated events.

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

- No swarm scheduler.
- No DAG engine.
- No distributed consensus.

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
