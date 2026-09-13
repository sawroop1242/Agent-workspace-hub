# AWE-006 / #27 — Atomic and Rollback-Safe Edits

> **Type:** P0 agent-grade editing foundation  
> **Dependencies:** AWE-001 / #22, AWE-002 / #23, AWE-003 / #24, AWE-004 / #25, AWE-005 / #26  
> **Primary scope:** atomic, verification-aware, rollback-capable execution of the canonical edit transaction  
> **Branch:** `rust`  
> **Status:** Issue-resolution master prompt

## 1. Mission

Implement production-grade **atomic and rollback-safe edit execution** for AWH by extending the canonical AWE-004 transaction pipeline rather than creating a second editing architecture.

The implementation must provide the strongest rollback guarantees that the actual filesystem primitives and tests can prove. It must never describe the result as ACID or crash-consistent unless those properties are explicitly demonstrated.

Required conceptual pipeline:

```text
validate
  → prepare
  → snapshot boundary
  → apply
  → verify
  → commit
```

Required failure path:

```text
apply / verify / later-commit failure
  → detect failure
  → conflict-check rollback targets
  → restore already-mutated files
  → clean temporary artifacts
  → verify restoration
  → report the original transaction failure plus rollback outcome
```

The hard invariant is:

> **A transaction that cannot complete successfully must not leave an unintended partial mutation behind when the implementation still has a valid, conflict-free recovery path.**

If rollback cannot safely proceed because the target changed externally, the system must fail closed and report that recovery is incomplete; it must never overwrite newer external changes merely to make rollback appear successful.

---

## 2. Required preflight

Before changing code, inspect the current `rust` branch and reconcile the implementation against:

- `docs/PROJECT_CONTEXT.md`
- `docs/PROJECT_ROADMAP.md`
- `docs/PROJECT_ROADMAP_STATUS.md`
- `docs/PROJECT_STATUS.md`
- `docs/FEATURES.md`
- `docs/CLI.md`
- `docs/architecture.md`
- `docs/mcp.md`
- `docs/issue-resolving-prompts/AWE-001-canonical-edit-transaction-model.md`
- `docs/issue-resolving-prompts/AWE-002-safe-contextual-replacement.md`
- `docs/issue-resolving-prompts/AWE-003-line-range-insert-delete.md`
- `docs/issue-resolving-prompts/AWE-004-multi-operation-filesystem-patch.md`
- `docs/issue-resolving-prompts/AWE-005-unified-diff-application.md`
- the AWE-006 GitHub issue
- the current edit service and filesystem security/atomic-write helpers

Do not assume the earlier milestones were implemented exactly as their prompts describe. Verify the actual source tree, public APIs, tests, error types, and filesystem primitives before designing AWE-006.

Use the existing architecture as the source of truth when it differs from stale documentation.

---

## 3. Forensic baseline

The AWE-006 issue explicitly establishes the following baseline:

- some filesystem paths already have atomic disk-write primitives;
- there is not yet a semantic transaction executor providing rollback;
- AWE-006 must build on AWE-004;
- temporary-file replacement alone is **not** sufficient transaction safety;
- single-file atomicity is required;
- multi-file transactions must avoid partial completion when recovery is possible;
- a stable transaction/edit ID must survive the complete lifecycle;
- temporary artifacts must be cleaned up;
- verification failure must be able to trigger recovery;
- rollback itself must be conflict-aware;
- process-interruption behavior must be documented and tested where practical;
- AWE-012 owns durable snapshot/provenance work;
- AWE-010 owns explicit user-requested rollback of completed edits.

Do not duplicate those later systems inside AWE-006.

---

## 4. Architectural rule: extend AWE-004, do not fork it

AWE-006 must consume the canonical transaction/preparation model established by AWE-004.

Do **not** create:

- a second `EditTransaction` type;
- a second edit parser/executor hierarchy;
- a second path-security implementation;
- a second atomic-write implementation;
- a second unified-diff executor;
- an unrelated snapshot database;
- a parallel history/rollback store;
- transport-specific rollback logic.

All operation types from AWE-001 through AWE-005 must converge on the same transaction execution boundary.

AWE-006 is about the **commit/recovery semantics of one transaction**, not about adding another mutation API.

---

## 5. Transaction lifecycle

Define and implement a precise lifecycle compatible with the existing `EditStatus` vocabulary.

