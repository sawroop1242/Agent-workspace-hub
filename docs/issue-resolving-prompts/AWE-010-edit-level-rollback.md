# AWE-010 / #31 — Edit-Level Rollback — Issue-Resolving Master Prompt

## 0. Mission

Implement **production-grade, edit-level rollback** for the canonical AWH editing system.

The objective is to expose an explicit rollback operation for **one specific completed edit transaction**, identified by its stable `EditId`, without restoring an entire workspace, silently overwriting newer work, or creating a second independent editing implementation.

The implementation must build on the existing canonical edit transaction, snapshot/recovery boundary, verification, policy/capability, and provenance architecture. It must remain transport-independent so MCP, CLI, TUI, and the Control API can later expose the same application-service behavior.

The required semantic contract is:

```text
locate completed edit
→ authorize rollback
→ locate exact reversible prior state
→ observe current state
→ verify current state == expected post-edit state
→ refuse on newer/conflicting changes
→ restore exact prior bytes/state
→ read filesystem again
→ verify restoration
→ record linked rollback provenance/audit event
→ return deterministic rollback result
```

Failure must never silently become success.

---

## 1. Scope Control — Work Only on AWE-010

Implement only **AWE-010 / GitHub issue #31: `Implement edit-level rollback`**.

Do not implement or expand unrelated roadmap items.

Do not turn this issue into a generic workspace restore mechanism, generic undo stack, second edit engine, Git reset facility, snapshot-management subsystem, MCP endpoint implementation, CLI command implementation, TUI feature, remote/cloud recovery service, or generic workflow engine.

Later milestones such as AWE-012 snapshot/provenance storage and AWE-013 structured persistent audit may enrich this implementation, but AWE-010 must establish the correct service-level rollback contract without prematurely implementing those later systems.

If the repository does not yet contain the exact storage/service abstraction needed by the contract, introduce the **smallest reusable abstraction required for AWE-010** and keep its responsibility narrowly scoped. Do not invent a parallel architecture merely to make the feature appear complete.

Hard rule:

> If a proposed change is not required to make edit-level rollback correct, safe, testable, and integrated with the existing canonical edit pipeline, do not make that change as part of AWE-010.

---

## 2. Mandatory Repository Preflight

Before editing code, inspect the current `rust` branch and establish the actual implementation state. Do not rely on roadmap text alone.

Read at minimum:

- `docs/PROJECT_CONTEXT.md`
- `docs/PROJECT_ROADMAP.md`
- `docs/PROJECT_ROADMAP_STATUS.md`
- `docs/PROJECT_STATUS.md`
- `docs/FEATURES.md`
- `docs/CLI.md`
- `docs/architecture.md`
- `docs/mcp.md`
- `docs/issue-resolving-prompts/AWE-010-edit-level-rollback.md`
- `src/services/edit.rs`

Then search the repository for the actual implementations of:

- `EditService`
- `EditTransaction`
- `EditId`
- `EditStatus`
- `ExpectedState`
- `FileState`
- snapshot creation/storage
- atomic file writes
- filesystem path/canonicalization helpers
- policy/capability checks
- provenance
- audit events
- edit history
- rollback/restore terminology
- MCP filesystem tools
- CLI filesystem tools

Also inspect AWE-006/AWE-008-related implementation if present, because rollback must consume the existing transaction safety and verification contracts rather than duplicate them.

Record the forensic reality before implementation:

1. What rollback infrastructure already exists?
2. What is only a data model or roadmap item?
3. Where is the actual edit transaction committed?
4. Where can the exact pre-edit bytes/state be recovered?
5. How is the expected post-edit state represented?
6. How are policy/capability decisions currently enforced?
7. How are audit/provenance records currently represented?
8. Which existing helpers are safe to reuse?

Do not claim an existing capability merely because its type, CLI command, documentation entry, or enum variant exists.

---

## 3. Architectural Invariants

AWH is an agent-agnostic workspace runtime. External agents own reasoning and planning; AWH owns controlled filesystem state and editing.

All interfaces must eventually call the same application services:

```text
MCP ─┐
CLI ─┼→ Edit/Rollback application service
TUI ─┤
API ─┘
```

