# AWE-012 / #33 — File Snapshots and Provenance

## Master Issue-Resolving Prompt

### Mission

Implement the production-grade **file snapshot and provenance substrate** for AWH and integrate it with the canonical agent-grade editing path.

The objective is to make every consequential edit capable of producing a durable, exact pre-edit recovery record and a queryable provenance record without creating a second editing model, a second identity model, or transport-specific storage.

The implementation must establish this canonical relationship:

```text
Caller / Agent
    → Session / Workspace identity
    → Capability + Policy authorization
    → EditTransaction
    → pre-edit FileState
    → persistent FileSnapshot
    → filesystem mutation
    → post-edit FileState
    → provenance record
    → audit linkage
```

The snapshot is the **recovery boundary** for edit-level rollback. Provenance is the **explainability/history boundary**. They are related, but they are not the same record and must not be conflated.

---

## 1. Authoritative forensic baseline

Before changing code, inspect the current `rust` branch and verify the implementation rather than trusting historical documentation.

The current architecture establishes that:

- `src/services/edit.rs` is the canonical, transport-independent edit transaction model.
- `EditTransaction` owns `EditId`, operations, expected state, before state, after state, and lifecycle status.
- `FileState` contains path, SHA-256 hash, byte size, and line count.
- AWE-006 defines atomic/recovery-aware edit behavior.
- AWE-007 defines stale-state/conflict semantics.
- AWE-008 defines post-edit verification semantics.
- AWE-010 defines edit-level rollback and requires exact restoration of the original state.
- AWE-011 establishes the capability/policy authorization boundary and caller identity requirements.
- `src/context/snapshot.rs` is a **context-engine snapshot subsystem**. It stores context-engine state and intentionally does not duplicate source-file bytes. It must remain conceptually separate from file recovery snapshots.
- The roadmap places snapshots/provenance after editing, policy, and rollback foundations and requires CLI, MCP, TUI, and Control API to share the same application services.

Do not infer missing production functionality from a type, CLI command, documentation entry, or context snapshot implementation. Verify what actually exists.

If the repository has evolved since this prompt was written, preserve the current canonical architecture unless there is concrete evidence that an existing abstraction is incorrect or unsafe.

---

## 2. Non-negotiable architectural rules

### 2.1 One canonical file snapshot service

Create or complete **one transport-independent file snapshot service** shared by:

- MCP
- CLI
- TUI
- Control API
- `EditService`
- rollback/recovery logic

Do not create separate snapshot stores or serialization formats for each interface.

### 2.2 File snapshots are not context snapshots

Do **not** reuse `ContextSnapshot` as a file-recovery mechanism.

The existing context snapshot system may continue to manage context items, token budgets, task metadata, and references, but file snapshots require their own explicit abstraction because they must preserve exact prior file bytes and filesystem recovery metadata.

A correct architecture may place both systems under `.agent`, but they must have distinct namespaces, types, ownership, lifecycle, and semantics.

For example:

```text
.agent/
├── context-engine/
│   └── snapshots/          # context snapshots; existing subsystem
└── snapshots/              # file snapshots; AWE-012
```

The exact final directory may differ if the existing storage architecture provides a better canonical location, but file snapshots must never silently share context-snapshot records.

### 2.3 Do not create a second identity model

AWE-011 is authoritative for caller identity, agent, session, workspace, capability, and policy context.

Provenance must reference the existing identity/session structures. Do not invent an independent `AgentId`, `SessionId`, trust record, or authorization identity solely for snapshots.

If an existing identity abstraction is incomplete, integrate with it through a narrow optional/reference field rather than creating a competing model.

### 2.4 Snapshots are not sandboxing

A file snapshot provides recovery and provenance. It does not provide process isolation, syscall isolation, network isolation, secrets isolation, or a security sandbox.

Do not expand AWE-012 into container/VM/sandbox implementation.

---

## 3. Define the canonical file snapshot model

