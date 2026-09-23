# Prompt 13 — Agent Worktree Isolation (GIT-001)

## Mission

Implement one canonical AWH-managed Git worktree isolation boundary for Git-backed agent sessions.

The implementation must turn the existing Git service, canonical workspace identity, and AWH agent/session model into a safe lifecycle for assigning an isolated Git worktree to a specific runtime session. A worktree is a filesystem/Git execution boundary, not merely a branch name or a path convention.

The implementation must be honest about the current `rust` branch: repository forensics show that `src/services/git.rs` currently provides structured Git operations such as status, stage/unstage, commit, log, branch, diff, push/pull, reset, clean, and branch deletion, but it does not currently own a worktree manager. Existing agent persistence also does not by itself prove runtime isolation. Do not treat roadmap commands or the existing TUI's phrase “diff worktree” as evidence that AWH-managed agent worktrees already exist.

This prompt is standalone and must be executable against the current `rust` branch without waiting for another implementation-prompt PR or assuming that another prompt has been merged.

---

## 1. Product boundary

AWH owns the mapping:

```
Workspace
  -> Agent
    -> AgentSession
      -> Worktree
        -> filesystem/Git operations
```

For a Git-backed session, the effective workspace used by consequential filesystem/Git operations must be the session's assigned worktree when isolation is enabled.

External agents remain responsible for reasoning, planning, model selection, and agent intelligence. AWH owns the workspace, worktree, authorization context, lifecycle, and safety boundary.

A worktree must never be treated as authorization by itself. Capability/policy authorization remains authoritative for consequential operations.

---

## 2. Scope

This prompt owns:

- canonical Worktree identity and lifecycle state;
- repository/worktree discovery and validation;
- create/list/inspect/remove lifecycle;
- binding a worktree to an AWH workspace, agent, and session;
- safe worktree path allocation;
- branch/ref selection and uniqueness;
- isolation of session filesystem roots;
- recovery/reconciliation of stale or missing lifecycle records;
- safe cleanup when policy permits;
- protection against cross-agent/workspace access;
- Git command safety and argument handling;
- lifecycle audit integration;
- CLI/service integration required to expose the worktree contract;
- tests proving real isolation.

This prompt does not own:

- agent reasoning or orchestration;
- model routing;
- distributed agent scheduling;
- merge strategy/product;
- GitHub/GitLab hosting integration;
- MCP protocol redesign;
- a new capability/policy engine;
- a new agent identity/session system;
- a new snapshot store;
- a new rollback engine;
- a new audit database;
- filesystem-wide locking or distributed locking;
- Git history rewriting;
- automatic conflict resolution;
- container/VM sandboxing.

Do not implement a second version of any of those systems.

---

## 3. Required repository forensics

Before changing source, inspect the current `rust` branch and map the actual implementation.

At minimum inspect:

- `docs/roadmap/GROWTH_STRATEGY.md`;
- `docs/roadmap/PROJECT_ROADMAP.md`;
- `docs/roadmap/ROADMAP_AGENT_PROFILES_POLICY_MCP.md`;
- `docs/FEATURES.md`;
- `docs/PROJECT_CONTEXT.md`;
- `docs/architecture.md`;
- `docs/security.md`;
- `docs/threat-model.md`;
- `docs/implementation-prompts/README.md`;
- Prompts 01–12 for existing identity, authorization, editing, snapshot, rollback, audit, MCP, and CLI boundaries;
- Prompts 14–17 as ownership/non-overlap references;
- relevant historical material under `docs/trust-wedge/`, `docs/issue-resolving-prompts/`, and archived roadmap/forensic documents when present;
- `src/services/git.rs`;
- `src/services/mod.rs`;
- `src/core/agents.rs`;
- `src/core/identity.rs`;
- the canonical session/runtime modules actually present on the branch;
- workspace/root/path-validation modules;
- `src/main.rs`;
- MCP Git/tool dispatch;
- TUI Git integration;
- Control API Git/runtime integration;
- audit, authorization, and filesystem services;
- existing tests and CI.

Also search the entire source tree for:

- `worktree`, `git worktree`, `AgentSession`, `SessionLifecycle`;
- direct `Command::new("git")` calls;
- branch/workspace path construction;
- direct filesystem access to another workspace;
- existing lifecycle/state persistence;
- existing locks and atomic persistence.

Produce an implementation map before coding. Explicitly classify each relevant component as reusable, partial, incompatible, or absent.

