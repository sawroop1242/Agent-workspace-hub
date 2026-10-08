# Prompt 08 — Durable File Snapshots, Provenance & Recovery Capture (AWE-012 / TW-005)

## Mission

Implement and harden the single canonical AWH file-snapshot and edit-provenance boundary that captures exact pre-mutation state before a consequential edit, persists it durably, validates it before recovery, and correlates it with the existing edit, agent/session, workspace, and later rollback/audit lifecycle.

The central invariant is:

> A recovery snapshot is a durable, integrity-verified representation of the exact filesystem state required to recover an authorized edit; provenance explains the relationship between that snapshot and the edit, but neither snapshot existence nor provenance grants authorization.

This prompt is standalone. The current rust branch is the source of truth. Do not require another implementation-prompt PR or merge order.

---

## 1. Product boundary

AWH owns controlled workspace mutation and the recovery evidence surrounding it.

External agents own reasoning, planning, model/provider selection, and agent orchestration.

The canonical lifecycle is:

    trusted caller
        ↓
    authorization
        ↓
    edit preparation / expected-state validation
        ↓
    snapshot + recovery capture
        ↓
    atomic mutation
        ↓
    post-edit verification
        ↓
    provenance / outcome
        ↓
    rollback when required

Snapshot capture is a recovery boundary, not an authorization boundary.
Provenance is an explanation/correlation record, not an authorization source.

---

## 2. Scope

### In scope

Implement or harden one authoritative subsystem for:

1. exact pre-edit file-state capture;
2. durable snapshot storage;
3. integrity verification;
4. snapshot identifiers and schema/versioning;
5. workspace-relative path validation;
6. existing-file versus newly-created-file semantics;
7. multi-file snapshot consistency;
8. atomic snapshot publication;
9. restart-safe reads;
10. corruption/tamper detection;
11. provenance records linking workspace, agent, session, edit transaction, snapshot, affected resources, before/after state, and final outcome;
12. recovery-read APIs used by the existing rollback/recovery boundary;
13. deterministic snapshot lifecycle;
14. resource limits and bounded storage;
15. snapshot-related failure semantics;
16. concurrency and crash/failure tests;
17. integration with the existing edit lifecycle.

### Explicitly out of scope

Do not implement or redesign:

- AgentProfile;
- AgentRegistry;
- AgentSession;
- capability grants;
- PolicyStore;
- MCP authentication/trust;
- edit authorization;
- edit operation semantics;
- edit execution;
- rollback execution;
- generic event/audit storage;
- whole-workspace backup;
- Git commits/reset/revert;
- model routing;
- LLM orchestration;
- distributed storage;
- cloud object storage;
- arbitrary filesystem versioning.

If an existing prerequisite is missing, use its current public contract or report the gap. Do not silently create a competing subsystem.

---

## 3. Forensic-first procedure

Before changing code, inspect the current repository rather than implementing from this prompt's assumptions.

### Product and roadmap

Read the current versions of:

- docs/roadmap/GROWTH_STRATEGY.md
- docs/roadmap/PROJECT_ROADMAP.md
- docs/roadmap/ROADMAP_AGENT_PROFILES_POLICY_MCP.md
- docs/FEATURES.md
- docs/PROJECT_CONTEXT.md
- docs/architecture.md
- docs/security.md
- docs/threat-model.md

### Implementation prompts

Inspect enough of docs/implementation-prompts/ to establish ownership and avoid duplication, especially:

- 01-init-runtime-contracts.md
- 02-agent-runtime-identity.md
- 03-mcp-routing-and-security.md
- 04-edit-transaction-model.md
- 05-edit-operation-engine.md
- 06-edit-safety.md
- 07-edit-authorization.md
- 09-rollback-recovery.md
- 10-persistent-audit.md
- 11-mcp-editing-validation.md
- 16-testing-and-acceptance.md
- 17-editing-contract-status-roadmap.md

Where present, inspect the historical docs/trust-wedge/ and docs/issue-resolving-prompts/ collections. Search them for AWE-012, TW-005, snapshot, provenance, recovery capture, pre-edit state, durability, corruption, retention, rollback linkage, and atomic snapshot requirements. Treat historical documents as requirements evidence only. The current Rust implementation wins when they conflict.

### Current source

Inspect at minimum, as applicable:

- src/services/snapshot.rs;
- src/services/edit.rs;
- src/services/files.rs;
- src/services/authorization.rs;
- src/services/mod.rs;
- src/core/errors.rs;
- snapshot/context-memory modules;
- persistence/storage helpers;
- StoreLock;
- audit/provenance code;
- rollback/recovery code if already present;
- tests for snapshots, edits, storage, and recovery.

Trace the complete call path from an edit request to snapshot creation and from recovery/rollback to snapshot reads.

---

## 4. Current repository contract to preserve

The current Rust branch already contains a substantial SnapshotStore in src/services/snapshot.rs.

Its existing storage layout is:

    .agent/snapshots/<snapshot_id>.json
    .agent/snapshot-contents/<entry_id>.bin
    .agent/provenance/<edit_id>.json

The existing contract includes concepts equivalent to:

- SnapshotId;
- SnapshotEntryId;
- FileSnapshot;
- FileSnapshotEntry;
- ProvenanceRecord;
- ProvenanceOutcome;
- SnapshotError;
- SnapshotStore;
- schema versioning;
- content-length and SHA-256 integrity;
- atomic tempfile/fsync/rename writes;
- StoreLock;
- bounded snapshot size/file count;
- recovery-view access.

Do not create a second snapshot store.
Do not rename stable public types merely for stylistic reasons.
First audit what is already implemented, then fill correctness/coverage gaps.

---

## 5. Ownership map

The final implementation must preserve this ownership boundary:

| Concern | Canonical owner | Snapshot responsibility |
|---|---|---|
| Agent identity | identity/runtime subsystem | record trusted identifiers |
| Authorization | EditAuthorizer | decide whether mutation is allowed |
| Edit model | EditService/edit domain | identify transaction/operations |
| Edit safety | EditService/FilesService | validate live state and mutation safety |
| Snapshot/recovery capture | SnapshotStore | persist exact pre-edit bytes/state |
| Rollback execution | rollback boundary | restore using validated snapshot |
| Audit | audit subsystem | record security/event history |
| MCP trust | MCP security/dispatcher | authenticate/trust transport |
| Provenance | SnapshotStore or existing canonical provenance owner | correlate edit + snapshot + outcome |

If current code assigns one responsibility differently, preserve the actual canonical owner and document it.

---

## 6. Snapshot invariants

The following are mandatory.

### 6.1 Exact bytes

A snapshot of an existing file must preserve the exact bytes that existed before mutation.

Do not reconstruct bytes through lossy text conversion.

Preserve LF, CRLF, mixed newline bytes, no-final-newline files, Unicode, Devanagari, emoji, empty files, and arbitrary valid bytes when the edit boundary permits them.

If the edit engine is text-only, snapshot storage must still not silently alter the bytes it receives.

### 6.2 Existing versus newly-created files

For each affected resource, record whether it existed before the edit.

    existed_before = true
        → recovery restores exact previous bytes

    existed_before = false
        → recovery must remove the file only if the rollback boundary
          confirms that the current file is the file produced by this edit

Snapshot storage must never encode missing as an empty file. An existing zero-byte file and a missing file are distinct states.

### 6.3 Workspace-relative paths

Every snapshot entry must use the canonical workspace-relative path representation.

Reject absolute paths, traversal, empty/ambiguous paths, invalid separators, workspace escapes, and detected symlink escape.

Reuse the canonical filesystem/path validation boundary; do not invent independent normalization semantics.

---

## 7. Snapshot lifecycle

Define and enforce one lifecycle:

    Requested
       ↓
    Prepared
       ↓
    Captured
       ↓
    Persisted
       ↓
    IntegrityVerified
       ↓
    AvailableForRecovery

Failure at any pre-publication stage must not expose a snapshot that appears valid but is incomplete.

A partially-written artifact must be unpublished, rejected by readers, safely replaceable/cleanable, and never treated as a valid recovery source.

The snapshot becomes recovery-valid only after all required content blobs exist, exact lengths are known, hashes match, the manifest is complete and valid, the manifest is atomically published, and the resulting artifact can be read and verified.

---

## 8. Atomic snapshot publication

Snapshot creation is itself a multi-artifact transaction.

Distinguish content-write atomicity, manifest publication, snapshot visibility, and crash durability.

Use the existing StoreLock/atomic write mechanism. Do not introduce distributed locking or claim local fsync/rename provides distributed durability.