Design a stable serializable model sufficient for exact recovery and provenance.

At minimum establish explicit concepts equivalent to:

```text
SnapshotId
FileSnapshot
SnapshotEntry
SnapshotStore
ProvenanceRecord
```

The exact Rust names may follow existing project conventions.

### 3.1 Snapshot identity

A snapshot must have a stable unique identifier that is safe for persistent storage and cannot escape its storage directory.

Requirements:

- deterministic validation rules for externally supplied IDs
- bounded length
- no `/`, `\\`, `..`, absolute paths, control characters, or unsafe filename components
- collision-resistant generated IDs
- stable serialization
- no dependence on in-memory process lifetime

A snapshot ID must be independently addressable after process restart.

### 3.2 Edit linkage

Every edit-generated snapshot must be linked to the exact `EditId` that caused it.

The relationship must be queryable in both useful directions where architecture permits:

```text
EditId → snapshot(s)
SnapshotId → originating EditId
```

For multi-operation transactions, preserve transaction-level identity rather than generating unrelated anonymous snapshots that cannot be associated with the edit transaction.

If the transaction touches multiple files, the implementation must support either:

- one transaction snapshot containing multiple file entries, or
- a deterministic set of per-file snapshots grouped under one transaction-level snapshot record.

Do not lose the transaction relationship.

### 3.3 Exact pre-edit bytes

The snapshot must preserve the exact bytes required to restore the pre-edit file state.

Do not reconstruct the old file from:

- inverse text replacement
- line operations
- unified-diff reversal
- current filesystem content
- hashes alone
- normalized text
- guessed encoding/newline rules

The snapshot must represent the actual pre-edit byte sequence.

This requirement is what makes AWE-010 edit-level rollback reliable.

### 3.4 File metadata

For every captured file include sufficient state to verify the snapshot and restore boundary, including at minimum:

- workspace-relative path
- pre-edit SHA-256
- byte size
- line count where meaningful to the canonical `FileState`
- existence state
- snapshot timestamp
- originating `EditId`

The design must be able to distinguish:

```text
existing file with empty contents
missing file
zero-byte file
non-empty file
```

Do not collapse those states.

### 3.5 Restoration metadata

The snapshot must preserve enough information to restore exact prior state for files that existed before the edit.

It must also support transaction cases where the edit creates a new file or otherwise changes existence state.

For example:

```text
before: missing → after: created
before: existing → after: modified
before: existing → after: deleted
```

The snapshot representation must make the correct rollback behavior unambiguous.

---

## 4. Persistent storage requirements

Snapshots must survive process restart.

The storage layer must:

1. live under the workspace's `.agent` state
2. be workspace-scoped
3. use a canonical storage path
4. validate IDs before constructing paths
5. avoid path traversal and symlink escape
6. use atomic persistence
7. use appropriate locking/concurrency coordination
8. detect partial/corrupt records
9. fail closed on corruption
10. avoid silently overwriting a different snapshot
11. provide deterministic retrieval/listing behavior

### 4.1 Atomic persistence

Do not rely on direct `fs::write` for a production snapshot commit if that can expose a partially written snapshot after interruption.

Use the project's canonical atomic-write/storage helper if one exists.

If no suitable helper exists, implement the minimum safe mechanism inside the snapshot storage abstraction rather than scattering temporary-file/rename logic across transports.

A snapshot is not considered persisted until the durable record and required content storage have successfully committed according to the storage contract.

### 4.2 Locking

Concurrent snapshot operations must not corrupt the store.

Define and test behavior for:

- two processes creating snapshots concurrently
- one process reading while another commits
- concurrent metadata updates
- concurrent restore/delete attempts
- repeated creation with the same snapshot ID

Prefer the repository's existing locking/storage primitive when available.

Do not introduce an unrelated global lock that serializes the entire workspace unnecessarily.

### 4.3 Crash boundaries

Reason explicitly about interruption during:

- snapshot content write
- metadata write
- temporary-file creation
- rename/commit
- multi-file snapshot creation
- provenance persistence

A failed snapshot must never be reported as successfully committed.

Temporary artifacts must not be mistaken for valid snapshots during normal listing/recovery.

---

## 5. Content storage design

Choose a storage representation based on actual project constraints, not convenience.

Possible designs include:

```text
metadata + embedded bytes
```

or:

```text
metadata record + content blob
```

If content blobs are separated from metadata, the metadata must contain a stable content reference and integrity hash.

If content is embedded, enforce resource limits so a malicious or accidentally huge file cannot exhaust memory or disk through one snapshot request.

### Integrity

Every stored content payload must be integrity-checkable.

At minimum:

```text
stored bytes
    ↓
SHA-256
    ↓
compare with recorded pre-edit hash
```

On mismatch, treat the snapshot as corrupted and refuse recovery rather than silently restoring potentially corrupted bytes.

Do not attempt “best effort” recovery from corrupted snapshot bytes.

---

## 6. Provenance model

Create a structured provenance record associated with the edit transaction and snapshot.

At minimum capture:

- `edit_id`
- `snapshot_id`
- workspace identity/reference
- session identity/reference when available
- agent identity/reference when available
- operation type(s)
- affected path(s)
- timestamp
- before hash(es)
- after hash(es) once verification succeeds
- before size/line-count metadata where available
- after size/line-count metadata where available
- outcome/status
- rollback linkage where applicable

The provenance record should explain **what happened**, while the snapshot contains **what exact bytes are required to recover**.

Do not put raw file contents into ordinary provenance records.

### Sensitive content boundary

Normal audit/provenance output must not dump:

- complete source files
- snapshot bytes
- secrets
- tokens
- credentials
- large binary payloads

Logs should contain identifiers, metadata, hashes, paths subject to existing redaction policy, and structured outcomes—not recovery payloads.

If a future privileged diagnostic operation needs access to snapshot contents, that must be an explicit protected operation and is outside the default provenance/logging path.

---

## 7. EditService integration

Integrate the snapshot service at the **canonical EditService boundary**.

Do not add snapshot creation separately to MCP handlers, CLI commands, or TUI callbacks.

The canonical sequence for a successful edit should be conceptually:

```text
Request
  ↓
shape validation
  ↓
identity/session resolution
  ↓
capability + policy authorization
  ↓
expected-state/conflict validation
  ↓
read exact pre-edit bytes
  ↓
construct FileState
  ↓
create/persist snapshot
  ↓
apply atomic edit transaction
  ↓
read actual resulting bytes
  ↓
verify post-edit FileState
  ↓
record provenance
  ↓
commit transaction
```

The exact lifecycle states must remain compatible with the canonical `EditStatus` model.

### Critical ordering rule

A snapshot must exist and be durably recoverable **before destructive mutation is committed**.

Never:

```text
mutate
→ snapshot
```

for an operation whose recovery guarantee depends on the snapshot.

The correct safety relationship is:

```text
validated pre-state
→ durable recovery snapshot
→ mutation
→ verification
```

If the snapshot cannot be safely committed, the edit must not proceed.

---

## 8. Interaction with AWE-006 atomic rollback

AWE-012 must integrate with the recovery guarantees established by AWE-006.

Do not duplicate rollback logic.

AWE-006 owns transaction-level recovery mechanics; AWE-012 owns durable exact pre-edit snapshot persistence and provenance.

A correct relationship is:

```text
AWE-012 Snapshot
       │
       ▼
exact durable pre-state
       │
       ▼
AWE-006 recovery/atomicity
       │
       ▼
filesystem restoration
```

If a transaction fails after mutation but before successful verification, recovery must be able to use the transaction's captured pre-state according to AWE-006 semantics.

Do not invent a second rollback algorithm inside the snapshot store.

---

## 9. Interaction with AWE-010 edit-level rollback

AWE-010 must be able to identify and consume the correct snapshot through the canonical edit identity.

