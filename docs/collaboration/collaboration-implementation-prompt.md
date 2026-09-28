# 24 — Collaboration (COL-001)

> Standalone implementation contract for the current `rust` branch. This prompt describes the implementation work; it does not claim that the feature is already complete.

## Prompt identity
**Prompt:** 24 / COL-001  
**Feature:** Collaboration  
**Primary roadmap area:** Multi-agent coordination  
**Target branch:** rust  
**Canonical folder:** docs/collaboration/

### Mission
Implement, converge, and verify Collaboration inside AWH's documented product boundary.

This prompt is standalone:
- inspect current `rust` before changing code;
- reuse existing contracts;
- do not require another prompt or PR to be merged first;
- do not create duplicate stores, services, authorization, policy, filesystem, edit, snapshot, rollback, audit, Git, MCP, identity, session, or task systems;
- keep external-agent reasoning, model selection, and agent intelligence outside AWH.

# 1. Scope

## In scope
- Implement or complete `awh collaboration agents|status|handoff|assign|conflicts|events`.
- Coordinate shared ownership, task/session/worktree references, explicit handoffs, conflict visibility, and correlated collaboration events.
- Reuse the canonical AgentRuntimeService, canonical task authority, canonical worktree/Git boundary, persistent AuditLog, authorization/PolicyEngine, and existing interface backends.
- Define explicit collaboration records, ownership transitions, conflict semantics, event correlation, idempotency, and concurrency behavior without becoming a generic workflow engine.
- Provide one collaboration domain/service authority consumed by CLI, MCP, TUI, and Control API adapters where those interfaces are exposed.

## Out of scope
- No generic DAG/workflow engine.
- No autonomous swarm scheduler.
- No distributed consensus or leader-election system.
- No replacement agent registry/session service.
- No replacement task manager.
- No second worktree/Git manager.
- No merge-orchestration engine unless an existing repository contract explicitly requires a narrowly scoped collaboration integration; Git merge mechanics remain owned by the Git/worktree boundary.

## Product boundary
AWH coordinates shared state, ownership, isolation, handoffs, and observable collaboration events. Agents remain responsible for reasoning, planning, tool strategy, and execution decisions.

# 2. Required repository forensics

Read at minimum:
- `Cargo.toml`
- `README.md`
- `AGENTS.md`
- `docs/PROJECT_CONTEXT.md`
- `docs/FEATURES.md`
- `docs/architecture.md`
- `docs/security.md`
- `docs/threat-model.md`
- `docs/CLI.md`
- `docs/roadmap/PROJECT_ROADMAP.md`
- `docs/roadmap/GROWTH_STRATEGY.md`
- `docs/roadmap/ROADMAP_AGENT_PROFILES_POLICY_MCP.md`
- implementation prompts 01–17
- the current collaboration prompt and related feature prompts on the checked-out `rust` branch.

Inspect at minimum:
- `src/services/agent_runtime.rs`
- `src/core/identity.rs`
- `src/core/agents.rs`
- the canonical task implementation actually selected by the current `rust` branch
- `src/services/git.rs` and the current worktree implementation/contract
- `src/services/audit.rs`
- `src/services/authorization.rs` and capability/policy modules
- `src/mcp/dispatcher.rs` and relevant MCP adapters
- `src/api/control.rs`
- `src/tui/backend.rs` and relevant TUI screens
- any current collaboration/handoff/conflict/event implementation or references
- tests covering agents, sessions, tasks, worktrees, authorization, audit, and persistence.

Search before adding abstractions:
```text
rg -n "Collaboration|collaboration|handoff|ownership|conflict|assign|agent status|event"
rg -n "Task|TaskStore|TaskStatus|assign_task|cancel_task"
rg -n "worktree|Worktree|owner|ownership"
rg -n "AuditLog|AuditEntry|event_id|sequence"
rg -n "AgentRuntimeService|AgentSession|SessionIdentity"
rg -n "TODO|FIXME|unimplemented!|todo!|panic!|unwrap\\(|expect\\("
```

### Existing-contract inventory

The inventory is part of the implementation contract: verify each row against the checked-out `rust` branch before coding. A prompt, issue, roadmap statement, or missing file name is **not** sufficient evidence that a contract is absent.