Never implement rollback separately inside MCP, CLI, or TUI.

Rollback is an AWH filesystem/editing operation, not an agent-specific behavior.

The rollback operation must remain compatible with the roadmap's editing sequence:

```text
EditTransaction
→ replace/insert/delete-range
→ patch
→ apply-diff
→ context validation
→ conflict detection
→ atomic commit
→ verification
→ history/rollback
```

The rollback operation must also respect the security ordering:

```text
PolicyEngine
→ capability check
→ session/identity context where available
→ audit/provenance boundary
→ rollback mutation
```

The MCP route or CLI command must never itself become an authorization boundary.

---

## 4. Define the Rollback Identity Correctly

Every successfully completed **reversible** edit must have a stable rollback identity.

The existing `EditId` is the canonical identity for the edit transaction. Do not create a second unrelated identifier unless a concrete storage requirement proves it necessary.

A rollback request must identify exactly one completed edit, for example conceptually:

```text
rollback(edit_id)
```

The implementation must reject:

- unknown edit IDs;
- edits that never successfully committed;
- edits still in progress;
- edits whose reversible state was never captured;
- malformed/invalid IDs;
- edits that are not eligible for rollback under the service contract.

Do not infer an edit from a file path, timestamp, most-recent change, Git history, or filesystem contents.

`edit_id` must identify the exact transaction being reversed.

---

## 5. Define the Reversible State Boundary

A rollback operation needs two logically distinct states:

```text
BEFORE = exact state captured before the original edit
AFTER  = exact state successfully produced by the original edit
```

The implementation must preserve enough information to restore `BEFORE` and to prove that the target is still at `AFTER` before doing so.

For each affected file, the rollback record must be able to establish, at minimum:

- canonical/validated relative path;
- existence state before the edit;
- exact prior bytes or an equivalent lossless restoration representation;
- prior hash;
- prior size;
- prior line metadata where the canonical edit model uses it;
- existence state after the edit;
- post-edit hash/state expected before rollback;
- enough transaction/edit identity to link the state to the original `EditId`.

For files that did not exist before the edit, rollback must be able to remove the newly-created file **only after proving that its current state is still the expected post-edit state**.

For files that existed before the edit and were modified, rollback must restore their exact original bytes.

If an edit can affect multiple files, rollback must cover the complete committed transaction, not silently restore only one arbitrary path.

Do not treat a context-engine snapshot as file rollback unless it actually contains the exact filesystem state required by this contract.

---

## 6. Exact-State Restoration Is Mandatory

Rollback is not a semantic inverse operation.

Do not attempt to reverse:

- `Replace` by swapping strings again;
- `Insert` by deleting an assumed range;
- `DeleteRange` by reconstructing text heuristically;
- `Patch` by applying an inverse patch;
- `ApplyDiff` by generating a reverse diff.

Those approaches can lose formatting, duplicate text, mishandle newline conventions, or overwrite later edits.

Instead, rollback must restore the exact previously captured bytes/state.

Required invariant:

```text
bytes_after_successful_rollback == bytes_before_original_edit
```

For files originally absent:

```text
exists_before_original_edit == false
→ rollback
→ target is absent again
```

For files originally present:

```text
original_bytes
→ edit
→ rollback
→ exact original_bytes
```

Preserve:

- LF vs CRLF;
- final newline/no final newline;
- UTF-8 bytes;
- Unicode/Devanagari/emoji;
- empty files;
- zero-length content;
- file-size metadata represented by the service;
- multi-file transaction boundaries.

Do not normalize text while restoring bytes.

---

## 7. Mandatory Conflict Check Before Rollback

The most important safety invariant is:

> Never overwrite a newer modification merely because an older edit has a valid rollback record.

Before mutating anything, read the **actual current filesystem state** and compare it with the exact expected post-edit state associated with the original transaction.

Conceptually:

```text
current_state == original_edit.after_state
```

must hold for every affected target.

If any target differs, return a structured conflict and perform **zero rollback mutation**.

Examples that must conflict:

```text
original edit
→ external modification
→ rollback requested
→ CONFLICT
```

