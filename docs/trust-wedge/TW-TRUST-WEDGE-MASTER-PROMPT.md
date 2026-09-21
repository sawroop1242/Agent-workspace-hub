# Trust Wedge — Master Implementation Prompt

## Purpose

Use this prompt as the governing implementation contract for every Trust-Wedge (TW) issue in Agent Workspace Hub (AWH).

The Trust Wedge is the minimum trustworthy runtime boundary between an external agent/operator and consequential AWH state changes. It covers:

- safe and repeatable workspace initialization;
- stable caller identity;
- agent profiles, registry, and runtime sessions;
- agent-scoped MCP routing;
- capability and policy enforcement;
- canonical safe editing;
- durable file snapshots and provenance;
- conflict-aware rollback and recovery;
- persistent structured audit;
- executable end-to-end acceptance.

The goal is **not** to build another agent framework. External agents remain external. AWH provides the controlled workspace/runtime boundary they use.

---

## 1. Non-negotiable issue independence

Every TW issue is independently executable.

An implementation agent MUST:

1. work against the current `rust` branch;
2. inspect the repository as it exists at implementation time;
3. implement the requested issue without assuming another TW issue, PR, branch, or unmerged commit exists;
4. reuse an existing compatible abstraction when it already exists;
5. create the smallest local compatibility layer when a prerequisite is absent;
6. never weaken security or correctness merely to avoid implementing a missing prerequisite;
7. document any compatibility layer that is intentionally temporary.

A TW issue may overlap conceptually with another issue, but it must never contain an implicit dependency such as:

> "This works after TW-003 is merged."

Instead, the implementation must either use the current repository's existing behavior or provide the minimum self-contained integration needed for the requested scope.

Do not implement unrelated TW issues just because their code would be convenient.

---

## 2. First step: repository forensics

Before changing code, inspect the actual current implementation.

At minimum:

- `Cargo.toml` and `Cargo.lock`;
- `src/main.rs`;
- `src/lib.rs` if present;
- relevant `src/core/*`;
- relevant `src/services/*`;
- relevant `src/mcp/*`;
- relevant models/types;
- relevant CLI commands;
- relevant persistence/store code;
- existing unit and integration tests;
- `docs/PROJECT_ROADMAP.md`;
- `docs/PROJECT_STATUS.md`;
- `docs/FEATURES.md`;
- `docs/CLI.md`;
- relevant security/threat-model documentation.

Search for equivalent types and services before creating anything. The repository already contains substantial infrastructure, including agent records, capability/policy records, MCP sessions, filesystem services, audit infrastructure, and context snapshots. Do not create duplicate versions merely because a new issue needs a different integration point.

For every important assumption, answer:

- Where is the current implementation?
- Who constructs it?
- Who calls it?
- Where is its state persisted?
- Is it authoritative or merely a model/scaffold?
- Is it already used by MCP, CLI, TUI, or Control API?
- What tests prove its behavior?
- What security boundary currently protects it?

If the source contradicts documentation, treat executable source and tests as the starting truth and update documentation only after behavior is intentionally changed.

---

## 3. Architectural boundary

AWH must remain an agent-agnostic runtime/workspace system.

The intended high-level flow is:

```
External Agent / Operator
        |
        v
MCP / CLI / TUI / Control API
        |
        v
Caller + Session Resolution
        |
        v
Capability + Policy Authorization
        |
        v
Canonical Domain Service
        |
        v
Workspace / Persistence / Runtime
        |
        v
Audit + Provenance
```

Interface adapters must not become alternative domain implementations.

### Ownership rules

**AgentProfile**
- describes an agent;
- stores configuration/metadata;
- does not itself grant authority.

**AgentRegistry**
- authoritative lookup and lifecycle registry for AWH agents;
- does not replace authorization.

**AgentSession**
- identifies an AWH runtime caller;
- is distinct from an MCP protocol/transport session;
- must bind the caller to an agent and workspace.

**MCP route**
- identifies which agent route is being addressed;
- never grants permission by route name.

**Capability/Policy**
- is the security authority;
- must be evaluated before consequential tool execution or mutation;
- defaults to denial when authority is absent.

**Canonical services**
- own domain operations and invariants;
- are shared by supported interfaces.

**Canonical stores**
- own durable persistence, serialization, locking, corruption handling, and migrations for their domain.

**EditService**
- remains the canonical mutation engine;
- no MCP/CLI/TUI adapter may create a second editing implementation.

**File snapshots**
- protect file state for recovery/provenance;
- must remain distinct from context-engine snapshots.

**Rollback**
- must be state-aware and conflict-aware;
- never blindly overwrite a file that changed after the protected edit state.

**Audit**
- has one canonical write/choke point;
- must not become a collection of unrelated per-interface logs.

---

## 4. Trust-Wedge security invariants

These invariants are mandatory.

### Authorization

```
identify caller
 -> resolve agent
 -> resolve session
 -> resolve workspace
 -> evaluate capability/policy
 -> validate operation
 -> mutate
 -> verify
 -> audit
```

Never move authorization after mutation.

A denied consequential operation MUST produce zero mutation.

Unknown agents, inactive agents, invalid sessions, expired capabilities, scope mismatches, and policy denials must fail before the protected operation executes.

### Workspace isolation

A caller must never gain authority over another workspace merely by changing a path, route, identifier, or request parameter.

### Identity

Consequential operations must preserve enough identity to reconstruct:

```
agent -> session -> workspace -> task
                     |
                     +-> edit -> snapshot -> audit
```

Use existing typed IDs when available. Do not create stringly typed duplicate identifiers.

### Recovery

If a protected edit changes a file from state A to state B:

```
A -> edit -> B
```