| Existing contract | Current repository evidence | Required action | Reason |
|---|---|---|---|
| AgentProfile / AgentRegistry / AgentSession / caller identity | `src/services/agent_runtime.rs`, `src/core/agents.rs`, `src/core/identity.rs` | Reuse | Collaboration must reference canonical agent/session identity and workspace binding. |
| Task model/store | `src/models/task.rs`, `src/core/tasks.rs`, `src/mcp/tasks.rs` | Reuse the canonical task authority selected by the task contract; adapt callers if needed | Current branch contains materially different task-facing implementations; collaboration must not become a third task authority. |
| Git invocation boundary | `src/services/git.rs` | Reuse | Collaboration must not execute Git through a second abstraction. |
| Worktree isolation contract | `docs/implementation-prompts/13-worktree-isolation.md` plus current worktree-related source | Reuse/extend only if source proves a shared extension point | Agent ownership must bind to the canonical worktree identity and lifecycle. |
| Authorization / PolicyEngine | `src/services/authorization.rs` and capability/policy modules | Reuse | Assignment/handoff are consequential ownership mutations and cannot rely on UI/operator intent. |
| Persistent audit | `src/services/audit.rs` | Reuse | Collaboration events need one durable, redacted, correlated audit authority. |
| MCP dispatcher/session | `src/mcp/dispatcher.rs`, `src/mcp/*` | Reuse/adapt | MCP is an adapter/transport boundary, not a second collaboration domain. |
| Control API | `src/api/control.rs` | Adapt | HTTP control must call the same collaboration authority when collaboration endpoints are exposed. |
| TUI backend | `src/tui/backend.rs`, `src/tui/*` | Adapt | TUI presents and invokes canonical collaboration state; it must not own it. |
| Workspace/project scope | init/workspace/project services | Reuse | Workspace is the primary isolation boundary for collaboration. |
| Existing locking/concurrency primitives | current store/service locks, including `StoreLock` where applicable | Reuse where compatible | Ownership transitions must be serialized or protected by explicit optimistic concurrency. |

If current source contradicts this table, update the implementation plan to match verified source and record the discrepancy; do not invent an implementation from documentation alone.

# 3. Architecture

## Canonical architecture flow

```text
CLI / MCP / TUI / Control API
          │
          ▼
trusted caller identity + workspace scope
          │
          ▼
authorization + capability/policy
          │
          ▼
canonical CollaborationService
          │
          ├── canonical AgentRuntimeService
          ├── canonical TaskService/TaskStore
          ├── canonical Worktree/Git boundary
          ├── ownership + handoff state
          ├── evidence-based conflict detection
          │
          ▼
atomic/version-checked state transition
          │
          ├── correlated CollaborationEvent
          └── persistent AuditLog
```

The exact service names may follow current source, but the responsibility boundaries must remain equivalent.

## Canonical responsibility

Collaboration owns the **relationship and coordination state** between already-authoritative runtime entities:
- which agent/session is assigned to which collaboration-scoped task/worktree/resource;
- who currently owns a collaboration resource;
- how ownership is transferred;
- what evidence constitutes a detected conflict;
- the immutable/correlated event describing a collaboration transition.

Collaboration does **not** own:
- agent identity or session validity;
- task lifecycle authority;
- Git/worktree mechanics;
- capabilities or policy decisions;
- file/edit/recovery semantics;
- audit storage;
- MCP transport sessions;
- process execution;
- model/provider routing.

## Architectural invariants

1. Exactly one canonical collaboration domain/service owns collaboration state.
2. Exactly one persistence authority exists for durable collaboration state.
3. Existing workspace, agent, session, task, and worktree identities remain authoritative.
4. A collaboration assignment never grants capabilities and never changes PolicyEngine decisions implicitly.
5. A handoff is an ownership transition, not merely an informational message.
6. A successful handoff cannot leave two current owners for the same exclusive resource.
7. A stale caller cannot overwrite a newer ownership state.
8. Conflict detection is evidence-based and deterministic; it must not claim semantic conflict resolution that the system cannot prove.
9. Interface adapters do not implement alternate collaboration semantics.
10. Collaboration events are correlated with canonical IDs and written through the existing audit authority.
11. Persistence/recovery must not reconstruct an impossible two-owner state.
12. Possession of an agent/task/worktree ID is never authority.
13. Operator confirmation in CLI/TUI is a UX control and never substitutes for authorization.
14. Source existence is not treated as proof of a complete contract; behavior and tests are the evidence.