Document what happens if the process crashes before blob publication, after some blobs, after all blobs but before manifest publication, after manifest publication, or during provenance publication.

A valid published snapshot must remain readable after process restart.

---

## 9. Integrity model

Every stored content blob must be bound to the manifest by stable entry ID, exact byte length, and SHA-256 hash.

Every manifest must validate snapshot ID, edit ID, schema version, entry count, entry IDs, paths, existence semantics, hash format, byte sizes, content lengths, referenced content presence, and content hashes.

Unknown schema versions, malformed identifiers, corrupt/truncated content, and integrity mismatches must fail closed. Never silently repair or reinterpret corrupted recovery material.

---

## 10. Tamper resistance and path safety

Snapshot IDs and entry IDs become filenames. Reject separators, traversal, control characters, and excessive length.

Snapshot lookup must never accept arbitrary filesystem paths. Use validated ID → fixed .agent subdirectory → deterministic filename.

Manifests must not reference content outside the fixed snapshot-content directory.

---

## 11. Multi-file snapshots

A multi-file edit must capture the complete pre-edit set before mutation.

Required sequence:

    enumerate affected resources
          ↓
    validate all resources
          ↓
    read all required pre-edit bytes
          ↓
    validate size/resource limits
          ↓
    write all snapshot blobs
          ↓
    verify all blobs
          ↓
    publish one manifest
          ↓
    allow mutation

If any required file cannot be captured, a recovery-required mutation must not proceed.

Do not capture only the first file and do not treat partial capture as a complete recovery snapshot.

---

## 12. Snapshot limits

Preserve existing safety limits such as maximum files, maximum content bytes per file, maximum total content, maximum ID length, and maximum provenance size.

Validate sizes before allocation/persistence, reject oversized input, avoid unbounded memory growth, and never silently truncate recovery data.

If limits need adjustment, justify and test the change from the actual repository contract. Do not remove limits for convenience.

---

## 13. Snapshot creation contract

The canonical creation API should consume already-validated transaction context and exact pre-edit bytes/state.

It must not authorize, edit, rollback, decide policy, infer identity from routes, or silently read unrelated workspace files.

For every entry, persist enough information to answer: path, existence, exact bytes, byte length, SHA-256, edit association, snapshot association, and capture time.

Line count may be recorded for conflict/diagnostic purposes, but never substitutes for exact bytes/hash.

---

## 14. Provenance model

Provenance must explain:

    Workspace
       ↕
    Agent / Session
       ↕
    EditTransaction
       ↕
    Snapshot
       ↕
    Affected resources
       ↕
    Post-edit outcome

Preserve the existing ProvenanceRecord fields where present: edit_id, snapshot_id, operations, paths, agent_id, session_id, workspace_id, hash_edges, outcome, created_at.

Provenance must not contain full file contents, API keys, bearer tokens, environment secrets, model credentials, arbitrary request bodies, or unnecessary absolute host paths. Prefer hashes and stable identifiers.

---

## 15. Provenance lifecycle

Record provenance at the appropriate existing edit boundary and distinguish snapshot captured, edit committed, edit failed, and edit rolled back.

Never record Committed before actual filesystem verification. Never record RolledBack merely because rollback was requested.

Do not use provenance to decide authorization or to establish snapshot integrity.

---

## 16. Snapshot-to-edit binding

A snapshot must be bound to exactly the transaction it was created for.

When the consuming recovery contract requires it, validate:

    snapshot.edit_id == requested edit_id

A recovery request must not substitute another snapshot, edit, workspace, or resource set.

The rollback boundary remains responsible for authorization and restoration safety. The snapshot subsystem proves recovery material identity and integrity.

---

## 17. Agent/session/workspace provenance

When trusted runtime identity is available, record agent ID, session ID, and workspace ID.

These are correlation fields, not authority. Never infer them from URL paths, arbitrary JSON fields, untrusted CLI arguments, or MCP tool names.

If a supported trusted operator flow legitimately has no agent identity, preserve absence according to the existing model rather than fabricating one.

---

## 18. Recovery-read contract

A recovery-safe read must:

1. validate snapshot/edit identifiers;
2. load the manifest;
3. verify schema;
4. verify every referenced content blob;
5. verify exact length;
6. verify SHA-256;
7. validate paths;
8. return exact bytes;
9. fail closed on any integrity problem.

