# Prompt 15 — Service/Store Convergence (ARCH-001)

## 1. Mission

Implement one canonical AWH application-service and persistence boundary so MCP, CLI, TUI, and Control API operate on the same domain behavior and authoritative state instead of maintaining divergent MCP-local implementations.

The implementation must begin from the current rust branch and be based on repository forensics, not assumptions from historical reports. The goal is convergence of existing behavior, not a broad rewrite.

The central invariant is:

```text
CLI ───────┐
MCP ───────┤
TUI ───────┼──> canonical AWH application service/domain boundary
Control API┘                    │
                                ▼
                     canonical core/store state
                                │
                                ▼
                   durable state or filesystem
```

A caller may have a different transport, authentication/session context, or presentation format, but it must not receive a second implementation of the same business operation merely because it entered through MCP.

The final implementation must leave exactly one authoritative owner for each converged domain contract and must make legacy/duplicate paths either: (1) thin adapters over that owner, (2) explicitly compatibility-only readers/migrators, or (3) removed when repository forensics proves they are unused.

---

## 2. Product and architecture boundary

AWH is an agent-agnostic, local-first workspace runtime for existing coding agents.

AWH owns:
- workspace and filesystem state;
- controlled file editing;
- Git/worktrees;
- capabilities and policy;
- snapshots, provenance, rollback;
- context, memory, skills, sessions, tasks;
- audit and observability;
- MCP, CLI, TUI, and Control API interfaces.

External agents own reasoning, planning, model selection, agent intelligence, and agent-specific orchestration.

The canonical architecture rule is:

```text
Interface
  ↓
Agent/session/request context
  ↓
authorization/policy boundary
  ↓
AWH application service
  ↓
canonical domain/store
  ↓
durable state or filesystem
```

The interface layer may validate transport-specific syntax and translate arguments, but it must not create a parallel domain model whose state can diverge from service/core state.

---

## 3. Scope

This prompt owns ARCH-001 convergence of duplicated service/store behavior, especially where MCP currently has local implementations that overlap canonical services/core stores.

Primary convergence targets include:
- filesystem/workspace operations;
- memory;
- tasks;
- context/project state where duplicate ownership exists;
- connector persistence where duplicate persistence exists;
- agent/capability/policy state when adapters maintain shadow state;
- edit/snapshot/provenance/audit references when an interface bypasses canonical services;
- shared locking/persistence conventions where duplicated stores use incompatible mechanisms.

The exact final set MUST be derived from current-source forensics.

This prompt also owns caller migration, compatibility/migration behavior for existing persisted data where needed, architectural tests preventing reintroduction of MCP-local duplicate stores, and verification that the major interfaces converge on the same service behavior.

---

## 4. Explicit non-goals

Do NOT use this prompt to:
- redesign the MCP protocol;
- redesign MCP routing or authentication;
- create a second PolicyEngine;
- create a second capability engine;
- create a second agent/session identity model;
- redesign EditTransaction or edit algorithms;
- implement the edit operation engine;
- redesign snapshots;
- redesign rollback;
- redesign persistent audit;
- implement worktree lifecycle;
- implement filesystem TOCTOU hardening as a new subsystem;
- create distributed locks;
- replace the repository with a database;
- introduce a generic ORM/repository framework merely for architectural purity;
- build an event-sourcing architecture;
- build an event bus;
- add remote synchronization;
- redesign the Control API;
- redesign the TUI;
- add model routing, agent orchestration, swarm scheduling, or workflow execution;
- rewrite Git history;
- add containers/VMs/sandboxing;
- change external-agent behavior.

Prompt 15 may migrate callers to existing canonical implementations owned by other subsystems. It must not absorb those subsystems' responsibilities.

---

## 5. Required repository forensics before coding

Do not start implementation until the current rust branch has been mapped.

Read at minimum:

### Roadmap/product
- docs/roadmap/GROWTH_STRATEGY.md
- docs/roadmap/PROJECT_ROADMAP.md
- docs/roadmap/ROADMAP_AGENT_PROFILES_POLICY_MCP.md
- docs/FEATURES.md
- docs/PROJECT_CONTEXT.md
- docs/architecture.md
- docs/security.md
- docs/threat-model.md
- docs/CLI.md if present
- docs/mcp.md if present
- docs/testing.md if present