At minimum, the execution model must distinguish:

1. request received;
2. transaction shape validated;
3. paths/security validated;
4. all affected files loaded;
5. expected state validated;
6. operations prepared in memory;
7. original file states captured;
8. rollback/snapshot boundary established;
9. mutations applied through the canonical atomic filesystem primitive;
10. resulting files verified;
11. transaction committed;
12. or, on failure, rollback initiated;
13. rollback targets conflict-checked;
14. rollback applied;
15. restored state verified;
16. temporary artifacts cleaned;
17. final transaction status/result emitted.

Do not invent lifecycle states merely for cosmetic logging. Add state vocabulary only where it materially improves correctness and diagnostics.

The stable `EditId` must remain attached to every stage and every error/result associated with the transaction.

---

## 6. Preparation must be completely mutation-free

AWE-006 inherits the AWE-004 invariant that all possible validation and preparation work occurs before the first real mutation.

Preparation must:

- validate transaction shape;
- resolve and validate every affected path;
- reject traversal and symlink escapes through the existing security boundary;
- load every affected file;
- capture exact original bytes/content;
- capture `FileState` for every affected file;
- enforce caller-provided expected state;
- compute all operation results in memory;
- validate all operations against the same original transaction view;
- detect invalid cross-operation interactions;
- establish deterministic affected-file ordering;
- establish the recovery data required by the current transaction;
- reject unsupported or unsafe cases before mutation.

A failure during preparation must produce **zero filesystem mutation**.

This must be tested explicitly.

---

## 7. Snapshot boundary without stealing AWE-012

AWE-006 needs enough original state to restore files during the active transaction, but it must not become the durable snapshot/provenance system.

The implementation may maintain transaction-local recovery material such as:

- original bytes;
- original `FileState`;
- target path;
- prepared replacement bytes;
- temporary-file references;
- transaction-local rollback metadata.

This recovery material exists only for the active transaction and its immediate failure recovery.

Do **not** implement:

- persistent snapshot history;
- long-term version storage;
- user-visible edit history;
- provenance databases;
- durable snapshot indexing.

Those belong to AWE-012.

The prompt must make the lifetime and cleanup policy of transaction-local recovery data explicit.

---

## 8. Single-file atomicity

For one affected file, use the existing secure atomic-write primitive rather than inventing a custom replacement mechanism.

The implementation must establish that:

- preparation completes before mutation;
- the final replacement is performed atomically according to the platform primitive's documented guarantee;
- permissions/metadata behavior is understood and not accidentally degraded;
- temporary files are created safely;
- temporary names cannot escape the intended directory/security boundary;
- temporary files are not left behind on success or ordinary failure;
- the target is not truncated before successful replacement;
- write errors leave the original target intact where the primitive guarantees that behavior.

Do not claim that atomic replacement means the entire multi-file transaction is atomic.

That distinction is fundamental.

---

## 9. Multi-file transaction semantics

For transactions affecting multiple files, establish a deterministic sequence and explicit recovery boundary.

The implementation must:

1. prepare every file before the first mutation;
2. capture original state for every affected file;
3. apply prepared mutations in deterministic order;
4. verify each mutation as required;
5. if any later operation fails, identify every file already changed by this transaction;
6. rollback changed files in a safe deterministic order;
7. verify each restored file;
8. report whether the complete transaction was restored.

The implementation must pass the canonical failure scenario:

```text
File A → valid
File B → valid
File C → intentionally fails during apply/verify

Expected result:
A = original bytes
B = original bytes
C = original bytes
```

Also test failures at multiple positions, not only the final file.

A transaction must never skip rollback simply because an earlier file succeeded.

---

## 10. Rollback conflict detection

Rollback is itself a protected mutation.

Never blindly restore a saved version over a file that has changed since AWH last wrote it.

For every rollback target, compare the currently observed state with the state that AWH expects to see before restoring it.

At minimum, the rollback decision must account for:

- content hash;
- file size;
- line count where relevant;
- existence/non-existence state;
- path identity/security;
- transaction ownership of the most recent mutation where such metadata is available.

The essential rule is:

```text
current state == transaction-produced state
    → rollback may proceed

current state != transaction-produced state
    → rollback conflict
    → do not overwrite external changes
```

