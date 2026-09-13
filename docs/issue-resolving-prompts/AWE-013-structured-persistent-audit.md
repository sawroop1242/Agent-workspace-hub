# AWE-013 / #34 — Structured Persistent Audit

## Master Issue-Resolving Prompt

### Mission

Implement the production-grade **structured, persistent, correlated audit subsystem** for AWH.

The objective is to evolve the existing process-local audit ring into a durable security and mutation history while preserving one canonical audit choke point shared by MCP, CLI, TUI, and the Control API.

The final architecture must make it possible to reconstruct the lifecycle of an authorized, denied, failed, committed, or rolled-back edit by correlation identifiers without exposing file contents, credentials, bearer tokens, snapshot bytes, or other secrets in ordinary audit records.

The intended relationship is:

```text
Caller / Agent
    → Session / Workspace identity
    → Capability + Policy decision
    → EditTransaction
    → Snapshot
    → Mutation
    → Verification
    → Commit / Failure / Rollback
    → Structured Audit Event(s)
    → durable append-only audit history
```

The audit subsystem is an **observability, security, accountability, and reconstruction boundary**. It is not the authorization engine, edit engine, snapshot store, or workflow engine.

---

## 1. Authoritative forensic baseline

Before changing implementation code, inspect the current `rust` branch and verify the actual architecture rather than trusting this prompt or historical assumptions.

The current audit implementation in `src/services/audit.rs` is a process-wide bounded ring buffer with a maximum of 1,000 entries. It already has centralized token-like redaction and is shared by the Control API and MCP plane. Its current `AuditEntry` is limited to timestamp, kind, action, subject, and free-form detail, and it is not restart-persistent. fileciteturn144file0

The canonical edit model in `src/services/edit.rs` provides `EditId`, `EditOperation`, `ExpectedState`, `FileState`, and `EditStatus`, including lifecycle states such as `Requested`, `Authorized`, `Snapshotted`, `Applied`, `Verified`, `Committed`, `Rejected`, `Conflict`, `ApplyFailed`, `VerificationFailed`, and `RolledBack`. fileciteturn145file0

AWE-006, AWE-007, AWE-008, AWE-010, AWE-011, and AWE-012 establish the recovery, conflict, verification, rollback, authorization, snapshot, and provenance foundations that this issue must integrate with.

Do not create competing models for:

- edit identity
- agent identity
- session identity
- workspace identity
- policy decisions
- capability grants
- snapshots
- provenance
- rollback state

Reuse canonical services and identifiers already established by those milestones.

If current code differs from this prompt, first determine whether the difference is an intentional newer implementation or an actual gap. Preserve correct existing behavior and extend it coherently.

---

## 2. Core architectural rules

### 2.1 One audit choke point

All security-relevant and edit-lifecycle audit events must pass through one canonical transport-independent audit service.

Do not implement separate audit writers for:

- MCP
- CLI
- TUI
- Control API
- EditService
- SnapshotService
- PolicyEngine
- rollback service

Transport adapters may add request-specific context, but they must submit the resulting event to the shared audit service.

### 2.2 Audit is append-oriented history

The durable audit stream must be append-only from the application's perspective.

Normal callers must not be able to:

- edit an existing audit event
- rewrite an event's timestamp
- replace an event ID
- silently remove an event
- mutate historical correlation fields

Retention/rotation is allowed and must be explicit, bounded, and deterministic.

### 2.3 Ring cache remains useful

The existing in-memory ring should remain as a hot cache for recent events unless the current architecture provides a demonstrably better equivalent.

The durable audit sink and the hot ring have different responsibilities:

```text
Durable store = restart-surviving history
Ring cache     = fast recent-event access
```

Do not make the ring the only source of truth once persistence is implemented.

### 2.4 Audit must not become a security bypass

Audit failures must never grant authorization, skip policy evaluation, bypass expected-state validation, skip verification, or suppress rollback requirements.

Define explicitly whether an unavailable audit sink is:

- fail-closed for specific security-critical operations, or
- fail-open with an explicit degraded mode.

For high-risk edit and authorization events, prefer fail-closed behavior where the security contract requires an authoritative audit trail. The decision must be documented and tested rather than left to incidental error handling.

---

## 3. Canonical structured event contract

Replace the current loose `kind/action/subject/detail`-only lifecycle representation with a structured event model while preserving backward compatibility where existing consumers require it.

At minimum support these fields:

```text
timestamp
event_id
edit_id
agent_id
session_id
workspace_id
path
operation
policy_decision
before_hash
after_hash
snapshot_id
result
duration
```