### Canonical implementation prompts
Read all current implementation prompts, including 01–14, Prompt 16 if present, and Prompt 17 if present. Use them only to identify ownership boundaries. Do not require another prompt's PR or implementation to complete this task.

### Historical evidence
Inspect relevant material under docs/trust-wedge/, docs/issue-resolving-prompts/, docs/archive/, AWH-FORENSIC-REPOSITORY-REPORT.md, AWH_FORENSIC_REPORT.md, and architecture/MCP completion reports when relevant.

Historical documents are evidence, not the current source of truth. Resolve contradictions against actual current rust source.

### Current source
At minimum inspect:
- src/services/mod.rs
- src/services/files.rs
- src/services/edit.rs
- src/services/authorization.rs
- src/services/snapshot.rs
- src/services/audit.rs
- src/services/init.rs
- src/core/mod.rs
- src/core/agents.rs
- src/core/capability_grants.rs
- src/core/policy.rs
- src/core/project.rs
- src/core/memory.rs
- src/core/tasks.rs
- src/core/context.rs
- src/mcp/mod.rs
- src/mcp/dispatcher.rs
- src/mcp/workspace.rs
- src/mcp/memory.rs
- src/mcp/tasks.rs
- src/mcp/store_lock.rs
- connector/provider persistence modules
- Control API handlers
- CLI handlers
- TUI backend/service calls.

Also search for:
```text
struct *Store
struct *Mcp
impl *Store
fs::read
fs::write
fs::read_to_string
NamedTempFile
persist(
rename(
remove_file(
StoreLock
Mutex
RwLock
serde_json::from_
serde_json::to_
PolicyStore
CapabilityGrantStore
AgentStore
TaskStore
MemoryStore
ProjectStore
WorkspaceMcp
FilesService
EditService
SnapshotStore
AuditLog
ContextStore
```

Search both definitions and call sites.

---

## 6. Build a before-state ownership map

Before changing code, create a concrete table in implementation notes or the final report:

| Domain | Current implementation | Persistence | Writers | Readers | Interfaces | Status |
|---|---|---|---|---|---|---|
| Filesystem | ... | ... | ... | ... | MCP/CLI/TUI/API | canonical/duplicate |
| Memory | ... | ... | ... | ... | ... | ... |
| Tasks | ... | ... | ... | ... | ... | ... |
| Context | ... | ... | ... | ... | ... | ... |
| Project/workspace | ... | ... | ... | ... | ... | ... |
| Connectors | ... | ... | ... | ... | ... | ... |
| Agents | ... | ... | ... | ... | ... | ... |
| Capabilities | ... | ... | ... | ... | ... | ... |
| Policy | ... | ... | ... | ... | ... | ... |
| Editing | ... | ... | ... | ... | ... | ... |
| Snapshots | ... | ... | ... | ... | ... | ... |
| Provenance | ... | ... | ... | ... | ... | ... |
| Audit | ... | ... | ... | ... | ... | ... |

For every implementation classify it as canonical domain/service, canonical persistence/store, interface adapter, protocol/session state, compatibility/migration layer, legacy/dead duplicate, test fixture, or unrelated.

Do not classify a module as duplicate merely because its name contains store or service.

---

## 7. Canonical ownership rule

For each converged domain, choose ownership using evidence from current architecture.

Preferred rule:
```text
Interface adapter
  ↓
Application service
  ↓
Canonical domain/store
```

A service owns business semantics when the repository already establishes that service as the shared application boundary.

A core store owns persistence when it is already the repository's authoritative persistent representation.

An MCP module may own JSON-RPC protocol translation, MCP session lifecycle, MCP-specific argument/schema validation, MCP tool registration, and transport-specific metadata.

An MCP module must NOT own a second persistent representation of a domain that already has a canonical service/store.

Control API handlers may own HTTP route extraction, HTTP status mapping, and request/response serialization. CLI handlers may own argv parsing, output formatting, and exit-code mapping. TUI backends may own presentation state and refresh behavior. None may become an alternative business-logic owner.

