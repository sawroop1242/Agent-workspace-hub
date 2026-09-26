# 22 — Terminal (TRM-001)

> Standalone implementation contract for the current rust branch. This prompt describes the implementation work; it does not claim that the feature is already complete.

## Prompt identity
**Prompt:** 22 / TRM-001  
**Feature:** Terminal  
**Primary roadmap area:** Bounded terminal execution  
**Target branch:** rust  
**Canonical folder:** docs/terminal/

### Mission
Implement, converge, and verify Terminal inside AWH's documented product boundary.

This prompt is standalone:
- inspect current rust before changing code;
- reuse existing contracts;
- do not require another prompt or PR to be merged first;
- do not create duplicate stores, services, authorization, policy, filesystem, edit, snapshot, rollback, audit, Git, MCP, or identity systems;
- keep external-agent reasoning/model selection outside AWH.

# 1. Scope

## In scope
- Make src/services/terminal.rs the canonical terminal boundary.
- Complete awh terminal run|list|kill.
- Enforce session identity, process.execute capability/policy, workspace/cwd restrictions, timeouts, output limits, resource limits and audit.
- Reuse existing sandbox/resource-limit primitives and define fail-closed behavior for mandatory controls.
- Persist only lifecycle metadata needed for list/status/reconciliation.

## Out of scope
- No unrestricted host shell service.
- No second sandbox implementation.
- No remote execution.
- No generic daemon supervisor.

## Product boundary
AWH owns controlled local process execution. The child process performs external work; AWH owns authorization, workspace, limits, lifecycle and audit.

# 2. Required repository forensics

Read at minimum:
Cargo.toml, README.md, AGENTS.md, docs/PROJECT_CONTEXT.md, docs/FEATURES.md, docs/architecture.md, docs/security.md, docs/threat-model.md, docs/CLI.md, docs/roadmap/PROJECT_ROADMAP.md, docs/roadmap/GROWTH_STRATEGY.md, docs/roadmap/ROADMAP_AGENT_PROFILES_POLICY_MCP.md, and implementation prompts 01–17.

Inspect:
src/services/terminal.rs; src/mcp/sandbox.rs and platform sandbox modules; authorization/capability; audit; CLI/API/TUI terminal paths.

Search before adding abstractions:
rg -n "TerminalService|terminal run|terminal kill|process.execute|Command::new|sandbox|timeout|output_limit"
rg -n "TODO|FIXME|unimplemented!|todo!|panic!|unwrap\(|expect\("

Record which existing contracts are reused, extended, adapted, or replaced and why. Missing prompt coverage is not evidence of missing Rust implementation.

# 3. Architecture

## Canonical responsibility
Caller → identity → authorization → argv/cwd/env validation → sandbox/limits → child process → bounded output → termination/result → audit.

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
Requested → Authorized → Spawned → Running → Completed/Failed/TimedOut/Killed. Restart reconciles stale metadata with actual OS state; it does not pretend child processes are durable.

# 4. Interfaces

## Rust/service interface
Define TerminalRequest/Process/Result/error contracts; structured argv, bounded output, correlation IDs and explicit status.

## CLI interface
awh terminal run|list|kill; explicit argv/cwd/timeout/output limits, safe output and scoped kill ID.

## Control API interface
API, if exposed, delegates to TerminalService; never expose raw PID as authorization.

## MCP interface
MCP terminal tool, if present, validates schema then uses same service and policy.

## TUI interface
TUI terminal actions call TerminalService; no local process manager.

All applicable interfaces must call the same canonical service/domain implementation.

# 5. Security

Security requirements are implementation requirements.

## Authorization
Require process.execute plus session/workspace policy before spawn. Internal callers cannot bypass. Shell execution, if supported, is a distinct high-risk capability.

## Input/resource safety
Validate argv/cwd/environment/timeouts/output limits; reuse dangerous-env and secret controls; bound process resources.

## Isolation
Bind cwd/resources to workspace/session/worktree policy. Do not inherit arbitrary host env/fds.

## Secrets and sensitive data
Never log secret environment values, tokens or full environments.

## Required threat cases
Test unauthorized callers, forged/mismatched identity, cross-scope access, malformed input/state, resource exhaustion, replay, concurrent mutation, partial failure, crash/restart, and secret leakage.

# 6. Persistence and recovery

Persist only process metadata required for list/reconciliation. Startup marks stale records terminal; no claim of process persistence across AWH restart.

For durable state prove:
write/publication → process termination → new process → reload → verification.

Corrupt, truncated, incompatible, or partially published state must fail closed or follow an explicitly documented recovery rule. If another subsystem owns persistence, use that owner.

# 7. Integration boundaries

| Boundary | Existing authority | Terminal responsibility |
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
Test validation, env filtering, timeout/limit arithmetic, lifecycle and error classification.

## Integration tests
Real benign command through compiled CLI, timeout/kill, output limits, restart reconciliation, audit correlation.

## Security/adversarial tests
Unauthorized execution, cwd escape, dangerous env, secret leakage, oversized argv/output, resource exhaustion, cross-session kill.

## Failure/recovery tests
Spawn failure, timeout, process exits during kill, sandbox setup failure, restart with stale child, audit failure.

## Compatibility/real-interface tests
CLI, service, API/TUI/MCP where present, sandbox tests.

## Verification gates
cargo fmt --all -- --check
cargo check --all-targets
cargo test --all-targets
cargo clippy --all-targets --all-features -- -D warnings
git diff --check

If an external service, platform, terminal emulator, remote target, or release environment is unavailable, record missing evidence instead of claiming verification.

# 9. Rollout

## Compatibility
Preserve safe invocation behavior; never weaken existing sandbox requirements.

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
If authorization, cwd validation, mandatory sandbox or resource limits cannot be established, do not spawn. Observer failure must not fabricate process success.

## Observability
Use existing structured logging and persistent audit. Include correlation identifiers where supported. Never log tokens, credentials, secret values, or unnecessarily raw sensitive payloads.

# 10. Acceptance criteria

## Functional
- [ ] One terminal service.
- [ ] Pre-spawn auth/policy.
- [ ] Workspace/env/resource limits.
- [ ] Timeout/output/kill semantics.
- [ ] Audit and restart reconciliation.

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

- No remote execution.
- No unrestricted shell.
- No second process supervisor.

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
