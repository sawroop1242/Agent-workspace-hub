# ARCH-001 — Unify MCP and Service/Core Stores — Master Implementation Prompt

## 0. Mission

Implement ARCH-001 as a production-grade architecture/convergence change that removes divergent business-logic and persistence implementations between the MCP layer and AWH's canonical service/core layer.

The target architecture is:

```text
Agent / MCP / CLI / TUI / Control API
                │
                ▼
      transport/interface adapters
                │
                ▼
       canonical services/domain
                │
                ▼
       canonical persistence/stores
                │
                ▼
          .agent / workspace
```

MCP must become an adapter over canonical services rather than an alternate implementation of filesystem, memory, task, connector, or related domain behavior.

Do not perform a speculative rewrite. First establish the current implementation truth from source, tests, schemas, persistence formats, and callers; then migrate one domain at a time with compatibility and recovery evidence.

---

## 1. Non-negotiable execution rules

1. Work only on ARCH-001.
2. Do not begin ARCH-002 or any later issue/prompt.
3. Do not reopen or redesign the completed AWE-001, AWE-002, or AWE-003 editing milestones unless ARCH-001 exposes a concrete integration defect that must be fixed for correctness.
4. Do not create a second authorization, policy, filesystem-security, audit, snapshot, or edit engine inside MCP.
5. Do not make MCP-specific business rules when an authoritative service already exists.
6. Preserve public behavior unless an intentional migration requires a documented compatibility change.
7. Prefer incremental migration over a flag-day rewrite.
8. Every migrated domain must have tests proving MCP and non-MCP callers reach the same authoritative implementation.
9. Fail closed on unreadable/corrupt persistent state and authorization/security failures; never silently fall back to an unsafe duplicate store.
10. Do not delete a legacy store until its consumers, persistence format, migration path, and rollback implications are understood and tested.
11. Do not claim a domain is unified merely because MCP calls a helper; prove that the helper is the single authoritative owner of the domain behavior and persistence semantics.
12. Keep the patch scoped. If unrelated defects are discovered, record them as follow-up work unless they are required to make ARCH-001 correct and testable.

---

## 2. Repository and architecture preflight — mandatory before coding

Before modifying code, inspect the current `rust` branch and build an implementation map.

Read at minimum:

- `docs/PROJECT_CONTEXT.md`
- `docs/PROJECT_STATUS.md`
- `docs/PROJECT_ROADMAP.md`
- `docs/PROJECT_ROADMAP_STATUS.md`
- `docs/MASTER_PROMPT.md`
- `docs/mcp.md`
- `docs/CLI.md`
- `docs/testing.md`
- the current ARCH-001 issue
- all relevant modules under `src/mcp/`
- all relevant canonical services under `src/services/`
- relevant domain/model modules
- relevant persistence/configuration modules
- existing integration and security tests

Then search the complete repository for:

- `MemoryMcp`
- `TasksMcp`
- `ConnectorsMcp`
- filesystem/workspace MCP implementations
- `StoreLock`
- JSON persistence paths under `.agent/`
- memory schemas and serializers
- task status/priority models
- connector models and registries
- canonical `FilesService`/filesystem helpers
- policy/capability checks
- audit calls
- snapshot/provenance calls
- MCP dispatcher/tool registry registrations
- CLI/TUI/Control API callers of the same domains

The current codebase already contains a cross-process `StoreLock` for JSON-backed project stores; it explicitly documents MemoryMcp, TasksMcp, and ConnectorsMcp persistence under `.agent/`. Treat that as evidence to investigate, not as proof that the architecture is already unified. fileciteturn282file0L2-L2

The current repository also exposes MCP domain implementations and store-related modules through `src/mcp/`; existing documentation identifies separate JSON-backed memory, task, connector, policy, and trust persistence paths. Verify all such statements against current source before relying on them. fileciteturn283file1L29-L48 fileciteturn283file3L67-L95

Produce a private forensic matrix before implementation:

| Domain | MCP owner | Canonical service/core owner | Persistence | Schema/model | Locking | Security/policy | Other callers | Migration risk |
|---|---|---|---|---|---|---|---|---|
| Filesystem | verify | verify | verify | verify | verify | verify | verify | verify |
| Memory | verify | verify | verify | verify | verify | verify | verify | verify |
| Tasks | verify | verify | verify | verify | verify | verify | verify | verify |
| Connectors | verify | verify | verify | verify | verify | verify | verify | verify |

Do not code until the matrix is sufficiently complete to identify the authoritative owner for each domain.

---

## 3. Define “canonical owner” precisely

For ARCH-001, a canonical owner is the single production implementation responsible for:

- domain invariants
- input validation
- authorization/security integration where applicable
- persistence semantics
- locking/concurrency semantics
- serialization/deserialization
- corruption handling
- error vocabulary
- lifecycle/state transitions
- migration compatibility
- observable behavior shared by all interfaces

A thin MCP adapter may translate JSON-RPC/MCP arguments and results, but it must not independently reimplement those responsibilities.

The following are not sufficient by themselves:

- calling a common utility while retaining MCP-local persistence
- sharing structs while maintaining separate stores
- sharing a JSON file while maintaining different validation/state machines
- duplicating code with equivalent behavior
- routing only one MCP operation to the service while other operations still mutate MCP-local state

---

## 4. Target architecture

Establish one authoritative implementation per domain.

### 4.1 Filesystem

All filesystem reads, writes, deletes, path validation, workspace containment, symlink protections, atomic mutation, edit semantics, expected-state checks, verification, rollback, and related security behavior must remain in the canonical filesystem/edit services.

MCP must not maintain a parallel filesystem implementation.

For editing, preserve the existing canonical chain and do not bypass it:

```text
MCP filesystem/edit tool
        ↓
canonical service boundary
        ↓
EditTransaction / EditService
        ↓
filesystem security + atomic mutation
        ↓
verification / provenance / audit as applicable
```

Do not reintroduce whole-file rewrite fallbacks that bypass precise edit semantics.

### 4.2 Memory

Identify the authoritative memory domain model and persistence format.

Requirements:

- one canonical memory model
- one canonical serializer/deserializer
- one authoritative persistence location/format for project memory
- one locking/concurrency strategy
- one validation/error model
- one migration mechanism for existing MCP-local or legacy formats
- MCP adapter delegates to the canonical owner

If multiple formats currently exist, implement an explicit versioned migration rather than silently interpreting incompatible data.

Migration must be:

- deterministic
- idempotent
- atomic
- corruption-aware
- recoverable
- tested against representative legacy data

Do not discard data that cannot be safely migrated.

### 4.3 Tasks

Identify and consolidate task models and status vocabulary.

Requirements:

- one canonical `Task` representation
- one canonical task ID model
- one canonical priority model
- one canonical status vocabulary and transition rules
- one persistence format
- one locking/concurrency strategy
- one validation/error model
- MCP delegates rather than maintaining an independent task state machine

Explicitly test status compatibility if the MCP and service layers currently use different names, values, or transition semantics.

### 4.4 Connectors

Identify the authoritative connector model, registry, persistence, enable/disable semantics, provider configuration, and security boundary.

MCP must not maintain a second connector registry or second lifecycle/persistence implementation.

Preserve the distinction between:

- connector configuration/state
- external MCP/provider trust
- capability/policy authorization
- actual connector invocation

Do not collapse these security boundaries merely to simplify storage.

### 4.5 Other stores discovered during forensics

If additional MCP-local stores are discovered, classify each as:

1. canonical and reusable;
2. adapter-only state that legitimately belongs to MCP transport/session handling;
3. duplicate domain state that must migrate under ARCH-001;
4. intentionally separate security/trust state that must remain separate for a documented reason.

Do not force transport/session state into domain persistence merely to satisfy the word “unify.”

---

## 5. Store and persistence contract

For every canonical persistent store, establish a common set of guarantees where applicable:

- deterministic path resolution
- workspace/project scoping
- schema/version identification
- atomic writes
- bounded locking
- stale-lock recovery where supported
- corrupt-store detection
- fail-closed behavior
- explicit migration/version handling
- no partial writes
- clear serialization errors
- bounded resource usage
- deterministic ordering where observable
- concurrent read-modify-write correctness
- restart persistence