```text
original edit
→ another AWH edit
→ rollback old edit requested
→ CONFLICT
```

```text
original edit creates file
→ agent modifies created file
→ rollback requested
→ CONFLICT
```

Do not compare only timestamps.

Do not compare only file size.

Do not compare only line count.

Use the strongest exact state available, normally the complete content hash plus existence/path metadata and any canonical state metadata required by the existing contract.

If the existing `FileState` model is insufficient to represent an important distinction, extend it only as required for correct rollback semantics and preserve compatibility.

---

## 8. Zero-Mutation-on-Conflict Invariant

Rollback preparation and conflict validation must be completed for **all affected targets before any rollback mutation occurs**.

For a multi-file transaction:

```text
prepare/locate all recovery states
→ observe all current states
→ validate all conflicts
→ only then mutate
```

If one target conflicts:

```text
file A safe
file B conflict
file C safe

→ restore NOTHING
```

Do not restore A and C while leaving B modified.

This preserves transaction-level safety and prevents rollback from becoming a destructive partial operation.

AWE-006 owns the general atomic/rollback-safe edit transaction machinery; AWE-010 must reuse that machinery where available rather than implement an incompatible transaction engine.

---

## 9. Rollback Mutation Must Be Atomic and Recovery-Aware

Use the repository's existing secure atomic filesystem primitives.

Do not implement direct unsafe truncation/write sequences if an existing canonical atomic-write helper exists.

For each restoration:

```text
validate target
→ prepare exact prior bytes/state
→ write through canonical atomic mechanism
→ verify resulting filesystem state
```

For multi-file rollback, use the existing transaction/recovery semantics from AWE-006 where available.

If rollback itself partially fails, it must not report success.

The result must distinguish at least:

- rollback completed successfully;
- rollback was already completed;
- rollback rejected/conflicted before mutation;
- rollback failed during preparation;
- rollback failed during mutation;
- rollback failed during post-restore verification;
- rollback recovery itself failed, if the existing service can represent that condition.

Do not hide recovery failure behind a generic success response.

---

## 10. Verify the Restoration From the Actual Filesystem

Never construct the rollback result from the bytes that the implementation intended to write.

After restoration, read the actual filesystem state again.

For every affected file verify:

- expected existence/non-existence;
- exact hash;
- exact byte length where applicable;
- canonical metadata represented by the service;
- exact restoration to the original pre-edit state.

For a multi-file rollback, verify every affected target before returning success.

Required lifecycle conceptually:

```text
rollback requested
→ authorized
→ located
→ current-state validated
→ recovery prepared
→ rollback applied
→ restoration verified
→ rollback committed
```

If verification fails, return a verification/recovery failure and use the existing rollback-safe transaction mechanism to recover if possible.

Never return `success` merely because the filesystem write call returned `Ok`.

---

## 11. Deterministic Repeated-Rollback Semantics

Repeated rollback must be deterministic and safe.

After:

```text
edit E
→ rollback(E)
```

calling:

```text
rollback(E)
```

again must **not** restore some older state over current work.

A valid deterministic result is:

```text
already_rolled_back
```

or another explicitly documented equivalent.

The implementation must distinguish this from a conflict where the current state differs from the expected post-edit state because somebody changed the file after the original edit.

Do not implement:

```text
if rollback record exists:
    blindly restore before-state
```

The rollback record must carry sufficient state to determine whether the edit is still the current reversible state or whether it has already been rolled back.

If the system cannot safely distinguish an already-completed rollback from a newer modification, fail closed rather than guessing.

---

## 12. Policy and Capability Enforcement

Rollback is a filesystem mutation and therefore must be policy/capability checked.

The authorization decision must occur before the mutation.

Do not rely on:

- MCP route names;
- CLI command names;
- agent URL namespaces;
- tool descriptions;
- ToolRisk metadata alone;
- configuration/TOML alone.

Use the repository's actual policy/capability architecture.

A denied rollback must:

1. return a structured permission-denied result;
2. perform no filesystem mutation;
3. not reveal unnecessary sensitive file contents;
4. preserve the original edit state;
5. be observable through the existing audit/provenance mechanism when that mechanism is already available.