The exact Rust field names may follow existing project conventions, but the semantic contract must remain equivalent.

### 3.1 Event identity

Every persisted event requires a unique `event_id`.

Requirements:

- collision-resistant generation
- stable serialization
- safe use across process restarts
- deterministic validation when supplied externally
- no secret material embedded in IDs

Do not rely on an in-memory counter alone for globally unique persistent event identity.

### 3.2 Timestamp

Record an authoritative timestamp for every event.

Use a representation that is:

- serializable
- timezone/unambiguous
- suitable for ordering
- robust against system-clock anomalies as far as practical

If duration measurement uses a monotonic clock, do not confuse monotonic duration values with wall-clock timestamps.

### 3.3 Correlation identity

Where applicable, events must reference the canonical:

- `EditId`
- agent identity
- session identity
- workspace identity
- `SnapshotId`

An event that is part of an edit lifecycle must never invent a second edit identifier.

### 3.4 Operation

Represent the actual canonical operation type(s), such as:

- replace
- insert
- delete_range
- patch
- apply_diff
- rollback
- authorization
- verification
- snapshot

Do not encode important machine-readable lifecycle semantics solely inside free-form strings.

### 3.5 Result

The result must distinguish at least:

- started/requested
- authorized
- denied
- conflict
- snapshot failure
- apply failure
- verification failure
- committed
- rolled back
- rollback failure
- audit/storage failure

Use structured result/error categories rather than relying only on human-readable messages.

---

## 4. Required edit lifecycle auditing

The audit service must support enough events to reconstruct a complete edit lifecycle.

At minimum cover:

```text
request/start
authorization decision
validation/conflict result
snapshot creation
mutation/apply
verification
commit
failure
rollback start
rollback completion/failure
```

Do not emit only a final “success” event. A production audit trail must explain where and why an operation stopped.

### Successful edit

A normal successful edit should be reconstructable approximately as:

```text
requested
→ authorized
→ validated
→ snapshotted
→ applied
→ verified
→ committed
```

### Denied edit

A denied operation should be reconstructable as:

```text
requested
→ authorization denied
```

without falsely claiming that a snapshot, mutation, or commit occurred.

### Failed edit

A failed operation must record the actual failure boundary.

For example:

```text
requested
→ authorized
→ snapshotted
→ applied
→ verification_failed
→ rolled_back
```

Do not report `committed` if the transaction did not commit.

### Rollback

Rollback events must correlate to the original edit and snapshot, and must distinguish:

- rollback requested
- rollback authorized
- rollback applied
- rollback verified
- rollback committed
- rollback failed/conflicted

---

## 5. Provenance and snapshot integration

AWE-012 owns persistent file snapshots and provenance storage semantics. AWE-013 owns audit events.

Do not duplicate snapshot bytes or full provenance documents inside every audit event.

Instead use references:

```text
AuditEvent
  ├── edit_id
  ├── snapshot_id
  ├── before_hash
  ├── after_hash
  └── lifecycle/result metadata
```

The audit record should answer **what happened and where to find the recovery/provenance record**, while AWE-012 remains responsible for the underlying snapshot bytes and detailed provenance substrate.

If a snapshot does not exist because authorization or conflict validation failed before snapshotting, `snapshot_id` must remain absent rather than fabricated.

---

## 6. Authorization and AWE-011 integration

Audit authorization decisions using the canonical policy/capability system.

Record enough information to establish:

- who requested the operation
- which session/workspace it belonged to
- what operation was requested
- whether policy/capability evaluation allowed or denied it
- the structured reason/category for denial where safe

Do not persist:

- capability secrets
- API keys
- bearer tokens
- authentication credentials
- private authorization material

An authorization-denied event must be generated through the same audit service as successful authorization events.

Do not allow a caller to disable authorization auditing by selecting a different transport.

---

## 7. Centralized redaction and data classification

Preserve the existing centralized redaction principle: the audit choke point itself must perform defense-in-depth redaction even if callers are expected to provide already-safe data. The current implementation masks long token-like segments centrally. fileciteturn144file0

Extend this into a structured data-classification policy.

### Never store in ordinary audit events

- complete file contents
- snapshot bytes
- secrets
- passwords
- API keys
- bearer tokens
- OAuth tokens
- private keys
- credentials
- large binary payloads
- arbitrary request bodies containing unknown sensitive data

### Generally safe structured fields

- hashes
- sizes
- line counts
- operation names
- stable non-secret identifiers
- policy decision
- result category
- bounded relative paths, subject to path/privacy policy
- durations
- timestamps

### Path privacy