---

## 8. Filesystem convergence

Current forensics must pay special attention to the known duplication between src/services/files.rs::FilesService and src/mcp/workspace.rs::WorkspaceMcp.

Compare path validation, workspace-root resolution, traversal rejection, absolute-path rejection, symlink handling, missing-target behavior, file-size limits, read semantics, write semantics, atomic-write behavior, delete semantics, listing limits, error behavior, locking/coordination, exact-byte support, and integration with authorization/edit/snapshot/audit.

Do not assume either implementation is correct merely because it is more complete.

The converged design must preserve the strongest applicable security and correctness guarantees without silently weakening existing behavior.

Where Prompt 14's filesystem coordination boundary exists in source, reuse it rather than creating an MCP-local lock or second filesystem coordination system.

Where Prompt 12's CLI editing boundary exists in source, CLI and MCP must converge on the same edit service rather than separate mutation paths.

Required invariant:
```text
MCP filesystem tool
  ↓
canonical Files/Edit service
  ↓
canonical path + coordination + mutation rules
```

Do not implement a new WorkspaceMcp write/delete algorithm to preserve old MCP behavior if the canonical service can provide the required behavior.

If a compatibility adapter is temporarily required, it must be thin, tested, and incapable of creating divergent persistent state.

---

## 9. Memory convergence

Inspect the MCP memory implementation and canonical memory store.

Determine authoritative data format, record identity, scope, size/count limits, locking, serialization, duplicate-ID semantics, create/update/delete semantics, restart behavior, and error behavior.

The final design must have one authoritative memory representation.

MCP memory operations must delegate to it.

Do not maintain an MCP-only cache as authoritative state, silently write a second file, translate one record format into another on every request, or invent a second ID scheme.

If existing data exists in more than one format, implement an explicit compatibility/migration path only when current evidence shows it is necessary. Migration must be deterministic, preserve valid records, fail closed on ambiguous/corrupt data, never silently discard records, and be idempotent.

---

## 10. Task convergence

Inspect the canonical task model/store and MCP task implementation.

Establish one authoritative task identifier, status vocabulary, priority vocabulary, timestamps, update semantics, persistence format, duplicate-ID behavior, list/filter semantics, and size/count limits.

MCP task calls must use the canonical model/store.

If an MCP task type is merely a protocol DTO, retain it only as a translation type and explicitly document that it is not authoritative.

Round-trip tests must prove create/list/get/update through MCP observes exactly the same state that direct service/core access observes.

---

## 11. Context/project/workspace convergence

Inspect all current context/project/workspace implementations. Distinguish static project instruction discovery, runtime context assembly, workspace identity, workspace persistence, MCP protocol context, and context-engine internal state.

Do not merge concepts merely because they share the word context.

Converge only genuinely duplicate domain state.

The final implementation must ensure one authoritative workspace identity, one authoritative project/runtime state representation where one already exists, no MCP-local workspace record that diverges from AWH workspace state, and no accidental mutation of read-only context discovery.

ContextEngine remains responsible for context assembly rather than becoming a generic persistence layer.

---

## 12. Connector persistence convergence

Inspect connector stores, custom MCP registry/trust stores, Composio/provider registries, and service/API adapters.

Separate external MCP server configuration, trust/authorization records, connector credential references, provider registry, and invocation runtime state.

Do not collapse distinct security boundaries into one store.

The convergence requirement is that two modules must not independently persist the same conceptual connector configuration with incompatible schemas or update semantics.

Secret material must remain under the existing secret boundary. Never migrate credentials by logging, embedding them in errors, or copying them into a general-purpose domain store.

---

## 13. Identity, capability, and policy convergence

Inspect AgentProfile/AgentStore, AgentSession, CapabilityGrantStore, PolicyStore, authorization service, and MCP authorization/trust mechanisms.

The goal is not to replace these systems. The goal is to ensure interfaces do not maintain shadow copies that can disagree with canonical state.

Required invariant:
```text
trusted caller identity
  ↓
canonical agent/session context
  ↓
canonical capability/policy evaluation
  ↓
canonical application service
```