Rollback must:

1. resolve the original `EditId`
2. resolve its snapshot
3. verify snapshot integrity
4. verify current filesystem state against the expected post-edit state
5. restore exact original bytes/state
6. verify the restored state
7. persist rollback provenance

If the snapshot is missing, corrupt, incomplete, or inconsistent with the original edit, rollback must fail safely without mutating the target.

Do not silently fall back to inverse editing.

---

## 10. Interaction with AWE-007 stale-state/conflict detection

Snapshot creation does not replace expected-state validation.

The system must still validate that the target filesystem is in the expected pre-edit state before mutation.

Correct ordering:

```text
caller expected state
        ↓
current filesystem state
        ↓
conflict check
        ↓
snapshot exact current bytes
        ↓
mutation
```

Do not create a snapshot of stale state and then mutate it.

If expected-state validation fails:

- return a structured conflict
- do not mutate the file
- do not create a misleading “successful edit” provenance record
- do not mark the snapshot as an applied edit snapshot

If the architecture permits diagnostic capture of a conflict state, it must be explicitly distinguished from an edit-recovery snapshot and must never be mistaken for one.

---

## 11. Interaction with AWE-008 verification

The snapshot records the **before** boundary.

AWE-008 establishes the verified **after** boundary.

Provenance should therefore only record an authoritative `after_hash`/after-state once the post-edit verification has actually succeeded.

Do not write a successful provenance record merely because the filesystem write returned `Ok(())`.

For failed verification:

```text
mutation attempted
→ verification failed
→ recovery according to AWE-006
→ final filesystem state verified
→ provenance records failure/recovery outcome
```

The final provenance state must accurately describe what happened.

---

## 12. Multi-file transaction semantics

AWE-004/AWE-006 semantics must remain authoritative for multi-operation transactions.

A multi-file edit must not create an ambiguous collection of unrelated snapshots.

The system must be able to answer:

- which transaction created these snapshots?
- which files were captured?
- what was each exact pre-state?
- what was each verified post-state?
- did the transaction commit?
- did recovery occur?
- was rollback later requested?

If any required pre-state snapshot fails before mutation begins, the transaction must not partially mutate files merely because some snapshots succeeded.

If a later snapshot/store operation fails while preparing a transaction, all prepared-but-uncommitted transaction state must be handled deterministically and must not be exposed as a successful edit.

Do not claim filesystem rollback semantics are solved merely because snapshot metadata was written.

---

## 13. Filesystem edge cases

Test and define behavior for:

- missing files
- empty files
- zero-byte files
- one-line files
- files with and without a final newline
- LF line endings
- CRLF line endings
- Unicode
- Devanagari text
- emoji
- very long lines
- large files within configured limits
- multiple files in one transaction
- newly created files
- deleted files
- paths containing spaces
- nested workspace paths
- symlink targets
- symlink replacement/escape attempts
- concurrent modification
- concurrent snapshot access

Exact bytes must survive the snapshot round trip.

Never normalize line endings or Unicode during snapshot capture.

---

## 14. Security requirements

Treat snapshot storage as security-sensitive persistent state.

### Path safety

- workspace containment must be enforced
- snapshot IDs must be validated before path construction
- canonical path/security helpers must be reused
- symlink escape must fail closed
- absolute paths must be rejected where the service contract requires workspace-relative paths
- `..` traversal must be rejected

### Authorization

Snapshot creation triggered by an edit inherits the already-authorized edit operation.

Explicit snapshot management operations such as inspect, restore, delete, or export must pass through the canonical capability/policy boundary established by AWE-011.

Do not allow the existence of a snapshot to bypass authorization for its original file.

A snapshot containing sensitive file bytes must not become a side channel around filesystem policy.

### Information disclosure

Errors and logs should avoid exposing full snapshot contents.

Hash values, IDs, metadata, and bounded paths may be exposed according to existing policy, but raw recovery content must remain protected.

---

