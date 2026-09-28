# 22 — Terminal (TRM-001)

> Standalone implementation contract for the current `rust` branch. This prompt describes the implementation work; it does not claim that the feature is already complete.

## Prompt identity

**Prompt:** 22 / TRM-001  
**Feature:** Terminal  
**Primary roadmap area:** Bounded terminal execution  
**Target branch:** rust  
**Canonical folder:** docs/terminal/

### Mission

Implement, converge, and verify AWH terminal execution as one controlled local-process boundary.

This prompt is standalone:
- inspect the current `rust` branch before changing code;
- reuse existing contracts before adding abstractions;
- do not require another prompt or PR to be merged first;
- do not create duplicate identity, session, authorization, capability, policy, filesystem, snapshot, rollback, audit, Git, MCP, sandbox, or process-execution systems;
- keep external-agent reasoning, planning, model selection, and orchestration outside AWH.

# 1. Scope

## In scope

- Make `src/services/terminal.rs` the canonical terminal execution service.
- Converge the existing TUI, Control API, and MCP terminal paths onto that canonical service without creating alternate execution semantics.
- Implement the documented `awh terminal run|list|kill` contract if the current CLI does not yet expose it.
- Define one canonical process/execution model and lifecycle.
- Enforce the appropriate execution trust boundary before spawn:
  - AWH agent/session identity and `process.execute` capability/policy for agent-originated execution;
  - the existing explicitly authenticated operator trust boundary for operator Control API/TUI paths, without treating transport authentication as a substitute for AWH policy where policy is applicable.
- Enforce workspace/project/worktree and cwd scope.
- Keep execution argv-based; shell interpretation is not implicit.
- Enforce bounded environment inheritance, output capture, wall-clock timeout, process termination, and resource limits.
- Reuse the existing sandbox/resource-limit primitives where applicable and fail closed when a mandatory control cannot be established.
- Provide safe lifecycle tracking for `list`/status and `kill`.
- Emit correlated, redacted audit events for consequential execution lifecycle outcomes.
- Define explicit process-state persistence/restart reconciliation semantics rather than pretending OS processes are durable AWH state.

## Out of scope

- No unrestricted host shell service.
- No second sandbox implementation.
- No second process supervisor.
- No remote execution; that belongs to the Remote feature.
- No generic workflow/DAG scheduler.
- No model/provider routing or autonomous agent orchestration.
- No Git command semantics beyond using the existing Git/worktree scope when terminal execution is bound to one.

## Product boundary

AWH owns the **control plane** for local process execution: identity/trust, authorization, scope, argv/environment validation, limits, lifecycle, termination, and audit.

The child process owns the **execution work** itself. AWH must not claim that a child process is durable merely because its PID was recorded.

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
- `docs/mcp.md`
- `docs/roadmap/PROJECT_ROADMAP.md`
- `docs/roadmap/GROWTH_STRATEGY.md`
- `docs/roadmap/ROADMAP_AGENT_PROFILES_POLICY_MCP.md`
- implementation prompts 01–17 and the current terminal prompt.

Inspect at minimum:

- `src/services/terminal.rs`
- `src/services/authorization.rs`
- `src/services/audit.rs`
- `src/services/files.rs`
- `src/services/agent_runtime.rs`
- capability/policy modules
- `src/mcp/dispatcher.rs`
- `src/mcp/sandbox.rs` and platform sandbox modules
- `src/api/control.rs`
- `src/tui/backend.rs` and terminal-facing TUI code
- `src/main.rs`
- `src/services/mod.rs`
- existing identity/session/worktree/resource-limit contracts.

Search before adding abstractions:

```text
rg -n "TerminalService|ExecOutcome|terminal\.run|terminal run|terminal kill|process\.execute|Command::new|kill_on_drop|sandbox|timeout|output|resource"
rg -n "authorize_builtin_tool|authorize_mcp_execution|PolicyEngine|Capability|AgentRuntimeService|SessionIdentity"
rg -n "AuditLog|audit_allow|audit_deny|record_correlated|correlation"
rg -n "terminal|run_command|/terminal/run" src/main.rs src/api src/tui src/mcp
rg -n "TODO|FIXME|unimplemented!|todo!|panic!|unwrap\(|expect\("
```