Never return partial/truncated bytes, an unreferenced entry, bytes from another snapshot, or arbitrary files outside the snapshot store.

If one entry is corrupt, the complete recovery snapshot is unusable for an operation that depends on the complete snapshot.

---

## 19. Recovery safety boundary

Snapshot integrity does not itself make restoration safe.

The rollback owner must still authorize rollback and verify live target state before restoration.

The intended relationship is:

    load snapshot
        ↓
    verify snapshot integrity
        ↓
    compare current target against expected post-edit state
        ↓
    authorize rollback
        ↓
    restore exact snapshot bytes
        ↓
    verify restored state

Do not implement that rollback algorithm here.

The snapshot API must provide enough metadata to distinguish original file existed versus did not exist, original hash, and exact original bytes.

---

## 20. Retention and cleanup

Retention must be explicit.

Do not silently delete a snapshot because it is old, already read, or because the workspace restarted.

Before implementing cleanup, establish who owns retention, eligibility for deletion, whether recovery can still reference it, provenance dangling-reference behavior, and cleanup/recovery race behavior.

If the current repository has no complete retention policy, do not invent destructive automatic cleanup in this prompt.

---

## 21. Restart and crash recovery

Use a real temporary workspace to test create → close/drop store → create new store → load → verify exact bytes.

Also test manifest missing, blob missing, blob truncated, blob hash modified, manifest malformed, schema changed, entry ID changed, path changed, edit ID changed, and provenance referencing a missing snapshot.

Every invalid state must fail closed.

If orphaned blobs can remain after a crash, ensure they cannot be interpreted as valid snapshots. Do not build a general garbage collector unless the current contract requires it.

---

## 22. Concurrency

Test concurrent snapshots for different edits, concurrent snapshots for the same workspace, concurrent reads, reads during publication, and provenance writes for different edits.

Required properties:

- no cross-snapshot content mix;
- no manifest points to another snapshot's entry;
- no torn reader-visible snapshot;
- no duplicate committed IDs;
- no lock-induced deadlock.

Do not claim global transaction serialization; edit/filesystem coordination owns mutation races.

---

## 23. Serialization compatibility

Snapshot and provenance records are durable contracts.

Preserve explicit schema versioning, stable serialized field names, and fail-closed handling of unsupported versions. Avoid lossy defaults that turn missing recovery fields into valid but incorrect state.

If migration is required, use an established repository migration convention. Never silently reinterpret an old snapshot as a different filesystem state.

---

## 24. Error contract

Use the existing SnapshotError taxonomy where present. It should distinguish invalid input, not found, integrity corruption, unsupported schema, resource limit, and storage failure.

Errors must not expose full file contents, credentials, secrets, arbitrary host paths, or raw sensitive records.

Client-visible errors must be deterministic and bounded.

---

## 25. Snapshot failure semantics

| Stage | Failure | Required behavior |
|---|---|---|
| input validation | invalid path/ID/size | reject before persistence |
| pre-edit capture | file cannot be read | fail recovery-required edit before mutation |
| blob write | I/O failure | no valid snapshot published |
| blob verification | mismatch | fail closed; no valid snapshot |
| manifest serialization | failure | no valid snapshot |
| manifest publication | failure | no valid snapshot |
| snapshot reload | corruption | snapshot unusable |
| provenance write | failure after edit | do not claim provenance committed; do not falsify edit result |
| recovery read | missing/corrupt entry | recovery fails closed |

Do not roll back a successfully committed edit merely because provenance persistence failed unless the existing edit contract explicitly requires it. Provenance failure and filesystem mutation outcome are separate concerns.

---

## 26. No hidden authorization

The snapshot subsystem must not decide whether an agent may edit or rollback, treat possession of a snapshot ID as authority, or expose arbitrary snapshot bytes through an unauthenticated low-level path.

Caller-facing recovery APIs must remain behind the existing authorized rollback/service boundary.

---

## 27. No hidden filesystem access

Read only explicitly supplied/validated resources required for the snapshot.

Do not recursively copy the workspace, snapshot .git or .agent indiscriminately, follow arbitrary symlinks outside the workspace, collect unrelated secrets/configuration, or include unrelated files.

The snapshot must be minimal sufficient recovery material.

---

## 28. Tests — unit

Add or strengthen focused tests for:

### IDs