Do not create a second authorization system inside rollback.

---

## 13. Provenance and Audit Linkage

A rollback is itself a meaningful state transition.

It must be linked to the original edit transaction:

```text
original EditId
      │
      └── rollback event/transaction
```

The rollback record/event should establish, using the repository's existing vocabulary where available:

- original `EditId`;
- rollback identity if a distinct rollback transaction ID is required;
- affected paths or safe path identifiers;
- actor/agent/session identity when available;
- authorization result;
- previous observed state;
- restoration result;
- final verification result;
- timestamp/ordering metadata already used by the project;
- failure/conflict reason where applicable.

Do not implement the complete future AWE-013 persistent audit subsystem here.

If structured audit/provenance infrastructure already exists, integrate with it.

If it does not yet exist, create only the minimum internal linkage required so that AWE-010 has a correct contract and can later be enriched by AWE-012/AWE-013.

Never store unnecessary full file contents in audit records.

---

## 14. Security Requirements

Preserve every existing filesystem security invariant.

Rollback must:

- validate/canonicalize paths using existing helpers;
- remain inside the authorized workspace;
- reject path traversal;
- reject absolute paths where the service requires relative workspace paths;
- preserve symlink-escape protections;
- avoid following attacker-controlled paths outside the workspace;
- avoid trusting rollback metadata alone when resolving the live filesystem target;
- revalidate target identity at the mutation boundary as far as the current architecture permits;
- fail closed on ambiguous target identity;
- avoid exposing full sensitive file contents in errors or audit output.

Do not claim complete TOCTOU protection unless the repository actually provides it and tests prove it.

Document any remaining concurrency boundary precisely.

A rollback record must never become an authorization bypass or a path-resolution bypass.

---

## 15. Concurrency and TOCTOU Boundaries

The implementation must explicitly reason about concurrent modification between:

```text
observe current state
→ authorize/prepare
→ restore
```

Use the strongest synchronization/atomic primitives already present in AWH.

If the process can detect a state change immediately before mutation, refuse rather than overwrite.

If the existing filesystem architecture cannot provide complete race-free coordination, document the limitation instead of claiming stronger guarantees.

Tests must cover the practical race/conflict boundary that the current architecture supports.

Do not introduce an unrelated global filesystem lock unless required by the existing architecture and justified by the implementation.

---

## 16. Interaction With AWE-006

AWE-006 establishes transaction-safe mutation and recovery behavior.

AWE-010 must **consume** that capability rather than duplicate it.

Clearly distinguish:

### AWE-006
Recovery from failure during the execution of the current transaction.

### AWE-010
Explicitly reversing a previously completed edit transaction at a later time.

The two paths may share:

- atomic write helpers;
- snapshots/recovery state;
- transaction records;
- verification helpers;
- conflict detection;
- rollback primitives.

But they have different semantic triggers and identities.

Do not accidentally expose AWE-006's internal emergency rollback as the public edit-level rollback API.

---

## 17. Interaction With AWE-007

AWE-007 owns stale-state/conflict detection.

AWE-010 must reuse the canonical conflict semantics where possible.

For rollback, the relevant precondition is reversed in time:

```text
Original edit:
    expected BEFORE → apply → observed AFTER

Later rollback:
    expected AFTER → restore BEFORE → observed restored BEFORE
```

The implementation must not weaken AWE-007's conflict guarantees merely because the operation is called rollback.

A rollback against a stale target is a conflict, not an overwrite opportunity.

---

## 18. Interaction With AWE-008

AWE-008 establishes post-edit verification semantics.

Rollback must use equivalent actual-filesystem verification after restoration.

The rollback success result must contain the actual observed restored state, not merely the intended state.

If the shared verification service exists, reuse it.

Do not create a second incompatible hash/state verification implementation.

---

## 19. Snapshot Boundary

The exact restoration state must have a clearly defined lifecycle.

At the successful edit boundary:

```text
BEFORE state captured
→ edit applied
→ AFTER state verified
→ reversible record becomes eligible for explicit rollback
```

The rollback implementation must never accept a record whose BEFORE state was not safely captured.