### Existing-contract inventory

The inventory is part of the implementation contract: verify each row against the checked-out `rust` branch before coding. A prompt, issue, or missing file name is not sufficient evidence that a contract is absent.

Before implementation, replace/confirm this table against the current branch. Do not infer absence from this prompt alone.

| Existing contract | Current location | Action | Reason |
|---|---|---|---|
| `TerminalService` + `ExecOutcome` | `src/services/terminal.rs` | Reuse/extend | Existing canonical service candidate; currently argv-only with 30s timeout and 256 KiB capture cap, but it lacks the complete lifecycle/auth/list/kill contract. |
| MCP `terminal.run` dispatch | `src/mcp/dispatcher.rs` | Adapt | Already reaches `TerminalService`; preserve MCP schema/dispatcher semantics while routing through the completed canonical boundary. |
| MCP execution authorization/audit | `src/mcp/dispatcher.rs`, `src/mcp/*` | Reuse/adapt | Existing MCP trust/policy gates and audit hooks must remain authoritative for the MCP plane. |
| Agent/session identity | `src/services/agent_runtime.rs`, `src/core/identity.rs` | Reuse | Agent/session/workspace binding already exists; terminal must not invent a second identity model. |
| Capability/policy authorization | `src/services/authorization.rs`, policy/capability modules | Reuse/extend | `process.execute` is a consequential capability; authorization must be explicit at the canonical execution boundary or an equally authoritative shared gate. |
| Persistent audit | `src/services/audit.rs` | Reuse | Terminal execution is an auditable consequential action; do not add a terminal-local audit log. |
| Workspace/filesystem safety | `src/services/files.rs` and workspace services | Reuse | cwd/path validation must use existing scope/path safety rules. |
| Linux/platform sandbox | `src/mcp/sandbox.rs` and platform modules | Reuse/extend | Existing sandbox primitives are the source of truth; do not build a second sandbox. |
| Control API terminal route | `src/api/control.rs` | Adapt | Existing `/api/v1/terminal/run` is an operator control-plane path; it must use the canonical terminal contract. |
| TUI terminal backend | `src/tui/backend.rs` | Adapt | Existing local backend constructs `TerminalService`; remove/avoid duplicated lifecycle semantics. |
| CLI conventions | `src/main.rs`, `docs/CLI.md` | Extend | Add/complete terminal commands using established parsing, output, error, and exit conventions. |
| Git/worktree/session scope | Git/worktree/session services | Reuse where applicable | Terminal cwd and lifecycle may be bound to an AWH worktree/session; terminal must not become a second Git/worktree authority. |

If the implementation discovers that any row is stale, update the row in the implementation report and explain the replacement rather than silently introducing another authority.

# 3. Architecture

## Canonical architecture flow

```text
CLI / MCP / Control API / TUI
          |
          v
transport/operator or agent identity
          |
          v
execution authorization + capability/policy
          |
          v
workspace/project/worktree/cwd validation
          |
          v
argv + environment + resource-limit validation
          |
          v
sandbox / platform execution boundary
          |
          v
TerminalService process registry
          |
          +----> bounded stdout/stderr
          |
          +----> timeout / kill / exit reconciliation
          |
          v
canonical TerminalResult / lifecycle state
          |
          v
persistent AuditLog (redacted + correlated)
```

Only the adapters differ by transport. The process semantics, validation, lifecycle state, termination semantics, and error taxonomy must be shared.

## Canonical responsibility

`TerminalService` owns terminal execution semantics and process lifecycle state.

It must not own:
- agent identity;
- MCP protocol sessions;
- global authorization policy;
- workspace filesystem policy;
- Git/worktree identity;
- audit persistence;
- remote transport.

Those remain existing authorities and are injected/called at the boundary.

## Architectural invariants

