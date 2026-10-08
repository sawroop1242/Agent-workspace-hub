# Prompt 09 — Edit-Level Rollback + Recovery (AWE-010 / TW-006)

## Mission

Implement and harden the single canonical AWH edit-level rollback/recovery boundary for a completed edit transaction.

Rollback is a consequential filesystem mutation. It must identify one exact edit transaction, pass through the existing authorization boundary, use canonical durable recovery material, refuse to overwrite newer external changes, restore or remove only resources provably still in the transaction-produced state, and verify the actual filesystem after restoration.

This prompt is standalone. It must be executable against the current rust branch without requiring another prompt, branch, or PR to be merged first.

The current rust branch already contains rollback-related code in src/services/edit.rs, durable file-snapshot/provenance code in src/services/snapshot.rs, canonical filesystem operations in src/services/files.rs, and the authoritative edit/rollback authorization boundary in src/services/authorization.rs. Inspect and harden those contracts. Do not create parallel rollback, snapshot, authorization, path, or audit systems.

---

# 1. Product boundary and ownership

AWH provides the controlled workspace, canonical edit transactions, filesystem safety, authorization, durable file snapshots/provenance, rollback/recovery, and later audit integration.

External agents provide reasoning, planning, model/provider selection, and orchestration. Rollback must never become an agent-owned best-effort file-writing operation.

Prompt 09 owns:

- explicit rollback of one completed edit transaction;
- exact edit eligibility and identity checks;
- linkage between the edit and its recovery snapshot/provenance;
- authorization invocation for rollback;
- conflict detection against the transaction-produced post-edit state;
- safe handling of files that existed before the edit;
- safe removal of files created by the edit;
- deterministic multi-file rollback planning and commit behavior;
- atomic restoration through the canonical filesystem primitive;
- post-rollback verification;
- deterministic rollback lifecycle/result semantics;
- idempotent/repeated rollback behavior;
- crash, race, corruption, and adversarial tests for rollback;
- correlation with provenance/audit without owning either subsystem.

Prompt 09 does not own:

- a second edit transaction model;
- a second edit executor;
- a second SnapshotStore;
- snapshot retention or generic backup management;
- a second authorization or PolicyEngine;
- MCP transport authentication/routing;
- agent identity/session implementation;
- persistent audit architecture;
- Git reset/revert;
- whole-workspace restore;
- generic undo/redo history;
- distributed locking;
- model/agent orchestration.

---

# 2. Required repository forensics

Before implementation, inspect the current repository rather than relying on historical assumptions.

## 2.1 Project and roadmap material

Read:

- docs/implementation-prompts/README.md
- docs/roadmap/GROWTH_STRATEGY.md
- docs/roadmap/PROJECT_ROADMAP.md
- docs/roadmap/ROADMAP_AGENT_PROFILES_POLICY_MCP.md
- docs/FEATURES.md
- docs/PROJECT_CONTEXT.md
- docs/architecture.md
- docs/security.md
- docs/threat-model.md

Use these to establish the product boundary, Trust Wedge sequence, security invariants, CLI/MCP expectations, and current implementation status.

## 2.2 Implementation prompts

Inspect the current copies of:

- Prompt 01 — init/runtime contracts;
- Prompt 02 — agent runtime identity/profile/registry/session;
- Prompt 03 — MCP routing/security;
- Prompt 04 — edit transaction model;
- Prompt 05 — edit operation engine;
- Prompt 06 — edit safety/atomicity/conflicts/verification;
- Prompt 07 — edit authorization;
- Prompt 08 — snapshots/provenance;
- Prompt 09 — this prompt;
- Prompt 10 — persistent audit, when present;
- Prompt 11 — MCP editing/client validation;
- Prompt 12 — CLI editing;
- Prompt 16 — testing/Trust-Wedge acceptance.

Determine which subsystem owns each contract. Do not require any other prompt PR to be merged in order to implement this prompt.

Prompt 06 owns safety around edit execution and recovery of failed edits. Prompt 08 owns durable exact pre-edit snapshot/provenance material. Prompt 09 owns explicit authorized rollback of an already completed edit.

## 2.3 Historical Trust Wedge and issue-resolving material

If the historical directories still exist in repository history or the current tree, inspect relevant material under:

- docs/trust-wedge/
- docs/issue-resolving-prompts/

Search specifically for:

AWE-010, TW-006, rollback, recovery, restore, undo, after_hash, before_bytes, snapshot, provenance, conflict, TOCTOU, authorization, filesystem.rollback, audit.

Historical material is forensic evidence, not authority. If it conflicts with current source or consolidated prompts, the current rust branch wins.

## 2.4 Current source

Inspect at minimum:

- src/services/edit.rs
- src/services/snapshot.rs
- src/services/files.rs
- src/services/authorization.rs
- src/services/mod.rs
- src/core/policy.rs
- agent/capability/session identity modules used by authorization
- audit service
- edit/snapshot/authorization/filesystem tests
- MCP and CLI adapters that already reference rollback

Search for:

EditService, EditTransaction, EditId, EditStatus, EditRefs, RollbackRecord, RollbackResult, RollbackOutcome, EditRollbackStatus, capture_rollback_records, rollback_edits, commit_verified_with_records, SnapshotStore, SnapshotId, SnapshotEntryId, FileSnapshot, FileSnapshotEntry, ProvenanceRecord, ProvenanceOutcome, recovery_view, write_atomic, read_bytes, resolve_checked, EditAuthorizer, EditAction::Rollback, filesystem.rollback, AuditLog.

Classify each hit as canonical implementation, adapter, test, compatibility/legacy, duplicate, or unrelated.

Do not add a second implementation merely because a current function is incomplete.

---

# 3. Current contracts to preserve

## 3.1 Canonical edit identity

EditTransaction owns the stable EditId and carries optional EditIdentity and EditRefs.

Rollback must operate on an exact EditId. Never:

- guess the most recent edit;
- select by display name;
- select by path alone;
- select by timestamp alone;
- select an arbitrary matching snapshot;
- infer an edit from filesystem history.

An unknown edit ID must fail safely and must not mutate the workspace.

## 3.2 Existing rollback domain types

The current edit service contains:

- RollbackRecord;
- RollbackOutcome;
- RollbackResult;
- EditRollbackStatus;
- capture_rollback_records;
- rollback_edits.

Treat these as existing domain vocabulary to inspect and harden.

Current RollbackRecord contains:

- edit_id;
- workspace-relative path;
- existed_before;
- exact before_bytes;
- transaction-produced after_hash.

Do not silently invent another rollback record vocabulary. If durable snapshot/provenance provides stronger recovery material, adapt the existing execution representation to reference canonical material instead of retaining competing storage.

## 3.3 Existing durable file snapshots

The canonical file snapshot boundary is src/services/snapshot.rs.

Current concepts include:

- SnapshotId;
- SnapshotEntryId;
- FileSnapshot;
- FileSnapshotEntry;
- ProvenanceRecord;
- ProvenanceOutcome;
- SnapshotError;
- SnapshotStore;
- durable content blobs;
- manifest integrity;
- SHA-256 and byte-length validation;
- provenance-to-edit linkage;
- recovery_view(edit_id).

Rollback should consume exact recovery material through this boundary where the edit has a durable snapshot. It must not create another snapshot directory or content store.

## 3.4 Existing filesystem boundary

FilesService is the canonical workspace filesystem boundary.

Reuse its:

- workspace-root containment;
- relative-path validation;
- symlink protections;
- size limits;
- read/read_bytes behavior;
- delete behavior;
- atomic write primitive.

Do not write directly to arbitrary PathBuf values from a rollback adapter.

## 3.5 Existing authorization boundary

src/services/authorization.rs is the authoritative transport-independent edit/rollback authorization point.

The existing EditAction::Rollback maps to filesystem.rollback.

Rollback must pass through this boundary with an explicit principal, workspace, resource, and transaction identity.

Authorization denial is a zero-side-effect result.

Do not infer authorization from possession of a snapshot, knowledge of an EditId, route names, agent display names, successful expected-state checks, provenance, previous authorization, or direct filesystem access.

---

# 4. Baseline and implementation discipline

Before changing code:

1. record git status --short;
2. record branch and HEAD;
3. inspect recent commits touching edit/snapshot/files/authorization;
4. run cargo metadata --no-deps;
5. run focused edit/rollback/snapshot/authorization tests;
6. run the broader verification baseline when practical;
7. identify the actual explicit-rollback call path;
8. identify whether it currently uses in-memory RollbackRecord or durable SnapshotStore;
9. identify where authorization is invoked;
10. identify whether current rollback is exposed through MCP/CLI or only service code;
11. identify exact-byte limitations;
12. identify TOCTOU/race windows;
13. identify current lifecycle transitions and result semantics.

Do not declare rollback safe from type names alone. Read the actual mutation path.

---

# 5. Rollback invariants

The implementation must establish and test these invariants.

## Invariant A — Exact target identity

One rollback request identifies exactly one EditId.

Ambiguous, missing, malformed, or unknown IDs fail closed.

## Invariant B — Authorization before mutation

Rollback authorization must succeed before any target is changed.

A valid snapshot is not authority.

## Invariant C — Snapshot/edit binding

Recovery material must belong to the exact requested edit.

Reject a snapshot from another edit, unrelated provenance, caller-supplied arbitrary content, an unbound snapshot ID, or recovery material whose workspace/resource set does not match the edit.

## Invariant D — Current state must match produced state

For every affected path, rollback may mutate only if the current filesystem state still matches the state produced by the target edit.

A newer external change is a conflict, not permission to overwrite.

## Invariant E — Preflight all affected paths

For a multi-file rollback, inspect and validate every target before starting restoration.

A conflict on file N must not occur after files 1..N-1 have already been restored.

## Invariant F — Exact restoration

Existing files are restored byte-for-byte from canonical pre-edit recovery material.

Do not normalize LF/CRLF, final newline, Unicode, Devanagari, emoji, whitespace, or encoding.

## Invariant G — Created-file removal is conditional

If a target did not exist before the edit, rollback may delete it only when the current target still matches the edit-produced state.