If the existing snapshot model stores exact file bytes, use it.

If the existing snapshot model is not yet suitable for file rollback, add a narrowly scoped reversible-edit record/storage abstraction rather than pretending context snapshots are sufficient.

Do not implement whole-workspace snapshots merely for this issue.

Do not delete recovery data before rollback eligibility semantics are satisfied.

Storage retention policy beyond AWE-010's immediate contract belongs to later snapshot/history milestones.

---

## 20. Multi-File Transaction Semantics

If one `EditTransaction` changes multiple files, rollback must treat it as one logical edit.

Required behavior:

```text
original transaction:
  A → before_A → after_A
  B → before_B → after_B

rollback:
  validate A current == after_A
  validate B current == after_B
  restore A → before_A
  restore B → before_B
  verify A == before_A
  verify B == before_B
```

If B conflicts during preparation:

```text
restore A = NO
restore B = NO
```

If mutation of a later target fails after an earlier target was restored, use the existing transaction recovery mechanism to return to a safe state where possible.

Do not silently downgrade a transaction-level rollback into independent per-file operations.

---

## 21. Filesystem Edge Cases

Tests and implementation must explicitly cover:

- empty file;
- single-line file;
- multi-line file;
- final newline present;
- final newline absent;
- LF;
- CRLF;
- Unicode;
- Devanagari;
- emoji;
- zero-byte file;
- originally absent file;
- newly-created file modified after creation;
- multiple files in one edit;
- file renamed/moved after edit if the service can observe it;
- target deleted after edit;
- target replaced by a different file with different bytes;
- symlink/path escape attempts;
- stale rollback record;
- already-rolled-back edit;
- rollback of failed/uncommitted edit;
- unknown edit ID.

Exact bytes must win over text normalization.

---

## 22. Error Contract

Rollback errors must be machine-actionable and safe to expose through MCP/CLI/API layers.

At minimum, distinguish semantically between:

- unknown edit;
- rollback-not-available;
- not-completed;
- permission denied;
- invalid rollback request;
- conflict/stale current state;
- preparation failure;
- apply failure;
- verification failure;
- recovery failure;
- already rolled back.

Where the existing error architecture supports structured fields, include:

- original `EditId`;
- operation/target information;
- expected state category;
- actual observed state category;
- conflict reason;
- rollback stage;
- whether mutation occurred;
- whether recovery was attempted;
- whether recovery succeeded.

Never include full file contents merely to explain a conflict.

Error serialization must remain transport-neutral.

---

## 23. Result Contract

Define a deterministic rollback result suitable for all transports.

Conceptually the result should communicate:

```text
edit_id
rollback_status
affected_targets
before/restored state summary
verification result
conflict information if any
provenance/audit linkage if available
```

Recommended semantic states include:

```text
rolled_back
already_rolled_back
conflict
rejected
failed
```

Use the repository's existing enums/error/result conventions where they exist instead of introducing duplicate naming systems.

The result must make it impossible for a caller to confuse:

```text
successfully restored
```

with:

```text
nothing changed because rollback was refused
```

---

## 24. Tests — Real Filesystem, Not Mock-Only

Implement comprehensive tests against the actual filesystem/service layer.

### Core success

- edit a file;
- capture original bytes;
- commit edit;
- rollback by exact `EditId`;
- assert byte-for-byte equality with original.

### New-file rollback

- create a file through an edit;
- commit;
- rollback;
- assert file is absent.

### Multi-file success

- modify multiple files in one transaction;
- rollback once;
- assert every file exactly matches its original bytes.

### Conflict

- edit;
- modify the file afterward externally;
- request rollback;
- assert conflict;
- assert newer bytes remain untouched.

### Multi-file conflict

- edit A and B;
- modify B afterward;
- rollback;
- assert neither A nor B is restored.

### Already rolled back

- edit;
- rollback successfully;
- rollback again;
- assert deterministic `already_rolled_back` or equivalent;
- assert no destructive second restore.

### Unknown/uncommitted edit

- rollback unknown ID;
- rollback failed edit;
- rollback incomplete edit;
- assert rejection and zero mutation.

### Verification failure