MCP route names, CLI arguments, or local adapter state must never become an alternative authorization source.

If an MCP registry contains transport metadata, it may remain MCP-owned. It must not become the authoritative agent identity or capability store.

---

## 14. Editing convergence

Reuse the current canonical edit vocabulary/service boundary: EditId, EditOperation, ExpectedState, FileState, EditStatus, EditTransaction, EditError, EditRefs, and EditIdentity.

MCP must not maintain a second edit engine.

The target call graph is:
```text
MCP filesystem mutation
  ↓
MCP argument/schema adapter
  ↓
authorization/policy
  ↓
EditService
  ↓
filesystem coordination
  ↓
snapshot/provenance/audit
```

The exact call order must follow current canonical services and their security contracts.

---

## 15. Snapshot, provenance, rollback, and audit convergence

Prompt 15 must consume these canonical boundaries rather than recreate them.

There must be one authoritative snapshot store, one authoritative provenance representation, one authoritative rollback/recovery implementation, and one authoritative persistent audit writer/store.

MCP/CLI/TUI/API adapters may translate results but must not create MCP rollback files, CLI history databases, MCP snapshot stores, per-interface authoritative audit logs, or parallel provenance records.

Transport-specific security events may remain transport-specific when required, but must not be confused with canonical application audit history.

---

## 16. Store locking and persistence convergence

Inspect src/mcp/store_lock.rs and every current user of its locking primitive.

Determine what resource the lock protects, whether it is process-local or filesystem-based, lock acquisition ordering, reader behavior, write atomicity, release behavior, crash behavior, stale-lock behavior, and incompatible locking rules between stores.

Prompt 15 may relocate or expose a shared primitive if necessary for convergence, but must not create an unrelated generic locking framework.

Lock ownership must follow the authoritative persistence owner.

A compatibility adapter must not acquire a different lock for the same store in a way that permits concurrent divergent writes.

---

## 17. Persistence and schema convergence

For every migrated store determine current path, schema, version marker, serialization format, record identity, atomicity, durability, corruption behavior, and compatibility behavior.

Do not silently rewrite persisted files merely because a new structure is cleaner.

If schema migration is required: detect old format; validate; convert deterministically; preserve semantic data; atomically publish; retain explicit version/migration state; make startup idempotent; fail closed on ambiguity; test interruption and restart.

If no migration is required, preserve the existing format.

---

## 18. Corruption and fail-closed behavior

AWH's security posture is fail-closed.

If an authoritative store is unreadable, malformed, schema-incompatible, truncated, or internally inconsistent, do not treat it as empty, silently create a second store, fall back to MCP-local memory, accept an unsafe default, or overwrite it without explicit recovery semantics.

Return a deterministic structured error appropriate to the existing service/error model.

For security-sensitive state, corruption must not become implicit authorization.

---

## 19. Read/write ownership

For every domain, identify all writers.

A converged domain must satisfy:
```text
many callers
   ↓
one business-logic owner
   ↓
one authoritative persistence owner
```

Multiple readers are acceptable. Multiple writers are acceptable only when they all invoke the same canonical owner.

Forbidden:
```text
MCP writer ─────> MCP store
CLI writer ─────> Core store
```

Desired:
```text
MCP writer ─┐
CLI writer ─┤
TUI writer ─┼──> canonical service/store
API writer ─┘
```

---

## 20. Adapter design

Adapters may validate JSON types, map arguments, map errors, map results, attach transport/session metadata, and enforce protocol-specific limits.

Adapters must not read/write authoritative stores directly, duplicate domain validation, duplicate business rules, implement alternate locking, generate alternate IDs, maintain authoritative shadow state, or silently retry a mutation with different semantics.

Any performance cache must be demonstrably non-authoritative and have explicit consistency/invalidation semantics. Do not introduce a cache merely to avoid a service call.

---

## 21. Caller migration sequence

Execute one implementation linearly:
1. establish canonical ownership;
2. preserve the canonical implementation;
3. add a shared service adapter only where required;
4. migrate MCP callers;
5. migrate CLI callers that bypass the service;
6. migrate TUI callers that bypass the service;
7. migrate Control API callers that bypass the service;
8. delete or deprecate proven duplicates;
9. add architecture tests;
10. run full regression verification.

