# TW-001 — AWH Init + Runtime Contracts

## Master implementation prompt

Implement the **AWH initialization and runtime identity contract** on the current `rust` branch.

This issue is standalone. Do not assume TW-002, TW-003, or any other issue/PR has already been merged. If the current repository lacks a prerequisite needed to make initialization safe, implement the smallest compatible piece inside this issue and keep the resulting behavior self-contained.

The objective is to make a newly initialized AWH workspace a **safe, deterministic, reloadable runtime boundary** with explicit identity contracts and no implicit authority.

---

## 1. Required first step: inspect the current repository

Before modifying anything, inspect the current implementation rather than relying on this prompt's historical filenames.

At minimum inspect:

- `src/main.rs` and the current CLI command enum/dispatch;
- `src/lib.rs`, if present;
- `src/core/workspace.rs`;
- `src/core/project.rs`;
- `src/core/agents.rs`;
- `src/core/capability_grants.rs`;
- `src/core/policy.rs`;
- relevant configuration modules;
- relevant persistent stores and `StoreLock`/locking code;
- existing model/ID definitions;
- MCP session/runtime construction;
- current `awh init` behavior, if any;
- current tests for workspace/configuration/persistence;
- `docs/PROJECT_ROADMAP.md`;
- `docs/CLI.md`;
- `docs/FEATURES.md`;
- `docs/PROJECT_STATUS.md`;
- relevant security/threat-model documentation.

Search the whole repository for existing definitions of:

- workspace/project identifiers;
- agent identifiers;
- session identifiers;
- task identifiers;
- edit identifiers;
- snapshot identifiers;
- audit identifiers;
- initialization/configuration paths.

Do not introduce duplicate ID types if an equivalent stable type already exists.

---

## 2. Problem to solve

AWH needs a reliable bootstrap contract before higher-level Trust-Wedge features can safely depend on it.

The initialization contract must answer:

- What directory is the AWH workspace root?
- How is a workspace identified?
- Where is AWH-owned persistent state stored?
- How is initialization detected?
- What happens on first initialization?
- What happens when `awh init` is run again?
- Which configuration is preserved?
- Which defaults are created?
- Is an agent automatically activated?
- Are capabilities automatically granted?
- How are runtime identities serialized and reloaded?
- What happens when persisted state is malformed or incompatible?

The implementation must make these answers explicit in code and tests.

---

## 3. `awh init` contract

Implement or reconcile the existing `awh init` command.

### Fresh initialization

For a valid fresh workspace:

1. resolve and validate the requested root;
2. establish the canonical workspace root;
3. create only the AWH-owned directories/files required by the current contract;
4. create secure default configuration/state;
5. initialize the workspace identity;
6. initialize required stores in their valid empty/default state;
7. leave the workspace usable by subsequent AWH commands;
8. do not activate an agent implicitly;
9. do not grant unrestricted capabilities implicitly;
10. return a deterministic, structured success result appropriate to the existing CLI conventions.

Initialization must not accidentally initialize state for a different directory.

### Repeated initialization

Running `awh init` against an already initialized valid workspace must be **idempotent**.

It must:

- preserve the existing workspace identity;
- preserve user configuration;
- preserve existing agent/session/runtime state;
- preserve existing persistent records;
- avoid resetting permissions or capabilities;
- avoid replacing valid stores with empty stores;
- avoid destructive migrations;
- avoid generating a new identity merely because initialization was invoked again.

If the command has a repair/reconcile behavior, that behavior must be explicit rather than silently destructive.

---

## 4. Workspace-root validation

The implementation must establish a single, unambiguous workspace root.

Validate at minimum:

- root exists or can be safely created according to the existing CLI contract;
- the path is a directory;
- the resolved path is deterministic;
- relative paths are resolved consistently;
- traversal cannot escape the selected root;
- symlink behavior follows the repository's existing filesystem-security contract;
- AWH state is never accidentally created in the process CWD when another root was explicitly requested.

Do not weaken existing filesystem containment checks.

If the repository already has a canonical workspace/path validation helper, reuse it.

---

## 5. Persistent initialization state

Identify the repository's existing persistence mechanism and use it instead of inventing an unrelated database/store.

The initialized workspace must have a clear persistent representation of:

- workspace identity;
- initialization/schema version where applicable;
- configuration/state required to reload the workspace;
- enough metadata to distinguish a valid initialized workspace from arbitrary files.

The persisted representation must be:

- deterministic;
- serializable/deserializable;
- restart-safe;
- corruption-detectable;
- atomic on write where the existing store contract requires it;
- protected by the repository's existing locking mechanism where concurrent writers are possible.

### Important failure rule

A malformed authoritative initialization/state file must **not** be silently interpreted as a fresh workspace.

Return a structured error that tells the caller that persisted state is invalid/corrupt/incompatible according to the repository's error conventions.