Inject or simulate a post-restore mismatch/I/O failure where practical and verify that success is never reported.

### Authorization

- deny rollback through the actual policy/capability path;
- assert zero mutation.

### Exact bytes

Include:

- CRLF;
- no final newline;
- Unicode/Devanagari;
- emoji;
- empty files.

### Security

Test traversal, symlink escape, invalid target identity, and workspace containment using existing security helpers.

### Failure injection

Where practical, inject failures during:

- recovery-state lookup;
- current-state observation;
- preparation;
- atomic restore;
- post-restore verification;
- recovery of a partially restored transaction.

Assert that the reported state accurately reflects what happened.

### Property-oriented tests

Where useful, establish the core invariant:

```text
edit(original) → successful rollback → original bytes
```

across generated text/line structures, while preserving newline/Unicode semantics.

---

## 25. Transport Integration Boundary

AWE-010 must implement the **application-service contract**, not duplicate transport code.

If an existing MCP/CLI filesystem service layer is already ready to expose rollback, wire it through the shared service only if that is required to complete the issue.

Do not build a separate MCP-only rollback implementation.

Do not build a separate CLI-only rollback implementation.

If transport exposure is not yet implemented, leave it as a clean service boundary for AWE-014/AWE-009-era integration rather than expanding scope unnecessarily.

The final roadmap requires `fs rollback`, but the command is not considered complete merely because help text exists. Its underlying service and validation must exist first.

---

## 26. Performance and Resource Safety

Rollback may restore large files, so avoid unnecessary memory duplication where the existing architecture provides safer streaming/temporary-file mechanisms.

Do not:

- read unrelated workspace files;
- build full workspace snapshots;
- duplicate file contents repeatedly;
- keep unbounded rollback history in memory;
- emit file contents in errors/logs.

Exact restoration may legitimately require the complete prior bytes somewhere in recovery storage, but storage ownership and lifecycle must be explicit.

Use deterministic cleanup for temporary restoration artifacts.

---

## 27. Backward Compatibility

Do not unnecessarily break:

- `EditTransaction` serialization;
- `EditId` serialization;
- existing edit statuses;
- MCP tool contracts already implemented;
- CLI contracts already implemented;
- existing filesystem security helpers;
- AWE-006 transaction behavior;
- AWE-008 verification behavior.

If a model extension is unavoidable, make it backward-compatible where practical and add serialization tests.

Do not silently reinterpret existing historical edit records in a way that could cause unsafe rollback.

If an old record lacks sufficient exact recovery information, return an explicit `rollback_not_available`/equivalent result rather than guessing.

---

## 28. Documentation Expectations

Update documentation **only where the AWE-010 implementation requires an accurate current contract** and only if documentation changes are necessary to make the implemented behavior truthful.

Do not rewrite roadmap documents or unrelated feature documentation as part of this issue.

Do not document future AWE-012/AWE-013 behavior as implemented.

Any user-facing rollback documentation must clearly state:

- rollback is edit-level, not whole-workspace restore;
- rollback requires an eligible completed edit;
- newer modifications cause conflict rather than overwrite;
- rollback restores exact prior state;
- repeated rollback has deterministic semantics;
- rollback is policy/capability controlled.

---

## 29. Verification Commands

Before declaring AWE-010 complete, run the repository's applicable gates:

```bash
cargo fmt --all -- --check
cargo check --all-targets
cargo test --all-targets
cargo clippy --all-targets --all-features -- -D warnings
```

Also run targeted rollback/edit tests and any relevant integration tests.

Perform a real filesystem validation, not only unit tests.

If MCP/CLI exposure is part of the current implementation surface, validate the real interface path as well.

Finally inspect:

```bash
git status --short
git diff --check
git diff
```

Confirm there are no unrelated modifications.

---

## 30. Definition of Done

AWE-010 is complete only when all of the following are true:

- [ ] A completed edit has a stable rollback identity.
- [ ] Exact prior file state is retained/retrievable for eligible edits.
- [ ] Rollback identifies the exact `EditId`; it never guesses.
- [ ] Current state is checked against the original edit's verified post-edit state.
- [ ] Newer/conflicting changes are rejected before mutation.
- [ ] Multi-file rollback performs complete preflight before mutation.
- [ ] Exact original bytes are restored.
- [ ] Newly-created files are safely removed only when their expected post-edit state still matches.
- [ ] Existing atomic filesystem primitives are reused.
- [ ] Restoration is verified from the actual filesystem.
- [ ] Rollback failures never report success.
- [ ] Repeated rollback has deterministic safe semantics.
- [ ] Policy/capability checks occur before mutation.
- [ ] Rollback is linked to the original edit through provenance/audit semantics.
- [ ] No whole-workspace restore behavior was introduced.
- [ ] No second independent editing engine was introduced.
- [ ] AWE-006 transaction safety is reused rather than duplicated.
- [ ] AWE-007 conflict semantics are preserved.
- [ ] AWE-008 verification semantics are reused or matched.
- [ ] Real filesystem tests cover success, conflict, multi-file, repeated rollback, failures, security, and exact-byte restoration.
- [ ] Unicode and newline behavior is verified.
- [ ] Existing serialization/contracts remain compatible or have explicit migration-safe behavior.
- [ ] Full Rust CI gates pass.
- [ ] `git diff` contains only intended AWE-010 implementation changes.

---

## 31. Explicit Non-Goals

Do **not** implement as part of AWE-010:

- whole-workspace restore;
- generic undo/redo framework;
- Git reset/revert as the rollback mechanism;
- second editing service;
- inverse-operation rollback based on text reconstruction;
- generic workflow engine;
- model routing;
- agent reasoning/orchestration;
- remote/cloud rollback;
- connector rollback;
- TUI-specific rollback semantics;
- a new MCP-only editor;
- complete future snapshot-management subsystem;
- complete future persistent audit subsystem;
- speculative distributed locking system;
- OS/container/VM sandboxing;
- unrelated CLI/TUI redesign;
- unrelated refactors.

---

## 32. Final Implementation Report

At completion, report concisely:

1. **Root cause / forensic baseline** — what was missing and what actually existed.
2. **Implementation** — exact rollback service/state/recovery changes made.
3. **State model** — how BEFORE and expected AFTER are represented.
4. **Conflict safety** — how newer changes are detected and prevented from being overwritten.
5. **Atomicity/recovery** — how multi-file rollback remains safe.
6. **Verification** — how actual restored filesystem state is verified.
7. **Authorization** — which policy/capability path protects rollback.
8. **Provenance/audit** — how rollback links to the original `EditId`.
9. **Tests** — exact tests added and important failure cases covered.
10. **Validation** — commands run and their results.
11. **Scope audit** — files changed and confirmation that no unrelated work was included.
12. **Known limitations** — especially any concurrency/TOCTOU boundary that the implementation cannot guarantee.

Do not claim stronger guarantees than the tests and implementation establish.

---

## 33. HARD STOP

After AWE-010 is correctly implemented, tested, and verified:

**STOP.**

Do not proceed automatically to:

- AWE-011 capability-policy edit paths;
- AWE-012 snapshots/provenance;
- AWE-013 persistent audit;
- AWE-014 CLI agent-grade editing;
- AWE-015 editing test suite;
- AWE-016 real MCP validation;
- AWE-017 acceptance workflow;
- AWE-018 editing contract;
- filesystem TOCTOU coordination;
- agent profiles/worktrees;
- security milestones;
- architecture unification.

Those are separate milestones.

### Final instruction

> **Implement only AWE-010 / GitHub issue #31: production-grade edit-level rollback. Reuse the canonical EditTransaction/EditService, existing atomic filesystem and policy/capability infrastructure, AWE-006 transaction safety, AWE-007 conflict semantics, and AWE-008 verification semantics. Restore exact prior bytes only after proving the current filesystem still matches the original edit's expected post-edit state. Refuse newer/conflicting changes, preserve multi-file atomicity, verify restoration from the actual filesystem, and emit a linked provenance/audit event using existing infrastructure. Add real filesystem and failure/recovery tests. Do not implement future milestones or unrelated refactors. Verify all repository gates and scope. Then STOP.**