Never delete a file merely because its path appears in the transaction.

## Invariant H — No external-change destruction

If a current target differs from the transaction-produced state, leave it untouched and report a structured conflict.

## Invariant I — Verify after mutation

A successful write/delete call is not proof of rollback success.

Observe the actual filesystem state and verify the expected pre-edit state.

## Invariant J — Honest partial outcomes

If rollback cannot complete all affected files, do not report full restoration.

Identify restored, deleted, unchanged, conflicted, and failed paths and whether full restoration was proven.

## Invariant K — Idempotence

A repeated rollback of an already restored edit must not overwrite unrelated newer content.

Return an explicit already-restored/already-rolled-back result where the existing lifecycle supports it, or a deterministic no-op result.

## Invariant L — No hidden security downgrade

Rollback must never bypass workspace containment, path validation, authorization, snapshot integrity, identity/session requirements, or audit correlation.

---

# 6. Canonical rollback lifecycle

Use one transport-independent lifecycle:

request exact edit
→ validate request shape
→ resolve exact edit identity
→ verify edit eligibility
→ resolve canonical snapshot/provenance
→ verify snapshot/provenance integrity
→ authorize filesystem.rollback
→ enumerate exact affected resources
→ validate every path
→ read current state of every target
→ compare current state with produced post-edit state
→ prepare complete restoration plan
→ commit restoration using canonical filesystem primitives
→ post-rollback verification
→ record provenance/audit outcome
→ return authoritative rollback result

No mutation is permitted before full preflight succeeds.

Failure before mutation must have zero filesystem side effects.

If a post-preflight race still occurs, fail closed and report the actual residual state.

---

# 7. AWE-010 — Exact edit eligibility

Rollback must define which edits are eligible.

At minimum an edit must:

- identify a real EditId;
- have completed a mutation successfully or otherwise have valid recovery material under the current lifecycle;
- have exact recoverable pre-edit state;
- have a verified transaction-produced post-edit state;
- have workspace/resource binding;
- not be malformed or corrupted.

Do not roll back:

- unknown IDs;
- edits with no affected resources;
- edits with missing recovery material;
- corrupt snapshots/provenance;
- snapshots bound to another edit;
- snapshots bound to another workspace;
- edits that never committed, unless the current failure-recovery contract explicitly routes them through Prompt 06;
- edits whose recovery material cannot be verified.

The eligibility check must be deterministic and testable.

---

# 8. Exact EditId resolution

The rollback API must accept an exact edit identity.

A service-level shape may be:

rollback(edit_id, authorization context, workspace context)

The implementation may use the existing EditService API or a thin adapter around it, but there must be one canonical service behavior.

Do not introduce rollback_latest, path-based implicit rollback, timestamp-based rollback, fuzzy ID matching, or nearest-snapshot selection.

A history/listing interface may help discover IDs, but the actual rollback operation must consume the exact identifier.

---

# 9. Snapshot/provenance resolution

Prompt 08 owns durable snapshot storage. Prompt 09 consumes it.

## 9.1 Resolve provenance

Use the edit ID to locate the canonical provenance record.

Validate:

- provenance ID/schema;
- edit ID;
- snapshot ID;
- workspace identity;
- agent/session correlation when present;
- outcome/lifecycle compatibility;
- affected resource set.

A provenance record that is readable but does not bind the requested edit must be rejected.

## 9.2 Resolve snapshot

Load the exact referenced snapshot.

Validate:

- snapshot ID;
- schema version;
- edit ID;
- workspace binding;
- entry IDs;
- path set;
- byte lengths;
- SHA-256 hashes;
- content blobs;
- manifest completeness.

Reuse SnapshotStore validation. Do not duplicate integrity logic inside rollback.

## 9.3 Recovery view

Prefer SnapshotStore::recovery_view(edit_id), or the equivalent canonical recovery-read API.

The recovery view must return exact pre-edit bytes only after integrity verification.

If recovery material is corrupt, truncated, missing, mismatched, or ambiguous:

reject rollback
→ zero filesystem mutation

Do not fall back to current file contents, lossy UTF-8 conversion, stale in-memory copies, guessed snapshots, or Git history.

---

# 10. Relationship to existing RollbackRecord

The current edit module already has RollbackRecord with exact before_bytes and after_hash.

Reconcile it with durable snapshot storage rather than leaving two independent recovery authorities.

Required ownership rule:

durable snapshot/provenance = canonical persistent recovery authority
RollbackRecord = edit-service correlation/execution representation

If the executor still needs an in-memory record, construct it from verified canonical snapshot data.

Do not persist a second copy merely for rollback.

If migration/compatibility is needed, make it explicit and fail closed on incomplete records.

A record with an empty or unset after_hash must never authorize restoration.

---

# 11. Pre-edit state semantics

For each affected path preserve the distinction between:

### Existing before edit

existed_before = true

Restore the exact snapshot bytes.

### Missing before edit

existed_before = false

Restore absence by deleting the edit-created target, but only after proving that the current target still represents the edit-produced state.

### Empty existing file

An existing zero-byte file is not equivalent to a missing file.

Explicit tests must cover:

- missing → created → rollback → missing;
- empty existing → modified → rollback → empty;
- non-empty existing → modified → rollback → exact original bytes.

---

# 12. Produced-state conflict model

The produced state is the rollback safety guard.

For every target identify the exact state produced by the edit.

Where the current contract uses after_hash, compare live bytes against that hash. Where stronger FileState data exists, reuse its canonical comparison semantics.

Safety rule:

current state == transaction-produced state
→ restoration may be prepared

current state != transaction-produced state
→ recovery conflict
→ do not mutate target

Conflicts include:

- content changed;
- file replaced;
- file deleted;
- file created where absence was expected;
- resource state no longer satisfies the current safety contract.

Do not treat "some edited text is still present" as sufficient.

---

# 13. Multi-file rollback preflight

For a transaction touching multiple files:

1. obtain the complete affected resource set;
2. reject duplicate/ambiguous resource identities;
3. sort resources deterministically for reporting/commit;
4. validate every path;
5. load every recovery entry;
6. verify every snapshot blob;
7. read every current target;
8. calculate every current state;
9. compare every current state with its produced state;
10. prepare every restoration action;
11. only then begin mutation.

Required invariant:

preflight all
→ any conflict/failure = zero rollback mutation
→ otherwise commit restoration

Do not restore file A and discover a conflict on file B afterward.

---

# 14. Deterministic commit order

When multiple files are restored:

- use deterministic ordering;
- never rely on hash-map iteration;
- document ordering;
- keep reporting deterministic.

Reverse edit/commit order may be appropriate if required by the current transaction model, but do not invent ordering semantics without inspecting the current operation model.

The critical properties are complete preflight and deterministic behavior.

---

# 15. Restoration plan

Build an in-memory restoration plan before mutation.

Conceptually each entry contains:

- edit_id;
- workspace-relative path;
- pre-edit existence;
- pre-edit exact bytes or verified snapshot reference;
- pre-edit hash/length;
- produced post-edit hash/length;
- action = restore or delete.

The plan is an execution-time representation derived from canonical recovery material. It is not a second persistence system.

Do not put full file contents in normal errors, audit entries, or user-facing diagnostics.

---

# 16. Existing-file restoration

For an existing pre-edit file:

1. validate the canonical path;
2. verify current bytes still match the produced post-edit state;
3. obtain exact pre-edit bytes from verified snapshot material;
4. commit through the canonical atomic filesystem primitive;
5. re-read the file;
6. verify exact byte length and SHA-256;
7. mark restored only after verification succeeds.

Do not use lossy UTF-8 conversion for canonical restoration.

If the current edit executor is text-only, rollback still must consume the canonical byte-oriented snapshot material wherever the snapshot contract supports it.

---

# 17. Edit-created file removal

For a target that did not exist before the edit:

1. validate the canonical path;
2. verify the target exists as required by the produced-state contract;
3. calculate current state;
4. compare with transaction-produced state;
5. delete through FilesService;
6. verify absence;
7. report Deleted only after verified absence.

If already absent, determine whether the current lifecycle supports an idempotent AlreadyRolledBack/Unchanged result.

If changed after the edit, return conflict and do not delete.

Never delete a path solely because existed_before is false.

---

# 18. TOCTOU and concurrency

Rollback has the race:

preflight current state
→ external writer changes target
→ rollback write/delete

A read-before-write check is not by itself proof that the target remains unchanged.

The implementation must:

- minimize the validation-to-mutation interval;
- reuse existing StoreLock/coordination primitives where applicable;
- avoid creating a new distributed lock service;
- use canonical atomic writing;
- re-check the produced-state guard as close to mutation as the current architecture permits;
- verify after every mutation;
- treat unexpected changes as recovery conflicts;
- document residual race limitations.

Do not claim "TOCTOU-free" without synchronization evidence.

---

# 19. Multi-file partial rollback

Per-file atomic replacement does not automatically provide multi-file transaction atomicity.

Distinguish:

1. per-file atomic restoration;
2. preflight atomicity;
3. multi-file transaction atomicity;
4. crash durability.

If restoration fails after earlier paths were restored:

- stop further unsafe mutations;
- report restored paths;
- report the failed path;
- re-check affected paths;
- do not claim full rollback;
- expose residual conflict/failure;
- preserve provenance/audit correlation.

Do not claim all-or-nothing multi-file semantics unless the implementation actually provides them.

---

# 20. Post-rollback verification

For every affected path verify one of:

### Existing-before file

exists
AND exact byte length matches snapshot
AND SHA-256 matches snapshot

### Missing-before file

path is absent

### Unaffected path

state was not mutated

A rollback result may be Restored only after actual verification.

---

# 21. Lifecycle/result semantics

Reuse current domain vocabulary rather than creating a second rollback state machine.

Current EditRollbackStatus includes concepts such as:

- Restored;
- AlreadyRolledBack;
- Conflict;
- Failed.

Current RollbackOutcome includes:

- Restored;
- Deleted;
- Unchanged;
- Conflict;
- Failed.

Preserve these semantics where compatible with current source.

Recommended interpretation:

| Condition | Result |
|---|---|
| Unknown edit ID | safe structured failure; no mutation |
| Malformed request | validation failure; no mutation |
| Authorization denied | denial; no mutation |
| Missing/corrupt snapshot | recovery failure; no mutation |
| Snapshot/edit mismatch | recovery conflict/failure; no mutation |
| Any preflight target conflict | conflict; no mutation |
| All targets already restored safely | already-rolled-back/idempotent result |
| All targets restored and verified | restored |
| One or more restorations fail | failed/partial result with exact outcomes |
| External change detected after some restoration | partial result + conflict |
| Verification mismatch | failed result with actual residual state |

Do not silently map all failures to one generic error.

---

# 22. Edit lifecycle integration

The edit transaction lifecycle includes terminal states such as Committed, ApplyFailed, VerificationFailed, and RolledBack.

Explicit rollback of a committed edit must produce a rollback-specific result while correlating with the edit lifecycle.

Do not mark a transaction RolledBack before the filesystem has actually been restored and verified.

If the lifecycle cannot represent explicit post-commit rollback cleanly, extend the existing lifecycle in the smallest compatible way rather than adding another lifecycle enum.

A rollback request is not a rollback success.

---

# 23. Authorization integration

Every explicit rollback must call the existing EditAuthorizer with:

- EditAction::Rollback;
- explicit agent identity where present;
- explicit session identity where required;
- explicit workspace identity;
- affected resource;
- exact transaction/edit ID.

The action must resolve to filesystem.rollback.

Policy denial must take precedence according to the current authorization contract.

Capability/policy infrastructure failure must fail closed.

Do not:

- duplicate policy matching;
- read capability files directly from rollback code;
- infer capability from snapshot ownership;
- allow rollback because the original edit was authorized;
- assume previous authorization remains valid forever.

Rollback is a new consequential mutation and requires the current authorization decision.

---

# 24. Zero-side-effect authorization denial

Before authorization succeeds:

- do not mutate;
- do not delete;
- do not change lifecycle state to success;
- do not emit an allow-side audit event.

If snapshot metadata is needed to determine the authorization resource, load only the minimum required metadata while preserving the same security boundary.

The critical invariant is:

authorization denied → filesystem unchanged

---

# 25. Provenance integration

Prompt 08 owns provenance persistence.

Prompt 09 must correlate rollback outcomes with the existing provenance record.

The relationship should be observable as:

edit
→ original snapshot
→ rollback requested
→ rollback outcome

On successful rollback, conflict, or failure, use the existing provenance contract where supported.

Do not create RollbackProvenance, another provenance directory, independent JSON history, or a new retention mechanism.

Provenance describes what happened. It does not authorize what may happen.

If provenance persistence fails after verified filesystem restoration, report the filesystem outcome and provenance error separately unless the current product contract explicitly makes provenance persistence transactional.

---

# 26. Audit integration

Prompt 10 owns persistent audit.

Prompt 09 must provide structured rollback outcomes and correlation data but must not create a second audit store.

Where the current audit service is available, record the consequential rollback decision/outcome through that existing boundary.

Audit data must not contain:

- full pre-edit bytes;
- full post-edit bytes;
- secrets;
- bearer tokens;
- unnecessary absolute host paths;
- arbitrary raw tool arguments.

Useful correlation fields include edit ID, action = rollback, agent/session identity where permitted, workspace/resource identifier, outcome class, conflict/failure code, and snapshot/provenance ID where safe.

Do not make rollback authorization depend on audit persistence.

---

# 27. Path and symlink safety

Every rollback target must remain workspace-relative.

Reject:

- absolute paths;
- .. traversal;
- root/prefix components;
- ambiguous path encodings;
- paths outside the workspace;
- symlink escapes;
- snapshot entry paths that fail the canonical containment boundary.

Do not trust persisted snapshot paths merely because they were previously accepted. Validate them again at rollback time.

Do not create a rollback-specific path resolver.

---

# 28. Snapshot corruption and tampering

Rollback must fail closed when recovery material is:

- missing;
- truncated;
- malformed;
- schema-incompatible;
- hash-mismatched;
- length-mismatched;
- bound to another edit;
- bound to another workspace;
- bound to another path;
- referenced by malformed IDs;
- incomplete across a multi-file snapshot.

Do not perform best-effort recovery from corrupt snapshot data.

Do not regenerate missing recovery material from current filesystem state.

---

# 29. Restart and crash behavior

Explicit rollback must remain safe across process restart.

Test:

- restart before rollback begins;
- restart after snapshot lookup but before mutation;
- interruption during single-file atomic restoration;
- interruption between multi-file restorations;
- restart after partial rollback;
- retry after partial rollback;
- retry after conflict;
- retry after successful rollback.

After restart, decisions must derive from durable canonical state rather than an in-memory list that may have disappeared.

A crash must never cause a subsequent rollback to assume all files were restored when they were not.

---

# 30. Idempotency and repeated rollback

Repeated rollback is a security property.

### First rollback succeeds

A second request should detect already-restored state, avoid overwriting it, avoid deleting unrelated changes, and return the canonical already-rolled-back/no-op result.

### First rollback conflicts