## 15. Corruption and fail-safe behavior

Corruption is a security/reliability failure, not a recoverable parsing inconvenience.

Test corruption of:

- metadata JSON/serialization
- content payload
- recorded hash
- snapshot ID/reference
- edit linkage
- provenance record
- partial/truncated files
- unknown schema versions
- missing content referenced by metadata

Expected behavior:

```text
corruption detected
→ structured error
→ no destructive mutation
→ no guessed recovery
→ no false success state
```

If schema versioning is introduced, reject unsupported future versions safely unless a forward-compatible strategy is explicitly implemented and tested.

---

## 16. Schema/version compatibility

Persistent snapshots survive process upgrades, so define an explicit serialization/versioning strategy.

At minimum consider:

- schema version
- backwards-compatible reads where practical
- explicit rejection of unsupported versions
- stable field semantics
- migration boundaries

Do not silently reinterpret old snapshot bytes as a new schema.

If migration is necessary, keep it inside the snapshot storage/service boundary rather than in MCP/CLI/TUI code.

---

## 17. Resource limits and DoS protection

A snapshot subsystem must not become an unbounded disk or memory sink.

Define bounded behavior for:

- maximum snapshot content size
- maximum transaction file count
- maximum total transaction snapshot size
- maximum metadata size
- maximum snapshot count
- maximum provenance record size
- maximum concurrent snapshot operations where relevant

Large-file behavior must fail predictably before unsafe resource consumption.

Do not silently truncate source bytes. If a snapshot cannot preserve the exact pre-state, the corresponding destructive edit must not proceed.

---

## 18. Provenance query model

The service must expose a transport-independent way to retrieve provenance by `EditId` and, where useful, by `SnapshotId`.

The result should contain structured metadata rather than raw content.

At minimum support the conceptual queries:

```text
get provenance by edit_id
get snapshot metadata by snapshot_id
list snapshots for workspace
list snapshots/provenance for an edit transaction
```

Sorting and pagination must be deterministic if the store can contain many records.

Do not make provenance dependent on scanning arbitrary log text.

---

## 19. Audit integration

AWE-012 must integrate with the project's canonical audit system if available.

Do not build a second audit/event store merely because provenance exists.

Audit events should reference:

- `edit_id`
- `snapshot_id`
- agent/session/workspace identifiers as permitted
- operation/outcome
- timestamp
- verification/recovery status

They must not contain the actual snapshot bytes.

Distinguish clearly between:

```text
snapshot data       = exact recovery payload
provenance record   = structured edit history
audit event         = security/observability event
```

These records may reference one another but have different responsibilities.

---

## 20. Transport integration

MCP, CLI, TUI, and Control API must not implement their own snapshot semantics.

They should call the same application/service layer.

If snapshot functionality is exposed through MCP, its schemas must represent stable snapshot identifiers and metadata without embedding arbitrary recovery payloads in ordinary responses.

CLI commands such as the roadmap's snapshot family should become thin adapters over the service rather than separate filesystem implementations.

TUI views must consume service results rather than opening `.agent` files directly.

---

## 21. Testing strategy

Do not stop at unit tests for serialization.

### Unit tests

Test:

- snapshot ID validation
- generated ID uniqueness
- exact byte hashing
- metadata serialization
- existence-state representation
- edit/snapshot linkage
- provenance serialization
- schema-version validation
- integrity mismatch detection
- resource limits

### Real filesystem tests

Use temporary workspaces and verify:

1. create file
2. capture snapshot
3. mutate file
4. reload snapshot after dropping the service instance
5. verify exact original bytes
6. restore bytes
7. verify hash/size/line count

### Restart persistence

Explicitly simulate:

```text
process A
→ edit/snapshot
→ process exits

process B
→ opens same workspace
→ resolves snapshot by edit_id
→ validates bytes
```

### Multi-file tests

Verify that all pre-states in a transaction are captured and remain correctly linked.

### Corruption tests