---

## 6. Runtime identity contracts

Establish or reconcile stable typed identifiers for the Trust-Wedge runtime.

Required conceptual identities:

| Identity | Meaning | Required relationship |
|---|---|---|
| Workspace ID | persistent identity of an AWH workspace | one runtime workspace |
| Agent ID | stable identity of an AWH agent | belongs to/operates within workspace model |
| Session ID | identity of an AWH runtime session | belongs to exactly one agent + workspace |
| Task ID | identity of an execution/task record | must not be confused with transport session |
| Edit ID | identity of a consequential edit | later links to snapshot/audit |
| Snapshot ID | identity of recoverable file state | later links to edit |
| Audit/Event ID | identity of an audit record | correlates consequential operations |

If equivalent types already exist, extend/reuse them.

Do not create seven disconnected UUID/string wrappers merely to satisfy the table.

---

## 7. Identity requirements

Identifiers must be:

- strongly typed where Rust types are appropriate;
- serializable;
- deserializable;
- deterministic in serialization;
- stable across restart for persisted identities;
- distinguishable from each other;
- validated before use;
- represented consistently across CLI/service/persistence boundaries.

A runtime restart must not silently turn an existing workspace into a new workspace.

Likewise, reloading persisted agent/session metadata must not change the persisted IDs.

For IDs that are intentionally ephemeral, document that fact instead of pretending they are durable.

---

## 8. Identity relationship invariants

The implementation must establish and test these relationships:

```
Workspace
   |
   +-- Agent
          |
          +-- Session
                 |
                 +-- Task
                        |
                        +-- Edit
                               |
                               +-- Snapshot
                               |
                               +-- Audit/Event
```

Not every record must physically nest inside another record, but the relationship must be representable and unambiguous.

Mandatory invariants:

1. A session references exactly one agent.
2. A session references exactly one workspace.
3. An agent cannot create a session in an unrelated workspace.
4. Unknown agent IDs cannot be treated as valid callers.
5. A disabled/inactive agent must not become active merely because its ID exists.
6. Runtime identity metadata must not itself grant capabilities.
7. Identity resolution must not bypass policy/capability enforcement.
8. IDs must survive persistence/reload where the associated record is durable.

Do not implement full agent lifecycle/routing/authorization in TW-001 unless the current repository requires a minimal compatibility hook to make these contracts coherent. Those behaviors belong to their dedicated Trust-Wedge issues.

---

## 9. Secure defaults

Initialization must be conservative.

On a fresh workspace:

- no external agent should receive implicit unrestricted authority;
- no agent should be implicitly activated unless the existing product contract explicitly requires it;
- no capability should be granted merely because an agent profile exists;
- security-sensitive configuration must use the repository's existing safe defaults;
- initialization must not disable trust/policy checks;
- default configuration must not expose a new network listener or remote-control surface unintentionally.

The principle is:

```
fresh workspace
    -> valid state
    -> no implicit authority
    -> explicit later activation/grant
```

---

## 10. Configuration preservation

Determine the current configuration precedence and storage model before modifying it.

Repeated initialization must not overwrite:

- user-selected workspace settings;
- configured agent metadata;
- policy configuration;
- trusted-server state;
- connector configuration;
- existing task/memory/context data;
- other supported persistent AWH state.

If initialization needs to add a newly introduced field, use the repository's normal defaulting/versioning mechanism.

Do not implement a broad configuration migration unrelated to this issue.

---

## 11. Persistence and concurrency

Use the existing persistence/locking primitives where they fit.

The implementation must account for:

- two initialization attempts against the same workspace;
- initialization while state already exists;
- interrupted writes;
- lock contention;
- stale locks where the repository supports stale-lock recovery;
- restart immediately after initialization;
- concurrent readers observing partially written state.

Never use an unsafe unlocked fallback merely because a lock cannot be acquired.

Where exact transactional initialization is required, make the write sequence atomic or recoverable rather than relying on best effort.

---

## 12. CLI behavior

Reconcile the implementation with the existing AWH CLI style.

If `awh init` already exists:

- preserve compatible flags and behavior;
- fix unsafe/incomplete behavior rather than creating a second command;
- keep errors structured and actionable.

If it does not exist or is only scaffolded:

- add it through the existing command architecture;
- do not put the initialization algorithm directly into a giant `main.rs` branch;
- keep domain logic in the appropriate service/core module.

The CLI should clearly distinguish at least:

- successful fresh initialization;
- successful idempotent re-initialization;
- invalid workspace root;
- invalid/corrupt existing state;
- permission failure;
- lock/concurrency failure.

Do not claim process/agent management that the repository does not actually implement.

---

## 13. Testing requirements

Use real temporary directories for initialization tests.

### Fresh-init tests

Prove:

- initialization succeeds in a valid empty directory;
- expected AWH state is created;
- workspace identity exists;
- persisted state can be read back;
- no agent is implicitly activated;
- no unrestricted capability is implicitly granted;
- the resulting workspace can be used by the next AWH operation.

### Idempotence tests

Run initialization twice and prove:

- same workspace ID;
- unchanged user configuration;
- unchanged existing state;
- no duplicated records;
- no reset capabilities/policies;
- no destructive replacement of stores.

### Invalid-input tests

Cover:

- nonexistent root when creation is not allowed;
- file passed where a directory is required;
- inaccessible/read-only location where applicable;
- invalid/corrupt persisted initialization state;
- unsupported schema/version;
- path/symlink cases covered by existing filesystem rules.

### Identity tests

For every durable identity introduced/changed:

- serialize;
- deserialize;
- compare identity;
- persist;
- restart/reconstruct;
- verify the same identity is recovered.

Test invalid relationships such as:

- session references unknown agent;
- session references wrong workspace;
- duplicate identity registration where the store forbids it.

### Restart test

Create a workspace, initialize it, terminate the process/test scope, reconstruct the runtime from disk, and prove that:

- workspace identity is unchanged;
- persisted configuration is preserved;
- initialization is still recognized;
- no implicit authority appears after reload.

### Real CLI smoke test

Run the compiled `awh init` command against a temporary directory rather than only testing internal functions.

---

## 14. Suggested implementation boundaries

These are investigation targets, not mandatory filenames.

Potential areas include:

```
src/main.rs
src/lib.rs
src/core/workspace.rs
src/core/project.rs
src/core/agents.rs
src/core/capability_grants.rs
src/core/policy.rs
src/models/*
src/services/*
configuration/persistence modules
tests/*
```

Choose the actual files from the current repository.

If a new focused module is required, keep it narrow, for example:

```
initialization
workspace_identity
runtime_identity
```

Do not create a generic "TrustWedgeManager" that owns every future TW concern.

---

## 15. What this issue must NOT implement

Do not pull these into TW-001 except for the smallest compatibility hooks required by the initialization contract:

- agent-specific MCP routing;
- full agent lifecycle management;
- capability grant/revoke workflows;
- policy-engine redesign;
- canonical edit implementation;
- file snapshots;
- rollback;
- persistent audit;
- worktree isolation;
- remote execution;
- external-agent orchestration;
- collaboration;
- benchmark integrations;
- broad store migrations;
- unrelated MCP refactoring.

TW-001 establishes the safe foundation. It does not implement the entire Trust Wedge.

---

## 16. Definition of Done

TW-001 is complete only when all of the following are true:

### Initialization
- [ ] `awh init` works against a valid fresh workspace.
- [ ] Re-running `awh init` is safe and idempotent.
- [ ] Existing valid state/configuration is preserved.
- [ ] Invalid/corrupt state fails explicitly rather than being reset.
- [ ] Workspace-root validation follows existing security rules.

### Identity
- [ ] Required stable identity contracts exist or existing equivalents are reconciled.
- [ ] IDs serialize and deserialize correctly.
- [ ] Durable IDs survive restart.
- [ ] Agent/session/workspace relationships are unambiguous.
- [ ] Identity does not grant authorization.

### Security
- [ ] No implicit unrestricted capabilities.
- [ ] No implicit agent activation.
- [ ] Existing policy/trust mechanisms are not bypassed.
- [ ] Initialization cannot escape its selected workspace.

### Persistence
- [ ] Initialization state is durable.
- [ ] Writes follow existing atomic/locking semantics.
- [ ] Restart reconstruction succeeds.
- [ ] Corruption/version failures are explicit.

### Verification
- [ ] Unit tests pass.
- [ ] Integration tests pass.
- [ ] Real CLI smoke test passes.
- [ ] `cargo fmt --all -- --check` passes.
- [ ] `cargo check --all-targets` passes.
- [ ] `cargo test --all-targets` passes.
- [ ] Relevant clippy/security checks pass when available.

---

## 17. Final report required from the implementation agent

Return a precise implementation report containing:

1. **Production files changed** — exact paths and why.
2. **Test files changed** — exact paths and what each proves.
3. **Existing abstractions reused** — identify them explicitly.
4. **New contracts** — IDs, initialization state, persistence format, or errors.
5. **Security behavior** — explain how implicit authority and workspace escape are prevented.
6. **Persistence behavior** — explain idempotence, corruption handling, locking, and restart.
7. **Commands executed** — exact commands.
8. **Results** — pass/fail with important output summarized.
9. **CLI smoke test** — exact scenario tested.
10. **Known limitations** — only genuine limitations.
11. **Out-of-scope items** — explicitly state what was intentionally not implemented.

Do not report "complete" merely because the code compiles. The acceptance claim must be backed by executable tests and a real `awh init` smoke test.