## State and lifecycle

Define states only after reconciling current source. A permitted conceptual lifecycle is:

```text
Unassigned
   │
   ▼
Assigned ──► Active
   │            │
   │            └──► HandoffRequested
   │                       │
   ▼                       ▼
Released ◄────────── HandedOff
```

Conflicts are an evidence/status dimension rather than an automatic ownership transition:

```text
NoConflict → Detected → Acknowledged / Resolved
```

The implementation must specify:
- legal transitions;
- actor allowed to initiate each transition;
- source/target agent and session requirements;
- task/worktree ownership constraints;
- expected version/revision used for concurrency;
- idempotency behavior;
- behavior after restart;
- behavior when the referenced agent/session/task/worktree is missing, stopped, disabled, or changed.

A task's lifecycle and an agent/session's execution lifecycle remain distinct. Assignment must not silently start a process, activate an agent, create a session, grant a capability, or mutate a worktree.

# 4. Interfaces

## Rust/service interface

Define or extend a canonical collaboration contract with:
- stable collaboration/resource identifiers;
- explicit assignment and ownership records;
- handoff request/result records;
- conflict records containing evidence/references rather than unverifiable conclusions;
- immutable/correlated event records where durable collaboration history is required;
- workspace, agent, session, task, and worktree references;
- version/revision or equivalent optimistic-concurrency token;
- stable structured error categories;
- bounded list/event query parameters.

Do not create a second representation of an existing AgentId, SessionId, TaskId, or WorktreeId.

## CLI interface

Support the documented surface:
`awh collaboration agents|status|handoff|assign|conflicts|events`.

Requirements:
- stable parsing and exit behavior;
- explicit source/target identifiers for ownership-changing operations;
- bounded event/conflict queries;
- stale-state and authorization failures are distinguishable;
- confirmation prompts, if present, are UX only;
- successful output reflects the canonical service result rather than locally inferred state.

## Control API interface

Where collaboration endpoints are exposed:
- use the existing versioned Control API conventions;
- delegate to the same CollaborationService;
- enforce scoped authentication/authorization before mutation;
- preserve stable error categories and correlation identifiers;
- use idempotency/version checks for retried ownership mutations;
- do not introduce a collaboration-specific RPC or persistence protocol.

## MCP interface

Where collaboration MCP tools are exposed:
- delegate to the same canonical service;
- resolve the caller through the existing AWH identity/session boundary;
- enforce policy/capability at the consequential mutation boundary;
- reject cross-workspace and cross-agent ownership substitution;
- never let an MCP transport/session identity become an AWH capability grant.

## TUI interface

Where collaboration views/actions are exposed:
- read through the canonical service/backend;
- do not keep authoritative collaboration/task/ownership state in widgets;
- invalidate stale selections when workspace/session/agent context changes;
- route consequential actions through the same authorization/service path;
- do not treat a visual confirmation as authorization.

### Interfaces acceptance

- [ ] CLI, MCP, Control API, and TUI use one canonical collaboration implementation where each surface is supported.
- [ ] The same ownership/handoff/conflict semantics apply across all surfaces.
- [ ] The same identity, workspace scope, authorization, concurrency, and error rules apply across all surfaces.
- [ ] Retried ownership mutations are deterministic/idempotent according to the documented contract.
- [ ] No interface can bypass the canonical collaboration service by writing its store directly.
- [ ] Real compiled/interface tests exercise applicable paths rather than only constructing domain structs.

# 5. Security

Security requirements are implementation requirements.

## Authorization

Ownership changes require the appropriate existing collaboration/task/worktree capability and policy decision at the canonical mutation boundary.

At minimum:
- verify trusted caller identity;
- verify caller/session/workspace binding;
- verify referenced source/target entities are in the same permitted workspace scope;
- verify the caller is permitted to assign, release, or accept ownership;
- re-check authorization on consequential operations rather than trusting persisted assignment state;
- do not allow handoff to grant capabilities;
- do not treat possession of IDs or a UI confirmation as authority.

## Concurrency and replay