Reuse the existing `StoreLock` where its semantics are appropriate rather than inventing another lock mechanism. The current implementation uses exclusive lock-file creation, bounded acquisition, stale-lock reclamation, and cleanup on guard drop; any migration must preserve or deliberately improve those concurrency guarantees. fileciteturn282file0L2-L2

Do not create a generic “super-store” abstraction solely to hide incompatible domain models. Unification means one authoritative owner per domain, not one giant undifferentiated database API.

---

## 6. MCP adapter boundary

Refactor MCP so each tool follows this pattern:

```text
MCP request
  → schema/argument validation
  → caller/session/workspace context
  → policy/capability authorization
  → canonical service call
  → canonical store/service behavior
  → canonical error mapping
  → MCP response
```

The MCP layer may own:

- MCP JSON schema
- JSON-RPC request/response mechanics
- transport concerns
- MCP tool metadata
- conversion between wire types and domain types
- MCP-specific error envelopes
- MCP session/transport state

The MCP layer must not own:

- duplicate domain stores
- duplicate filesystem security
- duplicate mutation engines
- duplicate task state machines
- duplicate memory schemas
- duplicate connector registries
- alternate policy engines
- alternate audit semantics

Inspect `dispatcher.rs` and `tool_registry.rs` carefully because MCP tool registration and dispatch currently expose domain operations such as workspace, memory, tasks, and connectors. The final architecture must ensure those tools delegate consistently rather than embedding divergent domain behavior. fileciteturn281file5L139-L147 fileciteturn281file3L97-L105

---

## 7. Identity, authorization, and security preservation

ARCH-001 must integrate with the existing authorization architecture rather than bypass it.

For every migrated mutation, preserve the ordering:

```text
identify caller
→ resolve agent/session/workspace
→ capability/policy decision
→ domain validation
→ canonical service/store operation
→ audit where required
```

Never move authorization below a persistence mutation.

Never let the new shared store become a way to bypass:

- capability grants
- policy decisions
- agent/session identity
- workspace containment
- path validation
- symlink protections
- MCP trust controls
- high-risk built-in tool restrictions
- audit requirements

Unknown or unauthorized operations must remain denied.

Corrupt policy/security state must remain fail closed.

---

## 8. Migration strategy

Use an incremental migration, preferably in this order unless forensic evidence proves a safer order:

1. establish canonical interfaces/contracts;
2. migrate filesystem/workspace behavior where duplicate MCP behavior remains;
3. migrate memory;
4. migrate tasks;
5. migrate connectors;
6. remove or deprecate orphaned MCP stores;
7. add architecture/conformance tests;
8. update documentation and migration notes.

For every domain:

### Phase A — inventory

Identify every producer and consumer.

### Phase B — canonical contract

Define the service/domain API and persistence contract without duplicating business logic.

### Phase C — compatibility adapter

If required, allow old MCP calls to translate into the canonical representation temporarily.

### Phase D — data migration

Migrate existing persisted data atomically and idempotently.

### Phase E — cutover

Route all MCP operations to the canonical implementation.

### Phase F — verification

Prove behavior equivalence and persistence correctness.

### Phase G — cleanup

Remove or deprecate the duplicate implementation only after all references are gone and migration/recovery evidence exists.

---

## 9. Backward compatibility

Before changing schemas or persistence paths, determine:

- existing released formats
- current on-disk files
- old field names
- old status values
- old IDs
- optional/missing fields
- empty-store behavior
- unknown fields
- corrupted files
- partially migrated files

Migration must not silently convert invalid state into valid-looking state.

If a compatibility layer is required, make it explicit, bounded, testable, and removable.

Do not keep two authoritative representations indefinitely.

---

## 10. Failure and recovery semantics

Define behavior for:

- missing store
- empty store
- malformed JSON
- schema-version mismatch
- unknown fields where relevant
- interrupted write
- failed atomic rename
- lock timeout
- stale lock
- concurrent writer
- migration failure
- migration crash/restart
- permission denied
- path outside workspace
- symlink escape
- authorization denial
- service-level validation failure
- MCP serialization failure