Do not stop after migrating MCP while another interface still writes a parallel store.

---

## 22. Migration safety

For each duplicate, prove all production callers are known, all tests are known, no public compatibility surface is accidentally broken, persisted data is preserved, error semantics are intentionally mapped, locking remains safe, and security checks remain enforced.

Before deleting a duplicate, search repository-wide references, tests/examples/docs, feature-gated modules, re-exports, and indirect constructors, then run the complete suite.

If evidence is insufficient, leave a clearly marked compatibility adapter rather than deleting blindly.

---

## 23. Service construction and dependency injection

Where services are instantiated independently by MCP, CLI, TUI, or Control API, determine whether this creates duplicate authoritative state.

Valid designs include one service instance per workspace over shared durable state, dependency-injected instances over the same canonical store, immutable configuration plus shared persistence, or scoped runtime services bound to an AgentSession.

The invariant is authoritative ownership, not object identity.

Avoid global mutable state solely to make interfaces converge.

---

## 24. Workspace and agent/session isolation

Convergence must not accidentally merge isolated state.

Persistent state must preserve required workspace, project, agent, session, worktree, and resource scope.

Do not use a global MCP store when the domain is workspace-scoped. Do not use an agent name as the only security boundary. Do not let an MCP session select another workspace merely by supplying a different path.

Where current services enforce workspace/session binding, adapters must consume those checks.

---

## 25. Concurrency

Analyze concurrent callers from multiple MCP sessions, MCP + CLI, MCP + Control API, multiple agent sessions, and multiple processes where applicable.

Tests must establish no lost update where serialization is required, no torn persistent representation, no duplicate identity allocation, no lock inversion/deadlock, no accidental last-writer-wins where conflict detection is required, no cross-workspace writes, and no stale adapter cache becoming authoritative.

Do not introduce distributed coordination. Use existing process/filesystem coordination boundaries.

---

## 26. Performance discipline

Convergence must not create unnecessary repeated filesystem scans, repeated JSON parsing, duplicate serialization, full-file buffering, lock contention across unrelated resources, network calls, or global mutexes.

Measure before optimizing. Correctness and authoritative state take priority over micro-optimizations.

---

## 27. Security invariants

The implementation must preserve:
1. authorization before consequential mutation;
2. route/CLI identity is not authorization;
3. configuration is not a substitute for PolicyEngine enforcement;
4. corrupt authorization/capability/policy state fails closed;
5. workspace containment remains centralized;
6. symlink/path traversal protections remain intact;
7. secrets are never copied into generic domain stores;
8. audit redaction remains centralized;
9. interface convergence does not expose more data than the previous boundary;
10. cross-workspace isolation remains intact;
11. compatibility paths cannot bypass canonical authorization.

Any migration path that temporarily bypasses a security boundary is invalid.

---

## 28. Error model convergence

Identify canonical error types/categories for each domain.

Interfaces need not expose identical text, but must share canonical domain classification, deterministic adapter mapping, no swallowed errors, no security denial converted into success, no altered-semantics retry, and no sensitive leakage.

Example:
```text
canonical PermissionDenied
   ├── MCP → structured JSON-RPC error
   ├── CLI → stable non-zero exit + sanitized stderr
   ├── API → appropriate HTTP status/error body
   └── TUI → user-visible error state
```

---

## 29. Serialization and compatibility

Protocol DTOs may differ from domain structs when the relationship is explicit:
```text
MCP DTO → domain request → canonical service → domain result → MCP DTO
```

Do not persist transport DTOs as authoritative state unless that is already the repository contract.

Test old valid data, new data, malformed data, unknown version, empty data, duplicate IDs, and restart for every changed persisted schema.

---

## 30. Architecture guardrails

Add tests or source-level architecture checks that prevent obvious regression.

At minimum detect MCP modules directly persisting domains already owned by canonical stores, new duplicate filesystem mutation implementations, direct MCP mutation of canonical JSON stores outside their owner, interface-local authoritative memory/task state, a second audit writer, a second snapshot store, a second rollback store, and direct bypass of canonical EditService for edit tools.

