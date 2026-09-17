# ARCH-001 — Unify MCP and Service/Core Stores — Master Implementation Prompt

## 0. Mission

Implement ARCH-001 as a production-grade architecture/convergence change that removes divergent domain logic and persistence implementations between MCP and AWH's canonical service/core layer.

Target architecture:

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
          workspace / .agent
```

MCP must become an adapter over canonical services. It must not remain an alternate implementation of filesystem, memory, task, connector, or other domain behavior.

Do not perform a speculative rewrite. Establish current implementation truth first, then migrate incrementally with compatibility, concurrency, security, and recovery evidence.

---

## 1. Non-negotiable execution rules

1. Work only on ARCH-001. Do not begin ARCH-002, FS-001, GIT-001, or later work.
2. Do not reopen AWE-001/AWE-002/AWE-003 unless ARCH-001 exposes a concrete integration defect required for correctness.
3. A canonical owner may already exist, or may need to be extracted/created during ARCH-001. Do not assume every domain already has a reusable service.
4. Do not create a second authorization, policy, filesystem-security, audit, snapshot, or edit engine inside MCP.
5. Do not create a second domain store merely because a shared helper or model exists.
6. Preserve public behavior unless an intentional migration requires a documented compatibility change.
7. Prefer incremental migration over a flag-day rewrite.
8. Every migrated domain must prove that MCP and other production interfaces reach the same authoritative implementation.
9. Fail closed on corrupt/unreadable security state and never silently fall back to an unsafe duplicate store.
10. Do not delete a legacy store until its production consumers, tests, persistence format, migration path, rollback implications, and restart behavior are understood and verified.
11. Shared state is not sufficient evidence of architectural unification. One authoritative service/store implementation is required.
12. Keep the patch scoped to ARCH-001. Record unrelated defects as follow-up work unless they block correctness or verification.

---

## 2. Mandatory forensic preflight

Before coding, inspect the current `rust` branch and the ARCH-001 issue.

Read at minimum:

- `docs/PROJECT_CONTEXT.md`
- `docs/PROJECT_STATUS.md`
- `docs/PROJECT_ROADMAP.md`
- `docs/PROJECT_ROADMAP_STATUS.md`
- `docs/MASTER_PROMPT.md`
- `docs/mcp.md`
- `docs/CLI.md`
- `docs/testing.md`
- relevant security/authorization documentation
- all relevant `src/mcp/` modules
- all relevant `src/services/` modules
- domain/model modules
- persistence/configuration modules
- integration, concurrency, and security tests

Search the complete repository for at least:

- `MemoryMcp`
- `TasksMcp`
- `ConnectorsMcp`
- filesystem/workspace MCP implementations
- `FilesService`
- `StoreLock`
- `.agent/` persistence paths
- memory/task/connector models and serializers
- `PolicyStore`
- `PersistentTrustStore`
- capability/policy authorization
- caller/session/agent/workspace context
- audit/provenance/snapshot code
- MCP dispatcher and tool registry
- CLI/TUI/control-plane callers of the same domains
- direct `std::fs`/`tokio::fs` persistence inside MCP domain code

The current branch contains MCP-side domain implementations and JSON-backed stores, while filesystem operations already have canonical service/edit infrastructure. Verify all such facts against current source before relying on them.

Build this forensic matrix before implementation:

| Domain/store | Current owner | Canonical target owner | Persistence | Schema/model | Locking | Security boundary | Other callers | Migration risk | Disposition |
|---|---|---|---|---|---|---|---|---|---|
| Filesystem | verify | verify/create | verify | verify | verify | verify | verify | verify | migrate/retain |
| Memory | verify | verify/create | verify | verify | verify | verify | verify | verify | migrate/retain |
| Tasks | verify | verify/create | verify | verify | verify | verify | verify | verify | migrate/retain |
| Connectors | verify | verify/create | verify | verify | verify | verify | verify | verify | migrate/retain |
| Policy | verify | security authority | verify | verify | verify | policy | verify | verify | separate |
| Trust | verify | security authority | verify | verify | verify | trust | verify | verify | separate |
| Other | verify | verify/create | verify | verify | verify | verify | verify | verify | classify |

Do not code until every relevant persistent domain has an explicit disposition.

---

## 3. Canonical-owner definition

A canonical owner is the single production implementation responsible for the domain's:

- invariants
- validation
- lifecycle/state transitions
- persistence semantics
- serialization/deserialization
- locking/concurrency semantics
- corruption handling
- error vocabulary
- migration compatibility
- security integration where applicable
- observable behavior shared by interfaces

A common helper, shared struct, or shared file is not enough.

The following do NOT constitute unification:

- MCP calls a helper but still persists data itself
- two stores use the same JSON schema
- two stores point at the same file but have different validation/state logic
- duplicated code with equivalent behavior
- only some MCP operations route through a service

If no canonical owner exists, extract or create one as part of ARCH-001 rather than preserving an MCP implementation and calling it canonical.

---

## 4. Explicit ownership and dependency contract

The final architecture must make these ownership boundaries observable in code.

### MCP adapter owns only

- MCP/JSON-RPC wire schemas
- request/response mechanics
- transport concerns
- MCP tool metadata
- wire-to-domain conversion
- MCP error envelopes
- legitimate MCP session/transport/request state

### Dispatcher owns only

- request context extraction
- caller/session/workspace resolution handoff
- argument decoding/validation handoff
- policy/capability decision orchestration
- canonical service invocation
- domain-error-to-MCP-error mapping

`dispatcher.rs` must not become a new domain service or persistence layer.

### Tool registry owns only

- tool names
- schemas
- metadata
- risk classification/registration data
- routing metadata

It must not own domain state or persistence.

### Canonical services own

- domain operations
- domain invariants
- validation
- lifecycle/state transitions
- authorization integration where applicable
- orchestration of stores and filesystem primitives

### Canonical stores own

- persistence
- serialization
- schema versions
- atomic writes
- store locking
- corruption handling
- migrations
- persistence-specific errors

Do not introduce a giant generic "super-store" solely to hide incompatible domains.

---

## 5. Service construction, dependency injection, and workspace ownership

Every migrated service must have an explicit construction contract.

Determine and document in code/tests:

- service lifetime
- workspace/project binding
- agent/session context requirements
- store construction and ownership
- dependency ownership
- test injection/mocking strategy
- initialization behavior
- whether a service is request-scoped, session-scoped, workspace-scoped, or process-scoped

A canonical store must never accidentally become process-global when its state is workspace-scoped.

The construction path must make it impossible or clearly unsafe to operate on the wrong workspace.

For every persistent domain answer:

```text
Which workspace owns this state?
Where is it persisted?
Who constructs the store?
Who may access it?
How is isolation enforced?
How is the same store reached by MCP/CLI/TUI/control callers?
```

---

## 6. Filesystem domain

Filesystem behavior must have one canonical implementation.

All filesystem reads/writes/deletes, path validation, workspace containment, symlink protections, atomic mutation, expected-state checking, verification, rollback, and related mutation semantics must remain behind the canonical filesystem/edit service boundary.

Preserve the existing editing architecture:

```text
MCP edit tool
    ↓