For all mutation failures, determine whether zero-mutation semantics are required and test them.

A failed migration must not leave the canonical store in a partially migrated state.

A failed read must not silently instantiate an empty authoritative store unless that behavior is explicitly safe and documented.

---

## 11. Concurrency requirements

This project is explicitly designed for multiple agents and potentially multiple processes.

Test real read-modify-write races for migrated persistent stores.

Requirements:

- no lost updates
- bounded lock acquisition
- no indefinite deadlock
- stale-lock recovery where supported
- no unsafe unlocked fallback
- deterministic behavior after contention
- safe concurrent readers/writers
- correct behavior across independent processes where practical

Do not assume an in-process mutex is sufficient for project persistence. The current `StoreLock` documentation specifically identifies cross-process contention as a requirement. fileciteturn282file0L2-L2

---

## 12. Testing strategy

Add behavior-focused tests at several layers.

### 12.1 Canonical service unit tests

For each migrated domain:

- create
- read
- update
- delete where supported
- invalid input
- missing item
- duplicate item
- persistence failure
- serialization failure
- corrupted state
- authorization failure
- concurrent access

### 12.2 Migration tests

Test:

- representative legacy store → canonical store
- empty legacy store
- large legacy store
- malformed legacy store
- unsupported schema version
- unknown fields
- interrupted migration simulation
- repeated migration/idempotence
- migration followed by normal reads/writes
- migration preserves all supported data

### 12.3 MCP conformance tests

For every MCP operation that was migrated, prove:

```text
MCP request → canonical service
```

and not:

```text
MCP request → MCP-local domain implementation
```

Where practical, instrument or mock the canonical service boundary and assert that the MCP adapter reaches it.

### 12.4 Cross-interface equivalence tests

For representative operations:

```text
MCP → canonical service
CLI → canonical service
TUI/control path → canonical service
```

must produce equivalent domain state and persistence results.

Do not compare only JSON response formatting; compare authoritative domain state.

### 12.5 Architecture/conformance tests

Create tests or static checks that fail when a new MCP module introduces a duplicate domain store or direct persistence path without an explicit architecture exception.

At minimum detect/review:

- MCP-local JSON persistence
- duplicate task structs/status enums
- duplicate memory serializers
- duplicate connector registries
- direct filesystem mutation from MCP tools
- direct `.agent` store writes outside canonical store owners

Avoid brittle textual checks if an AST/module-level check or architectural test can provide stronger guarantees.

### 12.6 Security regression tests

Pin that migration does not bypass:

- workspace containment
- symlink protection
- policy
- capabilities
- identity/session scope
- MCP trust
- high-risk built-in denial
- audit requirements

---

## 13. Filesystem and encoding edge cases

For all migrated persistent stores and filesystem adapters, test where applicable:

- UTF-8
- Devanagari
- CJK
- emoji
- spaces in paths
- Unicode filenames
- empty files/stores
- large records
- newline variations
- missing final newline where textual files are involved
- nested workspaces
- symlinks
- path traversal
- concurrent access
- read-only/permission failures

Do not assume ASCII-only data.

---

## 14. Performance and resource limits

Measure before and after where the migration changes persistence behavior.

Avoid:

- loading unbounded stores into memory without limits
- repeated full-store serialization for every tiny operation when a safer bounded approach exists
- excessive lock hold times
- repeated migration on every request
- duplicate parsing of the same state

Migration should normally happen once per legacy format/version and then operate on the canonical format.

Do not sacrifice security or correctness merely for micro-optimizations.

---

## 15. Documentation requirements

Update documentation only after implementation facts are established.

Document:

- canonical owner for each domain
- MCP adapter boundary
- persistence locations/formats
- schema versions
- migration behavior
- compatibility policy
- locking/concurrency behavior
- error behavior
- security boundaries
- which MCP-local stores were removed/deprecated
- any intentional exceptions and why they are not domain duplication

Do not claim “single source of truth” unless the code and tests prove it.

Keep architecture documentation aligned with source and tests.

---

## 16. Required implementation evidence