Do not create a duplicate implementation merely because the current service is incomplete.

---

## 4. Current repository contract

The current Git abstraction is `GitService`, which invokes Git through explicit argument vectors and applies a timeout. Reuse it as the canonical Git invocation boundary.

Do not build shell command strings such as `git worktree add ...` and execute them through a shell.

The existing `GitService` path validation is repository-relative and rejects absolute/traversal paths. Extend the canonical service only where necessary for worktree lifecycle semantics; do not create a parallel Git runner.

Existing `AgentStore` is persisted under the workspace's `.agent` directory. Existing agent records do not by themselves constitute runtime isolation. If a canonical AgentSession implementation exists on the branch, bind worktree ownership to that identity rather than inventing another session ID model.

If the current session implementation is still absent or protocol-only, implement only the minimum local binding contract required for worktree ownership and clearly document the boundary; do not recreate Prompt 02's complete identity/session subsystem.

---

## 5. Canonical Worktree model

Define one canonical Worktree representation containing, as appropriate to the existing identity contracts:

- stable WorktreeId;
- WorkspaceId;
- AgentId;
- AgentSessionId;
- repository identity;
- worktree filesystem path;
- branch/ref identity;
- lifecycle state;
- creation timestamp;
- optional removal timestamp;
- ownership/recovery metadata;
- schema version.

Use stable identifiers rather than deriving identity from arbitrary filesystem strings.

The model must distinguish at least:

- planned/creating;
- active;
- removing;
- removed;
- missing/orphaned;
- failed/recovery-required.

Do not report an `active` worktree when the filesystem/Git state cannot be verified.

Unknown schema versions and malformed lifecycle records must fail closed rather than being silently interpreted.

---

## 6. Repository and workspace validation

Before creating a worktree:

1. Resolve the canonical AWH workspace root.
2. Validate that the workspace is the intended Git working tree/repository.
3. Determine the repository identity using Git, not only directory names.
4. Validate the requested branch/ref according to Git's actual rules.
5. Allocate a dedicated worktree path that cannot escape the approved AWH worktree area.
6. Ensure the target path is absent or is an explicitly recognized managed worktree.
7. Confirm the target branch/ref and ownership do not conflict with another managed worktree.
8. Only then invoke Git worktree creation.

A non-Git workspace must not silently fall back to an ordinary shared directory when the caller requested isolated Git execution.

---

## 7. Worktree path containment

Managed worktrees must live under a canonical AWH-controlled location or another explicitly configured safe root.

Reject:

- absolute user-supplied paths outside the approved root;
- `..` traversal;
- empty/ambiguous paths;
- path-prefix tricks;
- symlink escapes;
- a worktree path equal to the main workspace root;
- a path already owned by another worktree;
- a path that resolves outside the approved worktree root.

Validate both lexical path structure and the filesystem state at the final mutation boundary.

Do not assume that a string prefix such as `/workspace/agent-a` proves containment of `/workspace/agent-a-other`.

Where canonicalization is possible, verify the resolved path. For paths that do not yet exist, validate the nearest existing ancestor and the intended final path according to the platform's capabilities.

Never claim universal symlink/race freedom when the OS cannot provide it.

---

## 8. Worktree creation

Creation must be transactional at the AWH lifecycle level:

```
validate identity
  -> validate repository
  -> allocate unique path
  -> reserve lifecycle identity
  -> invoke canonical Git worktree creation
  -> verify Git reports the expected worktree
  -> verify path containment
  -> persist active binding
  -> audit outcome
```

Do not publish `active` before Git and filesystem verification succeeds.

If Git creation fails:

- do not leave an apparently active record;
- clean up only artifacts proven to belong to this failed creation attempt;
- preserve evidence necessary for diagnosis without leaking secrets;
- return a structured failure.

If persistence fails after Git creation, enter an explicit recovery/reconciliation state rather than pretending creation did not happen.

A retry must not accidentally create a second worktree for the same session.

---

## 9. Branch and ref isolation

A worktree must have deterministic branch/ref semantics.

The implementation must define and test:

- whether a new branch is created;
- how an existing branch is handled;
- whether detached HEAD is allowed;
- whether two sessions may intentionally share a ref;
- whether two managed worktrees may point to the same branch;
- what happens when Git refuses a branch because it is already checked out;
- what happens when the requested ref disappears.

Default behavior must favor isolation. Never silently switch another worktree's branch, reset it, or alter another worktree's index.