rollback is allowed only when the current state is still compatible with the recorded rollback precondition.

If another actor changes B to C, rollback must detect the conflict and preserve C rather than silently restoring A.

### Persistence

If an implementation claims durable state, that state must survive process restart.

A missing first-run store may be initialized only when the domain contract explicitly allows it. A malformed authoritative store must not silently become an empty store.

### Secrets and data minimization

Do not write secrets, access tokens, credentials, or raw file contents into ordinary audit records or diagnostic logs.

Hash/metadata references should be used where content identity is required.

---

## 5. Safe implementation rules

Prefer extension over replacement.

Before adding a new type:

1. search for an existing equivalent;
2. determine whether it is authoritative;
3. determine whether it is currently wired into production;
4. extend it if semantics are compatible;
5. only create a new abstraction when ownership or semantics genuinely require it.

Do not:

- create a second AgentStore;
- create a second policy engine;
- create a second authorization gate;
- create a second edit engine;
- create a second file-snapshot system;
- use context snapshots as file backups;
- create MCP-local domain persistence for a domain owned by a canonical service/store;
- bypass path/symlink protections;
- add unrestricted default capabilities;
- silently overwrite conflicting external changes;
- put domain algorithms into `main.rs`;
- introduce broad remote-agent infrastructure into a TW issue;
- rewrite unrelated modules for style.

If an existing implementation is incomplete, wire it safely or add the smallest missing behavior. Do not declare scaffolding complete merely because types exist.

---

## 6. Required implementation workflow

Use this loop for every TW issue:

### Step 1 — Inspect

Read the issue, current source, tests, and relevant documentation.

### Step 2 — Build a local dependency map

Identify:

```
caller
  -> interface
  -> service
  -> store/filesystem
  -> security gate
  -> persistence
  -> audit
```

Record which parts already exist and which are missing.

### Step 3 — Define the smallest contract

Specify:

- inputs;
- outputs;
- state transitions;
- persistence;
- errors;
- authorization point;
- concurrency behavior;
- restart behavior;
- audit/provenance behavior.

### Step 4 — Implement

Make the smallest production-quality change that satisfies the issue.

### Step 5 — Test behavior, not only types

Use real temporary workspaces/files where filesystem behavior matters.

Prove both success and denial/failure paths.

### Step 6 — Validate integration

Run CLI, MCP, or other real entry points relevant to the issue.

### Step 7 — Run repository checks

At minimum, when available:

```bash
cargo fmt --all -- --check
cargo check --all-targets
cargo test --all-targets
```

Also run relevant clippy, security, persistence, integration, and platform checks already used by the repository.

### Step 8 — Inspect the final diff

Reject unrelated changes, duplicate abstractions, accidental permission broadening, and documentation claims that do not match executable behavior.

---

## 7. Failure behavior must be explicit

For every new persistent or mutating operation define behavior for:

- missing state;
- empty state;
- malformed state;
- unsupported schema;
- permission failure;
- lock timeout;
- stale lock;
- concurrent writer;
- invalid caller;
- authorization denial;
- stale expected state;
- external modification;
- serialization failure;
- process restart;
- partial/interrupted persistence.

For every mutation state whether failure guarantees:

- zero mutation;
- an intentionally recoverable partial operation; or
- transactional restoration.

Never leave mutation semantics implicit.

---

## 8. Testing standard

A TW issue is not complete because compilation succeeds.

Tests must prove the security and state invariants that make the feature trustworthy.

Prefer:

- unit tests for deterministic domain logic;
- integration tests for service/store boundaries;
- CLI tests for user-visible contracts;
- MCP tests for transport-to-service routing;
- real temporary files for filesystem operations;
- restart tests for durable state;
- multi-agent tests for identity isolation;
- failure-injection tests where practical;
- concurrency tests for persistent stores.

Every denial test should verify that the protected operation was **not executed** and that state did not change.

Every recovery test should verify the final bytes/state, not merely an error code.

---

## 9. Compatibility requirements

Before changing persisted structures, inspect existing data formats and compatibility expectations.

Consider:

- missing fields;
- unknown fields;
- old IDs;
- schema versions;
- empty files;
- malformed files;
- interrupted writes;
- Unicode;
- spaces in paths;
- nested paths;
- read-only workspaces;
- symlinks;
- concurrent processes.

Do not assume ASCII-only paths or content.

When changing a schema:

1. define the compatibility rule;
2. implement deterministic migration if required;
3. make migration idempotent;
4. preserve supported data;
5. test restart/crash behavior;
6. do not delete legacy state until a deletion gate is satisfied.

---

## 10. Definition of Done

A TW implementation may be reported complete only when:

- the requested production behavior exists;
- the implementation uses the canonical subsystem where one exists;
- authorization occurs before protected mutation;
- identity and workspace scope are preserved;
- persistence behavior is explicit and tested;
- failure/recovery semantics are tested;
- relevant CLI/MCP entry points work;
- tests cover both positive and negative paths;
- formatting/build/test checks pass, or failures are explicitly reported;
- no unrelated scope was added;
- documentation matches actual behavior;
- no hidden dependency on another unmerged issue exists.

Never claim an acceptance criterion passed without executable evidence.

---

## 11. Final implementation report

At completion, report:

1. exact production files changed;
2. exact test files changed;
3. new/changed public contracts;
4. security invariants enforced;
5. persistence/migration behavior;
6. commands executed;
7. test results;
8. integration/smoke-test results;
9. known limitations;
10. intentionally unsupported behavior;
11. any assumptions that could not be verified.

The final report must distinguish:

- implemented and verified;
- implemented but not fully verified;
- not implemented;
- intentionally out of scope.