Determine whether absolute paths can reveal sensitive local information. Prefer workspace-relative paths where the canonical architecture permits it.

Do not leak home-directory paths, environment variables, credentials embedded in paths, or unrelated filesystem topology.

### Redaction tests

Test redaction against:

- bearer tokens
- API keys
- token-shaped strings embedded in structured fields
- secrets separated by punctuation
- Unicode text
- long paths
- hashes
- UUID-like identifiers

Do not blindly redact every identifier if doing so destroys required correlation. Define explicit field-level classification instead.

---

## 8. Persistent append-only storage

Implement a durable audit sink under the workspace's canonical `.agent` state or the repository's established persistent-state location.

Do not invent a second unrelated workspace state hierarchy.

The store must:

1. survive process restart
2. preserve event ordering semantics
3. append safely
4. tolerate concurrent writers
5. prevent partial records from being treated as valid events
6. detect corruption
7. fail safely on malformed records
8. enforce bounded retention/rotation
9. avoid unbounded memory/disk growth
10. support efficient recent-event retrieval
11. support correlation by edit/event/session/workspace identifiers where required

### Storage format

Choose a structured format appropriate for append-only event storage.

Possible approaches include newline-delimited JSON or another self-delimiting structured representation. The choice must be justified by:

- atomic append characteristics
- recovery after interruption
- forward compatibility
- streaming/query performance
- corruption detection
- portability

Do not choose a binary/custom format merely for complexity or novelty.

### Atomicity

A partially written event must never be silently interpreted as a complete valid event.

Define behavior for interruption during:

- event serialization
- append
- flush
- rotation
- metadata update

On restart, the implementation must either safely recover a valid prefix or fail/mark the corrupted tail according to a documented deterministic policy.

Never silently rewrite historical events to hide corruption.

---

## 9. Retention and rotation

The durable audit store must be bounded.

Define explicit limits for:

- maximum events
- maximum bytes
- maximum event size
- number/size of rotated segments
- retention age if supported

The policy must be deterministic and testable.

When old events are removed due to retention, do not imply that the complete historical audit trail still exists.

The system should expose enough metadata to identify retention boundaries when practical.

Do not allow one oversized event to bypass the global storage limits.

---

## 10. Concurrency and ordering

Multiple AWH processes or threads may write audit events concurrently.

Define ordering semantics explicitly.

At minimum guarantee:

- each accepted event is persisted at most once per audit submission
- no interleaved/corrupted record bytes
- event IDs remain unique
- ring and durable sink remain internally consistent
- concurrent rotation does not lose committed records silently

Do not claim global causal ordering across independent processes unless the implementation actually provides it.

Use sequence numbers or another explicit ordering mechanism if consumers require deterministic ordering beyond timestamps.

---

## 11. Audit service API

Design a transport-independent API capable of supporting structured lifecycle events.

The API should make invalid audit states difficult to express.

Prefer typed fields/enums for:

- event kind
- operation
- policy decision
- result
- lifecycle stage

Avoid requiring every caller to construct arbitrary JSON or free-form detail strings.

Retain a bounded compatibility path for existing callers if needed, but migrate internal security-sensitive paths toward the structured contract.

The service should expose operations equivalent to:

```text
record(event)
recent(limit)
query/correlate(filter)
flush()
health/status()
```

Only expose query operations that are consistent with the current Control API/MCP security model.

Explicitly define whether `flush()` is automatic, caller-triggered, or both.

---

## 12. MCP and Control API correlation

MCP and Control API must be able to correlate security and mutation activity through the canonical audit service.

A caller should be able to answer questions such as:

```text
What happened to edit X?
Which agent/session initiated edit X?
Which policy decision authorized or denied it?
Which snapshot was created?
What was the before hash?
What was the verified after hash?
Did verification succeed?
Was rollback performed?
Why did the operation fail?
```

Do not expose unrestricted audit history to unauthenticated callers.

Audit queries are themselves security-sensitive and must pass through the canonical capability/policy boundary established by AWE-011.

Avoid allowing an audit-query endpoint to become a side channel for sensitive paths or identifiers.

---

## 13. Failure policy: audit storage unavailable

Define and implement explicit behavior for:

- disk full
- permission denied
- workspace unavailable
- lock acquisition failure
- corrupted audit file
- serialization failure
- rotation failure
- flush failure
- process interruption

The policy must distinguish between:

```text
hot-ring write succeeded
but durable write failed
```

and:

```text
neither durable nor hot audit state was accepted
```

Do not silently claim durable audit success when only memory accepted the event.

For security-critical operations, implement the documented fail-closed/degraded-mode policy consistently.