Prefer explicit dependency/API boundaries over brittle filename checks when practical.

---

## 31. Required convergence truth table

Produce and test a matrix similar to:

| Operation | MCP | CLI | TUI | Control API | Authoritative owner | Expected result |
|---|---|---|---|---|---|---|
| read file | adapter | adapter | adapter | adapter | FilesService | same bytes |
| write/edit file | adapter | adapter | adapter | adapter | EditService/FilesService | same safety semantics |
| memory create | adapter | adapter | adapter | adapter | MemoryStore/service | same record |
| task update | adapter | adapter | adapter | adapter | TaskStore/service | same status |
| policy check | adapter | adapter | adapter | adapter | PolicyStore/PolicyEngine | same decision |
| capability check | adapter | adapter | adapter | adapter | Capability subsystem | same decision |
| snapshot read | adapter | adapter | adapter | adapter | SnapshotStore | same state |
| rollback | adapter | adapter | adapter | adapter | rollback service | same recovery semantics |
| audit query | adapter | adapter | adapter | adapter | persistent AuditStore | same authoritative history |

Fill this matrix with actual repository operations, not hypothetical commands.

---

## 32. Required migration truth table

| Existing path | Canonical target | Data migration | Caller migration | Compatibility | Removal condition |
|---|---|---|---|---|---|
| ... | ... | ... | ... | ... | ... |

No duplicate may be deleted without a documented removal condition.

---

## 33. Required tests

### 33.1 Service/MCP parity
Create state through MCP, read through canonical service, read through another interface, update through canonical service, observe through MCP, restart, and observe again. Values must agree.

### 33.2 Reverse parity
Create through canonical service, read through MCP, update through MCP, read through service/API/CLI, restart, and verify persistence.

### 33.3 Persistence
Test valid state, empty state, multiple records, restart, atomic replacement, malformed JSON, truncation, unknown schema, duplicate IDs, and interrupted migration.

### 33.4 Concurrency
Test two MCP sessions, MCP + CLI, MCP + API, concurrent update/delete, concurrent create with the same identity, and unrelated workspaces concurrently.

### 33.5 Security
Test unauthorized callers, wrong workspace, inactive agent, denied capability, denied policy, corrupt policy/capability state, path traversal, symlink escape, secret-bearing arguments, and legacy adapters attempting direct persistence.

### 33.6 Filesystem
Include UTF-8, Devanagari, emoji, LF, CRLF, no final newline, empty file, zero-byte file, missing file, exact bytes where supported, size limits, and concurrent mutation.

### 33.7 Migration
Test old valid format to canonical format, repeated migration, failure injection, malformed old format, data preservation, and no duplicate records after migration.

### 33.8 Architecture regression
Tests must prove a new MCP-local store cannot silently become authoritative.

---

## 34. Real-interface validation

Do not validate only internal functions.

Where supported, run real awh CLI commands, real MCP JSON-RPC requests, real Control API requests, relevant TUI/backend paths, and restart/reload flows.

At least one real end-to-end path must demonstrate:
```text
interface → canonical service → canonical store → restart → interface
```

For MCP, use the existing dispatcher/transport harness and real-client validation where practical. Do not create a second MCP protocol implementation.

---

## 35. Compatibility requirements

Preserve externally visible behavior unless it conflicts with the canonical security/architecture contract.

When behavior changes because duplicate implementations previously disagreed, identify the disagreement, select behavior from the authoritative current contract, document compatibility impact, and add regression tests.

Do not preserve a bug merely because one interface had it if doing so would keep duplicate business logic.

---

## 36. Direct filesystem access audit

Search all MCP and interface modules for direct std::fs, tokio::fs, OpenOptions, NamedTempFile, rename, remove_file, read_to_string, and write usage.

Classify every hit as canonical implementation, protocol/session state, test fixture, migration, compatibility, or duplicate.

The goal is not zero filesystem calls outside FilesService; protocol/session code may legitimately persist MCP-owned state. The goal is zero duplicate authoritative domain mutation paths.

---