If rollback conflicts, return a structured rollback-conflict result and preserve the newer external content.

Do not silently force rollback.

---

## 11. Creation, deletion, and replacement cases

The recovery model must handle more than ordinary existing-file replacement.

Explicitly reason about:

- existing file → replaced file;
- nonexistent file → newly created file;
- file → empty file;
- file → deleted target if deletion is supported by the canonical executor;
- multiple operations against one file;
- multiple files with mixed existing/non-existing states.

For newly created files, rollback may require safe deletion, but only when the file still matches the transaction-produced state and no external change has occurred.

For deleted files, rollback may require recreation from the transaction-local original bytes, but only under the same conflict-aware rules.

Do not add arbitrary deletion semantics if the current canonical edit API does not support them; document the exact supported state transitions.

---

## 12. Verification after apply

Every mutation must have an explicit verification boundary.

Verification should establish, as appropriate:

- target exists when expected;
- target absence is correct when expected;
- resulting bytes equal the prepared content;
- resulting SHA-256 matches the expected post-state;
- file size is correct;
- line count is correct;
- all files in the transaction satisfy their intended post-state;
- no unexpected path was modified.

Verification failure is a transaction failure.

It must not be treated as a warning.

A verification failure after one or more successful writes must enter rollback.

---

## 13. Commit semantics

Define precisely what `commit` means.

AWE-006 must not imply that a commit marker makes prior filesystem writes transactional in the database sense.

A successful transaction should mean:

```text
all intended writes completed
+ post-write verification succeeded
+ no rollback is required
+ temporary artifacts were cleaned
+ final state was observed and recorded
```

Only after those conditions should the transaction be marked `Committed`.

If the system cannot establish that condition, it must not claim success.

---

## 14. Failure taxonomy

Use structured errors/results rather than generic strings.

At minimum distinguish:

- preparation failure;
- expected-state conflict;
- path/security failure;
- read failure;
- temporary-artifact creation failure;
- apply/write failure;
- verification failure;
- rollback conflict;
- rollback write failure;
- rollback verification failure;
- cleanup failure;
- process-interruption/recovery uncertainty;
- transaction completed successfully.

Preserve the **original failure** when rollback itself also fails. The final diagnostic must make it possible to determine:

1. what caused the transaction to fail;
2. which files had already changed;
3. which files were successfully restored;
4. which files could not be restored;
5. whether external changes prevented rollback;
6. whether the workspace is known to be fully restored.

Do not hide the primary failure behind a later cleanup or rollback error.

---

## 15. Temporary artifact lifecycle

Every temporary artifact created by the transaction must have a deterministic lifecycle:

```text
create → use → replace/rollback → cleanup
```

Cleanup must occur on:

- success;
- preparation failure after temporary creation;
- apply failure;
- verification failure;
- rollback success;
- rollback conflict;
- rollback failure;
- cancellation where the runtime supports cancellation.

Cleanup must be best-effort only after correctness has been protected. Never delete a recovery artifact before the system no longer needs it.

If cleanup cannot be completed, report it explicitly and do not pretend the transaction was perfectly cleaned up.

---

## 16. Process interruption and crash guarantees

This issue specifically requires honest crash semantics.

Document what happens if the process terminates during:

- preparation;
- temporary-file creation;
- first file replacement;
- middle of a multi-file transaction;
- verification;
- rollback;
- cleanup.

Distinguish clearly between:

- guaranteed behavior of the underlying atomic filesystem primitive;
- best-effort transaction recovery;
- behavior covered by tests;
- behavior not guaranteed after process termination.

Do **not** claim crash consistency, durable transactions, journaling, or ACID semantics unless the repository contains a real mechanism and tests proving those properties.

Where practical, add subprocess/process-interruption tests that leave realistic temporary artifacts or partially completed multi-file transactions and verify the documented recovery behavior.

If automatic restart recovery is not implemented in AWE-006, say so explicitly rather than building an undeclared persistence system.

---

## 17. Concurrency and TOCTOU boundary

AWE-006 must not create a false impression that an in-memory preflight snapshot prevents all concurrent modification races.

Document the race between:

```text
read/validate
        ↓
apply
```

and the stronger race between:

```text
apply
 ↓
verify
```

Use existing filesystem coordination or state checks where available.

At minimum:

- expected-state validation must occur before mutation;
- rollback must validate the transaction-produced state before overwriting;
- external changes must not be silently destroyed;
- the implementation must fail closed when safe ownership cannot be established.

If OS-level file locking is not part of the current architecture, do not invent it merely to claim perfect concurrency safety. Document the limitation and implement the strongest safe state checks supported by the current design.

---

## 18. Encoding and content correctness

Rollback must restore **exact original bytes**, not merely semantically equivalent text.

Tests must cover:

- UTF-8;
- Devanagari/Hindi text;
- emoji and non-BMP Unicode;
- LF line endings;
- CRLF line endings;
- trailing newline;
- no trailing newline;
- empty files;
- large text files where practical.

Do not normalize line endings or Unicode during recovery unless that transformation is explicitly part of the original transaction semantics.

For byte-preserving rollback, use the original bytes captured at the transaction boundary.

---

## 19. Security requirements

Reuse the canonical security boundary from the existing filesystem implementation.

Never bypass it for rollback.

Rollback must validate:

- workspace containment;
- canonical path identity;
- symlink escape protection;
- temporary-file location;
- target identity;
- safe deletion of transaction-created files;
- safe restoration of original files.

A rollback routine is a write capability and therefore must be subject to the same security controls as forward editing.

Fail closed if a rollback target cannot be safely identified.

---

## 20. Transaction-local recovery representation

Design the internal recovery representation so that it is explicit rather than relying on scattered local variables.

A suitable conceptual model is:

```text
TransactionRecovery
├── edit_id
├── affected files
│   ├── path
│   ├── original existence
│   ├── original bytes/state
│   ├── prepared bytes/state
│   └── applied/current expected state
├── temporary artifacts
├── mutation progress
└── rollback progress
```

The exact Rust types are implementation decisions.

Do not expose internal recovery machinery through MCP/CLI/TUI merely to make the implementation convenient.

---

## 21. Deterministic mutation and rollback ordering

The same transaction must produce deterministic behavior independent of hash-map iteration order.

Define stable ordering for:

- affected files;
- operations against the same file;
- mutation sequence;
- rollback sequence;
- error aggregation.

Do not let `HashMap`/`HashSet` iteration order become transaction semantics.

For rollback, reverse application order where that is required by dependency semantics, while preserving safety for independent files.

If the implementation chooses another ordering, document why and test it.

---

## 22. Interaction with AWE-005 unified diff

AWE-005 unified-diff application must use the same transaction safety boundary.

A diff that changes multiple files must receive exactly the same:

- preparation guarantees;
- original-state capture;
- atomic write behavior;
- verification;
- rollback behavior;
- conflict detection;
- cleanup semantics.

Do not create a special “diff rollback” implementation.

The unified diff is an operation/input representation; AWE-006 owns the transaction safety semantics.

---

## 23. Testing requirements

Tests are a core deliverable, not an optional follow-up.

### 23.1 Preparation-failure tests

Prove that invalid transactions leave every affected file byte-for-byte unchanged.

### 23.2 Single-file success

Prove:

```text
original → new
```

with atomic replacement and correct final verification.

### 23.3 Single-file apply failure

Inject a write failure where practical and prove the original file remains intact when the filesystem primitive provides that guarantee.

### 23.4 Multi-file success

Apply a transaction to several files and verify all final bytes and metadata.

### 23.5 Failure in first file

The first mutation fails; prove no unintended file remains changed.

### 23.6 Failure in middle file

Example:

```text
A succeeds
B fails
C was not yet applied
```

Expected:

```text
A restored
B original
C original
```

### 23.7 Failure after all files applied but verification fails

Force a verification failure after writes and prove every changed file is restored.

### 23.8 Rollback conflict

Externally modify a transaction-produced file before rollback and prove AWH refuses to overwrite it.

### 23.9 Rollback verification failure

Inject a failure during restoration and verify the result reports incomplete recovery rather than claiming success.

### 23.10 Newly-created-file rollback

Where supported, create a file as part of the transaction, force later failure, and prove the created file is safely removed only if it still matches the transaction-produced state.

### 23.11 Exact-byte restoration

Compare original and restored bytes, not only parsed text.

### 23.12 Temporary-artifact cleanup

Verify success and failure paths do not leave unexpected temporary artifacts.

### 23.13 Process interruption