A failed audit write must not leave the edit transaction reporting `Committed` if the system's contract requires durable audit before commit.

Conversely, do not retroactively mutate filesystem state merely because a post-commit audit append failed unless the architecture explicitly guarantees such compensation.

This boundary must be tested with failure injection.

---

## 14. Transaction ordering and lifecycle correctness

Audit emission must align with actual state transitions.

Do not emit:

```text
committed
```

before the canonical transaction has actually committed.

Do not emit:

```text
verified
```

before AWE-008 verification succeeds.

Do not emit:

```text
snapshotted
```

before AWE-012 confirms durable snapshot persistence.

Do not emit:

```text
authorized
```

without the actual AWE-011 policy decision.

For failed transitions, the audit result must identify the actual failure boundary.

This requirement is especially important because the existing `EditStatus` model already expresses the lifecycle states shared by the edit system. fileciteturn145file0

---

## 15. Security and tamper-resistance boundaries

The audit system is not a cryptographic ledger unless the repository explicitly requires one. Do not overclaim tamper-proofing.

However, the implementation must make ordinary application-level mutation of historical events difficult or impossible through its public API.

At minimum:

- no update API for historical events
- no arbitrary delete API
- append-only semantics
- safe file permissions according to workspace security conventions
- validated storage paths
- symlink escape protection
- workspace containment
- bounded event sizes
- no user-controlled storage filenames
- no path traversal through event IDs or rotation names

If cryptographic chaining, signatures, or external WORM storage are not already part of the architecture, treat them as explicit non-goals rather than silently introducing them.

---

## 16. Corruption and recovery

Implement deterministic recovery behavior for malformed durable audit state.

Test:

- truncated final record
- malformed JSON/record encoding
- invalid enum value
- missing required field
- impossible timestamp
- oversized record
- duplicate event ID
- invalid correlation ID
- checksum/content mismatch if checksums are used
- corrupted rotation metadata

A corrupted historical event must not cause arbitrary code execution or unsafe filesystem mutation.

The audit system should degrade to a safe state and report corruption clearly.

Never interpret corrupted audit data as authorization evidence.

---

## 17. Performance and resource safety

Audit must remain low-overhead enough for frequent agent operations.

Define and enforce bounds for:

- event size
- queue size if asynchronous persistence is used
- disk usage
- memory usage
- query result size
- recent-event limits

If asynchronous persistence is introduced, define:

- queue overflow behavior
- shutdown flushing
- ordering guarantees
- error propagation
- crash-loss window

Do not silently drop security-critical events because an in-memory queue is full.

Do not introduce an unbounded async channel.

---

## 18. Testing strategy

Tests must use real persistence behavior where the requirement concerns persistence. Mock-only tests are insufficient.

### Unit tests

Cover:

- event serialization/deserialization
- event ID generation/validation
- structured enum validation
- redaction
- size limits
- result classification
- correlation fields
- retention calculations

### Persistence tests

Use temporary real filesystem state to verify:

- write
- restart/reload
- append ordering
- multiple events
- rotation
- bounded retention
- malformed records
- partial final record
- duplicate IDs
- disk/storage failure behavior where injectable

### Lifecycle integration tests

Verify complete sequences for:

1. successful edit
2. denied edit
3. stale/conflicting edit
4. snapshot failure
5. apply failure
6. verification failure with rollback
7. successful explicit rollback
8. rollback conflict
9. rollback failure
10. multi-file transaction

For every case, reconstruct the expected lifecycle from durable audit records.

### Restart tests

Run a process, generate events, terminate it, start a new process, reload the store, and verify that durable events remain queryable.

### Redaction tests

Prove that ordinary audit persistence cannot leak:

- bearer tokens
- API keys
- passwords
- credentials
- snapshot bytes
- full source contents

Test both structured fields and compatibility/free-form fields.

### Concurrency tests

Exercise concurrent writers/readers and rotation.

### Failure injection

Inject failures at:

- serialization
- append
- flush
- rotation
- load
- snapshot integration
- policy decision recording
- commit audit recording

Verify that the resulting transaction/audit state matches the documented failure policy.

### Property-oriented invariants

Where practical, establish invariants such as:

```text
no event has duplicate event_id
no persisted event contains forbidden secret classes
no committed edit lacks its required lifecycle correlation
no denied edit claims mutation occurred
no audit record claims verification before verification succeeds
no audit record contains snapshot bytes
retention bounds are never exceeded beyond documented crash/recovery tolerance
```

---

## 19. Backward compatibility