1. There is one canonical terminal execution service.
2. There is one canonical execution request/result/error vocabulary.
3. Agent/session/workspace/worktree identity is reused rather than recreated.
4. Capability/policy decisions remain authoritative.
5. Operator authentication and agent authorization are distinguished; neither is silently substituted for the other.
6. Every interface reaches the same execution semantics.
7. Raw PIDs are process handles, not authorization identities.
8. A kill request is authorized against the AWH execution record/session/scope before a signal is sent.
9. argv entries are never silently joined into a shell command.
10. Environment inheritance is explicit and bounded.
11. cwd is validated against the authorized workspace/project/worktree scope.
12. Timeout and output limits are hard execution controls, not advisory metadata.
13. Process state is reconciled with the OS; stale metadata is never treated as a live process.
14. Audit records do not contain credentials, secret environment values, or unbounded command output.
15. Interface adapters do not introduce alternate business semantics.
16. Mandatory security/resource controls fail closed.
17. Existing locking/concurrency primitives are reused.

## Terminal lifecycle

Use one canonical lifecycle, for example:

```text
Requested
  -> Authorized
  -> Validated
  -> Spawned
  -> Running
  -> Completed | Failed | TimedOut | Killed | SpawnFailed
```

If a process disappears without a known terminal outcome, reconciliation must produce an explicit state such as `Unknown`/reconciled failure rather than inventing success.

Distinguish:
- **task state** from **process state**;
- **agent session state** from **terminal execution state**;
- **transport/MCP session state** from **OS process state**.

Starting a terminal process must not implicitly create/complete a Task, start an AgentSession, or grant a capability.

## Command semantics

The canonical request must represent:
- executable/program as one argv entry;
- arguments as separate argv entries;
- cwd as an explicitly scoped path;
- environment policy as an explicit decision;
- timeout/resource/output limits as typed values;
- caller identity/correlation metadata;
- an opaque AWH execution ID for lifecycle operations.

Shell execution, if ever supported, must be an explicit separate high-risk mode with its own capability/policy requirement. It must never be inferred from a string containing spaces or shell metacharacters.

# 4. Interfaces

## Rust/service interface

The canonical service should expose typed request/result/error contracts sufficient for:
- start/run;
- inspect/list;
- kill/terminate;
- lifecycle state;
- exit code/signal information where available;
- timeout/limit status;
- bounded stdout/stderr;
- execution ID and correlation IDs.

The service must not expose raw internal authorization objects as its public contract and must not use a PID alone as a stable AWH identifier.

## CLI interface

Implement/complete:

```text
awh terminal run ...
awh terminal list
awh terminal kill <execution-id>
```

The exact flags must follow existing CLI conventions, but the contract must make argv, cwd, timeout, output/resource limits, and execution identity unambiguous.

CLI requirements:
- stable exit codes;
- safe stdout/stderr separation;
- no secret environment printing;
- no shell reinterpretation;
- kill by AWH execution ID, not arbitrary PID;
- deterministic behavior for unknown/already-finished execution IDs.

## Control API interface

The existing `/api/v1/terminal/run` path must delegate to the canonical service.

If list/kill endpoints are exposed, they must use the same execution ID and result model.

The Control API is an authenticated operator plane in the current architecture. Preserve its documented bearer-token boundary, but do not let API transport authentication become an implicit authorization bypass for service calls that require AWH policy.

Never make a raw PID an authorization credential.

## MCP interface

The existing MCP `terminal.run` path must:
1. validate schema;
2. resolve/bind the MCP caller/session/workspace;
3. apply the existing MCP execution/trust/policy gates;
4. invoke the canonical TerminalService;
5. return bounded structured results;
6. emit the existing correlated audit outcome.

If MCP kill/list tools are added, they must use the same canonical execution ID and authorization semantics.

## TUI interface

TUI terminal actions must call the canonical TerminalService/backend abstraction.

The TUI may provide explicit operator confirmation, but confirmation is not a second authorization system and must not create TUI-local process records.

Remote TUI operations must use the existing Control API/remote backend; they must not create a second terminal RPC.

## Interfaces acceptance

All applicable interfaces are accepted only when:

- CLI, MCP, Control API, and TUI use the same canonical process request/result semantics;
- the same execution ID can be inspected/killed through applicable interfaces without changing its authorization meaning;
- transport-specific validation happens before service execution;
- authorization is enforced before spawn and before kill;
- bounded-output, timeout, termination, and error semantics are consistent;
- no adapter directly constructs an alternate process manager;
- real interface tests prove at least one end-to-end execution path for every implemented interface.

# 5. Security

Security requirements are implementation requirements, not documentation-only goals.

## Authorization

For agent-originated execution:
- require a valid AWH agent/session/workspace binding;
- require `process.execute`;
- evaluate the applicable PolicyEngine/capability rules;
- reject inactive/disabled/unknown/mismatched identities;
- enforce authorization before process spawn;
- enforce authorization again for lifecycle mutations such as kill.

For operator Control API/TUI execution:
- preserve the existing authenticated operator trust boundary;
- scope the operation to the authenticated control-plane root/session where applicable;
- do not silently treat an operator token as an agent identity.

Internal callers must not bypass the authoritative execution gate merely because they can construct `TerminalService`.

## Command injection and shell safety

- Never pass an assembled command string to a shell by default.
- Preserve argv boundaries exactly.
- Reject ambiguous executable representations where necessary.
- If explicit shell mode exists, require a distinct high-risk capability/policy and record that mode in audit.
- Never use user-controlled strings to construct an implicit `sh -c`, `cmd /C`, PowerShell, or equivalent invocation.

## cwd/path safety

- Resolve cwd relative to the authorized workspace/project/worktree scope.
- Reject traversal, encoded traversal, symlink escape, and out-of-scope paths using existing filesystem safety contracts.
- Reject nonexistent/invalid cwd before spawn.
- Do not let an agent choose an arbitrary host directory by passing an absolute path unless policy explicitly permits it.

## Environment safety

- Do not inherit the full server environment by default.
- Define an explicit allowlist/filtered inheritance model.
- Reject dangerous environment overrides where existing policy says they are unsafe.
- Never expose AWH/GitHub/provider/API credentials to child processes unless an explicit, authorized secret/environment contract permits it.
- Never include secret environment values in audit, logs, errors, or process listings.

## Resource exhaustion

Bound at minimum:
- wall-clock execution time;
- captured stdout bytes;
- captured stderr bytes;
- argv size/count where needed;
- environment size;
- concurrent running processes;
- process resource limits supported by the platform/sandbox.

Output caps must operate on bytes before unbounded conversion to strings.

## Termination safety

- Kill by canonical AWH execution ID plus authorized scope.
- Handle PID reuse safely.
- Prefer process-group/job-object/task-group termination where the platform contract supports it, so child processes cannot trivially survive the parent kill.
- Distinguish graceful termination, forced termination, timeout, and already-exited outcomes.
- Do not claim a kill succeeded until the lifecycle state has been reconciled.
- Prevent one session/agent/workspace from killing another execution.

## Sandbox

Reuse the existing sandbox/resource-limit primitives. If a mandatory sandbox is unavailable on a platform where the contract requires it, fail closed rather than silently degrading to unrestricted execution.

Platform-specific support must be explicit. Do not describe Linux sandbox behavior as universal across Windows, macOS, or Android/Termux without evidence.

## Audit and secrets

Audit at least:
- request/attempt;
- authorization outcome;
- spawn outcome;
- completion/failure;
- timeout/kill;
- resource-limit termination where applicable.

Correlate with execution ID and available agent/session/workspace/request/task identifiers.

Record program identity and safe argument metadata only within the existing redaction rules. Never persist full secret environments or unbounded stdout/stderr as audit payloads.

## Required threat cases

Test:
- unauthorized agent;
- disabled/inactive session;
- forged or mismatched workspace/session identity;
- cross-workspace cwd;
- symlink escape;
- shell/metacharacter injection;
- dangerous environment injection;
- secret environment leakage;
- oversized argv/environment/output;
- resource exhaustion;
- timeout race;
- PID reuse;
- cross-session kill;
- concurrent list/kill/run;
- duplicate lifecycle requests;
- crash/restart;
- sandbox setup failure;
- audit failure.

# 6. Persistence and recovery

## Persistence owner

