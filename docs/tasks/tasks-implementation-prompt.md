# 21 — Tasks (TSK-001)

> Standalone implementation contract for the current rust branch. This prompt describes the implementation work; it does not claim that the feature is already complete.

## Prompt identity
**Prompt:** 21 / TSK-001  
**Feature:** Tasks  
**Primary roadmap area:** Task and execution state  
**Target branch:** rust  
**Canonical folder:** docs/tasks/

### Mission
Implement, converge, and verify Tasks inside AWH's documented product boundary.

This prompt is standalone:
- inspect current rust before changing code;
- reuse existing contracts;
- do not require another prompt or PR to be merged first;
- do not create duplicate stores, services, authorization, policy, filesystem, edit, snapshot, rollback, audit, Git, MCP, or identity systems;
- keep external-agent reasoning/model selection outside AWH.

# 1. Scope

## In scope
- Converge src/models/task.rs, src/core/tasks.rs and src/mcp/tasks.rs around one task authority.
- Implement durable list/show/create/update/cancel/assign.
- Bind tasks to workspace/project and existing agent/session/worktree references.
- Define state transitions, stale-update/concurrency semantics, correlation and audit.
- Coordinate state without becoming an execution scheduler.

## Out of scope
- No DAG/workflow engine.
- No autonomous swarm scheduler.
- No generic job queue.
- No second task database.

## Product boundary
AWH owns durable task/work-item state and assignment metadata. External agents decide how work is performed.

# 2. Required repository forensics

Read at minimum:
Cargo.toml, README.md, AGENTS.md, docs/PROJECT_CONTEXT.md, docs/FEATURES.md, docs/architecture.md, docs/security.md, docs/threat-model.md, docs/CLI.md, docs/roadmap/PROJECT_ROADMAP.md, docs/roadmap/GROWTH_STRATEGY.md, docs/roadmap/ROADMAP_AGENT_PROFILES_POLICY_MCP.md, and implementation prompts 01–17.

Inspect:
src/models/task.rs, src/core/tasks.rs, src/mcp/tasks.rs, CLI task handlers, API/TUI/collaboration references and persisted task tests.

Search before adding abstractions:
rg -n "Task|TaskStore|TaskStatus|awh task|mcp.*task|assign_task|cancel_task"
rg -n "TODO|FIXME|unimplemented!|todo!|panic!|unwrap\(|expect\("

Record which existing contracts are reused, extended, adapted, or replaced and why. Missing prompt coverage is not evidence of missing Rust implementation.

# 3. Architecture

## Canonical responsibility
Caller → identity/scope → TaskService/Store → lifecycle/assignment validation → durable state → audit/event → adapter.

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
Use the current finite status model; reject impossible/stale transitions. Assignment references canonical AgentId/SessionId/WorktreeId. Restart reloads the authoritative state.

# 4. Interfaces

## Rust/service interface
Define canonical Task/TaskId/TaskStatus/TaskAssignment/TaskService APIs and typed transition/scope/assignment/persistence errors.

## CLI interface
awh task list|show|create|update|cancel|assign; explicit IDs, bounded filters, explicit assignment target, stable conflict errors.

## Control API interface
API, where supported, uses the same service with scoped auth and bounded listing/version checks.

## MCP interface
MCP task tools delegate to TaskService and carry agent/session identity.

## TUI interface
TUI displays and mutates tasks only through TaskService.

All applicable interfaces must call the same canonical service/domain implementation.

# 5. Security

Security requirements are implementation requirements.

## Authorization
Task mutations require capability/policy; assignment never grants capabilities to the target agent.

## Input/resource safety
Validate IDs, transitions, descriptions, filters, assignments and optimistic versions; bound task text and list results.

## Isolation
Tasks are workspace/project scoped; referenced agent/session/worktree must be within allowed scope unless explicit policy allows otherwise.

## Secrets and sensitive data
Task text may be sensitive. Do not persist or log credentials as task fields; redact audit.

## Required threat cases
Test unauthorized callers, forged/mismatched identity, cross-scope access, malformed input/state, resource exhaustion, replay, concurrent mutation, partial failure, crash/restart, and secret leakage.

# 6. Persistence and recovery

Choose one task store. Version/migrate existing records, atomically update state, detect corruption, reload on restart.

For durable state prove:
write/publication → process termination → new process → reload → verification.

Corrupt, truncated, incompatible, or partially published state must fail closed or follow an explicitly documented recovery rule. If another subsystem owns persistence, use that owner.

# 7. Integration boundaries

| Boundary | Existing authority | Tasks responsibility |
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
Test state machine, assignment, stale version, serialization, filtering and deterministic ordering.

## Integration tests
Real create/assign/update/cancel, restart, agent/session binding and concurrent updates.

## Security/adversarial tests
Cross-workspace assignment, forged IDs, unauthorized transitions, stale updates, oversized payloads, replayed cancel.

## Failure/recovery tests
Persistence failure, corrupt record, restart after partial update, concurrent transition, repeated cancel.

## Compatibility/real-interface tests
Existing task models/MCP, CLI/API/TUI, persisted records.

## Verification gates
cargo fmt --all -- --check
cargo check --all-targets
cargo test --all-targets
cargo clippy --all-targets --all-features -- -D warnings
git diff --check

If an external service, platform, terminal emulator, remote target, or release environment is unavailable, record missing evidence instead of claiming verification.

# 9. Rollout

## Compatibility
Preserve existing status meanings or provide explicit migration; unknown states fail closed.

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
Invalid/unauthorized transitions do not mutate. Persistence failure does not report success.

## Observability
Use existing structured logging and persistent audit. Include correlation identifiers where supported. Never log tokens, credentials, secret values, or unnecessarily raw sensitive payloads.

# 10. Acceptance criteria

## Functional
- [ ] One canonical task store/service.
- [ ] Lifecycle and concurrency semantics tested.
- [ ] Scope-safe assignment.
- [ ] Restart-safe CRUD.
- [ ] No scheduler introduced.

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

- No workflow engine.
- No swarm scheduler.
- No execution queue.

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