## 37. Store duplication audit

Search all JSON stores, JSONL stores, TOML state, filesystem-backed registries, in-memory authoritative maps, and caches that appear authoritative.

For each, record owner, path, schema, lock, writers, readers, migration, scope, and security boundary.

Do not merge distinct stores solely because they both use JSON.

---

## 38. MCP-specific state that may remain MCP-owned

The following may remain MCP-owned when current architecture requires it: JSON-RPC protocol lifecycle, negotiated protocol version, transport session IDs, transport-specific client metadata, MCP tool registry, MCP provider runtime clients, and MCP connection/session resources.

However, if MCP state represents an AWH domain entity such as an agent, workspace, capability, task, memory record, file edit, snapshot, or audit event, the AWH canonical domain remains authoritative.

---

## 39. Control API and TUI convergence

Inspect all Control API and TUI paths for direct store access.

Target:
```text
Control API → service
TUI → service
MCP → service
CLI → service
```

If an interface reads a canonical store directly for a read-only query and there is no business-logic duplication, do not create an unnecessary service layer solely for cosmetic purity.

Create or reuse a service boundary when business rules are duplicated, authorization is required, multiple interfaces need identical behavior, persistence semantics are duplicated, or state mutation occurs.

---

## 40. Dependency boundaries with other implementation prompts

Prompt 15 is standalone. It may consume current source corresponding to Prompts 01–14, but implementation must use whatever actually exists on the current rust branch.

Do not wait for another prompt, require another prompt's PR, modify another implementation prompt, or reimplement another prompt's subsystem.

If a referenced boundary is incomplete in current source, implement only the minimum adapter/convergence change needed to preserve the current contract and document the limitation.

---

## 41. Canonical implementation sequence

Execute linearly:
1. forensics;
2. ownership inventory;
3. canonical ownership selection;
4. semantic/schema/security/locking comparison;
5. preserve canonical implementation;
6. strengthen shared service boundary only where required;
7. migrate MCP;
8. migrate CLI/TUI/Control API;
9. migrate persisted data only where necessary;
10. remove or quarantine proven duplicates;
11. add architecture regression tests;
12. add parity/concurrency/security tests;
13. repeat duplicate-mechanism audit;
14. run verification;
15. produce final report.

---

## 42. Canonical target architecture

```text
                    ┌──────────────────────┐
                    │ External Agent       │
                    └──────────┬───────────┘
                               │
                ┌──────────────┼──────────────┐
                ▼              ▼              ▼
              MCP            CLI/TUI       Control API
                │              │              │
                └──────────────┼──────────────┘
                               ▼
                     Agent/session context
                               │
                               ▼
                    Authorization / Policy
                               │
                               ▼
                 AWH Application Services
        ┌──────────────┬──────────────┬──────────────┐
        ▼              ▼              ▼              ▼
   Files/Edit      Memory/Tasks   Context/Project   Git/etc.
        │              │              │
        └──────────────┴──────────────┴──────────────┘
                               │
                               ▼
                    Canonical persistence
                               │
             ┌─────────────────┼─────────────────┐
             ▼                 ▼                 ▼
        filesystem          .agent state      Git state
             │
             ▼
       Snapshot / Provenance / Audit
```

Do not create an additional MCP service layer beside the AWH service layer.

---

## 43. Acceptance criteria

Prompt 15 is complete only when all applicable criteria are true.

### Ownership
- [ ] Every targeted domain has one documented authoritative owner.
- [ ] Every production writer is mapped.
- [ ] Duplicate authoritative writers are removed or converted to thin adapters.
- [ ] Distinct security stores remain distinct where required.

### MCP
- [ ] MCP filesystem mutations use canonical services.
- [ ] MCP memory uses canonical memory state.
- [ ] MCP tasks use canonical task state.
- [ ] MCP policy/capability decisions use canonical authorization boundaries.
- [ ] MCP edit tools use canonical EditService.
- [ ] MCP snapshot/rollback/audit paths use canonical services.
- [ ] MCP protocol/session state remains separate from AWH domain state.

