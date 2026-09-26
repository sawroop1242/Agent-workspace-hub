# 23 — TUI (TUI-001)

> Standalone implementation contract for the current rust branch. This prompt describes the implementation work; it does not claim that the feature is already complete.

## Prompt identity
**Prompt:** 23 / TUI-001  
**Feature:** TUI  
**Primary roadmap area:** Terminal user interface/control and observability  
**Target branch:** rust  
**Canonical folder:** docs/tui/

### Mission
Implement, converge, and verify TUI inside AWH's documented product boundary.

This prompt is standalone:
- inspect current rust before changing code;
- reuse existing contracts;
- do not require another prompt or PR to be merged first;
- do not create duplicate stores, services, authorization, policy, filesystem, edit, snapshot, rollback, audit, Git, MCP, or identity systems;
- keep external-agent reasoning/model selection outside AWH.

# 1. Scope

## In scope
- Make src/tui/* and src/tui/backend.rs the canonical presentation boundary.
- Complete navigation/views/actions needed by the final product without duplicating domain logic.
- Ensure TUI actions call the same services/backend used by CLI/API/MCP.
- Preserve authorization/audit for consequential actions.
- Keep terminal rendering/input/navigation state separate from persistent domain state.

## Out of scope
- No second application core.
- No TUI-local filesystem/editor/task/memory/skill stores.
- No full IDE.
- No TUI-local authorization.

## Product boundary
AWH owns presentation/control; TUI owns rendering, input and ephemeral view state only. Services own persistence and semantics.

# 2. Required repository forensics

Read at minimum:
Cargo.toml, README.md, AGENTS.md, docs/PROJECT_CONTEXT.md, docs/FEATURES.md, docs/architecture.md, docs/security.md, docs/threat-model.md, docs/CLI.md, docs/roadmap/PROJECT_ROADMAP.md, docs/roadmap/GROWTH_STRATEGY.md, docs/roadmap/ROADMAP_AGENT_PROFILES_POLICY_MCP.md, and implementation prompts 01–17.

Inspect:
src/tui/backend.rs, src/tui/mod.rs, src/tui/screens/*, service calls, CLI/API references and existing TUI tests.

Search before adding abstractions:
rg -n "TuiBackend|LocalBackend|src/tui|Screen|render|handle_event"
rg -n "TODO|FIXME|unimplemented!|todo!|panic!|unwrap\(|expect\("

Record which existing contracts are reused, extended, adapted, or replaced and why. Missing prompt coverage is not evidence of missing Rust implementation.

# 3. Architecture

## Canonical responsibility
Terminal input → TUI event/router → backend/service → identity/policy → domain result → view refresh.

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
Start → workspace/session context → view → action → service → result/error → refresh. Workspace/session switching invalidates stale selections.

# 4. Interfaces

## Rust/service interface
Define backend traits/view models that do not leak terminal-specific types into domain services. Preserve service error categories.

## CLI interface
awh tui launches the UI. Configuration selects workspace/session/view; business operations remain service calls.

## Control API interface
Remote TUI may use Control API through an explicit remote backend; local mode should use local services.

## MCP interface
Do not implement MCP protocol directly for local domain operations.

## TUI interface
Define screens/actions for supported workspace, agent/session, files, Git/worktrees, memory/context/skills/tasks, audit/logs, terminal/connectors as evidence permits.

All applicable interfaces must call the same canonical service/domain implementation.

# 5. Security

Security requirements are implementation requirements.

## Authorization
Every consequential action uses existing authorization/policy/audit. Display-only data is still scope checked.

## Input/resource safety
Bound input/search/pasted content and avoid unbounded/blocking operations on the UI thread.

## Isolation
Active TUI scope is explicit; switching scope invalidates old selections and revalidates resource IDs.

## Secrets and sensitive data
Do not render secret values by default; mask credentials/tokens in logs/audit views.

## Required threat cases
Test unauthorized callers, forged/mismatched identity, cross-scope access, malformed input/state, resource exhaustion, replay, concurrent mutation, partial failure, crash/restart, and secret leakage.

# 6. Persistence and recovery

Navigation state may be ephemeral. Durable preferences use existing config authority; no domain-state persistence in TUI.

For durable state prove:
write/publication → process termination → new process → reload → verification.

Corrupt, truncated, incompatible, or partially published state must fail closed or follow an explicitly documented recovery rule. If another subsystem owns persistence, use that owner.

# 7. Integration boundaries

| Boundary | Existing authority | TUI responsibility |
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
Test event routing, view conversion, pagination, error rendering and backend calls.

## Integration tests
Run TUI against an isolated workspace/backend and verify real service-backed read and mutation paths.

## Security/adversarial tests
Stale selection after scope switch, unauthorized action, cross-session display, secret rendering, malformed input.

## Failure/recovery tests
Service failure, backend timeout/disconnect, terminal input/resize failure, application restart.

## Compatibility/real-interface tests
Existing screens/backend, service semantics, remote backend and API types.

## Verification gates
cargo fmt --all -- --check
cargo check --all-targets
cargo test --all-targets
cargo clippy --all-targets --all-features -- -D warnings
git diff --check

If an external service, platform, terminal emulator, remote target, or release environment is unavailable, record missing evidence instead of claiming verification.

# 9. Rollout

## Compatibility
Preserve existing navigation conventions; unsupported features must show unavailable/unverified rather than fake functionality.

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
UI errors cannot imply domain mutation. Reconcile before retrying consequential actions after timeout.

## Observability
Use existing structured logging and persistent audit. Include correlation identifiers where supported. Never log tokens, credentials, secret values, or unnecessarily raw sensitive payloads.

# 10. Acceptance criteria

## Functional
- [ ] TUI is a presentation layer.
- [ ] No duplicate domain/security state.
- [ ] Consequential actions use canonical auth/audit.
- [ ] Scope changes invalidate stale state.
- [ ] Real backend tests exist.

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

- No full IDE.
- No second application core.
- No autonomous execution.

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