Modify persisted snapshot data and verify fail-safe behavior.

### Failure-injection tests

Inject failures during:

- content persistence
- metadata persistence
- rename/commit
- lock acquisition
- post-write verification
- provenance persistence

Confirm that no unsafe edit is reported as successful.

### Rollback integration tests

Perform:

```text
edit
→ snapshot
→ verify
→ restart
→ rollback by EditId
→ verify exact original bytes
```

Also test:

```text
edit
→ snapshot
→ external modification
→ rollback
```

and confirm conflict/no-mutation behavior according to AWE-010.

### Unicode/newline tests

Use representative UTF-8, Devanagari, emoji, LF, CRLF, and no-final-newline files and compare raw bytes, not only strings.

### Security tests

Attempt:

- traversal IDs
- absolute paths
- symlink escape
- snapshot access outside workspace
- unauthorized snapshot restore/delete
- corrupted snapshot recovery
- forged edit/snapshot linkage

All must fail closed.

---

## 22. Property-oriented invariants

Where practical, encode these invariants in tests:

### Invariant A — exact capture

```text
snapshot.bytes == original_file_bytes
```

### Invariant B — hash integrity

```text
sha256(snapshot.bytes) == snapshot.before_hash
```

### Invariant C — restart durability

```text
persist(snapshot)
→ new process/store
→ load(snapshot)
→ equivalent record
```

### Invariant D — no destructive edit without snapshot

```text
snapshot_commit_failed
→ filesystem_before == filesystem_after
```

### Invariant E — rollback source integrity

```text
rollback(snapshot)
→ restored_bytes == snapshot.before_bytes
```

### Invariant F — transaction linkage

```text
snapshot.edit_id == originating EditTransaction.id
```

### Invariant G — provenance accuracy

```text
successful provenance
→ verified after-state exists
```

### Invariant H — no content leakage

```text
ordinary audit/log output
≠ snapshot recovery payload
```

---

## 23. Performance and implementation quality

Keep the snapshot implementation straightforward and reliable before optimizing.

Avoid unnecessary duplicate reads of very large files when the existing edit pipeline can safely reuse validated bytes.

However, **do not compromise correctness for micro-optimizations**.

If bytes are reused between validation, snapshotting, and mutation, ensure the implementation does not weaken TOCTOU protections or allow stale bytes to be committed.

Document any deliberate tradeoff between memory usage and repeated filesystem reads.

---

## 24. Backward compatibility

Preserve existing public types and behavior unless a change is required for correctness.

In particular:

- do not break `EditTransaction` serialization unnecessarily
- do not change existing context snapshot semantics
- do not remove AWE-006 rollback guarantees
- do not weaken AWE-007 conflict detection
- do not weaken AWE-008 verification
- do not bypass AWE-011 authorization
- do not create transport-specific alternatives to the canonical services

If an API must evolve, use additive/versioned changes where possible and document compatibility behavior.

---

## 25. Documentation and code quality

Update implementation-level documentation only where required by the actual code change.

The implementation must make the ownership boundaries obvious:

```text
EditService
  owns edit transaction orchestration

SnapshotService
  owns durable exact pre-state snapshots

ProvenanceService / record model
  owns structured edit history metadata

PolicyEngine
  owns authorization

Audit system
  owns security/observability events

ContextSnapshot
  owns context-engine state
```

Do not introduce circular dependencies between these layers.

Keep serialization/storage details behind the service boundary where possible.

---

## 26. Explicit non-goals

Do **not** use AWE-012 to implement:

- generic backup software
- filesystem version control replacing Git
- container/VM sandboxing
- a second context snapshot system
- a second identity/authentication model
- a second audit system
- a generic event-sourcing framework
- model/provider routing
- agent reasoning/orchestration
- unrelated Git worktree functionality
- generic object storage
- arbitrary cloud backup
- content-addressed storage as a standalone product

Only introduce abstractions required for durable file snapshots and provenance in the AWH edit lifecycle.

---