The **TerminalService owns the terminal execution lifecycle model**, but live OS process state is inherently ephemeral.

If durable metadata is required for `list`, restart reconciliation, or audit correlation, introduce exactly one terminal-owned persistence authority under the workspace's existing `.agent/` state. A suitable terminal-specific location may be `.agent/terminal/`, subject to current repository conventions.

The persistence contract must clearly distinguish:

| State | Durable? | Recovery meaning |
|---|---:|---|
| Program argv | bounded metadata only | Historical/request metadata; never an authorization grant. |
| Execution ID | Yes if durable tracking is implemented | Stable AWH correlation handle. |
| PID | At most advisory | Never a durable identity; validate ownership before signalling. |
| Running child process | No | OS process may outlive/restart independently; reconcile explicitly. |
| Terminal result | Yes if history is required | Historical outcome, not a live process claim. |
| Audit event | Yes | Owned by canonical AuditLog, not terminal storage. |
| Agent/session identity | No duplicate copy | Re-resolve against AgentRuntimeService/identity authority. |

Adapters must never write terminal state directly.

## Publication and schema

If durable terminal metadata is introduced:
- define one schema/version;
- use atomic publication;
- use the existing locking/concurrency primitives;
- validate IDs and paths before filesystem access;
- detect truncation/corruption/incompatible versions;
- never load partially published state as authoritative;
- define migration behavior before changing the schema.

## Restart reconciliation

On startup:
1. load and validate terminal metadata;
2. resolve the associated workspace/session/agent through existing authorities;
3. inspect whether recorded processes are still valid and attributable;
4. never trust a PID without ownership/identity validation;
5. mark stale/unverifiable records as reconciled terminal/unknown state;
6. do not claim a child process is durable merely because metadata survived;
7. emit an audit event when a material reconciliation occurs, subject to existing audit policy.

If durable terminal history is not required by the final implementation, keep lifecycle state ephemeral and document that `list` only covers the current process lifetime.

## Retention and cleanup

Define:
- maximum retained terminal-history size/count if history is durable;
- cleanup trigger;
- whether cleanup is automatic or explicit;
- whether audit retention is separate and remains owned by AuditLog;
- workspace isolation during cleanup.

Terminal cleanup must never delete another workspace's state and must not mutate audit history as a side effect.

# 7. Integration boundaries

| Boundary | Existing authority | Terminal responsibility |
|---|---|---|
| Workspace/project | init/workspace/project services | resolve and enforce execution scope |
| Agent/session | AgentRuntimeService + identity | resolve and propagate caller identity |
| Capability/policy | PolicyEngine + authorization | authorize execution and lifecycle mutations |
| Filesystem/cwd | FilesService/path safety | validate cwd and resource scope |
| Sandbox/resources | existing MCP/platform sandbox modules | invoke existing controls; no duplicate sandbox |
| Audit | persistent AuditLog | emit correlated, redacted lifecycle events |
| Git/worktree | Git/worktree services | reuse current worktree/session scope where applicable |
| MCP | dispatcher/session/auth gates | transport/schema adapter only |
| Control API | versioned `/api/v1` | authenticated operator adapter |
| TUI | WorkspaceBackend/remote backend | presentation/control adapter |
| Tasks | canonical TaskService | do not create or mutate task state implicitly |
| Context/memory | existing services | do not store terminal output as memory/context implicitly |

## No-duplication rule

Do not introduce parallel:
- process supervisors;
- shell runners;
- sandbox engines;
- identity/session systems;
- authorization/policy systems;
- filesystem path-safety systems;
- audit stores;
- MCP session systems;
- task/workflow systems.

# 8. Testing

## Unit tests

Test:
- argv validation;
- shell-mode rejection/default behavior;
- cwd scope validation;
- environment filtering;
- output byte caps;
- timeout calculation;
- lifecycle transitions;
- kill authorization;
- error classification;
- execution-ID validation;
- PID-reuse protection logic where representable without a live OS process.

## Service/integration tests