A second request while the external change remains must continue to report conflict and must never overwrite the external state.

### First rollback partially succeeds

A second request must independently inspect every target and restore only targets that still satisfy the produced-state guard.

Never assume a previous result is current filesystem truth.

---

# 31. Security/error discipline

Rollback errors must be structured and deterministic.

They may expose:

- stable error/outcome category;
- edit ID where safe;
- logical workspace-relative resource;
- snapshot/provenance correlation ID where safe;
- affected phase;
- recovery result.

They must not expose:

- file contents;
- secrets;
- authentication tokens;
- unnecessary absolute paths;
- internal stack traces through user-facing APIs.

For conflicts, distinguish external modification from snapshot corruption without disclosing conflicting file contents.

---

# 32. Performance discipline

Rollback is recovery work, not a reason to scan the entire workspace.

The implementation should:

- inspect only resources belonging to the exact edit;
- avoid loading the same snapshot blob multiple times;
- avoid duplicate hashing when canonical snapshot/read code already verifies it;
- avoid whole-workspace scans;
- avoid duplicate content storage;
- reuse canonical snapshot integrity checks;
- preserve resource limits.

For multi-file edits, loading all required recovery material before mutation is intentional and required for safe preflight.

---

# 33. Testing requirements

Tests must exercise the actual service path and canonical filesystem/snapshot/authorization boundaries.

## 33.1 Eligibility

Test:

- unknown edit ID;
- malformed edit ID;
- empty edit ID;
- committed edit;
- ineligible edit;
- missing recovery material;
- corrupt recovery material;
- mismatched edit/snapshot;
- mismatched workspace;
- mismatched resource set.

Every negative case must prove zero filesystem mutation.

## 33.2 Authorization

Test:

- operator/direct-call behavior under the existing policy;
- authorized agent rollback;
- missing capability;
- expired grant;
- out-of-scope grant;
- policy denial;
- authorization-store failure;
- wrong agent identity;
- wrong session/workspace binding where applicable.

Every denial must prove zero filesystem mutation.

## 33.3 Exact restoration

Cover:

- ASCII;
- UTF-8;
- Devanagari;
- emoji;
- LF;
- CRLF;
- no final newline;
- trailing whitespace;
- empty existing file;
- large-but-allowed file;
- multiple files.

Compare exact bytes, not only parsed text.

## 33.4 Created-file tests

Cover:

- absent before edit;
- created by edit;
- rollback deletes it;
- changed after edit;
- changed created file is not deleted;
- already absent;
- symlink replacement/escape attempt.

## 33.5 Conflict tests

Cover:

- content changed;
- file deleted;
- file replaced;
- same-size different-content change;
- malformed produced-state metadata;
- multi-file conflict discovered during preflight;
- external modification between preflight and commit where deterministically simulatable.

The key assertion is:

conflict → target remains untouched

for every target not already legitimately restored.

## 33.6 Multi-file tests

Cover:

- all files restore successfully;
- deterministic ordering;
- one target conflicts before mutation;
- one target fails during commit;
- partial restoration;
- post-rollback verification;
- repeated rollback after partial result.

Do not assert all-or-nothing semantics unless the implementation actually provides them.

## 33.7 Snapshot integrity

Cover:

- missing blob;
- truncated blob;
- wrong SHA-256;
- wrong byte length;
- malformed manifest;
- unknown schema;
- wrong snapshot ID;
- wrong edit ID;
- wrong path;
- malformed entry ID.

All must fail closed.

## 33.8 Exact-byte regression

Explicitly test that rollback never uses lossy UTF-8 conversion.

If the canonical snapshot/filesystem boundary supports non-UTF-8 bytes, include them.

If edit operations remain text-only, ensure rollback still preserves exact bytes from snapshot material rather than converting through text.

## 33.9 Crash/restart

Use temporary workspaces and controlled fault injection where practical.

Verify after interruption that:

- no success state is fabricated;
- durable snapshot remains valid;
- retry examines actual current state;
- external changes are preserved;
- partial restoration is reported honestly.

## 33.10 Concurrency

Simulate:

- concurrent writer;
- concurrent rollback request;
- repeated rollback;
- snapshot read during rollback;
- lock contention.

Prefer deterministic synchronization hooks over timing-only tests.

## 33.11 Security/adversarial

Include:

- traversal path;
- absolute path;
- symlink escape;
- malicious snapshot path;
- malicious snapshot ID;
- path mismatch between edit and snapshot;
- workspace mismatch;
- malformed serialized records;
- oversized recovery material;
- secret-like values in errors/audit context.

---

# 34. Required rollback truth table

| Stage | Condition | Mutation allowed? | Required result |
|---|---|---:|---|
| Request | malformed request | No | validation failure |
| Identity | unknown edit ID | No | not found/ineligible |
| Eligibility | edit cannot be rolled back | No | ineligible |
| Authorization | denied | No | authorization denial |
| Snapshot | missing/corrupt | No | recovery failure |
| Binding | edit/snapshot mismatch | No | recovery conflict |
| Path | invalid/escape | No | path/security failure |
| Preflight | any current target differs from produced state | No | conflict |
| Preflight | all targets match | Yes, commit may begin | restoration prepared |
| Commit | atomic restore succeeds | Yes | continue verification |
| Commit | delete created file succeeds | Yes | continue verification |
| Commit | one file fails after another succeeds | Partial | partial/failure result |
| Verification | all targets match pre-edit state | Complete | Restored |
| Verification | any target mismatches | Partial/failed | Failed with actual residual state |
| Repeat | all targets already restored | No | AlreadyRolledBack/idempotent result |
| Repeat | external change exists | No | Conflict |