Where practical, use a subprocess or controlled interruption to exercise the documented interruption boundary. Do not write a flaky test merely to satisfy a checklist.

### 23.14 Cross-platform filesystem behavior

Keep tests compatible with Linux/macOS/Windows CI. Avoid assuming POSIX-only rename behavior when the project claims cross-platform support.

### 23.15 Property-oriented testing

Where practical, generate small sequences of valid edits and injected failures and assert the invariant:

```text
successful transaction → expected final state
failed transaction + safe rollback → original state
rollback conflict → external state preserved
```

---

## 24. Failure injection design

Do not make production code depend on arbitrary `sleep`, random failures, or environment hacks to test rollback.

Introduce a narrow, testable abstraction if required, such as a filesystem/mutation boundary that can deterministically inject:

- write failure;
- replacement failure;
- verification failure;
- rollback-write failure;
- cleanup failure.

Keep failure injection test-only or explicitly controlled by internal interfaces.

Never expose a production API that allows an external caller to arbitrarily force rollback failures.

---

## 25. Performance and resource discipline

Do not trade correctness for premature optimization.

For AWE-006, transaction-local original bytes are intentionally allowed because exact rollback requires recoverable state.

Nevertheless:

- avoid unnecessary duplicate full-file copies;
- avoid repeated hashing when an existing state can be reused safely;
- avoid loading unrelated files;
- bound any temporary/recovery resource growth where appropriate;
- clean recovery buffers promptly after transaction completion;
- document any maximum file/transaction size if a limit is required.

Do not claim a memory or latency target without measurement.

---

## 26. Public API and transport boundaries

Keep the implementation transport-independent.

AWE-006 must not directly implement MCP handlers, CLI commands, TUI actions, or Control API routes.

Those interfaces should eventually call the canonical edit service.

If a new service/result type is necessary, make it reusable by all transports and document its invariants.

Do not expose transaction-local recovery buffers, temporary paths, or internal filesystem handles as public API state.

---

## 27. Backward compatibility

Preserve existing behavior and public contracts from AWE-001 through AWE-005 unless a correctness defect requires a deliberate change.

If an API or serialization shape must change:

1. identify the compatibility reason;
2. minimize the change;
3. preserve deserialization where practical;
4. add regression tests;
5. document the migration impact.

Do not casually rename canonical edit types or fields.

---

## 28. Explicit non-goals

Do **not** implement the following as part of AWE-006 unless an existing dependency is strictly required to make the transaction safe:

- AWE-010 explicit user-requested edit rollback/history;
- AWE-012 durable snapshots/provenance;
- AWE-013 persistent audit storage;
- AWE-007 stale-state coordination as a separate subsystem;
- AWE-008 standalone post-edit verification architecture;
- AWE-009 MCP agent-grade editing exposure;
- AWE-014 CLI agent-grade editing;
- AWE-015 full editing test-suite milestone beyond tests required to prove AWE-006;
- AWE-016 real MCP-client validation;
- AWE-017 acceptance workflow;
- AWE-018 contract formalization;
- agent profiles or caller identity;
- worktree isolation;
- cloud/remote editing;
- new connector integrations;
- new persistence databases;
- unrelated filesystem refactors.

Do not create parallel stores or APIs that later milestones will have to remove.

---

## 29. Code-quality requirements

Implementation must satisfy the repository's Rust quality gates:

```bash
cargo fmt --all -- --check
cargo check --all-targets
cargo test --all-targets
cargo clippy --all-targets --all-features -- -D warnings
```

Use idiomatic Rust and existing project conventions.

Avoid:

- `unwrap()`/`expect()` on runtime-controlled failure paths;
- hidden filesystem mutations;
- swallowed rollback errors;
- generic stringly-typed error handling where structured errors are appropriate;
- duplicated path-security logic;
- duplicated atomic-write logic;
- unsafe assumptions about platform rename semantics.

Add Rustdoc to new public APIs.

---

## 30. Required implementation audit

Before declaring AWE-006 complete, inspect the final diff specifically for:

- accidental changes outside the AWE-006 implementation scope;
- duplicate transaction executors;
- duplicate atomic-write helpers;
- rollback paths that bypass security validation;
- rollback paths that overwrite externally modified files;
- temporary files that survive failures;
- verification that can report success without proving final state;
- errors that lose the original failure;
- tests that only inspect strings instead of exact bytes;
- tests that do not exercise a later-file failure;
- unproven crash-consistency claims;
- accidental changes to MCP/CLI/TUI transports;
- future-milestone functionality pulled forward unnecessarily.