Use real benign commands through the compiled binary/service:
- successful argv execution;
- non-zero exit;
- missing executable;
- timeout;
- output truncation;
- kill;
- already-completed kill;
- concurrent run/list/kill;
- cwd inside/outside workspace;
- restart/reconciliation when durable metadata exists;
- audit correlation.

Do not use a shell merely to simplify test command construction.

## Security/adversarial tests

Prove:
- no execution before authorization;
- no execution after authorization denial;
- shell metacharacters remain argv data;
- cwd traversal/symlink escape is rejected;
- environment secrets are not inherited unexpectedly;
- output caps cannot be bypassed;
- cross-session/workspace kill is denied;
- PID reuse cannot target an unrelated process;
- mandatory sandbox/resource-control failure prevents spawn.

## Failure/recovery tests

Cover:
- spawn failure;
- timeout race;
- process exits while kill is requested;
- process-group termination failure;
- sandbox setup failure;
- corrupted/truncated terminal metadata;
- incompatible schema;
- concurrent metadata update;
- crash/restart;
- audit failure.

If audit is mandatory for a given execution plane, define whether an audit-write failure blocks spawn or converts the operation into a deterministic failure. Do not silently invent best-effort semantics.

## Compatibility/real-interface tests

For every implemented surface, test the real boundary:
- compiled CLI;
- MCP dispatcher/client path;
- Control API endpoint;
- TUI backend where terminal execution is exposed.

Also test platform-specific sandbox/resource behavior only on platforms where the environment provides the required evidence.

## Verification gates

```text
cargo fmt --all -- --check
cargo check --all-targets
cargo test --all-targets
cargo clippy --all-targets --all-features -- -D warnings
git diff --check
```

If an external platform, terminal emulator, sandbox facility, or release environment is unavailable, record the evidence gap instead of claiming verification.

# 9. Rollout

## Terminal-specific rollout sequence

1. **Forensics:** inventory the current `TerminalService`, MCP dispatcher path, Control API path, TUI backend, sandbox primitives, policy/capability gates, and audit behavior.
2. **Canonical model:** define one execution request/result/error model and one execution lifecycle.
3. **Execution safety:** establish argv, cwd, environment, timeout, output, resource, and sandbox invariants.
4. **Authorization convergence:** connect agent/session/capability/policy and preserve the explicitly operator-scoped Control API/TUI trust boundary without duplicating authorization systems.
5. **Lifecycle:** add execution IDs, list/inspect, kill, process-group handling, and race-safe state reconciliation.
6. **Persistence decision:** decide whether durable terminal history/restart reconciliation is required; if yes, establish the one terminal-owned store and schema; otherwise explicitly keep lifecycle state ephemeral.
7. **Interface convergence:** migrate CLI/MCP/Control API/TUI adapters to the canonical service and remove alternate process semantics.
8. **Audit integration:** emit correlated, redacted outcomes for all consequential execution states.
9. **Adversarial verification:** run command-injection, scope, environment, resource, timeout, kill-race, PID-reuse, restart, and sandbox tests.
10. **Repository verification:** run all verification gates and report evidence gaps honestly.
11. **Documentation:** update only feature-specific documentation whose behavior is proven by tests/evidence.

No step depends on another prompt or PR being merged first.

## Compatibility

Preserve:
- existing argv-only `TerminalService::run` behavior where compatible;
- the current 30-second default timeout and 256 KiB capture ceiling unless the final contract explicitly changes them;
- existing MCP terminal schema where possible;
- existing operator Control API authentication semantics.

Any intentional compatibility change must be documented with migration/test evidence.

## Failure behavior

If required authorization, cwd validation, environment policy, mandatory sandbox, or mandatory resource limits cannot be established, **do not spawn**.

If termination cannot be confirmed, return a deterministic non-success state rather than claiming the process was killed.

Observer/logging failures must never fabricate successful execution outcomes.

# 10. Acceptance criteria

## Functional