Protect against:
- two agents accepting the same exclusive assignment;
- source and target performing a handoff simultaneously;
- stale clients overwriting current ownership;
- replay of an old handoff request;
- duplicate event publication;
- crash after ownership publication but before event/audit publication.

Use existing locks or explicit version/compare-and-swap semantics. Do not add distributed locking.

## Input/resource safety

Validate and bound:
- agent/session/task/worktree IDs;
- ownership notes/reasons;
- event/conflict filters;
- pagination cursors;
- result sizes;
- number of referenced resources;
- repeated/replayed requests.

Reject traversal or path-like identifiers if any collaboration persistence maps identifiers to filesystem paths.

## Isolation

Workspace is the primary collaboration scope. Reject:
- cross-workspace assignment/handoff;
- session/agent substitution;
- task/worktree references outside the caller's permitted scope;
- disabled/unknown/stopped runtime identities where the contract requires an active identity.

## Secrets and sensitive data

Collaboration notes and events may contain project-sensitive information. Reuse the audit redaction boundary and never persist or emit:
- credentials/tokens/API keys;
- raw secret values;
- unrestricted tool payloads;
- unnecessary environment contents.

## Required threat cases

Test:
- unauthorized caller;
- forged/mismatched identity;
- cross-workspace ownership mutation;
- disabled/unknown agent;
- stale session;
- duplicate assignment;
- replayed handoff;
- concurrent handoff;
- stale version;
- malformed/corrupt collaboration state;
- resource exhaustion;
- secret leakage;
- crash/restart during state transition.

# 6. Persistence and recovery

## Persistence owner

The implementation must select **one** canonical collaboration persistence authority based on current `rust` source. Collaboration state may be durable because ownership and handoff history must survive process restart, but persistence must be narrowly scoped to collaboration-owned state.

The collaboration persistence owner is responsible for:
- schema/version;
- ownership/assignment records;
- revision/version metadata;
- durable transition state needed for recovery;
- migration;
- atomic publication;
- locking/concurrency;
- corruption detection;
- recovery behavior;
- retention/cleanup of collaboration-specific history where required.

It must **not** become a second owner of:
- agents/sessions;
- tasks;
- worktrees/Git;
- snapshots;
- audit history;
- capabilities/policy.

If the final design can derive collaboration state from an existing authoritative store without duplicating it, prefer that design. If collaboration-specific state must be persisted, document exactly why it cannot be derived and exactly which fields are owned.

## Durability and recovery contract

For required durable transitions, prove:

```text
validate + authorize
      ↓
read current version/state
      ↓
prepare complete transition
      ↓
atomic publish ownership state
      ↓
emit correlated audit/collaboration event
      ↓
restart process
      ↓
reload + validate invariants
```

The implementation must define the failure boundary between state publication and event/audit publication. A crash or audit failure must never fabricate a successful handoff or leave the system reporting an ownership state that the persisted state cannot prove.

Corrupt, truncated, incompatible, or partially published collaboration state must fail closed or follow an explicitly documented recovery rule. Never silently reset ownership to an arbitrary default.

## Retention and cleanup

Document:
- whether collaboration events are retained separately from the canonical audit log;
- maximum retained collaboration records if any;
- cleanup trigger and ownership;
- whether completed/released records remain queryable;
- how cleanup preserves active ownership and correlation integrity;
- how cleanup behaves across workspaces.

No destructive retention policy may be invented merely to control file size.

# 7. Integration boundaries

| Boundary | Existing authority | Collaboration responsibility |
|---|---|---|
| Workspace/project | init/workspace/project services | resolve and enforce scope |
| Agent/session | AgentRuntimeService | reference trusted identity and lifecycle state |
| Tasks | canonical task service/store selected by the task contract | reference assignment/task state; do not duplicate task lifecycle |
| Git/worktree | canonical Git/worktree service | reference ownership and isolation; do not implement Git mechanics |
| Capability/policy | PolicyEngine + authorization service | request/enforce authorization; never self-grant |
| Files/edit/recovery | FilesService/EditService/Snapshot/Rollback | do not replace; collaboration only coordinates ownership |
| Audit | persistent AuditLog | emit correlated, redacted collaboration events |
| MCP | MCP dispatcher/session | transport adapter only |
| Control API | versioned control layer | adapter to canonical service |
| TUI | TUI backend/screens | presentation/control adapter only |