Before declaring ARCH-001 complete, provide evidence for every domain migrated:

| Requirement | Evidence |
|---|---|
| One canonical owner | source path + call graph evidence |
| MCP delegates | MCP adapter/source evidence |
| No duplicate persistence | repository search + architecture test |
| Canonical schema | model/serializer source |
| Migration | migration code + tests |
| Locking | implementation + concurrency tests |
| Corruption handling | tests |
| Security preserved | authorization/security tests |
| Cross-interface equivalence | integration tests |
| Restart persistence | filesystem integration tests |
| Documentation | updated docs |

No row may be marked complete without concrete evidence.

---

## 17. Verification commands

Run the complete Rust verification suite after implementation:

```bash
cargo fmt --all -- --check
cargo check --all-targets
cargo test --all-targets
cargo clippy --all-targets --all-features -- -D warnings
```

Also run targeted tests for:

- every migrated store/domain
- migrations
- MCP adapters
- architecture/conformance checks
- concurrency
- corruption/recovery
- security/authorization

If repository-specific scripts or CI checks exist, run the relevant ones too.

Do not report success from compilation alone.

---

## 18. Git/diff discipline

Before finalizing:

```bash
git status --short
git diff --stat
git diff --check
git diff
```

Review every changed file.

Reject unrelated changes unless they are strictly necessary for ARCH-001 correctness.

Do not modify future issue prompts, unrelated architecture, product features, or research work.

---

## 19. Definition of Done

ARCH-001 is complete only when all applicable conditions are true:

- [ ] canonical owner identified for every targeted domain
- [ ] MCP no longer owns duplicate domain business logic
- [ ] MCP no longer owns duplicate persistent stores for migrated domains
- [ ] filesystem behavior routes through canonical services
- [ ] memory uses one authoritative model/store with migration
- [ ] tasks use one authoritative model/status vocabulary
- [ ] connectors use one authoritative model/store/registry boundary
- [ ] persistence formats are versioned or explicitly compatible
- [ ] migrations are atomic, idempotent, corruption-aware, and tested
- [ ] locking is correct for concurrent agents/processes
- [ ] no unsafe unlocked fallback exists
- [ ] security/capability/policy checks remain authoritative
- [ ] MCP/CLI/other interfaces share domain semantics
- [ ] architecture/conformance tests prevent regression
- [ ] corrupted state fails safely
- [ ] restart persistence is verified
- [ ] Unicode and relevant filesystem edge cases pass
- [ ] targeted tests pass
- [ ] full fmt/check/test/clippy gates pass
- [ ] documentation reflects actual implementation
- [ ] final diff contains no unrelated architectural changes

---

## 20. Explicit non-goals

Do not use ARCH-001 to:

- redesign the entire AWH architecture
- create a generic workflow engine
- create a generic database abstraction for every subsystem
- replace MCP itself
- redesign agent identity/session architecture
- redesign the policy engine
- redesign Git integration
- redesign the editing transaction model
- add unrelated UI features
- add a new model/router layer
- merge AWH Compute research into the core AWH architecture
- claim future roadmap items as implemented

If a discovered dependency genuinely requires one of these, document it as a blocker/follow-up rather than silently expanding scope.

---

## 21. Final implementation report

At completion, report:

1. forensic findings;
2. canonical owner selected for each domain;
3. files changed;
4. files removed/deprecated, with justification;
5. migration format and compatibility behavior;
6. MCP adapter changes;
7. security/authorization preservation;
8. concurrency/locking behavior;
9. architecture/conformance tests;
10. targeted test results;
11. full CI-equivalent verification results;
12. remaining known limitations;
13. explicit evidence for each Definition-of-Done item.

Clearly distinguish:

- implemented
- tested
- partially validated
- intentionally deferred
- blocked by another issue

Never turn “planned” into “complete.”

---

## 22. HARD STOP

After ARCH-001 is implemented, tested, documented, and verified:

**STOP.**

Do not begin ARCH-002 or any later issue.

Do not proactively modify another issue prompt.

Do not broaden the architecture beyond the accepted ARCH-001 scope.

The next issue must be started only by an explicit instruction to move next.