Use `git diff` and `git status` to prove scope.

---

## 31. Verification checklist

Run at minimum:

```bash
cargo fmt --all -- --check
cargo check --all-targets
cargo test --all-targets
cargo clippy --all-targets --all-features -- -D warnings
```

Then run focused tests for:

- single-file atomicity;
- multi-file rollback;
- verification failure;
- rollback conflict;
- temporary cleanup;
- exact-byte restoration;
- process interruption/recovery where implemented;
- Unicode/newline behavior;
- path/symlink security.

Review:

```bash
git status --short
git diff --check
git diff
```

Confirm that no unrelated source, documentation, workflow, dependency, or configuration changes are included.

---

## 32. Definition of Done

AWE-006 is complete only when all of the following are true:

- [ ] AWE-004 is extended rather than bypassed.
- [ ] All validation/preparation occurs before mutation.
- [ ] Original transaction state is captured sufficiently for immediate rollback.
- [ ] Single-file atomic writes reuse the existing secure filesystem primitive.
- [ ] Multi-file transactions have deterministic application order.
- [ ] A later apply/verification failure triggers rollback of earlier mutations.
- [ ] Rollback is conflict-aware and never blindly overwrites external changes.
- [ ] Rollback success is independently verified.
- [ ] Rollback failure is explicitly reported as incomplete recovery.
- [ ] Temporary artifacts are cleaned on success and failure paths.
- [ ] Exact original bytes are restored in successful rollback cases.
- [ ] Newly-created targets are safely handled where supported.
- [ ] Expected-state and path-security checks remain enforced during rollback.
- [ ] Unified-diff transactions use the same rollback boundary.
- [ ] Process-interruption guarantees are documented honestly.
- [ ] Tests prove no partial mutation after preparation failure.
- [ ] Tests prove a later-file failure restores earlier files.
- [ ] Tests prove verification failure restores all affected files.
- [ ] Tests prove rollback conflicts preserve external modifications.
- [ ] Tests cover exact-byte restoration and relevant Unicode/newline cases.
- [ ] Cross-platform CI remains compatible.
- [ ] `cargo fmt`, `cargo check`, `cargo test`, and `cargo clippy -D warnings` pass.
- [ ] `git diff --check` passes.
- [ ] Final diff contains only the intended AWE-006 implementation changes.
- [ ] No AWE-010/AWE-012 parallel persistence or user-facing rollback system was introduced.

---

## 33. Final implementation report

When the implementation is complete, report:

1. files changed;
2. canonical transaction/rollback architecture used;
3. exact atomic filesystem primitive reused;
4. transaction-local recovery representation;
5. rollback conflict-detection strategy;
6. failure/verification injection strategy;
7. crash/process-interruption guarantees actually proven;
8. tests added and their purpose;
9. verification commands and results;
10. any limitations that remain;
11. confirmation that no later-milestone subsystem was duplicated.

Do not report “atomic” or “rollback-safe” as a generic claim without explaining the exact boundary and guarantees.

---

## 34. Hard STOP rules

After AWE-006 is correctly implemented and verified:

- do not start AWE-007;
- do not implement stale-state coordination;
- do not implement AWE-010 user-requested rollback;
- do not implement AWE-012 durable snapshots/provenance;
- do not add MCP/CLI/TUI exposure unless it is strictly required by an existing compile contract;
- do not refactor unrelated modules;
- do not modify unrelated documentation;
- do not broaden the issue into a general editing rewrite.

The agent must stop after producing the final AWE-006 implementation report.

---

## 35. Final instruction

Implement **only AWE-006 / GitHub issue #27 — Atomic and Rollback-Safe Edits** on branch `rust`.

Build directly on AWE-004's canonical transaction executor and AWE-005's unified-diff path. Establish a real, tested, conflict-aware recovery boundary for single- and multi-file edits, while accurately documenting the limits of filesystem atomicity and process interruption.

Prioritize correctness, security, deterministic behavior, exact-byte recovery, structured failure reporting, and tests over breadth.

Do not claim guarantees that the implementation does not prove.

After AWE-006 is implemented, verified, and audited, **STOP**.