### Other interfaces
- [ ] CLI does not maintain a second authoritative domain store.
- [ ] TUI does not maintain a second authoritative domain store.
- [ ] Control API does not maintain a second authoritative domain store.

### Persistence
- [ ] Existing valid data is preserved.
- [ ] Required migration is atomic and idempotent.
- [ ] Corrupt data fails closed.
- [ ] Unknown schemas do not silently become empty stores.
- [ ] Restart preserves canonical state.

### Security
- [ ] Authorization remains centralized.
- [ ] Path containment remains centralized.
- [ ] Secrets are not copied into generic stores.
- [ ] Cross-workspace isolation remains intact.
- [ ] Compatibility paths cannot bypass authorization.

### Concurrency
- [ ] Concurrent writers use the canonical coordination boundary.
- [ ] No new lock inversion/deadlock is introduced.
- [ ] Concurrent interface operations observe consistent state.

### Architecture
- [ ] Architecture regression tests exist.
- [ ] No second filesystem business implementation remains for the same contract.
- [ ] No second memory/task authoritative store remains.
- [ ] No second audit/snapshot/rollback authority is introduced.
- [ ] The final call graph shows all interfaces converging.

### Validation
- [ ] Real CLI validation passes where applicable.
- [ ] Real MCP validation passes where applicable.
- [ ] Control API/TUI paths are exercised where applicable.
- [ ] Migration/restart tests pass.
- [ ] Security and concurrency tests pass.

---

## 44. Required verification gates

Run:
```bash
cargo fmt --all -- --check
cargo check --all-targets
cargo test --all-targets
cargo clippy --all-targets --all-features -- -D warnings
git diff --check
```

Also run targeted tests for MCP, filesystem, memory, tasks, authorization/policy, edit, snapshot/rollback, audit, Control API, CLI, migration, concurrency, and architecture regression.

If a required gate cannot run in the environment, report the exact reason and do not claim it passed.

---

## 45. Final duplicate-mechanism audit

Repeat repository-wide searches for WorkspaceMcp, FilesService, direct MCP filesystem writes/deletes, MemoryMcp, TasksMcp, PolicyStore, CapabilityGrantStore, AgentStore, EditService, SnapshotStore, AuditLog, StoreLock, direct JSON persistence, and direct std::fs mutation in interface modules.

For every remaining implementation explain why it is canonical, protocol-specific, compatibility-only, test-only, or intentionally separate.

Do not accept 'it was already there' as an explanation.

---

## 46. Performance and correctness review

Review lock duration, disk reads/writes, duplicate serialization, duplicate hashing, duplicate path canonicalization, repeated full-store scans, cache invalidation, startup migration cost, and concurrent request contention.

Any optimization must preserve authorization, isolation, persistence correctness, deterministic errors, and audit/provenance correlation.

---

## 47. Independence rule

This prompt is executable against the current rust branch.

Do not assume Prompt 14, Prompt 16, Prompt 17, or any historical PR has been merged.

Use current source as the final authority.

Do not modify another docs/implementation-prompts/*.md, historical prompt collections, or unrelated documentation. The documentation change for this task is limited to this Prompt 15 file.

---

## 48. Final implementation report

At completion report:
1. Forensics — documents and source modules inspected.
2. Before ownership map — duplicate implementations, writers/readers, persistence formats, interface paths.
3. Canonical ownership — final service/store owner for every converged domain.
4. Caller migration — MCP, CLI, TUI, Control API.
5. Persistence/migration — schemas, migration, corruption, restart.
6. Security — authorization, containment, isolation, secret handling.
7. Concurrency — locking/coordination, race tests, deadlock analysis.
8. Tests — parity, persistence, migration, concurrency, security, architecture regression, real-interface validation.
9. Verification — exact command results.
10. Changed files — every implementation file changed by this prompt.
11. Removed/deprecated duplicates — exact paths and removal evidence.
12. Remaining duplication — exact paths and why each remains.
13. Limitations — platform/runtime/compatibility limitations.
14. Scope confirmation — no unrelated architecture rewrite, no new database/ORM/event bus, no second authorization/edit/snapshot/rollback/audit system, and standalone execution.

The final report must distinguish verified behavior from assumptions and historical claims.