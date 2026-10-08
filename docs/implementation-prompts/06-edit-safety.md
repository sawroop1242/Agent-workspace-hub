# Prompt 06 — Edit Safety: Atomicity, Conflicts, Verification & Recovery Boundaries (AWE-006..008)

## Mission

Harden the canonical AWH edit execution path so mutation occurs only after the strongest state checks the current architecture can provide, commits use the existing atomic filesystem primitive, successful writes are verified against actual filesystem state, and failures leave the workspace in a precisely reported recoverable state.

This prompt owns the **safety envelope around edit execution**. It does not replace the canonical edit transaction model, reimplement the operation engine, or become the durable snapshot/explicit rollback product.

The current rust branch already contains substantial safety behavior in \`src/services/edit.rs\`, \`src/services/files.rs\`, and \`src/services/snapshot.rs\`. Treat those as existing contracts to inspect and harden rather than recreating them.

This prompt is standalone and must be executable against the current repository without requiring another prompt, branch, PR, or prescribed implementation order.

---

# 1. Product and ownership boundary

AWH owns workspace/filesystem state, controlled edits, expected-state/conflict detection, snapshots/rollback/provenance/audit at their dedicated boundaries, capability/policy enforcement, and shared MCP/CLI/TUI/API services.

External agents own reasoning, planning, model/provider selection, and agent-specific orchestration.

Prompt 06 owns:

- final safety validation around mutation;
- stale-state conflict enforcement;
- prepare-before-commit safety;
- atomic single-file commit through the canonical filesystem primitive;
- bounded handling of partial multi-file commits;
- actual post-commit verification;
- safe recovery of failed edits using existing recovery contracts;
- deterministic safety/error reporting;
- safety-focused tests.

Prompt 06 does not own:

- a second edit model;
- a second edit executor;
- agent identity/session;
- MCP routing/authentication;
- capability/policy evaluation;
- durable snapshot storage;
- snapshot retention;
- general rollback history;
- persistent audit;
- Git reset/revert;
- distributed locking;
- model/agent orchestration.

---

# 2. Required repository forensics

Before changing code, inspect the current repository.

### Project and roadmap

Read:

- \`docs/implementation-prompts/README.md\`;
- \`docs/roadmap/GROWTH_STRATEGY.md\`;
- \`docs/roadmap/PROJECT_ROADMAP.md\`;
- \`docs/roadmap/ROADMAP_AGENT_PROFILES_POLICY_MCP.md\`;
- \`docs/FEATURES.md\`;
- \`docs/PROJECT_CONTEXT.md\`;
- \`docs/architecture.md\`;
- \`docs/security.md\`;
- \`docs/threat-model.md\`.

### Implementation prompts

Inspect current copies of:

- Prompt 01;
- Prompt 02;
- Prompt 03;
- Prompt 04;
- Prompt 05;
- Prompt 07;
- Prompt 08;
- Prompt 09;
- Prompt 10;
- Prompt 11;
- Prompt 14;
- Prompt 16.

Use them to determine ownership and avoid duplicate mechanisms. Do not require their PRs to be merged.

### Historical material

If still present, inspect:

- \`docs/trust-wedge/\`;
- \`docs/issue-resolving-prompts/\`.

Search for:

\`\`\`text
AWE-006
AWE-007
AWE-008
atomic
conflict
verification
rollback
recovery
TOCTOU
write_atomic
expected state
post-edit verification
\`\`\`

Historical material is forensic evidence only. The current rust branch is authoritative.

### Source

Inspect:

- \`src/services/edit.rs\`;
- \`src/services/files.rs\`;
- \`src/services/mod.rs\`;
- \`src/services/snapshot.rs\`;
- path/containment helpers;
- lock/coordination helpers;
- authorization interfaces;
- error types;
- all edit/filesystem/snapshot tests.

Search for:

\`\`\`text
EditService
EditTransaction
EditStatus
PatchResult
PatchFailure
RollbackRecord
RollbackResult
EditRollbackStatus
write_atomic
FileState
ExpectedState
ContextConflictReason
sha256_hex
verify
snapshot
rollback
StoreLock
\`\`\`

Classify each result as canonical implementation, adapter, test, legacy, duplicate, or unrelated.

---

# 3. Baseline

Before implementation:

1. Record \`git status --short\`.
2. Record branch and HEAD.
3. Inspect recent commits affecting edit/files/snapshot code.
4. Run \`cargo metadata --no-deps\`.
5. Run focused edit tests.
6. Run broader verification if the baseline is expected to pass.
7. Record the actual mutation path.
8. Record whether preparation precedes every multi-file mutation.
9. Record where expected state is checked.
10. Record what post-write verification currently proves.
11. Record how current recovery behaves.
12. Record known TOCTOU limitations.

Do not infer safety from type names. Read the actual execution code.

---

# 4. Safety invariants

The implementation must establish and test these invariants.

## Invariant A — No mutation before final validation

No target is changed before required:

- transaction validation;
- path/containment validation;
- expected-state validation;
- contextual matching;
- operation preparation;
- snapshot/recovery preconditions required by the current contract.

## Invariant B — No stale overwrite

If expected state no longer matches live state, the edit is rejected before mutation.

An old request must never overwrite newer content merely because its operation can still technically be applied.

## Invariant C — Prepare before commit

For multi-operation/multi-file edits:

\`\`\`text
prepare all
    ↓
commit
\`\`\`

not:

\`\`\`text
write A
    ↓
discover B cannot be prepared
\`\`\`

Preparation failure must cause zero mutation.

## Invariant D — Canonical atomic commit

Use the existing FilesService atomic-write primitive for full-file replacement.

Do not create an edit-specific atomic writer.

## Invariant E — Verify actual filesystem state

A successful write call is not sufficient.

After commit, observe actual filesystem state and verify the intended result.

## Invariant F — Failure state is explicit

Distinguish validation, conflict, preparation, commit, verification, and recovery outcomes.

## Invariant G — Recovery never destroys an external change

If a target changed after this transaction produced its content, automatic recovery must not overwrite that newer state.

## Invariant H — Atomicity claims are precise

Per-file atomic replacement, multi-file transaction atomicity, and crash durability are different properties. Document them separately.

## Invariant I — Exact bytes

Do not accidentally normalize newline endings, Unicode, trailing whitespace, or unrelated bytes.

---

# 5. Canonical safety pipeline

Use one safety pipeline around the Prompt 05 operation engine:

\`\`\`text
EditTransaction
      ↓
shape validation
      ↓
authorization boundary
      ↓
path/resource validation
      ↓
live-state read
      ↓
expected-state/context validation
      ↓
operation preparation
      ↓
required snapshot/recovery capture
      ↓
canonical atomic commit
      ↓
post-commit verification
      ↓
Committed
\`\`\`

Failure paths:

\`\`\`text
pre-mutation failure
    → Conflict / ValidationFailed / Rejected

commit failure
    → ApplyFailed
    → recovery when mutation may have occurred

verification failure
    → VerificationFailed
    → recovery when safe

safe recovery
    → RolledBack

recovery conflict/failure
    → explicit residual-risk result
\`\`\`

Reuse existing lifecycle/result types. Do not create another state machine.

---

# 6. AWE-006 — Atomic and recovery-safe application

## 6.1 Explicit mutation boundary

Identify the exact point where prepared bytes become filesystem state.

The preferred single-file flow is:

\`\`\`text
read
→ validate
→ prepare
→ required recovery capture
→ write_atomic
→ re-read
→ verify
\`\`\`

Make this boundary clear in code.

Do not scatter filesystem writes through individual operation handlers.

## 6.2 Reuse FilesService::write_atomic

Inspect and reuse the canonical \`FilesService::write_atomic\`.

Verify its actual behavior:

- containment;
- temporary file location;
- complete staging;
- flush/sync;
- atomic rename/persist;
- temporary cleanup;
- existing-target replacement;
- directory durability behavior;
- platform limitations.

If a defect exists, fix it at the FilesService boundary. Do not implement a second writer inside EditService.

## 6.3 Single-file atomicity

For a normal full-file replacement, readers should observe either the previous complete content or the new complete content, not an intentionally truncated intermediate write.

Test the actual contract where practical.

Do not equate rename atomicity with crash durability.

## 6.4 Multi-file atomicity

Explicitly distinguish:

1. preparation atomicity — preparation failure causes zero changes;
2. per-file commit atomicity — each replacement is atomic;
3. transaction atomicity — all files change or none do;
4. crash durability — committed state survives process/system failure.

Only claim guarantees the implementation actually provides.

---

# 7. AWE-006 — Multi-file commit safety

## 7.1 Prepare all targets

Before the first write:

- resolve all targets;
- read required live state;
- validate expected state;
- resolve contextual matches;
- prepare every resulting byte buffer;
- enforce file-size limits;
- capture required recovery material.

Only then may commit begin.

## 7.2 Deterministic commit order

When multiple paths are committed:

- use deterministic ordering;
- never rely on hash-map iteration;
- document path ordering;
- preserve operation order independently.

## 7.3 Commit failure

If one file commits and a later file fails:

- report the transaction as partially mutated;
- identify committed paths;
- stop further unsafe commits;
- invoke existing safe recovery when appropriate;
- verify recovery;
- return the actual residual state.

Never report a clean failure as though nothing changed.

## 7.4 Recovery guard

For each already-mutated path, recovery may replace/delete the target only when the current state still matches the transaction-produced state, or when an existing stronger synchronization contract proves safety.

Otherwise:

\`\`\`text
current state differs
→ do not overwrite
→ recovery conflict
\`\`\`

This is mandatory protection against destroying an external edit.

---

# 8. AWE-007 — Stale-state conflict detection

## 8.1 Final live-state check

Expected state must be checked against live content as close to mutation as the current architecture permits.

Use the canonical:

- SHA-256 hash;
- byte size;
- line count;
- location-sensitive context.

Reuse \`FileState::check\` and the existing context-resolution behavior.

Do not create another matcher.

## 8.2 Hash mismatch

A hash mismatch is a conflict and must cause zero mutation.

## 8.3 Size and line-count mismatch

A mismatch is stale state and must prevent mutation.

Do not proceed merely because an old substring remains present.

## 8.4 Context

Distinguish:

- missing context;
- ambiguous context;
- context not anchored to the selected operation;
- successful context match.

A global substring match is not enough.

## 8.5 Multi-operation expected-state behavior

Preserve the current cardinality contract.

If the transaction has one expected state per operation, enforce that contract.

Do not silently broadcast a single expected state unless that is already canonical.

---

# 9. AWE-007 — TOCTOU

Explicitly handle the race:

\`\`\`text
validate
  ↓
external writer changes target
  ↓
commit
\`\`\`

A previous read cannot prove the file remains unchanged.

Where existing architecture permits:

- minimize the validation-to-write interval;
- perform final expected-state validation immediately before mutation;
- use existing coordination/locking primitives;
- guard recovery with produced-state comparison;
- test concurrent modification.

Do not create a new distributed lock service.

If a final-component symlink swap or other race remains possible, document it. Stronger filesystem coordination belongs to Prompt 14.

Never claim “TOCTOU-free” without actual synchronization evidence.

---

# 10. AWE-008 — Post-edit verification

## 10.1 Commit requires verification

A transaction must not become \`Committed\` merely because a write returned success.

After commit, read the actual filesystem state and verify:

- existence;
- exact content hash;
- byte size;
- line count;
- operation-specific result;
- deletion/creation semantics where relevant.

## 10.2 Verify filesystem, not only memory

Insufficient:

\`\`\`text
prepared = transform(old)
assert prepared == expected
return success
\`\`\`

Required:

\`\`\`text
prepare
→ commit
→ read actual bytes
→ calculate actual FileState
→ compare expected result
\`\`\`

## 10.3 Exact-byte verification

Hash actual bytes without newline or Unicode normalization.

Test:

- LF;
- CRLF;
- no final newline;
- Unicode;
- Devanagari;
- emoji;
- empty files;
- multiline edits.

## 10.4 Verification failure

On mismatch:

1. return a verification failure, never success;
2. determine whether recovery is safe;
3. recover only under the produced-state guard;
4. verify recovery;
5. report any residual state.

If safe recovery is impossible, leave the external state untouched and report the precise residual condition.

---

# 11. Recovery semantics

Prompt 06 may harden existing recovery behavior but must not create the durable rollback product.

Inspect existing:

- \`RollbackRecord\`;
- \`RollbackResult\`;
- \`RollbackOutcome\`;
- \`EditRollbackStatus\`.

## Existing-file target

Restore exact pre-edit bytes only when the current target still matches the transaction-produced post-edit state.

## Newly-created target

Delete it only when its current state still matches the transaction-produced state.

## Externally changed target

Do not overwrite it. Return a structured recovery conflict.

## Recovery verification

After recovery, re-read every affected path and prove:

- exact original hash/bytes for restored files;
- absence for files that should be deleted;
- unchanged state for files never mutated.

Only report full restoration when every affected path is actually restored or was never changed.

---

# 12. Snapshot boundary

The current SnapshotStore owns durable:

- exact pre-edit bytes;
- snapshot manifests;
- snapshot IDs;
- provenance linkage;
- integrity verification.

Prompt 06 must reuse that boundary when the edit contract requires a snapshot before mutation.

If required snapshot capture fails:

\`\`\`text
do not mutate
→ structured pre-mutation failure
\`\`\`

Do not continue with an unsafe edit.

Do not create:

- a second SnapshotStore;
- a new snapshot directory;
- snapshot retention;
- snapshot schema;
- snapshot listing/deletion/restore product.

Those belong to the snapshot milestone.

---

# 13. Authorization boundary

Prompt 06 does not implement authorization.

Do not:

- inspect capability files directly;
- infer permission from agent names;
- infer permission from MCP routes;
- bypass PolicyEngine;
- treat expected-state validity as authority.

If an unsafe bypass exists, document it or add a regression test for the owning authorization boundary rather than embedding a second policy engine here.

---

# 14. Path/filesystem security

Reuse the canonical FilesService containment boundary.

Verify that edit safety preserves:

- workspace-relative paths;
- absolute-path rejection;
- traversal rejection;
- symlink behavior;
- file-size limits;
- safe temporary-file placement.

Do not create a second path resolver.

Path validation is not authorization.

Atomic write is not proof against every filesystem race.

---

# 15. Lifecycle and error discipline

Reuse the existing \`EditStatus\`, \`PatchStatus\`, \`PatchFailure\`, and rollback result contracts where applicable.

The implementation must distinguish:

- validation failure;
- stale conflict;
- preparation failure;
- commit failure;
- verification failure;
- successful recovery;
- recovery conflict;
- recovery failure.

Important semantics:

- \`Conflict\`: no mutation occurred;
- \`ApplyFailed\`: application/commit failed, possibly after partial mutation;
- \`VerificationFailed\`: committed state did not satisfy the intended result;
- \`RolledBack\`: recovery actually restored the pre-edit state;
- \`Committed\`: actual filesystem state was verified.

Do not change lifecycle semantics for convenience.

Errors must not leak:

- complete file contents;
- secrets;
- authorization tokens;
- environment secrets;
- unnecessary absolute host paths.

For multi-file failures report transaction ID, phase, logical affected path, committed paths, recovery outcome, and whether full restoration was proven.

---

# 16. Required failure truth table

| Phase | Condition | Mutation allowed? | Required interpretation |
|---|---|---:|---|
| Validation | malformed request/path | No | ValidationFailed |
| Expected state | hash/size/line mismatch | No | Conflict |
| Context | missing/ambiguous/not anchored | No | Conflict |
| Preparation | operation cannot prepare | No | Preparation/apply failure |
| Required snapshot | capture unavailable | No | pre-mutation failure |
| Commit | write fails before replacement | No target change expected | ApplyFailed |
| Commit | later file fails after earlier commit | Partial | ApplyFailed + recovery result |
| Verification | resulting state incorrect | Already mutated | VerificationFailed + recovery |
| Recovery | produced-state still matches | Yes | RolledBack if fully restored |
| Recovery | produced-state changed | No overwrite | Recovery conflict |
| Recovery | restore itself fails | Residual possible | Recovery failure |
| Verification | exact state confirmed | N/A | Committed |

Use the repository's precise structured result type when it provides more detail than this table.

---

# 17. Test strategy

Use real temporary workspaces and actual filesystem operations.

## 17.1 Atomic write tests

Cover:

- successful replacement;
- complete target after commit;
- temp cleanup on failure;
- size-limit rejection;
- containment;
- existing-target replacement;
- creation where supported;
- failure before rename leaves the previous target intact.

## 17.2 Conflict tests

Cover:

- stale hash;
- size mismatch;
- line-count mismatch;
- missing context;
- ambiguous context;
- context not anchored;
- external modification;
- zero mutation after conflict.

## 17.3 Multi-file safety

Cover:

1. all preparation succeeds and all commits verify;
2. first preparation fails → zero mutation;
3. middle preparation fails → zero mutation;
4. final preparation fails → zero mutation;
5. first commit fails → no false success;
6. later commit fails → committed paths reported;
7. recovery restores all safely recoverable paths;
8. externally changed recovery target becomes conflict and is not overwritten;
9. recovery failure is surfaced;
10. recovery result is itself verified.

## 17.4 Verification

Cover:

- incorrect resulting hash;
- incorrect size;
- incorrect line count;
- unexpected missing target;
- unexpected existing target after delete;
- exact expected bytes;
- LF/CRLF;
- no final newline;
- Unicode/Devanagari/emoji.

## 17.5 Fault injection

If existing filesystem seams permit fault injection, test:

- temp-file creation;
- write;
- flush;
- fsync;
- rename/persist;
- post-write read;
- verification;
- recovery write.

Do not add a heavyweight mocking dependency unless existing seams are insufficient and the repository's architecture justifies it.

## 17.6 Recovery

Cover:

- existing-file restoration;
- deletion of newly created files;
- never-mutated files;
- produced-state match → restore;
- produced-state mismatch → conflict/no overwrite;
- restore failure;
- full-restoration calculation;
- recovery verification.

---

# 18. Concurrency tests

Where supported by the current architecture, simulate:

\`\`\`text
validator
    +
external writer
    ↓
edit commit
\`\`\`

Prove actual behavior:

- stale modification before final validation is rejected;
- external modification before recovery is never overwritten;
- any remaining race is documented.

Do not write tests that assume a lock exists when no lock exists.

---

# 19. Security/adversarial tests

Reject and prove no unsafe mutation for:

- absolute paths;
- traversal;
- control characters;
- symlink escape;
- malformed hashes;
- empty required matches;
- invalid occurrence;
- invalid line range;
- malformed diff;
- hunk context mismatch;
- stale expected state;
- ambiguous context;
- missing targets;
- oversized content;
- required snapshot failure;
- corrupted recovery data;
- post-edit tampering.

Also test malicious diff paths and partial-commit recovery.

---

# 20. Observability without leakage

Where repository conventions permit safety reporting/logging, include:

- edit ID;
- logical workspace-relative path;
- operation index;
- phase;
- status;
- hashes;
- byte counts;
- recovery outcome.

Do not include:

- full file contents;
- secrets;
- bearer tokens;
- environment secrets;
- complete untrusted diff payloads;
- unnecessary host paths.

Bound/sanitize untrusted error text.

---

# 21. Backward compatibility

Before changing:

- EditStatus;
- PatchStatus;
- PatchFailure;
- RollbackResult;
- RollbackOutcome;
- EditError;
- serialized JSON;
- snapshot references;

search callers, fixtures, tests, MCP/CLI/API adapters, and persisted examples.

Preserve existing serde names/tags where possible.

For a required compatibility change, document:

1. old representation;
2. new representation;
3. reason;
4. migration impact;
5. affected callers/tests.

Do not perform speculative schema cleanup.

---

# 22. Duplicate-mechanism audit

After implementation search again for:

\`\`\`text
write_atomic
FileState::check
ExpectedState
verify
RollbackRecord
RollbackResult
SnapshotStore
StoreLock
\`\`\`

The desired ownership is:

\`\`\`text
FilesService
    → containment + canonical atomic filesystem primitive

EditService
    → edit safety orchestration

SnapshotStore
    → durable snapshot/provenance

Authorization service
    → permission decision

Audit service
    → persistent audit/security events

Filesystem coordination
    → stronger TOCTOU/locking primitives
\`\`\`

There must not be a second owner for the same mechanism.

---

# 23. Linear implementation procedure

Follow this order.

## Step 1 — Baseline

Record actual repository state, tests, mutation flow, guarantees, and limitations.

## Step 2 — Ownership map

Identify EditService, FilesService, snapshot, authorization, audit, and coordination boundaries.

## Step 3 — Safety contract

Write down actual guarantees for single-file atomicity, multi-file preparation, multi-file commit, conflict detection, verification, recovery, and TOCTOU.

## Step 4 — Final validation

Ensure no stale/conflicting transaction reaches mutation.

## Step 5 — Preparation

Ensure every target is prepared before the first multi-file mutation.

## Step 6 — Atomic commit

Reuse FilesService::write_atomic and fix defects at its owning boundary if necessary.

## Step 7 — Partial commit handling

If later commit failure is possible, record committed paths and invoke safe existing recovery.

## Step 8 — Recovery guard

Refuse to overwrite any target whose current state no longer matches the transaction-produced state.

## Step 9 — Post-edit verification

Read actual filesystem bytes and compare exact resulting state.

## Step 10 — Lifecycle/results

Make statuses and recovery results reflect actual mutation.

## Step 11 — Fault/concurrency tests

Exercise commit, verification, recovery, and race boundaries using existing test seams.

## Step 12 — Security audit

Exercise traversal, symlink, stale-state, malformed input, and recovery-conflict cases.

## Step 13 — Duplicate audit

Confirm no second atomic writer, matcher, verifier, or recovery mechanism exists.

## Step 14 — Verification

Run all repository gates.

## Step 15 — Final scope audit

Inspect status, diff stat, diff check, and full diff. Remove unrelated changes.

---

# 24. Explicit non-goals

Prompt 06 must not implement:

### Edit model

No new EditTransaction, EditOperation, EditStatus, ExpectedState, or FileState model.

### Operation engine

No second Replace, Insert, DeleteRange, Patch, or unified-diff implementation.

Prompt 05 owns operation semantics.

### Authorization

No PolicyEngine, capability store, identity redesign, or trust routing.

### Snapshot subsystem

No SnapshotStore, snapshot schema, retention, or snapshot CLI.

### Rollback product

No generic undo/redo, rollback history product, Git reset/revert, or second rollback engine.

Prompt 09 owns explicit rollback/recovery product behavior.

### Audit

No persistent audit redesign.

### MCP/CLI/TUI/API

No public interface redesign.

### Filesystem coordination

No new distributed lock service or stronger safe-open architecture.

Prompt 14 owns broader filesystem coordination/TOCTOU infrastructure.

If a missing capability is discovered, document it or integrate through its existing boundary rather than silently expanding scope.

---

# 25. Verification gates

Run, where supported:

    cargo fmt --all -- --check
    cargo check --all-targets
    cargo test --all-targets
    cargo clippy --all-targets --all-features -- -D warnings
    git diff --check

Also run focused tests for:

- atomic writes;
- stale-state conflicts;
- contextual conflicts;
- multi-file preparation;
- partial commit;
- post-edit verification;
- recovery;
- recovery conflicts;
- concurrency;
- path/symlink security.

Report exact results.

If a gate cannot be run:

    NOT VERIFIED — <exact reason>

Never convert unavailable verification into a success claim.

---

# 26. Completion criteria

Prompt 06 is complete only when:

- mutation has an explicit safety boundary;
- required final validation occurs before mutation;
- stale state fails closed;
- contextual conflicts fail closed;
- all multi-file targets prepare before commit;
- canonical atomic filesystem primitives are reused;
- single-file atomicity claims match actual behavior;
- multi-file atomicity claims are honest;
- partial commit is explicitly represented;
- post-edit verification reads actual filesystem state;
- verification checks exact resulting bytes/state;
- verification failure cannot become success;
- recovery never overwrites an external post-edit change;
- recovery is itself verified;
- lifecycle/status reflects actual state;
- errors preserve phase/residual-state information without leakage;
- TOCTOU limitations are documented;
- adversarial tests cover path, stale-state, verification, and recovery;
- no duplicate safety mechanism was created;
- snapshot/authorization/audit/coordination boundaries remain separate;
- verification gates pass or unavailable gates are explicitly marked NOT VERIFIED;
- no unrelated changes remain.

Do not claim:

\`\`\`text
edit safety complete = snapshot system complete
edit safety complete = rollback product complete
edit safety complete = authorization complete
edit safety complete = TOCTOU-free filesystem complete
\`\`\`

These are separate capabilities.

---

# 27. Independence rule

This prompt must work against the current repository regardless of whether any other prompt is merged.

It must not require:

- Prompt 01;
- Prompt 02;
- Prompt 03;
- Prompt 04;
- Prompt 05;
- Prompt 07;
- Prompt 08;
- Prompt 09;
- Prompt 14;
- any other prompt PR;
- any prescribed implementation order.

If a referenced contract exists, inspect and reuse it.

If a referenced subsystem does not exist, implement only the minimum local compatibility boundary required by this prompt.

Never instruct the implementer to wait for another prompt or branch.

The current repository is the source of truth.

---

# 28. Final implementation report

The implementer must report:

## Baseline

- branch;
- starting commit;
- working-tree state;
- existing safety guarantees;
- known limitations.

## Mutation boundary

- validation;
- preparation;
- snapshot/recovery;
- atomic commit;
- post-commit verification.

## Conflict behavior

- hash;
- size;
- line count;
- context;
- concurrency/TOCTOU.

## Atomicity

- single-file guarantee;
- multi-file preparation guarantee;
- multi-file commit guarantee;
- crash-durability limitation.

## Recovery

- recovery trigger;
- produced-state guard;
- restored/deleted/unchanged/conflict/failed outcomes;
- verification evidence;
- residual state if not fully restored.

## Tests

- focused tests;
- integration tests;
- fault-injection tests, if available;
- concurrency tests;
- adversarial/security tests.

## Verification

Exact results for:

- \`cargo fmt --all -- --check\`;
- \`cargo check --all-targets\`;
- \`cargo test --all-targets\`;
- \`cargo clippy --all-targets --all-features -- -D warnings\`;
- \`git diff --check\`.

Mark unavailable gates as NOT VERIFIED.

## Changed files

List every changed file.

## Limitations

Explicitly state residual:

- TOCTOU;
- multi-file atomicity;
- crash durability;
- filesystem/platform;
- recovery;
- verification limitations.

## Scope confirmation

Explicitly confirm that no second:

- edit model;
- operation engine;
- atomic-write primitive;
- expected-state matcher;
- verifier;
- authorization engine;
- snapshot store;
- rollback product;
- audit subsystem;
- distributed lock system

was created.

The final report must describe what the implementation actually proves, not what later milestones are expected to provide.