Do not use `git reset --hard`, `git clean`, branch deletion, or force operations as hidden cleanup mechanisms.

---

## 10. Ownership and cross-agent isolation

The effective ownership invariant is:

```
Worktree.WorkspaceId == Session.WorkspaceId
Worktree.AgentId == Session.AgentId
Worktree.AgentSessionId == current session
```

where the corresponding identity fields exist.

A request operating on a worktree must resolve the canonical Worktree record and verify ownership before exposing or mutating it.

Never authorize access merely because a caller knows:

- the filesystem path;
- WorktreeId;
- branch name;
- repository name;
- agent name.

Authorization must use the existing AWH identity/capability/policy boundary.

Cross-agent and cross-workspace access must fail closed and must have zero mutation side effects.

---

## 11. Filesystem operation binding

When a session is assigned a worktree, downstream AWH services that operate on session-scoped workspace state must receive the worktree root as their effective root.

Do not implement a second FilesService, EditService, snapshot store, rollback store, or path validator.

The worktree layer should provide the validated root/context to existing services.

A caller must not be able to escape from a worktree by supplying:

- `../`;
- absolute paths;
- symlinks;
- another managed worktree path;
- the original repository root;
- a sibling agent's worktree.

Tests must prove that the same protections apply through CLI, MCP, TUI, and service callers where those interfaces expose worktree-scoped operations.

---

## 12. List and inspect

Implement one canonical worktree listing/inspection service.

`list` must:

- return only records visible to the authorized workspace/context;
- be deterministic;
- distinguish active, removing, removed, missing, and recovery-required states;
- not expose secrets;
- not require scanning arbitrary host directories.

`inspect` must verify the recorded Git worktree against actual Git state before declaring it active.

Detect at least:

- record exists but Git worktree is gone;
- Git worktree exists but AWH record is gone;
- path changed unexpectedly;
- branch/ref mismatch;
- repository mismatch;
- path containment failure;
- duplicate ownership;
- malformed metadata.

Never silently adopt an unmanaged worktree into another agent session.

---

## 13. Removal and cleanup

Removal must be explicit and ownership-checked.

Normal lifecycle:

```
authorize
  -> verify ownership
  -> verify current worktree identity
  -> transition to removing
  -> invoke canonical Git worktree removal
  -> verify Git/filesystem state
  -> persist removed state
  -> audit
```

Cleanup after session termination may occur only when the existing product/session policy permits it.

Before destructive removal, verify that:

- the worktree is still owned by the same session;
- the path is still the expected managed path;
- the repository identity is unchanged;
- the operation will not remove the main workspace;
- the worktree is not already removed;
- no unrelated path has replaced the expected worktree.

Never use recursive filesystem deletion as a substitute for Git worktree removal unless a narrowly defined, verified orphan-cleanup path explicitly permits it.

If Git reports uncommitted changes or refuses removal, preserve the worktree and report the condition. Do not silently discard user/agent changes.

---

## 14. Recovery and reconciliation

The runtime must tolerate:

- process crash during creation;
- process crash during removal;
- restart with stale lifecycle records;
- manually removed worktrees;
- manually added unmanaged worktrees;
- repository moved or deleted;
- branch deleted;
- corrupted worktree metadata;
- partial cleanup.

Startup/reconciliation must be deterministic.

For every managed record:

1. validate schema;
2. validate workspace/repository identity;
3. query Git's actual worktree state;
4. verify path containment;
5. compare ownership metadata;
6. classify the record;
7. repair only when the repair is unambiguous and safe;
8. otherwise mark recovery-required.

Never fabricate an active state from a missing or ambiguous worktree.

Never automatically delete an unmanaged path merely because it resembles an old worktree path.

---

## 15. Concurrency and duplicate creation

Multiple agents may create worktrees concurrently.

The implementation must guarantee, within the platform's supported concurrency model:

- unique managed worktree identity;
- no two sessions accidentally receive the same path;
- no duplicate active record for one session;
- no lost ownership update;
- no corrupt lifecycle record;
- no cleanup of a worktree concurrently being created or reused;
- deterministic behavior when two sessions request the same branch/path.

Use the smallest synchronization scope that protects the lifecycle state.

Reuse existing AWH lock/atomic-persistence primitives where applicable.

Do not introduce a distributed locking service.

Do not claim that filesystem races are universally impossible; document the remaining platform limits.

---

## 16. Git command safety

All Git operations must use structured argv.

Arguments supplied by users/agents must never be interpolated into a shell command.