canonical service
    ↓
EditTransaction / EditService
    ↓
filesystem security + mutation
    ↓
verification / provenance / audit as applicable
```

MCP must not directly persist files or implement a parallel mutation engine.

Do not weaken AWE edit semantics or reintroduce whole-file rewrite fallbacks that bypass precise mutation behavior.

FS-001 owns the deeper filesystem TOCTOU/mutation-coordination hardening. ARCH-001 must establish the correct canonical ownership boundary without attempting to absorb unrelated FS-001 work.

---

## 7. Memory domain

Establish one canonical memory domain owner.

Requirements:

- one canonical memory model
- one serializer/deserializer
- one authoritative project-memory persistence location/format
- one locking strategy appropriate to the store
- one validation/error model
- explicit migration for legacy/MCP-local formats
- all MCP memory operations delegate to the canonical owner

Migration must be deterministic, idempotent, atomic, corruption-aware, recoverable, and tested.

Do not silently discard unsupported or corrupt data.

---

## 8. Tasks domain

Establish one canonical task owner.

Requirements:

- one canonical Task representation
- one task-ID model
- one priority model
- one status vocabulary
- one transition/state-machine definition
- one persistence owner
- one concurrency strategy appropriate to the store
- one validation/error model
- MCP delegates to the canonical implementation

If current MCP and service semantics differ, define explicit compatibility mapping and tests before removing the old behavior.

---

## 9. Connectors domain

Establish one canonical connector configuration/state owner.

Unification must preserve the distinction between:

1. connector configuration/state;
2. external MCP/provider trust;
3. capability/policy authorization;
4. actual connector invocation/session behavior.

Do not merge security/trust stores into connector persistence simply to reduce store count.

Do not create a second connector registry or lifecycle implementation in MCP.

---

## 10. Policy and trust are security authorities, not generic domain stores

`PolicyStore`, `PersistentTrustStore`, or equivalent security authorities must be explicitly classified during forensics.

ARCH-001 must NOT merge, alias, replace, or co-own these stores with ordinary memory/task/connector persistence merely for architectural symmetry.

If policy or trust persistence is migrated, the migration must preserve the security authority, fail-closed semantics, identity scope, and existing tested authorization behavior.

Any proposed change to security-store ownership requires explicit justification and dedicated regression tests.

Transport/session state that is legitimately MCP-local may remain MCP-local. The agent must classify state rather than treating every MCP-local file as a defect.

---

## 11. Persistence contract

For each canonical persistent domain, apply the following guarantees where semantically applicable:

- deterministic path resolution
- workspace scoping/isolation
- schema/version identification
- atomic writes
- bounded lock acquisition
- stale-lock recovery where supported
- corruption detection
- fail-closed behavior where security requires it
- explicit migrations
- no partial writes
- clear serialization errors
- bounded resource usage
- deterministic ordering where observable
- correct concurrent read-modify-write semantics
- restart persistence

Reuse `StoreLock` when its semantics fit. Do not assume it is a universal locking primitive; a domain may require another concurrency mechanism when justified by its semantics.

Never use an unsafe unlocked fallback merely because a lock cannot be acquired.

---

## 12. Hard invariant: no direct MCP domain persistence

After migration, MCP domain adapters must not directly implement persistence for migrated domains.

Prohibited in MCP domain/tool/dispatcher code unless an explicitly documented architecture exception exists:

- direct reads/writes of `.agent/<domain>` files
- direct JSON persistence for domain state
- direct `OpenOptions`/file replacement for domain stores
- direct filesystem mutation that bypasses canonical services
- MCP-local duplicate serializers/state machines/registries

Approved MCP-local persistence may exist only for genuinely transport/session/security state with an explicit classification and ownership rationale.

Add architecture/conformance checks where practical so this invariant remains enforceable.

---

## 13. Migration strategy — dependency driven, not predetermined

Do not blindly follow a fixed domain order. Determine migration order from the actual dependency graph, shared invariants, testability, and risk.

For each domain use:

### Phase A — Inventory

Identify every producer, consumer, persistence path, schema, and security dependency.

### Phase B — Canonical contract

Define or extract the authoritative service/domain/store boundary.

### Phase C — Construction and ownership

Wire workspace, context, store, and service dependencies explicitly.

### Phase D — Compatibility

If required, translate legacy inputs/formats into the canonical representation.

### Phase E — Data migration

Migrate existing data atomically and idempotently.

### Phase F — Cutover

Route all production callers through the canonical implementation.

### Phase G — Verification

Prove domain equivalence, persistence correctness, security preservation, concurrency behavior, restart behavior, and failure semantics.

### Phase H — Cleanup

Only after the deletion gate is satisfied, remove or deprecate the duplicate implementation.

The order of filesystem, memory, tasks, connectors, and other domains must be chosen from the forensic dependency graph. A different order is valid if evidence shows it reduces risk.

---

## 14. Initialization and lifecycle semantics

For every persistent store explicitly distinguish:

- missing store on first use
- valid empty store
- malformed/corrupt store
- unsupported schema version
- permission failure
- interrupted migration
- partially written state

A missing first-run store may be initialized only when the domain contract explicitly permits it.

A malformed or unreadable authoritative store must not silently become a new empty store.

Document whether initialization is eager or lazy and test restart behavior.

---

## 15. Migration rollback and deletion gates

A migration is not complete until both success and failure paths are tested.

### Migration rollback requirements

Test that:

- migration failure leaves the source/legacy data valid when rollback semantics require it;
- canonical data is never partially migrated;
- crash/restart during migration is recoverable;
- repeated migration is safe/idempotent;
- successful migration is followed by normal reads/writes;
- no supported data is silently lost.

### Legacy deletion gate

Do not delete a legacy store/implementation until all are true:

- zero production callers remain;
- no test relies on the legacy path as an authority;
- canonical persistence is verified across restart;
- migration is verified on representative legacy data;
- rollback/recovery behavior is understood;
- repository search shows no unintended references;
- documentation no longer describes the legacy implementation as authoritative.

Deprecation is preferred before irreversible deletion when compatibility risk is non-trivial.

---

## 16. Security and identity preservation

For every migrated mutation preserve the effective ordering:

```text
identify caller
→ resolve agent/session/workspace
→ capability/policy decision
→ domain validation
→ canonical service/store operation
→ audit/provenance where required
```

Never move authorization below persistence.

The migration must not bypass:

- agent identity
- session scope
- workspace isolation
- capabilities
- policy
- path validation
- symlink protections
- MCP trust controls
- high-risk built-in tool restrictions
- audit requirements

Unknown or unauthorized operations remain denied.

---

## 17. Failure and recovery matrix

Define and test behavior for:

- missing store
- valid empty store
- malformed JSON/data
- schema mismatch
- unknown fields where relevant
- interrupted write
- atomic rename failure
- lock timeout
- stale lock
- concurrent writer
- migration failure
- migration crash/restart
- permission denied
- wrong workspace
- traversal/symlink escape
- authorization denial
- service validation failure
- MCP serialization failure

For every mutation, define whether failure guarantees zero mutation, partial mutation, or transactional rollback. Do not leave this implicit.

---

## 18. Concurrency requirements

AWH supports multiple agents/processes. Test real cross-task and, where practical, cross-process races.

Requirements:

- no lost updates
- bounded contention
- no indefinite deadlock
- safe stale-lock handling where supported
- no unsafe unlocked fallback
- deterministic behavior under contention
- correct concurrent readers/writers
- correct read-modify-write semantics
- restart-safe persisted state

Do not substitute an in-process mutex for cross-process persistence coordination when the domain requires process-level isolation.

---

## 19. Testing and architecture conformance

### Canonical service tests

For every migrated domain test create/read/update/delete as supported, invalid input, missing data, duplicate data, persistence failure, serialization failure, corrupt state, authorization failure, and concurrency.

### MCP delegation tests

For every migrated MCP operation prove:

```text
MCP request → canonical service
```

not:

```text
MCP request → MCP-local domain implementation
```

Instrument or inject the service boundary where practical.

### Cross-interface tests

Representative operations from MCP, CLI, TUI/control paths must produce equivalent authoritative domain state and persistence results.

### Migration tests

Cover representative legacy data, empty data, malformed data, unsupported versions, unknown fields, interrupted migration, idempotence, restart, and data preservation.

### Architecture checks

Add static/structural tests where practical to detect:

- MCP-local domain JSON persistence
- duplicate task state machines
- duplicate memory serializers
- duplicate connector registries
- direct filesystem mutation from MCP tools
- direct `.agent` writes outside canonical store owners

Prefer module/AST-level checks over brittle text matching when practical.

### Security regression tests

Pin workspace containment, identity/session scope, policy/capabilities, trust, high-risk tool restrictions, symlink protections, and audit behavior.

---

## 20. Compatibility and edge cases

Before changing schemas or paths inspect:

- released/on-disk formats
- old field names
- old IDs
- old status values
- optional/missing fields
- empty stores
- unknown fields
- corrupted files
- partially migrated files
- Unicode/UTF-8 data
- Devanagari/CJK/emoji
- spaces and Unicode filenames
- nested workspaces
- read-only permissions
- large records and bounded resource behavior

Do not assume ASCII-only data.

---

## 21. Verification protocol

Before declaring ARCH-001 complete:

1. Re-run repository-wide searches for all legacy MCP domain owners and persistence paths.
2. Confirm each migrated domain has exactly one authoritative production owner.
3. Confirm service construction enforces workspace/context ownership.
4. Confirm MCP dispatcher/tool registry remain adapter/routing layers.
5. Confirm no unauthorized direct MCP persistence remains.
6. Confirm policy/trust remain correctly separated security authorities.
7. Run formatting, compilation, unit tests, integration tests, concurrency tests, migration tests, and security regressions relevant to the changed code.
8. Test restart persistence.
9. Test migration failure and recovery.
10. Review the final diff for unrelated changes.
11. Update documentation only where it describes the new authoritative architecture or migration behavior.

If a verification requirement cannot be executed in the environment, report exactly what was and was not verified; do not claim success by inference.

---

## 22. Definition of Done

ARCH-001 is complete only when:

- MCP no longer owns duplicate migrated domain logic/persistence;
- canonical owners exist for every migrated domain, including newly extracted services where necessary;
- workspace/service/store construction is explicit and safe;
- direct MCP domain persistence is eliminated or explicitly justified as non-domain state;
- filesystem operations route through the canonical service boundary;
- memory/tasks/connectors have one authoritative domain implementation each;
- policy/trust security authorities remain appropriately separate;
- migrations are deterministic, atomic, idempotent, and recoverable;
- legacy deletion gates are satisfied;
- concurrent access is tested according to each store's real semantics;
- MCP, CLI, TUI/control callers converge on the same authoritative behavior;
- security/identity/policy guarantees remain intact;
- architecture conformance tests prevent regression;
- relevant documentation reflects the actual implementation.

---

## 23. Non-goals

Do not use ARCH-001 to:

- redesign the entire MCP protocol layer;
- redesign authorization from scratch;
- replace the existing AWE editing architecture;
- implement FS-001's complete TOCTOU solution;
- implement GIT-001 worktree isolation;
- merge security/trust authorities into ordinary domain stores without explicit security justification;
- introduce a speculative database migration unrelated to the observed architecture problem;
- rewrite unrelated modules for style or cleanup.

---

## 24. Hard stop conditions

Stop and report instead of guessing when:

- the authoritative owner cannot be established from source/tests;
- two stores have incompatible semantics that require an unresolved product decision;
- migration could lose data and no safe compatibility rule exists;
- security-store ownership would change without sufficient evidence;
- workspace ownership/isolation cannot be proven;
- concurrency semantics are unclear;
- a required behavior change would exceed ARCH-001 scope;
- tests contradict the assumed architecture and the contradiction cannot be safely resolved.

The objective is not merely fewer files or fewer stores. The objective is one demonstrably authoritative implementation per domain, with explicit boundaries, safe persistence, preserved security, recoverable migration, and consistent behavior across all AWH interfaces.