---

# 35. Canonical implementation sequence

Implement in this exact linear sequence.

### Step 1 — Forensics

Read the current project, roadmap, Trust Wedge, historical AWE material, implementation prompts, and source boundaries.

### Step 2 — Freeze ownership

Document which existing type/service owns edit identity, eligibility, authorization, snapshot lookup, snapshot integrity, path containment, filesystem mutation, provenance, and audit.

### Step 3 — Establish one rollback request boundary

Define the smallest transport-independent request needed to identify exact edit ID, principal, workspace, and resource context.

### Step 4 — Validate exact edit identity

Reject malformed, unknown, and ineligible edits before filesystem mutation.

### Step 5 — Resolve canonical recovery material

Resolve provenance and snapshot through SnapshotStore. Do not duplicate storage.

### Step 6 — Validate snapshot integrity and binding

Prove edit/snapshot/workspace/resource relationships before using recovery bytes.

### Step 7 — Authorize rollback

Call EditAuthorizer using EditAction::Rollback. No mutation before Allow.

### Step 8 — Build complete target set

Enumerate all affected resources from canonical transaction/snapshot state. Reject ambiguity and duplicates.

### Step 9 — Validate every path

Reuse canonical FilesService/edit path safety.

### Step 10 — Read every live target

Capture current bytes/state for every affected path before mutation.

### Step 11 — Check produced-state conflicts

Every target must still match the transaction-produced state. Any mismatch aborts rollback before mutation.

### Step 12 — Build restoration plan

Prepare exact restore/delete actions from verified snapshot data.

### Step 13 — Commit restorations

Use canonical atomic filesystem operations and deterministic ordering.

### Step 14 — Verify each restoration

Read actual filesystem state and compare with canonical pre-edit state.

### Step 15 — Resolve lifecycle/result

Return Restored, AlreadyRolledBack, Conflict, or Failed using existing vocabulary.

### Step 16 — Correlate provenance

Record rollback outcome through existing provenance.

### Step 17 — Correlate audit

Emit the structured consequential outcome through existing audit.

### Step 18 — Add fault/concurrency/security tests

Exercise actual mutation path and prove zero-side-effect denial/conflict behavior.

### Step 19 — Restart/recovery verification

Prove safe behavior across interruption and retry.

### Step 20 — Duplicate-mechanism audit

Search for newly introduced rollback stores, path resolvers, policy checks, snapshot stores, undo stacks, or audit logs. Remove duplicates.

### Step 21 — Verification

Run complete project verification gates.

### Step 22 — Final scope/security audit

Confirm rollback cannot overwrite newer edits, delete unrelated files, bypass authorization, consume corrupt snapshots, escape the workspace, or claim restoration without verification.

---

# 36. Explicit non-goals

Do not implement:

- whole-workspace restore;
- Git reset;
- Git revert;
- Git checkout as rollback;
- generic undo/redo;
- arbitrary historical version browsing;
- snapshot retention policy;
- snapshot deletion product;
- second SnapshotStore;
- second authorization engine;
- second policy evaluator;
- second identity/session system;
- MCP routing;
- CLI-specific mutation semantics;
- agent planning;
- distributed transaction coordination;
- distributed locking;
- cloud backup;
- remote disaster recovery.

Rollback is edit-level recovery, not a general backup product.

---

# 37. Prompt 06 boundary

Prompt 06 owns recovery safety for an edit that fails during application or verification.

Prompt 09 owns explicit rollback after an edit reaches a completed/eligible state.

Boundary:

Prompt 06:
failed edit → automatic safety recovery → honest failure outcome

Prompt 09:
completed edit → explicit authorized rollback → conflict-aware restoration → verified rollback outcome

Do not create two competing rollback engines. Share canonical recovery primitives.

---

# 38. Prompt 08 boundary

Prompt 08 owns:

- durable file snapshots;
- exact pre-edit bytes;
- snapshot manifests;
- content blobs;
- integrity verification;
- provenance records;
- recovery-read material.

Prompt 09 consumes those contracts.

Do not reimplement snapshot storage, content blob storage, snapshot hashing, snapshot schema, or provenance persistence.

If a Prompt 08-owned defect is discovered, make the smallest ownership-preserving correction or document the defect for the owning boundary. Do not work around it with a duplicate subsystem.

---

# 39. Prompt 10 boundary

Prompt 10 owns persistent audit.

Prompt 09 provides structured rollback outcomes and correlation data but must not create a second audit history.

Audit failure must not silently convert a verified filesystem restoration into a false filesystem failure unless the current product contract explicitly makes audit persistence transactional.

---