## No-duplication rule

Do not introduce parallel:
- agent identity;
- session lifecycle;
- task storage/lifecycle;
- worktree/Git execution;
- authorization/policy;
- filesystem safety;
- edit/snapshot/rollback;
- audit/event storage;
- MCP session;
- generic workflow/orchestration.

Collaboration may define collaboration-specific records and state, but must not re-model an existing authority merely to make integration easier.

# 8. Testing

## Unit tests
Test:
- legal/illegal ownership transitions;
- handoff preconditions;
- conflict classification from explicit evidence;
- version/revision checks;
- idempotency/replay handling;
- event correlation;
- workspace scope;
- bounded inputs.

## Integration tests
Use real canonical services where available:
- agent/session → collaboration → task/worktree;
- assign → active ownership → handoff;
- concurrent assignment/handoff attempts;
- restart and reload;
- audit correlation.

## Security/adversarial tests
Cover:
- forged IDs;
- cross-workspace handoff;
- unauthorized assignment;
- capability escalation;
- stale identity/session;
- replay;
- duplicate owner creation;
- malicious event/conflict filters;
- secret leakage.

## Failure/recovery tests
Cover:
- persistence failure before publication;
- persistence failure after publication;
- stale source version;
- crash during publication;
- crash before event publication;
- duplicate/replayed event;
- corrupt/truncated state;
- incompatible schema;
- recovery with active ownership.

## Compatibility/real-interface tests
Where implemented:
- CLI compiled behavior;
- real MCP request path;
- Control API request path;
- TUI backend/service path;
- task/worktree/agent/audit service integration.

Acceptance is based on behavior and repository evidence, not merely on the presence of types, routes, or helper functions.

## Verification gates

```text
cargo fmt --all -- --check
cargo check --all-targets
cargo test --all-targets
cargo clippy --all-targets --all-features -- -D warnings
git diff --check
```

If an external service, remote target, platform, or interface environment is unavailable, record the missing evidence instead of claiming verification.

# 9. Rollout

## Compatibility

Do not change:
- AgentRuntimeService identity/session semantics;
- canonical task lifecycle semantics;
- canonical worktree/Git semantics;
- authorization/capability meaning;
- persistent audit semantics.

Existing task assignment, worktree ownership, or agent/session state becomes collaboration input only after its canonical authority is identified.

## Collaboration-specific rollout sequence

1. **Forensics and evidence:** inspect current `rust` source, tests, interfaces, and persistence; record the contract inventory and all divergences.
2. **Canonical domain boundary:** define/extend one CollaborationService and collaboration-owned records without creating competing agent/task/worktree models.
3. **Ownership semantics:** implement explicit assignment, release, handoff, versioning, idempotency, and legal state transitions.
4. **Conflict evidence:** implement deterministic conflict detection/reporting from observable task/worktree/ownership state; do not claim semantic resolution that is not provable.
5. **Persistence/recovery:** establish or extend one collaboration persistence owner only where required; add schema, atomic publication, corruption handling, migration, and retention semantics.
6. **Security convergence:** integrate canonical identity, workspace scope, capability/policy authorization, concurrency protection, and secret redaction.
7. **Audit/events:** emit correlated collaboration events through the existing persistent AuditLog; do not create a second event history.
8. **Interface convergence:** connect CLI/MCP/Control API/TUI adapters to the same service and verify identical semantics.
9. **Adversarial verification:** test cross-scope, replay, stale-state, concurrent ownership, crash/restart, corruption, and resource-exhaustion cases.
10. **Repository verification:** run all verification gates and report exact evidence.
11. **Documentation:** update collaboration-specific documentation/status only for behavior actually implemented and verified.

No rollout step depends on another prompt or PR being merged first.

## Failure behavior

If atomic ownership transfer cannot be guaranteed by the available persistence/concurrency mechanism, do not report the handoff as successful.

If conflict evidence is incomplete, report an explicit unknown/insufficient-evidence state rather than inventing a conflict or claiming resolution.

## Observability

Use the existing structured logging and persistent audit authority. Correlate, where supported:
- workspace ID;
- agent ID;
- session ID;
- task ID;
- worktree ID;
- collaboration operation ID;
- audit event ID/sequence;
- reason/error code.