- valid IDs;
- generated uniqueness;
- separators;
- traversal;
- control characters;
- excessive length.

### Exact bytes

- empty existing file;
- missing-file semantics;
- LF;
- CRLF;
- no final newline;
- Unicode;
- Devanagari;
- emoji;
- arbitrary bytes if supported.

### Metadata

- exact hash;
- exact byte length;
- existence flag;
- path;
- edit ID;
- schema version.

### Integrity

- valid blob;
- missing blob;
- truncated blob;
- modified blob;
- modified manifest;
- mismatched manifest ID;
- unsupported schema;
- malformed entry.

### Provenance

- round trip;
- snapshot linkage;
- edit linkage;
- agent/session/workspace fields;
- outcome values;
- no file-content leakage.

---

## 29. Tests — integration

Use real temporary directories and actual SnapshotStore/edit service boundaries.

At minimum:

1. capture a file before edit;
2. mutate through the canonical edit path;
3. load the snapshot;
4. verify original bytes;
5. verify provenance linkage;
6. recreate the store;
7. load again;
8. verify identical recovery material.

For multi-file edits, capture all files, verify deterministic entry ordering, verify exact bytes for every file, and verify no unrelated file is captured.

For newly-created files, prove missing-before state is represented as missing rather than empty, and ensure metadata permits safe deletion by the rollback owner.

---

## 30. Tests — fault injection

Use existing test seams where possible. Simulate source read failure, oversized input, content write failure, fsync failure, manifest serialization failure, manifest publication failure, content corruption, manifest corruption, and provenance persistence failure.

Verify no false valid snapshot, no false recovery success, no mutation when a required pre-mutation snapshot cannot be established, and no rewriting of a committed edit merely because provenance storage failed.

Do not weaken the production durability path to make fault injection easier.

---

## 31. Tests — concurrency and crash consistency

Exercise concurrent snapshot creation and reads and, where practical, simulate crash points between content write, verification, and manifest publication.

After restart, valid snapshots must remain valid, incomplete snapshots must fail closed, and no reader may observe a torn published snapshot.

---

## 32. Tests — security/adversarial

Test unsafe snapshot/entry IDs, manifest references to external content, manifest references to another snapshot's content, modified hashes/lengths/edit IDs/paths, unsupported schemas, missing content, oversized content, snapshot access through an unauthorized low-level boundary, and provenance containing secret-like data.

All invalid/corrupt cases must fail closed.

---

## 33. Tests — property/boundary testing

Where practical, test that ID validation never accepts traversal, path normalization never widens the represented resource, serialization round trips preserve valid snapshots, persisted hashes equal original content hashes, any referenced-byte corruption causes integrity failure, an existing empty file remains distinct from missing, and one corrupt entry invalidates a complete recovery snapshot.

Reuse existing property/fuzz infrastructure if available. Do not add a large dependency solely for this prompt without justification.

---

## 34. Provenance and audit separation

Provenance answers which snapshot and edit are related and what the transaction outcome was.

Audit answers what security/event history should be recorded.

Use the existing audit system for persistent security/event records. Provenance may correlate an audit event ID where supported, but must not replace the audit store.

---

## 35. Provenance and authorization separation

Do not use snapshot existence, provenance existence, snapshot ownership, edit ID, or stored session ID as proof of authorization.

Authorization succeeds before a recovery-required mutation reaches the snapshot boundary. Snapshot records trusted identity metadata; it does not validate authority.

---

## 36. Edit lifecycle integration

Integrate with the existing edit lifecycle without creating a second state machine:

    Requested → Authorized → Located → Validated → Snapshotted
              → Applied → Verified → Committed

On snapshot failure:

    Validated → snapshot failure → no mutation

Use the current EditStatus contract. Do not invent a competing lifecycle.

---

## 37. Snapshot/result semantics

A successful snapshot result means all required pre-edit bytes were captured, metadata was persisted, integrity checks passed, the snapshot is reloadable, and recovery material is complete.

A failed snapshot result must never mean “probably captured”, “best effort”, or partial capture presented as complete recovery.

---

## 38. Storage synchronization

Use repository locking/storage conventions. Prevent duplicate ID publication, inconsistent manifests, and lock-order deadlocks.

Document lock ordering if snapshot creation participates in a larger edit transaction.

Do not add a new global workspace lock unless the current architecture proves it necessary.