Preserve existing public audit consumers where practical, but do not preserve an unsafe schema merely for convenience.

If `AuditEntry` must change:

- provide an intentional migration path
- preserve existing recent-event behavior where possible
- update serialization/query consumers coherently
- avoid transport-specific compatibility forks

Do not introduce duplicate old/new audit stores.

There must remain one authoritative audit history.

---

## 20. Documentation requirements

Update documentation only when the implementation actually requires it, and keep documentation changes scoped to AWE-013 implementation needs.

Document:

- audit event schema
- lifecycle semantics
- persistence location
- retention policy
- failure policy
- redaction guarantees
- query/correlation semantics
- security boundary
- restart behavior

Do not rewrite unrelated roadmap architecture.

---

## 21. Verification requirements

Before considering AWE-013 complete, run the repository's full validation gates:

```bash
cargo fmt --all -- --check
cargo check --all-targets
cargo test --all-targets
cargo clippy --all-targets --all-features -- -D warnings
```

Also perform explicit real-runtime validation for:

```text
successful edit
→ durable audit
→ process restart
→ audit reload
→ edit correlation
```

and:

```text
denied edit
→ durable denial event
→ no mutation
→ no false snapshot/commit event
```

and:

```text
edit
→ verification failure
→ rollback
→ durable lifecycle reconstruction
```

If MCP or Control API audit queries are part of the existing interface, exercise the real interface rather than only unit-calling the audit service.

---

## 22. Definition of Done

AWE-013 is complete only when all of the following are true:

- [ ] One canonical structured audit service exists.
- [ ] Existing ring-buffer hot-cache behavior remains bounded and useful.
- [ ] Durable audit history survives process restart.
- [ ] Events have stable unique IDs.
- [ ] Events correlate edit, agent, session, workspace, snapshot, and lifecycle data where applicable.
- [ ] Successful edits can be reconstructed end-to-end.
- [ ] Denied edits are recorded without falsely claiming mutation.
- [ ] Failed edits identify their actual failure boundary.
- [ ] Rollback lifecycle is recorded and correlated.
- [ ] Before/after hashes are recorded only from authoritative filesystem state.
- [ ] Snapshot references integrate with AWE-012 without duplicating snapshot bytes.
- [ ] Authorization decisions integrate with AWE-011.
- [ ] Verification semantics integrate with AWE-008.
- [ ] Atomic/recovery semantics integrate with AWE-006.
- [ ] Conflict semantics integrate with AWE-007.
- [ ] No audit event exposes ordinary file contents or secrets.
- [ ] Centralized redaction is tested.
- [ ] Durable storage is append-oriented and bounded.
- [ ] Rotation/retention is deterministic.
- [ ] Corruption fails safely.
- [ ] Concurrent writers cannot corrupt event records.
- [ ] Audit storage failure follows an explicit documented policy.
- [ ] MCP and Control API correlation uses the shared audit service.
- [ ] Audit queries are policy/capability protected.
- [ ] Real restart/persistence tests pass.
- [ ] Failure-injection tests pass.
- [ ] Full Rust CI gates pass.
- [ ] No unrelated architecture or transport-specific audit implementation was introduced.

---

## 23. Explicit non-goals

Do **not** use AWE-013 to implement:

- a generic workflow engine
- a generic event bus
- a separate authorization system
- a second identity system
- a second snapshot system
- a second provenance system
- filesystem sandboxing
- VM/container isolation
- cryptographic WORM storage unless separately required
- arbitrary user-facing analytics dashboards
- unrestricted unauthenticated audit access
- full source-content archival inside audit events

Keep the implementation focused on structured persistent audit for the canonical AWH security and edit lifecycle.

---

## 24. Required final implementation report

At completion, report:

1. files changed
2. canonical audit architecture implemented
3. durable storage format/location
4. event schema
5. lifecycle/correlation semantics
6. redaction/security guarantees
7. retention/rotation behavior
8. audit-unavailable failure policy
9. AWE-006/AWE-007/AWE-008/AWE-010/AWE-011/AWE-012 integration
10. tests executed and results
11. full CI results
12. known limitations or deferred work

Do not claim persistence, correlation, rollback auditing, or redaction guarantees that were not actually verified.

---

## HARD STOP

After AWE-013 is implemented, tested, and verified, **STOP**.

Do not begin AWE-014, CLI audit redesign, broader observability work, dashboards, analytics, or later roadmap items in the same implementation task.

Do not modify unrelated files or architecture merely because they could benefit from the new audit service.

The goal is a coherent, production-grade **structured persistent audit boundary** that becomes the single authoritative audit path for AWH.