## 27. Verification workflow

After implementation, run the complete project verification suite required by the repository:

```bash
cargo fmt --all -- --check
cargo check --all-targets
cargo test --all-targets
cargo clippy --all-targets --all-features -- -D warnings
```

Then perform targeted runtime validation covering at least:

```text
workspace initialization
→ create/edit file
→ snapshot creation
→ process restart
→ snapshot lookup by edit_id
→ exact byte comparison
→ rollback
→ post-rollback verification
→ provenance query
→ audit linkage
```

Also validate failure paths:

```text
snapshot write failure
corrupt snapshot
missing snapshot content
stale edit state
unauthorized restore
symlink/path escape
multi-file failure
rollback after external modification
```

For MCP-facing behavior, perform real MCP client validation where the existing repository infrastructure supports it.

Do not claim completion solely because compilation succeeds.

---

## 28. Definition of Done

AWE-012 is complete only when all of the following are true:

- [ ] A dedicated file snapshot abstraction exists.
- [ ] It is clearly separate from `context::snapshot`.
- [ ] Exact pre-edit bytes are durably captured.
- [ ] Snapshot records contain integrity-verifiable metadata.
- [ ] Snapshot IDs are stable, validated, and restart-safe.
- [ ] Snapshots are linked to `EditId`.
- [ ] Multi-file transactions preserve transaction-level linkage.
- [ ] Persistent storage lives under workspace `.agent` state.
- [ ] Persistence is atomic and concurrency-safe.
- [ ] Corruption fails closed.
- [ ] Resource limits prevent unsafe unbounded storage/memory usage.
- [ ] Provenance contains path, operation, agent, session, workspace, timestamp, before/after state, and snapshot reference as applicable.
- [ ] Provenance is queryable by edit ID.
- [ ] Normal audit/log output never contains raw snapshot bytes.
- [ ] EditService creates the required snapshot before destructive mutation.
- [ ] AWE-006 recovery uses the canonical snapshot boundary rather than a duplicate rollback implementation.
- [ ] AWE-010 can resolve the exact snapshot required for edit-level rollback.
- [ ] AWE-007 conflict semantics remain intact.
- [ ] AWE-008 verified after-state semantics remain intact.
- [ ] AWE-011 identity/capability/policy semantics remain intact.
- [ ] MCP, CLI, TUI, and Control API share the same snapshot/provenance service.
- [ ] Real filesystem tests pass.
- [ ] Restart persistence tests pass.
- [ ] Byte-identical restoration tests pass.
- [ ] Multi-file tests pass.
- [ ] Corruption/failure-injection tests pass.
- [ ] Security/path/symlink tests pass.
- [ ] Unicode/CRLF/LF/EOF edge-case tests pass.
- [ ] Full Rust CI verification passes.
- [ ] No unrelated architecture or feature scope was introduced.

---

## 29. Final implementation report

At completion, report:

1. the canonical snapshot/provenance architecture chosen
2. exact files changed and why
3. how snapshots are stored under `.agent`
4. how exact bytes and integrity are protected
5. how `EditId` linkage works
6. how AWE-006 and AWE-010 consume snapshots
7. how AWE-011 identity/policy context is preserved
8. how AWE-007 and AWE-008 semantics remain intact
9. how corruption and partial writes are handled
10. how multi-file transactions are represented
11. what tests were added
12. exact verification commands/results
13. any limitations that remain

Do not claim functionality that was not actually implemented and validated.

---

## HARD STOP

This issue is **AWE-012 only**.

Do not implement AWE-013 or later roadmap work while resolving this issue.

Do not modify unrelated files merely to make the architecture appear complete.

Do not create speculative abstractions for future phases.

Do not replace existing context snapshots.

Do not bypass the canonical `EditService`.

Do not create a second identity, policy, audit, or rollback system.

Complete the AWE-012 implementation, tests, verification, and necessary documentation, then **STOP** and wait for the next explicitly authorized issue.