---

## 39. Performance discipline

Snapshotting is I/O-intensive but must remain bounded.

Avoid unrelated workspace scans, duplicate full-file reads, unnecessary text conversion, repeated hashing, and unbounded buffering.

If the edit path already has exact pre-edit bytes, reuse them rather than rereading, but never weaken correctness to save a read.

---

## 40. Duplicate-mechanism audit

Search for SnapshotStore, snapshot, provenance, checkpoint, backup, pre_edit, recovery_view, snapshot_id, and provenance_id.

Classify each result as canonical file snapshot, context/memory snapshot, edit recovery, rollback, audit, or unrelated backup.

Do not merge context-engine snapshots into file-recovery snapshots merely because both are called snapshots.

There must be one canonical file-recovery snapshot owner.

---

## 41. Implementation sequence

Execute in this linear order:

1. Repository forensics — read roadmap, feature, security, Trust Wedge, historical issue-resolution material, current prompts, and source; trace snapshot and recovery callers.
2. Freeze ownership — confirm the single snapshot owner and consumers.
3. Freeze durable schema — confirm IDs, manifest, content blobs, provenance, and version.
4. Establish exact-state capture — preserve exact bytes and existence semantics.
5. Validate resource boundaries — reuse canonical path/filesystem safety.
6. Enforce limits — validate file count and byte limits.
7. Persist content atomically — use existing primitives.
8. Verify content — length + SHA-256 before manifest publication.
9. Publish manifest atomically.
10. Establish reload safety — corruption/schema/linkage fail closed.
11. Establish recovery reads — exact bytes for existing rollback boundary.
12. Integrate provenance — edit/snapshot/resource/identity/outcome correlation without secrets.
13. Integrate edit lifecycle — required snapshot precedes mutation.
14. Integrate restart behavior.
15. Add fault tests.
16. Add concurrency/security tests.
17. Audit retention — no destructive cleanup without contract.
18. Audit duplicate mechanisms.
19. Run verification.
20. Perform final scope/security audit.

---

## 42. Required snapshot truth table

| State | Snapshot result | Recovery meaning |
|---|---|---|
| existing non-empty file | valid | restore exact bytes |
| existing empty file | valid | restore zero-byte file |
| missing file | valid entry/state | rollback owner may remove edit-created file |
| oversized file | reject | no valid recovery snapshot |
| unreadable source | reject | mutation requiring recovery must not proceed |
| blob truncated | corruption | unusable |
| blob hash changed | corruption | unusable |
| manifest malformed | corruption | unusable |
| unsupported schema | reject | unusable |
| referenced blob missing | corruption | unusable |
| valid snapshot after restart | valid | exact recovery material available |
| provenance missing after successful edit | provenance failure | edit outcome remains separate; do not fabricate provenance |
| multi-file one-entry corruption | corruption | complete snapshot unusable |

---

## 43. Security invariants — must never regress

1. Exact pre-edit bytes are preserved.
2. Existing empty and missing files remain distinct.
3. Snapshot and entry IDs cannot escape fixed storage directories.
4. Snapshot manifests cannot reference arbitrary external files.
5. Corrupt snapshots fail closed.
6. Unknown schemas fail closed.
7. Missing recovery content fails closed.
8. Hash/length mismatches fail closed.
9. A snapshot does not grant authorization.
10. Provenance does not grant authorization.
11. Recovery material cannot bypass the authorized rollback boundary.
12. Required snapshot failure prevents recovery-required mutation.
13. Multi-file snapshots contain every required resource.
14. Readers never accept a torn published snapshot.
15. No unrelated workspace content is silently captured.
16. Secrets and file contents are not embedded in provenance/error output.
17. Retention does not silently destroy eligible recovery material.
18. Snapshot storage does not become a second audit/event system.
19. Snapshot storage does not become a second rollback engine.

---

## 44. Relationship to Prompt 06 — edit safety

Prompt 06 owns final live-state validation, stale-state conflict detection, atomic mutation, post-commit verification, and recovery conflict semantics.

Prompt 08 owns durable pre-edit recovery capture, snapshot integrity, and provenance.

Boundary:

    Prompt 06: Is it still safe to mutate this current filesystem state?
    Prompt 08: Do we have durable, exact recovery material for the state immediately before mutation?

Neither replaces the other.

---

## 45. Relationship to Prompt 07 — authorization