- [ ] One canonical TerminalService owns execution semantics.
- [ ] One canonical execution request/result/error vocabulary exists.
- [ ] `run`, `list`, and `kill` semantics are defined and tested where exposed.
- [ ] argv execution is shell-free by default.
- [ ] cwd/workspace/worktree scope is enforced.
- [ ] timeout, output, environment, and resource limits are enforced.
- [ ] kill uses an AWH execution ID and reconciles the resulting state.
- [ ] process/task/session lifecycles remain distinct.
- [ ] non-zero exits, spawn failures, timeout, and kill outcomes are distinguishable.

## Architecture

- [ ] Repository forensics are reflected in the existing-contract inventory.
- [ ] The architecture flow is implemented without a second process/sandbox/auth system.
- [ ] MCP, CLI, API, and TUI adapters preserve canonical service semantics.
- [ ] Existing identity, policy, filesystem, sandbox, audit, and Git/worktree contracts are reused.
- [ ] Raw PID is never treated as an authorization identity.
- [ ] Restart/reconciliation semantics are explicit.
- [ ] The implementation does not turn terminal execution into a task/workflow scheduler.

## Security

- [ ] Agent execution requires valid identity/session plus applicable `process.execute` authorization.
- [ ] Operator Control API/TUI trust remains explicitly authenticated and scoped.
- [ ] Authorization occurs before spawn and before kill.
- [ ] Shell injection is impossible in the default argv path.
- [ ] cwd traversal/symlink escape is rejected.
- [ ] Environment inheritance is bounded and secrets are protected.
- [ ] Output/resource/process limits are enforced.
- [ ] Mandatory sandbox/security controls fail closed.
- [ ] Cross-session/workspace process signalling is prevented.
- [ ] Audit records are redacted.

## Persistence/recovery

- [ ] Exactly one persistence owner exists if durable terminal metadata is required.
- [ ] Live OS process state is explicitly treated as ephemeral.
- [ ] Schema/version/publication/locking behavior is documented and tested if durable.
- [ ] Corruption/truncation/incompatible state is detected.
- [ ] Restart reconciliation cannot trust stale PIDs blindly.
- [ ] Retention/cleanup semantics are explicit and workspace-scoped.
- [ ] Terminal storage does not duplicate AuditLog history.

## Interfaces

- [ ] Rust/service contract is typed and stable.
- [ ] CLI behavior is tested through the compiled binary where implemented.
- [ ] MCP uses the existing dispatcher/auth/schema path.
- [ ] Control API delegates to the same service.
- [ ] TUI uses the same service/backend semantics.
- [ ] Applicable real-interface tests demonstrate semantic equivalence.

## Testing

- [ ] Unit tests pass.
- [ ] Integration tests pass.
- [ ] Security/adversarial tests pass.
- [ ] Failure/recovery tests pass.
- [ ] Concurrency/termination race tests pass.
- [ ] Applicable sandbox/platform tests pass.
- [ ] Repository verification gates pass.

## Documentation

- [ ] Terminal contract documents argv/shell semantics.
- [ ] Authorization/trust-plane distinctions are documented.
- [ ] Persistence/restart/retention behavior is documented.
- [ ] Runtime behavior is not overstated.
- [ ] Known platform/evidence gaps are reported.

# 11. Explicit non-goals

- No remote execution.
- No unrestricted shell.
- No second process supervisor.
- No second sandbox.
- No second authorization/policy system.
- No durable OS-process illusion.
- No generic task/DAG/workflow scheduler.
- No model routing or agent reasoning.

Do not expand this prompt into the Remote, Collaboration, Distribution, or Advanced Infrastructure feature families except where their existing contracts are required as integration boundaries.

# 12. Final implementation report

Report:

Feature:  
Canonical implementation:  
Existing contracts reused:  
Existing-contract divergences resolved:  
Files changed:  
Interfaces added/changed:  
Execution lifecycle:  
Authorization/trust planes:  
Sandbox/resource controls:  
Persistence owner/schema/retention:  
Restart/reconciliation behavior:  
Security controls:  
Audit/observability:  
Tests added:  
Real-interface evidence:  
Verification results:  
Known limitations/platform gaps:

## Standalone execution rule

A developer must be able to read this prompt, inspect current `rust`, identify existing implementations, implement only this feature's missing/incomplete contract, test it, and verify acceptance without another prompt, PR, or undocumented assumption.