Validate:

- branch names;
- ref names;
- worktree paths;
- repository paths;
- IDs;
- optional cleanup flags.

High-risk Git operations must remain explicit and must not be hidden inside normal lifecycle operations.

The worktree implementation must not silently invoke:

- hard reset;
- clean;
- force push;
- branch deletion;
- discard-file operations.

Git stderr may contain paths or repository details; return structured, sanitized errors at external interfaces according to existing AWH error conventions.

---

## 17. Authorization and capability boundary

The authorization sequence must remain conceptually:

```
caller identity
  -> agent/session/workspace binding
  -> capability authorization
  -> policy evaluation
  -> Worktree ownership/resource validation
  -> Git/filesystem operation
  -> audit
```

Use the existing authorization system.

Do not make possession of a WorktreeId, path, or branch an authorization mechanism.

Do not create a Git-specific policy engine.

Do not weaken default-deny behavior.

A denied create/remove/inspect operation must not create, remove, or mutate a worktree.

---

## 18. Audit and provenance

Use the canonical persistent audit boundary when available on the current branch.

Record consequential lifecycle events such as:

- create requested/allowed/denied;
- create succeeded/failed;
- inspect/reconciliation anomaly;
- remove requested/allowed/denied;
- remove succeeded/failed;
- recovery classification;
- cross-agent access denial.

Correlate with existing:

- workspace identity;
- agent identity;
- session identity;
- WorktreeId;
- request/task/edit IDs when available.

Do not store bearer tokens, API keys, private keys, passwords, full file contents, or unnecessary host-sensitive data.

Do not create a worktree-specific audit database.

Worktree lifecycle events must describe the Git/worktree operation; they must not impersonate edit/snapshot/rollback events owned by those services.

---

## 19. CLI contract

Expose only the worktree CLI surface supported by the current roadmap and existing command architecture, with a thin adapter over the canonical service.

The target contract includes the equivalent of:

```
awh worktree create
awh worktree list
awh worktree inspect
awh worktree remove
```

If an existing command parser already contains these commands, wire them to the canonical service instead of adding duplicates.

If merge is not already a clearly defined product contract, do not implement a merge engine merely because historical roadmaps mention `awh worktree merge`.

CLI behavior must have:

- stable success output;
- stable non-zero failure behavior;
- no leaked internal paths/secrets;
- no interactive confirmation that substitutes for authorization;
- bounded behavior in noninteractive agent execution;
- deterministic handling of missing worktrees and conflicts.

---

## 20. MCP/TUI/Control API integration

Where current interfaces expose Git/workspace lifecycle operations, route them through the canonical Worktree service.

MCP, CLI, TUI, and Control API must not each implement their own:

- worktree path allocator;
- Git worktree command runner;
- lifecycle state store;
- ownership check.

Keep adapters thin.

Do not redesign MCP routing, Control API versioning, or TUI architecture in this prompt.

---

## 21. Persistence and schema safety

Persist lifecycle metadata atomically using existing AWH persistence conventions.

Required properties:

- explicit schema version;
- deterministic serialization;
- atomic publication;
- corruption detection;
- no silent truncation;
- safe restart behavior;
- bounded record size;
- no arbitrary filesystem paths from untrusted metadata;
- no secret material.

A corrupted record must not be interpreted as ownership of a real worktree.

A persistence failure must not be converted into a successful lifecycle result.

If Git state exists but the record cannot be safely persisted, enter recovery-required and report the discrepancy.

---

## 22. Interaction with editing, snapshots, rollback, and audit

Worktree isolation changes the effective filesystem root; it does not redefine editing semantics.

Existing EditService/EditTransaction behavior remains canonical.

Snapshots and provenance remain responsible for file recovery lineage.

Rollback remains responsible for recovery authorization and produced-state conflict handling.

Persistent audit remains responsible for durable audit history.

Worktree ownership must be included in correlation data where the canonical contracts support it.

Do not add Git-reset-based rollback. AWH file rollback is not equivalent to Git reset.

Do not make a worktree disappear merely because an edit rollback succeeds.

---

## 23. Security invariants

The implementation is acceptable only if all are true:

1. An agent cannot access another agent's managed worktree merely by knowing its path.
2. A session cannot operate outside its assigned worktree.
3. A worktree cannot escape the approved workspace/worktree root.
4. Unknown or corrupt lifecycle records fail closed.
5. Worktree creation cannot overwrite an existing unrelated path.
6. Worktree removal cannot delete the main workspace.
7. Cleanup cannot remove a worktree that has changed ownership.
8. Git failures cannot be reported as successful lifecycle transitions.
9. Authorization denial produces zero worktree mutation.
10. Lifecycle state cannot claim `active` without verification.
11. Concurrent creation cannot silently assign the same managed path.
12. No secret values are written to worktree metadata or audit.
13. No shell interpretation occurs for Git arguments.
14. Unmanaged Git worktrees are never silently adopted.
15. A stale/missing worktree is classified explicitly rather than fabricated as healthy.

---

## 24. Required tests

Use real temporary Git repositories whenever possible.

### Repository/service tests

Test:

- Git repository detection;
- non-Git directory rejection;
- worktree creation;
- deterministic identity;
- unique path allocation;
- branch/ref handling;
- list;
- inspect;
- remove;
- repeated remove;
- missing worktree;
- repository deletion;
- invalid/corrupt metadata;
- restart/reconciliation.

### Isolation tests

Run at least two real agent/session contexts.

Prove:

- Agent A can access Worktree A;
- Agent B can access Worktree B;
- Agent A cannot access Worktree B;
- Agent B cannot access Worktree A;
- A cannot use B's path to escape its root;
- workspace A cannot access workspace B;
- branch/path collisions are rejected safely.

### Path-security tests

Cover:

- absolute paths;
- `..`;
- mixed separators where applicable;
- symlink-to-outside;
- symlink replacement;
- sibling-prefix confusion;
- worktree root itself;
- main repository root as a target;
- nonexistent target under a safe ancestor;
- already-owned path.

### Concurrency tests

Use concurrent tasks/processes where practical to cover:

- simultaneous creation;
- same branch;
- same requested path;
- simultaneous remove/inspect;
- session termination racing with creation;
- restart/reconciliation racing with lifecycle updates.

### Git safety tests

Prove user-controlled strings cannot become Git options unexpectedly.

Test branch/ref/path values beginning with `-`, malformed refs, spaces, Unicode, and shell metacharacters.

No shell is allowed to interpret the values.

### Cleanup/recovery tests

Cover crashes or injected failures between:

- reservation and Git creation;
- Git creation and verification;
- verification and persistence;
- removal transition and Git removal;
- Git removal and persistence.

The resulting state must be classified safely after restart.

### Cross-interface tests

Where supported, exercise the same worktree through:

- service;
- CLI;
- MCP;
- TUI;
- Control API.

All must converge on the same lifecycle and ownership semantics.

### Audit tests

Prove lifecycle outcomes are recorded through the canonical audit boundary without secrets.

---

## 25. Acceptance matrix

The implementation is complete only when evidence exists for at least:

| Scenario | Required result |
|---|---|
| Create isolated worktree | Dedicated verified worktree bound to session |
| Create same path twice | One succeeds; second fails safely |
| Two agents create worktrees | Distinct ownership and roots |
| Cross-agent access | Denied with zero mutation |
| Non-Git workspace | Explicit failure |
| Invalid path | Rejected before Git mutation |
| Existing unrelated path | Never overwritten |
| Inspect missing worktree | Explicit missing/recovery state |
| Remove owned worktree | Removed and verified |
| Remove another agent's worktree | Denied |
| Remove with changed ownership | Refused |
| Uncommitted changes | No silent discard |
| Crash during lifecycle | Restart classifies state safely |
| Corrupt metadata | Fail closed |
| Git command failure | Non-success lifecycle state |
| Audit write | Canonical audit event without secrets |
| Concurrent creation | No duplicate managed ownership |
| Main workspace target | Rejected |
| Symlink escape | Rejected |
| Unicode/path edge cases | Deterministic behavior |

---

## 26. Duplicate-mechanism audit

Before completion, search for duplicate implementations.

There must be one canonical owner for:

- Git invocation;
- Worktree lifecycle;
- Worktree ownership;
- worktree path allocation;
- workspace containment;
- lifecycle persistence;
- authorization;
- audit.

Remove or avoid any newly introduced duplicate.

If pre-existing duplicate behavior cannot safely be removed within this prompt, document it explicitly rather than silently allowing the new implementation to become a competing authority.

---

## 27. Performance and resource discipline

Do not scan the entire filesystem to discover worktrees.

Prefer Git's native worktree metadata for repository state.

Bound:

- number of managed records;
- lifecycle metadata size;
- Git command duration using the existing timeout model;
- list/inspect work per request.