Prompt 07 owns trusted caller authorization, capability evaluation, policy evaluation, and the allow/deny decision.

Prompt 08 must not duplicate these. A snapshot ID is not a capability token.

Boundary:

    authorization → snapshot/recovery capture → mutation

---

## 46. Relationship to Prompt 09 — rollback/recovery

Prompt 09 owns restoration behavior.

Prompt 08 supplies exact original bytes, existence semantics, integrity-verified metadata, and edit/snapshot linkage.

Prompt 09 must not guess what existed before the edit. Prompt 08 must not decide whether restoration is authorized.

---

## 47. Relationship to Prompt 10 — audit

Prompt 10 owns persistent security/event audit. Prompt 08 may persist provenance required to explain edit/snapshot relationships, but must not become a generic audit log.

---

## 48. Independence rule

This prompt must be executable against the current repository without requiring any other implementation-prompt PR, Trust Wedge PR, or issue-resolving-prompts PR.

If an adjacent contract is not yet implemented, integrate against the current available interface or report the missing contract. Do not weaken snapshot safety because an adjacent feature is incomplete.

---

## 49. Verification gates

Run:

    cargo fmt --all -- --check
    cargo check --all-targets
    cargo test --all-targets
    cargo clippy --all-targets --all-features -- -D warnings
    git diff --check

Also run focused tests for snapshot creation/loading, exact bytes, ID/path safety, integrity corruption, schema handling, multi-file capture, restart behavior, provenance, failure injection, concurrent reads/writes, and recovery-view integrity.

Do not weaken tests or lint configuration to obtain green verification.

---

## 50. Completion criteria

Do not declare complete until:

### Architecture

- one canonical file-recovery snapshot owner exists;
- context/memory snapshots remain separate;
- provenance has a clear owner;
- no duplicate snapshot store exists.

### Durability

- content and manifest publication are atomic according to documented local filesystem guarantees;
- valid snapshots survive restart;
- incomplete/corrupt artifacts fail closed.

### Exactness

- exact bytes are preserved;
- hashes and lengths are verified;
- missing and zero-byte states remain distinct;
- multi-file snapshots are complete.

### Security

- IDs and paths are contained;
- manifests cannot reference arbitrary files;
- snapshot existence never grants authorization;
- recovery data cannot bypass the authorized rollback boundary;
- errors/provenance do not leak secrets.

### Lifecycle

- required snapshot capture precedes mutation;
- snapshot failure prevents recovery-required mutation;
- provenance reflects actual outcome;
- committed/rolled-back outcomes are not fabricated.

### Recovery

- the existing rollback boundary can obtain exact, integrity-verified original bytes;
- snapshot/edit linkage is validated;
- no rollback algorithm is duplicated here.

### Testing

- unit, integration, fault, corruption, concurrency, restart, and adversarial tests pass;
- no existing security regression tests are weakened.

### Scope

- no second authorization system;
- no second edit executor;
- no second rollback engine;
- no second audit store;
- no second identity/session system;
- no MCP security redesign;
- no unrelated implementation prompt changed.

---

## 51. Final implementation report

At completion, report:

1. Canonical snapshot owner — module/type/function and why it is the single owner.
2. Snapshot format — manifest, entry metadata, content storage, schema version.
3. Exact-state guarantees — bytes, hash, size, existence semantics, multi-file behavior.
4. Durability — atomic write mechanism, lock behavior, crash boundary, restart behavior.
5. Integrity — corruption detection, schema validation, path/ID validation.
6. Provenance — edit/snapshot linkage, agent/session/workspace correlation, outcome semantics, leakage controls.
7. Recovery integration — exact recovery-read contract, rollback boundary used, limitations.
8. Failure behavior — pre-mutation snapshot failure, corruption, provenance failure, partial multi-file capture.
9. Tests — focused, integration, fault, concurrency, security/adversarial, restart.
10. Verification — fmt, check, test, clippy, diff check.
11. Changed files — exact list.
12. Residual limitations — crash/durability boundaries, retention limitations, platform constraints.
13. Scope confirmation — explicitly confirm no authorization, identity, edit executor, rollback engine, audit store, or MCP security subsystem was duplicated.

The final report must distinguish facts verified by code/tests from assumptions and must not claim stronger durability, atomicity, or recovery guarantees than the implementation establishes.
