# TW-005 — Durable File Snapshots + Provenance

## Master implementation prompt

Implement one durable file-recovery snapshot/provenance subsystem as a standalone Trust-Wedge issue. It must be independently executable against the current `rust` branch.

### 1. Repository forensics

Inspect:

- `src/services/snapshot.rs`;
- any existing context/agent snapshot implementation;
- EditService and edit models;
- workspace/persistence/storage abstractions;
- locking/atomic-write utilities;
- authorization and audit integration points;
- current tests and security documentation.

Determine whether an equivalent file-snapshot abstraction already exists. Extend it rather than creating a duplicate.

**File-recovery snapshots and context-engine snapshots are different concepts.** Never silently use one as the other.

### 2. Snapshot contract

A file snapshot represents recoverable pre-edit state.

For each snapshot, preserve using the repository's canonical types:

- stable Snapshot ID;
- exact prior file bytes;
- content hash;
- required file metadata needed for safe restoration;
- workspace/path identity;
- originating Edit ID;
- caller identity (agent/session) where applicable;
- creation timestamp;
- retention/status information as required.

Exact bytes are authoritative for recovery. Hashes provide integrity/conflict information and must not be treated as a replacement for the bytes.

### 3. Provenance contract

Provenance should connect:

`Agent/Session -> Workspace -> Edit -> Snapshot -> resulting state`

Record, where applicable:

- edit ID;
- agent ID;
- session ID;
- workspace ID;
- target path;
- operation;
- timestamp;
- before hash;
- after hash;
- snapshot ID;
- result/error/conflict information.

Use existing identity and event types. Do not invent duplicate IDs.

### 4. Durable storage and integrity

Use workspace-local durable storage consistent with the repository's persistence model.

Requirements:

- restart-safe persistence;
- atomic/transactional writes where supported;
- locking appropriate to concurrent access;
- corruption detection;
- deterministic lookup by snapshot/edit identity;
- explicit retention/cleanup semantics;
- no silent replacement of corrupt authoritative state with a new empty snapshot store.

Define behavior for missing, malformed, truncated, duplicate, or partially written snapshot records.

### 5. Security and data minimization

Snapshot contents are sensitive workspace data.

Therefore:

- normal logs must never contain file contents;
- errors should not dump snapshot bytes;
- audit records should reference IDs/hashes rather than copying file contents;
- access to snapshots must respect workspace/caller authorization;
- snapshot storage must remain inside the intended AWH-owned persistence boundary;
- avoid unnecessary duplication of secrets.

Do not expose a new unrestricted snapshot endpoint.

### 6. Snapshot lifecycle

Define and test the lifecycle:

1. validate the edit context;
2. capture exact pre-edit state;
3. calculate/store integrity metadata;
4. durably persist the snapshot;
5. link it to the edit/provenance record;
6. make it available to recovery;
7. retain/delete only according to explicit policy.

A snapshot must not be reported as durable if persistence failed.

Failed edits must not create misleading “successful edit” provenance.

### 7. Recovery compatibility

The snapshot representation must support exact restoration by the existing/future rollback service without embedding rollback algorithms into the snapshot store.

Preserve enough state to distinguish:

- no snapshot;
- valid snapshot;
- corrupt snapshot;
- stale/conflicting current file;
- successfully recovered file.

Do not silently overwrite a newer external state.

### 8. Tests

Use real temporary workspaces and durable storage.

Cover:

- exact byte capture;
- hash correctness;
- metadata capture;
- edit linkage;
- caller identity linkage;
- provenance round trip;
- restart persistence;
- corruption detection;
- missing snapshot;
- partial/truncated persistence;
- exact recovery bytes;
- sensitive-content redaction from logs/errors/audit;
- failed edit/no-false-success behavior;
- concurrent snapshot creation where applicable.

Where possible, reopen storage in a new process/instance rather than only testing an in-memory object.

### 9. Security gates

Before completion verify:

- snapshot access is authorized;
- exact bytes are never emitted to ordinary logs;
- corrupt state fails closed;
- persistence is not silently replaced;
- context snapshots remain separate;
- no second snapshot store exists;
- snapshot IDs cannot be used as authorization.

### 10. Definition of done

Complete only when:

- one canonical durable file-snapshot subsystem is wired into the real edit/provenance path;
- exact recovery state survives restart;
- corruption and missing-state behavior are explicit;
- sensitive data is not leaked;
- tests prove actual bytes and persistence behavior;
- repository checks pass;
- documentation reflects the implementation.

Final report must list changed files, snapshot format/ownership decisions, persistence behavior, tests, commands/results, and remaining limitations.

### Non-goals

Do not implement a second context-snapshot system, full rollback orchestration, remote execution, external agent infrastructure, or unrelated persistence refactoring.