Avoid repeated repository-wide scans and duplicate Git invocations where a single verified result can be reused safely.

---

## 28. Documentation consistency

Update source-facing documentation only when required to make the implemented contract accurate.

Do not update other implementation-prompt files.

Do not mark roadmap phases complete merely because this implementation exists.

Do not document merge/conflict resolution, distributed orchestration, or remote Git hosting as implemented unless this prompt actually implements and verifies those features.

---

## 29. Linear implementation sequence

Execute the work in this order:

1. Read the current repository and all relevant contracts.
2. Inventory existing Git, workspace, agent, session, authorization, audit, and persistence code.
3. Confirm the exact absence/partial state of worktree support.
4. Define the canonical Worktree model and lifecycle states.
5. Define repository identity and approved worktree-root rules.
6. Implement safe path allocation and ownership binding.
7. Extend the existing GitService with structured worktree operations only where necessary.
8. Implement create with reservation, Git mutation, verification, and durable state publication.
9. Implement list/inspect with real Git reconciliation.
10. Implement ownership-checked removal and policy-controlled cleanup.
11. Implement restart/crash reconciliation.
12. Integrate existing authorization and audit boundaries.
13. Wire thin CLI/service adapters where supported.
14. Wire existing MCP/TUI/Control API adapters only where their current contracts require it.
15. Add unit, integration, concurrency, adversarial, and failure-injection tests.
16. Perform the duplicate-mechanism audit.
17. Run all verification gates.
18. Review the final diff for scope leakage and false completion claims.

Do not skip directly to CLI commands before the canonical service contract exists.

---

## 30. Verification gates

Run:

```bash
cargo fmt --all -- --check
cargo check --all-targets
cargo test --all-targets
cargo clippy --all-targets --all-features -- -D warnings
git diff --check
```

Also run the repository's applicable security/audit checks and real Git integration tests.

For Git-dependent tests, verify that the test environment has a usable Git executable and does not accidentally operate on the developer's real repository.

Every test that creates a worktree must use an isolated temporary repository and clean up its resources.

---

## 31. Completion criteria

Prompt 13 is complete only when:

- one canonical Worktree model exists;
- worktree identity is bound to the appropriate workspace/agent/session context;
- creation is safe and verified;
- list/inspect reconcile with actual Git state;
- removal is ownership-checked and non-destructive;
- restart/crash states are explicit and recoverable;
- path containment and symlink protections are enforced;
- concurrent creation is deterministic;
- Git arguments use structured argv;
- existing authorization is enforced before mutation;
- canonical audit records lifecycle outcomes;
- CLI/MCP/TUI/Control API adapters, where applicable, use the same service;
- tests demonstrate real multi-agent isolation;
- no duplicate Git/worktree/authorization/audit system was introduced;
- verification gates pass;
- limitations imposed by Git/OS/platform behavior are documented;
- only files legitimately required by GIT-001 implementation are changed.

Do not claim that multi-agent collaboration, merge orchestration, remote Git hosting, or a distributed sandbox is complete.

---

## 32. Independence rule

This prompt must be executable against the current `rust` branch.

Do not wait for:

- Prompt 14;
- Prompt 15;
- Prompt 16;
- Prompt 17;
- any historical Trust Wedge PR;
- any issue-resolving-prompt PR;
- any other future branch.

Reuse contracts that already exist. If a referenced contract is absent, implement only the minimum local adapter needed to keep this prompt's boundary coherent and explicitly report the limitation.

Do not modify another `docs/implementation-prompts/*` file.

---

## 33. Final implementation report

The implementation report must state:

1. repository/source forensics performed;
2. existing Git/workspace/agent/session contracts reused;
3. Worktree data model and lifecycle states;
4. repository and path-containment rules;
5. creation and verification behavior;
6. ownership and authorization behavior;
7. removal/cleanup semantics;
8. restart/recovery behavior;
9. concurrency model;
10. audit integration;
11. CLI/MCP/TUI/Control API integrations;
12. security/adversarial tests;
13. failure-injection/restart evidence;
14. verification commands and exact results;
15. changed files;
16. known Git/OS/platform limitations;
17. duplicate mechanisms found and how they were handled;
18. explicit confirmation that no unrelated implementation prompt was modified;
19. explicit confirmation that no second Git/worktree/authorization/audit system was introduced.

The final report must distinguish implemented behavior from unverified or platform-dependent behavior.