# 40. Relationship to MCP and CLI

Prompt 09 is transport-independent.

If current MCP/CLI surfaces already call rollback, they must route through the same canonical rollback service.

Do not implement one algorithm for MCP and another for CLI/TUI/API.

Future interfaces may expose filesystem.rollback or awh fs rollback, but both must invoke the same service semantics and authorization boundary.

---

# 41. Security invariants checklist

Before completion, prove:

- [ ] exact EditId is required;
- [ ] unknown IDs cannot mutate;
- [ ] rollback authorization is explicit;
- [ ] filesystem.rollback policy is enforced;
- [ ] identity is not inferred from route names;
- [ ] snapshot is bound to the requested edit;
- [ ] snapshot is bound to the correct workspace/resource set;
- [ ] corrupt snapshot data fails closed;
- [ ] malicious snapshot paths are revalidated;
- [ ] all targets are preflighted before first mutation;
- [ ] produced-state mismatch is a conflict;
- [ ] external changes are never overwritten;
- [ ] created files are deleted only under produced-state proof;
- [ ] existing files are restored from exact bytes;
- [ ] no lossy UTF-8 conversion is used for canonical restoration;
- [ ] canonical atomic write is reused;
- [ ] post-rollback filesystem state is verified;
- [ ] partial rollback is reported honestly;
- [ ] repeated rollback is safe;
- [ ] restart/retry is safe;
- [ ] errors do not leak file contents/secrets;
- [ ] audit/provenance remain observational rather than authorization sources;
- [ ] no duplicate rollback/snapshot/policy/path/audit system exists.

---

# 42. Verification gates

Run:

cargo fmt --all -- --check
cargo check --all-targets
cargo test --all-targets
cargo clippy --all-targets --all-features -- -D warnings
git diff --check

Also run focused tests for:

- rollback eligibility;
- authorization;
- snapshot binding/integrity;
- exact bytes;
- missing-vs-empty;
- created-file deletion;
- produced-state conflict;
- multi-file preflight;
- partial rollback;
- post-rollback verification;
- repeated rollback;
- traversal/symlink escape;
- corrupt snapshot;
- restart/fault behavior;
- concurrency.

If repository CI requires additional gates, run those too.

Do not report tests as passing unless they actually ran successfully.

---

# 43. Completion criteria

Prompt 09 is complete only when:

1. one canonical explicit rollback service path exists;
2. rollback accepts an exact edit identity;
3. edit eligibility is deterministic;
4. rollback uses canonical durable snapshot/provenance;
5. snapshot/edit/workspace/resource binding is verified;
6. rollback passes through the existing authorization boundary;
7. all targets are preflighted before mutation;
8. current state must match transaction-produced state;
9. external changes are preserved;
10. existing files are restored byte-for-byte;
11. edit-created files are removed only when safely attributable to the edit;
12. canonical atomic filesystem primitives are reused;
13. post-rollback state is verified;
14. partial outcomes are represented honestly;
15. repeated rollback is safe and deterministic;
16. restart/crash behavior is tested;
17. concurrency/conflict behavior is tested;
18. corruption/tampering cases fail closed;
19. no duplicate snapshot/authorization/path/audit system was introduced;
20. all verification gates pass.

---

# 44. Final implementation report

At completion report concisely:

1. Repository forensics — relevant modules, historical evidence, and authoritative contracts.
2. Rollback boundary — exact EditId resolution, eligibility, and service entry point.
3. Authorization — EditAction::Rollback, filesystem.rollback, principal/workspace/resource handling, denial semantics.
4. Recovery material — SnapshotStore/provenance linkage, integrity checks, exact byte recovery.
5. Conflict safety — produced-state comparison, multi-file preflight, TOCTOU mitigation, external-change preservation.
6. Mutation — canonical atomic restoration, created-file deletion, deterministic ordering.
7. Verification — exact post-rollback checks, lifecycle/result semantics, partial results.
8. Observability — provenance/audit correlation without duplicate stores.
9. Tests — exact bytes, conflicts, multi-file, corruption, authorization, crash/restart, concurrency, security.
10. Verification commands — exact commands run and real outcomes.
11. Scope audit — confirm only rollback/recovery ownership changed and no duplicate subsystem was introduced.

---

# 45. Independence rule

This prompt is standalone.

The implementer must not:

- wait for Prompt 06 to be merged;
- wait for Prompt 08 to be merged;
- wait for Prompt 10 to be merged;
- require another implementation-prompt PR;
- assume another branch contains an unavailable contract;
- modify another implementation prompt to make Prompt 09 executable.

Instead:

1. inspect the current rust branch;
2. reuse contracts that actually exist;
3. adapt only within rollback ownership;
4. keep compatibility with current source;
5. verify complete rollback behavior in the same implementation.

The current rust branch is the source of truth. Historical documents are evidence only.

The finished implementation must leave AWH with one coherent rollback path:

exact edit
→ verified durable recovery material
→ current-state conflict check
→ authorized restoration
→ atomic filesystem mutation
→ actual filesystem verification
→ provenance/audit correlation
→ authoritative rollback result

That is the complete AWE-010 / TW-006 contract.