Never log tokens, credentials, secret values, or unnecessarily raw sensitive payloads.

# 10. Acceptance criteria

## Functional
- [ ] One canonical collaboration service/domain exists.
- [ ] Assignment, release, handoff, status, conflicts, and events have explicit semantics.
- [ ] Ownership is bound to canonical agent/session/task/worktree identities.
- [ ] A successful exclusive handoff cannot leave two owners.
- [ ] Stale/replayed mutations are rejected or handled idempotently according to the contract.
- [ ] Conflict results are based on explicit observable evidence.
- [ ] Collaboration events are correlated and queryable within documented bounds.

## Architecture
- [ ] One canonical collaboration implementation/domain owner exists.
- [ ] Existing agent/session/task/worktree/Git/auth/policy/audit contracts are reused or explicitly extended.
- [ ] No duplicate task, identity, worktree, authorization, or audit authority exists.
- [ ] Interface adapters preserve identical domain semantics.
- [ ] Task lifecycle is distinct from execution/session/worktree lifecycle.
- [ ] Restart and recovery behavior are explicit.

## Security
- [ ] Authorization is enforced at the canonical ownership mutation boundary.
- [ ] Identity and workspace mismatches are rejected.
- [ ] Ownership IDs cannot be used as authorization.
- [ ] Handoff cannot grant capabilities.
- [ ] Inputs, query sizes, and event resources are bounded.
- [ ] Secrets and sensitive tool payloads are redacted.
- [ ] Concurrent/replayed ownership mutations cannot bypass authorization or create invalid ownership.

## Persistence/recovery
- [ ] Exactly one authoritative collaboration persistence owner exists, or collaboration state is explicitly derived from another canonical owner.
- [ ] Required ownership state survives restart.
- [ ] Schema/version and migration behavior are documented.
- [ ] Atomic publication/concurrency semantics are tested.
- [ ] Corruption/truncation/partial publication is detected and handled fail-closed or by a documented recovery rule.
- [ ] Retention/cleanup ownership is explicit and cannot delete active ownership or cross-workspace state.

## Interfaces
- [ ] Applicable CLI commands use the canonical service.
- [ ] Applicable MCP tools use the canonical service.
- [ ] Applicable Control API endpoints use the canonical service.
- [ ] Applicable TUI actions use the canonical service.
- [ ] All applicable interfaces enforce the same identity, scope, authorization, concurrency, and error semantics.
- [ ] Real interface tests prove the paths work.

## Testing
- [ ] Unit tests pass.
- [ ] Integration tests pass.
- [ ] Adversarial/security tests pass.
- [ ] Failure/recovery tests pass.
- [ ] Concurrency/idempotency tests pass.
- [ ] Applicable real-interface tests pass.
- [ ] Repository verification gates pass.

## Documentation
- [ ] Collaboration contract and state transitions are documented.
- [ ] Ownership/persistence/recovery semantics are documented.
- [ ] Runtime behavior is not overstated.
- [ ] Known limitations and evidence gaps are reported.

# 11. Explicit non-goals

- No generic DAG/workflow engine.
- No autonomous swarm scheduler.
- No distributed consensus.
- No replacement AgentProfile/AgentRegistry/AgentSession system.
- No replacement task system.
- No second Git/worktree system.
- No second audit/event store.
- No distributed locking service.
- No model/provider router.
- No autonomous agent planner.
- No full merge-orchestration product unless a separately verified existing contract makes a narrowly scoped adapter necessary.

Do not expand into Remote, Distribution, Advanced Infrastructure, or unrelated orchestration except where their existing contracts are required as integration boundaries.

# 12. Final implementation report

Report:

```text
Feature:
Canonical implementation:
Canonical persistence owner:
Existing contracts reused:
Existing divergences found:
Files changed:
Interfaces added/changed:
Ownership/handoff semantics:
Conflict semantics:
Persistence/migrations:
Concurrency/idempotency:
Security controls:
Audit/observability:
Tests added:
Real-interface evidence:
Verification results:
Known limitations:
```

## Standalone execution rule

A developer must be able to read this prompt, inspect current `rust`, identify existing implementations, implement only this feature's missing/incomplete contract, test it, and verify acceptance without another prompt, PR, or undocumented assumption.
